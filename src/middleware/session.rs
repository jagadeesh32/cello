//! Session middleware for Cello.
//!
//! Provides:
//! - Cookie-based sessions
//! - In-memory session store
//! - Pluggable session backends
//! - Session security options

use parking_lot::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Session Data
// ============================================================================

/// Session data stored for each session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    /// Session values
    #[serde(flatten)]
    pub data: HashMap<String, serde_json::Value>,
    /// Creation time (Unix timestamp)
    pub created_at: u64,
    /// Last accessed time (Unix timestamp)
    pub last_accessed: u64,
    /// Whether session has been modified
    #[serde(skip)]
    pub modified: bool,
}

impl SessionData {
    /// Create new empty session.
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            data: HashMap::new(),
            created_at: now,
            last_accessed: now,
            modified: false,
        }
    }

    /// Get a value from the session.
    pub fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.data.get(key).and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a string value from the session.
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    /// Set a value in the session.
    pub fn set<T: Serialize>(&mut self, key: &str, value: T) {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), json_value);
            self.modified = true;
        }
    }

    /// Remove a value from the session.
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.modified = true;
        self.data.remove(key)
    }

    /// Check if session contains a key.
    pub fn contains(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Clear all session data.
    pub fn clear(&mut self) {
        self.data.clear();
        self.modified = true;
    }

    /// Get all keys in the session.
    pub fn keys(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    /// Update last accessed time.
    pub fn touch(&mut self) {
        self.last_accessed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Check if session has expired.
    pub fn is_expired(&self, max_age: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now - self.last_accessed > max_age.as_secs()
    }
}

impl Default for SessionData {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Session Store Trait
// ============================================================================

/// Trait for session storage backends.
pub trait SessionStore: Send + Sync {
    /// Get session data by ID.
    fn get(&self, id: &str) -> Option<SessionData>;

    /// Save session data.
    fn set(&self, id: &str, data: SessionData, ttl: Duration);

    /// Delete session.
    fn delete(&self, id: &str);

    /// Check if session exists.
    fn exists(&self, id: &str) -> bool;

    /// Cleanup expired sessions.
    fn cleanup(&self);

    /// Generate a new session ID.
    fn generate_id(&self) -> String {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        hex::encode(bytes)
    }
}

// ============================================================================
// In-Memory Session Store
// ============================================================================

/// In-memory session entry.
struct SessionEntry {
    data: SessionData,
    expires_at: Instant,
}

/// In-memory session store.
pub struct InMemorySessionStore {
    sessions: RwLock<HashMap<String, SessionEntry>>,
    _default_ttl: Duration,
}

impl InMemorySessionStore {
    /// Create new in-memory session store.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            _default_ttl: Duration::from_secs(3600), // 1 hour default
        }
    }

    /// Create with custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            _default_ttl: ttl,
        }
    }

    /// Get count of active sessions.
    pub fn count(&self) -> usize {
        self.sessions.read().len()
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore for InMemorySessionStore {
    fn get(&self, id: &str) -> Option<SessionData> {
        let sessions = self.sessions.read();
        sessions.get(id).and_then(|entry| {
            if entry.expires_at > Instant::now() {
                Some(entry.data.clone())
            } else {
                None
            }
        })
    }

    fn set(&self, id: &str, data: SessionData, ttl: Duration) {
        let expires_at = Instant::now() + ttl;
        let entry = SessionEntry { data, expires_at };
        self.sessions.write().insert(id.to_string(), entry);
    }

    fn delete(&self, id: &str) {
        self.sessions.write().remove(id);
    }

    fn exists(&self, id: &str) -> bool {
        let sessions = self.sessions.read();
        sessions
            .get(id)
            .map(|entry| entry.expires_at > Instant::now())
            .unwrap_or(false)
    }

    fn cleanup(&self) {
        let now = Instant::now();
        self.sessions.write().retain(|_, entry| entry.expires_at > now);
    }
}

// ============================================================================
// Cookie Configuration
// ============================================================================

/// SameSite cookie attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl std::fmt::Display for SameSite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SameSite::Strict => write!(f, "Strict"),
            SameSite::Lax => write!(f, "Lax"),
            SameSite::None => write!(f, "None"),
        }
    }
}

