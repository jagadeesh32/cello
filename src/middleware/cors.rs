//! CORS middleware for Cello.
//!
//! Provides:
//! - Cross-Origin Resource Sharing
//! - Preflight request handling
//! - Configurable origins, methods, headers
//! - Credentials support

use std::collections::HashSet;

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// CORS Configuration
// ============================================================================

/// Allowed origins configuration.
#[derive(Clone)]
pub enum AllowedOrigins {
    /// Allow all origins (*)
    Any,
    /// Allow specific origins
    List(HashSet<String>),
    /// Allow origins matching a pattern
    Pattern(fn(&str) -> bool),
    /// Mirror the request origin (with credentials)
    Mirror,
}

impl Default for AllowedOrigins {
    fn default() -> Self {
        AllowedOrigins::Any
    }
}

impl AllowedOrigins {
    /// Check if origin is allowed.
    pub fn is_allowed(&self, origin: &str) -> bool {
        match self {
            AllowedOrigins::Any => true,
            AllowedOrigins::List(list) => list.contains(origin),
            AllowedOrigins::Pattern(matcher) => matcher(origin),
            AllowedOrigins::Mirror => true,
        }
    }

    /// Get the header value for an origin.
    pub fn header_value(&self, origin: &str) -> Option<String> {
        match self {
            AllowedOrigins::Any => Some("*".to_string()),
            AllowedOrigins::List(list) => {
                if list.contains(origin) {
                    Some(origin.to_string())
                } else {
                    None
                }
            }
            AllowedOrigins::Pattern(matcher) => {
                if matcher(origin) {
                    Some(origin.to_string())
                } else {
                    None
                }
            }
            AllowedOrigins::Mirror => Some(origin.to_string()),
        }
    }
}

/// CORS middleware configuration.
#[derive(Clone)]
pub struct CorsConfig {
    /// Allowed origins
    pub origins: AllowedOrigins,
    /// Allowed HTTP methods
    pub methods: HashSet<String>,
    /// Allowed request headers
    pub allowed_headers: HashSet<String>,
    /// Headers exposed to the client
    pub exposed_headers: HashSet<String>,
    /// Allow credentials (cookies, auth)
    pub credentials: bool,
    /// Preflight cache duration (seconds)
    pub max_age: Option<u32>,
    /// Whether to pass preflight to next handler
    pub pass_preflight: bool,
    /// Success status for preflight (204 or 200)
    pub preflight_status: u16,
}

impl CorsConfig {
    /// Create new CORS config with permissive defaults.
    pub fn new() -> Self {
        let mut methods = HashSet::new();
        methods.insert("GET".to_string());
        methods.insert("POST".to_string());
        methods.insert("PUT".to_string());
        methods.insert("PATCH".to_string());
        methods.insert("DELETE".to_string());
        methods.insert("HEAD".to_string());
        methods.insert("OPTIONS".to_string());

        Self {
            origins: AllowedOrigins::Any,
            methods,
            allowed_headers: HashSet::new(),
            exposed_headers: HashSet::new(),
            credentials: false,
            max_age: Some(86400), // 24 hours
            pass_preflight: false,
            preflight_status: 204,
        }
    }

    /// Create strict CORS config (no origins allowed by default).
    pub fn strict() -> Self {
        Self {
            origins: AllowedOrigins::List(HashSet::new()),
            ..Self::new()
        }
    }

    /// Allow all origins.
    pub fn allow_any_origin(mut self) -> Self {
        self.origins = AllowedOrigins::Any;
        self
    }

    /// Allow specific origin.
    pub fn allow_origin(mut self, origin: &str) -> Self {
        match &mut self.origins {
            AllowedOrigins::List(list) => {
                list.insert(origin.to_string());
            }
            _ => {
                let mut list = HashSet::new();
                list.insert(origin.to_string());
                self.origins = AllowedOrigins::List(list);
            }
        }
        self
    }

    /// Allow multiple origins.
    pub fn allow_origins<I, S>(mut self, origins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let list: HashSet<String> = origins.into_iter().map(|s| s.as_ref().to_string()).collect();
        self.origins = AllowedOrigins::List(list);
        self
    }

