//! Redis Integration Support.
//!
//! Provides async Redis client with connection pooling for:
//! - Key/value operations
//! - Pub/Sub messaging
//! - Connection health monitoring
//! - Connection pool statistics
//!
//! # Example
//! ```python
//! from cello import App
//! from cello.cache import RedisConfig
//!
//! config = RedisConfig(
//!     url="redis://localhost:6379",
//!     pool_size=10,
//!     cluster_mode=False
//! )
//!
//! @app.on_startup
//! async def setup():
//!     app.state.redis = await Redis.connect(config)
//! ```

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Redis configuration.
#[derive(Clone, Debug)]
pub struct RedisConfig {
    /// Redis URL (e.g., redis://localhost:6379)
    pub url: String,
    /// Maximum number of connections in the pool
    pub pool_size: usize,
    /// Minimum number of idle connections
    pub min_idle: usize,
    /// Maximum time to wait for a connection
    pub connection_timeout: Duration,
    /// Idle timeout before closing a connection
    pub idle_timeout: Duration,
    /// Enable cluster mode
    pub cluster_mode: bool,
    /// Default TTL for keys (seconds)
    pub default_ttl: Option<u64>,
    /// Database number (0-15)
    pub database: u8,
    /// Connection password
    pub password: Option<String>,
    /// Enable TLS
    pub tls: bool,
    /// Key prefix for namespacing
    pub key_prefix: Option<String>,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            pool_size: 10,
            min_idle: 1,
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300),
            cluster_mode: false,
            default_ttl: None,
            database: 0,
            password: None,
            tls: false,
            key_prefix: None,
        }
    }
}

impl RedisConfig {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            ..Default::default()
        }
    }

    pub fn pool_size(mut self, size: usize) -> Self {
        self.pool_size = size;
        self
    }

    pub fn cluster_mode(mut self, enabled: bool) -> Self {
        self.cluster_mode = enabled;
        self
    }

    pub fn default_ttl(mut self, ttl: u64) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    pub fn database(mut self, db: u8) -> Self {
        self.database = db;
        self
    }

    pub fn key_prefix(mut self, prefix: &str) -> Self {
        self.key_prefix = Some(prefix.to_string());
        self
    }
}

/// Redis connection statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RedisStats {
    /// Total connections created
    pub total_connections: u64,
    /// Currently active connections
    pub active_connections: usize,
    /// Currently idle connections
    pub idle_connections: usize,
    /// Total commands executed
    pub total_commands: u64,
    /// Total command errors
    pub total_errors: u64,
    /// Average command time in milliseconds
    pub avg_command_time_ms: f64,
    /// Peak active connections
    pub peak_active_connections: usize,
    /// Connection timeouts
    pub connection_timeouts: u64,
    /// Total keys set
    pub total_keys_set: u64,
    /// Total keys retrieved
    pub total_keys_get: u64,
    /// Cache hit ratio
    pub cache_hit_ratio: f64,
}

/// Redis pool metrics.
pub struct RedisPoolMetrics {
    total_connections: AtomicU64,
    active_connections: AtomicUsize,
    idle_connections: AtomicUsize,
    total_commands: AtomicU64,
    total_errors: AtomicU64,
    command_time_sum_ms: AtomicU64,
    peak_active: AtomicUsize,
    connection_timeouts: AtomicU64,
    total_keys_set: AtomicU64,
    total_keys_get: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
}

impl Default for RedisPoolMetrics {
    fn default() -> Self {
        Self {
            total_connections: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            idle_connections: AtomicUsize::new(0),
            total_commands: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            command_time_sum_ms: AtomicU64::new(0),
            peak_active: AtomicUsize::new(0),
            connection_timeouts: AtomicU64::new(0),
            total_keys_set: AtomicU64::new(0),
            total_keys_get: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }
}

impl RedisPoolMetrics {
    pub fn record_connection(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        let active = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;

        let mut current_peak = self.peak_active.load(Ordering::Relaxed);
        while active > current_peak {
            match self.peak_active.compare_exchange_weak(
                current_peak,
                active,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(p) => current_peak = p,
            }
        }
    }

