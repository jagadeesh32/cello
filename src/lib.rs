//! Cello - Ultra-fast Rust-powered Python web framework
//!
//! This module provides the core HTTP server and routing functionality
//! that powers the Cello Python framework.
//!
//! ## Features
//! - SIMD-accelerated JSON parsing
//! - Arena allocators for zero-copy operations
//! - Middleware system with hooks
//! - WebSocket and SSE support
//! - Blueprint-based routing
//! - Enterprise-grade features:
//!   - Request context & dependency injection
//!   - RFC 7807 error handling
//!   - Lifecycle hooks & events
//!   - Timeout & limits configuration
//!   - Advanced routing with constraints
//!   - Streaming responses
//!   - Cluster mode & protocol support

// Silence PyO3 macro warning from older version
#![allow(non_local_definitions)]

// Core modules
pub mod arena;
pub mod blueprint;
pub mod handler;
pub mod json;
pub mod multipart;
pub mod router;
pub mod sse;
pub mod websocket;

// Enterprise modules (available for direct use)
pub mod context;
pub mod dependency;
pub mod dto;
pub mod error;
pub mod lifecycle;
pub mod timeout;
pub mod routing;
pub mod middleware;
pub mod request;
pub mod response;
pub mod server;

// New v0.5.0 modules
pub mod openapi;
pub mod background;
pub mod template;

use pyo3::prelude::*;
use std::sync::Arc;

use blueprint::Blueprint;
use handler::HandlerRegistry;
use router::Router;
use server::Server;
use sse::{SseEvent, SseStream};
use websocket::{WebSocket, WebSocketMessage, WebSocketRegistry};

/// The main Cello application class exposed to Python.
///
/// This class manages routes, middleware, and starts the HTTP server.
#[pyclass]
pub struct Cello {
    router: Router,
    handlers: HandlerRegistry,
    middleware: middleware::MiddlewareChain,
    websocket_handlers: WebSocketRegistry,
    dependency_container: Arc<dependency::DependencyContainer>,
    guards: Arc<middleware::guards::GuardsMiddleware>,
    prometheus: Arc<parking_lot::RwLock<Option<middleware::prometheus::PrometheusMiddleware>>>,
}

#[pymethods]
impl Cello {
    /// Create a new Cello application instance.
    #[new]
    pub fn new() -> Self {
        Cello {
            router: Router::new(),
            handlers: HandlerRegistry::new(),
            middleware: middleware::MiddlewareChain::new(),
            websocket_handlers: WebSocketRegistry::new(),
            dependency_container: Arc::new(dependency::DependencyContainer::new()),
            guards: Arc::new(middleware::guards::GuardsMiddleware::new()),
            prometheus: Arc::new(parking_lot::RwLock::new(None)),
        }
    }

    /// Register a GET route.
    pub fn get(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("GET", path, handler)
    }

    /// Register a POST route.
    pub fn post(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("POST", path, handler)
    }

    /// Register a PUT route.
    pub fn put(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("PUT", path, handler)
    }

    /// Register a DELETE route.
    pub fn delete(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("DELETE", path, handler)
    }

    /// Register a PATCH route.
    pub fn patch(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("PATCH", path, handler)
    }

    /// Register an OPTIONS route.
    pub fn options(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("OPTIONS", path, handler)
    }

    /// Register a HEAD route.
    pub fn head(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("HEAD", path, handler)
    }

    /// Register a WebSocket route.
    pub fn websocket(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.websocket_handlers.register(path, handler);
        Ok(())
    }

    /// Register a blueprint.
    pub fn register_blueprint(&mut self, blueprint: &Blueprint) -> PyResult<()> {
        let routes = blueprint.get_all_routes();
        for (method, path, handler) in routes {
            self.add_route(&method, &path, handler)?;
        }
        Ok(())
    }

    /// Enable CORS middleware.
    #[pyo3(signature = (origins=None))]
    pub fn enable_cors(&mut self, origins: Option<Vec<String>>) {
        let cors = middleware::CorsMiddleware::new();
        if let Some(o) = origins {
            // TODO: Update CorsConfig when ready
            let _ = o;
        }
        self.middleware.add(cors);
    }

    /// Enable Prometheus metrics.
    #[pyo3(signature = (endpoint=None, namespace=None, subsystem=None))]
    pub fn enable_prometheus(&mut self, endpoint: Option<String>, namespace: Option<String>, subsystem: Option<String>) -> PyResult<()> {
        let mut config = middleware::prometheus::PrometheusConfig::default();
        if let Some(e) = endpoint { config.endpoint = e; }
        if let Some(n) = namespace { config.namespace = n; }
        if let Some(s) = subsystem { config.subsystem = s; }

        let mw = middleware::prometheus::PrometheusMiddleware::with_config(config)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        
        *self.prometheus.write() = Some(mw);
        Ok(())
    }

