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
pub mod middleware;
pub mod request;
pub mod response;
pub mod routing;
pub mod server;
pub mod timeout;

// New v0.5.0 modules
pub mod background;
pub mod openapi;
pub mod template;

// v1.1.0 - MiniJinja template engine
pub mod minijinja_engine;

// Rust-native async HTTP client
pub mod http_client;

// Persistent asyncio event loop used to drive async def handlers.
pub mod async_loop;
// Rust future -> asyncio awaitable bridge (single `cello-io` thread) for native I/O.
pub mod async_bridge;

// Native async data layer: real Postgres pool + Redis client (issue #5).
pub mod db;

// v1.4.0 - Compiled ORM schema registry (one spec per model).
pub mod schema;

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
    /// Maximum request body size in bytes (0 = unlimited). Enforced by the server
    /// before the body is buffered, preventing unbounded-memory (OOM) requests.
    max_body_size: usize,
    /// Timeout (seconds, 0 = disabled) for reading the full request body.
    read_body_timeout_secs: u64,
    /// Timeout (seconds, 0 = disabled) for reading request headers (Slowloris guard).
    read_header_timeout_secs: u64,
    /// Timeout (seconds, 0 = disabled) for a single handler to produce a response.
    handler_timeout_secs: u64,
    /// Close keep-alive connections idle this long (seconds, 0 = disabled).
    idle_timeout_secs: u64,
    /// Maximum request head size in bytes (0 = hyper default).
    max_header_size: usize,
    /// Maximum concurrent connections (0 = unlimited).
    max_connections: usize,
    /// Requests per connection before it is closed (0 = unlimited).
    max_requests_per_connection: usize,
    /// Maximum number of blocking threads available for offloaded sync handlers.
    blocking_threads: usize,
    /// Proxies whose forwarding headers this app trusts (shared with the
    /// running server, which reads it per connection).
    trusted_proxies: Arc<parking_lot::RwLock<Arc<server::proxy::TrustedProxies>>>,
    /// Native Postgres pool built by `enable_database()`, exposed as `app.database`
    /// and injected into each request as `request.database`.
    database_client: Option<Py<db::PyDatabase>>,
    /// Native Redis client built by `enable_redis()`, exposed as `app.redis`
    /// and injected into each request as `request.redis`.
    redis_client: Option<Py<db::PyRedis>>,
}

