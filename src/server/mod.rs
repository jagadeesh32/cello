//! HTTP Server implementation for Cello.
//!
//! This module provides:
//! - High-performance async HTTP server
//! - Graceful shutdown support
//! - Cluster mode (multi-process)
//! - HTTP/1.1 (with WebSocket upgrades). `Http2Config` / `Http3Config` /
//!   `TlsConfig` are configuration records only and are not used by the accept
//!   loop; terminate TLS / HTTP/2 at a proxy.
//! - Server metrics

pub mod body;
pub mod cluster;
pub mod protocols;
pub mod proxy;

use body::{FileBody, ResBody};
use bytes::Bytes;
use http_body_util::{BodyExt, Limited};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{body::Incoming, Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use parking_lot::RwLock;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::handler::{HandlerRegistry, HandlerResult};
use crate::middleware::{MiddlewareAction, MiddlewareChain};
use crate::request::Request;
use crate::response::Response;
use crate::router::Router;
use crate::websocket::WebSocketRegistry;

pub use cluster::{ClusterConfig, ClusterManager};
pub use protocols::{Http2Config, Http3Config, TlsConfig};

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
    /// Close a keep-alive connection that has had no request in flight for this
    /// long; None = disabled (the header-read timeout still bounds idle waits).
    pub idle_timeout: Option<Duration>,
    /// Maximum size of the request head (request line + headers) in bytes;
    /// None = hyper's default. Larger heads get `431`.
    pub max_header_size: Option<usize>,
    /// Requests served on one connection before it is closed; 0 = unlimited.
    pub max_requests_per_connection: usize,
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
    /// Proxies whose forwarding headers this app trusts. Read once per
    /// connection, so `App.set_trusted_proxies` applies to new connections.
    pub trusted_proxies: Arc<RwLock<Arc<proxy::TrustedProxies>>>,
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
            // On by default: without it a client can hold a connection open
            // forever by trickling header bytes (Slowloris).
            read_header_timeout: Some(Duration::from_secs(30)),
            read_body_timeout: None,
            handler_timeout: None,
            idle_timeout: None,
            max_header_size: None,
            max_requests_per_connection: 0,
            shutdown_timeout: Duration::from_secs(30),
            tls: None,
            http2: None,
            http3: None,
            cluster: None,
            trusted_proxies: Arc::new(RwLock::new(Arc::new(proxy::TrustedProxies::default()))),
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

/// Whether unhandled handler errors are described to the client (development) or
/// replaced by a generic 500 (production). Error text can carry database messages,
/// constraint names and row values, so production must never echo it. Default is
/// off (fail safe); `App.run()` turns it on only in debug mode.
static EXPOSE_ERROR_DETAILS: AtomicBool = AtomicBool::new(false);

/// Set whether unhandled handler errors are described in 500 responses.
pub fn set_expose_error_details(expose: bool) {
    EXPOSE_ERROR_DETAILS.store(expose, Ordering::Relaxed);
}

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

        // Banner and server details are printed by Python

        let router = Arc::new(self.router);
        let handlers = Arc::new(self.handlers);
        let middleware = Arc::new(self.middleware);
        // Only consulted when at least one WebSocket route exists, so plain HTTP
        // servers pay nothing for it.
        let websocket_handlers =
            (!self.websocket_handlers.is_empty()).then(|| Arc::new(self.websocket_handlers));
        let metrics = Arc::new(self.metrics);
        let shutdown = Arc::new(self.shutdown);
        let dependency_container = self.dependency_container.clone();
        let guards = self.guards.clone();
        let prometheus = self.prometheus.clone();

        // Copy limit/timeout config out of self.config (all Copy) so each connection
        // task can capture them cheaply.
        let max_body_size = self.config.max_body_size;
        let trusted_proxies = self.config.trusted_proxies.clone();
        let read_body_timeout = self.config.read_body_timeout;
        let read_header_timeout = self.config.read_header_timeout;
        let handler_timeout = self.config.handler_timeout;
        let idle_timeout = self.config.idle_timeout;
        let max_header_size = self.config.max_header_size;
        let max_requests_per_connection = self.config.max_requests_per_connection;
        let max_connections = self.config.max_connections as u64;

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
        // iteration lost a SIGINT that landed while an accepted connection was
        // being set up — under load the server then ignored Ctrl-C and hung.
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
                            // Reserve the slot first, then check: a separate
                            // load-then-increment let a burst of accepts all pass
                            // the check before any of them was counted.
                            let previous = metrics.active_connections.fetch_add(1, Ordering::Relaxed);
                            if max_connections > 0 && previous >= max_connections {
                                metrics.dec_connections();
                                eprintln!("Connection limit reached, rejecting connection from {peer_addr}");
                                continue;
                            }

                            // PERF: Apply TCP_NODELAY to reduce latency for small responses
                            let _ = stream.set_nodelay(true);
                            let peer_ip = peer_addr.ip();

                            let io = TokioIo::new(stream);
                            let router = router.clone();
                            let handlers = handlers.clone();
                            let middleware = middleware.clone();
                            let metrics_for_service = metrics.clone();
                            let metrics_for_cleanup = metrics.clone();
                            let shutdown = shutdown.clone();

                            let dependency_container = dependency_container.clone();
                            let guards = guards.clone();
                            let prometheus = prometheus.clone();
                            let websocket_handlers = websocket_handlers.clone();
                            let trusted_proxies = trusted_proxies.read().clone();

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
                                let conn_websockets = websocket_handlers;
                                let conn_proxies = trusted_proxies;
                                // Copy limit/timeout config (all Copy) into the connection scope.
                                let conn_max_body_size = max_body_size;
                                let conn_read_body_timeout = read_body_timeout;
                                let conn_handler_timeout = handler_timeout;
                                // Per-connection bookkeeping for the request cap and
                                // the idle watchdog — only when one is configured,
                                // so the default path pays nothing for it.
                                let conn_track = (idle_timeout.is_some()
                                    || max_requests_per_connection > 0)
                                    .then(|| Arc::new(ConnTrack::new()));
                                let svc_track = conn_track.clone();

                                let service = service_fn(move |req| {
                                    let router = conn_router.clone();
                                    let handlers = conn_handlers.clone();
                                    let middleware = conn_middleware.clone();
                                    let metrics = conn_metrics.clone();
                                    let shutdown = conn_shutdown.clone();
                                    let dependency_container = conn_deps.clone();
                                    let guards = conn_guards.clone();
                                    let prometheus = conn_prometheus.clone();
                                    let websockets = conn_websockets.clone();
                                    let proxies = conn_proxies.clone();
                                    let req_max_body_size = conn_max_body_size;
                                    let req_read_body_timeout = conn_read_body_timeout;
                                    let req_handler_timeout = conn_handler_timeout;
                                    let track = svc_track.clone();

                                    async move {
                                        shutdown.request_started();
                                        // Decrements on drop too: a request future
                                        // dropped mid-flight (client gone) must not
                                        // leave the idle watchdog thinking it runs.
                                        let watch_idle = idle_timeout.is_some();
                                        let _in_flight = track
                                            .as_ref()
                                            .filter(|_| watch_idle)
                                            .map(|t| InFlight::enter(t.clone()));
                                        let start = Instant::now();

                                        let result = handle_request(
                                            req,
                                            peer_ip,
                                            &proxies,
                                            websockets.as_ref(),
                                            &router,
                                            &handlers,
                                            &middleware,
                                            &metrics,
                                            &dependency_container,
                                            &guards,
                                            &prometheus,
                                            req_max_body_size,
                                            req_read_body_timeout,
                                            req_handler_timeout,
                                        )
                                        .await;

                                        metrics.record_latency(start.elapsed());
                                        shutdown.request_finished();
                                        let Some(track) = track else {
                                            return result;
                                        };
                                        if watch_idle {
                                            track.touch();
                                        }

                                        // Ask hyper to close the connection once the
                                        // per-connection request budget is spent.
                                        let n = track.served.fetch_add(1, Ordering::Relaxed) + 1;
                                        if max_requests_per_connection > 0
                                            && n >= max_requests_per_connection as u64
                                        {
                                            return result.map(|mut resp| {
                                                resp.headers_mut().insert(
                                                    hyper::header::CONNECTION,
                                                    hyper::header::HeaderValue::from_static("close"),
                                                );
                                                resp
                                            });
                                        }

                                        result
                                    }
                                });

                                // PERF: Enable keep-alive and pipelining for better throughput.
                                // header_read_timeout guards against Slowloris (slow header send).
                                let mut builder = http1::Builder::new();
                                builder.keep_alive(true).pipeline_flush(true);
                                if let Some(t) = read_header_timeout {
                                    // hyper 1.x requires an explicit timer for
                                    // header_read_timeout; without it the
                                    // connection task panics on first use.
                                    builder.timer(hyper_util::rt::TokioTimer::new());
                                    builder.header_read_timeout(t);
                                }
                                if let Some(size) = max_header_size {
                                    // hyper's floor for the read buffer is 8 KiB.
                                    builder.max_buf_size(size.max(8192));
                                }
                                let conn = builder.serve_connection(io, service).with_upgrades();
                                tokio::pin!(conn);
                                let serve_res: Result<(), hyper::Error> = match idle_timeout {
                                    None => conn.as_mut().await,
                                    Some(idle) => {
                                        let track = conn_track.expect("tracked when idle_timeout is set");
                                        // Watchdog on its own task, so the
                                        // connection future is not re-polled
                                        // alongside a timer on every wake-up.
                                        // Once no request is in flight and
                                        // none has finished for `idle`, it
                                        // asks for a graceful close.
                                        let idle_signal = Arc::new(tokio::sync::Notify::new());
                                        let watchdog = tokio::task::spawn(idle_watchdog(
                                            Arc::downgrade(&track),
                                            idle,
                                            idle_signal.clone(),
                                        ));
                                        drop(track);
                                        let idle_notified = idle_signal.notified();
                                        tokio::pin!(idle_notified);
                                        let res = tokio::select! {
                                            biased;
                                            res = conn.as_mut() => res,
                                            _ = &mut idle_notified => {
                                                conn.as_mut().graceful_shutdown();
                                                conn.as_mut().await
                                            }
                                        };
                                        watchdog.abort();
                                        res
                                    }
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

/// Per-connection state for `max_requests_per_connection` and `idle_timeout`.
struct ConnTrack {
    served: AtomicU64,
    in_flight: AtomicU64,
    /// When the connection was accepted; `last_active` counts from here.
    opened: Instant,
    /// Milliseconds after `opened` at which the last request finished.
    last_active: AtomicU64,
}

impl ConnTrack {
    fn new() -> Self {
        ConnTrack {
            served: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            opened: Instant::now(),
            last_active: AtomicU64::new(0),
        }
    }

    /// Record that a request just finished.
    fn touch(&self) {
        let ms = self.opened.elapsed().as_millis() as u64;
        self.last_active.store(ms, Ordering::Relaxed);
    }

    /// Time since the last request finished (or since the accept).
    fn idle_for(&self) -> Duration {
        let last = Duration::from_millis(self.last_active.load(Ordering::Relaxed));
        self.opened.elapsed().saturating_sub(last)
    }
}

/// Signal `close` once the connection has had no request in flight and none
/// finished for `idle`. Ends when the connection is gone.
async fn idle_watchdog(
    track: std::sync::Weak<ConnTrack>,
    idle: Duration,
    close: Arc<tokio::sync::Notify>,
) {
    let tick = (idle / 4).max(Duration::from_millis(50));
    loop {
        tokio::time::sleep(tick).await;
        let Some(track) = track.upgrade() else {
            return;
        };
        if track.in_flight.load(Ordering::Relaxed) == 0 && track.idle_for() >= idle {
            close.notify_one();
            return;
        }
    }
}

/// In-flight request guard (decrements on drop, including a dropped future).
struct InFlight(Arc<ConnTrack>);

impl InFlight {
    fn enter(track: Arc<ConnTrack>) -> Self {
        track.in_flight.fetch_add(1, Ordering::Relaxed);
        InFlight(track)
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Parse an `application/x-www-form-urlencoded` query string. '+' denotes a
/// space (for BOTH keys and values) and the rest is percent-encoded.
fn parse_query(query_string: &str) -> HashMap<String, String> {
    if query_string.is_empty() {
        return HashMap::new();
    }
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
}

/// Copy request headers into a map. A repeated header is joined into one value
/// (`; ` for `Cookie`, `, ` otherwise, per RFC 9110 §5.3) instead of the last
/// occurrence silently replacing the earlier ones.
fn collect_headers(headers: &hyper::HeaderMap) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::with_capacity(headers.len());
    for (k, v) in headers.iter() {
        let value = v.to_str().unwrap_or("");
        match out.get_mut(k.as_str()) {
            Some(existing) => {
                existing.push_str(if k == hyper::header::COOKIE {
                    "; "
                } else {
                    ", "
                });
                existing.push_str(value);
            }
            None => {
                out.insert(k.as_str().to_owned(), value.to_owned());
            }
        }
    }
    out
}

/// Finish a response that was decided before the handler ran (a middleware
/// `Stop`, a middleware error, a guard rejection, an unrouted endpoint). The
/// sync `after` hooks still apply, so these responses get security headers,
/// CORS headers, compression and cookies.
async fn finish_early(
    middleware: &Arc<MiddlewareChain>,
    request: &mut Request,
    mut response: Response,
    metrics: &Arc<ServerMetrics>,
) -> Result<HyperResponse<ResBody>, Infallible> {
    let file = open_deferred_body(&mut response).await;
    if !middleware.is_empty() {
        // Lets accounting middleware (circuit breaker) tell a handler outcome
        // from a response the pipeline produced itself.
        request.context.insert(
            crate::middleware::EARLY_RESPONSE_KEY.to_string(),
            serde_json::Value::Bool(true),
        );
        middleware.execute_after_lenient(request, &mut response);
    }
    build_hyper_response_with(&response, metrics, file)
}

/// Resolve a deferred file body (static files, `sendfile`, `file_range`) with
/// `tokio::fs`, which runs the blocking calls on the blocking pool instead of
/// the thread that accepts and parses connections. Honours the internal
/// `X-Sendfile-Offset` / `X-Sendfile-Length` range headers and strips them.
///
/// A file up to [`body::STREAM_THRESHOLD`] is read into the response, so the
/// `after` hooks (compression, ETag, cache) see its bytes. A larger one is
/// returned as a [`FileBody`] to stream; the response keeps its deferred-file
/// marker and the hooks leave its body alone.
async fn open_deferred_body(response: &mut Response) -> Option<FileBody> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let path = response.deferred_file()?.to_owned();
    let offset = response
        .header("X-Sendfile-Offset")
        .and_then(|v| v.parse::<u64>().ok());
    let length = response
        .header("X-Sendfile-Length")
        .and_then(|v| v.parse::<u64>().ok());
    for internal in ["X-Sendfile-Path", "X-Sendfile-Offset", "X-Sendfile-Length"] {
        response.remove_header(internal);
    }

    let open = async {
        let mut file = tokio::fs::File::open(&path).await?;
        let size = file.metadata().await?.len();
        let (start, len) = match (offset, length) {
            (Some(off), Some(len)) => (off, len.min(size.saturating_sub(off))),
            _ => (0, size),
        };
        if start > 0 {
            file.seek(std::io::SeekFrom::Start(start)).await?;
        }
        if len <= body::STREAM_THRESHOLD {
            let mut buf = Vec::with_capacity(len as usize);
            file.take(len).read_to_end(&mut buf).await?;
            Ok::<_, std::io::Error>(Err(buf))
        } else {
            Ok(Ok(FileBody::new(file, len)))
        }
    };
    match open.await {
        Ok(Ok(stream)) => Some(stream),
        Ok(Err(bytes)) => {
            response.set_body(bytes);
            None
        }
        Err(e) => {
            eprintln!("[cello] failed to read {path}: {e}");
            *response = Response::not_found("Not Found");
            None
        }
    }
}

/// Complete a WebSocket handshake for a registered WebSocket path. The sync
/// middleware chain runs first, so authentication, rate limits and CORS apply
/// to the upgrade exactly as to an HTTP route.
async fn websocket_upgrade(
    mut req: HyperRequest<Incoming>,
    route: crate::websocket::WebSocketRoute,
    peer_ip: IpAddr,
    proxies: &proxy::TrustedProxies,
    middleware: &Arc<MiddlewareChain>,
    guards: &Arc<crate::middleware::guards::GuardsMiddleware>,
    metrics: &Arc<ServerMetrics>,
) -> Result<HyperResponse<ResBody>, Infallible> {
    let headers = collect_headers(req.headers());
    let version_ok = headers
        .get("sec-websocket-version")
        .is_some_and(|v| v.trim() == "13");
    let Some(key) = headers
        .get("sec-websocket-key")
        .filter(|_| version_ok)
        .map(|k| k.trim().to_owned())
    else {
        let response = Response::error(400, "Bad Request: invalid WebSocket handshake");
        return build_hyper_response(&response, metrics);
    };

    let uri = req.uri().clone();
    let mut request = Request::from_http(
        "GET".to_owned(),
        uri.path().to_owned(),
        HashMap::new(),
        parse_query(uri.query().unwrap_or("")),
        headers,
        Vec::new(),
    );
    let (client_addr, from_trusted_proxy) = proxies.resolve_client(peer_ip, &request.headers);
    request.remote_addr = Some(peer_ip);
    request.client_addr = Some(client_addr);
    request.from_trusted_proxy = from_trusted_proxy;

    // Cross-site WebSocket hijacking: a browser handshake must come from this
    // site (or an allowed origin) before cookies on it are trusted.
    let host = if from_trusted_proxy {
        request
            .headers
            .get("x-forwarded-host")
            .or_else(|| request.headers.get("host"))
    } else {
        request.headers.get("host")
    };
    if !crate::websocket::origin_allowed(
        request.headers.get("origin").map(String::as_str),
        host.map(String::as_str),
        &route.allowed_origins,
    ) {
        let response = Response::error(403, "Forbidden: WebSocket origin not allowed");
        return finish_early(middleware, &mut request, response, metrics).await;
    }

    if !middleware.is_empty() {
        match middleware.execute_before(&mut request) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return finish_early(middleware, &mut request, response, metrics).await;
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return finish_early(middleware, &mut request, response, metrics).await;
            }
        }
    }

    // App-level guards (`app.add_guard`) protect WebSocket routes too.
    if guards.has_guards() {
        use crate::middleware::Middleware;
        let outcome = Middleware::before(&**guards, &mut request);
        match outcome {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return finish_early(middleware, &mut request, response, metrics).await;
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return finish_early(middleware, &mut request, response, metrics).await;
            }
        }
    }

    let on_upgrade = hyper::upgrade::on(&mut req);
    let ws_metrics = metrics.clone();
    tokio::task::spawn(async move {
        match on_upgrade.await {
            Ok(upgraded) => {
                // The HTTP connection task ends at the upgrade; count the live
                // socket separately so it still counts toward max_connections.
                ws_metrics.inc_connections();
                crate::websocket::serve_socket(upgraded, route.handler, request).await;
                ws_metrics.dec_connections();
            }
            Err(e) => eprintln!("[cello] WebSocket upgrade failed: {e}"),
        }
    });

    let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
    Ok(HyperResponse::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(hyper::header::UPGRADE, "websocket")
        .header(hyper::header::CONNECTION, "Upgrade")
        .header("Sec-WebSocket-Accept", accept)
        .body(body::full(Bytes::new()))
        .unwrap_or_else(|_| HyperResponse::new(body::full(Bytes::new()))))
}

