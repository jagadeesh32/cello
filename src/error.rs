//! Advanced error handling for Cello.
//!
//! This module provides:
//! - RFC 7807 Problem Details for structured errors
//! - Error handler registry (global, status-based, exception-based)
//! - Blueprint-scoped error handlers
//! - Python exception capture with traceback

use parking_lot::RwLock;
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use crate::request::Request;
use crate::response::Response;

/// RFC 7807 Problem Details structure.
///
/// This provides a standardized format for HTTP error responses.
/// See: <https://www.rfc-editor.org/rfc/rfc7807>
#[derive(Debug, Clone, Serialize, Deserialize)]
#[pyclass]
pub struct ProblemDetails {
    /// URI reference identifying the problem type.
    #[pyo3(get, set)]
    #[serde(rename = "type")]
    pub type_uri: String,

    /// Short, human-readable summary of the problem.
    #[pyo3(get, set)]
    pub title: String,

    /// HTTP status code.
    #[pyo3(get, set)]
    pub status: u16,

    /// Human-readable explanation specific to this occurrence.
    #[pyo3(get, set)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// URI reference for the specific occurrence.
    #[pyo3(get, set)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,

    /// Additional problem-specific members (not exposed to Python).
    #[serde(flatten)]
    pub extensions: HashMap<String, JsonValue>,
}

#[pymethods]
impl ProblemDetails {
    #[new]
    #[pyo3(signature = (type_uri, title, status, detail=None, instance=None))]
    pub fn new(
        type_uri: String,
        title: String,
        status: u16,
        detail: Option<String>,
        instance: Option<String>,
    ) -> Self {
        Self {
            type_uri,
            title,
            status,
            detail,
            instance,
            extensions: HashMap::new(),
        }
    }

    /// Add an extension field (accepts string value).
    pub fn with_extension(&mut self, key: String, value: String) {
        self.extensions.insert(key, JsonValue::String(value));
    }

    /// Convert to JSON string.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Convert to Response.
    pub fn to_response(&self) -> Response {
        let body = serde_json::to_vec(self).unwrap_or_default();
        let mut response = Response::new(self.status);
        response.set_body(body);
        response.set_header("Content-Type", "application/problem+json");
        response
    }
}

impl Default for ProblemDetails {
    fn default() -> Self {
        Self {
            type_uri: "about:blank".to_string(),
            title: "Internal Server Error".to_string(),
            status: 500,
            detail: None,
            instance: None,
            extensions: HashMap::new(),
        }
    }
}

