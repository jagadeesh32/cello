//! Authentication middleware for Cello.
//!
//! Provides:
//! - JWT authentication
//! - Basic authentication
//! - API Key authentication

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

use super::{Middleware, MiddlewareAction, MiddlewareError, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

/// Constant-time string comparison to prevent timing attacks.
/// This is critical for password and token validation.
#[inline]
fn secure_compare(a: &str, b: &str) -> bool {
    // First check if lengths match (this can leak length, but that's acceptable)
    if a.len() != b.len() {
        return false;
    }
    // Use constant-time comparison for the actual bytes
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

// ============================================================================
// JWT Authentication
// ============================================================================

/// JWT claims structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Expiration time (Unix timestamp)
    pub exp: u64,
    /// Issued at time (Unix timestamp)
    pub iat: u64,
    /// Issuer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Audience
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Custom claims
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl JwtClaims {
    /// Create new JWT claims with subject and expiration duration.
    pub fn new(subject: &str, expires_in: Duration) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            sub: subject.to_string(),
            exp: now + expires_in.as_secs(),
            iat: now,
            iss: None,
            aud: None,
            custom: HashMap::new(),
        }
    }

    /// Add issuer claim.
    pub fn with_issuer(mut self, issuer: &str) -> Self {
        self.iss = Some(issuer.to_string());
        self
    }

    /// Add audience claim.
    pub fn with_audience(mut self, audience: &str) -> Self {
        self.aud = Some(audience.to_string());
        self
    }

    /// Add custom claim.
    pub fn with_claim(mut self, key: &str, value: serde_json::Value) -> Self {
        self.custom.insert(key.to_string(), value);
        self
    }

    /// Check if token is expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.exp < now
    }
}

/// JWT configuration.
#[derive(Clone)]
pub struct JwtConfig {
    /// Secret key for HMAC algorithms
    pub secret: Vec<u8>,
    /// Algorithm to use
    pub algorithm: Algorithm,
    /// Token issuer (optional)
    pub issuer: Option<String>,
    /// Token audience (optional)
    pub audience: Option<String>,
    /// Leeway for expiration validation (seconds)
    pub leeway: u64,
}

impl JwtConfig {
    /// Create new JWT config with secret.
    pub fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.to_vec(),
            algorithm: Algorithm::HS256,
            issuer: None,
            audience: None,
            leeway: 60,
        }
    }

    /// Set algorithm.
    pub fn algorithm(mut self, alg: Algorithm) -> Self {
        self.algorithm = alg;
        self
    }

    /// Set issuer.
    pub fn issuer(mut self, issuer: &str) -> Self {
        self.issuer = Some(issuer.to_string());
        self
    }

    /// Set audience.
    pub fn audience(mut self, audience: &str) -> Self {
        self.audience = Some(audience.to_string());
        self
    }

    /// Set expiration leeway.
    pub fn leeway(mut self, seconds: u64) -> Self {
        self.leeway = seconds;
        self
    }

    /// Create JWT token from claims.
    pub fn encode(&self, claims: &JwtClaims) -> Result<String, JwtError> {
        let header = Header::new(self.algorithm);
        let key = EncodingKey::from_secret(&self.secret);
        encode(&header, claims, &key).map_err(|e| JwtError::EncodingFailed(e.to_string()))
    }

    /// Decode and validate JWT token.
    pub fn decode(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let mut validation = Validation::new(self.algorithm);
        validation.leeway = self.leeway;

        if let Some(ref iss) = self.issuer {
            validation.set_issuer(&[iss]);
        }
        if let Some(ref aud) = self.audience {
            validation.set_audience(&[aud]);
        }

        let key = DecodingKey::from_secret(&self.secret);
        let data = decode::<JwtClaims>(token, &key, &validation)
            .map_err(|e| JwtError::ValidationFailed(e.to_string()))?;

        Ok(data.claims)
    }
}

/// JWT authentication errors.
#[derive(Debug, Clone)]
pub enum JwtError {
    MissingToken,
    InvalidFormat,
    EncodingFailed(String),
    ValidationFailed(String),
    Expired,
}

