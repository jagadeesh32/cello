//! Rate limiting middleware for Cello.
//!
//! Provides:
//! - Token bucket algorithm
//! - Sliding window algorithm
//! - Per-client rate limiting
//! - Custom key extraction

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Rate Limit Store Trait
// ============================================================================

/// Rate limit state for a single key.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RateLimitState {
    /// Remaining requests allowed
    pub remaining: u64,
    /// Total limit
    pub limit: u64,
    /// Reset time (Unix timestamp)
    pub reset: u64,
    /// Whether the limit was exceeded
    #[serde(default)]
    pub exceeded: bool,
}

/// Trait for rate limit storage backends.
pub trait RateLimitStore: Send + Sync {
    /// Check and consume a request for the given key.
    /// Returns the current state after the operation.
    fn check(&self, key: &str, config: &RateLimitConfig) -> RateLimitState;

    /// Get current state without consuming.
    fn peek(&self, key: &str, config: &RateLimitConfig) -> RateLimitState;

    /// Reset the rate limit for a key.
    fn reset(&self, key: &str);

    /// Cleanup expired entries.
    fn cleanup(&self);
}

// ============================================================================
// Token Bucket Algorithm
// ============================================================================

/// Token bucket configuration.
#[derive(Clone, Debug)]
pub struct TokenBucketConfig {
    /// Maximum tokens in bucket
    pub capacity: u64,
    /// Tokens added per second
    pub refill_rate: f64,
}

impl TokenBucketConfig {
    /// Create new token bucket config.
    pub fn new(capacity: u64, refill_rate: f64) -> Self {
        Self {
            capacity,
            refill_rate,
        }
    }

    /// Create config with requests per minute.
    pub fn per_minute(requests: u64) -> Self {
        Self {
            capacity: requests,
            refill_rate: requests as f64 / 60.0,
        }
    }

    /// Create config with requests per second.
    pub fn per_second(requests: u64) -> Self {
        Self {
            capacity: requests,
            refill_rate: requests as f64,
        }
    }

    /// Create config with requests per hour.
    pub fn per_hour(requests: u64) -> Self {
        Self {
            capacity: requests,
            refill_rate: requests as f64 / 3600.0,
        }
    }
}

impl Default for TokenBucketConfig {
    fn default() -> Self {
        Self::per_minute(60)
    }
}

/// Token bucket state.
struct TokenBucketState {
    tokens: f64,
    last_refill: Instant,
}

/// In-memory token bucket store.
pub struct TokenBucketStore {
    buckets: DashMap<String, TokenBucketState>,
}

impl TokenBucketStore {
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }
}

impl Default for TokenBucketStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitStore for TokenBucketStore {
    fn check(&self, key: &str, config: &RateLimitConfig) -> RateLimitState {
        let bucket_config = match config {
            RateLimitConfig::TokenBucket(c) => c,
            _ => return self.peek(key, config),
        };

        let now = Instant::now();
        let reset_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + 60; // Reset in ~1 minute

        let mut entry = self.buckets.entry(key.to_string()).or_insert_with(|| {
            TokenBucketState {
                tokens: bucket_config.capacity as f64,
                last_refill: now,
            }
        });

        // Refill tokens
        let elapsed = now.duration_since(entry.last_refill).as_secs_f64();
        let refill = elapsed * bucket_config.refill_rate;
        entry.tokens = (entry.tokens + refill).min(bucket_config.capacity as f64);
        entry.last_refill = now;

        // Try to consume a token
        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            RateLimitState {
                remaining: entry.tokens as u64,
                limit: bucket_config.capacity,
                reset: reset_time,
                exceeded: false,
            }
        } else {
            RateLimitState {
                remaining: 0,
                limit: bucket_config.capacity,
                reset: reset_time,
                exceeded: true,
            }
        }
    }

    fn peek(&self, key: &str, config: &RateLimitConfig) -> RateLimitState {
        let bucket_config = match config {
            RateLimitConfig::TokenBucket(c) => c,
            _ => {
                return RateLimitState {
                    remaining: 0,
                    limit: 0,
                    reset: 0,
                    exceeded: false,
                }
            }
        };

        let reset_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + 60;

        if let Some(entry) = self.buckets.get(key) {
            RateLimitState {
                remaining: entry.tokens as u64,
                limit: bucket_config.capacity,
                reset: reset_time,
                exceeded: entry.tokens < 1.0,
            }
        } else {
            RateLimitState {
                remaining: bucket_config.capacity,
                limit: bucket_config.capacity,
                reset: reset_time,
                exceeded: false,
            }
        }
    }

    fn reset(&self, key: &str) {
        self.buckets.remove(key);
    }

    fn cleanup(&self) {
        // Token bucket doesn't need cleanup as it auto-refills
    }
}

