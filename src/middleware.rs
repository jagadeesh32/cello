//! Middleware system for Cello.
//!
//! Provides a flexible middleware chain for request/response processing.

use parking_lot::RwLock;
use std::sync::Arc;

use crate::request::Request;
use crate::response::Response;

/// Result type for middleware operations.
pub type MiddlewareResult = Result<MiddlewareAction, MiddlewareError>;

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
}

impl MiddlewareError {
    pub fn new(message: &str, status: u16) -> Self {
        MiddlewareError {
            message: message.to_string(),
            status,
        }
    }

    pub fn internal(message: &str) -> Self {
        Self::new(message, 500)
    }

    pub fn bad_request(message: &str) -> Self {
        Self::new(message, 400)
    }

    pub fn unauthorized(message: &str) -> Self {
        Self::new(message, 401)
    }

    pub fn forbidden(message: &str) -> Self {
        Self::new(message, 403)
    }
}

/// Trait for implementing middleware.
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
}

/// A wrapper for middleware with priority.
struct MiddlewareEntry {
    middleware: Arc<dyn Middleware>,
    priority: i32,
}

/// Middleware chain that manages multiple middleware in order.
#[derive(Clone)]
pub struct MiddlewareChain {
    middlewares: Arc<RwLock<Vec<MiddlewareEntry>>>,
}

impl MiddlewareChain {
    /// Create a new empty middleware chain.
    pub fn new() -> Self {
        MiddlewareChain {
            middlewares: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a middleware to the chain.
    pub fn add<M: Middleware + 'static>(&self, middleware: M) {
        let priority = middleware.priority();
        let entry = MiddlewareEntry {
            middleware: Arc::new(middleware),
            priority,
        };

        let mut middlewares = self.middlewares.write();
        middlewares.push(entry);
        middlewares.sort_by_key(|e| e.priority);
    }

    /// Execute all middleware before handlers.
    pub fn execute_before(&self, request: &mut Request) -> MiddlewareResult {
        let middlewares = self.middlewares.read();
        for entry in middlewares.iter() {
            match entry.middleware.before(request)? {
                MiddlewareAction::Continue => continue,
                action @ MiddlewareAction::Stop(_) => return Ok(action),
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    /// Execute all middleware after handlers (in reverse order).
    pub fn execute_after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        let middlewares = self.middlewares.read();
        for entry in middlewares.iter().rev() {
            match entry.middleware.after(request, response)? {
                MiddlewareAction::Continue => continue,
                action @ MiddlewareAction::Stop(_) => return Ok(action),
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    /// Get the number of registered middleware.
    pub fn len(&self) -> usize {
        self.middlewares.read().len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.middlewares.read().is_empty()
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Middleware
// ============================================================================

/// CORS middleware for handling Cross-Origin Resource Sharing.
pub struct CorsMiddleware {
    pub allow_origins: Vec<String>,
    pub allow_methods: Vec<String>,
    pub allow_headers: Vec<String>,
    pub max_age: u32,
}

impl CorsMiddleware {
    pub fn new() -> Self {
        CorsMiddleware {
            allow_origins: vec!["*".to_string()],
            allow_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "PATCH".to_string(),
                "OPTIONS".to_string(),
            ],
            allow_headers: vec!["*".to_string()],
            max_age: 86400,
        }
    }

    pub fn allow_origin(mut self, origin: &str) -> Self {
        self.allow_origins = vec![origin.to_string()];
        self
    }

    pub fn allow_origins(mut self, origins: Vec<&str>) -> Self {
        self.allow_origins = origins.iter().map(|s| s.to_string()).collect();
        self
    }
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for CorsMiddleware {
    fn after(&self, _request: &Request, response: &mut Response) -> MiddlewareResult {
        response.set_header(
            "Access-Control-Allow-Origin",
            &self.allow_origins.join(", "),
        );
        response.set_header(
            "Access-Control-Allow-Methods",
            &self.allow_methods.join(", "),
        );
        response.set_header(
            "Access-Control-Allow-Headers",
            &self.allow_headers.join(", "),
        );
        response.set_header("Access-Control-Max-Age", &self.max_age.to_string());
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -100 // Run early
    }

    fn name(&self) -> &str {
        "cors"
    }
}

/// Logging middleware for request/response logging.
pub struct LoggingMiddleware {
    pub log_body: bool,
}

impl LoggingMiddleware {
    pub fn new() -> Self {
        LoggingMiddleware { log_body: false }
    }

    pub fn with_body(mut self) -> Self {
        self.log_body = true;
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
        // We print the "incoming" request log immediately
        println!(
            "--> {} {}",
            request.method,
            request.path
        );
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
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "",
    }
}

/// Simple timestamp function (avoiding chrono dependency).
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

/// Compression middleware for gzip response compression.
pub struct CompressionMiddleware {
    pub min_size: usize,
}

impl CompressionMiddleware {
    pub fn new() -> Self {
        CompressionMiddleware { min_size: 1024 }
    }

    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
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
            if encoding.contains("gzip") && response.body_bytes().len() >= self.min_size {
                // Compress the response body
                if let Ok(compressed) = compress_gzip(response.body_bytes()) {
                    response.set_body(compressed);
                    response.set_header("Content-Encoding", "gzip");
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
fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
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
    fn test_cors_middleware() {
        let cors = CorsMiddleware::new();
        assert_eq!(cors.name(), "cors");
        assert_eq!(cors.priority(), -100);
    }
}
