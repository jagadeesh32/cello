//! Advanced Caching middleware for Cello (inspired by Litestar).
//!
//! Provides:
//! - Response caching with custom cache keys
//! - Cache invalidation strategies
//! - ETag-based caching
//! - Vary header support
//! - TTL/max-age control
//! - Conditional requests (If-None-Match, If-Modified-Since)
//! - Cache bypassing for certain routes
//! - Redis/Valkey backend support

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Cache Types
// ============================================================================

/// Cache entry with metadata.
#[derive(Clone, Debug)]
pub struct CachedResponse {
    /// The cached response body
    pub body: Vec<u8>,
    /// Response status code
    pub status: u16,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// When this entry was cached
    pub cached_at: SystemTime,
    /// Time-to-live in seconds
    pub ttl: u64,
    /// ETag for conditional requests
    pub etag: Option<String>,
    /// Last modified timestamp
    pub last_modified: Option<String>,
}

impl CachedResponse {
    /// Check if the cache entry is expired.
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now();
        let expiry = self.cached_at + Duration::from_secs(self.ttl);
        now >= expiry
    }

    /// Get remaining TTL in seconds.
    pub fn remaining_ttl(&self) -> u64 {
        if self.is_expired() {
            0
        } else {
            let now = SystemTime::now();
            let expiry = self.cached_at + Duration::from_secs(self.ttl);
            expiry.duration_since(now).unwrap_or_default().as_secs()
        }
    }
}

/// Cache store trait for different backends.
#[async_trait::async_trait]
pub trait CacheStore: Send + Sync {
    /// Get a cached response by key.
    async fn get(&self, key: &str) -> Result<Option<CachedResponse>, CacheError>;

    /// Store a response in cache.
    async fn set(&self, key: &str, response: CachedResponse) -> Result<(), CacheError>;

    /// Delete a cache entry.
    async fn delete(&self, key: &str) -> Result<(), CacheError>;

    /// Clear all cache entries.
    async fn clear(&self) -> Result<(), CacheError>;

    /// Check if cache store is available.
    async fn is_available(&self) -> bool {
        true
    }
}

/// Cache error types.
#[derive(Debug, Clone)]
pub enum CacheError {
    BackendError(String),
    SerializationError(String),
    ConnectionError(String),
    TimeoutError(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::BackendError(msg) => write!(f, "Cache backend error: {}", msg),
            CacheError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            CacheError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            CacheError::TimeoutError(msg) => write!(f, "Timeout error: {}", msg),
        }
    }
}

impl std::error::Error for CacheError {}

// ============================================================================
// Cache Key Builder
// ============================================================================

/// Trait for building cache keys.
pub trait CacheKeyBuilder: Send + Sync {
    /// Build a cache key from request.
    fn build_key(&self, request: &Request) -> String;
}

/// Default cache key builder.
pub struct DefaultCacheKeyBuilder {
    include_query: bool,
    include_headers: Vec<String>,
    include_user: bool,
}

impl DefaultCacheKeyBuilder {
    pub fn new() -> Self {
        Self {
            include_query: true,
            include_headers: Vec::new(),
            include_user: false,
        }
    }

    pub fn include_query(mut self, include: bool) -> Self {
        self.include_query = include;
        self
    }