// ============================================================================
// Sliding Window Algorithm
// ============================================================================

/// Sliding window configuration.
#[derive(Clone, Debug)]
pub struct SlidingWindowConfig {
    /// Maximum requests in window
    pub max_requests: u64,
    /// Window duration
    pub window: Duration,
}

impl SlidingWindowConfig {
    /// Create new sliding window config.
    pub fn new(max_requests: u64, window: Duration) -> Self {
        Self { max_requests, window }
    }

    /// Create config with requests per minute.
    pub fn per_minute(requests: u64) -> Self {
        Self {
            max_requests: requests,
            window: Duration::from_secs(60),
        }
    }

    /// Create config with requests per second.
    pub fn per_second(requests: u64) -> Self {
        Self {
            max_requests: requests,
            window: Duration::from_secs(1),
        }
    }

    /// Create config with requests per hour.
    pub fn per_hour(requests: u64) -> Self {
        Self {
            max_requests: requests,
            window: Duration::from_secs(3600),
        }
    }
}

impl Default for SlidingWindowConfig {
    fn default() -> Self {
        Self::per_minute(60)
    }
}

/// Sliding window state.
struct SlidingWindowState {
    /// Request timestamps within the window
    timestamps: Vec<u64>,
    /// Window start time
    window_start: u64,
}

/// In-memory sliding window store.
pub struct SlidingWindowStore {
    windows: DashMap<String, SlidingWindowState>,
}

impl SlidingWindowStore {
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
        }
    }
}

