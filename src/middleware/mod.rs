//! Advanced middleware system for Cello.
//!
//! This module provides:
//! - Core middleware trait and chain
//! - Authentication (JWT, Basic, API Key)
//! - Rate limiting (Token bucket, Sliding window)
//! - Session management (Cookie, Redis)
//! - Static file serving
//! - Security headers (CSP, HSTS, etc.)
//! - Request validation (Body limit, CSRF)
//! - Request tracking (Request ID, ETag)

pub mod auth;
pub mod body_limit;
pub mod cache;
pub mod cors;
pub mod csrf;
pub mod etag;
pub mod exception_handler;
pub mod guards;
pub mod prometheus;
pub mod rate_limit;
pub mod request_id;
pub mod security;
pub mod session;
pub mod static_files;

// Re-export DTO types from the dto module
pub use crate::dto::*;

use parking_lot::RwLock;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::request::Request;
use crate::response::Response;

// Re-export all middleware types
pub use auth::{ApiKeyAuth, BasicAuth, JwtAuth};
pub use body_limit::BodyLimitMiddleware;
pub use cache::{CacheMiddleware, CacheConfig, CacheStore, CachedResponse, CacheError, CacheKeyBuilder, DefaultCacheKeyBuilder, InMemoryCacheStore, create_cache_key};
pub use cors::CorsMiddleware;
pub use csrf::CsrfMiddleware;
pub use etag::EtagMiddleware;
pub use exception_handler::{ExceptionHandler, ExceptionHandlerMiddleware, ExceptionHandlerConfig, ExceptionContext, ValidationErrorHandler, AuthenticationErrorHandler, AuthorizationErrorHandler, NotFoundErrorHandler, InternalServerErrorHandler, CustomExceptionHandler};
pub use guards::{Guard, GuardsMiddleware, RoleGuard, PermissionGuard, AuthenticatedGuard, AndGuard, OrGuard, NotGuard, CustomGuard};
pub use prometheus::{PrometheusMiddleware, PrometheusConfig, PrometheusMetrics};
pub use rate_limit::{RateLimitMiddleware, RateLimitStore, TokenBucketConfig, SlidingWindowConfig};
pub use request_id::RequestIdMiddleware;
pub use security::{SecurityHeadersMiddleware, ContentSecurityPolicy, HstsConfig};
pub use session::{SessionMiddleware, SessionStore, InMemorySessionStore};
pub use static_files::StaticFilesMiddleware;

// ============================================================================
// Core Middleware Types
// ============================================================================

/// Result type for middleware operations.
pub type MiddlewareResult = Result<MiddlewareAction, MiddlewareError>;

/// Async middleware result.
pub type AsyncMiddlewareResult = Pin<Box<dyn Future<Output = MiddlewareResult> + Send>>;

/// Action to take after middleware execution.
#[derive(Debug, Clone)]
pub enum MiddlewareAction {
    /// Continue to next middleware/handler
    Continue,
    /// Stop processing and return response immediately
    Stop(Response),
}

/// Middleware error type.
#[derive(Debug, Clone)]
pub struct MiddlewareError {
    pub message: String,
    pub status: u16,
    pub code: Option<String>,
}

impl MiddlewareError {
    pub fn new(message: &str, status: u16) -> Self {
        MiddlewareError {
            message: message.to_string(),
            status,
            code: None,
        }
    }

    pub fn with_code(mut self, code: &str) -> Self {
        self.code = Some(code.to_string());
        self
    }

    pub fn internal(message: &str) -> Self {
        Self::new(message, 500).with_code("INTERNAL_ERROR")
    }

    pub fn bad_request(message: &str) -> Self {
        Self::new(message, 400).with_code("BAD_REQUEST")
    }

    pub fn unauthorized(message: &str) -> Self {
        Self::new(message, 401).with_code("UNAUTHORIZED")
    }

    pub fn forbidden(message: &str) -> Self {
        Self::new(message, 403).with_code("FORBIDDEN")
    }

    pub fn not_found(message: &str) -> Self {
        Self::new(message, 404).with_code("NOT_FOUND")
    }

    pub fn too_many_requests(message: &str) -> Self {
        Self::new(message, 429).with_code("TOO_MANY_REQUESTS")
    }

    pub fn payload_too_large(message: &str) -> Self {
        Self::new(message, 413).with_code("PAYLOAD_TOO_LARGE")
    }
}

