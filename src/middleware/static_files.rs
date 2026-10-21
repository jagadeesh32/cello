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
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{accepts_encoding, path_matches_skip, Middleware, MiddlewareAction, MiddlewareResult};
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
        "y" => Duration::from_secs(num.saturating_mul(365 * 24 * 60 * 60)),
        "d" => Duration::from_secs(num.saturating_mul(24 * 60 * 60)),
        "h" => Duration::from_secs(num.saturating_mul(60 * 60)),
        "m" => Duration::from_secs(num.saturating_mul(60)),
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
    /// Hidden path-segment prefixes: any segment of the requested path that
    /// starts with one of these is refused (default `"."` = dotfiles such as
    /// `.env`, `.git/`).
    pub hidden_patterns: Vec<String>,
    /// Largest file served, in bytes; bigger files are not served.
    pub max_file_size: u64,
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
            hidden_patterns: vec![".".to_string()],
            max_file_size: 256 * 1024 * 1024,
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
        self.custom_headers
            .insert(name.to_string(), value.to_string());
        self
    }

    /// Add hidden pattern.
    pub fn hide_pattern(mut self, pattern: &str) -> Self {
        self.hidden_patterns.push(pattern.to_string());
        self
    }

    /// Set the largest file size served (bytes).
    pub fn max_file_size(mut self, bytes: u64) -> Self {
        self.max_file_size = bytes;
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

    /// Whether the request path is under the mount, on a `/` boundary: a
    /// mount at `/static` serves `/static/x` but not `/static-other/x`.
    fn under_mount(&self, request_path: &str) -> bool {
        self.config.url_path.is_empty() || path_matches_skip(request_path, &self.config.url_path)
    }

    /// Resolve file path from request path.
    fn resolve_path(&self, request_path: &str) -> Option<PathBuf> {
        if !self.under_mount(request_path) {
            return None;
        }

        // Get relative path
        let relative = request_path
            .strip_prefix(&self.config.url_path)
            .unwrap_or("")
            .trim_start_matches('/');

        // Block null bytes (can truncate paths on some OSes)
        if relative.contains('\0') {
            return None;
        }

        // URL-decode the path to catch encoded traversal attempts (%2e%2e, %2f, etc.)
        let decoded = urlencoding::decode(relative).ok()?;
        let decoded = decoded.as_ref();

        // Hidden segments (on the decoded path): `a/.env` and `.git/config`
        // are refused, `app.js` is not.
        let hidden = decoded.split('/').any(|segment| {
            self.config
                .hidden_patterns
                .iter()
                .any(|p| !p.is_empty() && segment.starts_with(p.as_str()))
        });
        if hidden {
            return None;
        }

        // Prevent directory traversal: reject any path component that is ".."
        // Check both raw and decoded forms
        if relative.contains("..") || decoded.contains("..") {
            return None;
        }

        // Also reject backslashes (Windows-style traversal on some systems)
        if decoded.contains('\\') {
            return None;
        }

        // Build absolute path
        let mut path = self.config.root_dir.clone();
        if !decoded.is_empty() {
            path.push(decoded);
        }

        // Canonicalize to prevent traversal via symlinks or other tricks.
        // The security invariant is: canonical(requested) must be within canonical(root).
        let canonical = path.canonicalize().ok()?;
        if !self.within_root(&canonical) {
            return None;
        }

        Some(canonical)
    }

    /// Whether an already-canonical path lies inside the canonical root,
    /// compared component by component.
    fn within_root(&self, canonical: &Path) -> bool {
        let Ok(root_canonical) = self.config.root_dir.canonicalize() else {
            return false;
        };
        // On Windows, canonicalize() may return verbatim paths (\\?\C:\...) for
        // one path but not the other; strip that prefix, then compare as paths
        // (component-wise) so `C:\site\static-evil` is not inside `C:\site\static`.
        #[cfg(windows)]
        {
            let strip = |p: &Path| -> PathBuf {
                let s = p.to_string_lossy();
                PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
            };
            strip(canonical).starts_with(strip(&root_canonical))
        }
        #[cfg(not(windows))]
        {
            canonical.starts_with(&root_canonical)
        }
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
        format!("\"{modified:x}-{size:x}\"")
    }

    /// Find a precompressed sibling (`x.js.br`, `x.js.gz`) the client accepts.
    /// The sibling is canonicalized and re-checked against the root: a symlink
    /// planted next to a real asset must not be served from outside it.
    fn find_precompressed(
        &self,
        path: &Path,
        accept_encoding: &str,
    ) -> Option<(PathBuf, &'static str)> {
        if !self.config.precompressed {
            return None;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        for (coding, suffix) in [("br", "br"), ("gzip", "gz")] {
            if !accepts_encoding(accept_encoding, coding) {
                continue;
            }
            let candidate = path.with_extension(format!("{ext}.{suffix}"));
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if self.within_root(&canonical) && canonical.is_file() {
                return Some((canonical, coding));
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
                // The index name is configuration, but it is joined onto a
                // request path: re-check the result like any other path.
                let index_path = path.join(index);
                if let Ok(canonical) = index_path.canonicalize() {
                    if self.within_root(&canonical) && canonical.is_file() {
                        return self.serve_file(&canonical, request);
                    }
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

        // The file is not read here: this hook runs on the server thread, and a
        // large or cold file would stall every connection. The response carries
        // the path and the server reads it with `tokio::fs` (blocking pool).
        let served_len = fs::metadata(&file_path).ok()?.len();
        if served_len > self.config.max_file_size {
            return None;
        }

        // Get MIME type
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let content_type = mime_type_for_extension(ext);

        // Build response
        let mut response = Response::new(200);
        response.set_header("Content-Type", content_type);
        if ext.eq_ignore_ascii_case("svg") {
            // An SVG opened directly is a document that can run script; served
            // as a static asset it never needs to.
            response.set_header(
                "Content-Security-Policy",
                "default-src 'none'; style-src 'unsafe-inline'; sandbox",
            );
        }
        response.set_header(
            "Cache-Control",
            &self.get_cache_control(path).to_header_value(),
        );

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
        }
        if self.config.precompressed {
            super::add_vary(&mut response, "Accept-Encoding");
        }

        // Add custom headers
        for (name, value) in &self.config.custom_headers {
            response.set_header(name, value);
        }

        if request.method == "HEAD" {
            response.set_header("Content-Length", &served_len.to_string());
        } else {
            response.defer_file_body(file_path.to_string_lossy().into_owned());
        }
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
            // File names are attacker-influenced (uploads): escape for both the
            // URL and the HTML text.
            let href = urlencoding::encode(&name);
            let text = html_escape(&name);

            html.push_str(&format!(
                "<li><a href=\"{href}{suffix}\" class=\"{class}\">{text}{suffix}</a></li>"
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
        if !self.under_mount(&request.path) {
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

/// Format SystemTime as an HTTP date (RFC 9110 IMF-fixdate).
fn format_http_date(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(time)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

/// Minimal HTML text escaping.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
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
        assert_eq!(
            mime_type_for_extension("js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(mime_type_for_extension("png"), "image/png");
        assert_eq!(
            mime_type_for_extension("unknown"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_resolve_path_rules() {
        let dir = std::env::temp_dir().join(format!("cello-static-{}", std::process::id()));
        let _ = fs::create_dir_all(dir.join("css"));
        fs::write(dir.join("app.js"), "x").unwrap();
        fs::write(dir.join("css/site.css"), "x").unwrap();
        fs::write(dir.join(".env"), "SECRET=1").unwrap();
        let mw = StaticFilesMiddleware::new("/static", dir.to_str().unwrap());

        // Dotted names are ordinary files.
        assert!(mw.resolve_path("/static/app.js").is_some());
        assert!(mw.resolve_path("/static/css/site.css").is_some());
        // Dotfiles and traversal are refused.
        assert!(mw.resolve_path("/static/.env").is_none());
        assert!(mw.resolve_path("/static/%2eenv").is_none());
        assert!(mw.resolve_path("/static/../etc/passwd").is_none());
        // The mount needs a `/` boundary.
        assert!(mw.resolve_path("/static-other/app.js").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_name_cannot_leave_the_root() {
        let base = std::env::temp_dir().join(format!("cello-static-idx-{}", std::process::id()));
        let root = base.join("public");
        let _ = fs::create_dir_all(root.join("docs"));
        fs::write(base.join("secret.txt"), "SECRET").unwrap();
        fs::write(root.join("docs/index.html"), "ok").unwrap();
        let request = Request::default();

        let escape = StaticFilesMiddleware::with_config(
            StaticFilesConfig::new("/static", root.to_str().unwrap()).index("../../secret.txt"),
        );
        assert!(escape.serve_file(&root.join("docs"), &request).is_none());

        let normal = StaticFilesMiddleware::with_config(
            StaticFilesConfig::new("/static", root.to_str().unwrap()).index("index.html"),
        );
        let response = normal.serve_file(&root.join("docs"), &request).unwrap();
        assert_eq!(response.status, 200);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_html_escape_and_http_date() {
        assert_eq!(
            html_escape("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        assert_eq!(
            format_http_date(UNIX_EPOCH),
            "Thu, 01 Jan 1970 00:00:00 GMT"
        );
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