    /// Allow origins matching pattern.
    pub fn allow_origin_pattern(mut self, matcher: fn(&str) -> bool) -> Self {
        self.origins = AllowedOrigins::Pattern(matcher);
        self
    }

    /// Mirror request origin.
    pub fn mirror_origin(mut self) -> Self {
        self.origins = AllowedOrigins::Mirror;
        self
    }

    /// Allow specific HTTP method.
    pub fn allow_method(mut self, method: &str) -> Self {
        self.methods.insert(method.to_uppercase());
        self
    }

    /// Allow multiple HTTP methods.
    pub fn allow_methods<I, S>(mut self, methods: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for method in methods {
            self.methods.insert(method.as_ref().to_uppercase());
        }
        self
    }

    /// Allow specific request header.
    pub fn allow_header(mut self, header: &str) -> Self {
        self.allowed_headers.insert(header.to_lowercase());
        self
    }

    /// Allow multiple request headers.
    pub fn allow_headers<I, S>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for header in headers {
            self.allowed_headers.insert(header.as_ref().to_lowercase());
        }
        self
    }

    /// Allow all request headers (reflect Access-Control-Request-Headers).
    pub fn allow_any_header(mut self) -> Self {
        self.allowed_headers.clear();
        self.allowed_headers.insert("*".to_string());
        self
    }

    /// Expose header to client.
    pub fn expose_header(mut self, header: &str) -> Self {
        self.exposed_headers.insert(header.to_string());
        self
    }

    /// Expose multiple headers to client.
    pub fn expose_headers<I, S>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for header in headers {
            self.exposed_headers.insert(header.as_ref().to_string());
        }
        self
    }

    /// Allow credentials (cookies, authorization).
    pub fn allow_credentials(mut self) -> Self {
        self.credentials = true;
        // Can't use * with credentials
        if let AllowedOrigins::Any = self.origins {
            self.origins = AllowedOrigins::Mirror;
        }
        self
    }

    /// Set preflight cache duration.
    pub fn max_age(mut self, seconds: u32) -> Self {
        self.max_age = Some(seconds);
        self
    }

    /// Disable preflight caching.
    pub fn no_max_age(mut self) -> Self {
        self.max_age = None;
        self
    }

    /// Pass preflight to next handler.
    pub fn pass_preflight(mut self) -> Self {
        self.pass_preflight = true;
        self
    }

    /// Set preflight response status (204 or 200).
    pub fn preflight_status(mut self, status: u16) -> Self {
        self.preflight_status = status;
        self
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// CORS Middleware
// ============================================================================

/// CORS middleware.
pub struct CorsMiddleware {
    config: CorsConfig,
}

impl CorsMiddleware {
    /// Create new CORS middleware with permissive defaults.
    pub fn new() -> Self {
        Self {
            config: CorsConfig::default(),
        }
    }

    /// Create with config.
    pub fn with_config(config: CorsConfig) -> Self {
        Self { config }
    }

    /// Create permissive CORS (allow all).
    pub fn permissive() -> Self {
        Self {
            config: CorsConfig::new()
                .allow_any_origin()
                .allow_any_header(),
        }
    }

    /// Check if request is a preflight request.
    fn is_preflight(&self, request: &Request) -> bool {
        request.method == "OPTIONS" && request.headers.contains_key("access-control-request-method")
    }

    /// Get origin from request.
    fn get_origin(&self, request: &Request) -> Option<String> {
        request.headers.get("origin").cloned()
    }

    /// Build preflight response.
    fn build_preflight_response(&self, request: &Request) -> Response {
        let mut response = Response::new(self.config.preflight_status);

        // Get origin
        let origin = self.get_origin(request).unwrap_or_default();

        // Access-Control-Allow-Origin
        if let Some(allowed_origin) = self.config.origins.header_value(&origin) {
            response.set_header("Access-Control-Allow-Origin", &allowed_origin);
        }

        // Access-Control-Allow-Methods
        let methods: Vec<&String> = self.config.methods.iter().collect();
        response.set_header("Access-Control-Allow-Methods", &methods.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));

        // Access-Control-Allow-Headers
        if self.config.allowed_headers.contains("*") {
            // Reflect requested headers
            if let Some(requested) = request.headers.get("access-control-request-headers") {
                response.set_header("Access-Control-Allow-Headers", requested);
            }
        } else if !self.config.allowed_headers.is_empty() {
            let headers: Vec<&String> = self.config.allowed_headers.iter().collect();
            response.set_header("Access-Control-Allow-Headers", &headers.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        }

        // Access-Control-Allow-Credentials
        if self.config.credentials {
            response.set_header("Access-Control-Allow-Credentials", "true");
        }

        // Access-Control-Max-Age
        if let Some(max_age) = self.config.max_age {
            response.set_header("Access-Control-Max-Age", &max_age.to_string());
        }

        // Vary header
        response.set_header("Vary", "Origin, Access-Control-Request-Method, Access-Control-Request-Headers");

        response
    }

    /// Add CORS headers to response.
    fn add_cors_headers(&self, request: &Request, response: &mut Response) {
        let origin = match self.get_origin(request) {
            Some(o) => o,
            None => return, // No origin header, not a CORS request
        };

        // Check if origin is allowed
        if !self.config.origins.is_allowed(&origin) {
            return;
        }

        // Access-Control-Allow-Origin
        if let Some(allowed_origin) = self.config.origins.header_value(&origin) {
            response.set_header("Access-Control-Allow-Origin", &allowed_origin);
        }

        // Access-Control-Allow-Credentials
        if self.config.credentials {
            response.set_header("Access-Control-Allow-Credentials", "true");
        }

        // Access-Control-Expose-Headers
        if !self.config.exposed_headers.is_empty() {
            let headers: Vec<&String> = self.config.exposed_headers.iter().collect();
            response.set_header("Access-Control-Expose-Headers", &headers.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        }

        // Vary header
        let vary = response
            .headers
            .get("Vary")
            .map(|v| format!("{}, Origin", v))
            .unwrap_or_else(|| "Origin".to_string());
        response.set_header("Vary", &vary);
    }
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for CorsMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Handle preflight requests
        if self.is_preflight(request) {
            if self.config.pass_preflight {
                // Let handler process it
                return Ok(MiddlewareAction::Continue);
            }

            // Return preflight response
            let response = self.build_preflight_response(request);
            return Ok(MiddlewareAction::Stop(response));
        }

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Add CORS headers to actual response
        if !self.is_preflight(request) {
            self.add_cors_headers(request, response);
        }
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -100 // Run very early
    }

    fn name(&self) -> &str {
        "cors"
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if domain matches pattern (e.g., *.example.com).
pub fn domain_matches(pattern: &str, domain: &str) -> bool {
    if pattern.starts_with("*.") {
        let suffix = &pattern[2..];
        domain.ends_with(suffix) || domain == &suffix[1..]
    } else {
        pattern == domain
    }
}

/// Extract domain from origin URL.
pub fn extract_domain(origin: &str) -> Option<&str> {
    origin
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split(':')
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_origins_any() {
        let origins = AllowedOrigins::Any;
        assert!(origins.is_allowed("https://example.com"));
        assert_eq!(origins.header_value("https://example.com"), Some("*".to_string()));
    }

    #[test]
    fn test_allowed_origins_list() {
        let mut list = HashSet::new();
        list.insert("https://example.com".to_string());
        let origins = AllowedOrigins::List(list);

        assert!(origins.is_allowed("https://example.com"));
        assert!(!origins.is_allowed("https://other.com"));
    }

    #[test]
    fn test_cors_config() {
        let config = CorsConfig::new()
            .allow_origin("https://example.com")
            .allow_methods(["GET", "POST"])
            .allow_headers(["Content-Type", "Authorization"])
            .allow_credentials()
            .max_age(3600);

        assert!(config.methods.contains("GET"));
        assert!(config.methods.contains("POST"));
        assert!(config.credentials);
        assert_eq!(config.max_age, Some(3600));
    }

    #[test]
    fn test_domain_matches() {
        assert!(domain_matches("*.example.com", "sub.example.com"));
        assert!(domain_matches("*.example.com", "deep.sub.example.com"));
        assert!(!domain_matches("*.example.com", "example.org"));
        assert!(domain_matches("example.com", "example.com"));
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://example.com"), Some("example.com"));
        assert_eq!(extract_domain("https://example.com:8080"), Some("example.com"));
        assert_eq!(extract_domain("http://localhost:3000"), Some("localhost"));
    }
}