impl std::fmt::Display for JwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JwtError::MissingToken => write!(f, "Missing authorization token"),
            JwtError::InvalidFormat => write!(f, "Invalid token format"),
            JwtError::EncodingFailed(e) => write!(f, "Token encoding failed: {}", e),
            JwtError::ValidationFailed(e) => write!(f, "Token validation failed: {}", e),
            JwtError::Expired => write!(f, "Token has expired"),
        }
    }
}

impl std::error::Error for JwtError {}

/// JWT authentication middleware.
pub struct JwtAuth {
    config: JwtConfig,
    header_name: String,
    token_prefix: String,
    query_param: Option<String>,
    cookie_name: Option<String>,
    skip_paths: Vec<String>,
    claims_key: String,
}

impl JwtAuth {
    /// Create new JWT auth middleware.
    pub fn new(config: JwtConfig) -> Self {
        Self {
            config,
            header_name: "Authorization".to_string(),
            token_prefix: "Bearer ".to_string(),
            query_param: None,
            cookie_name: None,
            skip_paths: Vec::new(),
            claims_key: "jwt_claims".to_string(),
        }
    }

    /// Set custom header name.
    pub fn header_name(mut self, name: &str) -> Self {
        self.header_name = name.to_string();
        self
    }

    /// Set token prefix (e.g., "Bearer ").
    pub fn token_prefix(mut self, prefix: &str) -> Self {
        self.token_prefix = prefix.to_string();
        self
    }

    /// Enable query parameter token extraction.
    pub fn query_param(mut self, param: &str) -> Self {
        self.query_param = Some(param.to_string());
        self
    }

    /// Enable cookie token extraction.
    pub fn cookie(mut self, name: &str) -> Self {
        self.cookie_name = Some(name.to_string());
        self
    }

    /// Skip authentication for specific paths.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }

    /// Set key for storing claims in request context.
    pub fn claims_key(mut self, key: &str) -> Self {
        self.claims_key = key.to_string();
        self
    }

    /// Extract token from request.
    fn extract_token(&self, request: &Request) -> Option<String> {
        // Try header first
        if let Some(header_value) = request.headers.get(&self.header_name.to_lowercase()) {
            if header_value.starts_with(&self.token_prefix) {
                return Some(header_value[self.token_prefix.len()..].to_string());
            }
        }

        // Try query parameter
        if let Some(ref param) = self.query_param {
            if let Some(value) = request.query_params.get(param) {
                return Some(value.clone());
            }
        }

        // Try cookie
        if let Some(ref cookie_name) = self.cookie_name {
            if let Some(cookie_header) = request.headers.get("cookie") {
                for cookie in cookie_header.split(';') {
                    let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
                    if parts.len() == 2 && parts[0] == cookie_name {
                        return Some(parts[1].to_string());
                    }
                }
            }
        }

        None
    }
}

impl Middleware for JwtAuth {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Check if path should be skipped
        for skip_path in &self.skip_paths {
            if request.path.starts_with(skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        // Extract token
        let token = self.extract_token(request).ok_or_else(|| {
            MiddlewareError::unauthorized("Missing authentication token")
        })?;

        // Validate token
        let claims = self.config.decode(&token).map_err(|e| {
            MiddlewareError::unauthorized(&e.to_string())
        })?;

        // Store claims in request context
        request
            .context
            .insert(self.claims_key.clone(), serde_json::to_value(&claims).unwrap());

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -50 // Run early, after logging
    }

    fn name(&self) -> &str {
        "jwt_auth"
    }
}

// ============================================================================
// Basic Authentication
// ============================================================================

/// Credential validator function type.
pub type CredentialValidator = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;

/// Basic authentication middleware.
pub struct BasicAuth {
    realm: String,
    validator: CredentialValidator,
    skip_paths: Vec<String>,
    user_key: String,
}

impl BasicAuth {
    /// Create new Basic auth with static credentials.
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn new(username: &str, password: &str) -> Self {
        let expected_user = username.to_string();
        let expected_pass = password.to_string();

        Self {
            realm: "Restricted".to_string(),
            // Use secure_compare for timing-attack-resistant validation
            validator: Arc::new(move |u, p| {
                secure_compare(u, &expected_user) && secure_compare(p, &expected_pass)
            }),
            skip_paths: Vec::new(),
            user_key: "basic_auth_user".to_string(),
        }
    }