impl Default for SlidingWindowStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimitStore for SlidingWindowStore {
    fn check(&self, key: &str, config: &RateLimitConfig) -> RateLimitState {
        let window_config = match config {
            RateLimitConfig::SlidingWindow(c) => c,
            _ => return self.peek(key, config),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let window_start = now - window_config.window.as_secs();
        let reset_time = now + window_config.window.as_secs();

        let mut entry = self.windows.entry(key.to_string()).or_insert_with(|| {
            SlidingWindowState {
                timestamps: Vec::new(),
                window_start,
            }
        });

        // Remove old timestamps
        entry.timestamps.retain(|&ts| ts >= window_start);
        entry.window_start = window_start;

        // Check if limit exceeded
        if entry.timestamps.len() as u64 >= window_config.max_requests {
            RateLimitState {
                remaining: 0,
                limit: window_config.max_requests,
                reset: reset_time,
                exceeded: true,
            }
        } else {
            // Add new timestamp
            entry.timestamps.push(now);
            RateLimitState {
                remaining: window_config.max_requests - entry.timestamps.len() as u64,
                limit: window_config.max_requests,
                reset: reset_time,
                exceeded: false,
            }
        }
    }

    fn peek(&self, key: &str, config: &RateLimitConfig) -> RateLimitState {
        let window_config = match config {
            RateLimitConfig::SlidingWindow(c) => c,
            _ => {
                return RateLimitState {
                    remaining: 0,
                    limit: 0,
                    reset: 0,
                    exceeded: false,
                }
            }
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let window_start = now - window_config.window.as_secs();
        let reset_time = now + window_config.window.as_secs();

        if let Some(entry) = self.windows.get(key) {
            let count = entry
                .timestamps
                .iter()
                .filter(|&&ts| ts >= window_start)
                .count() as u64;

            RateLimitState {
                remaining: window_config.max_requests.saturating_sub(count),
                limit: window_config.max_requests,
                reset: reset_time,
                exceeded: count >= window_config.max_requests,
            }
        } else {
            RateLimitState {
                remaining: window_config.max_requests,
                limit: window_config.max_requests,
                reset: reset_time,
                exceeded: false,
            }
        }
    }

    fn reset(&self, key: &str) {
        self.windows.remove(key);
    }

    fn cleanup(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Remove entries with no recent activity
        self.windows.retain(|_, state| {
            !state.timestamps.is_empty() && state.timestamps.iter().any(|&ts| ts >= now - 3600)
        });
    }
}

// ============================================================================
// Fixed Window Counter (Simpler Alternative)
// ============================================================================

/// Fixed window state.
struct FixedWindowState {
    count: AtomicU64,
    window_start: u64,
}

/// In-memory fixed window store (simpler but less accurate).
pub struct FixedWindowStore {
    windows: DashMap<String, FixedWindowState>,
    window_seconds: u64,
}

impl FixedWindowStore {
    pub fn new(window_seconds: u64) -> Self {
        Self {
            windows: DashMap::new(),
            window_seconds,
        }
    }

    fn current_window(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now / self.window_seconds
    }
}

impl RateLimitStore for FixedWindowStore {
    fn check(&self, key: &str, config: &RateLimitConfig) -> RateLimitState {
        let limit = match config {
            RateLimitConfig::TokenBucket(c) => c.capacity,
            RateLimitConfig::SlidingWindow(c) => c.max_requests,
        };

        let current_window = self.current_window();
        let reset_time = (current_window + 1) * self.window_seconds;

        let entry = self.windows.entry(key.to_string()).or_insert_with(|| {
            FixedWindowState {
                count: AtomicU64::new(0),
                window_start: current_window,
            }
        });

        // Check if window has changed
        if entry.window_start != current_window {
            entry.count.store(0, Ordering::SeqCst);
            // Note: This is a simplified approach; in production, use compare_exchange
        }

        let count = entry.count.fetch_add(1, Ordering::SeqCst);

        if count >= limit {
            entry.count.fetch_sub(1, Ordering::SeqCst); // Undo increment
            RateLimitState {
                remaining: 0,
                limit,
                reset: reset_time,
                exceeded: true,
            }
        } else {
            RateLimitState {
                remaining: limit - count - 1,
                limit,
                reset: reset_time,
                exceeded: false,
            }
        }
    }

    fn peek(&self, key: &str, config: &RateLimitConfig) -> RateLimitState {
        let limit = match config {
            RateLimitConfig::TokenBucket(c) => c.capacity,
            RateLimitConfig::SlidingWindow(c) => c.max_requests,
        };

        let current_window = self.current_window();
        let reset_time = (current_window + 1) * self.window_seconds;

        if let Some(entry) = self.windows.get(key) {
            if entry.window_start == current_window {
                let count = entry.count.load(Ordering::SeqCst);
                return RateLimitState {
                    remaining: limit.saturating_sub(count),
                    limit,
                    reset: reset_time,
                    exceeded: count >= limit,
                };
            }
        }

        RateLimitState {
            remaining: limit,
            limit,
            reset: reset_time,
            exceeded: false,
        }
    }

    fn reset(&self, key: &str) {
        self.windows.remove(key);
    }

    fn cleanup(&self) {
        let current_window = self.current_window();
        self.windows.retain(|_, state| state.window_start >= current_window - 1);
    }
}

// ============================================================================
// Rate Limit Configuration
// ============================================================================

/// Rate limit algorithm configuration.
#[derive(Clone, Debug)]
pub enum RateLimitConfig {
    TokenBucket(TokenBucketConfig),
    SlidingWindow(SlidingWindowConfig),
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig::TokenBucket(TokenBucketConfig::default())
    }
}

// ============================================================================
// Key Extraction
// ============================================================================

/// Key extractor function type.
pub type KeyExtractor = Arc<dyn Fn(&Request) -> String + Send + Sync>;

/// Built-in key extractors.
pub struct KeyExtractors;

impl KeyExtractors {
    /// Extract client IP address.
    pub fn client_ip() -> KeyExtractor {
        Arc::new(|request: &Request| {
            // Check X-Forwarded-For first
            if let Some(xff) = request.headers.get("x-forwarded-for") {
                if let Some(ip) = xff.split(',').next() {
                    return ip.trim().to_string();
                }
            }
            // Check X-Real-IP
            if let Some(ip) = request.headers.get("x-real-ip") {
                return ip.clone();
            }
            // Fallback to remote address (would need to be set by server)
            request
                .context
                .get("remote_addr")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string()
        })
    }

