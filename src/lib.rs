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
    cache_store: Arc<parking_lot::RwLock<Option<Arc<dyn middleware::cache::CacheStore>>>>,
    startup_handlers: Vec<PyObject>,
    shutdown_handlers: Vec<PyObject>,
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
            cache_store: Arc::new(parking_lot::RwLock::new(None)),
            startup_handlers: Vec::new(),
            shutdown_handlers: Vec::new(),
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

    /// Enable rate limiting.
    #[pyo3(signature = (config))]
    pub fn enable_rate_limit(&mut self, config: PyRateLimitConfig) -> PyResult<()> {
        let mw = match config.algorithm.as_str() {
            "token_bucket" => {
                let bucket = middleware::rate_limit::TokenBucketConfig::new(
                    config.capacity,
                    config.refill_rate as f64,
                );
                middleware::rate_limit::RateLimitMiddleware::token_bucket(bucket)
            }
            "sliding_window" => {
                let window = middleware::rate_limit::SlidingWindowConfig::new(
                    config.capacity,
                    std::time::Duration::from_secs(config.window_secs),
                );
                middleware::rate_limit::RateLimitMiddleware::sliding_window(window)
            }
            "adaptive" => {
                let base = middleware::rate_limit::TokenBucketConfig::new(
                    config.capacity,
                    config.refill_rate as f64,
                );
                let adaptive_config = middleware::rate_limit::AdaptiveConfig::new(
                    base,
                    config.min_capacity.unwrap_or(config.capacity / 2),
                    config.error_threshold.unwrap_or(0.10),
                );
                middleware::rate_limit::RateLimitMiddleware::adaptive(adaptive_config)
            }
            _ => return Err(pyo3::exceptions::PyValueError::new_err("Unknown rate limit algorithm")),
        };
        
        self.middleware.add(mw);
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

    /// Enable caching middleware.
    #[pyo3(signature = (ttl=300, methods=None, exclude_paths=None))]
    pub fn enable_caching(&mut self, ttl: u64, methods: Option<Vec<String>>, exclude_paths: Option<Vec<String>>) {
         let mut config = middleware::cache::CacheConfig::default();
         config.default_ttl = ttl;
         if let Some(m) = methods {
             config.methods = m;
         }
         if let Some(e) = exclude_paths {
             config.exclude_paths = e;
         }
         

         
         let mw = middleware::cache::CacheMiddleware::with_config(config.clone());
         
         // Store reference for invalidation
         *self.cache_store.write() = Some(config.store);
         
         self.middleware.add_async(mw);
    }

    /// Enable circuit breaker middleware.
    #[pyo3(signature = (failure_threshold=5, reset_timeout=30, half_open_target=3, failure_codes=None))]
    pub fn enable_circuit_breaker(
        &mut self,
        failure_threshold: u32,
        reset_timeout: u64,
        half_open_target: u32,
        failure_codes: Option<Vec<u16>>
    ) {
         let mut config = middleware::circuit_breaker::CircuitBreakerConfig::default();
         config.failure_threshold = failure_threshold;
         config.reset_timeout = std::time::Duration::from_secs(reset_timeout);
         config.half_open_target = half_open_target;
         if let Some(codes) = failure_codes {
             config.failure_codes = codes;
         }
         
         let mw = middleware::circuit_breaker::CircuitBreakerMiddleware::new(config);
         self.middleware.add(mw);
    }

    /// Register a startup handler.
    pub fn on_startup(&mut self, handler: PyObject) {
        self.startup_handlers.push(handler);
    }

    /// Register a shutdown handler.
    pub fn on_shutdown(&mut self, handler: PyObject) {
        self.shutdown_handlers.push(handler);
    }

    /// Invalidate cache tags.
    #[pyo3(signature = (tags))]
    pub fn invalidate_cache(&self, tags: Vec<String>) -> PyResult<()> {
        if let Some(store) = self.cache_store.read().as_ref() {
            let store = store.clone();
            // Use std::thread to spawn if runtime not available or just spawn on default
            // Since this runs in Python thread, we might not be in tokio context.
            // But Cello starts a runtime?
            // Safer to use block_in_place or just spawn if we knew we are in runtime.
            // For now, let's assume we can just ignore errors or use simple blocking if the store is InMemory.
            // But store is async.
            // Let's spawn a thread that creates a runtime? No too heavy.
            // let's try to get handle.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                 handle.spawn(async move {
                     let _ = store.invalidate_tags(&tags).await;
                 });
            } else {
                 // Fallback: This might happen if called before app.run() or from outside.
                 // We can start a temp runtime or just print warning.
                 eprintln!("Warning: Cache invalidation failed - no async runtime");
            }
        }
        Ok(())
    }

    // ========================================================================
    // Enterprise Features (v0.7.0+)
    // ========================================================================

    /// Enable OpenTelemetry distributed tracing and metrics.
    #[pyo3(signature = (config))]
    pub fn enable_telemetry(&mut self, config: PyOpenTelemetryConfig) {
        let service_name = config.service_name.clone();
        let otel_config = middleware::telemetry::OpenTelemetryConfig {
            service_name: config.service_name,
            service_version: config.service_version,
            otlp_endpoint: config.otlp_endpoint,
            sampling_rate: config.sampling_rate,
            export_traces: config.export_traces,
            export_metrics: config.export_metrics,
            propagate_context: true,
            excluded_paths: config.excluded_paths,
            resource_attributes: std::collections::HashMap::new(),
        };

        let mw = middleware::telemetry::OpenTelemetryMiddleware::new(otel_config);
        self.middleware.add_async(mw);

        println!("📊 OpenTelemetry enabled for service: {}", service_name);
    }

    /// Enable health check endpoints.
    #[pyo3(signature = (config=None))]
    pub fn enable_health_checks(&mut self, config: Option<PyHealthCheckConfig>) {
        let config = config.unwrap_or_else(|| PyHealthCheckConfig::new("/health", true, false, None, 5, Some(5)));

        let health_config = middleware::health::HealthCheckConfig {
            base_path: config.base_path.clone(),
            include_details: config.include_details,
            include_system_info: config.include_system_info,
            version: config.version,
            timeout: std::time::Duration::from_secs(config.timeout_secs),
            cache_duration: config.cache_secs.map(std::time::Duration::from_secs),
        };

        let mw = middleware::health::HealthCheckMiddleware::new(health_config);
        self.middleware.add(mw);

        println!("🏥 Health checks enabled:");
        println!("   Liveness:  {}/live", config.base_path);
        println!("   Readiness: {}/ready", config.base_path);
        println!("   Full:      {}", config.base_path);
    }

    /// Enable GraphQL endpoint.
    #[pyo3(signature = (config=None))]
    pub fn enable_graphql(&mut self, config: Option<PyGraphQLConfig>) {
        let config = config.unwrap_or_else(|| PyGraphQLConfig::new("/graphql", true, true, Some(10), Some(1000), false, false));

        let gql_config = middleware::graphql::GraphQLConfig {
            path: config.path.clone(),
            playground: config.playground,
            playground_path: None,
            introspection: config.introspection,
            max_depth: config.max_depth,
            max_complexity: config.max_complexity,
            batching: config.batching,
            tracing: config.tracing,
        };

        let mw = middleware::graphql::GraphQLMiddleware::new(gql_config);
        self.middleware.add_async(mw);

        println!("🔷 GraphQL enabled:");
        println!("   Endpoint:   {}", config.path);
        if config.playground {
            println!("   Playground: {} (GET)", config.path);
        }
    }

    // ========================================================================
    // End Enterprise Features
    // ========================================================================

    /// Enable OpenAPI documentation endpoints.
    /// This adds:
    /// - GET /docs - Swagger UI
    /// - GET /redoc - ReDoc documentation
    /// - GET /openapi.json - OpenAPI JSON schema
    #[pyo3(signature = (title=None, version=None))]
    pub fn enable_openapi(&mut self, py: Python<'_>, title: Option<String>, version: Option<String>) -> PyResult<()> {
        let title = title.unwrap_or_else(|| "Cello API".to_string());
        let version = version.unwrap_or_else(|| "0.7.0".to_string());

        // Store title and version for later use
        let title_clone = title.clone();
        let version_clone = version.clone();

        // Create a Python handler for /docs (Swagger UI)
        let docs_code = format!(r#"
def docs_handler(request):
    from cello import Response
    html = '''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
    <style>
        body {{ margin: 0; padding: 0; }}
        .swagger-ui .topbar {{ display: none; }}
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
    <script>
        window.onload = () => {{
            window.ui = SwaggerUIBundle({{
                url: "/openapi.json",
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
                layout: "StandaloneLayout"
            }});
        }};
    </script>
</body>
</html>'''
    return Response.html(html)
"#, title = title_clone);

        // Create /redoc handler
        let redoc_code = format!(r#"
def redoc_handler(request):
    from cello import Response
    html = '''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - ReDoc</title>
    <link href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700" rel="stylesheet">
    <style>body {{ margin: 0; padding: 0; }}</style>
</head>
<body>
    <redoc spec-url="/openapi.json"></redoc>
    <script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
</body>
</html>'''
    return Response.html(html)
"#, title = title_clone);

        // Create /openapi.json handler
        let openapi_code = format!(r#"
def openapi_handler(request):
    return {{
        "openapi": "3.0.3",
        "info": {{
            "title": "{title}",
            "version": "{version}",
            "description": "{title} - Powered by Cello Framework"
        }},
        "paths": {{}}
    }}
"#, title = title_clone, version = version_clone);

        // Execute Python code and register handlers
        let docs_handler = py.eval(&format!("{}\ndocs_handler", docs_code), None, None)?;
        let redoc_handler = py.eval(&format!("{}\nredoc_handler", redoc_code), None, None)?;
        let openapi_handler = py.eval(&format!("{}\nopenapi_handler", openapi_code), None, None)?;

        self.add_route("GET", "/docs", docs_handler.into())?;
        self.add_route("GET", "/redoc", redoc_handler.into())?;
        self.add_route("GET", "/openapi.json", openapi_handler.into())?;

        println!("📚 OpenAPI docs enabled:");
        println!("   Swagger UI: /docs");
        println!("   ReDoc:      /redoc");
        println!("   OpenAPI:    /openapi.json");

        Ok(())
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
                
                // Execute startup handlers
                Python::with_gil(|py| {
                    for handler in &self.startup_handlers {
                        if let Err(e) = call_lifecycle_handler(py, handler) {
                            eprintln!("Error in startup handler: {}", e);
                        }
                    }
                });

                let result = server.run().await;
                
                // Execute shutdown handlers
                Python::with_gil(|py| {
                    for handler in &self.shutdown_handlers {
                        if let Err(e) = call_lifecycle_handler(py, handler) {
                            eprintln!("Error in shutdown handler: {}", e);
                        }
                    }
                });
                
                result
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
    #[pyo3(get, set)]
    pub min_capacity: Option<u64>,
    #[pyo3(get, set)]
    pub error_threshold: Option<f64>,
}

#[pymethods]
impl PyRateLimitConfig {
    #[new]
    #[pyo3(signature = (algorithm="token_bucket", capacity=100, refill_rate=10, window_secs=60, key_by="ip", min_capacity=None, error_threshold=None))]
    pub fn new(
        algorithm: &str,
        capacity: u64,
        refill_rate: u64,
        window_secs: u64,
        key_by: &str,
        min_capacity: Option<u64>,
        error_threshold: Option<f64>,
    ) -> Self {
        Self {
            algorithm: algorithm.to_string(),
            capacity,
            refill_rate,
            window_secs,
            key_by: key_by.to_string(),
            min_capacity,
            error_threshold,
        }
    }

    /// Create token bucket config.
    #[staticmethod]
    pub fn token_bucket(capacity: u64, refill_rate: u64) -> Self {
        Self::new("token_bucket", capacity, refill_rate, 60, "ip", None, None)
    }

    /// Create adaptive config.
    #[staticmethod]
    pub fn adaptive(capacity: u64, refill_rate: u64, min_capacity: u64, error_threshold: f64) -> Self {
        Self::new("adaptive", capacity, refill_rate, 60, "ip", Some(min_capacity), Some(error_threshold))
    }

    /// Create sliding window config.
    #[staticmethod]
    pub fn sliding_window(max_requests: u64, window_secs: u64) -> Self {
        Self::new("sliding_window", max_requests, 0, window_secs, "ip", None, None)
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

// ============================================================================
// Enterprise Configuration Classes (v0.7.0+)
// ============================================================================

/// Python-exposed OpenTelemetry configuration.
#[pyclass(name = "OpenTelemetryConfig")]
#[derive(Clone)]
pub struct PyOpenTelemetryConfig {
    #[pyo3(get, set)]
    pub service_name: String,
    #[pyo3(get, set)]
    pub service_version: String,
    #[pyo3(get, set)]
    pub otlp_endpoint: Option<String>,
    #[pyo3(get, set)]
    pub sampling_rate: f64,
    #[pyo3(get, set)]
    pub export_traces: bool,
    #[pyo3(get, set)]
    pub export_metrics: bool,
    #[pyo3(get, set)]
    pub excluded_paths: Vec<String>,
}

#[pymethods]
impl PyOpenTelemetryConfig {
    #[new]
    #[pyo3(signature = (service_name, service_version="0.1.0", otlp_endpoint=None, sampling_rate=1.0, export_traces=true, export_metrics=true, excluded_paths=None))]
    pub fn new(
        service_name: &str,
        service_version: &str,
        otlp_endpoint: Option<String>,
        sampling_rate: f64,
        export_traces: bool,
        export_metrics: bool,
        excluded_paths: Option<Vec<String>>,
    ) -> Self {
        Self {
            service_name: service_name.to_string(),
            service_version: service_version.to_string(),
            otlp_endpoint,
            sampling_rate: sampling_rate.clamp(0.0, 1.0),
            export_traces,
            export_metrics,
            excluded_paths: excluded_paths.unwrap_or_else(|| vec!["/health".to_string(), "/metrics".to_string()]),
        }
    }
}

/// Python-exposed Health Check configuration.
#[pyclass(name = "HealthCheckConfig")]
#[derive(Clone)]
pub struct PyHealthCheckConfig {
    #[pyo3(get, set)]
    pub base_path: String,
    #[pyo3(get, set)]
    pub include_details: bool,
    #[pyo3(get, set)]
    pub include_system_info: bool,
    #[pyo3(get, set)]
    pub version: Option<String>,
    #[pyo3(get, set)]
    pub timeout_secs: u64,
    #[pyo3(get, set)]
    pub cache_secs: Option<u64>,
}

#[pymethods]
impl PyHealthCheckConfig {
    #[new]
    #[pyo3(signature = (base_path="/health", include_details=true, include_system_info=false, version=None, timeout_secs=5, cache_secs=None))]
    pub fn new(
        base_path: &str,
        include_details: bool,
        include_system_info: bool,
        version: Option<String>,
        timeout_secs: u64,
        cache_secs: Option<u64>,
    ) -> Self {
        Self {
            base_path: base_path.to_string(),
            include_details,
            include_system_info,
            version,
            timeout_secs,
            cache_secs,
        }
    }

    /// Create Kubernetes-compatible health check config.
    #[staticmethod]
    pub fn kubernetes() -> Self {
        Self::new("/health", false, false, None, 5, Some(5))
    }

    /// Create detailed health check config.
    #[staticmethod]
    pub fn detailed() -> Self {
        Self::new("/health", true, true, None, 10, None)
    }
}

/// Python-exposed Database configuration.
#[pyclass(name = "DatabaseConfig")]
#[derive(Clone)]
pub struct PyDatabaseConfig {
    #[pyo3(get, set)]
    pub url: String,
    #[pyo3(get, set)]
    pub pool_size: usize,
    #[pyo3(get, set)]
    pub min_idle: usize,
    #[pyo3(get, set)]
    pub max_lifetime_secs: u64,
    #[pyo3(get, set)]
    pub connection_timeout_secs: u64,
    #[pyo3(get, set)]
    pub idle_timeout_secs: u64,
    #[pyo3(get, set)]
    pub application_name: Option<String>,
}

#[pymethods]
impl PyDatabaseConfig {
    #[new]
    #[pyo3(signature = (url, pool_size=10, min_idle=1, max_lifetime_secs=1800, connection_timeout_secs=5, idle_timeout_secs=300, application_name=None))]
    pub fn new(
        url: &str,
        pool_size: usize,
        min_idle: usize,
        max_lifetime_secs: u64,
        connection_timeout_secs: u64,
        idle_timeout_secs: u64,
        application_name: Option<String>,
    ) -> Self {
        Self {
            url: url.to_string(),
            pool_size,
            min_idle,
            max_lifetime_secs,
            connection_timeout_secs,
            idle_timeout_secs,
            application_name,
        }
    }

    /// Create PostgreSQL config.
    #[staticmethod]
    #[pyo3(signature = (host, port=5432, database="postgres", user="postgres", password=None, pool_size=10))]
    pub fn postgres(
        host: &str,
        port: u16,
        database: &str,
        user: &str,
        password: Option<String>,
        pool_size: usize,
    ) -> Self {
        let url = if let Some(pw) = password {
            format!("postgresql://{}:{}@{}:{}/{}", user, pw, host, port, database)
        } else {
            format!("postgresql://{}@{}:{}/{}", user, host, port, database)
        };
        Self::new(&url, pool_size, 1, 1800, 5, 300, Some("cello".to_string()))
    }
}

/// Python-exposed GraphQL configuration.
#[pyclass(name = "GraphQLConfig")]
#[derive(Clone)]
pub struct PyGraphQLConfig {
    #[pyo3(get, set)]
    pub path: String,
    #[pyo3(get, set)]
    pub playground: bool,
    #[pyo3(get, set)]
    pub introspection: bool,
    #[pyo3(get, set)]
    pub max_depth: Option<usize>,
    #[pyo3(get, set)]
    pub max_complexity: Option<usize>,
    #[pyo3(get, set)]
    pub batching: bool,
    #[pyo3(get, set)]
    pub tracing: bool,
}

#[pymethods]
impl PyGraphQLConfig {
    #[new]
    #[pyo3(signature = (path="/graphql", playground=true, introspection=true, max_depth=None, max_complexity=None, batching=false, tracing=false))]
    pub fn new(
        path: &str,
        playground: bool,
        introspection: bool,
        max_depth: Option<usize>,
        max_complexity: Option<usize>,
        batching: bool,
        tracing: bool,
    ) -> Self {
        Self {
            path: path.to_string(),
            playground,
            introspection,
            max_depth,
            max_complexity,
            batching,
            tracing,
        }
    }

    /// Create production-safe config (no playground, no introspection).
    #[staticmethod]
    pub fn production() -> Self {
        Self::new("/graphql", false, false, Some(10), Some(1000), false, false)
    }

    /// Create development config (playground enabled).
    #[staticmethod]
    pub fn development() -> Self {
        Self::new("/graphql", true, true, Some(20), None, true, true)
    }
}

/// Helper to call lifecycle handlers (sync or async).
fn call_lifecycle_handler(py: Python<'_>, handler: &PyObject) -> PyResult<()> {
    // Call the handler. If it returns a coroutine, run it.
    let result = handler.call0(py)?;
    
    let inspect = py.import("inspect")?;
    let is_coroutine = inspect
        .call_method1("iscoroutine", (result.as_ref(py),))?
        .is_true()?;
    
    if is_coroutine {
        let asyncio = py.import("asyncio")?;
        let _ = asyncio.call_method1("run", (result.as_ref(py),))?;
    }
    
    Ok(())
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

    // v0.7.0 - Enterprise Configuration Classes
    m.add_class::<PyOpenTelemetryConfig>()?;
    m.add_class::<PyHealthCheckConfig>()?;
    m.add_class::<PyDatabaseConfig>()?;
    m.add_class::<PyGraphQLConfig>()?;

    Ok(())
}