    pub fn release_connection(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
        self.idle_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_command(&self, duration_ms: u64, error: bool) {
        self.total_commands.fetch_add(1, Ordering::Relaxed);
        self.command_time_sum_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        if error {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_set(&self) {
        self.total_keys_set.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_get(&self, hit: bool) {
        self.total_keys_get.fetch_add(1, Ordering::Relaxed);
        if hit {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_timeout(&self) {
        self.connection_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> RedisStats {
        let total_commands = self.total_commands.load(Ordering::Relaxed);
        let command_time_sum = self.command_time_sum_ms.load(Ordering::Relaxed);
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total_lookups = hits + misses;

        RedisStats {
            total_connections: self.total_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            idle_connections: self.idle_connections.load(Ordering::Relaxed),
            total_commands,
            total_errors: self.total_errors.load(Ordering::Relaxed),
            avg_command_time_ms: if total_commands > 0 {
                command_time_sum as f64 / total_commands as f64
            } else {
                0.0
            },
            peak_active_connections: self.peak_active.load(Ordering::Relaxed),
            connection_timeouts: self.connection_timeouts.load(Ordering::Relaxed),
            total_keys_set: self.total_keys_set.load(Ordering::Relaxed),
            total_keys_get: self.total_keys_get.load(Ordering::Relaxed),
            cache_hit_ratio: if total_lookups > 0 {
                hits as f64 / total_lookups as f64
            } else {
                0.0
            },
        }
    }
}

/// Redis error types.
#[derive(Debug, Clone)]
pub enum RedisError {
    /// Connection error
    Connection(String),
    /// Command error
    Command(String),
    /// Pool exhausted
    PoolExhausted,
    /// Connection timeout
    Timeout,
    /// Serialization error
    Serialization(String),
    /// Key not found
    NotFound(String),
    /// Cluster error
    Cluster(String),
    /// Unknown error
    Unknown(String),
}

impl std::fmt::Display for RedisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RedisError::Connection(msg) => write!(f, "Redis connection error: {}", msg),
            RedisError::Command(msg) => write!(f, "Redis command error: {}", msg),
            RedisError::PoolExhausted => write!(f, "Redis connection pool exhausted"),
            RedisError::Timeout => write!(f, "Redis connection timeout"),
            RedisError::Serialization(msg) => write!(f, "Redis serialization error: {}", msg),
            RedisError::NotFound(key) => write!(f, "Redis key not found: {}", key),
            RedisError::Cluster(msg) => write!(f, "Redis cluster error: {}", msg),
            RedisError::Unknown(msg) => write!(f, "Redis unknown error: {}", msg),
        }
    }
}

impl std::error::Error for RedisError {}

/// Redis value types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RedisValue {
    Nil,
    String(String),
    Bytes(Vec<u8>),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<RedisValue>),
    Map(HashMap<String, RedisValue>),
}

impl RedisValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RedisValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            RedisValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            RedisValue::Bytes(b) => Some(b),
            RedisValue::String(s) => Some(s.as_bytes()),
            _ => None,
        }
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, RedisValue::Nil)
    }
}

/// Generic Redis client trait.
pub trait RedisClient: Send + Sync {
    /// Get a value by key.
    fn get(&self, key: &str) -> Result<Option<RedisValue>, RedisError>;

    /// Set a value with optional TTL.
    fn set(&self, key: &str, value: RedisValue, ttl: Option<Duration>) -> Result<(), RedisError>;

    /// Delete a key.
    fn delete(&self, key: &str) -> Result<bool, RedisError>;

    /// Check if a key exists.
    fn exists(&self, key: &str) -> Result<bool, RedisError>;

    /// Set TTL on a key.
    fn expire(&self, key: &str, ttl: Duration) -> Result<bool, RedisError>;

    /// Increment a key's integer value.
    fn incr(&self, key: &str) -> Result<i64, RedisError>;

    /// Decrement a key's integer value.
    fn decr(&self, key: &str) -> Result<i64, RedisError>;

    /// Get multiple keys.
    fn mget(&self, keys: &[&str]) -> Result<Vec<Option<RedisValue>>, RedisError>;