    /// Extract user ID from context.
    pub fn user_id(context_key: &str) -> KeyExtractor {
        let key = context_key.to_string();
        Arc::new(move |request: &Request| {
            request
                .context
                .get(&key)
                .and_then(|v| v.as_str())
                .unwrap_or("anonymous")
                .to_string()
        })
    }

    /// Extract API key from header.
    pub fn api_key(header_name: &str) -> KeyExtractor {
        let header = header_name.to_lowercase();
        Arc::new(move |request: &Request| {
            request
                .headers
                .get(&header)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string())
        })
    }

    /// Combine multiple keys (e.g., IP + user ID).
    pub fn composite(extractors: Vec<KeyExtractor>) -> KeyExtractor {
        Arc::new(move |request: &Request| {
            extractors
                .iter()
                .map(|e| e(request))
                .collect::<Vec<_>>()
                .join(":")
        })
    }
}

// ============================================================================
// Rate Limit Middleware
// ============================================================================

/// Rate limiting middleware.
pub struct RateLimitMiddleware {
    store: Arc<dyn RateLimitStore>,
    config: RateLimitConfig,
    key_extractor: KeyExtractor,
    skip_paths: Vec<String>,
    headers_enabled: bool,
    custom_exceeded_response: Option<Arc<dyn Fn() -> Response + Send + Sync>>,
}

impl RateLimitMiddleware {
    /// Create new rate limit middleware with token bucket.
    pub fn token_bucket(config: TokenBucketConfig) -> Self {
        Self {
            store: Arc::new(TokenBucketStore::new()),
            config: RateLimitConfig::TokenBucket(config),
            key_extractor: KeyExtractors::client_ip(),
            skip_paths: Vec::new(),
            headers_enabled: true,
            custom_exceeded_response: None,
        }
    }

    /// Create new rate limit middleware with sliding window.
    pub fn sliding_window(config: SlidingWindowConfig) -> Self {
        Self {
            store: Arc::new(SlidingWindowStore::new()),
            config: RateLimitConfig::SlidingWindow(config),
            key_extractor: KeyExtractors::client_ip(),
            skip_paths: Vec::new(),
            headers_enabled: true,
            custom_exceeded_response: None,
        }
    }