/// Application error types.
#[derive(Error, Debug, Clone)]
pub enum AppError {
    #[error("Validation error: {message}")]
    Validation { message: String, errors: Vec<FieldError> },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limited")]
    RateLimited {
        retry_after: Option<u64>,
        message: String,
    },

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Handler error: {0}")]
    Handler(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Python exception: {}: {}", .0.exception_type, .0.message)]
    PythonException(PythonExceptionInfo),

    #[error("{message}")]
    Custom {
        status: u16,
        message: String,
        type_uri: Option<String>,
    },
}

impl AppError {
    /// Get the HTTP status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::Validation { .. } => 422,
            AppError::NotFound(_) => 404,
            AppError::Unauthorized(_) => 401,
            AppError::Forbidden(_) => 403,
            AppError::BadRequest(_) => 400,
            AppError::Conflict(_) => 409,
            AppError::RateLimited { .. } => 429,
            AppError::Timeout(_) => 504,
            AppError::Handler(_) => 500,
            AppError::Internal(_) => 500,
            AppError::PythonException(_) => 500,
            AppError::Custom { status, .. } => *status,
        }
    }

    /// Convert to ProblemDetails.
    pub fn to_problem_details(&self) -> ProblemDetails {
        let (type_uri, title) = match self {
            AppError::Validation { .. } => ("/errors/validation", "Validation Error"),
            AppError::NotFound(_) => ("/errors/not-found", "Not Found"),
            AppError::Unauthorized(_) => ("/errors/unauthorized", "Unauthorized"),
            AppError::Forbidden(_) => ("/errors/forbidden", "Forbidden"),
            AppError::BadRequest(_) => ("/errors/bad-request", "Bad Request"),
            AppError::Conflict(_) => ("/errors/conflict", "Conflict"),
            AppError::RateLimited { .. } => ("/errors/rate-limited", "Too Many Requests"),
            AppError::Timeout(_) => ("/errors/timeout", "Gateway Timeout"),
            AppError::Handler(_) => ("/errors/handler-error", "Handler Error"),
            AppError::Internal(_) => ("/errors/internal", "Internal Server Error"),
            AppError::PythonException(_) => ("/errors/python-exception", "Internal Server Error"),
            AppError::Custom { type_uri, .. } => {
                (type_uri.as_deref().unwrap_or("/errors/custom"), "Error")
            }
        };

        let mut details = ProblemDetails {
            type_uri: type_uri.to_string(),
            title: title.to_string(),
            status: self.status_code(),
            detail: Some(self.to_string()),
            instance: None,
            extensions: HashMap::new(),
        };

        // Add error-specific extensions
        match self {
            AppError::Validation { errors, .. } => {
                details.extensions.insert(
                    "errors".to_string(),
                    serde_json::to_value(errors).unwrap_or_default(),
                );
            }
            AppError::RateLimited { retry_after, .. } => {
                if let Some(seconds) = retry_after {
                    details
                        .extensions
                        .insert("retry_after".to_string(), serde_json::json!(seconds));
                }
            }
            AppError::PythonException(info) => {
                details.extensions.insert(
                    "exception_type".to_string(),
                    serde_json::json!(info.exception_type),
                );
            }
            _ => {}
        }

        details
    }

    /// Convert to Response.
    pub fn to_response(&self) -> Response {
        self.to_problem_details().to_response()
    }
}

/// Field-level validation error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldError {
    /// Field name or path.
    pub field: String,
    /// Error message.
    pub message: String,
    /// Error code for programmatic handling.
    pub code: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            code: code.into(),
        }
    }
}

/// Python exception information with traceback.
#[derive(Debug, Clone)]
pub struct PythonExceptionInfo {
    /// Exception type name (e.g., "ValueError").
    pub exception_type: String,
    /// Exception message.
    pub message: String,
    /// Full traceback (if available).
    pub traceback: Option<String>,
    /// Source file (if available).
    pub file: Option<String>,
    /// Line number (if available).
    pub line: Option<u32>,
}

impl PythonExceptionInfo {
    /// Create from a PyErr.
    pub fn from_pyerr(py: Python<'_>, err: &PyErr) -> Self {
        let exception_type = err
            .get_type(py)
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());

        let message = err.to_string();

        let traceback = err.traceback(py).and_then(|tb| tb.format().ok());

        Self {
            exception_type,
            message,
            traceback,
            file: None,
            line: None,
        }
    }
}

impl std::fmt::Display for PythonExceptionInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.exception_type, self.message)
    }
}

/// Error handler function type.
pub type ErrorHandlerFn = Arc<dyn Fn(&AppError, &Request) -> Response + Send + Sync>;

/// Python error handler wrapper.
pub struct PyErrorHandler {
    handler: PyObject,
}

impl PyErrorHandler {
    pub fn new(handler: PyObject) -> Self {
        Self { handler }
    }