    pub fn add_guard(&mut self, guard: PyObject) -> PyResult<()> {
        let python_guard = middleware::guards::PythonGuard::new(guard);
        self.guards.add_guard(python_guard);
        Ok(())
    }

    /// Register a singleton dependency.
    pub fn register_singleton(&mut self, name: String, value: PyObject) {
        self.dependency_container.register_py_singleton(&name, value);
    }

    /// Enable logging middleware.
    pub fn enable_logging(&mut self) {
        self.middleware.add(middleware::LoggingMiddleware::new());
    }

    /// Enable compression middleware.
    #[pyo3(signature = (min_size=None))]
    pub fn enable_compression(&mut self, min_size: Option<usize>) {
        let mut compression = middleware::CompressionMiddleware::new();
        if let Some(size) = min_size {
            compression.min_size = size;
        }
        self.middleware.add(compression);
    }

    /// Start the HTTP server.
    #[pyo3(signature = (host=None, port=None, workers=None))]
    pub fn run(&self, py: Python<'_>, host: Option<&str>, port: Option<u16>, workers: Option<usize>) -> PyResult<()> {
        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(8000);

        println!("🐍 Cello v2 server starting at http://{}:{}", host, port);
        if let Some(w) = workers {
            println!("   Workers: {}", w);
        }

        // Release the GIL and run the server
        py.allow_threads(|| {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.enable_all();

            if let Some(w) = workers {
                builder.worker_threads(w);
            }

            let rt = builder.build()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            rt.block_on(async {
                let mut config = server::ServerConfig::new(host, port);
                config.workers = workers.unwrap_or(0);

                let server = Server::new(
                    config,
                    self.router.clone(),
                    self.handlers.clone(),
                    self.middleware.clone(),
                    self.websocket_handlers.clone(),
                    self.dependency_container.clone(),
                    self.guards.clone(),
                    self.prometheus.clone(),
                );
                server.run().await
            })
        })
    }

    /// Internal route registration.
    fn add_route(&mut self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let handler_id = self.handlers.register(handler);
        self.router
            .add_route(method, path, handler_id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }
}

impl Default for Cello {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Python Configuration Classes
// ============================================================================

/// Python-exposed timeout configuration.
#[pyclass(name = "TimeoutConfig")]
#[derive(Clone)]
pub struct PyTimeoutConfig {
    #[pyo3(get, set)]
    pub read_header_timeout: u64,
    #[pyo3(get, set)]
    pub read_body_timeout: u64,
    #[pyo3(get, set)]
    pub write_timeout: u64,
    #[pyo3(get, set)]
    pub idle_timeout: u64,
    #[pyo3(get, set)]
    pub handler_timeout: u64,
}

#[pymethods]
impl PyTimeoutConfig {
    #[new]
    #[pyo3(signature = (read_header=5, read_body=30, write=30, idle=60, handler=30))]
    pub fn new(read_header: u64, read_body: u64, write: u64, idle: u64, handler: u64) -> Self {
        Self {
            read_header_timeout: read_header,
            read_body_timeout: read_body,
            write_timeout: write,
            idle_timeout: idle,
            handler_timeout: handler,
        }
    }
}

/// Python-exposed limits configuration.
#[pyclass(name = "LimitsConfig")]
#[derive(Clone)]
pub struct PyLimitsConfig {
    #[pyo3(get, set)]
    pub max_header_size: usize,
    #[pyo3(get, set)]
    pub max_body_size: usize,
    #[pyo3(get, set)]
    pub max_connections: usize,
    #[pyo3(get, set)]
    pub max_requests_per_connection: usize,
}

#[pymethods]
impl PyLimitsConfig {
    #[new]
    #[pyo3(signature = (max_header_size=8192, max_body_size=10485760, max_connections=10000, max_requests_per_connection=1000))]
    pub fn new(
        max_header_size: usize,
        max_body_size: usize,
        max_connections: usize,
        max_requests_per_connection: usize,
    ) -> Self {
        Self {
            max_header_size,
            max_body_size,
            max_connections,
            max_requests_per_connection,
        }
    }
}

/// Python-exposed cluster configuration.
#[pyclass(name = "ClusterConfig")]
#[derive(Clone)]
pub struct PyClusterConfig {
    #[pyo3(get, set)]
    pub workers: usize,
    #[pyo3(get, set)]
    pub cpu_affinity: bool,
    #[pyo3(get, set)]
    pub max_restarts: u32,
    #[pyo3(get, set)]
    pub graceful_shutdown: bool,
    #[pyo3(get, set)]
    pub shutdown_timeout: u64,
}

#[pymethods]
impl PyClusterConfig {
    #[new]
    #[pyo3(signature = (workers=None, cpu_affinity=false, max_restarts=5, graceful_shutdown=true, shutdown_timeout=30))]
    pub fn new(
        workers: Option<usize>,
        cpu_affinity: bool,
        max_restarts: u32,
        graceful_shutdown: bool,
        shutdown_timeout: u64,
    ) -> Self {
        Self {
            workers: workers.unwrap_or_else(num_cpus::get),
            cpu_affinity,
            max_restarts,
            graceful_shutdown,
            shutdown_timeout,
        }
    }

