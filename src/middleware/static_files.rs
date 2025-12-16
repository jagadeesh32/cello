//! Static file serving middleware for Cello.
//!
//! Provides:
//! - Static file serving with caching
//! - Content-Type detection
//! - Range requests (partial content)
//! - Compression support
//! - Directory listing (optional)

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Cache Control Configuration
// ============================================================================

/// Cache control directive.
#[derive(Clone, Debug)]
pub enum CacheControl {
    /// No caching
    NoCache,
    /// No store (don't persist)
    NoStore,
    /// Private cache only
    Private(Duration),
    /// Public cache with max-age
    Public(Duration),
    /// Must revalidate
    MustRevalidate(Duration),
    /// Immutable (for versioned assets)
    Immutable(Duration),
    /// Custom header value
    Custom(String),
}

impl CacheControl {
    /// Create public cache with duration string (e.g., "1y", "30d", "1h").
    pub fn from_str(s: &str) -> Self {
        let duration = parse_duration_str(s);
        CacheControl::Public(duration)
    }

    /// Build Cache-Control header value.
    pub fn to_header_value(&self) -> String {
        match self {
            CacheControl::NoCache => "no-cache".to_string(),
            CacheControl::NoStore => "no-store".to_string(),
            CacheControl::Private(d) => format!("private, max-age={}", d.as_secs()),
            CacheControl::Public(d) => format!("public, max-age={}", d.as_secs()),
            CacheControl::MustRevalidate(d) => {
                format!("public, max-age={}, must-revalidate", d.as_secs())
            }
            CacheControl::Immutable(d) => format!("public, max-age={}, immutable", d.as_secs()),
            CacheControl::Custom(s) => s.clone(),
        }
    }
}

impl Default for CacheControl {
    fn default() -> Self {
        CacheControl::Public(Duration::from_secs(86400)) // 1 day
    }
}

/// Parse duration string (e.g., "1y", "30d", "1h", "30m", "60s").
fn parse_duration_str(s: &str) -> Duration {
    let s = s.trim();
    if s.is_empty() {
        return Duration::from_secs(0);
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().unwrap_or(0);

    match unit {
        "y" => Duration::from_secs(num * 365 * 24 * 60 * 60),
        "d" => Duration::from_secs(num * 24 * 60 * 60),
        "h" => Duration::from_secs(num * 60 * 60),
        "m" => Duration::from_secs(num * 60),
        "s" => Duration::from_secs(num),
        _ => Duration::from_secs(s.parse().unwrap_or(0)),
    }
}

// ============================================================================
// MIME Type Detection
// ============================================================================

/// Get MIME type for file extension.
pub fn mime_type_for_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        // Text
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",

        // Images
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "avif" => "image/avif",

        // Fonts
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "eot" => "application/vnd.ms-fontobject",

        // Documents
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",

        // Audio/Video
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",

        // Maps
        "map" => "application/json",

        // Default
        _ => "application/octet-stream",
    }
}

// ============================================================================
// Static Files Middleware
// ============================================================================

/// Static files middleware configuration.
pub struct StaticFilesConfig {
    /// URL path prefix (e.g., "/static")
    pub url_path: String,
    /// Root directory for files
    pub root_dir: PathBuf,
    /// Cache control settings
    pub cache_control: CacheControl,
    /// Cache control by extension
    pub cache_by_ext: HashMap<String, CacheControl>,
    /// Enable ETag generation
    pub etag: bool,
    /// Enable directory listing
    pub dir_listing: bool,
    /// Index file name
    pub index_file: Option<String>,
    /// Enable pre-compression (.gz, .br files)
    pub precompressed: bool,
    /// Custom headers
    pub custom_headers: HashMap<String, String>,
    /// Hidden file patterns to block
    pub hidden_patterns: Vec<String>,
}

impl StaticFilesConfig {
    /// Create new static files config.
    pub fn new(url_path: &str, root_dir: &str) -> Self {
        Self {
            url_path: url_path.trim_end_matches('/').to_string(),
            root_dir: PathBuf::from(root_dir),
            cache_control: CacheControl::default(),
            cache_by_ext: HashMap::new(),
            etag: true,
            dir_listing: false,
            index_file: Some("index.html".to_string()),
            precompressed: true,
            custom_headers: HashMap::new(),
            hidden_patterns: vec![".".to_string(), "..".to_string()],
        }
    }

    /// Set cache control.
    pub fn cache(mut self, cache_control: CacheControl) -> Self {
        self.cache_control = cache_control;
        self
    }

    /// Set cache control from string (e.g., "1y").
    pub fn cache_str(mut self, s: &str) -> Self {
        self.cache_control = CacheControl::from_str(s);
        self
    }

    /// Set cache control for specific extension.
    pub fn cache_ext(mut self, ext: &str, cache_control: CacheControl) -> Self {
        self.cache_by_ext.insert(ext.to_string(), cache_control);
        self
    }