/// Cookie configuration for session.
#[derive(Clone)]
pub struct CookieConfig {
    /// Cookie name
    pub name: String,
    /// Cookie path
    pub path: String,
    /// Cookie domain (optional)
    pub domain: Option<String>,
    /// Secure flag (HTTPS only)
    pub secure: bool,
    /// HttpOnly flag (no JS access)
    pub http_only: bool,
    /// SameSite attribute
    pub same_site: SameSite,
    /// Max age in seconds
    pub max_age: Option<u64>,
}

impl CookieConfig {
    /// Create new cookie config.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            path: "/".to_string(),
            domain: None,
            secure: true,
            http_only: true,
            same_site: SameSite::Lax,
            max_age: None,
        }
    }

    /// Set cookie path.
    pub fn path(mut self, path: &str) -> Self {
        self.path = path.to_string();
        self
    }

    /// Set cookie domain.
    pub fn domain(mut self, domain: &str) -> Self {
        self.domain = Some(domain.to_string());
        self
    }

    /// Set secure flag.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Set HttpOnly flag.
    pub fn http_only(mut self, http_only: bool) -> Self {
        self.http_only = http_only;
        self
    }

    /// Set SameSite attribute.
    pub fn same_site(mut self, same_site: SameSite) -> Self {
        self.same_site = same_site;
        self
    }

    /// Set max age.
    pub fn max_age(mut self, seconds: u64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    /// Build cookie header value.
    pub fn build_cookie(&self, session_id: &str) -> String {
        let mut cookie = format!("{}={}", self.name, session_id);

        cookie.push_str(&format!("; Path={}", self.path));

        if let Some(ref domain) = self.domain {
            cookie.push_str(&format!("; Domain={}", domain));
        }

        if self.secure {
            cookie.push_str("; Secure");
        }

        if self.http_only {
            cookie.push_str("; HttpOnly");
        }

        cookie.push_str(&format!("; SameSite={}", self.same_site));

        if let Some(max_age) = self.max_age {
            cookie.push_str(&format!("; Max-Age={}", max_age));
        }

        cookie
    }

    /// Build cookie for deletion.
    pub fn build_delete_cookie(&self) -> String {
        format!(
            "{}=; Path={}; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
            self.name, self.path
        )
    }
}

impl Default for CookieConfig {
    fn default() -> Self {
        Self::new("session_id")
    }
}

// ============================================================================
// Session Middleware
// ============================================================================

/// Session middleware.
pub struct SessionMiddleware {
    store: Arc<dyn SessionStore>,
    cookie_config: CookieConfig,
    session_ttl: Duration,
    regenerate_on_auth: bool,
    skip_paths: Vec<String>,
}

impl SessionMiddleware {
    /// Create new session middleware with in-memory store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(InMemorySessionStore::new()),
            cookie_config: CookieConfig::default(),
            session_ttl: Duration::from_secs(3600),
            regenerate_on_auth: true,
            skip_paths: Vec::new(),
        }
    }

    /// Create with custom store.
    pub fn with_store<S: SessionStore + 'static>(store: S) -> Self {
        Self {
            store: Arc::new(store),
            cookie_config: CookieConfig::default(),
            session_ttl: Duration::from_secs(3600),
            regenerate_on_auth: true,
            skip_paths: Vec::new(),
        }
    }

    /// Set cookie configuration.
    pub fn cookie(mut self, config: CookieConfig) -> Self {
        self.cookie_config = config;
        self
    }

    /// Set cookie name.
    pub fn cookie_name(mut self, name: &str) -> Self {
        self.cookie_config.name = name.to_string();
        self
    }

    /// Set session TTL.
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.session_ttl = ttl;
        if self.cookie_config.max_age.is_none() {
            self.cookie_config.max_age = Some(ttl.as_secs());
        }
        self
    }

    /// Enable/disable session regeneration on auth changes.
    pub fn regenerate_on_auth(mut self, regenerate: bool) -> Self {
        self.regenerate_on_auth = regenerate;
        self
    }

    /// Skip session for specific paths.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }

    /// Extract session ID from request cookies.
    fn extract_session_id(&self, request: &Request) -> Option<String> {
        if let Some(cookie_header) = request.headers.get("cookie") {
            for cookie in cookie_header.split(';') {
                let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
                if parts.len() == 2 && parts[0] == self.cookie_config.name {
                    return Some(parts[1].to_string());
                }
            }
        }
        None
    }

    /// Get or create session.
    fn get_or_create_session(&self, request: &mut Request) -> (String, SessionData, bool) {
        // Try to get existing session
        if let Some(session_id) = self.extract_session_id(request) {
            if let Some(mut data) = self.store.get(&session_id) {
                data.touch();
                return (session_id, data, false);
            }
        }

        // Create new session
        let session_id = self.store.generate_id();
        let data = SessionData::new();
        (session_id, data, true)
    }
}