    pub fn handle(&self, error: &AppError, request: &Request) -> Response {
        Python::with_gil(|py| {
            // Create error info dict for Python
            let error_dict = pyo3::types::PyDict::new(py);
            let _ = error_dict.set_item("message", error.to_string());
            let _ = error_dict.set_item("status", error.status_code());
            let _ = error_dict.set_item("type", format!("{:?}", std::mem::discriminant(error)));

            if let AppError::PythonException(info) = error {
                let _ = error_dict.set_item("exception_type", &info.exception_type);
                if let Some(tb) = &info.traceback {
                    let _ = error_dict.set_item("traceback", tb);
                }
            }

            match self.handler.call1(py, (error_dict, request.clone())) {
                Ok(result) => {
                    // Check if result is a Response
                    if let Ok(response) = result.extract::<Response>(py) {
                        return response;
                    }
                    // Check if result is a ProblemDetails
                    if let Ok(problem) = result.extract::<ProblemDetails>(py) {
                        return problem.to_response();
                    }
                    // Fallback: try to convert result to string
                    if let Ok(s) = result.extract::<String>(py) {
                        let mut resp = Response::new(error.status_code());
                        resp.set_body(s.into_bytes());
                        resp.set_header("Content-Type", "application/json");
                        return resp;
                    }
                    // Fallback to default error response
                    error.to_response()
                }
                Err(_) => error.to_response(),
            }
        })
    }
}

/// Error handler registry.
///
/// Manages error handlers at different levels:
/// - Global handlers
/// - Status-code specific handlers
/// - Exception-type specific handlers
/// - Blueprint-scoped handlers
pub struct ErrorHandlerRegistry {
    /// Global error handler.
    global: RwLock<Option<Arc<PyErrorHandler>>>,
    /// Per-status code handlers.
    status_handlers: RwLock<HashMap<u16, Arc<PyErrorHandler>>>,
    /// Per-exception type handlers (by type name string).
    exception_handlers: RwLock<HashMap<String, Arc<PyErrorHandler>>>,
    /// Blueprint-scoped handler registries.
    blueprint_handlers: RwLock<HashMap<String, Arc<ErrorHandlerRegistry>>>,
    /// Debug mode flag (shows tracebacks).
    debug_mode: RwLock<bool>,
}

impl ErrorHandlerRegistry {
    /// Create a new error handler registry.
    pub fn new() -> Self {
        Self {
            global: RwLock::new(None),
            status_handlers: RwLock::new(HashMap::new()),
            exception_handlers: RwLock::new(HashMap::new()),
            blueprint_handlers: RwLock::new(HashMap::new()),
            debug_mode: RwLock::new(false),
        }
    }

    /// Set debug mode.
    pub fn set_debug(&self, debug: bool) {
        *self.debug_mode.write() = debug;
    }

    /// Check if debug mode is enabled.
    pub fn is_debug(&self) -> bool {
        *self.debug_mode.read()
    }

    /// Register a global error handler.
    pub fn set_global_handler(&self, handler: PyObject) {
        *self.global.write() = Some(Arc::new(PyErrorHandler::new(handler)));
    }

    /// Register a handler for a specific status code.
    pub fn set_status_handler(&self, status: u16, handler: PyObject) {
        self.status_handlers
            .write()
            .insert(status, Arc::new(PyErrorHandler::new(handler)));
    }

    /// Register a handler for a specific exception type.
    pub fn set_exception_handler(&self, exception_type: impl Into<String>, handler: PyObject) {
        self.exception_handlers
            .write()
            .insert(exception_type.into(), Arc::new(PyErrorHandler::new(handler)));
    }

    /// Register a blueprint-scoped handler registry.
    pub fn set_blueprint_handlers(&self, blueprint: impl Into<String>, registry: Arc<ErrorHandlerRegistry>) {
        self.blueprint_handlers
            .write()
            .insert(blueprint.into(), registry);
    }