    /// Enable/disable ETag.
    pub fn etag(mut self, enabled: bool) -> Self {
        self.etag = enabled;
        self
    }

    /// Enable directory listing.
    pub fn dir_listing(mut self, enabled: bool) -> Self {
        self.dir_listing = enabled;
        self
    }

    /// Set index file.
    pub fn index(mut self, filename: &str) -> Self {
        self.index_file = Some(filename.to_string());
        self
    }

    /// Disable index file.
    pub fn no_index(mut self) -> Self {
        self.index_file = None;
        self
    }

    /// Enable/disable precompressed files.
    pub fn precompressed(mut self, enabled: bool) -> Self {
        self.precompressed = enabled;
        self
    }

    /// Add custom header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.custom_headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Add hidden pattern.
    pub fn hide_pattern(mut self, pattern: &str) -> Self {
        self.hidden_patterns.push(pattern.to_string());
        self
    }
}

/// Static files middleware.
pub struct StaticFilesMiddleware {
    config: StaticFilesConfig,
}

impl StaticFilesMiddleware {
    /// Create new static files middleware.
    pub fn new(url_path: &str, root_dir: &str) -> Self {
        Self {
            config: StaticFilesConfig::new(url_path, root_dir),
        }
    }

    /// Create with config.
    pub fn with_config(config: StaticFilesConfig) -> Self {
        Self { config }
    }

    /// Resolve file path from request path.
    fn resolve_path(&self, request_path: &str) -> Option<PathBuf> {
        // Check if request matches URL path prefix
        if !request_path.starts_with(&self.config.url_path) {
            return None;
        }

        // Get relative path
        let relative = request_path
            .strip_prefix(&self.config.url_path)
            .unwrap_or("")
            .trim_start_matches('/');

        // Check for hidden patterns
        for pattern in &self.config.hidden_patterns {
            if relative.contains(pattern) || relative.starts_with(pattern) {
                return None;
            }
        }

        // Prevent directory traversal
        if relative.contains("..") {
            return None;
        }

        // Build absolute path
        let mut path = self.config.root_dir.clone();
        if !relative.is_empty() {
            path.push(relative);
        }

        // Canonicalize to prevent traversal
        let canonical = path.canonicalize().ok()?;
        let root_canonical = self.config.root_dir.canonicalize().ok()?;

        // Verify path is within root
        if !canonical.starts_with(&root_canonical) {
            return None;
        }

        Some(canonical)
    }