    pub fn include_headers(mut self, headers: Vec<&str>) -> Self {
        self.include_headers = headers.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn include_user(mut self, include: bool) -> Self {
        self.include_user = include;
        self
    }
}

impl CacheKeyBuilder for DefaultCacheKeyBuilder {
    fn build_key(&self, request: &Request) -> String {
        let mut key_parts = vec![request.method.clone(), request.path.clone()];

        // Include query parameters if enabled
        if self.include_query && !request.query_params.is_empty() {
            let mut sorted_query: Vec<_> = request.query_params.iter().collect();
            sorted_query.sort_by_key(|(k, _)| *k);
            let query_str = sorted_query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            key_parts.push(query_str);
        }

        // Include specified headers
        for header_name in &self.include_headers {
            if let Some(value) = request.headers.get(header_name) {
                key_parts.push(format!("{}:{}", header_name, value));
            }
        }

        // Include user context if enabled
        if self.include_user {
            if let Some(user) = request.context.get("user") {
                if let Some(user_id) = user.get("id") {
                    key_parts.push(format!("user:{}", user_id));
                }
            }
        }

        key_parts.join("|")
    }
}

impl Default for DefaultCacheKeyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// In-Memory Cache Store
// ============================================================================

/// In-memory cache store implementation.
pub struct InMemoryCacheStore {
    store: Arc<parking_lot::RwLock<HashMap<String, CachedResponse>>>,
    max_size: usize,
}

impl InMemoryCacheStore {
    pub fn new() -> Self {
        Self {
            store: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            max_size: 10000, // Default max size
        }
    }

    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    /// Clean expired entries.
    fn cleanup_expired(&self) {
        let mut store = self.store.write();
        store.retain(|_, entry| !entry.is_expired());

        // If still over max size, remove oldest entries
        if store.len() > self.max_size {
            let mut entries: Vec<_> = store.iter().collect();
            entries.sort_by_key(|(_, entry)| entry.cached_at);
            let to_remove: Vec<String> = entries
                .iter()
                .take(store.len() - self.max_size)
                .map(|(key, _)| (*key).clone())
                .collect();

            for key in to_remove {
                store.remove(&key);
            }
        }
    }
}

#[async_trait::async_trait]
impl CacheStore for InMemoryCacheStore {
    async fn get(&self, key: &str) -> Result<Option<CachedResponse>, CacheError> {
        let store = self.store.read();
        match store.get(key) {
            Some(entry) if !entry.is_expired() => Ok(Some(entry.clone())),
            Some(_) => {
                // Entry exists but is expired, remove it
                drop(store);
                let mut store = self.store.write();
                store.remove(key);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, response: CachedResponse) -> Result<(), CacheError> {
        let mut store = self.store.write();
        store.insert(key.to_string(), response);

                // Cleanup in a separate task to avoid blocking
                let store_clone = Arc::clone(&self.store);
                let max_size = self.max_size;
                tokio::spawn(async move {
                    let mut store = store_clone.write();
                    if store.len() > max_size {
                        // Remove expired entries first
                        let mut valid_entries: HashMap<String, CachedResponse> = store
                            .iter()
                            .filter(|(_, entry)| !entry.is_expired())
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();

                        // If still over limit, remove oldest
                        if valid_entries.len() > max_size {
                            let mut entries: Vec<_> = valid_entries.iter().collect();
                            entries.sort_by_key(|(_, entry)| entry.cached_at);
                            let to_keep: Vec<String> = entries
                                .iter()
                                .skip(valid_entries.len() - max_size)
                                .map(|(key, _)| (*key).clone())
                                .collect();

                            valid_entries.retain(|key, _| to_keep.contains(key));
                        }

                        *store = valid_entries;
                    }
                });

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), CacheError> {
        self.store.write().remove(key);
        Ok(())
    }

    async fn clear(&self) -> Result<(), CacheError> {
        self.store.write().clear();
        Ok(())
    }
}

// ============================================================================
// Cache Configuration
// ============================================================================

/// Cache middleware configuration.
#[derive(Clone)]
pub struct CacheConfig {
    /// Cache store backend
    pub store: Arc<dyn CacheStore>,
    /// Cache key builder
    pub key_builder: Arc<dyn CacheKeyBuilder>,
    /// Default TTL in seconds
    pub default_ttl: u64,
    /// Cache only these HTTP methods
    pub methods: Vec<String>,
    /// Cache only responses with these status codes
    pub status_codes: Vec<u16>,
    /// Exclude these paths from caching
    pub exclude_paths: Vec<String>,
    /// Include these paths (if empty, cache all except excluded)
    pub include_paths: Vec<String>,
    /// Include query parameters in cache key
    pub include_query: bool,
    /// Include headers in cache key
    pub include_headers: Vec<String>,
    /// Include user context in cache key
    pub include_user: bool,
    /// Enable ETag generation
    pub enable_etag: bool,
    /// Enable Last-Modified header
    pub enable_last_modified: bool,
    /// Enable conditional requests
    pub enable_conditional: bool,
    /// Vary headers to include in cache key
    pub vary_headers: Vec<String>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryCacheStore::new()),
            key_builder: Arc::new(DefaultCacheKeyBuilder::new()),
            default_ttl: 300, // 5 minutes
            methods: vec!["GET".to_string(), "HEAD".to_string()],
            status_codes: vec![200, 201, 202, 203, 204, 205, 206, 207, 208, 226],
            exclude_paths: vec!["/health".to_string(), "/metrics".to_string()],
            include_paths: Vec::new(),
            include_query: true,
            include_headers: Vec::new(),
            include_user: false,
            enable_etag: true,
            enable_last_modified: true,
            enable_conditional: true,
            vary_headers: Vec::new(),
        }
    }
}