impl Default for SessionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for SessionMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Check if path should be skipped
        for skip_path in &self.skip_paths {
            if request.path.starts_with(skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        let (session_id, session_data, is_new) = self.get_or_create_session(request);

        // Store session in request context
        request
            .context
            .insert("session_id".to_string(), serde_json::Value::String(session_id));
        request.context.insert(
            "session".to_string(),
            serde_json::to_value(&session_data).unwrap_or_default(),
        );
        request.context.insert(
            "session_is_new".to_string(),
            serde_json::Value::Bool(is_new),
        );

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Check if path was skipped
        for skip_path in &self.skip_paths {
            if request.path.starts_with(skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        // Get session data from context
        let session_id = request
            .context
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let session_data = request
            .context
            .get("session")
            .and_then(|v| serde_json::from_value::<SessionData>(v.clone()).ok());

        let is_new = request
            .context
            .get("session_is_new")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if let (Some(session_id), Some(data)) = (session_id, session_data) {
            // Save session if new or modified
            if is_new || data.modified {
                self.store.set(&session_id, data, self.session_ttl);
            }

            // Set cookie for new sessions
            if is_new {
                response.set_header(
                    "Set-Cookie",
                    &self.cookie_config.build_cookie(&session_id),
                );
            }
        }

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -60 // Run before auth
    }

    fn name(&self) -> &str {
        "session"
    }
}

// ============================================================================
// Session Helper Functions
// ============================================================================

/// Regenerate session ID (for security after login).
pub fn regenerate_session(
    request: &mut Request,
    store: &dyn SessionStore,
    _cookie_config: &CookieConfig,
) -> String {
    // Get current session data
    let old_data = request
        .context
        .get("session")
        .and_then(|v| serde_json::from_value::<SessionData>(v.clone()).ok())
        .unwrap_or_default();

    // Generate new session ID
    let new_id = store.generate_id();

    // Delete old session if exists
    if let Some(old_id) = request.context.get("session_id").and_then(|v| v.as_str()) {
        store.delete(old_id);
    }

    // Update context with new session ID
    request
        .context
        .insert("session_id".to_string(), serde_json::Value::String(new_id.clone()));
    request.context.insert(
        "session".to_string(),
        serde_json::to_value(&old_data).unwrap_or_default(),
    );
    request
        .context
        .insert("session_is_new".to_string(), serde_json::Value::Bool(true));

    new_id
}

/// Destroy session.
pub fn destroy_session(
    request: &mut Request,
    store: &dyn SessionStore,
) {
    if let Some(session_id) = request.context.get("session_id").and_then(|v| v.as_str()) {
        store.delete(session_id);
    }
    request.context.remove("session_id");
    request.context.remove("session");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_data() {
        let mut session = SessionData::new();
        session.set("user_id", 123);
        session.set("name", "Test User");

        assert_eq!(session.get::<i32>("user_id"), Some(123));
        assert_eq!(session.get_string("name"), Some("Test User".to_string()));
        assert!(session.modified);
    }

    #[test]
    fn test_in_memory_store() {
        let store = InMemorySessionStore::new();
        let mut session = SessionData::new();
        session.set("test", "value");

        store.set("test_session", session.clone(), Duration::from_secs(60));
        assert!(store.exists("test_session"));

        let retrieved = store.get("test_session").unwrap();
        assert_eq!(retrieved.get_string("test"), Some("value".to_string()));

        store.delete("test_session");
        assert!(!store.exists("test_session"));
    }

    #[test]
    fn test_cookie_config() {
        let config = CookieConfig::new("my_session")
            .path("/app")
            .secure(true)
            .http_only(true)
            .same_site(SameSite::Strict)
            .max_age(3600);

        let cookie = config.build_cookie("abc123");
        assert!(cookie.contains("my_session=abc123"));
        assert!(cookie.contains("Path=/app"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(cookie.contains("Max-Age=3600"));
    }

    #[test]
    fn test_session_expiry() {
        let mut session = SessionData::new();
        assert!(!session.is_expired(Duration::from_secs(60)));

        // Manually set old last_accessed time
        session.last_accessed = 0;
        assert!(session.is_expired(Duration::from_secs(60)));
    }
}
