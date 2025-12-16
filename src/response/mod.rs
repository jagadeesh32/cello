//! HTTP Response types and builders for Cello.
//!
//! This module provides:
//! - Standard HTTP responses (JSON, HTML, text, binary)
//! - Streaming responses with backpressure
//! - File responses with zero-copy sendfile
//! - XML serialization
//! - Content negotiation helpers

pub mod streaming;
pub mod xml;

use pyo3::prelude::*;
use std::collections::HashMap;
use std::path::Path;

use crate::json::python_to_json;

pub use streaming::{StreamingResponse, ChunkedBody, FileBody, StreamItem};
pub use xml::{XmlResponse, XmlSerializer};

// ============================================================================
// Response Body Types
// ============================================================================

/// Response body variants for different content types.
#[derive(Clone, Debug)]
pub enum ResponseBody {
    /// Standard bytes body
    Bytes(Vec<u8>),
    /// Empty body
    Empty,
    /// Marker for streaming (actual stream handled separately)
    Streaming,
    /// Marker for file body (path stored in headers)
    File(String),
    /// Marker for chunked transfer
    Chunked,
}

impl Default for ResponseBody {
    fn default() -> Self {
        ResponseBody::Empty
    }
}

// ============================================================================
// HTTP Response
// ============================================================================

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

    /// Body type marker
    body_type: ResponseBody,
}

