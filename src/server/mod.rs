//! HTTP Server implementation for Cello.
//!
//! This module provides:
//! - High-performance async HTTP server
//! - Graceful shutdown support
//! - Cluster mode (multi-process)
//! - HTTP/1.1, HTTP/2, and HTTP/3 support
//! - TLS configuration
//! - Server metrics

pub mod cluster;
pub mod http3;
pub mod protocols;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse, StatusCode, body::Incoming};
use hyper_util::rt::TokioIo;
use parking_lot::RwLock;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
type TlsAcceptor = tokio_rustls::TlsAcceptor;
use tokio::sync::broadcast;

use crate::error::{AppError, ErrorHandlerRegistry};
use crate::handler::{HandlerError, HandlerRegistry, HandlerResult};
use crate::middleware::{MiddlewareAction, MiddlewareChain};
use crate::request::Request;
use crate::response::Response;
use crate::router::Router;
use crate::websocket::WebSocketRegistry;

pub use cluster::{ClusterConfig, ClusterManager};
pub use protocols::{Http2Config, Http3Config, TlsConfig};

fn build_tls_acceptor(config: &TlsConfig) -> Result<TlsAcceptor, String> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::fs::File;
    use std::io::BufReader;

    let mut cert_reader = BufReader::new(
        File::open(&config.cert_path)
            .map_err(|e| format!("failed to open TLS certificate: {e}"))?,
    );
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to parse TLS certificate: {e}"))?;
    if certs.is_empty() {
        return Err("TLS certificate file contains no certificates".to_string());
    }

    let mut key_reader = BufReader::new(
        File::open(&config.key_path).map_err(|e| format!("failed to open TLS private key: {e}"))?,
    );
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| format!("failed to parse TLS private key: {e}"))?
        .ok_or_else(|| "TLS private key file contains no private key".to_string())?;

    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("invalid TLS certificate/key pair: {e}"))?;
    server_config.alpn_protocols = config
        .alpn_protocols
        .iter()
        .map(|protocol| protocol.as_bytes().to_vec())
        .collect();
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(server_config)))
}
// ============================================================================
// Server Configuration
// ============================================================================

/// Server configuration options.
#[derive(Clone)]
pub struct ServerConfig {
    /// Host address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Number of worker threads (0 = auto)
    pub workers: usize,
    /// Connection backlog
    pub backlog: u32,
    /// Keep-alive timeout
    pub keep_alive: Option<Duration>,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Enable TCP_NODELAY
    pub tcp_nodelay: bool,
    /// Read timeout
    pub read_timeout: Option<Duration>,
    /// Write timeout
    pub write_timeout: Option<Duration>,
    /// Maximum request body size in bytes (0 = unlimited)
    pub max_body_size: usize,
    /// Timeout for reading request headers (Slowloris guard); None = disabled
    pub read_header_timeout: Option<Duration>,
    /// Timeout for reading the full request body; None = disabled
    pub read_body_timeout: Option<Duration>,
    /// Timeout for a single handler to produce a response; None = disabled
    pub handler_timeout: Option<Duration>,
    /// Graceful shutdown timeout
    pub shutdown_timeout: Duration,
    /// TLS configuration
    pub tls: Option<TlsConfig>,
    /// HTTP/2 configuration
    pub http2: Option<Http2Config>,
    /// HTTP/3 configuration (QUIC)
    pub http3: Option<Http3Config>,
    /// Cluster configuration
    pub cluster: Option<ClusterConfig>,
}

impl ServerConfig {
    /// Create new server config with defaults.
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
            workers: 0,
            backlog: 1024,
            keep_alive: Some(Duration::from_secs(75)),
            max_connections: 10000,
            tcp_nodelay: true,
            read_timeout: Some(Duration::from_secs(30)),
            write_timeout: Some(Duration::from_secs(30)),
            max_body_size: 100 * 1024 * 1024,
            read_header_timeout: None,
            read_body_timeout: None,
            handler_timeout: None,
            shutdown_timeout: Duration::from_secs(30),
            tls: None,
            http2: None,
            http3: None,
            cluster: None,
        }
    }

    /// Set number of worker threads.
    pub fn workers(mut self, n: usize) -> Self {
        self.workers = n;
        self
    }

    /// Set keep-alive timeout.
    pub fn keep_alive(mut self, duration: Duration) -> Self {
        self.keep_alive = Some(duration);
        self
    }

    /// Disable keep-alive.
    pub fn no_keep_alive(mut self) -> Self {
        self.keep_alive = None;
        self
    }

    /// Set maximum concurrent connections.
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }

    /// Enable TLS.
    pub fn tls(mut self, config: TlsConfig) -> Self {
        self.tls = Some(config);
        self
    }

    /// Enable HTTP/2.
    pub fn http2(mut self, config: Http2Config) -> Self {
        self.http2 = Some(config);
        self
    }

    /// Enable cluster mode.
    pub fn cluster(mut self, config: ClusterConfig) -> Self {
        self.cluster = Some(config);
        self
    }

    /// Set shutdown timeout.
    pub fn shutdown_timeout(mut self, duration: Duration) -> Self {
        self.shutdown_timeout = duration;
        self
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self::new("127.0.0.1", 8000)
    }
}

// ============================================================================
// Server Metrics
// ============================================================================

/// Server performance metrics.
#[derive(Clone)]
pub struct ServerMetrics {
    /// Total requests received
    pub total_requests: Arc<AtomicU64>,
    /// Active connections
    pub active_connections: Arc<AtomicU64>,
    /// Total bytes received
    pub bytes_received: Arc<AtomicU64>,
    /// Total bytes sent
    pub bytes_sent: Arc<AtomicU64>,
    /// Total errors
    pub total_errors: Arc<AtomicU64>,
    /// Server start time
    pub start_time: Instant,
    /// PERF: Request latency ring buffer (VecDeque for O(1) push/pop)
    latencies: Arc<RwLock<VecDeque<Duration>>>,
}