    /// Create new Basic auth with custom validator.
    pub fn with_validator<F>(validator: F) -> Self
    where
        F: Fn(&str, &str) -> bool + Send + Sync + 'static,
    {
        Self {
            realm: "Restricted".to_string(),
            validator: Arc::new(validator),
            skip_paths: Vec::new(),
            user_key: "basic_auth_user".to_string(),
        }
    }

    /// Set realm name.
    pub fn realm(mut self, realm: &str) -> Self {
        self.realm = realm.to_string();
        self
    }

    /// Skip authentication for specific paths.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }

    /// Set key for storing username in request context.
    pub fn user_key(mut self, key: &str) -> Self {
        self.user_key = key.to_string();
        self
    }

    /// Extract and decode Basic auth credentials.
    fn extract_credentials(&self, request: &Request) -> Option<(String, String)> {
        let header = request.headers.get("authorization")?;
        if !header.starts_with("Basic ") {
            return None;
        }

        let encoded = &header[6..];
        let decoded = BASE64.decode(encoded).ok()?;
        let credentials = String::from_utf8(decoded).ok()?;

        let parts: Vec<&str> = credentials.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }

        Some((parts[0].to_string(), parts[1].to_string()))
    }
}

impl Middleware for BasicAuth {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Check if path should be skipped
        for skip_path in &self.skip_paths {
            if request.path.starts_with(skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        // Extract credentials
        let (username, password) = self.extract_credentials(request).ok_or_else(|| {
            MiddlewareError::unauthorized("Invalid or missing Basic authentication")
        })?;

        // Validate credentials
        if !(self.validator)(&username, &password) {
            return Err(MiddlewareError::unauthorized("Invalid credentials"));
        }

        // Store username in context
        request.context.insert(
            self.user_key.clone(),
            serde_json::Value::String(username),
        );

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, _request: &Request, response: &mut Response) -> MiddlewareResult {
        // Add WWW-Authenticate header on 401 responses
        if response.status == 401 {
            response.set_header(
                "WWW-Authenticate",
                &format!("Basic realm=\"{}\"", self.realm),
            );
        }
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -50
    }

    fn name(&self) -> &str {
        "basic_auth"
    }
}

// ============================================================================
// API Key Authentication
// ============================================================================

/// API key location.
#[derive(Clone, Debug)]
pub enum ApiKeyLocation {
    Header(String),
    Query(String),
    Cookie(String),
}

impl Default for ApiKeyLocation {
    fn default() -> Self {
        ApiKeyLocation::Header("X-API-Key".to_string())
    }
}

/// API key validator.
pub type ApiKeyValidator = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// API key authentication middleware.
pub struct ApiKeyAuth {
    location: ApiKeyLocation,
    validator: ApiKeyValidator,
    skip_paths: Vec<String>,
    client_key: String,
}

impl ApiKeyAuth {
    /// Create new API key auth with static key.
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn new(api_key: &str) -> Self {
        let expected_key = api_key.to_string();
        Self {
            location: ApiKeyLocation::default(),
            // Use secure_compare for timing-attack-resistant validation
            validator: Arc::new(move |key| {
                if secure_compare(key, &expected_key) {
                    Some("default".to_string())
                } else {
                    None
                }
            }),
            skip_paths: Vec::new(),
            client_key: "api_key_client".to_string(),
        }
    }

    /// Create new API key auth with validator (returns client ID if valid).
    pub fn with_validator<F>(validator: F) -> Self
    where
        F: Fn(&str) -> Option<String> + Send + Sync + 'static,
    {
        Self {
            location: ApiKeyLocation::default(),
            validator: Arc::new(validator),
            skip_paths: Vec::new(),
            client_key: "api_key_client".to_string(),
        }
    }

    /// Create API key auth from HashMap of keys.
    pub fn from_keys(keys: HashMap<String, String>) -> Self {
        let keys = Arc::new(keys);
        Self {
            location: ApiKeyLocation::default(),
            validator: Arc::new(move |key| keys.get(key).cloned()),
            skip_paths: Vec::new(),
            client_key: "api_key_client".to_string(),
        }
    }

