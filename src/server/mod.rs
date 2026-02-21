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
pub mod protocols;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{body::Incoming, Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use parking_lot::RwLock;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
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
        let _websocket_handlers = Arc::new(self.websocket_handlers);
        let metrics = Arc::new(self.metrics);
        let shutdown = Arc::new(self.shutdown);
        let dependency_container = self.dependency_container.clone();
        let guards = self.guards.clone();
        let prometheus = self.prometheus.clone();

        let mut shutdown_rx = shutdown.subscribe();

        // Listen for SIGTERM (systemd, process manager, multi-worker shutdown)
        let shutdown_sigterm = shutdown.clone();
        tokio::task::spawn(async move {
            #[cfg(unix)]
            if let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                let _ = sig.recv().await;
                shutdown_sigterm.shutdown();
            }
        });

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
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

                                let service = service_fn(move |req| {
                                    let router = conn_router.clone();
                                    let handlers = conn_handlers.clone();
                                    let middleware = conn_middleware.clone();
                                    let metrics = conn_metrics.clone();
                                    let shutdown = conn_shutdown.clone();
                                    let dependency_container = conn_deps.clone();
                                    let guards = conn_guards.clone();
                                    let prometheus = conn_prometheus.clone();

                                    async move {
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
                                        )
                                        .await;

                                        metrics.record_latency(start.elapsed());
                                        shutdown.request_finished();

                                        result
                                    }
                                });

                                // PERF: Enable keep-alive and pipelining for better throughput
                                let serve_res: Result<(), hyper::Error> = http1::Builder::new()
                                    .keep_alive(true)
                                    .pipeline_flush(true)
                                    .serve_connection(io, service)
                                    .await;
                                
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

async fn handle_request(
    req: HyperRequest<Incoming>,
    router: &Arc<Router>,
    handlers: &Arc<HandlerRegistry>,
    middleware: &Arc<MiddlewareChain>,
    metrics: &Arc<ServerMetrics>,
    dependency_container: &Arc<crate::dependency::DependencyContainer>,
    guards: &Arc<crate::middleware::guards::GuardsMiddleware>,
    prometheus: &Arc<
        parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>,
    >,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    metrics.inc_requests();

    // PERF: Extract method and path WITHOUT owning - use references as long as possible
    let method = req.method().clone();
    let method_str = method.as_str();
    let uri = req.uri().clone();
    let path = uri.path();

    // PERF: Route match FIRST - fail fast on 404 before any allocation
    let route_match = router.match_route(method_str, path);

    // PERF: Fast-return 404 before allocating Request object
    let route_match = match route_match {
        Some(m) => m,
        None => {
            let response = Response::not_found(&format!("Not Found: {method_str} {path}"));
            return build_hyper_response(&response, metrics);
        }
    };

    let params = route_match.params.clone();

    // PERF: Only parse query string when present
    let query_string = uri.query().unwrap_or("");
    let query: HashMap<String, String> = if query_string.is_empty() {
        HashMap::new()
    } else {
        query_string
            .split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                match (parts.next(), parts.next()) {
                    (Some(key), Some(value)) => {
                        let value_with_spaces = value.replace('+', " ");
                        Some((
                            urlencoding::decode(key).unwrap_or_default().to_string(),
                            urlencoding::decode(&value_with_spaces)
                                .unwrap_or_default()
                                .to_string(),
                        ))
                    }
                    (Some(key), None) => Some((
                        urlencoding::decode(key).unwrap_or_default().to_string(),
                        String::new(),
                    )),
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

    // PERF: Only collect body for methods that carry payloads
    let body_bytes: Vec<u8> = match method_str {
        "GET" | "HEAD" | "OPTIONS" | "DELETE" => {
            // Fast path: drop body without draining - hyper handles cleanup
            drop(req);
            Vec::new()
        }
        _ => match req.collect().await {
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
                metrics.inc_errors();
                Vec::new()
            }
        },
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
                return build_hyper_response(&response, metrics);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_hyper_response(&response, metrics);
            }
        }
    }

    // PERF: Skip async middleware if none registered
    if !middleware.is_async_empty() {
        match middleware.execute_before_async(&mut request).await {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return build_hyper_response(&response, metrics);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_hyper_response(&response, metrics);
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
                    return build_hyper_response(&response, metrics);
                }
                Err(e) => {
                    metrics.inc_errors();
                    let response = Response::error(e.status, &e.message);
                    return build_hyper_response(&response, metrics);
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
                return build_hyper_response(&response, metrics);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_hyper_response(&response, metrics);
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
    let handler_id = route_match.handler_id;
    let result = handlers
        .invoke_async(handler_id, request, dependency_container.clone())
        .await;

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

    // Restore request for after-middleware (the original was moved into the handler)
    let request = after_request.unwrap_or_default();

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
            Response::error(500, &err)
        }
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

    build_hyper_response(&response, metrics)
}

/// Build a Hyper response from our Response type.
/// PERF: Avoid unnecessary copies - use Bytes::copy_from_slice directly.
#[inline]
fn build_hyper_response(
    response: &Response,
    metrics: &Arc<ServerMetrics>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = HyperResponse::builder().status(status);

    for (key, value) in &response.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    // PERF: Create Bytes directly from slice - avoids intermediate Vec allocation
    let body_slice = response.body_bytes();
    metrics.add_bytes_sent(body_slice.len() as u64);
    let body = Full::new(Bytes::copy_from_slice(body_slice));

    Ok(builder.body(body).unwrap_or_else(|_| {
        HyperResponse::new(Full::new(Bytes::from_static(b"Internal Server Error")))
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
