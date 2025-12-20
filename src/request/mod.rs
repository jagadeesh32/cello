//! HTTP Request types for Cello.
//!
//! This module provides:
//! - HTTP Request wrapper with typed parameters
//! - Lazy body parsing (JSON, form, multipart)
//! - Request context for middleware data
//! - Streaming multipart uploads

pub mod parsing;
pub mod multipart_streaming;

use pyo3::prelude::*;
use std::collections::HashMap;

use crate::json::{json_to_python, parse_json, python_to_json};
use crate::multipart::parse_urlencoded;

pub use parsing::{LazyBody, TypedParams, ParamError};
pub use multipart_streaming::{StreamingMultipart, MultipartPart};

// ============================================================================
// HTTP Request
// ============================================================================

/// HTTP Request wrapper exposed to Python.
#[pyclass]
#[derive(Clone, Default)]
pub struct Request {
    /// HTTP method (GET, POST, etc.)
    #[pyo3(get)]
    pub method: String,

    /// Request path (e.g., "/users/123")
    #[pyo3(get)]
    pub path: String,

    /// Path parameters extracted from the route (e.g., {"id": "123"})
    #[pyo3(get)]
    pub params: HashMap<String, String>,

    /// Query string parameters
    #[pyo3(get)]
    pub query_params: HashMap<String, String>,

    /// Request headers
    #[pyo3(get)]
    pub headers: HashMap<String, String>,

    /// Request body as bytes
    pub body: Vec<u8>,

    /// Content type
    content_type: Option<String>,

    /// Request context for middleware data sharing (internal)
    pub context: HashMap<String, serde_json::Value>,

    /// Lazy body cache (internal)
    lazy_cache: LazyCache,
}

/// Internal cache for lazy parsing results.
/// Uses RwLock for thread-safety to support async middleware.
#[derive(Clone, Default)]
pub struct LazyCache {
    json_parsed: std::sync::Arc<parking_lot::RwLock<Option<Result<serde_json::Value, String>>>>,
    form_parsed: std::sync::Arc<parking_lot::RwLock<Option<Result<HashMap<String, String>, String>>>>,
    text_parsed: std::sync::Arc<parking_lot::RwLock<Option<Result<String, String>>>>,
}

#[pymethods]
impl Request {
    /// Create a new Request (primarily for testing).
    #[new]
    #[pyo3(signature = (method, path, params=None, query=None, headers=None, body=None))]
    pub fn py_new(
        method: String,
        path: String,
        params: Option<HashMap<String, String>>,
        query: Option<HashMap<String, String>>,
        headers: Option<HashMap<String, String>>,
        body: Option<Vec<u8>>,
    ) -> Self {
        let headers_map = headers.unwrap_or_default();
        let content_type = headers_map.get("content-type").cloned();

        Request {
            method,
            path,
            params: params.unwrap_or_default(),
            query_params: query.unwrap_or_default(),
            headers: headers_map,
            body: body.unwrap_or_default(),
            content_type,
            context: HashMap::new(),
            lazy_cache: LazyCache::default(),
        }
    }

    /// Get the query parameters dict.
    #[getter]
    pub fn query(&self) -> HashMap<String, String> {
        self.query_params.clone()
    }