#[allow(clippy::too_many_arguments)]
async fn handle_request(
    req: HyperRequest<Incoming>,
    peer_ip: IpAddr,
    proxies: &proxy::TrustedProxies,
    websockets: Option<&Arc<WebSocketRegistry>>,
    router: &Arc<Router>,
    handlers: &Arc<HandlerRegistry>,
    middleware: &Arc<MiddlewareChain>,
    metrics: &Arc<ServerMetrics>,
    dependency_container: &Arc<crate::dependency::DependencyContainer>,
    guards: &Arc<crate::middleware::guards::GuardsMiddleware>,
    prometheus: &Arc<
        parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>,
    >,
    max_body_size: usize,
    read_body_timeout: Option<Duration>,
    handler_timeout: Option<Duration>,
) -> Result<HyperResponse<ResBody>, Infallible> {
    metrics.inc_requests();

    // PERF: Extract method and path WITHOUT owning - use references as long as possible
    let method = req.method().clone();
    let method_str = method.as_str();
    let uri = req.uri().clone();
    let path = uri.path();

    // WebSocket upgrade for a registered WebSocket path.
    if let Some(ws) = websockets {
        if method == hyper::Method::GET && crate::websocket::is_upgrade_request(req.headers()) {
            if let Some(route) = ws.get(path) {
                return websocket_upgrade(
                    req, route, peer_ip, proxies, middleware, guards, metrics,
                )
                .await;
            }
        }
    }

    // PERF: Route match FIRST - fail fast on 404 before any allocation
    let route_match = router.match_route(method_str, path);

    // An unrouted path is served only when something owns it: the Prometheus
    // endpoint or a path-owning middleware (health checks, GraphQL). Those
    // requests go through the normal middleware chain below, so authentication,
    // rate limiting and security headers apply to them as to any route. A
    // genuine miss still fast-404s before auth (no 404→401 route-existence leak).
    let unrouted = route_match.is_none();
    if unrouted {
        let prometheus_owns = prometheus.read().as_ref().is_some_and(|p| p.serves(path));
        if !prometheus_owns && !middleware.has_unrouted(method_str, path) {
            let response = Response::not_found(&format!("Not Found: {method_str} {path}"));
            return build_hyper_response(&response, metrics);
        }
    }

    let (handler_id, params) = match route_match {
        Some(m) => (m.handler_id, m.params),
        None => (0, HashMap::new()),
    };

    // PERF: Only parse query string when present
    let query = parse_query(uri.query().unwrap_or(""));

    let headers = collect_headers(req.headers());

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
                return build_hyper_response(&response, metrics);
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
                        return build_hyper_response(&response, metrics);
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
                        Vec::from(bytes)
                    }
                }
                Err(_) => {
                    // Limit exceeded or a body stream error: reject rather than
                    // silently truncating to an empty body.
                    metrics.inc_errors();
                    let response =
                        Response::error(413, "Payload Too Large: request body exceeds limit");
                    return build_hyper_response(&response, metrics);
                }
            }
        }
    };

    // Create request object with owned data
    let method_owned = method_str.to_owned();
    let path_owned = path.to_owned();
    let mut request =
        Request::from_http(method_owned, path_owned, params, query, headers, body_bytes);
    let (client_addr, from_trusted_proxy) = proxies.resolve_client(peer_ip, &request.headers);
    request.remote_addr = Some(peer_ip);
    request.client_addr = Some(client_addr);
    request.from_trusted_proxy = from_trusted_proxy;

    // PERF: Skip middleware execution if no middleware registered
    if !middleware.is_empty() {
        let result = if unrouted {
            middleware.execute_before_deferring_owners(&mut request)
        } else {
            middleware.execute_before(&mut request)
        };
        match result {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return finish_early(middleware, &mut request, response, metrics).await;
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return finish_early(middleware, &mut request, response, metrics).await;
            }
        }
    }

    // PERF: Skip async middleware if none registered
    if !middleware.is_async_empty() {
        let result = if unrouted {
            middleware
                .execute_before_async_deferring_owners(&mut request)
                .await
        } else {
            middleware.execute_before_async(&mut request).await
        };
        match result {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                // A cache HIT lands here: it still gets the sync `after` hooks.
                return finish_early(middleware, &mut request, response, metrics).await;
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return finish_early(middleware, &mut request, response, metrics).await;
            }
        }
    }

    if unrouted {
        // Authentication and rate limiting passed and no path-owning middleware
        // answered: this is the Prometheus endpoint, or nothing.
        let served = prometheus.read().as_ref().and_then(|p| p.try_serve(path));
        let response = served
            .unwrap_or_else(|| Response::not_found(&format!("Not Found: {method_str} {path}")));
        return finish_early(middleware, &mut request, response, metrics).await;
    }

    // PERF: Only check Prometheus when it's actually configured (avoid lock when None)
    let prom_result = {
        let prom_guard = prometheus.read();
        prom_guard.as_ref().map(|p| {
            use crate::middleware::Middleware;
            p.before(&mut request)
        })
    };
    match prom_result {
        None | Some(Ok(MiddlewareAction::Continue)) => {}
        Some(Ok(MiddlewareAction::Stop(response))) => {
            return finish_early(middleware, &mut request, response, metrics).await;
        }
        Some(Err(e)) => {
            metrics.inc_errors();
            let response = Response::error(e.status, &e.message);
            return finish_early(middleware, &mut request, response, metrics).await;
        }
    }

    // PERF: Only execute Guards if guards are registered
    if guards.has_guards() {
        use crate::middleware::{Middleware, MiddlewareAction};
        match Middleware::before(&**guards, &mut request) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return finish_early(middleware, &mut request, response, metrics).await;
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return finish_early(middleware, &mut request, response, metrics).await;
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

    // Pass the full request (with body) to the handler by value - no clone needed
    let invoke_fut = handlers.invoke_async(handler_id, request, dependency_container.clone());
    let result = match handler_timeout {
        Some(t) => match tokio::time::timeout(t, invoke_fut).await {
            Ok(r) => r,
            Err(_) => {
                metrics.inc_errors();
                let response = Response::error(504, "Gateway Timeout: handler exceeded time limit");
                return build_hyper_response(&response, metrics);
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
                        .body(body::full(bytes))
                        .unwrap_or_else(|_| {
                            HyperResponse::new(body::full(Bytes::from_static(
                                b"Internal Server Error",
                            )))
                        });
                    return Ok(hyper_resp);
                }
                _ => {}
            }
        }
    }

    // Restore request for after-middleware (the original was moved into the handler)
    let request = after_request.unwrap_or_default();

    let mut response = match result {
        Ok(handler_result) => match handler_result {
            // PERF: Fast path - pre-serialized JSON bytes, no serde_json::Value involved
            HandlerResult::JsonBytes(bytes) => Response::from_json_bytes(bytes, 200),
            HandlerResult::Response(response) => *response,
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
            if EXPOSE_ERROR_DETAILS.load(Ordering::Relaxed) {
                Response::error(500, &err)
            } else {
                eprintln!("[cello] 500 {method_str} {path}: {err}");
                Response::error(500, "Internal Server Error")
            }
        }
    };

    let file = if response.deferred_file().is_some() {
        open_deferred_body(&mut response).await
    } else {
        None
    };

    // PERF: Skip after middleware if none registered
    if !middleware.is_async_empty() {
        match middleware
            .execute_after_async(&request, &mut response)
            .await
        {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(new_response)) => {
                return build_hyper_response(&new_response, metrics);
            }
            Err(e) => {
                metrics.inc_errors();
                let error_response = Response::error(e.status, &e.message);
                return build_hyper_response(&error_response, metrics);
            }
        }
    }

    if !middleware.is_empty() {
        match middleware.execute_after(&request, &mut response) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(new_response)) => {
                return build_hyper_response(&new_response, metrics);
            }
            Err(e) => {
                metrics.inc_errors();
                let error_response = Response::error(e.status, &e.message);
                return build_hyper_response(&error_response, metrics);
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

    build_hyper_response_with(&response, metrics, file)
}

