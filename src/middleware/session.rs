//! Session middleware for Cello.
//!
//! Provides:
//! - Cookie-based sessions
//! - In-memory session store
//! - Pluggable session backends
//! - Session security options

use parking_lot::RwLock;
use rand::rngs::OsRng;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pyo3::prelude::*;

use super::{path_matches_skip, Middleware, MiddlewareAction, MiddlewareResult};
use crate::json::{json_to_python, python_to_json};
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
        self.data
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get a string value from the session.
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.data
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
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
        now.saturating_sub(self.last_accessed) > max_age.as_secs()
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

    /// Extend an existing session's lifetime without rewriting its data.
    fn touch(&self, id: &str, ttl: Duration) {
        if let Some(data) = self.get(id) {
            self.set(id, data, ttl);
        }
    }

    /// Generate a new session ID using cryptographically secure randomness.
    fn generate_id(&self) -> String {
        let bytes: [u8; 32] = OsRng.gen();
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

/// Default cap on sessions held by an [`InMemorySessionStore`].
pub const DEFAULT_MAX_SESSIONS: usize = 100_000;

/// In-memory session store.
///
/// Holds at most `max_sessions` entries. A new session that would exceed the
/// cap first sweeps expired entries, then evicts the sixteenth of the store
/// closest to expiry (with sliding expiry, the least recently used), so the
/// store is bounded and eviction costs O(1) amortised per insert.
pub struct InMemorySessionStore {
    sessions: RwLock<HashMap<String, SessionEntry>>,
    _default_ttl: Duration,
    /// Writes since the last expiry sweep.
    writes: std::sync::atomic::AtomicUsize,
    max_sessions: usize,
}

impl InMemorySessionStore {
    /// Create new in-memory session store.
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(3600)) // 1 hour default
    }

    /// Create with custom TTL.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            _default_ttl: ttl,
            writes: std::sync::atomic::AtomicUsize::new(0),
            max_sessions: DEFAULT_MAX_SESSIONS,
        }
    }

    /// Cap the number of stored sessions (0 = unlimited).
    pub fn max_sessions(mut self, max: usize) -> Self {
        self.max_sessions = max;
        self
    }

    /// Get count of active sessions.
    pub fn count(&self) -> usize {
        self.sessions.read().len()
    }
}

/// Remove the `n` entries that expire soonest (O(len), no full sort).
fn evict_soonest(sessions: &mut HashMap<String, SessionEntry>, n: usize) {
    let mut by_expiry: Vec<(Instant, String)> = sessions
        .iter()
        .map(|(k, e)| (e.expires_at, k.clone()))
        .collect();
    let n = n.min(by_expiry.len());
    if n == 0 {
        return;
    }
    if n < by_expiry.len() {
        by_expiry.select_nth_unstable_by_key(n - 1, |(t, _)| *t);
    }
    for (_, k) in by_expiry.into_iter().take(n) {
        sessions.remove(&k);
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
        let now = Instant::now();
        let entry = SessionEntry {
            data,
            expires_at: now + ttl,
        };
        let mut sessions = self.sessions.write();
        if self.max_sessions > 0
            && sessions.len() >= self.max_sessions
            && !sessions.contains_key(id)
        {
            sessions.retain(|_, entry| entry.expires_at > now);
            if sessions.len() >= self.max_sessions {
                evict_soonest(&mut sessions, (self.max_sessions / 16).max(1));
            }
        }
        sessions.insert(id.to_string(), entry);
        // Expired entries are otherwise never removed: sweep periodically.
        let n = self
            .writes
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n % 1024 == 1023 {
            sessions.retain(|_, entry| entry.expires_at > now);
        }
    }

    fn touch(&self, id: &str, ttl: Duration) {
        if let Some(entry) = self.sessions.write().get_mut(id) {
            entry.expires_at = Instant::now() + ttl;
        }
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
        self.sessions
            .write()
            .retain(|_, entry| entry.expires_at > now);
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
            cookie.push_str(&format!("; Domain={domain}"));
        }

        if self.secure {
            cookie.push_str("; Secure");
        }

        if self.http_only {
            cookie.push_str("; HttpOnly");
        }

        cookie.push_str(&format!("; SameSite={}", self.same_site));

        if let Some(max_age) = self.max_age {
            cookie.push_str(&format!("; Max-Age={max_age}"));
        }

        cookie
    }

    /// Build cookie for deletion.
    pub fn build_delete_cookie(&self) -> String {
        let mut cookie = format!(
            "{}=; Path={}; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
            self.name, self.path
        );

        // Preserve security flags on deletion cookie to match the original cookie attributes
        if self.secure {
            cookie.push_str("; Secure");
        }
        if self.http_only {
            cookie.push_str("; HttpOnly");
        }
        cookie.push_str(&format!("; SameSite={}", self.same_site));

        cookie
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
    /// Keys whose change marks a login or privilege change.
    auth_keys: Arc<[String]>,
    /// Largest serialized session accepted (bytes, 0 = unlimited).
    max_session_size: usize,
}