#[pymethods]
impl Response {
    /// Create a new Response.
    #[new]
    #[pyo3(signature = (body=None, status=None, headers=None, content_type=None))]
    pub fn py_new(
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
            body_type: if body.is_some() {
                ResponseBody::Bytes(Vec::new())
            } else {
                ResponseBody::Empty
            },
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
            body_type: ResponseBody::Bytes(Vec::new()),
        })
    }

    /// Create a plain text response.
    #[staticmethod]
    #[pyo3(signature = (content, status=None))]
    pub fn text(content: &str, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "text/plain; charset=utf-8".to_string(),
        );

        Response {
            status: status.unwrap_or(200),
            headers,
            body: content.as_bytes().to_vec(),
            content_type: "text/plain".to_string(),
            body_type: ResponseBody::Bytes(Vec::new()),
        }
    }

    /// Create an HTML response.
    #[staticmethod]
    #[pyo3(signature = (content, status=None))]
    pub fn html(content: &str, status: Option<u16>) -> Self {
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "text/html; charset=utf-8".to_string(),
        );

        Response {
            status: status.unwrap_or(200),
            headers,
            body: content.as_bytes().to_vec(),
            content_type: "text/html".to_string(),
            body_type: ResponseBody::Bytes(Vec::new()),
        }
    }

    /// Create a binary response.
    #[staticmethod]
    #[pyo3(signature = (data, content_type=None, status=None))]
    pub fn binary(data: Vec<u8>, content_type: Option<&str>, status: Option<u16>) -> Self {
        let ct = content_type
            .unwrap_or("application/octet-stream")
            .to_string();
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), ct.clone());

        Response {
            status: status.unwrap_or(200),
            headers,
            body: data,
            content_type: ct,
            body_type: ResponseBody::Bytes(Vec::new()),
        }
    }

    /// Create a file download response.
    #[staticmethod]
    #[pyo3(signature = (path, filename=None, content_type=None))]
    pub fn file(path: &str, filename: Option<&str>, content_type: Option<&str>) -> PyResult<Self> {
        let data = std::fs::read(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

        let ct = content_type.map(|s| s.to_string()).unwrap_or_else(|| {
            mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string()
        });

        let download_name = filename.map(|s| s.to_string()).unwrap_or_else(|| {
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
            body_type: ResponseBody::Bytes(Vec::new()),
        })
    }

    /// Create a sendfile response (zero-copy).
    #[staticmethod]
    #[pyo3(signature = (path, content_type=None))]
    pub fn sendfile(path: &str, content_type: Option<&str>) -> PyResult<Self> {
        // Verify file exists
        let metadata = std::fs::metadata(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

        let ct = content_type.map(|s| s.to_string()).unwrap_or_else(|| {
            mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string()
        });

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), ct.clone());
        headers.insert("Content-Length".to_string(), metadata.len().to_string());
        headers.insert("X-Sendfile-Path".to_string(), path.to_string());

        Ok(Response {
            status: 200,
            headers,
            body: Vec::new(),
            content_type: ct,
            body_type: ResponseBody::File(path.to_string()),
        })
    }

    /// Create a file range response (partial content).
    #[staticmethod]
    #[pyo3(signature = (path, range_header, content_type=None))]
    pub fn file_range(
        path: &str,
        range_header: &str,
        content_type: Option<&str>,
    ) -> PyResult<Self> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;

        let file_size = metadata.len();
        let ct = content_type.map(|s| s.to_string()).unwrap_or_else(|| {
            mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string()
        });

        // Parse range header (bytes=start-end)
        let (start, end) = parse_range_header(range_header, file_size)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        let content_length = end - start + 1;

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), ct.clone());
        headers.insert("Content-Length".to_string(), content_length.to_string());
        headers.insert(
            "Content-Range".to_string(),
            format!("bytes {}-{}/{}", start, end, file_size),
        );
        headers.insert("Accept-Ranges".to_string(), "bytes".to_string());
        headers.insert("X-Sendfile-Path".to_string(), path.to_string());
        headers.insert("X-Sendfile-Offset".to_string(), start.to_string());
        headers.insert("X-Sendfile-Length".to_string(), content_length.to_string());

        Ok(Response {
            status: 206,
            headers,
            body: Vec::new(),
            content_type: ct,
            body_type: ResponseBody::File(path.to_string()),
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
            body_type: ResponseBody::Empty,
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
            body_type: ResponseBody::Empty,
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

    /// Create an XML response.
    #[staticmethod]
    #[pyo3(signature = (data, status=None, root_name=None))]
    pub fn xml(
        py: Python<'_>,
        data: &PyAny,
        status: Option<u16>,
        root_name: Option<&str>,
    ) -> PyResult<Self> {
        let xml_content = xml::python_to_xml(py, data, root_name.unwrap_or("root"))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;

        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/xml; charset=utf-8".to_string(),
        );

        Ok(Response {
            status: status.unwrap_or(200),
            headers,
            body: xml_content.into_bytes(),
            content_type: "application/xml".to_string(),
            body_type: ResponseBody::Bytes(Vec::new()),
        })
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
    /// Create a new response with status code (Rust API).
    pub fn new(status: u16) -> Self {
        Response {
            status,
            headers: HashMap::new(),
            body: Vec::new(),
            content_type: "text/plain".to_string(),
            body_type: ResponseBody::Empty,
        }
    }

    /// Get the body bytes (internal use).
    pub fn body_bytes(&self) -> &[u8] {
        &self.body
    }

    /// Set the body (internal use).
    pub fn set_body(&mut self, body: Vec<u8>) {
        self.body = body;
        self.body_type = ResponseBody::Bytes(Vec::new());
    }

    /// Get body type.
    pub fn body_type(&self) -> &ResponseBody {
        &self.body_type
    }

    /// Check if this is a streaming response.
    pub fn is_streaming(&self) -> bool {
        matches!(self.body_type, ResponseBody::Streaming)
    }

    /// Check if this is a file response.
    pub fn is_file(&self) -> bool {
        matches!(self.body_type, ResponseBody::File(_))
    }

    /// Check if this is a chunked response.
    pub fn is_chunked(&self) -> bool {
        matches!(self.body_type, ResponseBody::Chunked)
    }

    /// Mark as streaming response.
    pub fn set_streaming(&mut self) {
        self.body_type = ResponseBody::Streaming;
        self.headers
            .insert("Transfer-Encoding".to_string(), "chunked".to_string());
    }

    /// Mark as chunked response.
    pub fn set_chunked(&mut self) {
        self.body_type = ResponseBody::Chunked;
        self.headers
            .insert("Transfer-Encoding".to_string(), "chunked".to_string());
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
            body_type: ResponseBody::Bytes(Vec::new()),
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
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse HTTP Range header.
fn parse_range_header(header: &str, file_size: u64) -> Result<(u64, u64), String> {
    // Format: bytes=start-end or bytes=start- or bytes=-suffix
    let header = header.trim();
    if !header.starts_with("bytes=") {
        return Err("Invalid range header format".to_string());
    }

    let range = &header[6..];
    let parts: Vec<&str> = range.split('-').collect();

    if parts.len() != 2 {
        return Err("Invalid range format".to_string());
    }

    let start = if parts[0].is_empty() {
        // Suffix range: -500 means last 500 bytes
        let suffix: u64 = parts[1]
            .parse()
            .map_err(|_| "Invalid suffix range")?;
        file_size.saturating_sub(suffix)
    } else {
        parts[0].parse().map_err(|_| "Invalid start range")?
    };

    let end = if parts[1].is_empty() {
        file_size - 1
    } else {
        parts[1].parse().map_err(|_| "Invalid end range")?
    };

    if start > end || end >= file_size {
        return Err("Range not satisfiable".to_string());
    }

    Ok((start, end))
}

/// Get content type for Accept header negotiation.
pub fn negotiate_content_type(accept: &str, available: &[&str]) -> Option<String> {
    // Parse Accept header and find best match
    let mut accepted: Vec<(&str, f32)> = accept
        .split(',')
        .filter_map(|part| {
            let mut iter = part.trim().split(';');
            let media_type = iter.next()?.trim();
            let quality = iter
                .find_map(|p| {
                    let p = p.trim();
                    if p.starts_with("q=") {
                        p[2..].parse().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(1.0);
            Some((media_type, quality))
        })
        .collect();

    // Sort by quality (highest first)
    accepted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Find first match
    for (media_type, _) in accepted {
        if media_type == "*/*" {
            return available.first().map(|s| s.to_string());
        }
        if available.contains(&media_type) {
            return Some(media_type.to_string());
        }
        // Check for type/* matches
        if let Some(slash_pos) = media_type.find('/') {
            let type_part = &media_type[..slash_pos];
            if media_type.ends_with("/*") {
                for avail in available {
                    if avail.starts_with(type_part) {
                        return Some(avail.to_string());
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_header() {
        assert_eq!(parse_range_header("bytes=0-499", 1000), Ok((0, 499)));
        assert_eq!(parse_range_header("bytes=500-", 1000), Ok((500, 999)));
        assert_eq!(parse_range_header("bytes=-100", 1000), Ok((900, 999)));
        assert!(parse_range_header("bytes=500-400", 1000).is_err());
        assert!(parse_range_header("invalid", 1000).is_err());
    }

    #[test]
    fn test_negotiate_content_type() {
        let available = &["application/json", "text/html", "text/plain"];

        assert_eq!(
            negotiate_content_type("application/json", available),
            Some("application/json".to_string())
        );

        assert_eq!(
            negotiate_content_type("text/html, application/json;q=0.9", available),
            Some("text/html".to_string())
        );

        assert_eq!(
            negotiate_content_type("*/*", available),
            Some("application/json".to_string())
        );

        assert_eq!(
            negotiate_content_type("text/*", available),
            Some("text/html".to_string())
        );
    }

    #[test]
    fn test_response_body_types() {
        let mut resp = Response::new(200);
        assert!(matches!(resp.body_type(), ResponseBody::Empty));

        resp.set_body(b"hello".to_vec());
        assert!(matches!(resp.body_type(), ResponseBody::Bytes(_)));

        resp.set_streaming();
        assert!(resp.is_streaming());

        let mut resp2 = Response::new(200);
        resp2.set_chunked();
        assert!(resp2.is_chunked());
    }
}
