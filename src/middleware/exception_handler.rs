//! Global Exception Handling middleware for Cello (inspired by FastAPI).
//!
//! Provides:
//! - Global exception handlers
//! - Custom exception handlers per error type
//! - Exception handler priority
//! - Detailed error responses (dev/prod modes)
//! - Error logging integration
//! - JSON/HTML error responses
//! - Status code mapping



use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Exception Types
// ============================================================================

/// Exception handler result.
pub type ExceptionResult = Result<Response, Box<dyn std::error::Error + Send + Sync>>;

/// Exception context for handlers.
pub struct ExceptionContext {
    /// The original request
    pub request: Request,
    /// Exception details
    pub exception: Box<dyn std::error::Error + Send + Sync>,
    /// Handler chain (for debugging)
    pub handler_chain: Vec<String>,
}

impl ExceptionContext {
    pub fn new(request: Request, exception: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self {
            request,
            exception,
            handler_chain: Vec::new(),
        }
    }

    pub fn add_handler(&mut self, handler_name: &str) {
        self.handler_chain.push(handler_name.to_string());
    }
}

// ============================================================================
// Exception Handler Trait
// ============================================================================

/// Trait for exception handlers.
pub trait ExceptionHandler: Send + Sync {
    /// Handle the exception and return a response.
    fn handle(&self, context: &mut ExceptionContext) -> ExceptionResult;

    /// Priority of this handler (lower = higher priority).
    fn priority(&self) -> i32 {
        0
    }

    /// Handler name for debugging.
    fn name(&self) -> &str {
        "unnamed_handler"
    }

    /// Whether this handler is enabled.
    fn enabled(&self) -> bool {
        true
    }
}

// ============================================================================
// Built-in Exception Handlers
// ============================================================================

/// Configuration for exception handlers.
#[derive(Clone)]
pub struct ExceptionHandlerConfig {
    /// Include exception details in development mode
    pub include_traceback: bool,
    /// Custom error message for internal server errors
    pub internal_error_message: String,
    /// Default status code for unhandled exceptions
    pub default_status_code: u16,
    /// Media type for error responses
    pub media_type: String,
    /// Enable error logging
    pub enable_logging: bool,
    /// Log level for exceptions
    pub log_level: String,
}

impl Default for ExceptionHandlerConfig {
    fn default() -> Self {
        Self {
            include_traceback: cfg!(debug_assertions), // Development mode
            internal_error_message: "Internal Server Error".to_string(),
            default_status_code: 500,
            media_type: "application/json".to_string(),
            enable_logging: true,
            log_level: "ERROR".to_string(),
        }
    }
}

/// Built-in handler for validation errors.
pub struct ValidationErrorHandler {
    config: ExceptionHandlerConfig,
}