/// Default largest serialized session (bytes).
pub const DEFAULT_MAX_SESSION_SIZE: usize = 64 * 1024;

impl SessionMiddleware {
    /// Create new session middleware with in-memory store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(InMemorySessionStore::new()),
            cookie_config: CookieConfig::default(),
            session_ttl: Duration::from_secs(3600),
            regenerate_on_auth: true,
            skip_paths: Vec::new(),
            auth_keys: default_auth_keys(),
            max_session_size: DEFAULT_MAX_SESSION_SIZE,
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
            auth_keys: default_auth_keys(),
            max_session_size: DEFAULT_MAX_SESSION_SIZE,
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

    /// Replace the keys whose change triggers `regenerate_on_auth`.
    pub fn auth_keys(mut self, keys: Vec<String>) -> Self {
        self.auth_keys = keys.into();
        self
    }

    /// Largest serialized session accepted (bytes, 0 = unlimited). A write
    /// past it raises `ValueError` in the handler.
    pub fn max_session_size(mut self, bytes: usize) -> Self {
        self.max_session_size = bytes;
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
            if path_matches_skip(&request.path, skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        let (session_id, session_data, is_new) = self.get_or_create_session(request);

        // Read-only snapshot kept in the context for `request.get_context(...)`.
        request.context.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id.clone()),
        );
        request.context.insert(
            "session".to_string(),
            serde_json::to_value(&session_data).unwrap_or_default(),
        );
        request.context.insert(
            "session_is_new".to_string(),
            serde_json::Value::Bool(is_new),
        );

        // The live, mutable session. The handle is shared (Arc) between the
        // request the handler sees and the copy the `after` hook gets, so
        // `request.session["k"] = v` in a handler is what gets persisted.
        let auth_snapshot = auth_values(&self.auth_keys, &session_data);
        request.session = Some(Arc::new(parking_lot::Mutex::new(SessionState {
            id: session_id,
            data: session_data,
            is_new,
            modified: false,
            regenerate: false,
            destroyed: false,
            auth_snapshot,
            max_size: self.max_session_size,
        })));

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        let Some(handle) = request.session.as_ref() else {
            return Ok(MiddlewareAction::Continue);
        };
        let mut st = handle.lock();

        if st.destroyed {
            if !st.is_new {
                self.store.delete(&st.id);
            }
            response.append_header("Set-Cookie", &self.cookie_config.build_delete_cookie());
            return Ok(MiddlewareAction::Continue);
        }

        // Session fixation: a session that becomes authenticated gets a new id,
        // so an id planted or observed before login is worthless afterwards.
        // Covers both login (no user -> user) and a switch to another user.
        if self.regenerate_on_auth
            && !st.is_new
            && self.auth_keys.iter().any(|k| st.data.contains(k))
            && auth_values(&self.auth_keys, &st.data) != st.auth_snapshot
        {
            st.regenerate = true;
        }
        if st.regenerate && !st.is_new {
            self.store.delete(&st.id);
            st.id = self.store.generate_id();
            st.is_new = true;
            st.modified = true;
        }

