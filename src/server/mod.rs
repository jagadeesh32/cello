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
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::handler::HandlerRegistry;
use crate::middleware::{MiddlewareAction, MiddlewareChain};
use crate::request::Request;
use crate::response::Response;
use crate::router::{RouteMatch, Router};
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
    /// Request latency histogram (simplified)
    latencies: Arc<RwLock<Vec<Duration>>>,
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
            latencies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Increment request count.
    pub fn inc_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment connection count.
    pub fn inc_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement connection count.
    pub fn dec_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Add bytes received.
    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Add bytes sent.
    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Increment error count.
    pub fn inc_errors(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record request latency.
    pub fn record_latency(&self, latency: Duration) {
        let mut latencies = self.latencies.write();
        latencies.push(latency);
        // Keep only last 1000 latencies
        if latencies.len() > 1000 {
            latencies.remove(0);
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
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_initiated.load(Ordering::SeqCst)
    }

    /// Increment active request count.
    pub fn request_started(&self) {
        self.active_requests.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement active request count.
    pub fn request_finished(&self) {
        self.active_requests.fetch_sub(1, Ordering::SeqCst);
    }

    /// Get active request count.
    pub fn active_requests(&self) -> u64 {
        self.active_requests.load(Ordering::SeqCst)
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
    prometheus: Arc<parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>>,
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
        prometheus: Arc<parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>>,
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
                pyo3::exceptions::PyValueError::new_err(format!("Invalid address: {}", e))
            })?;

        let listener = TcpListener::bind(addr).await.map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to bind: {}", e))
        })?;

        println!("Cello server running at http://{}", addr);
        println!("   Middleware: {} registered", self.middleware.len());
        println!("   Max connections: {}", self.config.max_connections);
        println!("   Press CTRL+C to stop the server");

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

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("\nShutting down gracefully...");
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
                                eprintln!("Connection limit reached, rejecting connection from {}", peer_addr);
                                continue;
                            }

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
                                let service = service_fn(move |req| {
                                    let router = router.clone();
                                    let handlers = handlers.clone();
                                    let middleware = middleware.clone();
                                    let metrics = metrics_for_service.clone();
                                    let shutdown = shutdown.clone();
                                    let dependency_container = dependency_container.clone();
                                    let guards = guards.clone();
                                    let prometheus = prometheus.clone();

                                    async move {
                                        shutdown.request_started();
                                        let start = Instant::now();

                                        let result = handle_request(
                                            req,
                                            router,
                                            handlers,
                                            middleware,
                                            metrics.clone(),
                                            dependency_container,
                                            guards,
                                            prometheus,
                                        )
                                        .await;

                                        metrics.record_latency(start.elapsed());
                                        shutdown.request_finished();

                                        result
                                    }
                                });

                                if let Err(err) = http1::Builder::new()
                                    .serve_connection(io, service)
                                    .await
                                {
                                    // Only log if not a normal connection close
                                    if !err.is_incomplete_message() {
                                        eprintln!("Connection error: {:?}", err);
                                    }
                                }

                                metrics_for_cleanup.dec_connections();
                            });
                        }
                        Err(e) => {
                            eprintln!("Accept error: {}", e);
                        }
                    }
                }
            }
        }

        // Wait for active requests to complete
        println!("Draining {} active requests...", shutdown.active_requests());
        shutdown.drain().await;
        println!("Server stopped");

        Ok(())
    }
}

async fn handle_request(
    req: HyperRequest<Incoming>,
    router: Arc<Router>,
    handlers: Arc<HandlerRegistry>,
    middleware: Arc<MiddlewareChain>,
    metrics: Arc<ServerMetrics>,
    dependency_container: Arc<crate::dependency::DependencyContainer>,
    guards: Arc<crate::middleware::guards::GuardsMiddleware>,
    prometheus: Arc<parking_lot::RwLock<Option<crate::middleware::prometheus::PrometheusMiddleware>>>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    metrics.inc_requests();

    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query_string = req.uri().query().unwrap_or("");

    // Parse query parameters
    let query: HashMap<String, String> = query_string
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
        .collect();

    // Extract headers
    let headers: HashMap<String, String> = req
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    // Read body
    let body_bytes = match req.collect().await {
        Ok(collected) => {
            let bytes = collected.to_bytes().to_vec();
            metrics.add_bytes_received(bytes.len() as u64);
            bytes
        }
        Err(_) => {
            metrics.inc_errors();
            Vec::new()
        }
    };

    // Match route
    let route_match = router.match_route(&method, &path);
    let params = match &route_match {
        Some(m) => m.params.clone(),
        None => HashMap::new(),
    };

    // Create request object
    let mut request =
        Request::from_http(method.clone(), path.clone(), params, query, headers, body_bytes);

    // Execute before middleware
    match middleware.execute_before(&mut request) {
        Ok(MiddlewareAction::Continue) => {}
        Ok(MiddlewareAction::Stop(response)) => {
            return build_hyper_response(&response, &metrics);
        }
        Err(e) => {
            metrics.inc_errors();
            let response = Response::error(e.status, &e.message);
            return build_hyper_response(&response, &metrics);
        }
    }

    // Execute Prometheus before middleware
    if let Some(ref p) = *prometheus.read() {
        use crate::middleware::{Middleware, MiddlewareAction};
        match p.before(&mut request) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return build_hyper_response(&response, &metrics);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_hyper_response(&response, &metrics);
            }
        }
    }

    // Execute Guards
    {
        use crate::middleware::{Middleware, MiddlewareAction};
        match Middleware::before(&*guards, &mut request) {
            Ok(MiddlewareAction::Continue) => {}
            Ok(MiddlewareAction::Stop(response)) => {
                return build_hyper_response(&response, &metrics);
            }
            Err(e) => {
                metrics.inc_errors();
                let response = Response::error(e.status, &e.message);
                return build_hyper_response(&response, &metrics);
            }
        }
    }

    // Handle route
    let mut response = match route_match {
        Some(RouteMatch { handler_id, .. }) => {
            // Invoke handler
            let result = handlers.invoke_async(handler_id, request.clone(), dependency_container.clone()).await;

            match result {
                Ok(json_value) => Response::from_json_value(json_value, 200),
                Err(err) => {
                    metrics.inc_errors();
                    Response::error(500, &err)
                }
            }
        }
        None => {
            // 404 Not Found
            Response::not_found(&format!("Not Found: {} {}", method, path))
        }
    };

    // Execute after middleware
    match middleware.execute_after(&request, &mut response) {
        Ok(MiddlewareAction::Continue) => {}
        Ok(MiddlewareAction::Stop(new_response)) => {
            return build_hyper_response(&new_response, &metrics);
        }
        Err(e) => {
            metrics.inc_errors();
            let error_response = Response::error(e.status, &e.message);
            return build_hyper_response(&error_response, &metrics);
        }
    }

    // Execute Prometheus after middleware
    if let Some(ref p) = *prometheus.read() {
        use crate::middleware::Middleware;
        let _ = p.after(&request, &mut response);
    }

    build_hyper_response(&response, &metrics)
}

/// Build a Hyper response from our Response type.
fn build_hyper_response(
    response: &Response,
    metrics: &Arc<ServerMetrics>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    let status =
        StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = HyperResponse::builder().status(status);

    for (key, value) in &response.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    let body_bytes = response.body_bytes().to_vec();
    metrics.add_bytes_sent(body_bytes.len() as u64);

    let body = Full::new(Bytes::from(body_bytes));

    Ok(builder.body(body).unwrap())
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