    /// Get the request body as a string (cached).
    pub fn text(&self) -> PyResult<String> {
        let mut cache = self.lazy_cache.text_parsed.write();
        if let Some(ref result) = *cache {
            return result
                .clone()
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e));
        }

        let result = String::from_utf8(self.body.clone()).map_err(|e| e.to_string());
        let return_value = result.clone();
        *cache = Some(result);

        return_value.map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }

    /// Get the request body as bytes.
    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }

    /// Parse the request body as JSON using SIMD acceleration (cached).
    pub fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
        let mut cache = self.lazy_cache.json_parsed.write();

        let value = if let Some(ref result) = *cache {
            result
                .clone()
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?
        } else {
            let text = String::from_utf8(self.body.clone())
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

            let result = parse_json(&text);
            let value = result
                .clone()
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.clone()))?;
            *cache = Some(result);
            value
        };

        json_to_python(py, &value)
    }

    /// Parse the request body as form data (cached).
    pub fn form(&self) -> PyResult<HashMap<String, String>> {
        let mut cache = self.lazy_cache.form_parsed.write();

        if let Some(ref result) = *cache {
            return result
                .clone()
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e));
        }

        let result = parse_urlencoded(&self.body);
        let return_value = result.clone();
        *cache = Some(result);

        return_value.map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }

    /// Get the content type.
    pub fn content_type(&self) -> Option<String> {
        self.content_type.clone()
    }

    /// Check if the request is JSON.
    pub fn is_json(&self) -> bool {
        self.content_type
            .as_ref()
            .map(|ct| ct.contains("application/json"))
            .unwrap_or(false)
    }

    /// Check if the request is form data.
    pub fn is_form(&self) -> bool {
        self.content_type
            .as_ref()
            .map(|ct| ct.contains("application/x-www-form-urlencoded"))
            .unwrap_or(false)
    }

    /// Check if the request is multipart.
    pub fn is_multipart(&self) -> bool {
        self.content_type
            .as_ref()
            .map(|ct| ct.contains("multipart/form-data"))
            .unwrap_or(false)
    }

    /// Get a query parameter by name.
    #[pyo3(signature = (key, default=None))]
    pub fn get_query_param(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.query_params
            .get(key)
            .cloned()
            .or_else(|| default.map(|s| s.to_string()))
    }

    /// Get a query parameter as integer.
    #[pyo3(signature = (key, default=None))]
    pub fn get_query_int(&self, key: &str, default: Option<i64>) -> PyResult<Option<i64>> {
        match self.query_params.get(key) {
            Some(value) => value
                .parse::<i64>()
                .map(Some)
                .map_err(|_| pyo3::exceptions::PyValueError::new_err(format!(
                    "Query parameter '{}' is not a valid integer",
                    key
                ))),
            None => Ok(default),
        }
    }

    /// Get a query parameter as float.
    #[pyo3(signature = (key, default=None))]
    pub fn get_query_float(&self, key: &str, default: Option<f64>) -> PyResult<Option<f64>> {
        match self.query_params.get(key) {
            Some(value) => value
                .parse::<f64>()
                .map(Some)
                .map_err(|_| pyo3::exceptions::PyValueError::new_err(format!(
                    "Query parameter '{}' is not a valid float",
                    key
                ))),
            None => Ok(default),
        }
    }

    /// Get a query parameter as boolean.
    #[pyo3(signature = (key, default=None))]
    pub fn get_query_bool(&self, key: &str, default: Option<bool>) -> Option<bool> {
        match self.query_params.get(key) {
            Some(value) => {
                let lower = value.to_lowercase();
                Some(lower == "true" || lower == "1" || lower == "yes" || lower == "on")
            }
            None => default,
        }
    }

    /// Get a header by name (case-insensitive).
    #[pyo3(signature = (key, default=None))]
    pub fn get_header(&self, key: &str, default: Option<&str>) -> Option<String> {
        let key_lower = key.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == key_lower)
            .map(|(_, v)| v.clone())
            .or_else(|| default.map(|s| s.to_string()))
    }

    /// Get a path parameter by name.
    #[pyo3(signature = (key, default=None))]
    pub fn get_param(&self, key: &str, default: Option<&str>) -> Option<String> {
        self.params
            .get(key)
            .cloned()
            .or_else(|| default.map(|s| s.to_string()))
    }

    /// Get a path parameter as integer.
    #[pyo3(signature = (key, default=None))]
    pub fn get_param_int(&self, key: &str, default: Option<i64>) -> PyResult<Option<i64>> {
        match self.params.get(key) {
            Some(value) => value
                .parse::<i64>()
                .map(Some)
                .map_err(|_| pyo3::exceptions::PyValueError::new_err(format!(
                    "Path parameter '{}' is not a valid integer",
                    key
                ))),
            None => Ok(default),
        }
    }

    /// Get the client IP address (from X-Forwarded-For or X-Real-IP).
    pub fn client_ip(&self) -> Option<String> {
        // Check X-Forwarded-For first (get first IP in chain)
        if let Some(xff) = self.get_header("x-forwarded-for", None) {
            if let Some(ip) = xff.split(',').next() {
                return Some(ip.trim().to_string());
            }
        }
        // Fall back to X-Real-IP
        self.get_header("x-real-ip", None)
    }

    /// Get the User-Agent header.
    pub fn user_agent(&self) -> Option<String> {
        self.get_header("user-agent", None)
    }

    /// Check if the request accepts a specific content type.
    pub fn accepts(&self, content_type: &str) -> bool {
        self.get_header("accept", None)
            .map(|accept| accept.contains(content_type))
            .unwrap_or(false)
    }

    /// Check if the request is an AJAX/XHR request.
    pub fn is_xhr(&self) -> bool {
        self.get_header("x-requested-with", None)
            .map(|v| v.to_lowercase() == "xmlhttprequest")
            .unwrap_or(false)
    }

    /// Check if the request is secure (HTTPS).
    pub fn is_secure(&self) -> bool {
        // Check X-Forwarded-Proto header
        if let Some(proto) = self.get_header("x-forwarded-proto", None) {
            return proto.to_lowercase() == "https";
        }
        // Check scheme header
        self.get_header("x-forwarded-scheme", None)
            .map(|s| s.to_lowercase() == "https")
            .unwrap_or(false)
    }

    /// Get the request host.
    pub fn host(&self) -> Option<String> {
        self.get_header("x-forwarded-host", None)
            .or_else(|| self.get_header("host", None))
    }

    /// Get the request ID from context.
    pub fn request_id(&self) -> Option<String> {
        self.context
            .get("request_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get a context value by key.
    pub fn get_context(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        match self.context.get(key) {
            Some(value) => json_to_python(py, value),
            None => Ok(py.None()),
        }
    }

    /// Set a context value by key.
    pub fn set_context(&mut self, py: Python<'_>, key: String, value: PyObject) -> PyResult<()> {
        let json_value = python_to_json(py, value.as_ref(py))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
        self.context.insert(key, json_value);
        Ok(())
    }

    /// Get a context value as string by key.
    pub fn get_context_string(&self, key: &str) -> Option<String> {
        self.context
            .get(key)
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
    }
}

impl Request {
    /// Create a new request (Rust API).
    pub fn new(method: &str, path: &str) -> Self {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            params: HashMap::new(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: Vec::new(),
            content_type: None,
            context: HashMap::new(),
            lazy_cache: LazyCache::default(),
        }
    }

    /// Create a request from HTTP components (internal use).
    pub fn from_http(
        method: String,
        path: String,
        params: HashMap<String, String>,
        query: HashMap<String, String>,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    ) -> Self {
        let content_type = headers.get("content-type").cloned();

        Request {
            method,
            path,
            params,
            query_params: query,
            headers,
            body,
            content_type,
            context: HashMap::new(),
            lazy_cache: LazyCache::default(),
        }
    }

    /// Get the raw body bytes (internal use).
    pub fn body_bytes(&self) -> &[u8] {
        &self.body
    }

    /// Get typed parameters helper.
    pub fn typed_params(&self) -> TypedParams {
        TypedParams::from_map(&self.params)
    }

    /// Get typed query parameters helper.
    pub fn typed_query(&self) -> TypedParams {
        TypedParams::from_map(&self.query_params)
    }

    /// Get lazy body parser.
    pub fn lazy_body(&self) -> LazyBody {
        LazyBody::new(&self.body)
    }

    /// Get the multipart boundary if this is a multipart request.
    pub fn multipart_boundary(&self) -> Option<String> {
        self.content_type.as_ref().and_then(|ct| {
            if ct.contains("multipart/form-data") {
                ct.split("boundary=")
                    .nth(1)
                    .map(|b| b.trim_matches('"').to_string())
            } else {
                None
            }
        })
    }

    /// Set a context value (Rust-only, internal use).
    pub fn set_context_internal(&mut self, key: &str, value: serde_json::Value) {
        self.context.insert(key.to_string(), value);
    }

    /// Get a context value by key (Rust-only, internal use).
    pub fn get_context_internal(&self, key: &str) -> Option<serde_json::Value> {
        self.context.get(key).cloned()
    }

    /// Get a context value as string (Rust-only, internal use).
    pub fn get_context_str(&self, key: &str) -> Option<String> {
        self.context
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_creation() {
        let request = Request::new("GET", "/users/123");
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/users/123");
    }

    #[test]
    fn test_typed_params() {
        let mut params = HashMap::new();
        params.insert("id".to_string(), "123".to_string());
        params.insert("name".to_string(), "test".to_string());

        let typed = TypedParams::from_map(&params);
        assert_eq!(typed.get::<i64>("id").unwrap(), 123);
        assert_eq!(typed.get::<String>("name").unwrap(), "test");
    }

    #[test]
    fn test_query_params() {
        let mut query = HashMap::new();
        query.insert("page".to_string(), "2".to_string());
        query.insert("active".to_string(), "true".to_string());

        let mut request = Request::new("GET", "/items");
        request.query_params = query;

        assert_eq!(request.get_query_param("page", None), Some("2".to_string()));
        assert_eq!(request.get_query_bool("active", None), Some(true));
    }

    #[test]
    fn test_context() {
        let mut request = Request::new("GET", "/test");
        request.set_context_internal("user_id", serde_json::json!(123));
        request.set_context_internal("role", serde_json::json!("admin"));

        assert_eq!(
            request.get_context_internal("user_id"),
            Some(serde_json::json!(123))
        );
        assert_eq!(
            request.get_context_str("role"),
            Some("admin".to_string())
        );
    }

    #[test]
    fn test_multipart_boundary() {
        let mut request = Request::new("POST", "/upload");
        request.content_type = Some("multipart/form-data; boundary=----WebKitFormBoundary123".to_string());

        assert_eq!(
            request.multipart_boundary(),
            Some("----WebKitFormBoundary123".to_string())
        );
    }
}