    /// Handle an error, finding the most specific handler.
    pub fn handle(&self, error: &AppError, request: &Request, blueprint: Option<&str>) -> Response {
        // Try blueprint-scoped handlers first
        if let Some(bp_name) = blueprint {
            if let Some(bp_registry) = self.blueprint_handlers.read().get(bp_name) {
                let response = bp_registry.handle(error, request, None);
                // Only use blueprint handler if it produced a non-default response
                if response.status != 500 || bp_registry.has_handlers() {
                    return response;
                }
            }
        }

        // Try exception-type handler for Python exceptions
        if let AppError::PythonException(info) = error {
            if let Some(handler) = self.exception_handlers.read().get(&info.exception_type) {
                return handler.handle(error, request);
            }
        }

        // Try status code handler
        let status = error.status_code();
        if let Some(handler) = self.status_handlers.read().get(&status) {
            return handler.handle(error, request);
        }

        // Try global handler
        if let Some(handler) = self.global.read().as_ref() {
            return handler.handle(error, request);
        }

        // Default: convert error to response
        let mut response = error.to_response();

        // In debug mode, include traceback for Python exceptions
        if self.is_debug() {
            if let AppError::PythonException(info) = error {
                if let Some(tb) = &info.traceback {
                    let mut problem = error.to_problem_details();
                    problem.extensions.insert("traceback".to_string(), serde_json::json!(tb));
                    response = problem.to_response();
                }
            }
        }

        response
    }

    /// Check if any handlers are registered.
    pub fn has_handlers(&self) -> bool {
        self.global.read().is_some()
            || !self.status_handlers.read().is_empty()
            || !self.exception_handlers.read().is_empty()
    }
}

impl Default for ErrorHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ErrorHandlerRegistry {
    fn clone(&self) -> Self {
        Self {
            global: RwLock::new(self.global.read().clone()),
            status_handlers: RwLock::new(self.status_handlers.read().clone()),
            exception_handlers: RwLock::new(self.exception_handlers.read().clone()),
            blueprint_handlers: RwLock::new(self.blueprint_handlers.read().clone()),
            debug_mode: RwLock::new(*self.debug_mode.read()),
        }
    }
}

/// Python-exposed error handler registry.
#[pyclass]
pub struct PyErrorHandlerRegistry {
    inner: Arc<ErrorHandlerRegistry>,
}

#[pymethods]
impl PyErrorHandlerRegistry {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ErrorHandlerRegistry::new()),
        }
    }

    /// Set debug mode.
    pub fn set_debug(&self, debug: bool) {
        self.inner.set_debug(debug);
    }

    /// Register a global error handler.
    pub fn error_handler(&self, handler: PyObject) {
        self.inner.set_global_handler(handler);
    }

    /// Register a status code handler.
    pub fn status_handler(&self, status: u16, handler: PyObject) {
        self.inner.set_status_handler(status, handler);
    }

    /// Register an exception type handler.
    pub fn exception_handler(&self, exception_type: String, handler: PyObject) {
        self.inner.set_exception_handler(exception_type, handler);
    }
}

impl Default for PyErrorHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_status_codes() {
        assert_eq!(AppError::NotFound("test".into()).status_code(), 404);
        assert_eq!(AppError::Unauthorized("test".into()).status_code(), 401);
        assert_eq!(AppError::Forbidden("test".into()).status_code(), 403);
        assert_eq!(AppError::BadRequest("test".into()).status_code(), 400);
        assert_eq!(
            AppError::RateLimited {
                retry_after: Some(60),
                message: "test".into()
            }
            .status_code(),
            429
        );
    }

    #[test]
    fn test_problem_details_serialization() {
        let problem = ProblemDetails {
            type_uri: "/errors/test".to_string(),
            title: "Test Error".to_string(),
            status: 400,
            detail: Some("Something went wrong".to_string()),
            instance: None,
            extensions: HashMap::new(),
        };

        let json = serde_json::to_string(&problem).unwrap();
        assert!(json.contains("\"type\":\"/errors/test\""));
        assert!(json.contains("\"title\":\"Test Error\""));
        assert!(json.contains("\"status\":400"));
    }

    #[test]
    fn test_field_error() {
        let error = FieldError::new("email", "Invalid email format", "invalid_format");
        assert_eq!(error.field, "email");
        assert_eq!(error.message, "Invalid email format");
        assert_eq!(error.code, "invalid_format");
    }
}