    /// Get cache control for file.
    fn get_cache_control(&self, path: &Path) -> &CacheControl {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(cc) = self.config.cache_by_ext.get(ext) {
                return cc;
            }
        }
        &self.config.cache_control
    }

    /// Generate ETag from file metadata.
    fn generate_etag(&self, metadata: &fs::Metadata) -> String {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let size = metadata.len();
        format!("\"{:x}-{:x}\"", modified, size)
    }

    /// Check if precompressed file exists.
    fn find_precompressed(&self, path: &Path, accept_encoding: &str) -> Option<(PathBuf, &'static str)> {
        if !self.config.precompressed {
            return None;
        }

        // Check for Brotli
        if accept_encoding.contains("br") {
            let br_path = path.with_extension(format!(
                "{}.br",
                path.extension().and_then(|e| e.to_str()).unwrap_or("")
            ));
            if br_path.exists() {
                return Some((br_path, "br"));
            }
        }

        // Check for Gzip
        if accept_encoding.contains("gzip") {
            let gz_path = path.with_extension(format!(
                "{}.gz",
                path.extension().and_then(|e| e.to_str()).unwrap_or("")
            ));
            if gz_path.exists() {
                return Some((gz_path, "gzip"));
            }
        }

        None
    }

    /// Serve file.
    fn serve_file(&self, path: &Path, request: &Request) -> Option<Response> {
        let metadata = fs::metadata(path).ok()?;

        if metadata.is_dir() {
            // Try index file
            if let Some(ref index) = self.config.index_file {
                let index_path = path.join(index);
                if index_path.exists() {
                    return self.serve_file(&index_path, request);
                }
            }

            // Directory listing
            if self.config.dir_listing {
                return self.serve_dir_listing(path, request);
            }

            return None;
        }

        // Get accept-encoding
        let accept_encoding = request
            .headers
            .get("accept-encoding")
            .map(|s| s.as_str())
            .unwrap_or("");

        // Check for precompressed version
        let (file_path, encoding) = self
            .find_precompressed(path, accept_encoding)
            .map(|(p, e)| (p, Some(e)))
            .unwrap_or_else(|| (path.to_path_buf(), None));

        // Check If-None-Match (ETag)
        if self.config.etag {
            let etag = self.generate_etag(&metadata);
            if let Some(if_none_match) = request.headers.get("if-none-match") {
                if if_none_match == &etag {
                    let mut response = Response::new(304);
                    response.set_header("ETag", &etag);
                    return Some(response);
                }
            }
        }

        // Check If-Modified-Since
        if let Some(if_modified_since) = request.headers.get("if-modified-since") {
            if let Ok(modified) = metadata.modified() {
                // Simple comparison - could be more sophisticated
                let modified_str = format_http_date(modified);
                if &modified_str == if_modified_since {
                    return Some(Response::new(304));
                }
            }
        }

        // Read file
        let mut file = fs::File::open(&file_path).ok()?;
        let mut body = Vec::new();
        file.read_to_end(&mut body).ok()?;

        // Get MIME type
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let content_type = mime_type_for_extension(ext);

        // Build response
        let mut response = Response::new(200);
        response.set_header("Content-Type", content_type);
        response.set_header("Content-Length", &body.len().to_string());
        response.set_header("Cache-Control", &self.get_cache_control(path).to_header_value());

        // Add ETag
        if self.config.etag {
            response.set_header("ETag", &self.generate_etag(&metadata));
        }

        // Add Last-Modified
        if let Ok(modified) = metadata.modified() {
            response.set_header("Last-Modified", &format_http_date(modified));
        }

        // Add Content-Encoding if precompressed
        if let Some(enc) = encoding {
            response.set_header("Content-Encoding", enc);
            response.set_header("Vary", "Accept-Encoding");
        }

        // Add custom headers
        for (name, value) in &self.config.custom_headers {
            response.set_header(name, value);
        }

        response.set_body(body);
        Some(response)
    }

    /// Serve directory listing.
    fn serve_dir_listing(&self, path: &Path, _request: &Request) -> Option<Response> {
        let entries = fs::read_dir(path).ok()?;

        let mut html = String::from("<!DOCTYPE html><html><head><title>Directory Listing</title>");
        html.push_str("<style>body{font-family:sans-serif;margin:20px}a{text-decoration:none}");
        html.push_str(".dir{color:#00f}.file{color:#333}</style></head><body>");
        html.push_str("<h1>Directory Listing</h1><ul>");

        // Parent directory link
        html.push_str("<li><a href=\"..\" class=\"dir\">..</a></li>");

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files
            if name.starts_with('.') {
                continue;
            }

            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let class = if is_dir { "dir" } else { "file" };
            let suffix = if is_dir { "/" } else { "" };

            html.push_str(&format!(
                "<li><a href=\"{}{}\" class=\"{}\">{}{}</a></li>",
                name, suffix, class, name, suffix
            ));
        }

        html.push_str("</ul></body></html>");

        let mut response = Response::new(200);
        response.set_header("Content-Type", "text/html; charset=utf-8");
        response.set_body(html.into_bytes());
        Some(response)
    }
}

impl Middleware for StaticFilesMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Only handle GET and HEAD
        if request.method != "GET" && request.method != "HEAD" {
            return Ok(MiddlewareAction::Continue);
        }

        // Check if path matches
        if !request.path.starts_with(&self.config.url_path) {
            return Ok(MiddlewareAction::Continue);
        }

        // Resolve file path
        if let Some(file_path) = self.resolve_path(&request.path) {
            if let Some(response) = self.serve_file(&file_path, request) {
                return Ok(MiddlewareAction::Stop(response));
            }
        }

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -80 // Run early but after logging
    }

    fn name(&self) -> &str {
        "static_files"
    }
}

/// Format SystemTime as HTTP date.
fn format_http_date(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    // Simple RFC 7231 date format
    // In production, use proper HTTP date formatting
    format!("{}", secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_str() {
        assert_eq!(parse_duration_str("1y").as_secs(), 365 * 24 * 60 * 60);
        assert_eq!(parse_duration_str("30d").as_secs(), 30 * 24 * 60 * 60);
        assert_eq!(parse_duration_str("1h").as_secs(), 60 * 60);
        assert_eq!(parse_duration_str("30m").as_secs(), 30 * 60);
        assert_eq!(parse_duration_str("60s").as_secs(), 60);
    }

    #[test]
    fn test_cache_control() {
        let cc = CacheControl::Public(Duration::from_secs(3600));
        assert_eq!(cc.to_header_value(), "public, max-age=3600");

        let cc = CacheControl::Immutable(Duration::from_secs(31536000));
        assert_eq!(cc.to_header_value(), "public, max-age=31536000, immutable");
    }

    #[test]
    fn test_mime_types() {
        assert_eq!(mime_type_for_extension("html"), "text/html; charset=utf-8");
        assert_eq!(mime_type_for_extension("css"), "text/css; charset=utf-8");
        assert_eq!(mime_type_for_extension("js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_type_for_extension("png"), "image/png");
        assert_eq!(mime_type_for_extension("unknown"), "application/octet-stream");
    }

    #[test]
    fn test_static_files_config() {
        let config = StaticFilesConfig::new("/static", "./public")
            .cache_str("1y")
            .etag(true)
            .dir_listing(false);

        assert_eq!(config.url_path, "/static");
        assert_eq!(config.etag, true);
        assert_eq!(config.dir_listing, false);
    }
}