/// Default request body cap (10 MiB), shared with `LimitsConfig()`.
const DEFAULT_MAX_BODY_SIZE: usize = 10 * 1024 * 1024;
/// Default request head cap (16 KiB), shared with `LimitsConfig()`.
const DEFAULT_MAX_HEADER_SIZE: usize = 16 * 1024;

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
            // The defaults are `LimitsConfig()` and the body/handler values
            // of `TimeoutConfig()`, so an app that never calls
            // `set_limits()` / `set_timeouts()` is already bounded. Raise or
            // disable (0) any of them explicitly.
            max_body_size: DEFAULT_MAX_BODY_SIZE,
            read_body_timeout_secs: 30,
            // Slowloris guard; also bounds how long an idle keep-alive
            // connection waits for its next request.
            read_header_timeout_secs: 30,
            handler_timeout_secs: 30,
            // Off: the header-read timeout above already closes a keep-alive
            // connection that sends no next request within 30 s, and the
            // watchdog costs a little on every request.
            idle_timeout_secs: 0,
            max_header_size: DEFAULT_MAX_HEADER_SIZE,
            max_connections: 10_000,
            max_requests_per_connection: 1000,
            // 64 concurrent blocking handlers is enough to absorb typical blocking
            // I/O without the memory and GIL contention of a very large pool.
            blocking_threads: 64,
            trusted_proxies: Arc::new(parking_lot::RwLock::new(Arc::new(
                server::proxy::TrustedProxies::default(),
            ))),
            database_client: None,
            redis_client: None,
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

    /// Middleware that keep their state in this process's memory (sessions,
    /// rate limits, response cache): with `workers > 1` each worker has its
    /// own copy.
    pub fn per_process_state(&self) -> Vec<String> {
        self.middleware
            .middleware_names()
            .into_iter()
            .filter(|n| matches!(n.as_str(), "session" | "rate_limit" | "cache"))
            .collect()
    }

    /// Register a WebSocket route. Browser handshakes are accepted from the
    /// same origin and from `allowed_origins` (`["*"]` allows any).
    #[pyo3(signature = (path, handler, allowed_origins=None))]
    pub fn websocket(
        &mut self,
        path: &str,
        handler: PyObject,
        allowed_origins: Option<Vec<String>>,
    ) -> PyResult<()> {
        self.websocket_handlers
            .register(path, handler, allowed_origins.unwrap_or_default());
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

    /// Configure request and connection limits.
    ///
    /// Pass a `LimitsConfig`. Every field is enforced: `max_body_size` (413),
    /// `max_header_size` (431, minimum 8 KiB), `max_connections` (excess
    /// connections are closed on accept) and `max_requests_per_connection`
    /// (the connection is closed after that many responses). 0 = unlimited.
    pub fn set_limits(&mut self, config: &PyLimitsConfig) {
        self.max_body_size = config.max_body_size;
        self.max_header_size = config.max_header_size;
        self.max_connections = config.max_connections;
        self.max_requests_per_connection = config.max_requests_per_connection;
    }

    /// Configure server timeouts (all values in seconds; 0 disables that timeout).
    ///
    /// Pass a `TimeoutConfig`. Uses `read_header_timeout` (Slowloris guard),
    /// `read_body_timeout`, `handler_timeout` and `idle_timeout` (keep-alive
    /// connections with no request in flight). `write_timeout` is not enforced.
    pub fn set_timeouts(&mut self, config: &PyTimeoutConfig) {
        self.read_header_timeout_secs = config.read_header_timeout;
        self.read_body_timeout_secs = config.read_body_timeout;
        self.handler_timeout_secs = config.handler_timeout;
        self.idle_timeout_secs = config.idle_timeout;
    }

    /// Trust `X-Forwarded-For` / `X-Forwarded-Proto` / `X-Forwarded-Host` /
    /// (and, with `trust_x_real_ip=True`, `X-Real-IP`) only from these peers (IP
    /// addresses or CIDR networks, e.g. `["10.0.0.0/8", "127.0.0.1"]`). The
    /// list belongs to this app. By default no proxy is trusted and the client
    /// address is the TCP peer; an empty list restores that. Enable
    /// `trust_x_real_ip` only when every trusted proxy overwrites that header.
    #[pyo3(signature = (proxies, trust_x_real_ip=false))]
    pub fn set_trusted_proxies(&self, proxies: Vec<String>, trust_x_real_ip: bool) -> PyResult<()> {
        let parsed = proxies
            .iter()
            .map(|p| server::proxy::Cidr::parse(p))
            .collect::<Result<Vec<_>, _>>()
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        *self.trusted_proxies.write() =
            Arc::new(server::proxy::TrustedProxies::new(parsed, trust_x_real_ip));
        Ok(())
    }

    /// Configure the blocking threadpool used for blocking sync handlers.
    ///
    /// Pass a `ThreadPoolConfig`. Must be called before `run()` — the pool size is
    /// fixed when the runtime is built.
    pub fn set_threadpool(&mut self, config: &PyThreadPoolConfig) {
        self.blocking_threads = config.size.max(1);
        self.handlers.set_offload_threshold_us(if config.adaptive {
            config.offload_threshold_ms.saturating_mul(1_000)
        } else {
            u64::MAX
        });
    }

    /// Enable CORS middleware.
    #[pyo3(signature = (origins=None))]
    pub fn enable_cors(&mut self, origins: Option<Vec<String>>) {
        let mut cors = middleware::CorsMiddleware::new();
        if let Some(o) = origins {
            cors.set_origins(o);
        }
        self.middleware.add(cors);
    }

    /// Enable Prometheus metrics.
    #[pyo3(signature = (endpoint=None, namespace=None, subsystem=None))]
    pub fn enable_prometheus(
        &mut self,
        endpoint: Option<String>,
        namespace: Option<String>,
        subsystem: Option<String>,
    ) -> PyResult<()> {
        let mut config = middleware::prometheus::PrometheusConfig::default();
        if let Some(e) = endpoint {
            config.endpoint = e;
        }
        if let Some(n) = namespace {
            config.namespace = n;
        }
        if let Some(s) = subsystem {
            config.subsystem = s;
        }

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
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "Unknown rate limit algorithm",
                ))
            }
        };

        use middleware::rate_limit::KeyExtractors;
        let key = config.key_by.as_str();
        let extractor = match key {
            "ip" => KeyExtractors::client_ip(),
            // `request.context["user"]["id"]` / `["sub"]` (JWT claims), falling
            // back to the client IP for anonymous callers.
            "user" => KeyExtractors::user_or_ip(),
            "api_key" => KeyExtractors::header_or_ip("x-api-key"),
            _ if key.starts_with("header:") && key.len() > "header:".len() => {
                KeyExtractors::header_or_ip(&key["header:".len()..])
            }
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Unknown rate limit key_by {key:?}: use \"ip\", \"user\", \"api_key\" or \"header:<name>\""
                )))
            }
        };

        self.middleware.add(mw.key_extractor(extractor));
        Ok(())
    }

    pub fn add_guard(&mut self, guard: PyObject) -> PyResult<()> {
        let python_guard = middleware::guards::PythonGuard::new(guard);
        self.guards.add_guard(python_guard);
        Ok(())
    }

    /// Register a singleton dependency.
    pub fn register_singleton(&mut self, name: String, value: PyObject) {
        self.dependency_container
            .register_py_singleton(&name, value);
        // Without this the registry's fast-path check stays false and `Depends(...)`
        // parameters are never resolved — the handler receives the Depends marker.
        self.handlers.set_has_dependencies(true);
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
    ///
    /// `compress` (default `True`) gzips a cache HIT inline for clients that send
    /// `Accept-Encoding: gzip`, so cached large responses stay compressed even
    /// though a HIT short-circuits the compression middleware.
    ///
    /// Requests carrying `Authorization` or `Cookie` are not cached unless
    /// `cache_authenticated=True`, in which case those headers join the key.
    #[pyo3(signature = (ttl=300, methods=None, exclude_paths=None, compress=true, cache_authenticated=false, max_entries=10000))]
    pub fn enable_caching(
        &mut self,
        ttl: u64,
        methods: Option<Vec<String>>,
        exclude_paths: Option<Vec<String>>,
        compress: bool,
        cache_authenticated: bool,
        max_entries: usize,
    ) {
        let mut config = middleware::cache::CacheConfig::default();
        config.default_ttl = ttl;
        config.compress = compress;
        config.cache_authenticated = cache_authenticated;
        config.store = Arc::new(
            middleware::cache::InMemoryCacheStore::new().with_max_size(max_entries.max(1)),
        );
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
        failure_codes: Option<Vec<u16>>,
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

    /// Enable JWT authentication middleware.
    #[pyo3(signature = (config, skip_paths=None))]
    pub fn enable_jwt(
        &mut self,
        config: PyJwtConfig,
        skip_paths: Option<Vec<String>>,
    ) -> PyResult<()> {
        use jsonwebtoken::Algorithm;
        let alg = match config.algorithm.as_str() {
            "HS256" => Algorithm::HS256,
            "HS384" => Algorithm::HS384,
            "HS512" => Algorithm::HS512,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Unsupported JWT algorithm: {}",
                    config.algorithm
                )))
            }
        };
        let jwt_config = middleware::auth::JwtConfig {
            secret: config.secret.into_bytes(),
            algorithm: alg,
            issuer: config.issuer.clone(),
            audience: config.audience.clone(),
            leeway: config.leeway,
        };
        let mut jwt_auth = middleware::auth::JwtAuth::new(jwt_config);
        jwt_auth = jwt_auth.header_name(&config.header_name);
        if let Some(cookie) = config.cookie_name {
            jwt_auth = jwt_auth.cookie(&cookie);
        }
        if let Some(paths) = skip_paths {
            for path in paths {
                jwt_auth = jwt_auth.skip_path(&path);
            }
        }
        self.middleware.add(jwt_auth);
        Ok(())
    }

    /// Enable session middleware.
    #[pyo3(signature = (config=None))]
    pub fn enable_session(&mut self, config: Option<PySessionConfig>) {
        use middleware::session::{CookieConfig, SameSite};
        let mut mw = middleware::session::SessionMiddleware::new();
        if let Some(cfg) = config {
            let store =
                middleware::session::InMemorySessionStore::new().max_sessions(cfg.max_sessions);
            mw = middleware::session::SessionMiddleware::with_store(store)
                .max_session_size(cfg.max_session_size);
            if let Some(keys) = &cfg.auth_keys {
                mw = mw.auth_keys(keys.clone());
            }
            let same_site = match cfg.cookie_same_site.to_ascii_lowercase().as_str() {
                "strict" => SameSite::Strict,
                "none" => SameSite::None,
                _ => SameSite::Lax,
            };
            let mut cookie = CookieConfig::new(&cfg.cookie_name)
                .path(&cfg.cookie_path)
                .secure(cfg.cookie_secure)
                .http_only(cfg.cookie_http_only)
                .same_site(same_site)
                .max_age(cfg.max_age);
            if let Some(domain) = &cfg.cookie_domain {
                cookie = cookie.domain(domain);
            }
            mw = mw
                .cookie(cookie)
                .ttl(std::time::Duration::from_secs(cfg.max_age));
        }
        self.middleware.add(mw);
    }

    /// Enable security headers middleware.
    ///
    /// Accepts either:
    /// - nothing / `None` — sensible defaults,
    /// - a `bool` — `True` selects the strict preset (CSP, 2y HSTS preload, …),
    /// - a `SecurityHeadersConfig` — explicit header configuration.
    #[pyo3(signature = (config=None))]
    pub fn enable_security_headers(&mut self, config: Option<&PyAny>) -> PyResult<()> {
        let mw = match config {
            None => middleware::security::SecurityHeadersMiddleware::new(),
            Some(obj) => {
                if let Ok(cfg) = obj.extract::<PySecurityHeadersConfig>() {
                    build_security_headers_mw(&cfg)
                } else if let Ok(strict) = obj.extract::<bool>() {
                    if strict {
                        middleware::security::SecurityHeadersMiddleware::strict()
                    } else {
                        middleware::security::SecurityHeadersMiddleware::new()
                    }
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "enable_security_headers expects a bool or SecurityHeadersConfig",
                    ));
                }
            }
        };
        self.middleware.add(mw);
        Ok(())
    }

    /// Enable CSRF protection middleware.
    #[pyo3(signature = (cookie_name=None, header_name=None, allowed_origins=None))]
    pub fn enable_csrf(
        &mut self,
        cookie_name: Option<String>,
        header_name: Option<String>,
        allowed_origins: Option<Vec<String>>,
    ) {
        let mut config = middleware::csrf::CsrfConfig::default();
        if let Some(c) = cookie_name {
            config.cookie_name = c;
        }
        if let Some(h) = header_name {
            config.header_name = h;
        }
        if let Some(o) = allowed_origins {
            config.allowed_origins = o;
        }
        let mw = middleware::csrf::CsrfMiddleware::with_config(config);
        self.middleware.add(mw);
    }

    /// Enable Basic authentication middleware.
    #[pyo3(signature = (credentials, realm=None))]
    pub fn enable_basic_auth(
        &mut self,
        credentials: std::collections::HashMap<String, String>,
        realm: Option<String>,
    ) {
        let realm_str = realm.unwrap_or_else(|| "Restricted".to_string());
        let creds = std::sync::Arc::new(credentials);
        let mw = middleware::auth::BasicAuth::with_validator(move |u, p| {
            middleware::auth::credentials_match(&creds, u, p)
        })
        .realm(&realm_str);
        self.middleware.add(mw);
    }

    /// Enable API Key authentication middleware.
    #[pyo3(signature = (keys, header=None))]
    pub fn enable_api_key(
        &mut self,
        keys: std::collections::HashMap<String, String>,
        header: Option<String>,
    ) {
        let mut mw = middleware::auth::ApiKeyAuth::from_keys(keys);
        if let Some(h) = header {
            mw = mw.header(&h);
        }
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
    // Enterprise Features (v0.7.0+) / Data Layer (v0.8.0)
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

        println!("📊 OpenTelemetry enabled for service: {service_name}");
    }

    /// Enable health check endpoints.
    #[pyo3(signature = (config=None))]
    pub fn enable_health_checks(&mut self, config: Option<PyHealthCheckConfig>) {
        let config = config
            .unwrap_or_else(|| PyHealthCheckConfig::new("/health", true, false, None, 5, Some(5)));

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
        let config = config.unwrap_or_else(|| {
            PyGraphQLConfig::new("/graphql", false, false, Some(10), Some(1000), false, false)
        });

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

    /// Enable the native Postgres connection pool (real, backed by
    /// `deadpool-postgres`). After this, `app.database` and `request.database`
    /// return a live pool exposing `fetch`/`fetchrow`/`fetchval`/`execute`/
    /// `transaction` (see `db::PyDatabase`).
    #[pyo3(signature = (config))]
    pub fn enable_database(&mut self, py: Python<'_>, config: PyDatabaseConfig) -> PyResult<()> {
        let tls_opts = db::tls::TlsOptions {
            sslmode: config.sslmode.clone(),
            sslrootcert: config.sslrootcert.clone(),
            sslcert: config.sslcert.clone(),
            sslkey: config.sslkey.clone(),
        };
        let database = db::PyDatabase::connect(&config.url, config.pool_size, tls_opts)?;
        let sslmode = database.ssl_mode();
        self.database_client = Some(Py::new(py, database)?);
        println!("🗄️  Database pool enabled (native deadpool-postgres):");
        println!("   Pool size: {}", config.pool_size);
        println!("   TLS:       sslmode={}", sslmode.as_str());
        Ok(())
    }

    /// Enable the native async Redis client (real, backed by the `redis` crate's
    /// connection manager). After this, `app.redis` and `request.redis` return a
    /// live client (see `db::PyRedis`).
    #[pyo3(signature = (config))]
    pub fn enable_redis(&mut self, py: Python<'_>, config: PyRedisConfig) -> PyResult<()> {
        let redis = db::PyRedis::connect(&config.url)?;
        self.redis_client = Some(Py::new(py, redis)?);
        println!("🔴 Redis connection enabled (native redis crate):");
        if config.cluster_mode {
            println!("   Cluster mode: requested (native client uses standard mode)");
        }
        Ok(())
    }

    /// Give a freshly forked worker its own database pool and Redis connection.
    /// Called by `App.run()` in each child right after `fork()`.
    pub fn _reset_after_fork(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(db) = &self.database_client {
            db.borrow_mut(py).reset_after_fork()?;
        }
        if let Some(r) = &self.redis_client {
            r.borrow_mut(py).reset_after_fork();
        }
        Ok(())
    }

    /// The native Postgres pool, or `None` if `enable_database()` was not called.
    /// Use inside `on_event("startup")` to create tables, etc.
    #[getter]
    pub fn database(&self, py: Python<'_>) -> Option<PyObject> {
        self.database_client.as_ref().map(|d| d.to_object(py))
    }

    /// The native Redis client, or `None` if `enable_redis()` was not called.
    #[getter]
    pub fn redis(&self, py: Python<'_>) -> Option<PyObject> {
        self.redis_client.as_ref().map(|r| r.to_object(py))
    }

    // ========================================================================
    // v0.9.0 - API Protocol Features
    // ========================================================================

    /// Record gRPC configuration.
    ///
    /// NOTE: this does **not** start a gRPC server on its own — it only records
    /// intent and validates the config. The gRPC runtime is provided by the
    /// `cello.grpc` Python module; use that to serve gRPC services.
    #[pyo3(signature = (config=None))]
    pub fn enable_grpc(&mut self, config: Option<PyGrpcConfig>) {
        let config = config
            .unwrap_or_else(|| PyGrpcConfig::new("[::]:50051", true, 4194304, false, 60, 100));

        // Validate the config by constructing it (dropped immediately).
        let _grpc_config = middleware::grpc::GrpcConfig {
            address: config.address.clone(),
            services: Vec::new(),
            reflection: config.reflection,
            max_message_size: config.max_message_size,
            enable_web: config.enable_web,
            keepalive_secs: config.keepalive_secs,
            concurrency_limit: config.concurrency_limit,
        };

        eprintln!(
            "ℹ️  gRPC configured (address {}). This records config only — serve gRPC via the `cello.grpc` module.",
            config.address
        );
    }

    /// Record a gRPC service name (config only; see `enable_grpc`).
    #[pyo3(signature = (name, methods=None))]
    pub fn add_grpc_service(&mut self, name: String, methods: Option<Vec<String>>) {
        let _methods = methods.unwrap_or_default();
        eprintln!("ℹ️  gRPC service recorded: {name} (runtime lives in the `cello.grpc` module).");
    }

    /// Record message-queue (Kafka) configuration.
    ///
    /// NOTE: config only — producing/consuming is provided by the
    /// `cello.messaging` Python module; this does not start a broker client.
    #[pyo3(signature = (config))]
    pub fn enable_messaging(&mut self, config: PyKafkaConfig) {
        eprintln!(
            "ℹ️  Kafka configured (brokers: {}). This records config only — use the `cello.messaging` module.",
            config.brokers.join(", ")
        );
    }

    /// Record RabbitMQ configuration (config only; see `cello.messaging`).
    #[pyo3(signature = (config))]
    pub fn enable_rabbitmq(&mut self, config: PyRabbitMQConfig) {
        eprintln!(
            "ℹ️  RabbitMQ configured (url: {}). This records config only — use the `cello.messaging` module.",
            config.url
        );
    }

    /// Record SQS configuration (config only; see `cello.messaging`).
    #[pyo3(signature = (config))]
    pub fn enable_sqs(&mut self, config: PySqsConfig) {
        eprintln!(
            "ℹ️  SQS configured (region: {}, queue: {}). This records config only — use the `cello.messaging` module.",
            config.region, config.queue_url
        );
    }

    // ========================================================================
    // End API Protocol Features
    // ========================================================================

    // ========================================================================
    // v0.10.0 - Advanced Pattern Features
    // ========================================================================

    /// Record event-sourcing configuration.
    ///
    /// NOTE: config only. Event stores, aggregates, and snapshots are provided
    /// by the `cello.eventsourcing` Python module — this does not wire a store
    /// into the request pipeline.
    #[pyo3(signature = (config=None))]
    pub fn enable_event_sourcing(&mut self, config: Option<PyEventSourcingConfig>) {
        let config = config.unwrap_or_else(PyEventSourcingConfig::memory);

        // Validate the config by constructing it (dropped immediately).
        let _es_config = middleware::eventsourcing::EventSourcingConfig {
            store_type: config.store_type.clone(),
            snapshot_interval: config.snapshot_interval,
            enable_snapshots: config.enable_snapshots,
            max_events_per_aggregate: config.max_events_per_aggregate,
            event_ttl_secs: config.event_ttl_secs,
            connection_url: config.connection_url.clone(),
        };

        eprintln!(
            "ℹ️  Event sourcing configured (store: {}). This records config only — use the `cello.eventsourcing` module.",
            config.store_type
        );
    }

    /// Record CQRS configuration.
    ///
    /// NOTE: config only. Command/query buses are provided by the `cello.cqrs`
    /// Python module — this does not register handlers into the pipeline.
    #[pyo3(signature = (config=None))]
    pub fn enable_cqrs(&mut self, config: Option<PyCqrsConfig>) {
        let config = config.unwrap_or_else(PyCqrsConfig::default);

        let _cqrs_config = middleware::cqrs::CqrsConfig {
            enable_event_sync: config.enable_event_sync,
            command_timeout_ms: config.command_timeout_ms,
            query_timeout_ms: config.query_timeout_ms,
            max_retries: config.max_retries,
        };

        eprintln!(
            "ℹ️  CQRS configured (event_sync: {}). This records config only — use the `cello.cqrs` module.",
            config.enable_event_sync
        );
    }

    /// Record Saga configuration.
    ///
    /// NOTE: config only. Saga orchestration is provided by the `cello.saga`
    /// Python module — this does not start an orchestrator.
    #[pyo3(signature = (config=None))]
    pub fn enable_saga(&mut self, config: Option<PySagaConfig>) {
        let config = config.unwrap_or_else(PySagaConfig::default);

        let _saga_config = middleware::saga::SagaConfig {
            max_retries: config.max_retries,
            retry_delay_ms: config.retry_delay_ms,
            timeout_ms: config.timeout_ms,
            enable_logging: config.enable_logging,
        };

        eprintln!(
            "ℹ️  Saga configured (max_retries: {}). This records config only — use the `cello.saga` module.",
            config.max_retries
        );
    }

    // ========================================================================
    // End Advanced Pattern Features
    // ========================================================================

    /// Enable OpenAPI documentation endpoints.
    /// This adds:
    /// - GET /docs - Swagger UI
    /// - GET /redoc - ReDoc documentation
    /// - GET /openapi.json - OpenAPI JSON schema
    #[pyo3(signature = (title=None, version=None))]
    pub fn enable_openapi(
        &mut self,
        py: Python<'_>,
        title: Option<String>,
        version: Option<String>,
    ) -> PyResult<()> {
        let title = title.unwrap_or_else(|| "Cello API".to_string());
        let version = version.unwrap_or_else(|| "1.4.1".to_string());

        // The handler code is a constant; title and version are passed in as
        // values (never spliced into source), and HTML-escaped where rendered.
        // Interpolating them into code to `eval` broke on a quote and turned
        // an attacker-influenced title into code execution.
        const OPENAPI_HANDLERS: &str = r##"
import html as _html

def make_handlers(title, version):
    from cello import Response
    t = _html.escape(title, quote=True)

    def docs_handler(request):
        return Response.html(
            '<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8">'
            '<meta name="viewport" content="width=device-width, initial-scale=1.0">'
            '<title>' + t + ' - Swagger UI</title>'
            '<link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />'
            '<style>body { margin: 0; padding: 0; } .swagger-ui .topbar { display: none; }</style>'
            '</head><body><div id="swagger-ui"></div>'
            '<script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>'
            '<script>window.onload = () => { window.ui = SwaggerUIBundle({ url: "/openapi.json", '
            'dom_id: "#swagger-ui", deepLinking: true, presets: [SwaggerUIBundle.presets.apis, '
            'SwaggerUIBundle.SwaggerUIStandalonePreset], layout: "StandaloneLayout" }); };</script>'
            '</body></html>'
        )

    def redoc_handler(request):
        return Response.html(
            '<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8">'
            '<meta name="viewport" content="width=device-width, initial-scale=1.0">'
            '<title>' + t + ' - ReDoc</title><style>body { margin: 0; padding: 0; }</style>'
            '</head><body><redoc spec-url="/openapi.json"></redoc>'
            '<script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>'
            '</body></html>'
        )

    def openapi_handler(request):
        return {
            "openapi": "3.0.3",
            "info": {
                "title": title,
                "version": version,
                "description": title + " - Powered by Cello Framework",
            },
            "paths": {},
        }

    return docs_handler, redoc_handler, openapi_handler
"##;
        let module = pyo3::types::PyModule::from_code(
            py,
            OPENAPI_HANDLERS,
            "cello_openapi_handlers.py",
            "cello_openapi_handlers",
        )?;
        let (docs_handler, redoc_handler, openapi_handler): (PyObject, PyObject, PyObject) = module
            .getattr("make_handlers")?
            .call1((title, version))?
            .extract()?;

        self.add_route("GET", "/docs", docs_handler)?;
        self.add_route("GET", "/redoc", redoc_handler)?;
        self.add_route("GET", "/openapi.json", openapi_handler)?;

        println!("📚 OpenAPI docs enabled:");
        println!("   Swagger UI: /docs");
        println!("   ReDoc:      /redoc");
        println!("   OpenAPI:    /openapi.json");

        Ok(())
    }

    /// Choose whether unhandled handler errors are described in 500 responses
    /// (development) or replaced by a generic `Internal Server Error` with the
    /// details written to the server log (production).
    pub fn set_expose_errors(&self, expose: bool) {
        server::set_expose_error_details(expose);
    }

    /// Start the HTTP server.
    #[pyo3(signature = (host=None, port=None, workers=None))]
    pub fn run(
        &self,
        py: Python<'_>,
        host: Option<&str>,
        port: Option<u16>,
        workers: Option<usize>,
    ) -> PyResult<()> {
        let host_owned = host.unwrap_or("127.0.0.1").to_string();
        let port = port.unwrap_or(8000);

        // Clone everything needed inside the Send + 'static async block.
        let router = self.router.clone();
        let handlers = self.handlers.clone();
        let middleware = self.middleware.clone();
        let websocket_handlers = self.websocket_handlers.clone();
        let dependency_container = self.dependency_container.clone();
        let guards = self.guards.clone();
        let prometheus = self.prometheus.clone();
        let startup_handlers = self.startup_handlers.clone();
        let shutdown_handlers = self.shutdown_handlers.clone();

        // Limits/timeouts are plain Copy values captured for the server config.
        let max_body_size = self.max_body_size;
        let trusted_proxies = self.trusted_proxies.clone();
        let blocking_threads = self.blocking_threads;
        let read_body_timeout_secs = self.read_body_timeout_secs;
        let read_header_timeout_secs = self.read_header_timeout_secs;
        let handler_timeout_secs = self.handler_timeout_secs;
        let idle_timeout_secs = self.idle_timeout_secs;
        let max_header_size = self.max_header_size;
        let max_connections = self.max_connections;
        let max_requests_per_connection = self.max_requests_per_connection;

        // Start the persistent asyncio loop now, while single-threaded and holding the
        // GIL, so the first async request neither pays init cost nor races on it.
        async_loop::ensure_started(py);

        // Release the GIL and run a native Tokio current-thread runtime.
        //
        // pyo3_asyncio::tokio::run was previously used here but it drives Tokio I/O
        // through Python's asyncio selector loop, which breaks socket binding in
        // environments where the two event loops don't integrate (Python 3.12+ / pyo3 0.20).
        // Since the server's hot path is pure Rust I/O, we release the GIL with
        // allow_threads and block on a self-contained Tokio runtime. Python handlers
        // re-acquire the GIL individually via Python::with_gil when they need it.
        py.allow_threads(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                // Bounds the blocking pool that offloaded sync handlers, async
                // coroutine waits, and background tasks all share.
                .max_blocking_threads(blocking_threads)
                .build()
                .expect("failed to build Tokio runtime")
                .block_on(async move {
                    let mut config = server::ServerConfig::new(&host_owned, port);
                    config.workers = workers.unwrap_or(0);
                    config.max_body_size = max_body_size;
                    let secs_to_opt = |s: u64| {
                        if s == 0 {
                            None
                        } else {
                            Some(std::time::Duration::from_secs(s))
                        }
                    };
                    config.read_body_timeout = secs_to_opt(read_body_timeout_secs);
                    config.read_header_timeout = secs_to_opt(read_header_timeout_secs);
                    config.handler_timeout = secs_to_opt(handler_timeout_secs);
                    config.idle_timeout = secs_to_opt(idle_timeout_secs);
                    config.max_header_size = (max_header_size > 0).then_some(max_header_size);
                    config.max_connections = max_connections;
                    config.max_requests_per_connection = max_requests_per_connection;
                    config.trusted_proxies = trusted_proxies;

                    let server = Server::new(
                        config,
                        router,
                        handlers,
                        middleware,
                        websocket_handlers,
                        dependency_container,
                        guards,
                        prometheus,
                    );

                    // Startup hooks
                    for handler in &startup_handlers {
                        if let Err(e) = run_lifecycle_handler_async(handler.clone()).await {
                            eprintln!("Error in startup handler: {e}");
                        }
                    }

                    let _ = server.run().await;

                    // Ctrl-C (SIGINT) is delivered to CPython's own handler as well as
                    // Tokio's, which leaves a KeyboardInterrupt pending. The first Python
                    // call below would raise it, dropping the hook's coroutine un-awaited
                    // (and the error is filtered out below). Consume it so the hooks run.
                    Python::with_gil(|py| {
                        let _ = py.check_signals();
                    });

                    // Shutdown hooks
                    for handler in &shutdown_handlers {
                        match run_lifecycle_handler_async(handler.clone()).await {
                            Err(e) if !e.to_string().contains("KeyboardInterrupt") => {
                                eprintln!("Error in shutdown handler: {e}");
                            }
                            _ => {}
                        }
                    }
                })
        });
        Ok(())
    }

    /// Internal route registration.
    fn add_route(&mut self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let handler_id = self.handlers.register(handler);
        self.router
            .add_route(method, path, handler_id)
            .map_err(pyo3::exceptions::PyValueError::new_err)
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
    #[pyo3(signature = (max_header_size=DEFAULT_MAX_HEADER_SIZE, max_body_size=DEFAULT_MAX_BODY_SIZE, max_connections=10000, max_requests_per_connection=1000))]
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

/// Python-exposed blocking-threadpool configuration.
///
/// Sync `def` handlers run inline on the server thread by default, which is the
/// fastest path for handlers that just build and return a value. A handler that
/// *blocks* (sleep, database driver, socket I/O) would stall the server thread for
/// its whole duration, so Cello times sync handlers and moves any that exceed
/// `offload_threshold_ms` onto a bounded pool of OS threads, permanently.
///
/// Note the pool is shared with async-coroutine waits and background tasks, so size
/// it for the total of those plus concurrent blocking handlers.
#[pyclass(name = "ThreadPoolConfig")]
#[derive(Clone)]
pub struct PyThreadPoolConfig {
    /// Maximum number of blocking threads.
    #[pyo3(get, set)]
    pub size: usize,
    /// Sync handler duration (milliseconds) above which it is moved to the pool.
    #[pyo3(get, set)]
    pub offload_threshold_ms: u64,
    /// When false, only handlers marked `blocking=True` are offloaded.
    #[pyo3(get, set)]
    pub adaptive: bool,
}

#[pymethods]
impl PyThreadPoolConfig {
    #[new]
    #[pyo3(signature = (size=64, offload_threshold_ms=1, adaptive=true))]
    pub fn new(size: usize, offload_threshold_ms: u64, adaptive: bool) -> Self {
        Self {
            size,
            offload_threshold_ms,
            adaptive,
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
///
/// NOTE: configuration record only. The server does not terminate TLS in this
/// release; put a TLS-terminating proxy in front and use
/// `App.set_trusted_proxies()` so `X-Forwarded-Proto` is honoured.
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
///
/// NOTE: configuration record only. The server speaks HTTP/1.1 in this
/// release; this config is not read by the accept loop.
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
///
/// NOTE: configuration record only. The server speaks HTTP/1.1 in this
/// release; this config is not read by the accept loop.
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
    /// Required `iss` claim (rejects tokens from other issuers when set).
    #[pyo3(get, set)]
    pub issuer: Option<String>,
    /// Required `aud` claim (rejects tokens meant for other services when set).
    #[pyo3(get, set)]
    pub audience: Option<String>,
}

#[pymethods]
impl PyJwtConfig {
    #[new]
    #[pyo3(signature = (secret, algorithm="HS256", header_name="Authorization", cookie_name=None, leeway=0, issuer=None, audience=None))]
    pub fn new(
        secret: String,
        algorithm: &str,
        header_name: &str,
        cookie_name: Option<String>,
        leeway: u64,
        issuer: Option<String>,
        audience: Option<String>,
    ) -> Self {
        Self {
            secret,
            algorithm: algorithm.to_string(),
            header_name: header_name.to_string(),
            cookie_name,
            leeway,
            issuer,
            audience,
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
    pub fn adaptive(
        capacity: u64,
        refill_rate: u64,
        min_capacity: u64,
        error_threshold: f64,
    ) -> Self {
        Self::new(
            "adaptive",
            capacity,
            refill_rate,
            60,
            "ip",
            Some(min_capacity),
            Some(error_threshold),
        )
    }

    /// Create sliding window config.
    #[staticmethod]
    pub fn sliding_window(max_requests: u64, window_secs: u64) -> Self {
        Self::new(
            "sliding_window",
            max_requests,
            0,
            window_secs,
            "ip",
            None,
            None,
        )
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
    /// Sessions held in memory per process before the least recently used are
    /// evicted (0 = unlimited).
    #[pyo3(get, set)]
    pub max_sessions: usize,
    /// Largest serialized session in bytes (0 = unlimited); a write past it
    /// raises `ValueError`.
    #[pyo3(get, set)]
    pub max_session_size: usize,
    /// Keys whose change issues a new session id (login / privilege change).
    /// `None` keeps the built-in list.
    #[pyo3(get, set)]
    pub auth_keys: Option<Vec<String>>,
}

#[pymethods]
impl PySessionConfig {
    #[new]
    #[pyo3(signature = (cookie_name="session_id", cookie_path="/", cookie_domain=None, cookie_secure=true, cookie_http_only=true, cookie_same_site="Lax", max_age=86400, max_sessions=middleware::session::DEFAULT_MAX_SESSIONS, max_session_size=middleware::session::DEFAULT_MAX_SESSION_SIZE, auth_keys=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cookie_name: &str,
        cookie_path: &str,
        cookie_domain: Option<String>,
        cookie_secure: bool,
        cookie_http_only: bool,
        cookie_same_site: &str,
        max_age: u64,
        max_sessions: usize,
        max_session_size: usize,
        auth_keys: Option<Vec<String>>,
    ) -> Self {
        Self {
            cookie_name: cookie_name.to_string(),
            cookie_path: cookie_path.to_string(),
            cookie_domain,
            cookie_secure,
            cookie_http_only,
            cookie_same_site: cookie_same_site.to_string(),
            max_age,
            max_sessions,
            max_session_size,
            auth_keys,
        }
    }

    /// The keys that trigger automatic session-id regeneration by default.
    #[staticmethod]
    pub fn default_auth_keys() -> Vec<String> {
        middleware::session::DEFAULT_AUTH_KEYS
            .iter()
            .map(|k| k.to_string())
            .collect()
    }
}

/// Python-exposed security headers configuration.
#[pyclass(name = "SecurityHeadersConfig")]
#[derive(Clone, Default)]
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
    /// Content-Security-Policy directives (from a `CSP` builder).
    pub csp_directives: Option<std::collections::HashMap<String, Vec<String>>>,
    /// Permissions-Policy directives, e.g. `{"geolocation": [], "camera": ["'self'"]}`.
    pub permissions_policy: Option<std::collections::HashMap<String, Vec<String>>>,
    /// Cross-Origin-Embedder-Policy: "unsafe-none" | "require-corp" | "credentialless".
    pub coep: Option<String>,
    /// Cross-Origin-Opener-Policy: "unsafe-none" | "same-origin" | "same-origin-allow-popups".
    pub coop: Option<String>,
    /// Cross-Origin-Resource-Policy: "same-site" | "same-origin" | "cross-origin".
    pub corp: Option<String>,
}

#[pymethods]
impl PySecurityHeadersConfig {
    #[new]
    #[pyo3(signature = (x_frame_options="DENY", x_content_type_options=true, x_xss_protection="1; mode=block", referrer_policy="strict-origin-when-cross-origin", hsts_max_age=None, hsts_include_subdomains=false, hsts_preload=false, csp=None, permissions_policy=None, coep=None, coop=None, corp=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        x_frame_options: &str,
        x_content_type_options: bool,
        x_xss_protection: &str,
        referrer_policy: &str,
        hsts_max_age: Option<u64>,
        hsts_include_subdomains: bool,
        hsts_preload: bool,
        csp: Option<PyCsp>,
        permissions_policy: Option<std::collections::HashMap<String, Vec<String>>>,
        coep: Option<String>,
        coop: Option<String>,
        corp: Option<String>,
    ) -> Self {
        Self {
            x_frame_options: Some(x_frame_options.to_string()),
            x_content_type_options,
            x_xss_protection: Some(x_xss_protection.to_string()),
            referrer_policy: Some(referrer_policy.to_string()),
            hsts_max_age,
            hsts_include_subdomains,
            hsts_preload,
            csp_directives: csp.map(|c| c.directives),
            permissions_policy,
            coep,
            coop,
            corp,
        }
    }

    /// Create default secure headers (strict CSP + cross-origin isolation).
    #[staticmethod]
    pub fn secure() -> Self {
        let mut cfg = Self {
            x_frame_options: Some("DENY".to_string()),
            x_content_type_options: true,
            x_xss_protection: Some("1; mode=block".to_string()),
            referrer_policy: Some("strict-origin-when-cross-origin".to_string()),
            hsts_max_age: Some(31536000),
            hsts_include_subdomains: true,
            hsts_preload: false,
            ..Default::default()
        };
        cfg.coep = Some("require-corp".to_string());
        cfg.coop = Some("same-origin".to_string());
        cfg.corp = Some("same-origin".to_string());
        cfg
    }
}

/// Build a native `SecurityHeadersMiddleware` from the Python-facing config,
/// mapping string/optional fields onto the Rust enums. Used by
/// `App::enable_security_headers` when a `SecurityHeadersConfig` is passed.
fn build_security_headers_mw(
    cfg: &PySecurityHeadersConfig,
) -> middleware::security::SecurityHeadersMiddleware {
    use middleware::security::{
        HstsConfig, ReferrerPolicy, SecurityHeadersMiddleware, XFrameOptions,
    };

    let mut mw = SecurityHeadersMiddleware::new();

    mw.x_frame_options = cfg
        .x_frame_options
        .as_ref()
        .map(|v| match v.to_uppercase().as_str() {
            "DENY" => XFrameOptions::Deny,
            "SAMEORIGIN" => XFrameOptions::SameOrigin,
            other => XFrameOptions::AllowFrom(other.to_string()),
        });

    mw.x_content_type_options = cfg.x_content_type_options;

    // The middleware models X-XSS-Protection as an on/off toggle; treat an
    // empty or "0" value as disabled.
    mw.x_xss_protection = cfg
        .x_xss_protection
        .as_ref()
        .map(|s| !s.is_empty() && s != "0")
        .unwrap_or(false);

    mw.referrer_policy = cfg.referrer_policy.as_ref().map(|v| match v.as_str() {
        "no-referrer" => ReferrerPolicy::NoReferrer,
        "no-referrer-when-downgrade" => ReferrerPolicy::NoReferrerWhenDowngrade,
        "origin" => ReferrerPolicy::Origin,
        "origin-when-cross-origin" => ReferrerPolicy::OriginWhenCrossOrigin,
        "same-origin" => ReferrerPolicy::SameOrigin,
        "strict-origin" => ReferrerPolicy::StrictOrigin,
        "unsafe-url" => ReferrerPolicy::UnsafeUrl,
        _ => ReferrerPolicy::StrictOriginWhenCrossOrigin,
    });

    // HSTS is only emitted when a max-age is configured (it is meaningless and
    // risky to send over plain HTTP).
    mw.hsts = cfg.hsts_max_age.map(|age| {
        let mut h = HstsConfig::new(age);
        if cfg.hsts_include_subdomains {
            h = h.include_subdomains();
        }
        if cfg.hsts_preload {
            h = h.preload();
        }
        h
    });

    // Content-Security-Policy (built from the CSP builder's directives).
    if let Some(ref directives) = cfg.csp_directives {
        use middleware::security::ContentSecurityPolicy;
        let mut csp = ContentSecurityPolicy::new();
        for (name, values) in directives {
            let vals: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            csp = csp.directive(name, vals);
        }
        mw.csp = Some(csp);
    }

    // Permissions-Policy (built from the directive map).
    if let Some(ref directives) = cfg.permissions_policy {
        use middleware::security::PermissionsPolicy;
        let mut pp = PermissionsPolicy::new();
        for (name, values) in directives {
            let vals: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
            pp = pp.directive(name, vals);
        }
        mw.permissions_policy = Some(pp);
    }

    // Cross-Origin isolation headers (mapped from their string values).
    mw.coep = cfg.coep.as_deref().and_then(|v| {
        use middleware::security::CrossOriginEmbedderPolicy as E;
        match v {
            "unsafe-none" => Some(E::UnsafeNone),
            "require-corp" => Some(E::RequireCorp),
            "credentialless" => Some(E::Credentialless),
            _ => None,
        }
    });
    mw.coop = cfg.coop.as_deref().and_then(|v| {
        use middleware::security::CrossOriginOpenerPolicy as O;
        match v {
            "unsafe-none" => Some(O::UnsafeNone),
            "same-origin" => Some(O::SameOrigin),
            "same-origin-allow-popups" => Some(O::SameOriginAllowPopups),
            _ => None,
        }
    });
    mw.corp = cfg.corp.as_deref().and_then(|v| {
        use middleware::security::CrossOriginResourcePolicy as R;
        match v {
            "same-site" => Some(R::SameSite),
            "same-origin" => Some(R::SameOrigin),
            "cross-origin" => Some(R::CrossOrigin),
            _ => None,
        }
    });

    mw
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
// Enterprise Configuration Classes (v0.7.0+ / v0.8.0 Data Layer)
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
            excluded_paths: excluded_paths
                .unwrap_or_else(|| vec!["/health".to_string(), "/metrics".to_string()]),
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
    /// libpq `sslmode`: `disable` | `prefer` (default) | `require` |
    /// `verify-ca` | `verify-full`. Overrides an `sslmode` in the URL.
    #[pyo3(get, set)]
    pub sslmode: Option<String>,
    /// PEM file with the CA certificate(s) to trust (`"system"` = bundled
    /// Mozilla roots). Overrides `sslrootcert` in the URL.
    #[pyo3(get, set)]
    pub sslrootcert: Option<String>,
    /// PEM client certificate (chain) for certificate authentication.
    #[pyo3(get, set)]
    pub sslcert: Option<String>,
    /// PEM private key for `sslcert`.
    #[pyo3(get, set)]
    pub sslkey: Option<String>,
}

#[pymethods]
impl PyDatabaseConfig {
    #[new]
    #[pyo3(signature = (url, pool_size=10, min_idle=1, max_lifetime_secs=1800, connection_timeout_secs=5, idle_timeout_secs=300, application_name=None, sslmode=None, sslrootcert=None, sslcert=None, sslkey=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: &str,
        pool_size: usize,
        min_idle: usize,
        max_lifetime_secs: u64,
        connection_timeout_secs: u64,
        idle_timeout_secs: u64,
        application_name: Option<String>,
        sslmode: Option<String>,
        sslrootcert: Option<String>,
        sslcert: Option<String>,
        sslkey: Option<String>,
    ) -> Self {
        Self {
            url: url.to_string(),
            pool_size,
            min_idle,
            max_lifetime_secs,
            connection_timeout_secs,
            idle_timeout_secs,
            application_name,
            sslmode,
            sslrootcert,
            sslcert,
            sslkey,
        }
    }

    /// Create PostgreSQL config.
    #[staticmethod]
    #[pyo3(signature = (host, port=5432, database="postgres", user="postgres", password=None, pool_size=10, sslmode=None, sslrootcert=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn postgres(
        host: &str,
        port: u16,
        database: &str,
        user: &str,
        password: Option<String>,
        pool_size: usize,
        sslmode: Option<String>,
        sslrootcert: Option<String>,
    ) -> Self {
        let url = if let Some(pw) = password {
            format!("postgresql://{user}:{pw}@{host}:{port}/{database}")
        } else {
            format!("postgresql://{user}@{host}:{port}/{database}")
        };
        Self::new(
            &url,
            pool_size,
            1,
            1800,
            5,
            300,
            Some("cello".to_string()),
            sslmode,
            sslrootcert,
            None,
            None,
        )
    }
}

/// Python-exposed Redis configuration.
#[pyclass(name = "RedisConfig")]
#[derive(Clone)]
pub struct PyRedisConfig {
    #[pyo3(get, set)]
    pub url: String,
    #[pyo3(get, set)]
    pub pool_size: usize,
    #[pyo3(get, set)]
    pub min_idle: usize,
    #[pyo3(get, set)]
    pub connection_timeout_secs: u64,
    #[pyo3(get, set)]
    pub idle_timeout_secs: u64,
    #[pyo3(get, set)]
    pub cluster_mode: bool,
    #[pyo3(get, set)]
    pub default_ttl: Option<u64>,
    #[pyo3(get, set)]
    pub database: u8,
    #[pyo3(get, set)]
    pub password: Option<String>,
    #[pyo3(get, set)]
    pub tls: bool,
    #[pyo3(get, set)]
    pub key_prefix: Option<String>,
}

#[pymethods]
impl PyRedisConfig {
    #[new]
    #[pyo3(signature = (url="redis://127.0.0.1:6379", pool_size=10, min_idle=1, connection_timeout_secs=5, idle_timeout_secs=300, cluster_mode=false, default_ttl=None, database=0, password=None, tls=false, key_prefix=None))]
    pub fn new(
        url: &str,
        pool_size: usize,
        min_idle: usize,
        connection_timeout_secs: u64,
        idle_timeout_secs: u64,
        cluster_mode: bool,
        default_ttl: Option<u64>,
        database: u8,
        password: Option<String>,
        tls: bool,
        key_prefix: Option<String>,
    ) -> Self {
        Self {
            url: url.to_string(),
            pool_size,
            min_idle,
            connection_timeout_secs,
            idle_timeout_secs,
            cluster_mode,
            default_ttl,
            database,
            password,
            tls,
            key_prefix,
        }
    }

    /// Create config for local development.
    #[staticmethod]
    pub fn local() -> Self {
        Self::new(
            "redis://127.0.0.1:6379",
            5,
            1,
            5,
            300,
            false,
            None,
            0,
            None,
            false,
            None,
        )
    }

    /// Create config for cluster mode.
    #[staticmethod]
    #[pyo3(signature = (url, pool_size=20, password=None))]
    pub fn cluster(url: &str, pool_size: usize, password: Option<String>) -> Self {
        Self::new(
            url, pool_size, 2, 5, 300, true, None, 0, password, false, None,
        )
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
    /// Playground and introspection are off by default: both expose the schema
    /// to anyone who can reach the endpoint. Turn them on for development.
    #[pyo3(signature = (path="/graphql", playground=false, introspection=false, max_depth=Some(10), max_complexity=Some(1000), batching=false, tracing=false))]
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

// ==========================================================================
// v0.9.0 - API Protocol Configuration Classes
// ==========================================================================

/// Python-exposed gRPC configuration.
#[pyclass(name = "GrpcConfig")]
#[derive(Clone)]
pub struct PyGrpcConfig {
    #[pyo3(get, set)]
    pub address: String,
    #[pyo3(get, set)]
    pub reflection: bool,
    #[pyo3(get, set)]
    pub max_message_size: usize,
    #[pyo3(get, set)]
    pub enable_web: bool,
    #[pyo3(get, set)]
    pub keepalive_secs: u64,
    #[pyo3(get, set)]
    pub concurrency_limit: usize,
}

#[pymethods]
impl PyGrpcConfig {
    #[new]
    #[pyo3(signature = (address="[::]:50051", reflection=true, max_message_size=4194304, enable_web=false, keepalive_secs=60, concurrency_limit=100))]
    pub fn new(
        address: &str,
        reflection: bool,
        max_message_size: usize,
        enable_web: bool,
        keepalive_secs: u64,
        concurrency_limit: usize,
    ) -> Self {
        Self {
            address: address.to_string(),
            reflection,
            max_message_size,
            enable_web,
            keepalive_secs,
            concurrency_limit,
        }
    }

    /// Create config for local development.
    #[staticmethod]
    pub fn local() -> Self {
        Self::new("[::]:50051", true, 4194304, true, 60, 100)
    }

    /// Create config for production.
    #[staticmethod]
    #[pyo3(signature = (address="[::]:50051", max_message_size=4194304))]
    pub fn production(address: &str, max_message_size: usize) -> Self {
        Self::new(address, false, max_message_size, false, 120, 1000)
    }
}

/// Python-exposed Kafka configuration.
#[pyclass(name = "KafkaConfig")]
#[derive(Clone)]
pub struct PyKafkaConfig {
    #[pyo3(get, set)]
    pub brokers: Vec<String>,
    #[pyo3(get, set)]
    pub group_id: Option<String>,
    #[pyo3(get, set)]
    pub client_id: Option<String>,
    #[pyo3(get, set)]
    pub auto_commit: bool,
    #[pyo3(get, set)]
    pub session_timeout_ms: u64,
    #[pyo3(get, set)]
    pub max_poll_records: usize,
}

#[pymethods]
impl PyKafkaConfig {
    #[new]
    #[pyo3(signature = (brokers=None, group_id=None, client_id=None, auto_commit=true, session_timeout_ms=30000, max_poll_records=500))]
    pub fn new(
        brokers: Option<Vec<String>>,
        group_id: Option<String>,
        client_id: Option<String>,
        auto_commit: bool,
        session_timeout_ms: u64,
        max_poll_records: usize,
    ) -> Self {
        Self {
            brokers: brokers.unwrap_or_else(|| vec!["localhost:9092".to_string()]),
            group_id,
            client_id,
            auto_commit,
            session_timeout_ms,
            max_poll_records,
        }
    }

    /// Create config for local development.
    #[staticmethod]
    pub fn local() -> Self {
        Self::new(None, None, None, true, 30000, 500)
    }
}

/// Python-exposed RabbitMQ configuration.
#[pyclass(name = "RabbitMQConfig")]
#[derive(Clone)]
pub struct PyRabbitMQConfig {
    #[pyo3(get, set)]
    pub url: String,
    #[pyo3(get, set)]
    pub vhost: String,
    #[pyo3(get, set)]
    pub prefetch_count: u16,
    #[pyo3(get, set)]
    pub heartbeat: u16,
    #[pyo3(get, set)]
    pub connection_timeout_secs: u16,
}

#[pymethods]
impl PyRabbitMQConfig {
    #[new]
    #[pyo3(signature = (url="amqp://localhost", vhost="/", prefetch_count=10, heartbeat=60, connection_timeout_secs=5))]
    pub fn new(
        url: &str,
        vhost: &str,
        prefetch_count: u16,
        heartbeat: u16,
        connection_timeout_secs: u16,
    ) -> Self {
        Self {
            url: url.to_string(),
            vhost: vhost.to_string(),
            prefetch_count,
            heartbeat,
            connection_timeout_secs,
        }
    }

    /// Create config for local development.
    #[staticmethod]
    pub fn local() -> Self {
        Self::new("amqp://localhost", "/", 10, 60, 5)
    }
}

/// Python-exposed SQS configuration.
#[pyclass(name = "SqsConfig")]
#[derive(Clone)]
pub struct PySqsConfig {
    #[pyo3(get, set)]
    pub region: String,
    #[pyo3(get, set)]
    pub queue_url: String,
    #[pyo3(get, set)]
    pub endpoint_url: Option<String>,
    #[pyo3(get, set)]
    pub max_messages: i32,
    #[pyo3(get, set)]
    pub wait_time_secs: i32,
    #[pyo3(get, set)]
    pub visibility_timeout_secs: i32,
}

#[pymethods]
impl PySqsConfig {
    #[new]
    #[pyo3(signature = (region="us-east-1", queue_url="", endpoint_url=None, max_messages=10, wait_time_secs=20, visibility_timeout_secs=30))]
    pub fn new(
        region: &str,
        queue_url: &str,
        endpoint_url: Option<String>,
        max_messages: i32,
        wait_time_secs: i32,
        visibility_timeout_secs: i32,
    ) -> Self {
        Self {
            region: region.to_string(),
            queue_url: queue_url.to_string(),
            endpoint_url,
            max_messages,
            wait_time_secs,
            visibility_timeout_secs,
        }
    }

    /// Create config for local development (LocalStack).
    #[staticmethod]
    #[pyo3(signature = (queue_url))]
    pub fn local(queue_url: &str) -> Self {
        Self::new(
            "us-east-1",
            queue_url,
            Some("http://localhost:4566".to_string()),
            10,
            20,
            30,
        )
    }
}

// ==========================================================================
// v0.10.0 - Advanced Pattern Configuration Classes
// ==========================================================================

/// Python-exposed Event Sourcing configuration.
#[pyclass(name = "EventSourcingConfig")]
#[derive(Clone)]
pub struct PyEventSourcingConfig {
    #[pyo3(get, set)]
    pub store_type: String,
    #[pyo3(get, set)]
    pub snapshot_interval: u32,
    #[pyo3(get, set)]
    pub enable_snapshots: bool,
    #[pyo3(get, set)]
    pub max_events_per_aggregate: usize,
    #[pyo3(get, set)]
    pub event_ttl_secs: u64,
    #[pyo3(get, set)]
    pub connection_url: Option<String>,
}

#[pymethods]
impl PyEventSourcingConfig {
    #[new]
    #[pyo3(signature = (store_type="memory", snapshot_interval=100, enable_snapshots=true, max_events_per_aggregate=10000, event_ttl_secs=0, connection_url=None))]
    pub fn new(
        store_type: &str,
        snapshot_interval: u32,
        enable_snapshots: bool,
        max_events_per_aggregate: usize,
        event_ttl_secs: u64,
        connection_url: Option<String>,
    ) -> Self {
        Self {
            store_type: store_type.to_string(),
            snapshot_interval,
            enable_snapshots,
            max_events_per_aggregate,
            event_ttl_secs,
            connection_url,
        }
    }

    /// Create an in-memory event sourcing configuration.
    #[staticmethod]
    pub fn memory() -> Self {
        Self::new("memory", 100, true, 10000, 0, None)
    }

    /// Create a PostgreSQL-backed event sourcing configuration.
    #[staticmethod]
    #[pyo3(signature = (url))]
    pub fn postgresql(url: &str) -> Self {
        Self::new("postgresql", 100, true, 10000, 0, Some(url.to_string()))
    }
}

/// Python-exposed CQRS configuration.
#[pyclass(name = "CqrsConfig")]
#[derive(Clone)]
pub struct PyCqrsConfig {
    #[pyo3(get, set)]
    pub enable_event_sync: bool,
    #[pyo3(get, set)]
    pub command_timeout_ms: u64,
    #[pyo3(get, set)]
    pub query_timeout_ms: u64,
    #[pyo3(get, set)]
    pub max_retries: u32,
}

#[pymethods]
impl PyCqrsConfig {
    #[new]
    #[pyo3(signature = (enable_event_sync=true, command_timeout_ms=5000, query_timeout_ms=3000, max_retries=3))]
    pub fn new(
        enable_event_sync: bool,
        command_timeout_ms: u64,
        query_timeout_ms: u64,
        max_retries: u32,
    ) -> Self {
        Self {
            enable_event_sync,
            command_timeout_ms,
            query_timeout_ms,
            max_retries,
        }
    }

    /// Create a default CQRS configuration.
    #[staticmethod]
    pub fn default() -> Self {
        Self::new(true, 5000, 3000, 3)
    }
}

/// Python-exposed Saga configuration.
#[pyclass(name = "SagaConfig")]
#[derive(Clone)]
pub struct PySagaConfig {
    #[pyo3(get, set)]
    pub max_retries: u32,
    #[pyo3(get, set)]
    pub retry_delay_ms: u64,
    #[pyo3(get, set)]
    pub timeout_ms: u64,
    #[pyo3(get, set)]
    pub enable_logging: bool,
}

#[pymethods]
impl PySagaConfig {
    #[new]
    #[pyo3(signature = (max_retries=3, retry_delay_ms=1000, timeout_ms=30000, enable_logging=true))]
    pub fn new(
        max_retries: u32,
        retry_delay_ms: u64,
        timeout_ms: u64,
        enable_logging: bool,
    ) -> Self {
        Self {
            max_retries,
            retry_delay_ms,
            timeout_ms,
            enable_logging,
        }
    }

    /// Create a default Saga configuration.
    #[staticmethod]
    pub fn default() -> Self {
        Self::new(3, 1000, 30000, true)
    }
}

/// Helper to call lifecycle handlers (sync or async).
/// Drive a lifecycle hook (startup/shutdown) to completion.
///
/// Handles both sync `def` and `async def` hooks. For async hooks the coroutine
/// is driven by Tokio via pyo3-asyncio so the GIL is released during I/O waits,
/// consistent with how request handlers are executed.
async fn run_lifecycle_handler_async(handler: PyObject) -> Result<(), String> {
    // Phase 1 (GIL): call the handler; detect whether it returned a coroutine.
    let (result, is_coro) = Python::with_gil(|py| -> PyResult<(PyObject, bool)> {
        let ret = handler.call0(py)?;
        let inspect = py.import("inspect")?;
        let is_coro = inspect
            .call_method1("iscoroutine", (ret.as_ref(py),))?
            .is_true()?;
        Ok((ret, is_coro))
    })
    .map_err(|e| e.to_string())?;

    // Phase 2 (GIL released): drive the coroutine on the persistent asyncio loop.
    // (The previous `pyo3_asyncio::tokio::into_future` path failed at runtime because
    // pyo3_asyncio is never initialised, so async startup/shutdown hooks silently did
    // not run.)
    if is_coro {
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
        tokio::task::spawn_blocking(move || {
            let r = Python::with_gil(|py| {
                async_loop::run_coroutine_blocking(py, result.as_ref(py)).map(|_| ())
            });
            let _ = tx.send(r);
        });
        match rx.await {
            Ok(inner) => inner?,
            Err(e) => return Err(format!("Lifecycle channel error: {e}")),
        }
    }

    Ok(())
}

/// Python module definition.
/// Parse and check a GraphQL query for the Python engine (`cello.graphql`).
///
/// Returns `{"operation", "fields", "introspection", "complexity"}`, where each
/// field is `{"name", "alias", "arguments", "selection"}` with variables already
/// substituted. Raises `ValueError` for a syntax error, an unknown operation, a
/// missing variable, a fragment cycle, or a query over `max_depth` /
/// `max_complexity`, or one that uses introspection while it is disabled.
#[pyfunction]
#[pyo3(signature = (query, operation_name=None, variables=None, max_depth=None, max_complexity=None, introspection=false))]
fn graphql_plan(
    py: Python<'_>,
    query: &str,
    operation_name: Option<&str>,
    variables: Option<&PyAny>,
    max_depth: Option<usize>,
    max_complexity: Option<usize>,
    introspection: bool,
) -> PyResult<PyObject> {
    let variables = match variables {
        None => serde_json::Map::new(),
        Some(v) if v.is_none() => serde_json::Map::new(),
        Some(v) => {
            match json::python_to_json(py, v).map_err(pyo3::exceptions::PyValueError::new_err)? {
                serde_json::Value::Object(map) => map,
                _ => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "variables must be a dict",
                    ))
                }
            }
        }
    };
    let limits = middleware::graphql_plan::Limits {
        max_depth,
        max_complexity,
        introspection,
    };
    let plan = py
        .allow_threads(|| middleware::graphql_plan::plan(query, operation_name, &variables, limits))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let value = serde_json::to_value(&plan)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    json::json_to_python(py, &value)
}