    /// Set multiple key-value pairs.
    fn mset(&self, pairs: &[(&str, RedisValue)]) -> Result<(), RedisError>;

    /// Hash operations: get field from hash.
    fn hget(&self, key: &str, field: &str) -> Result<Option<RedisValue>, RedisError>;

    /// Hash operations: set field in hash.
    fn hset(&self, key: &str, field: &str, value: RedisValue) -> Result<(), RedisError>;

    /// Hash operations: get all fields.
    fn hgetall(&self, key: &str) -> Result<HashMap<String, RedisValue>, RedisError>;

    /// List: push to left.
    fn lpush(&self, key: &str, value: RedisValue) -> Result<i64, RedisError>;

    /// List: push to right.
    fn rpush(&self, key: &str, value: RedisValue) -> Result<i64, RedisError>;

    /// List: pop from left.
    fn lpop(&self, key: &str) -> Result<Option<RedisValue>, RedisError>;

    /// List: get range.
    fn lrange(&self, key: &str, start: i64, stop: i64) -> Result<Vec<RedisValue>, RedisError>;

    /// Set: add member.
    fn sadd(&self, key: &str, member: RedisValue) -> Result<bool, RedisError>;

    /// Set: get all members.
    fn smembers(&self, key: &str) -> Result<Vec<RedisValue>, RedisError>;

    /// Publish a message to a channel.
    fn publish(&self, channel: &str, message: &str) -> Result<i64, RedisError>;

    /// Check if pool is healthy.
    fn is_healthy(&self) -> bool;

    /// Get pool statistics.
    fn stats(&self) -> RedisStats;

    /// Close all connections.
    fn close(&self);
}

/// In-memory mock Redis client for testing.
pub struct MockRedisClient {
    #[allow(dead_code)]
    config: RedisConfig,
    metrics: Arc<RedisPoolMetrics>,
    data: Arc<RwLock<HashMap<String, RedisValue>>>,
    ttls: Arc<RwLock<HashMap<String, Instant>>>,
    healthy: AtomicBool,
}

impl MockRedisClient {
    pub fn new(config: RedisConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(RedisPoolMetrics::default()),
            data: Arc::new(RwLock::new(HashMap::new())),
            ttls: Arc::new(RwLock::new(HashMap::new())),
            healthy: AtomicBool::new(true),
        }
    }

    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::SeqCst);
    }

    fn is_expired(&self, key: &str) -> bool {
        let ttls = self.ttls.read();
        if let Some(expiry) = ttls.get(key) {
            Instant::now() > *expiry
        } else {
            false
        }
    }

    fn clean_expired(&self, key: &str) {
        if self.is_expired(key) {
            self.data.write().remove(key);
            self.ttls.write().remove(key);
        }
    }
}

impl RedisClient for MockRedisClient {
    fn get(&self, key: &str) -> Result<Option<RedisValue>, RedisError> {
        let start = Instant::now();
        self.clean_expired(key);
        let data = self.data.read();
        let result = data.get(key).cloned();
        self.metrics.record_get(result.is_some());
        self.metrics
            .record_command(start.elapsed().as_millis() as u64, false);
        Ok(result)
    }

