//! CSRF protection middleware for Cello.
//!
//! Provides:
//! - Double-submit cookie pattern
//! - Synchronizer token pattern
//! - Same-site cookie protection
//! - Origin/Referer validation

use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

use super::{Middleware, MiddlewareAction, MiddlewareError, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

/// Constant-time string comparison to prevent timing attacks on CSRF tokens.
#[inline]
fn secure_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

// ============================================================================
// CSRF Token Generation
// ============================================================================

type HmacSha256 = Hmac<Sha256>;

/// CSRF token with timestamp for expiration.
#[derive(Clone)]
pub struct CsrfToken {
    /// Token value
    pub token: String,
    /// Creation timestamp
    pub timestamp: u64,
}

impl CsrfToken {
    /// Generate a new CSRF token.
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            token: hex::encode(bytes),
            timestamp,
        }
    }

    /// Generate signed token with secret.
    pub fn generate_signed(secret: &[u8]) -> Self {
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 16] = rng.gen();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Create message: timestamp + random
        let message = format!("{}.{}", timestamp, hex::encode(random_bytes));

        // Sign with HMAC
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC key creation failed");
        mac.update(message.as_bytes());
        let signature = mac.finalize().into_bytes();

        // Token format: message.signature
        let token = format!("{}.{}", message, hex::encode(signature));

        Self { token, timestamp }
    }

    /// Verify a signed token.
    pub fn verify_signed(token: &str, secret: &[u8], max_age: Duration) -> bool {
        let parts: Vec<&str> = token.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            return false;
        }

        let signature = parts[0];
        let message = parts[1];

        // Verify signature
        let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC key creation failed");
        mac.update(message.as_bytes());

        let expected_signature = hex::decode(signature).unwrap_or_default();
        if mac.verify_slice(&expected_signature).is_err() {
            return false;
        }

        // Check timestamp
        let timestamp_str = message.split('.').next().unwrap_or("0");
        let timestamp: u64 = timestamp_str.parse().unwrap_or(0);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now - timestamp <= max_age.as_secs()
    }

    /// Check if token is expired.
    pub fn is_expired(&self, max_age: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now - self.timestamp > max_age.as_secs()
    }
}

// ============================================================================
// CSRF Configuration
// ============================================================================

/// CSRF protection method.
#[derive(Clone)]
pub enum CsrfMethod {
    /// Double-submit cookie (stateless)
    DoubleSubmit,
    /// Signed tokens (stateless with expiration)
    SignedToken {
        secret: Vec<u8>,
        max_age: Duration,
    },
    /// Origin/Referer header validation only
    OriginCheck,
}

impl Default for CsrfMethod {
    fn default() -> Self {
        CsrfMethod::DoubleSubmit
    }
}

/// CSRF middleware configuration.
#[derive(Clone)]
pub struct CsrfConfig {
    /// Protection method
    pub method: CsrfMethod,
    /// Cookie name for token
    pub cookie_name: String,
    /// Header name for token
    pub header_name: String,
    /// Form field name for token
    pub form_field: String,
    /// Cookie path
    pub cookie_path: String,
    /// Cookie secure flag
    pub cookie_secure: bool,
    /// Cookie SameSite attribute
    pub cookie_same_site: String,
    /// Safe HTTP methods (no validation required)
    pub safe_methods: Vec<String>,
    /// Paths to skip CSRF protection
    pub skip_paths: Vec<String>,
    /// Allowed origins for origin check
    pub allowed_origins: Vec<String>,
    /// Context key for storing token
    pub context_key: String,
}

impl CsrfConfig {
    /// Create new CSRF config.
    pub fn new() -> Self {
        Self {
            method: CsrfMethod::default(),
            cookie_name: "_csrf".to_string(),
            header_name: "X-CSRF-Token".to_string(),
            form_field: "_csrf".to_string(),
            cookie_path: "/".to_string(),
            cookie_secure: true,
            cookie_same_site: "Lax".to_string(),
            safe_methods: vec![
                "GET".to_string(),
                "HEAD".to_string(),
                "OPTIONS".to_string(),
                "TRACE".to_string(),
            ],
            skip_paths: Vec::new(),
            allowed_origins: Vec::new(),
            context_key: "csrf_token".to_string(),
        }
    }

    /// Use double-submit cookie method.
    pub fn double_submit(mut self) -> Self {
        self.method = CsrfMethod::DoubleSubmit;
        self
    }

    /// Use signed token method.
    pub fn signed_token(mut self, secret: &[u8], max_age: Duration) -> Self {
        self.method = CsrfMethod::SignedToken {
            secret: secret.to_vec(),
            max_age,
        };
        self
    }

