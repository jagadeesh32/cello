//! HTTP Response type and builder.
//!
//! Provides JSON serialization, file serving, and streaming responses.

use pyo3::prelude::*;
use std::collections::HashMap;
use std::path::Path;

use crate::json::python_to_json;

/// HTTP Response class exposed to Python.
#[pyclass]
#[derive(Clone, Debug)]
pub struct Response {
    /// HTTP status code
    #[pyo3(get, set)]
    pub status: u16,

    /// Response headers
    #[pyo3(get)]
    pub headers: HashMap<String, String>,

    /// Response body
    body: Vec<u8>,

    /// Content type
    content_type: String,
    
    /// Is this a streaming response
    is_streaming: bool,
}

#[pymethods]
impl Response {
    /// Create a new Response.
    #[new]
    #[pyo3(signature = (body=None, status=None, headers=None, content_type=None))]
    pub fn new(
        body: Option<&str>,
        status: Option<u16>,
        headers: Option<HashMap<String, String>>,
        content_type: Option<&str>,
    ) -> Self {
        let mut h = headers.unwrap_or_default();
        let ct = content_type.unwrap_or("text/plain").to_string();
        h.insert("Content-Type".to_string(), ct.clone());
        
        Response {
            status: status.unwrap_or(200),
            headers: h,
            body: body.map(|s| s.as_bytes().to_vec()).unwrap_or_default(),
            content_type: ct,
            is_streaming: false,
        }
    }

    /// Create a JSON response.
    #[staticmethod]
    #[pyo3(signature = (data, status=None))]
    pub fn json(py: Python<'_>, data: &PyAny, status: Option<u16>) -> PyResult<Self> {
        let json_value = python_to_json(py, data)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
        let body = serde_json::to_vec(&json_value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        Ok(Response {
            status: status.unwrap_or(200),
            headers,
            body,
            content_type: "application/json".to_string(),
            is_streaming: false,
        })
    }

    /// Create a plain text response.
    #[staticmethod]
    #[pyo3(signature = (content, status=None))]
    pub fn text(content: &str, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());

        Response {
            status: status.unwrap_or(200),
            headers,
            body: content.as_bytes().to_vec(),
            content_type: "text/plain".to_string(),
            is_streaming: false,
        }
    }

    /// Create an HTML response.
    #[staticmethod]
    #[pyo3(signature = (content, status=None))]
    pub fn html(content: &str, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());

        Response {
            status: status.unwrap_or(200),
            headers,
            body: content.as_bytes().to_vec(),
            content_type: "text/html".to_string(),
            is_streaming: false,
        }
    }

    /// Create a binary response.
    #[staticmethod]
    #[pyo3(signature = (data, content_type=None, status=None))]
    pub fn binary(data: Vec<u8>, content_type: Option<&str>, status: Option<u16>) -> Self {
        let ct = content_type.unwrap_or("application/octet-stream").to_string();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), ct.clone());

        Response {
            status: status.unwrap_or(200),
            headers,
            body: data,
            content_type: ct,
            is_streaming: false,
        }
    }

    /// Create a file download response.
    #[staticmethod]
    #[pyo3(signature = (path, filename=None, content_type=None))]
    pub fn file(path: &str, filename: Option<&str>, content_type: Option<&str>) -> PyResult<Self> {
        let data = std::fs::read(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
        
        let ct = content_type
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string()
            });
        
        let download_name = filename
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "download".to_string())
            });
        
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), ct.clone());
        headers.insert(
            "Content-Disposition".to_string(),
            format!("attachment; filename=\"{}\"", download_name),
        );
        headers.insert("Content-Length".to_string(), data.len().to_string());

        Ok(Response {
            status: 200,
            headers,
            body: data,
            content_type: ct,
            is_streaming: false,
        })
    }

    /// Create a redirect response.
    #[staticmethod]
    #[pyo3(signature = (url, permanent=None))]
    pub fn redirect(url: &str, permanent: Option<bool>) -> Self {
        let status = if permanent.unwrap_or(false) { 301 } else { 302 };
        let mut headers = HashMap::new();
        headers.insert("Location".to_string(), url.to_string());

        Response {
            status,
            headers,
            body: Vec::new(),
            content_type: "text/plain".to_string(),
            is_streaming: false,
        }
    }

    /// Create a "204 No Content" response.
    #[staticmethod]
    pub fn no_content() -> Self {
        Response {
            status: 204,
            headers: HashMap::new(),
            body: Vec::new(),
            content_type: "text/plain".to_string(),
            is_streaming: false,
        }
    }

    /// Create a "201 Created" response.
    #[staticmethod]
    #[pyo3(signature = (data=None, location=None))]
    pub fn created(py: Python<'_>, data: Option<&PyAny>, location: Option<&str>) -> PyResult<Self> {
        let mut resp = if let Some(d) = data {
            Self::json(py, d, Some(201))?
        } else {
            Self::no_content()
        };
        resp.status = 201;
        
        if let Some(loc) = location {
            resp.set_header("Location", loc);
        }
        
        Ok(resp)
    }

    /// Set a response header.
    pub fn set_header(&mut self, key: &str, value: &str) {
        self.headers.insert(key.to_string(), value.to_string());
    }

    /// Get the response body as bytes.
    pub fn body(&self) -> Vec<u8> {
        self.body.clone()
    }

    /// Get the content type.
    pub fn content_type(&self) -> String {
        self.content_type.clone()
    }

    /// Get the content length.
    pub fn content_length(&self) -> usize {
        self.body.len()
    }
}

impl Response {
    /// Get the body bytes (internal use).
    pub fn body_bytes(&self) -> &[u8] {
        &self.body
    }

    /// Set the body (internal use).
    pub fn set_body(&mut self, body: Vec<u8>) {
        self.body = body;
    }

    /// Create a response from JSON value (internal use).
    pub fn from_json_value(value: serde_json::Value, status: u16) -> Self {
        let body = serde_json::to_vec(&value).unwrap_or_default();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());

        Response {
            status,
            headers,
            body,
            content_type: "application/json".to_string(),
            is_streaming: false,
        }
    }

    /// Create an error response (internal use).
    pub fn error(status: u16, message: &str) -> Self {
        let body = serde_json::json!({
            "error": message,
            "status": status
        });
        Self::from_json_value(body, status)
    }

    /// Create a 400 Bad Request response.
    pub fn bad_request(message: &str) -> Self {
        Self::error(400, message)
    }

    /// Create a 401 Unauthorized response.
    pub fn unauthorized(message: &str) -> Self {
        Self::error(401, message)
    }

    /// Create a 403 Forbidden response.
    pub fn forbidden(message: &str) -> Self {
        Self::error(403, message)
    }

    /// Create a 404 Not Found response.
    pub fn not_found(message: &str) -> Self {
        Self::error(404, message)
    }

    /// Create a 500 Internal Server Error response.
    pub fn internal_error(message: &str) -> Self {
        Self::error(500, message)
    }

    /// Check if this is a streaming response.
    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }
}