        if st.modified && st.too_large() {
            // Only reachable through Rust-side writes (Python writes are
            // checked as they happen): keep the stored copy unchanged.
            eprintln!("[cello] session exceeds max_session_size; not saved");
            st.modified = false;
        }
        if st.modified {
            st.data.touch();
            self.store.set(&st.id, st.data.clone(), self.session_ttl);
            if st.is_new {
                response.append_header("Set-Cookie", &self.cookie_config.build_cookie(&st.id));
            }
        } else if !st.is_new {
            // Sliding expiry for an active, unchanged session.
            self.store.touch(&st.id, self.session_ttl);
        }
        // A new session nobody wrote to is neither stored nor sent: a cookieless
        // client (crawler, attacker) no longer allocates a store entry per request.

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -60 // Run before auth
    }

    fn name(&self) -> &str {
        "session"
    }
}

/// Session keys whose appearance or change marks a login or privilege change
/// (triggers id regeneration when `regenerate_on_auth` is on). This is a
/// safety net: call `session.regenerate()` after login to be explicit, or set
/// `SessionConfig(auth_keys=[...])` to the keys your app uses.
pub const DEFAULT_AUTH_KEYS: &[&str] = &[
    "user_id",
    "userid",
    "uid",
    "user",
    "username",
    "account_id",
    "email",
    "auth",
    "authenticated",
    "is_authenticated",
    "logged_in",
    "role",
    "roles",
];

fn default_auth_keys() -> Arc<[String]> {
    DEFAULT_AUTH_KEYS.iter().map(|k| k.to_string()).collect()
}

// ============================================================================
// Live session state (shared between handler and middleware)
// ============================================================================

/// Mutable per-request session state.
#[derive(Debug)]
pub struct SessionState {
    pub id: String,
    pub data: SessionData,
    pub is_new: bool,
    pub modified: bool,
    /// Issue a new id at the end of the request (keeps the data).
    pub regenerate: bool,
    /// Delete the session and clear the cookie at the end of the request.
    pub destroyed: bool,
    /// Auth-key values when the request started (for `regenerate_on_auth`).
    auth_snapshot: Vec<Option<serde_json::Value>>,
    /// Largest serialized size accepted (bytes, 0 = unlimited).
    pub max_size: usize,
}

impl SessionState {
    /// Whether the data is over `max_size` once serialized.
    pub fn too_large(&self) -> bool {
        self.max_size > 0
            && serde_json::to_vec(&self.data.data).map_or(true, |v| v.len() > self.max_size)
    }
}

/// The values of the auth keys, in `keys` order.
fn auth_values(keys: &[String], data: &SessionData) -> Vec<Option<serde_json::Value>> {
    keys.iter().map(|k| data.data.get(k).cloned()).collect()
}

/// Shared handle to the request's session.
pub type SessionHandle = Arc<parking_lot::Mutex<SessionState>>;

/// The request's session, available as `request.session` when sessions are
/// enabled. Behaves like a dict of JSON-compatible values.
#[pyclass(name = "Session")]
pub struct PySession {
    pub(crate) handle: SessionHandle,
}