    /// Use origin check only.
    pub fn origin_check(mut self) -> Self {
        self.method = CsrfMethod::OriginCheck;
        self
    }

    /// Set cookie name.
    pub fn cookie_name(mut self, name: &str) -> Self {
        self.cookie_name = name.to_string();
        self
    }

    /// Set header name.
    pub fn header_name(mut self, name: &str) -> Self {
        self.header_name = name.to_string();
        self
    }

    /// Set form field name.
    pub fn form_field(mut self, name: &str) -> Self {
        self.form_field = name.to_string();
        self
    }

    /// Set cookie path.
    pub fn cookie_path(mut self, path: &str) -> Self {
        self.cookie_path = path.to_string();
        self
    }

    /// Set cookie secure flag.
    pub fn cookie_secure(mut self, secure: bool) -> Self {
        self.cookie_secure = secure;
        self
    }

    /// Set cookie SameSite attribute.
    pub fn cookie_same_site(mut self, same_site: &str) -> Self {
        self.cookie_same_site = same_site.to_string();
        self
    }

    /// Add safe method (no CSRF check).
    pub fn safe_method(mut self, method: &str) -> Self {
        self.safe_methods.push(method.to_uppercase());
        self
    }

    /// Add path to skip CSRF protection.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }

    /// Add allowed origin for origin check.
    pub fn allowed_origin(mut self, origin: &str) -> Self {
        self.allowed_origins.push(origin.to_string());
        self
    }

    /// Set context key.
    pub fn context_key(mut self, key: &str) -> Self {
        self.context_key = key.to_string();
        self
    }
}

impl Default for CsrfConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// CSRF Middleware
// ============================================================================

/// CSRF protection middleware.
pub struct CsrfMiddleware {
    config: CsrfConfig,
}

impl CsrfMiddleware {
    /// Create new CSRF middleware with default config.
    pub fn new() -> Self {
        Self {
            config: CsrfConfig::default(),
        }
    }

    /// Create with config.
    pub fn with_config(config: CsrfConfig) -> Self {
        Self { config }
    }

    /// Create with double-submit cookie pattern.
    pub fn double_submit() -> Self {
        Self {
            config: CsrfConfig::new().double_submit(),
        }
    }

    /// Create with signed tokens.
    pub fn signed(secret: &[u8]) -> Self {
        Self {
            config: CsrfConfig::new().signed_token(secret, Duration::from_secs(3600)),
        }
    }

    /// Check if request method is safe.
    fn is_safe_method(&self, method: &str) -> bool {
        self.config.safe_methods.iter().any(|m| m == method)
    }

    /// Check if path should skip CSRF.
    fn should_skip(&self, path: &str) -> bool {
        self.config.skip_paths.iter().any(|p| path.starts_with(p))
    }

    /// Extract token from cookie.
    fn get_cookie_token(&self, request: &Request) -> Option<String> {
        if let Some(cookie_header) = request.headers.get("cookie") {
            for cookie in cookie_header.split(';') {
                let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
                if parts.len() == 2 && parts[0] == self.config.cookie_name {
                    return Some(parts[1].to_string());
                }
            }
        }
        None
    }

    /// Extract token from request (header or form).
    fn get_request_token(&self, request: &Request) -> Option<String> {
        // Try header first
        if let Some(token) = request.headers.get(&self.config.header_name.to_lowercase()) {
            return Some(token.clone());
        }

        // Try form field (from query params for now)
        if let Some(token) = request.query_params.get(&self.config.form_field) {
            return Some(token.clone());
        }

        // Could also parse body for form submissions
        None
    }

    /// Validate origin/referer headers.
    fn validate_origin(&self, request: &Request) -> bool {
        let host = request.headers.get("host").cloned().unwrap_or_default();

        // Check Origin header
        if let Some(origin) = request.headers.get("origin") {
            if self.config.allowed_origins.is_empty() {
                // If no allowed origins specified, check if origin matches host
                let origin_host = origin
                    .trim_start_matches("http://")
                    .trim_start_matches("https://");
                return origin_host.starts_with(&host);
            }
            return self.config.allowed_origins.contains(origin);
        }

        // Fall back to Referer header
        if let Some(referer) = request.headers.get("referer") {
            if self.config.allowed_origins.is_empty() {
                return referer.contains(&host);
            }
            return self.config.allowed_origins.iter().any(|o| referer.starts_with(o));
        }

        // No origin or referer - reject for unsafe methods
        false
    }