    /// Create rate limiter with custom store.
    pub fn with_store<S: RateLimitStore + 'static>(store: S, config: RateLimitConfig) -> Self {
        Self {
            store: Arc::new(store),
            config,
            key_extractor: KeyExtractors::client_ip(),
            skip_paths: Vec::new(),
            headers_enabled: true,
            custom_exceeded_response: None,
        }
    }

    /// Set key extractor.
    pub fn key<F>(mut self, extractor: F) -> Self
    where
        F: Fn(&Request) -> String + Send + Sync + 'static,
    {
        self.key_extractor = Arc::new(extractor);
        self
    }

    /// Set key extractor from built-in.
    pub fn key_extractor(mut self, extractor: KeyExtractor) -> Self {
        self.key_extractor = extractor;
        self
    }

    /// Skip rate limiting for specific paths.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }

    /// Disable rate limit headers.
    pub fn without_headers(mut self) -> Self {
        self.headers_enabled = false;
        self
    }

    /// Set custom response for exceeded limits.
    pub fn exceeded_response<F>(mut self, response_fn: F) -> Self
    where
        F: Fn() -> Response + Send + Sync + 'static,
    {
        self.custom_exceeded_response = Some(Arc::new(response_fn));
        self
    }

    /// Add rate limit headers to response.
    fn add_headers(&self, response: &mut Response, state: &RateLimitState) {
        if self.headers_enabled {
            response.set_header("X-RateLimit-Limit", &state.limit.to_string());
            response.set_header("X-RateLimit-Remaining", &state.remaining.to_string());
            response.set_header("X-RateLimit-Reset", &state.reset.to_string());
        }
    }
}

impl Middleware for RateLimitMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Check if path should be skipped
        for skip_path in &self.skip_paths {
            if request.path.starts_with(skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        // Extract key
        let key = (self.key_extractor)(request);

        // Check rate limit
        let state = self.store.check(&key, &self.config);

        // Store state for after middleware
        request.context.insert(
            "rate_limit_state".to_string(),
            serde_json::json!({
                "remaining": state.remaining,
                "limit": state.limit,
                "reset": state.reset,
            }),
        );

        if state.exceeded {
            if let Some(ref response_fn) = self.custom_exceeded_response {
                let mut response = response_fn();
                self.add_headers(&mut response, &state);
                return Ok(MiddlewareAction::Stop(response));
            }

            let retry_after = state.reset.saturating_sub(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );

            let mut response = Response::new(429);
            response.set_header("Content-Type", "application/json");
            response.set_header("Retry-After", &retry_after.to_string());
            self.add_headers(&mut response, &state);
            response.set_body(
                serde_json::json!({
                    "error": "Too Many Requests",
                    "message": "Rate limit exceeded. Please try again later.",
                    "retry_after": retry_after,
                })
                .to_string()
                .into_bytes(),
            );

            return Ok(MiddlewareAction::Stop(response));
        }

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Add rate limit headers to successful responses
        if let Some(state_value) = request.context.get("rate_limit_state") {
            if let Ok(state) = serde_json::from_value::<RateLimitState>(state_value.clone()) {
                self.add_headers(response, &state);
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -40 // Run after auth but early
    }

    fn name(&self) -> &str {
        "rate_limit"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_config() {
        let config = TokenBucketConfig::per_minute(60);
        assert_eq!(config.capacity, 60);
        assert!((config.refill_rate - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_sliding_window_config() {
        let config = SlidingWindowConfig::per_hour(100);
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window, Duration::from_secs(3600));
    }

    #[test]
    fn test_token_bucket_store() {
        let store = TokenBucketStore::new();
        let config = RateLimitConfig::TokenBucket(TokenBucketConfig::new(10, 1.0));

        // First request should succeed
        let state = store.check("test_key", &config);
        assert!(!state.exceeded);
        assert_eq!(state.remaining, 9);

        // Consume all tokens
        for _ in 0..9 {
            store.check("test_key", &config);
        }

        // Next request should be rate limited
        let state = store.check("test_key", &config);
        assert!(state.exceeded);
    }

    #[test]
    fn test_sliding_window_store() {
        let store = SlidingWindowStore::new();
        let config = RateLimitConfig::SlidingWindow(SlidingWindowConfig::new(5, Duration::from_secs(60)));

        // First 5 requests should succeed
        for i in 0..5 {
            let state = store.check("test_key", &config);
            assert!(!state.exceeded, "Request {} should not be rate limited", i);
        }

        // 6th request should be rate limited
        let state = store.check("test_key", &config);
        assert!(state.exceeded);
    }
}
