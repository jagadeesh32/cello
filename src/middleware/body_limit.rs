//! Body limit middleware for Cello.
//!
//! Provides:
//! - Request body size limits
//! - Per-route overrides
//! - Content-Type based limits

use std::collections::HashMap;

use super::{Middleware, MiddlewareAction, MiddlewareError, MiddlewareResult};
use crate::request::Request;

// ============================================================================
// Size Parsing
// ============================================================================

/// Parse size string to bytes (e.g., "10mb", "1gb", "512kb").
pub fn parse_size(s: &str) -> Option<usize> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    // Try to parse as pure number first
    if let Ok(n) = s.parse::<usize>() {
        return Some(n);
    }

    // Parse with unit suffix
    let (num_str, unit) = if s.ends_with("gb") {
        (&s[..s.len() - 2], "gb")
    } else if s.ends_with("mb") {
        (&s[..s.len() - 2], "mb")
    } else if s.ends_with("kb") {
        (&s[..s.len() - 2], "kb")
    } else if s.ends_with("b") {
        (&s[..s.len() - 1], "b")
    } else if s.ends_with("g") {
        (&s[..s.len() - 1], "gb")
    } else if s.ends_with("m") {
        (&s[..s.len() - 1], "mb")
    } else if s.ends_with("k") {
        (&s[..s.len() - 1], "kb")
    } else {
        return None;
    };

    let num: f64 = num_str.trim().parse().ok()?;
    let multiplier: f64 = match unit {
        "gb" => 1024.0 * 1024.0 * 1024.0,
        "mb" => 1024.0 * 1024.0,
        "kb" => 1024.0,
        "b" => 1.0,
        _ => return None,
    };

    Some((num * multiplier) as usize)
}

/// Format bytes as human-readable string.
pub fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

// ============================================================================
// Body Limit Configuration
// ============================================================================

/// Body limit configuration.
#[derive(Clone)]
pub struct BodyLimitConfig {
    /// Default maximum body size
    pub default_limit: usize,
    /// Limits by route pattern
    pub route_limits: HashMap<String, usize>,
    /// Limits by Content-Type
    pub content_type_limits: HashMap<String, usize>,
    /// Limit for JSON requests
    pub json_limit: Option<usize>,
    /// Limit for form data
    pub form_limit: Option<usize>,
    /// Limit for multipart uploads
    pub multipart_limit: Option<usize>,
    /// Custom error message
    pub error_message: Option<String>,
}

impl BodyLimitConfig {
    /// Create new body limit config with default limit.
    pub fn new(default_limit: &str) -> Self {
        Self {
            default_limit: parse_size(default_limit).unwrap_or(10 * 1024 * 1024), // 10MB default
            route_limits: HashMap::new(),
            content_type_limits: HashMap::new(),
            json_limit: None,
            form_limit: None,
            multipart_limit: None,
            error_message: None,
        }
    }

    /// Set default limit from size string.
    pub fn set_default(mut self, size: &str) -> Self {
        if let Some(bytes) = parse_size(size) {
            self.default_limit = bytes;
        }
        self
    }

    /// Set limit for specific route.
    pub fn route(mut self, path: &str, size: &str) -> Self {
        if let Some(bytes) = parse_size(size) {
            self.route_limits.insert(path.to_string(), bytes);
        }
        self
    }

    /// Set limit for specific Content-Type.
    pub fn content_type(mut self, content_type: &str, size: &str) -> Self {
        if let Some(bytes) = parse_size(size) {
            self.content_type_limits.insert(content_type.to_string(), bytes);
        }
        self
    }

    /// Set limit for JSON requests.
    pub fn json(mut self, size: &str) -> Self {
        self.json_limit = parse_size(size);
        self
    }

    /// Set limit for form data.
    pub fn form(mut self, size: &str) -> Self {
        self.form_limit = parse_size(size);
        self
    }

    /// Set limit for multipart uploads.
    pub fn multipart(mut self, size: &str) -> Self {
        self.multipart_limit = parse_size(size);
        self
    }

    /// Set custom error message.
    pub fn error_message(mut self, message: &str) -> Self {
        self.error_message = Some(message.to_string());
        self
    }

    /// Get limit for a request.
    pub fn get_limit(&self, path: &str, content_type: Option<&str>) -> usize {
        // Check route-specific limits first
        for (pattern, limit) in &self.route_limits {
            if path.starts_with(pattern) || path == pattern {
                return *limit;
            }
        }

        // Check content-type specific limits
        if let Some(ct) = content_type {
            let ct_lower = ct.to_lowercase();

            // Check JSON
            if ct_lower.contains("application/json") {
                if let Some(limit) = self.json_limit {
                    return limit;
                }
            }

            // Check form
            if ct_lower.contains("application/x-www-form-urlencoded") {
                if let Some(limit) = self.form_limit {
                    return limit;
                }
            }

            // Check multipart
            if ct_lower.contains("multipart/form-data") {
                if let Some(limit) = self.multipart_limit {
                    return limit;
                }
            }

            // Check custom content-type limits
            for (ct_pattern, limit) in &self.content_type_limits {
                if ct_lower.contains(ct_pattern) {
                    return *limit;
                }
            }
        }

        self.default_limit
    }
}

impl Default for BodyLimitConfig {
    fn default() -> Self {
        Self::new("10mb")
    }
}

// ============================================================================
// Body Limit Middleware
// ============================================================================

/// Body limit middleware.
pub struct BodyLimitMiddleware {
    config: BodyLimitConfig,
}