impl ServerMetrics {
    /// Create new metrics.
    pub fn new() -> Self {
        Self {
            total_requests: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
            latencies: Arc::new(RwLock::new(VecDeque::with_capacity(1024))),
        }
    }

    /// Increment request count.
    #[inline]
    pub fn inc_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment connection count.
    #[inline]
    pub fn inc_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement connection count.
    #[inline]
    pub fn dec_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Add bytes received.
    #[inline]
    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Add bytes sent.
    #[inline]
    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Increment error count.
    #[inline]
    pub fn inc_errors(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record request latency.
    /// PERF: Sample-based recording - only records every 64th request to avoid
    /// write lock contention on the VecDeque under high load. This gives accurate
    /// enough latency statistics while eliminating the lock as a bottleneck.
    #[inline]
    pub fn record_latency(&self, latency: Duration) {
        // Sample every 64th request (cheap power-of-2 modulo via bitwise AND)
        if self.total_requests.load(Ordering::Relaxed) & 63 == 0 {
            let mut latencies = self.latencies.write();
            latencies.push_back(latency);
            if latencies.len() > 1000 {
                latencies.pop_front();
            }
        }
    }

    /// Get average latency.
    pub fn avg_latency(&self) -> Duration {
        let latencies = self.latencies.read();
        if latencies.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = latencies.iter().sum();
        total / latencies.len() as u32
    }

    /// Get requests per second.
    pub fn requests_per_second(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_requests.load(Ordering::Relaxed) as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Get uptime.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get snapshot of all metrics.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            uptime_secs: self.start_time.elapsed().as_secs(),
            requests_per_second: self.requests_per_second(),
            avg_latency_ms: self.avg_latency().as_millis() as f64,
        }
    }
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics snapshot for serialization.
#[derive(Clone, Debug, serde::Serialize)]
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub active_connections: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub total_errors: u64,
    pub uptime_secs: u64,
    pub requests_per_second: f64,
    pub avg_latency_ms: f64,
}

// ============================================================================
// Shutdown Coordinator
// ============================================================================

/// Coordinates graceful shutdown.
pub struct ShutdownCoordinator {
    /// Shutdown signal sender
    notify: broadcast::Sender<()>,
    /// Whether shutdown has been initiated
    shutdown_initiated: Arc<AtomicBool>,
    /// Active request count
    active_requests: Arc<AtomicU64>,
    /// Drain timeout
    drain_timeout: Duration,
}

impl ShutdownCoordinator {
    /// Create new shutdown coordinator.
    pub fn new(drain_timeout: Duration) -> Self {
        let (notify, _) = broadcast::channel(1);
        Self {
            notify,
            shutdown_initiated: Arc::new(AtomicBool::new(false)),
            active_requests: Arc::new(AtomicU64::new(0)),
            drain_timeout,
        }
    }