/// Build a Hyper response from our Response type.
/// PERF: Avoid unnecessary copies - use Bytes::copy_from_slice directly.
#[inline]
fn build_hyper_response(
    response: &Response,
    metrics: &Arc<ServerMetrics>,
) -> Result<HyperResponse<ResBody>, Infallible> {
    build_hyper_response_with(response, metrics, None)
}

/// [`build_hyper_response`], streaming `file` as the body when the response
/// still describes that file (an `after` hook may have replaced it).
#[inline]
fn build_hyper_response_with(
    response: &Response,
    metrics: &Arc<ServerMetrics>,
    file: Option<FileBody>,
) -> Result<HyperResponse<ResBody>, Infallible> {
    let file = file.filter(|_| response.deferred_file().is_some());
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = HyperResponse::builder().status(status);

    for (key, value) in &response.headers {
        // The body is always sent as one `Full` buffer and hyper derives the
        // framing from it. A stale `Content-Length` (left by compression or a
        // file helper) or a `Transfer-Encoding` would contradict the bytes sent.
        // An empty body keeps its declared length: that is a HEAD response
        // describing the representation it omits.
        if key.eq_ignore_ascii_case("transfer-encoding")
            || (key.eq_ignore_ascii_case("content-length")
                && (file.is_some() || !response.body_bytes().is_empty()))
        {
            continue;
        }
        builder = builder.header(key.as_str(), value.as_str());
    }
    for (key, value) in response.extra_headers() {
        builder = builder.header(key.as_str(), value.as_str());
    }

    let body = match file {
        // hyper takes the length from the body's exact size hint.
        Some(file) => {
            metrics.add_bytes_sent(file.remaining());
            http_body_util::Either::Right(file)
        }
        None => {
            // PERF: Create Bytes directly from slice - avoids intermediate Vec allocation
            let body_slice = response.body_bytes();
            metrics.add_bytes_sent(body_slice.len() as u64);
            body::full(Bytes::copy_from_slice(body_slice))
        }
    };

    Ok(builder.body(body).unwrap_or_else(|_| {
        HyperResponse::new(body::full(Bytes::from_static(b"Internal Server Error")))
    }))
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
