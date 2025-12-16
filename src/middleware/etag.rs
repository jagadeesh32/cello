//! ETag middleware for Cello.
//!
//! Provides:
//! - Automatic ETag generation
//! - Conditional request handling (If-None-Match)
//! - Weak and strong ETags
//! - Content-based hashing

use sha2::{Digest, Sha256};
use std::collections::HashSet;

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// ETag Generation
// ============================================================================

/// ETag strength.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtagStrength {
    /// Strong ETag (byte-for-byte identical)
    Strong,
    /// Weak ETag (semantically equivalent)
    Weak,
}

impl Default for EtagStrength {
    fn default() -> Self {
        EtagStrength::Weak
    }
}

/// ETag generation method.
#[derive(Clone)]
pub enum EtagMethod {
    /// Hash of response body (SHA-256)
    ContentHash,
    /// CRC32 checksum (faster, less collision resistant)
    Crc32,
    /// Length + hash prefix (very fast)
    LengthHash,
    /// Custom generator function
    Custom(fn(&[u8]) -> String),
}

impl Default for EtagMethod {
    fn default() -> Self {
        EtagMethod::LengthHash
    }
}

impl EtagMethod {
    /// Generate ETag value from body.
    pub fn generate(&self, body: &[u8]) -> String {
        match self {
            EtagMethod::ContentHash => {
                let mut hasher = Sha256::new();
                hasher.update(body);
                let hash = hasher.finalize();
                // Use first 16 bytes (32 hex chars)
                hex::encode(&hash[..16])
            }
            EtagMethod::Crc32 => {
                let crc = crc32fast::hash(body);
                format!("{:08x}", crc)
            }
            EtagMethod::LengthHash => {
                // Fast: length + first 8 bytes of SHA-256
                let len = body.len();
                let mut hasher = Sha256::new();
                hasher.update(body);
                let hash = hasher.finalize();
                format!("{:x}-{}", len, hex::encode(&hash[..8]))
            }
            EtagMethod::Custom(generator) => generator(body),
        }
    }
}

// ============================================================================
// ETag Configuration
// ============================================================================

/// ETag middleware configuration.
#[derive(Clone)]
pub struct EtagConfig {
    /// ETag generation method
    pub method: EtagMethod,
    /// ETag strength
    pub strength: EtagStrength,
    /// Minimum body size to generate ETag
    pub min_size: usize,
    /// Maximum body size to generate ETag (0 = no limit)
    pub max_size: usize,
    /// Content types to process (empty = all)
    pub content_types: HashSet<String>,
    /// Status codes to process
    pub status_codes: HashSet<u16>,
    /// Skip paths
    pub skip_paths: Vec<String>,
}

impl EtagConfig {
    /// Create new ETag config.
    pub fn new() -> Self {
        let mut status_codes = HashSet::new();
        status_codes.insert(200);
        status_codes.insert(201);

        Self {
            method: EtagMethod::default(),
            strength: EtagStrength::default(),
            min_size: 0,
            max_size: 10 * 1024 * 1024, // 10MB default max
            content_types: HashSet::new(),
            status_codes,
            skip_paths: Vec::new(),
        }
    }

    /// Set generation method.
    pub fn method(mut self, method: EtagMethod) -> Self {
        self.method = method;
        self
    }

    /// Use content hash method.
    pub fn content_hash(mut self) -> Self {
        self.method = EtagMethod::ContentHash;
        self
    }

    /// Use CRC32 method.
    pub fn crc32(mut self) -> Self {
        self.method = EtagMethod::Crc32;
        self
    }

    /// Use length + hash method.
    pub fn length_hash(mut self) -> Self {
        self.method = EtagMethod::LengthHash;
        self
    }

    /// Set ETag strength.
    pub fn strength(mut self, strength: EtagStrength) -> Self {
        self.strength = strength;
        self
    }

    /// Use strong ETags.
    pub fn strong(mut self) -> Self {
        self.strength = EtagStrength::Strong;
        self
    }

    /// Use weak ETags.
    pub fn weak(mut self) -> Self {
        self.strength = EtagStrength::Weak;
        self
    }

    /// Set minimum body size.
    pub fn min_size(mut self, size: usize) -> Self {
        self.min_size = size;
        self
    }

    /// Set maximum body size.
    pub fn max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }

    /// Add content type to process.
    pub fn content_type(mut self, content_type: &str) -> Self {
        self.content_types.insert(content_type.to_string());
        self
    }

    /// Add status code to process.
    pub fn status_code(mut self, code: u16) -> Self {
        self.status_codes.insert(code);
        self
    }

    /// Add path to skip.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }

    /// Format ETag value with strength.
    pub fn format_etag(&self, value: &str) -> String {
        match self.strength {
            EtagStrength::Strong => format!("\"{}\"", value),
            EtagStrength::Weak => format!("W/\"{}\"", value),
        }
    }
}

impl Default for EtagConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ETag Middleware
// ============================================================================

/// ETag middleware.
pub struct EtagMiddleware {
    config: EtagConfig,
}

impl EtagMiddleware {
    /// Create new ETag middleware.
    pub fn new() -> Self {
        Self {
            config: EtagConfig::default(),
        }
    }

    /// Create with config.
    pub fn with_config(config: EtagConfig) -> Self {
        Self { config }
    }