#[pymethods]
impl PySession {
    /// Get a value, or `default` when absent.
    #[pyo3(signature = (key, default=None))]
    fn get(&self, py: Python<'_>, key: &str, default: Option<PyObject>) -> PyResult<PyObject> {
        match self.handle.lock().data.data.get(key) {
            Some(v) => json_to_python(py, v),
            None => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    /// Set a value (must be JSON-serializable).
    fn set(&self, py: Python<'_>, key: String, value: &PyAny) -> PyResult<()> {
        let v = python_to_json(py, value).map_err(pyo3::exceptions::PyValueError::new_err)?;
        let mut st = self.handle.lock();
        let previous = st.data.data.insert(key.clone(), v);
        if st.too_large() {
            // Undo, so the session stays storable.
            match previous {
                Some(old) => st.data.data.insert(key, old),
                None => st.data.data.remove(&key),
            };
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "session would exceed max_session_size ({} bytes)",
                st.max_size
            )));
        }
        st.modified = true;
        Ok(())
    }

    /// Remove and return a value.
    #[pyo3(signature = (key, default=None))]
    fn pop(&self, py: Python<'_>, key: &str, default: Option<PyObject>) -> PyResult<PyObject> {
        let mut st = self.handle.lock();
        match st.data.data.remove(key) {
            Some(v) => {
                st.modified = true;
                json_to_python(py, &v)
            }
            None => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    fn keys(&self) -> Vec<String> {
        self.handle.lock().data.keys()
    }

    /// Remove every value (the session id is kept).
    fn clear(&self) {
        let mut st = self.handle.lock();
        st.data.clear();
        st.modified = true;
    }

    /// Issue a fresh session id at the end of this request, keeping the data.
    /// Call after login or any privilege change.
    fn regenerate(&self) {
        let mut st = self.handle.lock();
        st.regenerate = true;
        st.modified = true;
    }

    /// Delete the session server-side and expire the cookie (logout).
    fn destroy(&self) {
        let mut st = self.handle.lock();
        st.destroyed = true;
        st.data.data.clear();
    }

    /// Whether the session was created by this request.
    #[getter]
    fn is_new(&self) -> bool {
        self.handle.lock().is_new
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        match self.handle.lock().data.data.get(key) {
            Some(v) => json_to_python(py, v),
            None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_string())),
        }
    }

    fn __setitem__(&self, py: Python<'_>, key: String, value: &PyAny) -> PyResult<()> {
        self.set(py, key, value)
    }

    fn __delitem__(&self, key: &str) -> PyResult<()> {
        let mut st = self.handle.lock();
        match st.data.data.remove(key) {
            Some(_) => {
                st.modified = true;
                Ok(())
            }
            None => Err(pyo3::exceptions::PyKeyError::new_err(key.to_string())),
        }
    }

    fn __contains__(&self, key: &str) -> bool {
        self.handle.lock().data.contains(key)
    }

    fn __len__(&self) -> usize {
        self.handle.lock().data.data.len()
    }

    fn __repr__(&self) -> String {
        format!("Session(keys={:?})", self.handle.lock().data.keys())
    }
}

// ============================================================================
// Session Helper Functions
// ============================================================================

/// Regenerate the session ID at the end of this request (for security after
/// login). The data is kept; the old id is deleted from the store.
pub fn regenerate_session(request: &mut Request) {
    if let Some(handle) = request.session.as_ref() {
        let mut st = handle.lock();
        st.regenerate = true;
        st.modified = true;
    }
}

/// Destroy the session at the end of this request.
pub fn destroy_session(request: &mut Request) {
    if let Some(handle) = request.session.as_ref() {
        handle.lock().destroyed = true;
    }
    request.context.remove("session_id");
    request.context.remove("session");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_is_capped_and_evicts_least_recently_used() {
        let store = InMemorySessionStore::new().max_sessions(32);
        let ttl = Duration::from_secs(60);
        store.set("keep", SessionData::new(), ttl);
        for i in 0..200 {
            // Keep "keep" the most recently refreshed entry.
            store.touch("keep", ttl + Duration::from_secs(10));
            store.set(&format!("s{i}"), SessionData::new(), ttl);
        }
        assert!(store.count() <= 32, "{}", store.count());
        assert!(store.get("keep").is_some());
        assert!(store.get("s0").is_none());
        assert!(store.get("s199").is_some());
    }

    #[test]
    fn oversized_session_is_detected() {
        let mut data = SessionData::new();
        data.set("blob", "x".repeat(100));
        let st = SessionState {
            id: "id".into(),
            data,
            is_new: true,
            modified: true,
            regenerate: false,
            destroyed: false,
            auth_snapshot: Vec::new(),
            max_size: 64,
        };
        assert!(st.too_large());
    }

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