    fn set(
        &self,
        key: &str,
        value: RedisValue,
        ttl: Option<Duration>,
    ) -> Result<(), RedisError> {
        let start = Instant::now();
        self.data.write().insert(key.to_string(), value);
        if let Some(ttl) = ttl {
            self.ttls
                .write()
                .insert(key.to_string(), Instant::now() + ttl);
        }
        self.metrics.record_set();
        self.metrics
            .record_command(start.elapsed().as_millis() as u64, false);
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<bool, RedisError> {
        let start = Instant::now();
        let removed = self.data.write().remove(key).is_some();
        self.ttls.write().remove(key);
        self.metrics
            .record_command(start.elapsed().as_millis() as u64, false);
        Ok(removed)
    }

    fn exists(&self, key: &str) -> Result<bool, RedisError> {
        self.clean_expired(key);
        Ok(self.data.read().contains_key(key))
    }

    fn expire(&self, key: &str, ttl: Duration) -> Result<bool, RedisError> {
        if self.data.read().contains_key(key) {
            self.ttls
                .write()
                .insert(key.to_string(), Instant::now() + ttl);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn incr(&self, key: &str) -> Result<i64, RedisError> {
        let mut data = self.data.write();
        let current = data
            .get(key)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let new_val = current + 1;
        data.insert(key.to_string(), RedisValue::Integer(new_val));
        Ok(new_val)
    }

    fn decr(&self, key: &str) -> Result<i64, RedisError> {
        let mut data = self.data.write();
        let current = data
            .get(key)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let new_val = current - 1;
        data.insert(key.to_string(), RedisValue::Integer(new_val));
        Ok(new_val)
    }

    fn mget(&self, keys: &[&str]) -> Result<Vec<Option<RedisValue>>, RedisError> {
        let data = self.data.read();
        Ok(keys.iter().map(|k| data.get(*k).cloned()).collect())
    }

    fn mset(&self, pairs: &[(&str, RedisValue)]) -> Result<(), RedisError> {
        let mut data = self.data.write();
        for (key, value) in pairs {
            data.insert(key.to_string(), value.clone());
        }
        Ok(())
    }

    fn hget(&self, key: &str, field: &str) -> Result<Option<RedisValue>, RedisError> {
        let data = self.data.read();
        if let Some(RedisValue::Map(map)) = data.get(key) {
            Ok(map.get(field).cloned())
        } else {
            Ok(None)
        }
    }

    fn hset(&self, key: &str, field: &str, value: RedisValue) -> Result<(), RedisError> {
        let mut data = self.data.write();
        let map = data
            .entry(key.to_string())
            .or_insert_with(|| RedisValue::Map(HashMap::new()));
        if let RedisValue::Map(ref mut m) = map {
            m.insert(field.to_string(), value);
        }
        Ok(())
    }

    fn hgetall(&self, key: &str) -> Result<HashMap<String, RedisValue>, RedisError> {
        let data = self.data.read();
        if let Some(RedisValue::Map(map)) = data.get(key) {
            Ok(map.clone())
        } else {
            Ok(HashMap::new())
        }
    }

    fn lpush(&self, key: &str, value: RedisValue) -> Result<i64, RedisError> {
        let mut data = self.data.write();
        let list = data
            .entry(key.to_string())
            .or_insert_with(|| RedisValue::Array(Vec::new()));
        if let RedisValue::Array(ref mut arr) = list {
            arr.insert(0, value);
            Ok(arr.len() as i64)
        } else {
            Err(RedisError::Command("Key is not a list".to_string()))
        }
    }

    fn rpush(&self, key: &str, value: RedisValue) -> Result<i64, RedisError> {
        let mut data = self.data.write();
        let list = data
            .entry(key.to_string())
            .or_insert_with(|| RedisValue::Array(Vec::new()));
        if let RedisValue::Array(ref mut arr) = list {
            arr.push(value);
            Ok(arr.len() as i64)
        } else {
            Err(RedisError::Command("Key is not a list".to_string()))
        }
    }

    fn lpop(&self, key: &str) -> Result<Option<RedisValue>, RedisError> {
        let mut data = self.data.write();
        if let Some(RedisValue::Array(ref mut arr)) = data.get_mut(key) {
            if arr.is_empty() {
                Ok(None)
            } else {
                Ok(Some(arr.remove(0)))
            }
        } else {
            Ok(None)
        }
    }

    fn lrange(&self, key: &str, start: i64, stop: i64) -> Result<Vec<RedisValue>, RedisError> {
        let data = self.data.read();
        if let Some(RedisValue::Array(arr)) = data.get(key) {
            let len = arr.len() as i64;
            let start = if start < 0 { (len + start).max(0) } else { start } as usize;
            let stop = if stop < 0 { (len + stop).max(0) } else { stop.min(len - 1) } as usize;
            if start > stop || start >= arr.len() {
                Ok(Vec::new())
            } else {
                Ok(arr[start..=stop.min(arr.len() - 1)].to_vec())
            }
        } else {
            Ok(Vec::new())
        }
    }

    fn sadd(&self, key: &str, member: RedisValue) -> Result<bool, RedisError> {
        let mut data = self.data.write();
        let set = data
            .entry(key.to_string())
            .or_insert_with(|| RedisValue::Array(Vec::new()));
        if let RedisValue::Array(ref mut arr) = set {
            // Simple dedup check for strings
            let exists = arr.iter().any(|v| {
                if let (RedisValue::String(a), RedisValue::String(b)) = (v, &member) {
                    a == b
                } else {
                    false
                }
            });
            if !exists {
                arr.push(member);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err(RedisError::Command("Key is not a set".to_string()))
        }
    }

    fn smembers(&self, key: &str) -> Result<Vec<RedisValue>, RedisError> {
        let data = self.data.read();
        if let Some(RedisValue::Array(arr)) = data.get(key) {
            Ok(arr.clone())
        } else {
            Ok(Vec::new())
        }
    }

    fn publish(&self, _channel: &str, _message: &str) -> Result<i64, RedisError> {
        Ok(0) // Mock: no subscribers
    }

    fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    fn stats(&self) -> RedisStats {
        self.metrics.get_stats()
    }

    fn close(&self) {
        // No-op for mock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_config() {
        let config = RedisConfig::new("redis://localhost:6379")
            .pool_size(20)
            .cluster_mode(false)
            .default_ttl(3600)
            .database(1)
            .key_prefix("myapp");

        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.pool_size, 20);
        assert!(!config.cluster_mode);
        assert_eq!(config.default_ttl, Some(3600));
        assert_eq!(config.database, 1);
        assert_eq!(config.key_prefix, Some("myapp".to_string()));
    }

    #[test]
    fn test_mock_redis_basic_ops() {
        let client = MockRedisClient::new(RedisConfig::default());

        // Set and get
        client
            .set("key1", RedisValue::String("value1".to_string()), None)
            .unwrap();
        let result = client.get("key1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().as_str(), Some("value1"));

        // Delete
        assert!(client.delete("key1").unwrap());
        assert!(client.get("key1").unwrap().is_none());

        // Exists
        client
            .set("key2", RedisValue::Integer(42), None)
            .unwrap();
        assert!(client.exists("key2").unwrap());
        assert!(!client.exists("key3").unwrap());
    }

    #[test]
    fn test_mock_redis_incr_decr() {
        let client = MockRedisClient::new(RedisConfig::default());

        assert_eq!(client.incr("counter").unwrap(), 1);
        assert_eq!(client.incr("counter").unwrap(), 2);
        assert_eq!(client.decr("counter").unwrap(), 1);
    }

    #[test]
    fn test_mock_redis_hash() {
        let client = MockRedisClient::new(RedisConfig::default());

        client
            .hset("user:1", "name", RedisValue::String("Alice".to_string()))
            .unwrap();
        client
            .hset("user:1", "age", RedisValue::Integer(30))
            .unwrap();

        let name = client.hget("user:1", "name").unwrap();
        assert_eq!(name.unwrap().as_str(), Some("Alice"));

        let all = client.hgetall("user:1").unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_mock_redis_list() {
        let client = MockRedisClient::new(RedisConfig::default());

        client
            .rpush("queue", RedisValue::String("first".to_string()))
            .unwrap();
        client
            .rpush("queue", RedisValue::String("second".to_string()))
            .unwrap();
        client
            .lpush("queue", RedisValue::String("zero".to_string()))
            .unwrap();

        let range = client.lrange("queue", 0, -1).unwrap();
        assert_eq!(range.len(), 3);

        let popped = client.lpop("queue").unwrap();
        assert_eq!(popped.unwrap().as_str(), Some("zero"));
    }

    #[test]
    fn test_mock_redis_stats() {
        let client = MockRedisClient::new(RedisConfig::default());

        client
            .set("k1", RedisValue::String("v1".to_string()), None)
            .unwrap();
        client.get("k1").unwrap();
        client.get("missing").unwrap();

        let stats = client.stats();
        assert_eq!(stats.total_keys_set, 1);
        assert_eq!(stats.total_keys_get, 2);
        assert!(stats.cache_hit_ratio > 0.0);
    }
}