    /// Get a shutdown receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.notify.subscribe()
    }

    /// Initiate shutdown.
    pub fn shutdown(&self) {
        self.shutdown_initiated.store(true, Ordering::SeqCst);
        let _ = self.notify.send(());
    }

    /// Check if shutdown has been initiated.
    #[inline]
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_initiated.load(Ordering::SeqCst)
    }

    /// Increment active request count.
    /// PERF: Use Relaxed ordering on hot path - exact count not critical for request processing.
    #[inline]
    pub fn request_started(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active request count.
    #[inline]
    pub fn request_finished(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get active request count.
    pub fn active_requests(&self) -> u64 {
        self.active_requests.load(Ordering::Relaxed)
    }

    /// Wait for all requests to complete or timeout.
    pub async fn drain(&self) {
        let start = Instant::now();
        while self.active_requests() > 0 {
            if start.elapsed() > self.drain_timeout {
                eprintln!(
                    "Warning: {} requests still active after drain timeout",
                    self.active_requests()
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

// ============================================================================
// HTTP Server
// ============================================================================

/// The main HTTP server.
pub struct Server {
    config: ServerConfig,
    router: Router,
    handlers: HandlerRegistry,
    middleware: MiddlewareChain,
    websocket_handlers: WebSocketRegistry,
    metrics: ServerMetrics,
    shutdown: ShutdownCoordinator,
    dependency_container: Arc<crate::dependency::DependencyContainer>,
    guards: Arc<crate::middleware::guards::GuardsMiddleware>,
    prometheus:
        Arc<parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>>,
    error_handlers: Arc<ErrorHandlerRegistry>,
}

impl Server {
    pub fn new(
        config: ServerConfig,
        router: Router,
        handlers: HandlerRegistry,
        middleware: MiddlewareChain,
        websocket_handlers: WebSocketRegistry,
        dependency_container: Arc<crate::dependency::DependencyContainer>,
        guards: Arc<crate::middleware::guards::GuardsMiddleware>,
        prometheus: Arc<
            parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>,
        >,
        error_handlers: Arc<ErrorHandlerRegistry>,
    ) -> Self {
        let shutdown = ShutdownCoordinator::new(config.shutdown_timeout);
        Server {
            config,
            router,
            handlers,
            middleware,
            websocket_handlers,
            metrics: ServerMetrics::new(),
            shutdown,
            dependency_container,
            guards,
            prometheus,
            error_handlers,
        }
    }

    /// Create a server with simple parameters (legacy compatibility).
    pub fn simple(
        host: String,
        port: u16,
        router: Router,
        handlers: HandlerRegistry,
        middleware: MiddlewareChain,
        websocket_handlers: WebSocketRegistry,
    ) -> Self {
        let config = ServerConfig::new(&host, port);
        Self::new(
            config,
            router,
            handlers,
            middleware,
            websocket_handlers,
            Arc::new(crate::dependency::DependencyContainer::new()),
            Arc::new(crate::middleware::guards::GuardsMiddleware::new()),
            Arc::new(parking_lot::RwLock::new(None)),
            Arc::new(ErrorHandlerRegistry::new()),
        )
    }

    /// Get server metrics.
    pub fn metrics(&self) -> &ServerMetrics {
        &self.metrics
    }

    /// Initiate graceful shutdown.
    pub fn shutdown(&self) {
        self.shutdown.shutdown();
    }

    /// Run the server (blocking).
    pub async fn run(self) -> PyResult<()> {
        let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
            .parse()
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid address: {e}"))
            })?;

        // PERF: Use SO_REUSEPORT for multi-process scaling.
        // This allows multiple processes to bind to the same port,
        // with the kernel distributing connections across them.
        let socket = socket2::Socket::new(
            if addr.is_ipv4() {
                socket2::Domain::IPV4
            } else {
                socket2::Domain::IPV6
            },
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create socket: {e}"))
        })?;

        // SO_REUSEPORT: Unix only. Allows multiple processes to bind to the same port
        // with kernel-level load balancing. On Windows, SO_REUSEADDR (set below) allows
        // the port to be reused after a process exits, but does not provide load balancing
        // across concurrent processes. Windows multi-process mode relies on each process
        // racing to accept connections.
        #[cfg(unix)]
        socket.set_reuse_port(true).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to set SO_REUSEPORT: {e}"))
        })?;
        socket.set_reuse_address(true).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to set SO_REUSEADDR: {e}"))
        })?;
        socket.set_nonblocking(true).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to set nonblocking: {e}"))
        })?;
        socket.bind(&addr.into()).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to bind: {e}"))
        })?;
        socket.listen(self.config.backlog as i32).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to listen: {e}"))
        })?;

        let std_listener: std::net::TcpListener = socket.into();
        let listener = TcpListener::from_std(std_listener).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create listener: {e}"))
        })?;

        let tls_acceptor = self
            .config
            .tls
            .as_ref()
            .map(build_tls_acceptor)
            .transpose()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?
            .map(Arc::new);

        // Banner and server details are printed by Python

        let router = Arc::new(self.router);
        let handlers = Arc::new(self.handlers);
        let middleware = Arc::new(self.middleware);
        let websocket_handlers = Arc::new(self.websocket_handlers);
        let metrics = Arc::new(self.metrics);
        let shutdown = Arc::new(self.shutdown);
        let dependency_container = self.dependency_container.clone();
        let guards = self.guards.clone();
        let prometheus = self.prometheus.clone();
        let error_handlers = self.error_handlers.clone();

        // Copy limit/timeout config out of self.config (all Copy) so each connection
        // task can capture them cheaply.
        let max_body_size = self.config.max_body_size;
        let read_body_timeout = self.config.read_body_timeout;
        let read_header_timeout = self.config.read_header_timeout;
        let handler_timeout = self.config.handler_timeout;
        let http2_config = self.config.http2.clone();

        // HTTP/3 (QUIC): bind a UDP endpoint on the same host:port when
        // configured. HTTP/3 mandates TLS 1.3, so the TLS certificate/key from
        // `enable_tls` are reused for the QUIC handshake.
        if let (Some(http3_config), Some(tls_config)) =
            (self.config.http3.clone(), self.config.tls.clone())
        {
            let h3_addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
                .parse()
                .map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid HTTP/3 address: {e}"))
                })?;
            let h3_ctx = http3::ServeCtx {
                router: router.clone(),
                handlers: handlers.clone(),
                middleware: middleware.clone(),
                metrics: metrics.clone(),
                shutdown: shutdown.clone(),
                dependency_container: dependency_container.clone(),
                guards: guards.clone(),
                prometheus: prometheus.clone(),
                error_handlers: error_handlers.clone(),
                max_body_size,
                read_body_timeout,
                handler_timeout,
            };
            let h3_cert = tls_config.cert_path.to_string_lossy().into_owned();
            let h3_key = tls_config.key_path.to_string_lossy().into_owned();
            tokio::spawn(async move {
                if let Err(err) =
                    http3::run(&http3_config, &h3_cert, &h3_key, h3_addr, h3_ctx).await
                {
                    eprintln!("HTTP/3 server error: {err}");
                }
            });
        }

        let mut shutdown_rx = shutdown.subscribe();

        // Listen for termination signals (SIGTERM on Unix, ctrl_c fallback on Windows)
        let shutdown_sigterm = shutdown.clone();
        tokio::task::spawn(async move {
            #[cfg(unix)]
            {
                if let Ok(mut sig) =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                {
                    let _ = sig.recv().await;
                    shutdown_sigterm.shutdown();
                }
            }
            #[cfg(windows)]
            {
                // On Windows, SIGTERM is not delivered to processes.
                // The primary shutdown mechanism is ctrl_c (handled in the main select! loop).
                // This task provides a secondary ctrl_c listener in case the main loop's
                // ctrl_c branch is not reached (e.g., during a long-running accept).
                // Windows process managers use CtrlBreak/CtrlC events, both handled by ctrl_c().
                if let Ok(()) = tokio::signal::ctrl_c().await {
                    shutdown_sigterm.shutdown();
                }
            }
        });

        // Created once, outside the loop: a `ctrl_c()` future only observes
        // signals delivered after it was created, so re-creating it on every
        // iteration dropped a SIGINT that landed while an accepted connection
        // was being set up. Under load the server then ignored Ctrl-C and hung
        // until the next connection arrived.
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    shutdown.shutdown();
                    break;
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
                accept_result = listener.accept() => {
                    if shutdown.is_shutting_down() {
                        break;
                    }

                    match accept_result {
                        Ok((stream, peer_addr)) => {
                            // Check connection limit
                            if metrics.active_connections.load(Ordering::Relaxed)
                                >= self.config.max_connections as u64
                            {
                                eprintln!("Connection limit reached, rejecting connection from {peer_addr}");
                                continue;
                            }

                            // PERF: Apply TCP_NODELAY to reduce latency for small responses
                            let _ = stream.set_nodelay(true);

                            metrics.inc_connections();

                            let router = router.clone();
                            let handlers = handlers.clone();
                            let middleware = middleware.clone();
                            let websocket_handlers = websocket_handlers.clone();
                            let peer_str = peer_addr.to_string();
                            let metrics_for_service = metrics.clone();
                            let metrics_for_cleanup = metrics.clone();
                            let shutdown = shutdown.clone();
                            let tls_acceptor = tls_acceptor.clone();
                            let http2_config = http2_config.clone();

                            let dependency_container = dependency_container.clone();
                            let guards = guards.clone();
                            let prometheus = prometheus.clone();
                            let error_handlers = error_handlers.clone();

                            tokio::task::spawn(async move {
                                // PERF: Clone Arcs once per connection, not per request.
                                // For keep-alive connections, this avoids repeated Arc refcount bumps.
                                let conn_router = router;
                                let conn_handlers = handlers;
                                let conn_middleware = middleware;
                                let conn_metrics = metrics_for_service;
                                let conn_shutdown = shutdown;
                                let conn_deps = dependency_container;
                                let conn_guards = guards;
                                let conn_prometheus = prometheus;
                                let conn_error_handlers = error_handlers;
                                let conn_tls_acceptor = tls_acceptor;
                                let conn_http2_config = http2_config;
                                let conn_ws_handlers = websocket_handlers;
                                let conn_peer = peer_str;
                                // Copy limit/timeout config (all Copy) into the connection scope.
                                let conn_max_body_size = max_body_size;
                                let conn_read_body_timeout = read_body_timeout;
                                let conn_handler_timeout = handler_timeout;

                                let service = service_fn(move |req| {
                                    let router = conn_router.clone();
                                    let handlers = conn_handlers.clone();
                                    let middleware = conn_middleware.clone();
                                    let metrics = conn_metrics.clone();
                                    let shutdown = conn_shutdown.clone();
                                    let dependency_container = conn_deps.clone();
                                    let guards = conn_guards.clone();
                                    let prometheus = conn_prometheus.clone();
                                    let error_handlers = conn_error_handlers.clone();
                                    let ws_handlers = conn_ws_handlers.clone();
                                    let peer_str = conn_peer.clone();
                                    let req_max_body_size = conn_max_body_size;
                                    let req_read_body_timeout = conn_read_body_timeout;
                                    let req_handler_timeout = conn_handler_timeout;

                                    async move {
                                        // Rebind as mutable: the WebSocket upgrade path
                                        // needs `&mut req` to extract the OnUpgrade
                                        // future from the request extensions.
                                        let mut req = req;

                                        // WebSocket upgrade fast-path: the handshake is
                                        // answered with 101 and the session is spawned
                                        // before any Request allocation or routing.
                                        if req.method() == hyper::Method::GET
                                            && crate::websocket::is_websocket_upgrade(req.headers())
                                            && ws_handlers.contains(req.uri().path())
                                        {
                                            if let Some(resp) = handle_websocket_upgrade(
                                                &mut req,
                                                &ws_handlers,
                                                peer_str,
                                            )
                                            .await
                                            {
                                                return Ok(resp);
                                            }
                                        }

                                        shutdown.request_started();
                                        let start = Instant::now();

                                        let result = handle_request(
                                            req,
                                            &router,
                                            &handlers,
                                            &middleware,
                                            &metrics,
                                            &dependency_container,
                                            &guards,
                                            &prometheus,
                                            &error_handlers,
                                            req_max_body_size,
                                            req_read_body_timeout,
                                            req_handler_timeout,
                                        )
                                        .await;

                                        metrics.record_latency(start.elapsed());
                                        shutdown.request_finished();

                                        result
                                    }
                                });

                                // TLS connections negotiated with ALPN `h2` use
                                // Hyper's HTTP/2 builder. Plain TCP remains HTTP/1.1.
                                let serve_res: Result<(), hyper::Error> = if let Some(acceptor) = conn_tls_acceptor {
                                    match acceptor.accept(stream).await {
                                        Ok(tls_stream) => {
                                            let is_h2 = tls_stream
                                                .get_ref()
                                                .1
                                                .alpn_protocol()
                                                .map(|protocol| protocol == b"h2")
                                                .unwrap_or(false);
                                            if is_h2 {
                                                let mut builder = hyper::server::conn::http2::Builder::new(
                                                    hyper_util::rt::TokioExecutor::new(),
                                                );
                                                if let Some(h2) = &conn_http2_config {
                                                    builder.max_concurrent_streams(h2.max_concurrent_streams);
                                                    builder.initial_connection_window_size(h2.initial_connection_window_size);
                                                    builder.initial_stream_window_size(h2.initial_stream_window_size);
                                                    builder.max_frame_size(h2.max_frame_size);
                                                    builder.max_header_list_size(h2.max_header_list_size);
                                                }
                                                builder.serve_connection(TokioIo::new(tls_stream), service).await
                                            } else {
                                                let mut builder = http1::Builder::new();
                                                builder.keep_alive(true).pipeline_flush(true);
                                                if let Some(t) = read_header_timeout {
                                                    builder.timer(hyper_util::rt::TokioTimer::new());
                                                    builder.header_read_timeout(t);
                                                }
                                                builder
                                                    .serve_connection(TokioIo::new(tls_stream), service)
                                                    .await
                                            }
                                        }
                                        Err(err) => {
                                            eprintln!("TLS handshake error: {err:?}");
                                            Ok(())
                                        }
                                    }
                                } else {
                                    let mut builder = http1::Builder::new();
                                    builder.keep_alive(true).pipeline_flush(true);
                                    if let Some(t) = read_header_timeout {
                                        builder.timer(hyper_util::rt::TokioTimer::new());
                                        builder.header_read_timeout(t);
                                    }
                                    builder
                                        .serve_connection(TokioIo::new(stream), service)
                                        .await
                                };

                                if let Err(err) = serve_res {
                                    // Only log if not a normal connection close
                                    if !err.is_incomplete_message() {
                                        eprintln!("Connection error: {err:?}");
                                    }
                                }

                                metrics_for_cleanup.dec_connections();
                            });
                        }
                        Err(e) => {
                            eprintln!("Accept error: {e}");
                        }
                    }
                }
            }
        }

        // Wait for active requests to complete
        if shutdown.active_requests() > 0 {
            shutdown.drain().await;
        }

        Ok(())
    }
}