#[pymodule]
fn _cello(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    // Core classes
    m.add_class::<Cello>()?;
    m.add_class::<request::Request>()?;
    m.add_class::<response::Response>()?;
    m.add_class::<middleware::session::PySession>()?;

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
    m.add_class::<PyThreadPoolConfig>()?;
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

    // v1.1.0 - MiniJinja Template Engine
    m.add_class::<minijinja_engine::PyMiniJinjaEngine>()?;

    // Rust-native async HTTP client
    m.add_class::<http_client::PyAsyncClient>()?;
    m.add_class::<http_client::PyHttpResponse>()?;

    // v1.3.0 - Native async data layer (real Postgres pool + Redis client)
    m.add_class::<db::PyDatabase>()?;
    m.add_class::<db::PyTransaction>()?;
    m.add_class::<db::PyRedis>()?;
    // v1.4.0 - Database exception hierarchy (SQLSTATE → DatabaseError subclasses)
    db::error::register(_py, m)?;

    // v1.4.0 - ORM schema registry
    m.add_class::<schema::PyModelSpec>()?;
    m.add_class::<schema::PyFieldSpec>()?;
    m.add_function(wrap_pyfunction!(graphql_plan, m)?)?;
    m.add_function(wrap_pyfunction!(schema::python::schema_register, m)?)?;
    m.add_function(wrap_pyfunction!(schema::python::schema_compile_field, m)?)?;
    m.add_function(wrap_pyfunction!(schema::python::schema_get, m)?)?;
    m.add_function(wrap_pyfunction!(schema::python::schema_count, m)?)?;
    // v1.4.0 Phase 04 - alias validation for aggregates / annotations
    m.add_function(wrap_pyfunction!(schema::python::schema_ident, m)?)?;
    // v1.4.0 Phase 05 - experimental `cello migrate` session
    m.add_class::<schema::migrate_conn::PyMigrationConnection>()?;
    schema::native::register(m)?;
    // v1.4.0 Phase 02 - row pipeline (pre-serialized rows, ORM serializer hooks)
    m.add_class::<db::RawJson>()?;
    m.add_function(wrap_pyfunction!(schema::runtime::orm_bind, m)?)?;

    // v0.7.0+ / v0.8.0 - Enterprise & Data Layer Configuration Classes
    m.add_class::<PyOpenTelemetryConfig>()?;
    m.add_class::<PyHealthCheckConfig>()?;
    m.add_class::<PyDatabaseConfig>()?;
    m.add_class::<PyGraphQLConfig>()?;

    // v0.8.0 - Data Layer Configuration Classes
    m.add_class::<PyRedisConfig>()?;

    // v0.9.0 - API Protocol Configuration Classes
    m.add_class::<PyGrpcConfig>()?;
    m.add_class::<PyKafkaConfig>()?;
    m.add_class::<PyRabbitMQConfig>()?;
    m.add_class::<PySqsConfig>()?;

    // v0.10.0 - Advanced Pattern Configuration Classes
    m.add_class::<PyEventSourcingConfig>()?;
    m.add_class::<PyCqrsConfig>()?;
    m.add_class::<PySagaConfig>()?;

    // RFC 7807 error type
    m.add_class::<error::ProblemDetails>()?;

    Ok(())
}