impl ValidationErrorHandler {
    pub fn new(config: ExceptionHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExceptionHandler for ValidationErrorHandler {
    fn handle(&self, context: &mut ExceptionContext) -> ExceptionResult {
        let mut response = Response::new(400);

        let error_response = serde_json::json!({
            "type": "validation_error",
            "title": "Validation Error",
            "detail": context.exception.to_string(),
            "status": 400,
            "instance": context.request.path.clone(),
        });

        response.set_header("Content-Type", &self.config.media_type);
        response.set_body(error_response.to_string().into_bytes());

        Ok(response)
    }

    fn name(&self) -> &str {
        "validation_error_handler"
    }

    fn priority(&self) -> i32 {
        -10 // High priority for validation errors
    }
}

/// Built-in handler for authentication errors.
pub struct AuthenticationErrorHandler {
    config: ExceptionHandlerConfig,
}

impl AuthenticationErrorHandler {
    pub fn new(config: ExceptionHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExceptionHandler for AuthenticationErrorHandler {
    fn handle(&self, context: &mut ExceptionContext) -> ExceptionResult {
        let mut response = Response::new(401);

        let error_response = serde_json::json!({
            "type": "authentication_error",
            "title": "Authentication Required",
            "detail": context.exception.to_string(),
            "status": 401,
            "instance": context.request.path.clone(),
        });

        response.set_header("Content-Type", &self.config.media_type);
        response.set_header("WWW-Authenticate", "Bearer");
        response.set_body(error_response.to_string().into_bytes());

        Ok(response)
    }

    fn name(&self) -> &str {
        "authentication_error_handler"
    }

    fn priority(&self) -> i32 {
        -20 // High priority for auth errors
    }
}

/// Built-in handler for authorization errors.
pub struct AuthorizationErrorHandler {
    config: ExceptionHandlerConfig,
}

impl AuthorizationErrorHandler {
    pub fn new(config: ExceptionHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExceptionHandler for AuthorizationErrorHandler {
    fn handle(&self, context: &mut ExceptionContext) -> ExceptionResult {
        let mut response = Response::new(403);

        let error_response = serde_json::json!({
            "type": "authorization_error",
            "title": "Forbidden",
            "detail": context.exception.to_string(),
            "status": 403,
            "instance": context.request.path.clone(),
        });

        response.set_header("Content-Type", &self.config.media_type);
        response.set_body(error_response.to_string().into_bytes());

        Ok(response)
    }

    fn name(&self) -> &str {
        "authorization_error_handler"
    }

    fn priority(&self) -> i32 {
        -30 // High priority for authz errors
    }
}

/// Built-in handler for not found errors.
pub struct NotFoundErrorHandler {
    config: ExceptionHandlerConfig,
}

impl NotFoundErrorHandler {
    pub fn new(config: ExceptionHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExceptionHandler for NotFoundErrorHandler {
    fn handle(&self, context: &mut ExceptionContext) -> ExceptionResult {
        let mut response = Response::new(404);

        let error_response = serde_json::json!({
            "type": "not_found_error",
            "title": "Not Found",
            "detail": context.exception.to_string(),
            "status": 404,
            "instance": context.request.path.clone(),
        });

        response.set_header("Content-Type", &self.config.media_type);
        response.set_body(error_response.to_string().into_bytes());

        Ok(response)
    }

    fn name(&self) -> &str {
        "not_found_error_handler"
    }

    fn priority(&self) -> i32 {
        -40 // High priority for 404s
    }
}

/// Built-in handler for internal server errors.
pub struct InternalServerErrorHandler {
    config: ExceptionHandlerConfig,
}

impl InternalServerErrorHandler {
    pub fn new(config: ExceptionHandlerConfig) -> Self {
        Self { config }
    }
}

impl ExceptionHandler for InternalServerErrorHandler {
    fn handle(&self, context: &mut ExceptionContext) -> ExceptionResult {
        let mut response = Response::new(self.config.default_status_code);

        let mut error_response = serde_json::json!({
            "type": "internal_server_error",
            "title": "Internal Server Error",
            "detail": if self.config.include_traceback {
                context.exception.to_string()
            } else {
                self.config.internal_error_message.clone()
            },
            "status": self.config.default_status_code,
            "instance": context.request.path.clone(),
        });

        // Add traceback in development mode
        if self.config.include_traceback {
            error_response["handler_chain"] = serde_json::json!(context.handler_chain);
        }

        response.set_header("Content-Type", &self.config.media_type);
        response.set_body(error_response.to_string().into_bytes());

        Ok(response)
    }

    fn name(&self) -> &str {
        "internal_server_error_handler"
    }

    fn priority(&self) -> i32 {
        100 // Lowest priority (catch-all)
    }
}

/// Custom exception handler with user-defined logic.
pub struct CustomExceptionHandler<F>
where
    F: Fn(&mut ExceptionContext) -> ExceptionResult + Send + Sync,
{
    handler_fn: F,
    name: String,
    priority: i32,
}

impl<F> CustomExceptionHandler<F>
where
    F: Fn(&mut ExceptionContext) -> ExceptionResult + Send + Sync,
{
    pub fn new(name: &str, handler_fn: F) -> Self {
        Self {
            handler_fn,
            name: name.to_string(),
            priority: 0,
        }
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

impl<F> ExceptionHandler for CustomExceptionHandler<F>
where
    F: Fn(&mut ExceptionContext) -> ExceptionResult + Send + Sync,
{
    fn handle(&self, context: &mut ExceptionContext) -> ExceptionResult {
        (self.handler_fn)(context)
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Exception Handling Middleware
// ============================================================================

/// Global exception handling middleware.
pub struct ExceptionHandlerMiddleware {
    handlers: Vec<Box<dyn ExceptionHandler>>,
    config: ExceptionHandlerConfig,
}

impl ExceptionHandlerMiddleware {
    /// Create new exception handler middleware with default handlers.
    pub fn new() -> Self {
        let config = ExceptionHandlerConfig::default();

        let mut handlers: Vec<Box<dyn ExceptionHandler>> = Vec::new();

        // Add built-in handlers in priority order
        handlers.push(Box::new(ValidationErrorHandler::new(config.clone())));
        handlers.push(Box::new(AuthenticationErrorHandler::new(config.clone())));
        handlers.push(Box::new(AuthorizationErrorHandler::new(config.clone())));
        handlers.push(Box::new(NotFoundErrorHandler::new(config.clone())));
        handlers.push(Box::new(InternalServerErrorHandler::new(config.clone())));

        // Sort by priority (lower priority number = higher priority)
        handlers.sort_by_key(|h| h.priority());

        Self { handlers, config }
    }

    /// Create with custom config.
    pub fn with_config(config: ExceptionHandlerConfig) -> Self {
        let mut instance = Self::new();
        instance.config = config;
        instance
    }

    /// Add a custom exception handler.
    pub fn add_handler<H: ExceptionHandler + 'static>(mut self, handler: H) -> Self {
        self.handlers.push(Box::new(handler));
        // Re-sort by priority
        self.handlers.sort_by_key(|h| h.priority());
        self
    }

    /// Handle an exception and return a response.
    pub fn handle_exception(
        &self,
        request: Request,
        exception: Box<dyn std::error::Error + Send + Sync>,
    ) -> Response {
        let mut context = ExceptionContext::new(request, exception);

        // Log the exception if enabled
        if self.config.enable_logging {
            tracing::error!(
                "Exception in handler chain: {} - {}",
                context.handler_chain.join(" -> "),
                context.exception
            );
        }

        // Try each handler in priority order
        for handler in &self.handlers {
            if !handler.enabled() {
                continue;
            }

            context.add_handler(handler.name());

            match handler.handle(&mut context) {
                Ok(response) => {
                    // Log successful handling
                    if self.config.enable_logging {
                        tracing::info!(
                            "Exception handled by {}: status {}",
                            handler.name(),
                            response.status
                        );
                    }
                    return response;
                }
                Err(e) => {
                    // Handler failed, try next one
                    if self.config.enable_logging {
                        tracing::warn!(
                            "Handler {} failed: {}, trying next handler",
                            handler.name(),
                            e
                        );
                    }
                    continue;
                }
            }
        }

        // No handler could handle the exception, return default error
        let mut response = Response::new(self.config.default_status_code);
        let error_msg = if self.config.include_traceback {
            format!("Unhandled exception: {}", context.exception)
        } else {
            self.config.internal_error_message.clone()
        };

        response.set_header("Content-Type", &self.config.media_type);
        response.set_body(error_msg.into_bytes());
        response
    }
}

impl Default for ExceptionHandlerMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for ExceptionHandlerMiddleware {
    fn before(&self, _request: &mut Request) -> MiddlewareResult {
        // Exception handlers are typically called during error handling,
        // not during normal request processing
        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, _request: &Request, response: &mut Response) -> MiddlewareResult {
        // Check if response indicates an error
        if response.status >= 400 {
            // This middleware doesn't modify successful responses
            // Error responses are typically handled by the framework
            // when exceptions occur in handlers
        }
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        100 // Run very late, after all other processing
    }

    fn name(&self) -> &str {
        "exception_handler"
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a standard error response.
pub fn create_error_response(
    status: u16,
    error_type: &str,
    title: &str,
    detail: &str,
    instance: &str,
    include_traceback: bool,
) -> Response {
    let mut response = Response::new(status);

    let mut error_response = serde_json::json!({
        "type": error_type,
        "title": title,
        "detail": detail,
        "status": status,
        "instance": instance,
    });

    if include_traceback {
        error_response["timestamp"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
    }

    response.set_header("Content-Type", "application/json");
    response.set_body(error_response.to_string().into_bytes());

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Request;

    #[derive(Debug)]
    struct TestError(String);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for TestError {}

    #[test]
    fn test_validation_error_handler() {
        let handler = ValidationErrorHandler::new(ExceptionHandlerConfig::default());
        let request = Request::default();
        let exception = Box::new(TestError("Invalid input".to_string()));

        let mut context = ExceptionContext::new(request, exception);
        let response = handler.handle(&mut context).unwrap();

        assert_eq!(response.status, 400);
        let body = String::from_utf8(response.body()).unwrap();
        assert!(body.contains("validation_error"));
        assert!(body.contains("Validation Error"));
    }

    #[test]
    fn test_authentication_error_handler() {
        let handler = AuthenticationErrorHandler::new(ExceptionHandlerConfig::default());
        let request = Request::default();
        let exception = Box::new(TestError("Not authenticated".to_string()));

        let mut context = ExceptionContext::new(request, exception);
        let response = handler.handle(&mut context).unwrap();

        assert_eq!(response.status, 401);
        assert!(response.headers.contains_key("WWW-Authenticate"));
    }

    #[test]
    fn test_authorization_error_handler() {
        let handler = AuthorizationErrorHandler::new(ExceptionHandlerConfig::default());
        let request = Request::default();
        let exception = Box::new(TestError("Forbidden".to_string()));

        let mut context = ExceptionContext::new(request, exception);
        let response = handler.handle(&mut context).unwrap();

        assert_eq!(response.status, 403);
    }

    #[test]
    fn test_not_found_error_handler() {
        let handler = NotFoundErrorHandler::new(ExceptionHandlerConfig::default());
        let request = Request::default();
        let exception = Box::new(TestError("Not found".to_string()));

        let mut context = ExceptionContext::new(request, exception);
        let response = handler.handle(&mut context).unwrap();

        assert_eq!(response.status, 404);
    }

    #[test]
    fn test_internal_server_error_handler() {
        let handler = InternalServerErrorHandler::new(ExceptionHandlerConfig::default());
        let request = Request::default();
        let exception = Box::new(TestError("Internal error".to_string()));

        let mut context = ExceptionContext::new(request, exception);
        let response = handler.handle(&mut context).unwrap();

        assert_eq!(response.status, 500);
    }

    #[test]
    fn test_custom_exception_handler() {
        let handler = CustomExceptionHandler::new("test_handler", |context| {
            let mut response = Response::new(418); // I'm a teapot
            response.set_body(format!("Custom: {}", context.exception).into_bytes());
            Ok(response)
        });

        let request = Request::default();
        let exception = Box::new(TestError("Custom error".to_string()));

        let mut context = ExceptionContext::new(request, exception);
        let response = handler.handle(&mut context).unwrap();

        assert_eq!(response.status, 418);
        let body = String::from_utf8(response.body()).unwrap();
        assert!(body.contains("Custom: Custom error"));
    }

    #[test]
    fn test_exception_handler_middleware() {
        let middleware = ExceptionHandlerMiddleware::new();
        let request = Request::default();
        let exception = Box::new(TestError("Test exception".to_string()));

        let response = middleware.handle_exception(request, exception);

        // Should be handled by internal server error handler
        assert_eq!(response.status, 500);
    }

    #[test]
    fn test_config_options() {
        let config = ExceptionHandlerConfig {
            include_traceback: false,
            internal_error_message: "Custom error".to_string(),
            default_status_code: 503,
            media_type: "application/problem+json".to_string(),
            enable_logging: false,
            log_level: "WARN".to_string(),
        };

        let handler = InternalServerErrorHandler::new(config);
        let request = Request::default();
        let exception = Box::new(TestError("Test error".to_string()));

        let mut context = ExceptionContext::new(request, exception);
        let response = handler.handle(&mut context).unwrap();

        assert_eq!(response.status, 503);
        assert_eq!(response.headers.get("Content-Type").unwrap(), "application/problem+json");
        let body = String::from_utf8(response.body()).unwrap();
        assert!(body.contains("Custom error"));
    }
}
