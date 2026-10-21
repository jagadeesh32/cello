//! CSRF protection middleware for Cello.
//!
//! Provides:
//! - Double-submit cookie pattern
//! - Synchronizer token pattern
//! - Same-site cookie protection
//! - Origin/Referer validation

use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::Rng;
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

use super::{path_matches_skip, Middleware, MiddlewareAction, MiddlewareError, MiddlewareResult};
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

/// Extract the `host[:port]` authority from a URL-ish string, stripping the scheme
/// and any path/query/fragment. Used for exact same-origin comparison.
///
/// `https://example.com:8443/login?x=1` -> `example.com:8443`
#[inline]
fn origin_authority(value: &str) -> &str {
    let without_scheme = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value);
    // Authority ends at the first path/query/fragment delimiter.
    match without_scheme.find(['/', '?', '#']) {
        Some(idx) => &without_scheme[..idx],
        None => without_scheme,
    }
}

// ============================================================================
// CSRF Token Generation
// ============================================================================

type HmacSha256 = Hmac<Sha256>;

/// Default CSRF cookie name (see `CsrfConfig::new`).
pub const DEFAULT_COOKIE_NAME: &str = "__Host-csrf";

/// CSRF token with timestamp for expiration.
#[derive(Clone)]
pub struct CsrfToken {
    /// Token value
    pub token: String,
    /// Creation timestamp
    pub timestamp: u64,
}

impl CsrfToken {
    /// Generate a new CSRF token using cryptographically secure randomness.
    pub fn generate() -> Self {
        let bytes: [u8; 32] = OsRng.gen();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            token: hex::encode(bytes),
            timestamp,
        }
    }

    /// Generate signed token with secret using cryptographically secure randomness.
    pub fn generate_signed(secret: &[u8]) -> Self {
        let random_bytes: [u8; 16] = OsRng.gen();
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

        // A timestamp in the future is not "fresh"; `now - timestamp` would
        // wrap (release) or panic (debug) instead.
        timestamp <= now && now - timestamp <= max_age.as_secs()
    }

    /// Check if token is expired.
    pub fn is_expired(&self, max_age: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp) > max_age.as_secs()
    }
}

// ============================================================================
// CSRF Configuration
// ============================================================================

/// CSRF protection method.
#[derive(Clone, Default)]
pub enum CsrfMethod {
    /// Double-submit cookie (stateless)
    #[default]
    DoubleSubmit,
    /// Signed tokens (stateless with expiration)
    SignedToken { secret: Vec<u8>, max_age: Duration },
    /// Origin/Referer header validation only
    OriginCheck,
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
            // `__Host-`: the browser accepts this cookie only from this exact
            // host, over HTTPS, with `Path=/` and no `Domain` — a sibling
            // subdomain cannot plant a token the double-submit check would
            // then accept.
            cookie_name: DEFAULT_COOKIE_NAME.to_string(),
            header_name: "X-CSRF-Token".to_string(),
            form_field: "_csrf".to_string(),
            cookie_path: "/".to_string(),
            cookie_secure: true,
            cookie_same_site: "Lax".to_string(),
            // TRACE is deliberately not "safe" here: it is never needed by
            // an application and must not bypass validation.
            safe_methods: vec!["GET".to_string(), "HEAD".to_string(), "OPTIONS".to_string()],
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
    ///
    /// A `__Host-` / `__Secure-` cookie name whose requirements the config does
    /// not meet (browsers drop such a cookie) loses its prefix, so the cookie
    /// still works, e.g. over plain HTTP in development.
    pub fn with_config(mut config: CsrfConfig) -> Self {
        let name = &config.cookie_name;
        let host_ok = config.cookie_secure && config.cookie_path == "/";
        if let Some(rest) = name.strip_prefix("__Host-").filter(|_| !host_ok) {
            config.cookie_name = rest.to_string();
        } else if let Some(rest) = name
            .strip_prefix("__Secure-")
            .filter(|_| !config.cookie_secure)
        {
            config.cookie_name = rest.to_string();
        }
        Self { config }
    }