/// Answer a WebSocket upgrade handshake (RFC 6455) and run the registered
/// Python handler over a `tokio-tungstenite` session.
///
/// Returns `Some(101 Switching Protocols)` once the session has been spawned.
/// Returns `None` when the request is not a valid handshake or no handler is
/// registered for the path, so the caller can fall through to normal handling.
async fn handle_websocket_upgrade(
    req: &mut HyperRequest<Incoming>,
    registry: &Arc<WebSocketRegistry>,
    peer: String,
) -> Option<HyperResponse<Full<Bytes>>> {
    let key = crate::websocket::websocket_key(req.headers())?;
    let handler = Python::attach(|py| registry.get(req.uri().path(), py))?;
    let accept = crate::websocket::accept_key(&key);
    // Take the OnUpgrade future out of the request extensions. Unlike
    // `hyper::upgrade::on`, this does not panic when the extension is missing
    // (e.g. on HTTP/2 connections, which cannot be upgraded this way).
    let on_upgrade = match req.extensions_mut().remove::<hyper::upgrade::OnUpgrade>() {
        Some(upgrade) => upgrade,
        None => return None,
    };

    let resp = HyperResponse::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(hyper::header::UPGRADE, "websocket")
        .header(hyper::header::CONNECTION, "Upgrade")
        .header("Sec-WebSocket-Accept", accept)
        .body(Full::new(Bytes::new()))
        .ok()?;

    tokio::spawn(async move {
        match on_upgrade.await {
            Ok(upgraded) => {
                crate::websocket::run_session(upgraded, handler, peer).await;
            }
            Err(err) => {
                eprintln!("WebSocket upgrade failed: {err}");
            }
        }
    });

    Some(resp)
}