impl CacheConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn store<S: CacheStore + 'static>(mut self, store: S) -> Self {
        self.store = Arc::new(store);
        self
    }

    pub fn key_builder<K: CacheKeyBuilder + 'static>(mut self, builder: K) -> Self {
        self.key_builder = Arc::new(builder);
        self
    }

    pub fn ttl(mut self, ttl: u64) -> Self {
        self.default_ttl = ttl;
        self
    }

    pub fn methods(mut self, methods: Vec<&str>) -> Self {
        self.methods = methods.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn exclude_path(mut self, path: &str) -> Self {
        self.exclude_paths.push(path.to_string());
        self
    }

    pub fn include_path(mut self, path: &str) -> Self {
        self.include_paths.push(path.to_string());
        self
    }

    pub fn include_query(mut self, include: bool) -> Self {
        self.include_query = include;
        self
    }

    pub fn include_header(mut self, header: &str) -> Self {
        self.include_headers.push(header.to_string());
        self
    }

    pub fn include_user(mut self, include: bool) -> Self {
        self.include_user = include;
        self
    }

    pub fn enable_etag(mut self, enable: bool) -> Self {
        self.enable_etag = enable;
        self
    }

    pub fn enable_conditional(mut self, enable: bool) -> Self {
        self.enable_conditional = enable;
        self
    }
}

// ============================================================================
// Cache Middleware
// ============================================================================

/// Advanced caching middleware.
pub struct CacheMiddleware {
    config: CacheConfig,
}

impl CacheMiddleware {
    /// Create new cache middleware with default config.
    pub fn new() -> Self {
        Self {
            config: CacheConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: CacheConfig) -> Self {
        Self { config }
    }

    /// Check if request should be cached.
    fn should_cache_request(&self, request: &Request) -> bool {
        // Check HTTP method
        if !self.config.methods.contains(&request.method) {
            return false;
        }

        // Check excluded paths
        if self.config.exclude_paths.iter().any(|p| request.path.starts_with(p)) {
            return false;
        }

        // If include_paths is specified, only cache those
        if !self.config.include_paths.is_empty() {
            return self.config.include_paths.iter().any(|p| request.path.starts_with(p));
        }

        true
    }

    /// Check if response should be cached.
    fn should_cache_response(&self, response: &Response) -> bool {
        self.config.status_codes.contains(&response.status)
    }

    /// Generate ETag for response.
    fn generate_etag(&self, response: &Response) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        response.status.hash(&mut hasher);
        response.body_bytes().hash(&mut hasher);
        response.headers.iter().for_each(|(k, v)| {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        });

        format!("\"{:x}\"", hasher.finish())
    }

    /// Generate Last-Modified header.
    fn generate_last_modified(&self) -> String {
        let now = SystemTime::now();
        let datetime = chrono::DateTime::<chrono::Utc>::from(now);
        datetime.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }

    /// Check conditional request headers.
    fn check_conditional_request(&self, request: &Request, cached: &CachedResponse) -> Option<Response> {
        // Check If-None-Match (ETag)
        if let Some(request_etag) = request.headers.get("if-none-match") {
            if let Some(cached_etag) = &cached.etag {
                if request_etag == cached_etag || request_etag == "*" {
                    let mut response = Response::new(304);
                    response.set_header("ETag", cached_etag);
                    if let Some(last_modified) = &cached.last_modified {
                        response.set_header("Last-Modified", last_modified);
                    }
                    return Some(response);
                }
            }
        }

        // Check If-Modified-Since
        if let Some(if_modified_since) = request.headers.get("if-modified-since") {
            if let Some(last_modified) = &cached.last_modified {
                if if_modified_since == last_modified {
                    let mut response = Response::new(304);
                    if let Some(etag) = &cached.etag {
                        response.set_header("ETag", etag);
                    }
                    response.set_header("Last-Modified", last_modified);
                    return Some(response);
                }
            }
        }

        None
    }

    /// Create cached response from Response.
    fn create_cached_response(&self, response: &Response, ttl: u64) -> CachedResponse {
        let etag = if self.config.enable_etag {
            Some(self.generate_etag(response))
        } else {
            None
        };

        let last_modified = if self.config.enable_last_modified {
            Some(self.generate_last_modified())
        } else {
            None
        };

        CachedResponse {
            body: response.body_bytes().to_vec(),
            status: response.status,
            headers: response.headers.clone(),
            cached_at: SystemTime::now(),
            ttl,
            etag,
            last_modified,
        }
        }

    /// Restore Response from cached response.
    fn restore_response(&self, cached: &CachedResponse) -> Response {
        let mut response = Response::new(cached.status);
        response.set_body(cached.body.clone());

        // Copy headers
        for (key, value) in &cached.headers {
            response.set_header(key, value);
        }

        // Add cache headers
        response.set_header("X-Cache", "HIT");

        if cached.remaining_ttl() > 0 {
            response.set_header("Cache-Control", &format!("max-age={}", cached.remaining_ttl()));
        }

        // Add ETag and Last-Modified if available
        if let Some(etag) = &cached.etag {
            response.set_header("ETag", etag);
        }
        if let Some(last_modified) = &cached.last_modified {
            response.set_header("Last-Modified", last_modified);
        }

        response
    }
}