    /// Set key location.
    pub fn location(mut self, location: ApiKeyLocation) -> Self {
        self.location = location;
        self
    }

    /// Set header name for key.
    pub fn header(mut self, name: &str) -> Self {
        self.location = ApiKeyLocation::Header(name.to_string());
        self
    }

    /// Set query param for key.
    pub fn query(mut self, name: &str) -> Self {
        self.location = ApiKeyLocation::Query(name.to_string());
        self
    }

    /// Skip authentication for specific paths.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }

    /// Set key for storing client ID in request context.
    pub fn client_key(mut self, key: &str) -> Self {
        self.client_key = key.to_string();
        self
    }

    /// Extract API key from request.
    fn extract_key(&self, request: &Request) -> Option<String> {
        match &self.location {
            ApiKeyLocation::Header(name) => {
                request.headers.get(&name.to_lowercase()).cloned()
            }
            ApiKeyLocation::Query(name) => {
                request.query_params.get(name).cloned()
            }
            ApiKeyLocation::Cookie(name) => {
                if let Some(cookie_header) = request.headers.get("cookie") {
                    for cookie in cookie_header.split(';') {
                        let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
                        if parts.len() == 2 && parts[0] == name {
                            return Some(parts[1].to_string());
                        }
                    }
                }
                None
            }
        }
    }
}

impl Middleware for ApiKeyAuth {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Check if path should be skipped
        for skip_path in &self.skip_paths {
            if request.path.starts_with(skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        // Extract key
        let key = self.extract_key(request).ok_or_else(|| {
            MiddlewareError::unauthorized("Missing API key")
        })?;

        // Validate key
        let client_id = (self.validator)(&key).ok_or_else(|| {
            MiddlewareError::unauthorized("Invalid API key")
        })?;

        // Store client ID in context
        request.context.insert(
            self.client_key.clone(),
            serde_json::Value::String(client_id),
        );

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -50
    }

    fn name(&self) -> &str {
        "api_key_auth"
    }
}

// ============================================================================
// Token Blacklist (for logout/revocation)
// ============================================================================

/// Token blacklist for JWT revocation.
pub struct TokenBlacklist {
    tokens: Arc<RwLock<HashMap<String, u64>>>,
}

impl TokenBlacklist {
    /// Create new token blacklist.
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add token to blacklist with expiration.
    pub fn add(&self, token: &str, expires_at: u64) {
        self.tokens.write().insert(token.to_string(), expires_at);
    }

    /// Check if token is blacklisted.
    pub fn is_blacklisted(&self, token: &str) -> bool {
        self.tokens.read().contains_key(token)
    }

    /// Remove expired tokens.
    pub fn cleanup(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.tokens.write().retain(|_, exp| *exp > now);
    }
}

impl Default for TokenBlacklist {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_claims() {
        let claims = JwtClaims::new("user123", Duration::from_secs(3600))
            .with_issuer("test-app")
            .with_claim("role", serde_json::json!("admin"));

        assert_eq!(claims.sub, "user123");
        assert_eq!(claims.iss, Some("test-app".to_string()));
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_jwt_encode_decode() {
        let config = JwtConfig::new(b"secret-key-for-testing");
        let claims = JwtClaims::new("user123", Duration::from_secs(3600));

        let token = config.encode(&claims).unwrap();
        let decoded = config.decode(&token).unwrap();

        assert_eq!(decoded.sub, "user123");
    }

    #[test]
    fn test_api_key_from_keys() {
        let mut keys = HashMap::new();
        keys.insert("key1".to_string(), "client1".to_string());
        keys.insert("key2".to_string(), "client2".to_string());

        let auth = ApiKeyAuth::from_keys(keys);
        // Test would need actual request handling
        assert_eq!(auth.name(), "api_key_auth");
    }

    #[test]
    fn test_token_blacklist() {
        let blacklist = TokenBlacklist::new();

        let future_exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;

        blacklist.add("token1", future_exp);
        assert!(blacklist.is_blacklisted("token1"));
        assert!(!blacklist.is_blacklisted("token2"));
    }
}