/// Serve a request whose path did not match any route but is claimed by a
/// path-owning middleware (health checks, GraphQL, …). Builds a `Request`
/// (including body for POST-style endpoints) and runs only the middleware that
/// reported `serves_unrouted() == true`. Returns `Some(response)` if one of
/// them served the request, `None` to fall through to a 404.
///
/// Generic over the body so the HTTP/3 (QUIC) path can reuse the same logic.
#[allow(clippy::too_many_arguments)]
async fn serve_unrouted<B>(
    req: HyperRequest<B>,
    method_str: &str,
    uri: &hyper::Uri,
    path: &str,
    middleware: &Arc<MiddlewareChain>,
    metrics: &Arc<ServerMetrics>,
    max_body_size: usize,
    read_body_timeout: Option<Duration>,
) -> Option<Response>
where
    B: http_body::Body<Data = Bytes> + Unpin + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // Copy headers before the body is consumed.
    let mut headers: HashMap<String, String> = HashMap::with_capacity(req.headers().len());
    for (k, v) in req.headers().iter() {
        headers.insert(k.as_str().to_owned(), v.to_str().unwrap_or("").to_owned());
    }

    // Parse query string (same rules as the routed path).
    let query_string = uri.query().unwrap_or("");
    let query: HashMap<String, String> = if query_string.is_empty() {
        HashMap::new()
    } else {
        let decode = |raw: &str| -> String {
            urlencoding::decode(&raw.replace('+', " "))
                .unwrap_or_default()
                .into_owned()
        };
        query_string
            .split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                match (parts.next(), parts.next()) {
                    (Some(key), Some(value)) => Some((decode(key), decode(value))),
                    (Some(key), None) => Some((decode(key), String::new())),
                    _ => None,
                }
            })
            .collect()
    };

    // Read the body for methods that carry one (GraphQL queries POST a body).
    let body_bytes: Vec<u8> = match method_str {
        "GET" | "HEAD" => {
            drop(req);
            Vec::new()
        }
        _ => {
            let limit = if max_body_size == 0 {
                usize::MAX
            } else {
                max_body_size
            };
            let collect_fut = Limited::new(req.into_body(), limit).collect();
            let collected = match read_body_timeout {
                Some(t) => match tokio::time::timeout(t, collect_fut).await {
                    Ok(inner) => inner,
                    Err(_) => {
                        return Some(Response::error(408, "Request Timeout: body read timed out"));
                    }
                },
                None => collect_fut.await,
            };
            match collected {
                Ok(collected) => collected.to_bytes().to_vec(),
                Err(_) => {
                    return Some(Response::error(
                        413,
                        "Payload Too Large: request body exceeds limit",
                    ));
                }
            }
        }
    };

    let mut request = Request::from_http(
        method_str.to_owned(),
        path.to_owned(),
        HashMap::new(),
        query,
        headers,
        body_bytes,
    );

    // Sync path-owning middleware (e.g. health checks).
    match middleware.execute_before_unrouted(&mut request) {
        Ok(MiddlewareAction::Stop(response)) => return Some(response),
        Ok(MiddlewareAction::Continue) => {}
        Err(e) => {
            metrics.inc_errors();
            return Some(Response::error(e.status, &e.message));
        }
    }

    // Async path-owning middleware (e.g. GraphQL).
    match middleware.execute_before_async_unrouted(&mut request).await {
        Ok(MiddlewareAction::Stop(response)) => Some(response),
        Ok(MiddlewareAction::Continue) => None,
        Err(e) => {
            metrics.inc_errors();
            Some(Response::error(e.status, &e.message))
        }
    }
}