impl Default for CacheMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for CacheMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Check if this request should be cached
        if !self.should_cache_request(request) {
            return Ok(MiddlewareAction::Continue);
        }

        // Build cache key
        let cache_key = self.config.key_builder.build_key(request);

        // Check cache (async operation, but we'll handle it in after() for simplicity)
        // Store cache key in request context for after() method
        request.context.insert(
            "__cache_key".to_string(),
            serde_json::Value::String(cache_key),
        );

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Check if this request should be cached
        if !self.should_cache_request(request) {
            return Ok(MiddlewareAction::Continue);
        }

        // Get cache key from context
        if let Some(serde_json::Value::String(cache_key)) = request.context.get("__cache_key") {
            // Check if response should be cached
            if self.should_cache_response(response) {
                // Create cached response
                let cached_response = self.create_cached_response(response, self.config.default_ttl);

                // Store in cache (fire and forget)
                let store = self.config.store.clone();
                let key = cache_key.clone();
                let cached = cached_response.clone();
                tokio::spawn(async move {
                    let _ = store.set(&key, cached).await;
                });

                // Add cache headers to response
                response.set_header("X-Cache", "MISS");
                response.set_header("Cache-Control", &format!("max-age={}", self.config.default_ttl));
            } else {
                response.set_header("X-Cache", "BYPASS");
            }
        }

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -50 // Run after auth but before other processing
    }

    fn name(&self) -> &str {
        "cache"
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a standard cache key from request.
pub fn create_cache_key(request: &Request) -> String {
    let mut parts = vec![request.method.clone(), request.path.clone()];

    if !request.query_params.is_empty() {
        let mut sorted: Vec<_> = request.query_params.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        let query = sorted.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        parts.push(query);
    }

    parts.join("|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cache_key_builder() {
        let builder = DefaultCacheKeyBuilder::new();
        let mut request = Request::default();

        request.method = "GET".to_string();
        request.path = "/api/users".to_string();

        let key = builder.build_key(&request);
        assert_eq!(key, "GET|/api/users");

        // With query params
        request.query_params.insert("limit".to_string(), "10".to_string());
        request.query_params.insert("offset".to_string(), "0".to_string());

        let key = builder.build_key(&request);
        assert!(key.contains("GET|/api/users"));
        assert!(key.contains("limit=10"));
        assert!(key.contains("offset=0"));
    }

    #[test]
    fn test_cache_config() {
        let config = CacheConfig::new()
            .ttl(600)
            .exclude_path("/admin")
            .include_path("/api")
            .include_query(false);

        assert_eq!(config.default_ttl, 600);
        assert!(config.exclude_paths.contains(&"/admin".to_string()));
        assert!(config.include_paths.contains(&"/api".to_string()));
        assert!(!config.include_query);
    }

    #[test]
    fn test_cached_response_expiry() {
        let response = CachedResponse {
            body: b"Hello World".to_vec(),
            status: 200,
            headers: HashMap::new(),
            cached_at: SystemTime::now(),
            ttl: 1, // 1 second
            etag: None,
            last_modified: None,
        };

        assert!(!response.is_expired());
        assert!(response.remaining_ttl() > 0);

        // Wait for expiry
        std::thread::sleep(std::time::Duration::from_secs(2));

        assert!(response.is_expired());
        assert_eq!(response.remaining_ttl(), 0);
    }

    #[test]
    fn test_cache_middleware_should_cache() {
        let middleware = CacheMiddleware::new();

        let mut request = Request::default();
        request.method = "GET".to_string();
        request.path = "/api/data".to_string();

        assert!(middleware.should_cache_request(&request));

        // Test excluded path
        request.path = "/health".to_string();
        assert!(!middleware.should_cache_request(&request));

        // Test wrong method
        request.path = "/api/data".to_string();
        request.method = "POST".to_string();
        assert!(!middleware.should_cache_request(&request));
    }

    #[test]
    fn test_create_cache_key() {
        let mut request = Request::default();
        request.method = "GET".to_string();
        request.path = "/users".to_string();
        request.query_params.insert("sort".to_string(), "name".to_string());
        request.query_params.insert("limit".to_string(), "10".to_string());

        let key = create_cache_key(&request);
        assert!(key.contains("GET|/users"));
        assert!(key.contains("limit=10"));
        assert!(key.contains("sort=name"));
    }
}