impl BodyLimitMiddleware {
    /// Create new body limit middleware with default 10MB limit.
    pub fn new() -> Self {
        Self {
            config: BodyLimitConfig::default(),
        }
    }

    /// Create with size string.
    pub fn with_limit(size: &str) -> Self {
        Self {
            config: BodyLimitConfig::new(size),
        }
    }

    /// Create with config.
    pub fn with_config(config: BodyLimitConfig) -> Self {
        Self { config }
    }

    /// Set default limit.
    pub fn default_limit(mut self, size: &str) -> Self {
        self.config = self.config.set_default(size);
        self
    }

    /// Set route-specific limit.
    pub fn route(mut self, path: &str, size: &str) -> Self {
        self.config = self.config.route(path, size);
        self
    }

    /// Set JSON limit.
    pub fn json(mut self, size: &str) -> Self {
        self.config = self.config.json(size);
        self
    }

    /// Set form limit.
    pub fn form(mut self, size: &str) -> Self {
        self.config = self.config.form(size);
        self
    }

    /// Set multipart limit.
    pub fn multipart(mut self, size: &str) -> Self {
        self.config = self.config.multipart(size);
        self
    }
}

impl Default for BodyLimitMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for BodyLimitMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Only check for requests that might have a body
        if request.method == "GET" || request.method == "HEAD" || request.method == "OPTIONS" {
            return Ok(MiddlewareAction::Continue);
        }

        // Get Content-Type
        let content_type = request.headers.get("content-type").map(|s| s.as_str());

        // Get limit for this request
        let limit = self.config.get_limit(&request.path, content_type);

        // Check Content-Length header first (before body is read)
        if let Some(content_length) = request.headers.get("content-length") {
            if let Ok(length) = content_length.parse::<usize>() {
                if length > limit {
                    let message = self.config.error_message.clone().unwrap_or_else(|| {
                        format!(
                            "Request body too large. Maximum allowed size is {}.",
                            format_size(limit)
                        )
                    });

                    return Err(MiddlewareError::payload_too_large(&message));
                }
            }
        }

        // Check actual body size if already read
        if request.body.len() > limit {
            let message = self.config.error_message.clone().unwrap_or_else(|| {
                format!(
                    "Request body too large. Maximum allowed size is {}.",
                    format_size(limit)
                )
            });

            return Err(MiddlewareError::payload_too_large(&message));
        }

        // Store limit in context for later use
        request.context.insert(
            "body_limit".to_string(),
            serde_json::Value::Number(serde_json::Number::from(limit)),
        );

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -90 // Run very early, before body parsing
    }

    fn name(&self) -> &str {
        "body_limit"
    }
}

// ============================================================================
// Limited Body Reader
// ============================================================================

/// A body reader that enforces size limits during streaming.
pub struct LimitedBodyReader {
    data: Vec<u8>,
    limit: usize,
    _position: usize,
    exceeded: bool,
}

impl LimitedBodyReader {
    /// Create new limited body reader.
    pub fn new(limit: usize) -> Self {
        Self {
            data: Vec::new(),
            limit,
            _position: 0,
            exceeded: false,
        }
    }

    /// Write data, respecting the limit.
    pub fn write(&mut self, data: &[u8]) -> Result<usize, LimitExceededError> {
        let new_len = self.data.len() + data.len();
        if new_len > self.limit {
            self.exceeded = true;
            return Err(LimitExceededError {
                limit: self.limit,
                attempted: new_len,
            });
        }
        self.data.extend_from_slice(data);
        Ok(data.len())
    }

    /// Get the accumulated data.
    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }

    /// Check if limit was exceeded.
    pub fn exceeded(&self) -> bool {
        self.exceeded
    }

    /// Get current size.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get remaining capacity.
    pub fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.data.len())
    }
}

/// Error when body limit is exceeded.
#[derive(Debug, Clone)]
pub struct LimitExceededError {
    pub limit: usize,
    pub attempted: usize,
}

impl std::fmt::Display for LimitExceededError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Body limit exceeded: attempted {} bytes, limit is {} bytes",
            self.attempted, self.limit
        )
    }
}

impl std::error::Error for LimitExceededError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("100"), Some(100));
        assert_eq!(parse_size("1kb"), Some(1024));
        assert_eq!(parse_size("1KB"), Some(1024));
        assert_eq!(parse_size("10mb"), Some(10 * 1024 * 1024));
        assert_eq!(parse_size("1gb"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size("1.5mb"), Some((1.5 * 1024.0 * 1024.0) as usize));
        assert_eq!(parse_size("invalid"), None);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(10 * 1024 * 1024), "10.0MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0GB");
    }

    #[test]
    fn test_body_limit_config() {
        let config = BodyLimitConfig::new("10mb")
            .route("/upload", "100mb")
            .json("1mb")
            .multipart("50mb");

        assert_eq!(config.get_limit("/api/data", Some("application/json")), 1024 * 1024);
        assert_eq!(config.get_limit("/upload/file", None), 100 * 1024 * 1024);
        assert_eq!(
            config.get_limit("/api/upload", Some("multipart/form-data; boundary=---")),
            50 * 1024 * 1024
        );
        assert_eq!(config.get_limit("/other", None), 10 * 1024 * 1024);
    }

    #[test]
    fn test_limited_body_reader() {
        let mut reader = LimitedBodyReader::new(100);

        assert!(reader.write(b"hello").is_ok());
        assert_eq!(reader.len(), 5);
        assert_eq!(reader.remaining(), 95);

        // Try to exceed limit
        let large_data = vec![0u8; 200];
        assert!(reader.write(&large_data).is_err());
        assert!(reader.exceeded());
    }
}