/// Route and serve a single request through the full pipeline (middleware,
/// guards, handler, Prometheus, error handling).
///
/// Generic over the body type so both hyper HTTP/1.1|2 (`Incoming`) and the
/// HTTP/3 QUIC path (a constructed `hyper::body::Body`) share this logic.
pub(crate) async fn handle_request<B>(
    req: HyperRequest<B>,
    router: &Arc<Router>,
    handlers: &Arc<HandlerRegistry>,
    middleware: &Arc<MiddlewareChain>,
    metrics: &Arc<ServerMetrics>,
    dependency_container: &Arc<crate::dependency::DependencyContainer>,
    guards: &Arc<crate::middleware::guards::GuardsMiddleware>,
    prometheus: &Arc<
        parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>,
    >,
    error_handlers: &Arc<ErrorHandlerRegistry>,
    max_body_size: usize,
    read_body_timeout: Option<Duration>,
    handler_timeout: Option<Duration>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible>
where
    B: http_body::Body<Data = Bytes> + Unpin + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    metrics.inc_requests();

    // PERF: Extract method and path WITHOUT owning - use references as long as possible
    let method = req.method().clone();
    let method_str = method.as_str();
    let uri = req.uri().clone();
    let path = uri.path();

    // PERF: `build_resp` only needs method/path so that error handlers on an
    // early return (404/413/408/…) get context. `build_hyper_response` builds
    // that lightweight context lazily, so successful requests never allocate it.
    let build_resp = |response: &Response| {
        build_hyper_response(response, metrics, error_handlers, method_str, path, None)
    };

    // PERF: Route match FIRST - fail fast on 404 before any allocation
    let route_match = router.match_route(method_str, path);

    // PERF: Fast-return 404 before allocating Request object
    let route_match = match route_match {
        Some(m) => m,
        None => {
            // `/metrics` is not a registered route, so serve it here before 404.
            {
                let prom_guard = prometheus.read();
                if let Some(ref p) = *prom_guard {
                    if let Some(response) = p.try_serve(path) {
                        return build_resp(&response);
                    }
                }
            }
            // Path-owning middleware (health checks, GraphQL, …) live in the
            // chain and normally run only after a route match. Give any that
            // claim this unrouted path a chance to serve it before we 404.
            if middleware.has_unrouted(method_str, path) {
                if let Some(response) = serve_unrouted(
                    req,
                    method_str,
                    &uri,
                    path,
                    middleware,
                    metrics,
                    max_body_size,
                    read_body_timeout,
                )
                .await
                {
                    return build_resp(&response);
                }
            }
            let response = Response::not_found(&format!("Not Found: {method_str} {path}"));
            return build_resp(&response);
        }
    };

    let params = route_match.params.clone();

    // PERF: Only parse query string when present
    let query_string = uri.query().unwrap_or("");
    let query: HashMap<String, String> = if query_string.is_empty() {
        HashMap::new()
    } else {
        // application/x-www-form-urlencoded: '+' denotes a space (for BOTH keys and
        // values) and the rest is percent-encoded. Previously only values had '+'
        // decoded, so a key like `a+b` was left as `a+b` instead of `a b`.
        //
        // PERF: skip the `replace`/decode work entirely when the component has
        // nothing to decode (the common case for simple query strings).
        let decode = |raw: &str| -> String {
            if !raw.as_bytes().iter().any(|&b| b == b'+' || b == b'%') {
                return raw.to_string();
            }
            let plus_decoded;
            let to_decode = if raw.contains('+') {
                plus_decoded = raw.replace('+', " ");
                plus_decoded.as_str()
            } else {
                raw
            };
            urlencoding::decode(to_decode)
                .unwrap_or_default()
                .into_owned()
        };
        query_string
            .split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                match (parts.next(), parts.next()) {
                    (Some(key), Some(value)) => Some((decode(key), decode(value))),
                    (Some(key), None) => Some((decode(key), String::new())),
                    _ => None,
                }
            })
            .collect()
    };

    // PERF: Only copy headers for matched routes (skip for 404s)
    let header_count = req.headers().len();
    let mut headers: HashMap<String, String> = HashMap::with_capacity(header_count);
    for (k, v) in req.headers().iter() {
        headers.insert(k.as_str().to_owned(), v.to_str().unwrap_or("").to_owned());
    }

    // Reject oversized bodies up-front using the declared Content-Length. This avoids
    // buffering anything for an attacker who advertises a huge payload.
    if max_body_size > 0 {
        if let Some(len) = headers
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok())
        {
            if len > max_body_size {
                metrics.inc_errors();
                let response =
                    Response::error(413, "Payload Too Large: request body exceeds limit");
                return build_resp(&response);
            }
        }
    }

    // PERF: Only collect body for methods that carry payloads.
    // GET/HEAD bodies have no defined semantics and are dropped, but DELETE and
    // OPTIONS may legitimately carry a body (RFC 7231), so those are collected below.
    let body_bytes: Vec<u8> = match method_str {
        "GET" | "HEAD" => {
            // Fast path: drop body without draining - hyper handles cleanup
            drop(req);
            Vec::new()
        }
        _ => {
            // Cap the streamed body with `Limited` so a chunked request (no
            // Content-Length) also cannot exhaust memory. `max_body_size == 0`
            // means unlimited.
            let limit = if max_body_size == 0 {
                usize::MAX
            } else {
                max_body_size
            };
            let collect_fut = Limited::new(req.into_body(), limit).collect();
            let collected = match read_body_timeout {
                Some(t) => match tokio::time::timeout(t, collect_fut).await {
                    Ok(inner) => inner,
                    Err(_) => {
                        metrics.inc_errors();
                        let response = Response::error(408, "Request Timeout: body read timed out");
                        return build_resp(&response);
                    }
                },
                None => collect_fut.await,
            };
            match collected {
                Ok(collected) => {
                    let bytes = collected.to_bytes();
                    if bytes.is_empty() {
                        Vec::new()
                    } else {
                        metrics.add_bytes_received(bytes.len() as u64);
                        bytes.to_vec()
                    }
                }
                Err(_) => {
                    // Limit exceeded or a body stream error: reject rather than
                    // silently truncating to an empty body.
                    metrics.inc_errors();
                    let response =
                        Response::error(413, "Payload Too Large: request body exceeds limit");
                    return build_resp(&response);
                }
            }
        }
    };

    // Create request object with owned data
    let method_owned = method_str.to_owned();
    let path_owned = path.to_owned();
    let mut request =
        Request::from_http(method_owned, path_owned, params, query, headers, body_bytes);

    // PERF: Skip middleware execution if no middleware registered
    if !middleware.is_empty() {
        match middleware.execute_before(&mut request) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return build_resp(&response);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_resp(&response);
            }
        }
    }

    // PERF: Skip async middleware if none registered
    if !middleware.is_async_empty() {
        match middleware.execute_before_async(&mut request).await {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return build_resp(&response);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_resp(&response);
            }
        }
    }

    // PERF: Only check Prometheus when it's actually configured (avoid lock when None)
    {
        let prom_guard = prometheus.read();
        if let Some(ref p) = *prom_guard {
            use crate::middleware::{Middleware, MiddlewareAction};
            match p.before(&mut request) {
                Ok(MiddlewareAction::Continue) => {}
                Ok(MiddlewareAction::Stop(response)) => {
                    return build_resp(&response);
                }
                Err(e) => {
                    metrics.inc_errors();
                    let response = Response::error(e.status, &e.message);
                    return build_resp(&response);
                }
            }
        }
    }

    // PERF: Only execute Guards if guards are registered
    if guards.has_guards() {
        use crate::middleware::{Middleware, MiddlewareAction};
        match Middleware::before(&**guards, &mut request) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return build_resp(&response);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_resp(&response);
            }
        }
    }

    // Handle route
    let has_after_middleware = !middleware.is_empty() || !middleware.is_async_empty();

    // PERF: Create lightweight request for after-middleware (no body copy)
    let after_request = if has_after_middleware {
        Some(request.clone_without_body())
    } else {
        None
    };

    // PERF: The error-handler chain only reads the request context when a handler
    // is actually registered. Snapshotting it unconditionally copied every header
    // on every request; now it is skipped entirely when no handlers exist (the
    // default) and reused from `after_request` otherwise.
    let error_request = if !has_after_middleware && error_handlers.has_handlers() {
        Some(request.clone_without_body())
    } else {
        None
    };

    // Pass the full request (with body) to the handler by value - no clone needed
    let handler_id = route_match.handler_id;
    let invoke_fut = handlers.invoke_async(handler_id, request, dependency_container.clone());
    let result = match handler_timeout {
        Some(t) => match tokio::time::timeout(t, invoke_fut).await {
            Ok(r) => r,
            Err(_) => {
                metrics.inc_errors();
                let response = Response::error(504, "Gateway Timeout: handler exceeded time limit");
                return build_resp(&response);
            }
        },
        None => invoke_fut.await,
    };

    // PERF: Ultra-fast path for the most common case:
    // Handler returned a dict (JsonBytes), no after-middleware, no Prometheus.
    // Skip Response struct allocation entirely and build hyper response directly.
    if !has_after_middleware && !guards.has_guards() {
        let prom_guard = prometheus.read();
        let no_prometheus = prom_guard.is_none();
        drop(prom_guard);

        if no_prometheus {
            match result {
                Ok(HandlerResult::JsonBytes(bytes)) => {
                    metrics.add_bytes_sent(bytes.len() as u64);
                    let hyper_resp = HyperResponse::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/json")
                        .body(Full::new(Bytes::from(bytes)))
                        .unwrap_or_else(|_| {
                            HyperResponse::new(Full::new(Bytes::from_static(
                                b"Internal Server Error",
                            )))
                        });
                    return Ok(hyper_resp);
                }
                _ => {}
            }
        }
    }

    let mut response = match result {
        Ok(handler_result) => match handler_result {
            // PERF: Fast path - pre-serialized JSON bytes, no serde_json::Value involved
            HandlerResult::JsonBytes(bytes) => Response::from_json_bytes(bytes, 200),
            // Slow path - Response objects that need special handling via serde_json::Value
            HandlerResult::JsonValue(json_value) => {
                if let Some(obj) = json_value.as_object() {
                    if obj
                        .get("__cello_response__")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        // Reconstruct Response from serialized format
                        let status =
                            obj.get("status").and_then(|v| v.as_u64()).unwrap_or(200) as u16;
                        let body = obj.get("body").and_then(|v| v.as_str()).unwrap_or("");

                        let mut resp = Response::new(status);
                        resp.set_body(body.as_bytes().to_vec());

                        // Copy headers
                        if let Some(headers) = obj.get("headers").and_then(|v| v.as_object()) {
                            for (key, value) in headers {
                                if let Some(v) = value.as_str() {
                                    resp.set_header(key, v);
                                }
                            }
                        }

                        resp
                    } else {
                        Response::from_json_value(json_value, 200)
                    }
                } else {
                    Response::from_json_value(json_value, 200)
                }
            }
        },
        Err(err) => {
            metrics.inc_errors();
            let app_error = match err {
                HandlerError::Python(info) => AppError::PythonException(info),
                HandlerError::Message(message) => AppError::Internal(message),
            };
            // Prefer the after-middleware snapshot for context; fall back to the
            // error snapshot (used when there is no after-middleware) or an empty
            // request when no error handlers are registered — then it is unused.
            let owned_ctx;
            let ctx: &Request = match after_request.as_ref().or(error_request.as_ref()) {
                Some(r) => r,
                None => {
                    owned_ctx = Request::default();
                    &owned_ctx
                }
            };
            // Already dispatched through the error-handler chain; mark it so
            // `build_hyper_response` does not dispatch it a second time.
            let mut handled = error_handlers.handle(&app_error, ctx, None);
            handled.set_header(ERROR_HANDLED_HEADER, "1");
            handled
        }
    };

    // Restore request for after-middleware (the original was moved into the handler)
    let request = after_request.unwrap_or_default();

    // PERF: Skip after middleware if none registered
    if !middleware.is_async_empty() {
        match middleware
            .execute_after_async(&request, &mut response)
            .await
        {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(new_response)) => {
                return build_resp(&new_response);
            }
            Err(e) => {
                metrics.inc_errors();
                let error_response = Response::error(e.status, &e.message);
                return build_resp(&error_response);
            }
        }
    }

    if !middleware.is_empty() {
        match middleware.execute_after(&request, &mut response) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(new_response)) => {
                return build_resp(&new_response);
            }
            Err(e) => {
                metrics.inc_errors();
                let error_response = Response::error(e.status, &e.message);
                return build_resp(&error_response);
            }
        }
    }

    // PERF: Only run Prometheus after if configured
    {
        let prom_guard = prometheus.read();
        if let Some(ref p) = *prom_guard {
            use crate::middleware::Middleware;
            let _ = p.after(&request, &mut response);
        }
    }

    build_resp(&response)
}