    /// Build cookie header value.
    fn build_cookie(&self, token: &str) -> String {
        let mut cookie = format!("{}={}", self.config.cookie_name, token);
        cookie.push_str(&format!("; Path={}", self.config.cookie_path));

        if self.config.cookie_secure {
            cookie.push_str("; Secure");
        }

        cookie.push_str("; HttpOnly");
        cookie.push_str(&format!("; SameSite={}", self.config.cookie_same_site));

        cookie
    }
}

impl Default for CsrfMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for CsrfMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Skip for safe methods
        if self.is_safe_method(&request.method) {
            // Generate token for safe methods (for use in forms)
            let token = match &self.config.method {
                CsrfMethod::SignedToken { secret, .. } => {
                    CsrfToken::generate_signed(secret).token
                }
                _ => CsrfToken::generate().token,
            };
            request.context.insert(
                self.config.context_key.clone(),
                serde_json::Value::String(token),
            );
            return Ok(MiddlewareAction::Continue);
        }

        // Skip for configured paths
        if self.should_skip(&request.path) {
            return Ok(MiddlewareAction::Continue);
        }

        // Validate based on method
        let valid = match &self.config.method {
            CsrfMethod::DoubleSubmit => {
                let cookie_token = self.get_cookie_token(request);
                let request_token = self.get_request_token(request);
                match (cookie_token, request_token) {
                    // Use constant-time comparison to prevent timing attacks
                    (Some(c), Some(r)) => secure_compare(&c, &r),
                    _ => false,
                }
            }
            CsrfMethod::SignedToken { secret, max_age } => {
                if let Some(token) = self.get_request_token(request) {
                    CsrfToken::verify_signed(&token, secret, *max_age)
                } else {
                    false
                }
            }
            CsrfMethod::OriginCheck => self.validate_origin(request),
        };

        if !valid {
            return Err(MiddlewareError::forbidden("CSRF token validation failed"));
        }

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Set CSRF cookie for safe methods
        if self.is_safe_method(&request.method) {
            if let Some(token) = request.context.get(&self.config.context_key) {
                if let Some(token_str) = token.as_str() {
                    response.set_header("Set-Cookie", &self.build_cookie(token_str));
                }
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -30 // Run after session/auth
    }

    fn name(&self) -> &str {
        "csrf"
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get CSRF token from request context.
pub fn get_csrf_token(request: &Request) -> Option<String> {
    request
        .context
        .get("csrf_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Generate hidden form field HTML for CSRF token.
pub fn csrf_hidden_field(request: &Request) -> String {
    if let Some(token) = get_csrf_token(request) {
        format!(r#"<input type="hidden" name="_csrf" value="{}">"#, token)
    } else {
        String::new()
    }
}

/// Generate meta tag for CSRF token (for AJAX).
pub fn csrf_meta_tag(request: &Request) -> String {
    if let Some(token) = get_csrf_token(request) {
        format!(r#"<meta name="csrf-token" content="{}">"#, token)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csrf_token_generation() {
        let token = CsrfToken::generate();
        assert_eq!(token.token.len(), 64); // 32 bytes = 64 hex chars
        assert!(!token.is_expired(Duration::from_secs(60)));
    }

    #[test]
    fn test_signed_token() {
        let secret = b"test-secret-key-for-csrf";
        let token = CsrfToken::generate_signed(secret);

        assert!(CsrfToken::verify_signed(
            &token.token,
            secret,
            Duration::from_secs(60)
        ));

        // Wrong secret should fail
        assert!(!CsrfToken::verify_signed(
            &token.token,
            b"wrong-secret",
            Duration::from_secs(60)
        ));
    }

    #[test]
    fn test_csrf_config() {
        let config = CsrfConfig::new()
            .cookie_name("my_csrf")
            .header_name("X-My-CSRF")
            .skip_path("/api/webhook")
            .allowed_origin("https://example.com");

        assert_eq!(config.cookie_name, "my_csrf");
        assert_eq!(config.header_name, "X-My-CSRF");
        assert!(config.skip_paths.contains(&"/api/webhook".to_string()));
        assert!(config.allowed_origins.contains(&"https://example.com".to_string()));
    }

    #[test]
    fn test_safe_methods() {
        let middleware = CsrfMiddleware::new();
        assert!(middleware.is_safe_method("GET"));
        assert!(middleware.is_safe_method("HEAD"));
        assert!(middleware.is_safe_method("OPTIONS"));
        assert!(!middleware.is_safe_method("POST"));
        assert!(!middleware.is_safe_method("PUT"));
        assert!(!middleware.is_safe_method("DELETE"));
    }
}
