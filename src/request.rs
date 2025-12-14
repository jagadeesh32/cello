//! HTTP Request type exposed to Python.
//!
//! Provides access to request data with minimal copying.

use pyo3::prelude::*;
use std::collections::HashMap;

use crate::json::{json_to_python, parse_json};
use crate::multipart::parse_urlencoded;

/// HTTP Request wrapper exposed to Python.
#[pyclass]
#[derive(Clone)]
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
    body: Vec<u8>,
    
    /// Content type
    content_type: Option<String>,
}

#[pymethods]
impl Request {
    /// Create a new Request (primarily for testing).
    #[new]
    #[pyo3(signature = (method, path, params=None, query=None, headers=None, body=None))]
    pub fn new(
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
        }
    }

    /// Get the query parameters dict.
    #[getter]
    pub fn query(&self) -> HashMap<String, String> {
        self.query_params.clone()
    }

    /// Get the request body as a string.
    pub fn text(&self) -> PyResult<String> {
        String::from_utf8(self.body.clone())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Get the request body as bytes.
    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }

    /// Parse the request body as JSON using SIMD acceleration.
    pub fn json(&self, py: Python<'_>) -> PyResult<PyObject> {
        let text = self.text()?;
        let value = parse_json(&text)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
        json_to_python(py, &value)
    }

    /// Parse the request body as form data.
    pub fn form(&self) -> PyResult<HashMap<String, String>> {
        parse_urlencoded(&self.body)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
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

    /// Get the client IP address (from X-Forwarded-For or X-Real-IP).
    pub fn client_ip(&self) -> Option<String> {
        self.get_header("x-forwarded-for", None)
            .or_else(|| self.get_header("x-real-ip", None))
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
}

impl Request {
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
        }
    }

    /// Get the raw body bytes (internal use).
    pub fn body_bytes(&self) -> &[u8] {
        &self.body
    }
}