/// Header marking a response that already went through the error-handler chain;
/// `build_hyper_response` skips such responses to avoid double dispatch.
const ERROR_HANDLED_HEADER: &str = "x-cello-error-handled";

/// Map an HTTP status code to the corresponding `AppError` variant so
/// framework-generated error responses can be dispatched to
/// `@app.exception_handler(...)` / status / global handlers.
fn status_to_app_error(status: u16, message: &str) -> AppError {
    match status {
        400 => AppError::BadRequest(message.to_string()),
        401 => AppError::Unauthorized(message.to_string()),
        403 => AppError::Forbidden(message.to_string()),
        404 => AppError::NotFound(message.to_string()),
        409 => AppError::Conflict(message.to_string()),
        429 => AppError::RateLimited {
            retry_after: None,
            message: message.to_string(),
        },
        504 => AppError::Timeout(message.to_string()),
        500 => AppError::Internal(message.to_string()),
        _ => AppError::Custom {
            status,
            message: message.to_string(),
            type_uri: None,
        },
    }
}

/// Build a Hyper response from our Response type.
/// PERF: the body is shared as `Bytes` (refcount bump), not copied.
#[inline]
fn build_hyper_response_inner(
    response: &Response,
    metrics: &Arc<ServerMetrics>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = HyperResponse::builder().status(status);

    for (key, value) in &response.headers {
        // Strip the internal "already handled" marker from the wire response.
        if key == ERROR_HANDLED_HEADER {
            continue;
        }
        builder = builder.header(key.as_str(), value.as_str());
    }

    // PERF: `body_buffer()` is a cheap `Bytes` clone (atomic increment) — the
    // response body is shared with Hyper instead of copied.
    let body = response.body_buffer();
    metrics.add_bytes_sent(body.len() as u64);
    let body = Full::new(body);

    Ok(builder.body(body).unwrap_or_else(|_| {
        HyperResponse::new(Full::new(Bytes::from_static(b"Internal Server Error")))
    }))
}