impl std::fmt::Display for MiddlewareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = &self.code {
            write!(f, "[{}] {}", code, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for MiddlewareError {}

/// Trait for implementing synchronous middleware.
pub trait Middleware: Send + Sync {
    /// Called before the request is handled.
    fn before(&self, _request: &mut Request) -> MiddlewareResult {
        Ok(MiddlewareAction::Continue)
    }

    /// Called after the response is generated.
    fn after(&self, _request: &Request, _response: &mut Response) -> MiddlewareResult {
        Ok(MiddlewareAction::Continue)
    }

    /// Middleware priority (lower = runs first).
    fn priority(&self) -> i32 {
        0
    }

    /// Middleware name for debugging.
    fn name(&self) -> &str {
        "unnamed"
    }

    /// Whether this middleware should run for a given path.
    fn should_run(&self, _path: &str) -> bool {
        true
    }

    /// Get route patterns to skip (if any).
    fn skip_paths(&self) -> &[String] {
        &[]
    }
}

/// Trait for implementing asynchronous middleware.
pub trait AsyncMiddleware: Send + Sync {
    /// Called before the request is handled (async).
    fn before_async<'a>(
        &'a self,
        request: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = MiddlewareResult> + Send + 'a>> {
        Box::pin(async move {
            let _ = request;
            Ok(MiddlewareAction::Continue)
        })
    }

    /// Called after the response is generated (async).
    fn after_async<'a>(
        &'a self,
        request: &'a Request,
        response: &'a mut Response,
    ) -> Pin<Box<dyn Future<Output = MiddlewareResult> + Send + 'a>> {
        Box::pin(async move {
            let _ = (request, response);
            Ok(MiddlewareAction::Continue)
        })
    }

    /// Middleware priority (lower = runs first).
    fn priority(&self) -> i32 {
        0
    }

    /// Middleware name for debugging.
    fn name(&self) -> &str {
        "unnamed_async"
    }
}

/// A wrapper for middleware with priority.
struct MiddlewareEntry {
    middleware: Arc<dyn Middleware>,
    priority: i32,
}

/// A wrapper for async middleware with priority.
#[derive(Clone)]
struct AsyncMiddlewareEntry {
    middleware: Arc<dyn AsyncMiddleware>,
    priority: i32,
}

/// Middleware chain that manages multiple middleware in order.
#[derive(Clone)]
pub struct MiddlewareChain {
    sync_middlewares: Arc<RwLock<Vec<MiddlewareEntry>>>,
    async_middlewares: Arc<RwLock<Vec<AsyncMiddlewareEntry>>>,
}

impl MiddlewareChain {
    /// Create a new empty middleware chain.
    pub fn new() -> Self {
        MiddlewareChain {
            sync_middlewares: Arc::new(RwLock::new(Vec::new())),
            async_middlewares: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a synchronous middleware to the chain.
    pub fn add<M: Middleware + 'static>(&self, middleware: M) {
        let priority = middleware.priority();
        let entry = MiddlewareEntry {
            middleware: Arc::new(middleware),
            priority,
        };

        let mut middlewares = self.sync_middlewares.write();
        middlewares.push(entry);
        middlewares.sort_by_key(|e| e.priority);
    }

    /// Add an asynchronous middleware to the chain.
    pub fn add_async<M: AsyncMiddleware + 'static>(&self, middleware: M) {
        let priority = middleware.priority();
        let entry = AsyncMiddlewareEntry {
            middleware: Arc::new(middleware),
            priority,
        };

        let mut middlewares = self.async_middlewares.write();
        middlewares.push(entry);
        middlewares.sort_by_key(|e| e.priority);
    }