    /// Create with auto-detected worker count.
    #[staticmethod]
    pub fn auto() -> Self {
        Self::new(None, false, 5, true, 30)
    }
}

/// Python-exposed TLS configuration.
#[pyclass(name = "TlsConfig")]
#[derive(Clone)]
pub struct PyTlsConfig {
    #[pyo3(get, set)]
    pub cert_path: String,
    #[pyo3(get, set)]
    pub key_path: String,
    #[pyo3(get, set)]
    pub ca_path: Option<String>,
    #[pyo3(get, set)]
    pub min_version: String,
    #[pyo3(get, set)]
    pub max_version: String,
    #[pyo3(get, set)]
    pub require_client_cert: bool,
}

#[pymethods]
impl PyTlsConfig {
    #[new]
    #[pyo3(signature = (cert_path, key_path, ca_path=None, min_version="1.2", max_version="1.3", require_client_cert=false))]
    pub fn new(
        cert_path: String,
        key_path: String,
        ca_path: Option<String>,
        min_version: &str,
        max_version: &str,
        require_client_cert: bool,
    ) -> Self {
        Self {
            cert_path,
            key_path,
            ca_path,
            min_version: min_version.to_string(),
            max_version: max_version.to_string(),
            require_client_cert,
        }
    }
}

/// Python-exposed HTTP/2 configuration.
#[pyclass(name = "Http2Config")]
#[derive(Clone)]
pub struct PyHttp2Config {
    #[pyo3(get, set)]
    pub max_concurrent_streams: u32,
    #[pyo3(get, set)]
    pub initial_window_size: u32,
    #[pyo3(get, set)]
    pub max_frame_size: u32,
    #[pyo3(get, set)]
    pub enable_push: bool,
}

#[pymethods]
impl PyHttp2Config {
    #[new]
    #[pyo3(signature = (max_concurrent_streams=100, initial_window_size=1048576, max_frame_size=16384, enable_push=false))]
    pub fn new(
        max_concurrent_streams: u32,
        initial_window_size: u32,
        max_frame_size: u32,
        enable_push: bool,
    ) -> Self {
        Self {
            max_concurrent_streams,
            initial_window_size,
            max_frame_size,
            enable_push,
        }
    }
}

/// Python-exposed HTTP/3 configuration.
#[pyclass(name = "Http3Config")]
#[derive(Clone)]
pub struct PyHttp3Config {
    #[pyo3(get, set)]
    pub max_idle_timeout: u64,
    #[pyo3(get, set)]
    pub max_udp_payload_size: u16,
    #[pyo3(get, set)]
    pub initial_max_streams_bidi: u64,
    #[pyo3(get, set)]
    pub enable_0rtt: bool,
}

#[pymethods]
impl PyHttp3Config {
    #[new]
    #[pyo3(signature = (max_idle_timeout=30, max_udp_payload_size=1350, initial_max_streams_bidi=100, enable_0rtt=false))]
    pub fn new(
        max_idle_timeout: u64,
        max_udp_payload_size: u16,
        initial_max_streams_bidi: u64,
        enable_0rtt: bool,
    ) -> Self {
        Self {
            max_idle_timeout,
            max_udp_payload_size,
            initial_max_streams_bidi,
            enable_0rtt,
        }
    }
}

/// Python-exposed JWT configuration.
#[pyclass(name = "JwtConfig")]
#[derive(Clone)]
pub struct PyJwtConfig {
    #[pyo3(get, set)]
    pub secret: String,
    #[pyo3(get, set)]
    pub algorithm: String,
    #[pyo3(get, set)]
    pub header_name: String,
    #[pyo3(get, set)]
    pub cookie_name: Option<String>,
    #[pyo3(get, set)]
    pub leeway: u64,
}

#[pymethods]
impl PyJwtConfig {
    #[new]
    #[pyo3(signature = (secret, algorithm="HS256", header_name="Authorization", cookie_name=None, leeway=0))]
    pub fn new(
        secret: String,
        algorithm: &str,
        header_name: &str,
        cookie_name: Option<String>,
        leeway: u64,
    ) -> Self {
        Self {
            secret,
            algorithm: algorithm.to_string(),
            header_name: header_name.to_string(),
            cookie_name,
            leeway,
        }
    }
}

/// Python-exposed rate limit configuration.
#[pyclass(name = "RateLimitConfig")]
#[derive(Clone)]
pub struct PyRateLimitConfig {
    #[pyo3(get, set)]
    pub algorithm: String,
    #[pyo3(get, set)]
    pub capacity: u64,
    #[pyo3(get, set)]
    pub refill_rate: u64,
    #[pyo3(get, set)]
    pub window_secs: u64,
    #[pyo3(get, set)]
    pub key_by: String,
}

#[pymethods]
impl PyRateLimitConfig {
    #[new]
    #[pyo3(signature = (algorithm="token_bucket", capacity=100, refill_rate=10, window_secs=60, key_by="ip"))]
    pub fn new(
        algorithm: &str,
        capacity: u64,
        refill_rate: u64,
        window_secs: u64,
        key_by: &str,
    ) -> Self {
        Self {
            algorithm: algorithm.to_string(),
            capacity,
            refill_rate,
            window_secs,
            key_by: key_by.to_string(),
        }
    }