/// Build a Hyper response, routing non-2xx responses through the error-handler
/// chain when a matching handler is registered.
///
/// This is the single hook for every framework-generated error (404, 413, 408,
/// 504, middleware/guard failures, …): a `status >= 400` response becomes the
/// corresponding `AppError` and is dispatched via
/// [`ErrorHandlerRegistry::handle`]. Responses already marked with
/// [`ERROR_HANDLED_HEADER`] (i.e. produced by the error-handler chain itself)
/// are returned unchanged to avoid double dispatch.
#[inline]
fn build_hyper_response(
    response: &Response,
    metrics: &Arc<ServerMetrics>,
    error_handlers: &Arc<ErrorHandlerRegistry>,
    method: &str,
    path: &str,
    request: Option<&Request>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    if response.status >= 400 && !response.headers.contains_key(ERROR_HANDLED_HEADER) {
        let message = String::from_utf8_lossy(response.body_bytes());
        let error = status_to_app_error(response.status, &message);
        if error_handlers.has_handler_for(&error) {
            // PERF: build the lightweight method/path context only on the rare
            // path that actually dispatches to a registered error handler.
            let owned_ctx;
            let err_request = match request {
                Some(r) => r,
                None => {
                    owned_ctx = Request::from_http(
                        method.to_owned(),
                        path.to_owned(),
                        HashMap::new(),
                        HashMap::new(),
                        HashMap::new(),
                        Vec::new(),
                    );
                    &owned_ctx
                }
            };
            let handled = error_handlers.handle(&error, err_request, None);
            return build_hyper_response_inner(&handled, metrics);
        }
    }
    build_hyper_response_inner(response, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config() {
        let config = ServerConfig::new("0.0.0.0", 8080)
            .workers(4)
            .max_connections(5000)
            .shutdown_timeout(Duration::from_secs(60));

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.workers, 4);
        assert_eq!(config.max_connections, 5000);
    }

    #[test]
    fn test_server_metrics() {
        let metrics = ServerMetrics::new();

        metrics.inc_requests();
        metrics.inc_requests();
        metrics.inc_connections();

        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 1);

        metrics.dec_connections();
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_snapshot() {
        let metrics = ServerMetrics::new();
        metrics.inc_requests();
        metrics.add_bytes_received(100);
        metrics.add_bytes_sent(200);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 1);
        assert_eq!(snapshot.bytes_received, 100);
        assert_eq!(snapshot.bytes_sent, 200);
    }

    #[tokio::test]
    async fn test_shutdown_coordinator() {
        let shutdown = ShutdownCoordinator::new(Duration::from_secs(5));

        shutdown.request_started();
        assert_eq!(shutdown.active_requests(), 1);

        shutdown.request_finished();
        assert_eq!(shutdown.active_requests(), 0);

        assert!(!shutdown.is_shutting_down());
        shutdown.shutdown();
        assert!(shutdown.is_shutting_down());
    }
}