    /// Execute all sync middleware before handlers.
    pub fn execute_before(&self, request: &mut Request) -> MiddlewareResult {
        let middlewares = self.sync_middlewares.read();
        for entry in middlewares.iter() {
            if !entry.middleware.should_run(&request.path) {
                continue;
            }
            match entry.middleware.before(request)? {
                MiddlewareAction::Continue => continue,
                action @ MiddlewareAction::Stop(_) => return Ok(action),
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    /// Execute all sync middleware after handlers (in reverse order).
    pub fn execute_after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        let middlewares = self.sync_middlewares.read();
        for entry in middlewares.iter().rev() {
            if !entry.middleware.should_run(&request.path) {
                continue;
            }
            match entry.middleware.after(request, response)? {
                MiddlewareAction::Continue => continue,
                action @ MiddlewareAction::Stop(_) => return Ok(action),
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    /// Execute all async middleware before handlers.
    pub async fn execute_before_async(&self, request: &mut Request) -> MiddlewareResult {
        let middlewares = self.async_middlewares.read().clone();
        for entry in middlewares.iter() {
            match entry.middleware.before_async(request).await? {
                MiddlewareAction::Continue => continue,
                action @ MiddlewareAction::Stop(_) => return Ok(action),
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    /// Execute all async middleware after handlers (in reverse order).
    pub async fn execute_after_async(
        &self,
        request: &Request,
        response: &mut Response,
    ) -> MiddlewareResult {
        let middlewares = self.async_middlewares.read().clone();
        for entry in middlewares.iter().rev() {
            match entry.middleware.after_async(request, response).await? {
                MiddlewareAction::Continue => continue,
                action @ MiddlewareAction::Stop(_) => return Ok(action),
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    /// Get the number of registered sync middleware.
    pub fn len(&self) -> usize {
        self.sync_middlewares.read().len()
    }

    /// Get the number of registered async middleware.
    pub fn async_len(&self) -> usize {
        self.async_middlewares.read().len()
    }

    /// Check if the chain has no sync middleware.
    pub fn is_empty(&self) -> bool {
        self.sync_middlewares.read().is_empty()
    }

    /// Check if the chain has no async middleware.
    pub fn is_async_empty(&self) -> bool {
        self.async_middlewares.read().is_empty()
    }

    /// Get names of all registered middleware.
    pub fn middleware_names(&self) -> Vec<String> {
        let sync_names: Vec<String> = self
            .sync_middlewares
            .read()
            .iter()
            .map(|e| e.middleware.name().to_string())
            .collect();
        let async_names: Vec<String> = self
            .async_middlewares
            .read()
            .iter()
            .map(|e| e.middleware.name().to_string())
            .collect();
        [sync_names, async_names].concat()
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Logging Middleware
// ============================================================================

/// Logging middleware for request/response logging.
pub struct LoggingMiddleware {
    pub log_body: bool,
    pub log_headers: bool,
    pub skip_paths: Vec<String>,
}

impl LoggingMiddleware {
    pub fn new() -> Self {
        LoggingMiddleware {
            log_body: false,
            log_headers: false,
            skip_paths: vec!["/health".to_string(), "/metrics".to_string()],
        }
    }

    pub fn with_body(mut self) -> Self {
        self.log_body = true;
        self
    }

    pub fn with_headers(mut self) -> Self {
        self.log_headers = true;
        self
    }

    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }
}

impl Default for LoggingMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for LoggingMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        println!("--> {} {}", request.method, request.path);
        if self.log_headers {
            for (key, value) in &request.headers {
                println!("    {}: {}", key, value);
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        println!(
            "<-- {} {} {} {}",
            request.method,
            request.path,
            response.status,
            status_text(response.status)
        );
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -200 // Run very early
    }

    fn name(&self) -> &str {
        "logging"
    }

    fn should_run(&self, path: &str) -> bool {
        !self.skip_paths.iter().any(|p| path.starts_with(p))
    }
}

/// Get status text for HTTP status code.
pub fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

// ============================================================================
// Compression Middleware
// ============================================================================

/// Compression middleware for gzip response compression.
pub struct CompressionMiddleware {
    pub min_size: usize,
    pub compression_level: u32,
    pub content_types: Vec<String>,
}

impl CompressionMiddleware {
    pub fn new() -> Self {
        CompressionMiddleware {
            min_size: 1024,
            compression_level: 6,
            content_types: vec![
                "text/html".to_string(),
                "text/css".to_string(),
                "text/javascript".to_string(),
                "application/javascript".to_string(),
                "application/json".to_string(),
                "application/xml".to_string(),
                "text/xml".to_string(),
                "text/plain".to_string(),
            ],
        }
    }

    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
    }

    pub fn compression_level(mut self, level: u32) -> Self {
        self.compression_level = level.min(9);
        self
    }

    pub fn add_content_type(mut self, content_type: &str) -> Self {
        self.content_types.push(content_type.to_string());
        self
    }

    fn should_compress(&self, response: &Response) -> bool {
        if response.body_bytes().len() < self.min_size {
            return false;
        }

        // Check if content type is compressible
        if let Some(content_type) = response.headers.get("Content-Type") {
            return self
                .content_types
                .iter()
                .any(|ct| content_type.starts_with(ct));
        }

        false
    }
}

impl Default for CompressionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for CompressionMiddleware {
    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Check if client accepts gzip
        let accept_encoding = request.headers.get("accept-encoding").map(|s| s.as_str());
        if let Some(encoding) = accept_encoding {
            if encoding.contains("gzip") && self.should_compress(response) {
                // Compress the response body
                if let Ok(compressed) = compress_gzip(response.body_bytes(), self.compression_level)
                {
                    response.set_body(compressed);
                    response.set_header("Content-Encoding", "gzip");
                    response.set_header("Vary", "Accept-Encoding");
                }
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        100 // Run late
    }

    fn name(&self) -> &str {
        "compression"
    }
}

/// Compress bytes using gzip.
fn compress_gzip(data: &[u8], level: u32) -> Result<Vec<u8>, std::io::Error> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(data)?;
    encoder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestMiddleware {
        name: String,
        priority: i32,
    }

    impl Middleware for TestMiddleware {
        fn priority(&self) -> i32 {
            self.priority
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_middleware_chain_ordering() {
        let chain = MiddlewareChain::new();

        chain.add(TestMiddleware {
            name: "second".to_string(),
            priority: 10,
        });
        chain.add(TestMiddleware {
            name: "first".to_string(),
            priority: 5,
        });
        chain.add(TestMiddleware {
            name: "third".to_string(),
            priority: 15,
        });

        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn test_middleware_error() {
        let err = MiddlewareError::unauthorized("Invalid token");
        assert_eq!(err.status, 401);
        assert_eq!(err.code, Some("UNAUTHORIZED".to_string()));
    }

    #[test]
    fn test_middleware_names() {
        let chain = MiddlewareChain::new();
        chain.add(TestMiddleware {
            name: "test1".to_string(),
            priority: 0,
        });
        chain.add(TestMiddleware {
            name: "test2".to_string(),
            priority: 1,
        });

        let names = chain.middleware_names();
        assert!(names.contains(&"test1".to_string()));
        assert!(names.contains(&"test2".to_string()));
    }
}