    /// Check if response should have ETag.
    fn should_process(&self, request: &Request, response: &Response) -> bool {
        // Check method
        if request.method != "GET" && request.method != "HEAD" {
            return false;
        }

        // Check status code
        if !self.config.status_codes.contains(&response.status) {
            return false;
        }

        // Check body size
        let body_len = response.body_bytes().len();
        if body_len < self.config.min_size {
            return false;
        }
        if self.config.max_size > 0 && body_len > self.config.max_size {
            return false;
        }

        // Check skip paths
        for path in &self.config.skip_paths {
            if request.path.starts_with(path) {
                return false;
            }
        }

        // Check content type if specified
        if !self.config.content_types.is_empty() {
            if let Some(ct) = response.headers.get("Content-Type") {
                let ct_lower = ct.to_lowercase();
                if !self
                    .config
                    .content_types
                    .iter()
                    .any(|t| ct_lower.contains(t))
                {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Check if ETag already exists
        if response.headers.contains_key("ETag") {
            return false;
        }

        true
    }

    /// Parse If-None-Match header.
    fn parse_if_none_match(header: &str) -> Vec<String> {
        if header == "*" {
            return vec!["*".to_string()];
        }

        header
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Check if ETags match (considering weak comparison).
    fn etags_match(client_etag: &str, server_etag: &str) -> bool {
        // Remove W/ prefix for weak comparison
        let client = client_etag
            .trim()
            .trim_start_matches("W/")
            .trim_matches('"');
        let server = server_etag
            .trim()
            .trim_start_matches("W/")
            .trim_matches('"');

        client == server
    }
}

impl Default for EtagMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for EtagMiddleware {
    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Check if we should process this response
        if !self.should_process(request, response) {
            return Ok(MiddlewareAction::Continue);
        }

        // Generate ETag
        let etag_value = self.config.method.generate(response.body_bytes());
        let etag = self.config.format_etag(&etag_value);

        // Check If-None-Match header
        if let Some(if_none_match) = request.headers.get("if-none-match") {
            let client_etags = Self::parse_if_none_match(if_none_match);

            // Check for wildcard or matching ETag
            let matches = client_etags.iter().any(|client_etag| {
                client_etag == "*" || Self::etags_match(client_etag, &etag)
            });

            if matches {
                // Return 304 Not Modified
                let mut not_modified = Response::new(304);
                not_modified.set_header("ETag", &etag);

                // Preserve cache headers
                if let Some(cc) = response.headers.get("Cache-Control") {
                    not_modified.set_header("Cache-Control", cc);
                }
                if let Some(exp) = response.headers.get("Expires") {
                    not_modified.set_header("Expires", exp);
                }
                if let Some(vary) = response.headers.get("Vary") {
                    not_modified.set_header("Vary", vary);
                }

                return Ok(MiddlewareAction::Stop(not_modified));
            }
        }

        // Add ETag header
        response.set_header("ETag", &etag);

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        80 // Run late, after response is built
    }

    fn name(&self) -> &str {
        "etag"
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate ETag for arbitrary data.
pub fn generate_etag(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    format!("W/\"{}\"", hex::encode(&hash[..16]))
}

/// Generate strong ETag for arbitrary data.
pub fn generate_strong_etag(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    format!("\"{}\"", hex::encode(&hash[..16]))
}

/// Check if ETag matches If-None-Match header.
pub fn matches_if_none_match(if_none_match: &str, etag: &str) -> bool {
    let client_etags = EtagMiddleware::parse_if_none_match(if_none_match);
    client_etags
        .iter()
        .any(|client_etag| client_etag == "*" || EtagMiddleware::etags_match(client_etag, etag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etag_generation() {
        let data = b"Hello, World!";

        let content_hash = EtagMethod::ContentHash.generate(data);
        assert!(!content_hash.is_empty());

        let crc = EtagMethod::Crc32.generate(data);
        assert_eq!(crc.len(), 8);

        let length_hash = EtagMethod::LengthHash.generate(data);
        assert!(length_hash.contains('-'));
    }

    #[test]
    fn test_etag_formatting() {
        let config = EtagConfig::new().weak();
        assert_eq!(config.format_etag("abc123"), "W/\"abc123\"");

        let config = EtagConfig::new().strong();
        assert_eq!(config.format_etag("abc123"), "\"abc123\"");
    }

    #[test]
    fn test_etag_matching() {
        assert!(EtagMiddleware::etags_match("\"abc123\"", "\"abc123\""));
        assert!(EtagMiddleware::etags_match("W/\"abc123\"", "\"abc123\""));
        assert!(EtagMiddleware::etags_match("\"abc123\"", "W/\"abc123\""));
        assert!(!EtagMiddleware::etags_match("\"abc123\"", "\"xyz789\""));
    }

    #[test]
    fn test_parse_if_none_match() {
        let etags = EtagMiddleware::parse_if_none_match("\"abc\", \"xyz\"");
        assert_eq!(etags.len(), 2);
        assert!(etags.contains(&"\"abc\"".to_string()));
        assert!(etags.contains(&"\"xyz\"".to_string()));

        let etags = EtagMiddleware::parse_if_none_match("*");
        assert_eq!(etags, vec!["*"]);
    }

    #[test]
    fn test_helper_functions() {
        let data = b"test data";
        let etag = generate_etag(data);
        assert!(etag.starts_with("W/"));

        let strong_etag = generate_strong_etag(data);
        assert!(!strong_etag.starts_with("W/"));
        assert!(strong_etag.starts_with('"'));
    }
}