    /// Create token bucket config.
    #[staticmethod]
    pub fn token_bucket(capacity: u64, refill_rate: u64) -> Self {
        Self::new("token_bucket", capacity, refill_rate, 60, "ip")
    }

    /// Create sliding window config.
    #[staticmethod]
    pub fn sliding_window(max_requests: u64, window_secs: u64) -> Self {
        Self::new("sliding_window", max_requests, 0, window_secs, "ip")
    }
}

/// Python-exposed session configuration.
#[pyclass(name = "SessionConfig")]
#[derive(Clone)]
pub struct PySessionConfig {
    #[pyo3(get, set)]
    pub cookie_name: String,
    #[pyo3(get, set)]
    pub cookie_path: String,
    #[pyo3(get, set)]
    pub cookie_domain: Option<String>,
    #[pyo3(get, set)]
    pub cookie_secure: bool,
    #[pyo3(get, set)]
    pub cookie_http_only: bool,
    #[pyo3(get, set)]
    pub cookie_same_site: String,
    #[pyo3(get, set)]
    pub max_age: u64,
}

#[pymethods]
impl PySessionConfig {
    #[new]
    #[pyo3(signature = (cookie_name="session_id", cookie_path="/", cookie_domain=None, cookie_secure=true, cookie_http_only=true, cookie_same_site="Lax", max_age=86400))]
    pub fn new(
        cookie_name: &str,
        cookie_path: &str,
        cookie_domain: Option<String>,
        cookie_secure: bool,
        cookie_http_only: bool,
        cookie_same_site: &str,
        max_age: u64,
    ) -> Self {
        Self {
            cookie_name: cookie_name.to_string(),
            cookie_path: cookie_path.to_string(),
            cookie_domain,
            cookie_secure,
            cookie_http_only,
            cookie_same_site: cookie_same_site.to_string(),
            max_age,
        }
    }
}

/// Python-exposed security headers configuration.
#[pyclass(name = "SecurityHeadersConfig")]
#[derive(Clone)]
pub struct PySecurityHeadersConfig {
    #[pyo3(get, set)]
    pub x_frame_options: Option<String>,
    #[pyo3(get, set)]
    pub x_content_type_options: bool,
    #[pyo3(get, set)]
    pub x_xss_protection: Option<String>,
    #[pyo3(get, set)]
    pub referrer_policy: Option<String>,
    #[pyo3(get, set)]
    pub hsts_max_age: Option<u64>,
    #[pyo3(get, set)]
    pub hsts_include_subdomains: bool,
    #[pyo3(get, set)]
    pub hsts_preload: bool,
}

#[pymethods]
impl PySecurityHeadersConfig {
    #[new]
    #[pyo3(signature = (x_frame_options="DENY", x_content_type_options=true, x_xss_protection="1; mode=block", referrer_policy="strict-origin-when-cross-origin", hsts_max_age=None, hsts_include_subdomains=false, hsts_preload=false))]
    pub fn new(
        x_frame_options: &str,
        x_content_type_options: bool,
        x_xss_protection: &str,
        referrer_policy: &str,
        hsts_max_age: Option<u64>,
        hsts_include_subdomains: bool,
        hsts_preload: bool,
    ) -> Self {
        Self {
            x_frame_options: Some(x_frame_options.to_string()),
            x_content_type_options,
            x_xss_protection: Some(x_xss_protection.to_string()),
            referrer_policy: Some(referrer_policy.to_string()),
            hsts_max_age,
            hsts_include_subdomains,
            hsts_preload,
        }
    }