    /// Store `token` in a session this response persists that has none yet,
    /// so the next unsafe request is checked against the session (synchronizer
    /// token) rather than the cookie. Covers the request that creates the
    /// session (login), without creating sessions for anonymous visitors.
    fn bind_to_session(&self, request: &Request, token: &str) {
        let Some(handle) = request.session.as_ref() else {
            return;
        };
        let mut st = handle.lock();
        let persisted = !st.destroyed && (st.modified || !st.is_new);
        if persisted && !st.data.contains(SESSION_KEY) {
            st.data.set(SESSION_KEY, token);
            st.modified = true;
        }
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
        self.config
            .skip_paths
            .iter()
            .any(|p| path_matches_skip(path, p))
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

    /// Extract the submitted token: the header, else the form field in a
    /// `application/x-www-form-urlencoded` or `multipart/form-data` body (what
    /// `csrf_hidden_field` renders). The query string is not accepted: a token
    /// there leaks into access logs, `Referer` and browser history.
    fn get_request_token(&self, request: &Request) -> Option<String> {
        if let Some(token) = request.headers.get(&self.config.header_name.to_lowercase()) {
            return Some(token.clone());
        }

        let content_type = request.headers.get("content-type")?.to_ascii_lowercase();
        if content_type.starts_with("application/x-www-form-urlencoded") {
            return crate::multipart::parse_urlencoded(request.body_bytes())
                .ok()
                .and_then(|mut form| form.remove(&self.config.form_field));
        }
        if content_type.starts_with("multipart/form-data") {
            // Take the boundary from the original (case-preserving) header,
            // right after `boundary=`.
            let original = request.headers.get("content-type")?;
            let at = content_type.find("boundary=")? + "boundary=".len();
            let boundary = original
                .get(at..)?
                .split(';')
                .next()?
                .trim()
                .trim_matches('"');
            return multipart_field(request.body_bytes(), boundary, &self.config.form_field);
        }
        None
    }

    /// The token this request should carry for later unsafe requests: the
    /// session's token when a session exists, else the existing cookie, else new.
    fn issue_token(&self, request: &Request) -> String {
        if let Some(handle) = request.session.as_ref() {
            let mut st = handle.lock();
            if !st.is_new {
                if let Some(t) = st.data.get_string(SESSION_KEY) {
                    // A signed token expires; replace it instead of returning a
                    // token that every later submission would fail on.
                    let still_valid = match &self.config.method {
                        CsrfMethod::SignedToken { secret, max_age } => {
                            CsrfToken::verify_signed(&t, secret, *max_age)
                        }
                        _ => true,
                    };
                    if still_valid {
                        return t;
                    }
                }
                let t = self.new_token();
                st.data.set(SESSION_KEY, &t);
                st.modified = true;
                return t;
            }
        }
        if let Some(existing) = self.get_cookie_token(request) {
            let reusable = match &self.config.method {
                CsrfMethod::SignedToken { secret, max_age } => {
                    CsrfToken::verify_signed(&existing, secret, *max_age)
                }
                _ => existing.len() == 64 && existing.bytes().all(|b| b.is_ascii_hexdigit()),
            };
            // Reusing the cookie keeps forms in other open tabs valid.
            if reusable {
                return existing;
            }
        }
        self.new_token()
    }

    fn new_token(&self) -> String {
        match &self.config.method {
            CsrfMethod::SignedToken { secret, .. } => CsrfToken::generate_signed(secret).token,
            _ => CsrfToken::generate().token,
        }
    }

    /// Validate origin/referer headers.
    ///
    /// SECURITY: comparisons are done on the full authority (`host[:port]`) and must
    /// match *exactly*. Earlier versions used `starts_with`/`contains`, which allowed
    /// `example.com.evil.com` (Origin) or `evil.com/?x=example.com` (Referer) to pass.
    fn validate_origin(&self, request: &Request) -> bool {
        let host = match request.headers.get("host") {
            // Empty Host cannot be verified against; reject rather than match "".
            Some(h) if !h.is_empty() => h.as_str(),
            _ => return false,
        };

        // Check Origin header first (most reliable — set by the browser, not spoofable
        // from script for cross-origin requests).
        if let Some(origin) = request.headers.get("origin") {
            if !self.config.allowed_origins.is_empty() {
                // Exact full-origin match against the configured allow-list.
                return self.config.allowed_origins.iter().any(|o| o == origin);
            }
            // Same-origin: the Origin authority must equal the Host exactly.
            return origin_authority(origin) == host;
        }

        // Fall back to Referer header.
        if let Some(referer) = request.headers.get("referer") {
            let ref_authority = origin_authority(referer);
            if !self.config.allowed_origins.is_empty() {
                return self
                    .config
                    .allowed_origins
                    .iter()
                    .any(|o| origin_authority(o) == ref_authority);
            }
            return ref_authority == host;
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

        // CSRF double-submit cookie must NOT be HttpOnly — JavaScript needs to
        // read this value to send it back in the X-CSRF-Token request header.
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
            // Token for forms/AJAX rendered by this response.
            let token = self.issue_token(request);
            if self.get_cookie_token(request).as_deref() != Some(token.as_str()) {
                // New to this visitor: the response is visitor-specific.
                request
                    .context
                    .insert(FRESH_TOKEN_KEY.to_string(), serde_json::Value::Bool(true));
            }
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

        // With an established session that holds a token, the submitted token
        // must equal it (synchronizer token): a cookie an attacker managed to
        // plant (e.g. from a sibling subdomain) is then useless.
        let session_token = request.session.as_ref().and_then(|h| {
            let st = h.lock();
            if st.is_new {
                None
            } else {
                st.data.get_string(SESSION_KEY)
            }
        });

        // Validate based on method
        let valid = match &self.config.method {
            CsrfMethod::OriginCheck => self.validate_origin(request),
            method => {
                let request_token = self.get_request_token(request);
                let cookie_token = self.get_cookie_token(request);
                match (request_token, session_token) {
                    (Some(r), Some(s)) => {
                        secure_compare(&r, &s)
                            && match method {
                                CsrfMethod::SignedToken { secret, max_age } => {
                                    CsrfToken::verify_signed(&r, secret, *max_age)
                                }
                                _ => true,
                            }
                    }
                    // No session token: double-submit. The submitted token must
                    // match the cookie (constant-time), and in signed mode also
                    // carry a valid, unexpired signature — a signed token alone is
                    // no longer a bearer credential.
                    (Some(r), None) => {
                        cookie_token.is_some_and(|c| secure_compare(&c, &r))
                            && match method {
                                CsrfMethod::SignedToken { secret, max_age } => {
                                    CsrfToken::verify_signed(&r, secret, *max_age)
                                }
                                _ => true,
                            }
                    }
                    (None, _) => false,
                }
            }
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
                    // Only (re)send the cookie when it changes.
                    if self.get_cookie_token(request).as_deref() != Some(token_str) {
                        response.append_header("Set-Cookie", &self.build_cookie(token_str));
                    }
                    self.bind_to_session(request, token_str);
                }
            }
        } else if !matches!(self.config.method, CsrfMethod::OriginCheck) && response.status < 400 {
            // An unsafe request that passed the double-submit check (its token
            // equals the cookie): bind that token to the session it may create.
            if let Some(cookie) = self.get_cookie_token(request) {
                self.bind_to_session(request, &cookie);
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

/// Request-context flag: this request issued a token the client does not hold
/// yet (the response cache must not share such a response).
pub const FRESH_TOKEN_KEY: &str = "__csrf_fresh_token";

/// Session key holding the synchronizer token.
const SESSION_KEY: &str = "_csrf_token";

/// Extract a simple text field from a `multipart/form-data` body.
fn multipart_field(body: &[u8], boundary: &str, field: &str) -> Option<String> {
    let delimiter = format!("--{boundary}");
    let text = String::from_utf8_lossy(body);
    let wanted = format!("name=\"{field}\"");
    for part in text.split(delimiter.as_str()) {
        let Some((head, value)) = part.split_once("\r\n\r\n") else {
            continue;
        };
        let disposition_matches = head.lines().any(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("content-disposition:") && line.split(';').any(|p| p.trim() == wanted)
        });
        if disposition_matches {
            return Some(value.trim_end_matches("\r\n").to_string());
        }
    }
    None
}

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
        // Tokens are hex (or `ts.hex.hex`), so they need no HTML escaping.
        format!(r#"<input type="hidden" name="_csrf" value="{token}">"#)
    } else {
        String::new()
    }
}

/// Generate meta tag for CSRF token (for AJAX).
pub fn csrf_meta_tag(request: &Request) -> String {
    if let Some(token) = get_csrf_token(request) {
        format!(r#"<meta name="csrf-token" content="{token}">"#)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signed_token_future_timestamp_rejected() {
        let secret = b"k";
        let message = format!("{}.{}", u64::MAX - 5, "00");
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(message.as_bytes());
        let token = format!("{}.{}", message, hex::encode(mac.finalize().into_bytes()));
        assert!(!CsrfToken::verify_signed(
            &token,
            secret,
            Duration::from_secs(60)
        ));
    }

    #[test]
    fn test_token_read_from_form_bodies() {
        let mw = CsrfMiddleware::new();
        let mut req = Request::new("POST", "/f");
        req.headers.insert(
            "content-type".into(),
            "application/x-www-form-urlencoded".into(),
        );
        req.body = b"a=1&_csrf=abc".to_vec();
        assert_eq!(mw.get_request_token(&req).as_deref(), Some("abc"));

        let mut req = Request::new("POST", "/f");
        req.headers.insert(
            "content-type".into(),
            "multipart/form-data; boundary=XyZ".into(),
        );
        req.body = b"--XyZ\r\nContent-Disposition: form-data; name=\"x\"\r\n\r\n1\r\n--XyZ\r\nContent-Disposition: form-data; name=\"_csrf\"\r\n\r\ntok\r\n--XyZ--\r\n".to_vec();
        assert_eq!(mw.get_request_token(&req).as_deref(), Some("tok"));

        // Query string is not accepted.
        let mut req = Request::new("POST", "/f");
        req.query_params.insert("_csrf".into(), "abc".into());
        assert!(mw.get_request_token(&req).is_none());
    }

    #[test]
    fn test_trace_is_not_safe() {
        let mw = CsrfMiddleware::new();
        assert!(!mw.is_safe_method("TRACE"));
    }

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
        assert!(config
            .allowed_origins
            .contains(&"https://example.com".to_string()));
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

    #[test]
    fn test_origin_authority_strips_scheme_and_path() {
        assert_eq!(origin_authority("https://example.com"), "example.com");
        assert_eq!(origin_authority("http://example.com"), "example.com");
        assert_eq!(
            origin_authority("https://example.com:8443/login?x=1"),
            "example.com:8443"
        );
        // No scheme -> value is already an authority (Host header form).
        assert_eq!(origin_authority("example.com:80"), "example.com:80");
    }

    #[test]
    fn test_origin_authority_rejects_suffix_attack() {
        // Regression: `example.com.evil.com` must NOT be treated as `example.com`.
        let host = "example.com";
        assert_ne!(origin_authority("https://example.com.evil.com"), host);
        assert_ne!(origin_authority("https://evil.com"), host);
        assert_eq!(origin_authority("https://example.com"), host);
    }
}