    /// Create default secure headers.
    #[staticmethod]
    pub fn secure() -> Self {
        Self::new("DENY", true, "1; mode=block", "strict-origin-when-cross-origin", Some(31536000), true, false)
    }
}

/// Python-exposed CSP builder.
#[pyclass(name = "CSP")]
#[derive(Clone, Default)]
pub struct PyCsp {
    directives: std::collections::HashMap<String, Vec<String>>,
}

#[pymethods]
impl PyCsp {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set default-src directive.
    pub fn default_src(&mut self, sources: Vec<String>) -> Self {
        self.directives.insert("default-src".to_string(), sources);
        self.clone()
    }

    /// Set script-src directive.
    pub fn script_src(&mut self, sources: Vec<String>) -> Self {
        self.directives.insert("script-src".to_string(), sources);
        self.clone()
    }

    /// Set style-src directive.
    pub fn style_src(&mut self, sources: Vec<String>) -> Self {
        self.directives.insert("style-src".to_string(), sources);
        self.clone()
    }

    /// Set img-src directive.
    pub fn img_src(&mut self, sources: Vec<String>) -> Self {
        self.directives.insert("img-src".to_string(), sources);
        self.clone()
    }

    /// Build CSP header value.
    pub fn build(&self) -> String {
        self.directives
            .iter()
            .map(|(k, v)| format!("{} {}", k, v.join(" ")))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Python-exposed static files configuration.
#[pyclass(name = "StaticFilesConfig")]
#[derive(Clone)]
pub struct PyStaticFilesConfig {
    #[pyo3(get, set)]
    pub root: String,
    #[pyo3(get, set)]
    pub prefix: String,
    #[pyo3(get, set)]
    pub index_file: Option<String>,
    #[pyo3(get, set)]
    pub enable_etag: bool,
    #[pyo3(get, set)]
    pub enable_last_modified: bool,
    #[pyo3(get, set)]
    pub cache_control: Option<String>,
    #[pyo3(get, set)]
    pub directory_listing: bool,
}

#[pymethods]
impl PyStaticFilesConfig {
    #[new]
    #[pyo3(signature = (root, prefix="/static", index_file="index.html", enable_etag=true, enable_last_modified=true, cache_control=None, directory_listing=false))]
    pub fn new(
        root: String,
        prefix: &str,
        index_file: &str,
        enable_etag: bool,
        enable_last_modified: bool,
        cache_control: Option<String>,
        directory_listing: bool,
    ) -> Self {
        Self {
            root,
            prefix: prefix.to_string(),
            index_file: Some(index_file.to_string()),
            enable_etag,
            enable_last_modified,
            cache_control,
            directory_listing,
        }
    }
}

/// Python module definition.
#[pymodule]
fn _cello(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    // Core classes
    m.add_class::<Cello>()?;
    m.add_class::<request::Request>()?;
    m.add_class::<response::Response>()?;

    // Blueprint
    m.add_class::<Blueprint>()?;

    // WebSocket
    m.add_class::<WebSocket>()?;
    m.add_class::<WebSocketMessage>()?;

    // SSE
    m.add_class::<SseEvent>()?;
    m.add_class::<SseStream>()?;

    // Multipart
    m.add_class::<multipart::FormData>()?;
    m.add_class::<multipart::UploadedFile>()?;

    // Configuration classes
    m.add_class::<PyTimeoutConfig>()?;
    m.add_class::<PyLimitsConfig>()?;
    m.add_class::<PyClusterConfig>()?;
    m.add_class::<PyTlsConfig>()?;
    m.add_class::<PyHttp2Config>()?;
    m.add_class::<PyHttp3Config>()?;
    m.add_class::<PyJwtConfig>()?;
    m.add_class::<PyRateLimitConfig>()?;
    m.add_class::<PySessionConfig>()?;
    m.add_class::<PySecurityHeadersConfig>()?;
    m.add_class::<PyCsp>()?;
    m.add_class::<PyStaticFilesConfig>()?;

    // v0.5.0 - Background Tasks
    m.add_class::<background::PyBackgroundTasks>()?;

    // v0.5.0 - Template Engine
    m.add_class::<template::PyTemplateEngine>()?;

    Ok(())
}
