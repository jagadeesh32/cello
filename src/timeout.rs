//! Timeout and limits configuration for Cello.
//!
//! This module provides:
//! - Request/response timeouts
//! - Connection limits
//! - Body size limits
//! - Per-route timeout overrides
//! - Async cancellation support

use pyo3::prelude::*;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Timeout configuration for the server.
#[derive(Debug, Clone)]
#[pyclass]
pub struct TimeoutConfig {
    /// Time to read request headers (default: 5s).
    #[pyo3(get, set)]
    pub read_header_timeout_ms: u64,

    /// Time to read entire request body (default: 30s).
    #[pyo3(get, set)]
    pub read_body_timeout_ms: u64,

    /// Time to write response (default: 30s).
    #[pyo3(get, set)]
    pub write_timeout_ms: u64,

    /// Idle connection timeout (default: 60s).
    #[pyo3(get, set)]
    pub idle_timeout_ms: u64,

    /// Handler execution timeout (default: 30s).
    #[pyo3(get, set)]
    pub handler_timeout_ms: u64,

    /// Keep-alive timeout between requests (default: 5s).
    #[pyo3(get, set)]
    pub keep_alive_timeout_ms: u64,
}

#[pymethods]
impl TimeoutConfig {
    #[new]
    #[pyo3(signature = (
        read_header_timeout=None,
        read_body_timeout=None,
        write_timeout=None,
        idle_timeout=None,
        handler_timeout=None,
        keep_alive_timeout=None
    ))]
    pub fn new(
        read_header_timeout: Option<u64>,
        read_body_timeout: Option<u64>,
        write_timeout: Option<u64>,
        idle_timeout: Option<u64>,
        handler_timeout: Option<u64>,
        keep_alive_timeout: Option<u64>,
    ) -> Self {
        Self {
            read_header_timeout_ms: read_header_timeout.unwrap_or(5000),
            read_body_timeout_ms: read_body_timeout.unwrap_or(30000),
            write_timeout_ms: write_timeout.unwrap_or(30000),
            idle_timeout_ms: idle_timeout.unwrap_or(60000),
            handler_timeout_ms: handler_timeout.unwrap_or(30000),
            keep_alive_timeout_ms: keep_alive_timeout.unwrap_or(5000),
        }
    }
}

impl TimeoutConfig {
    /// Get read header timeout as Duration.
    pub fn read_header_timeout(&self) -> Duration {
        Duration::from_millis(self.read_header_timeout_ms)
    }

    /// Get read body timeout as Duration.
    pub fn read_body_timeout(&self) -> Duration {
        Duration::from_millis(self.read_body_timeout_ms)
    }

    /// Get write timeout as Duration.
    pub fn write_timeout(&self) -> Duration {
        Duration::from_millis(self.write_timeout_ms)
    }

    /// Get idle timeout as Duration.
    pub fn idle_timeout(&self) -> Duration {
        Duration::from_millis(self.idle_timeout_ms)
    }

    /// Get handler timeout as Duration.
    pub fn handler_timeout(&self) -> Duration {
        Duration::from_millis(self.handler_timeout_ms)
    }

    /// Get keep-alive timeout as Duration.
    pub fn keep_alive_timeout(&self) -> Duration {
        Duration::from_millis(self.keep_alive_timeout_ms)
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            read_header_timeout_ms: 5000,
            read_body_timeout_ms: 30000,
            write_timeout_ms: 30000,
            idle_timeout_ms: 60000,
            handler_timeout_ms: 30000,
            keep_alive_timeout_ms: 5000,
        }
    }
}

/// Connection and request limits configuration.
#[derive(Debug, Clone)]
#[pyclass]
pub struct LimitsConfig {
    /// Maximum request header size in bytes (default: 8KB).
    #[pyo3(get, set)]
    pub max_header_size: usize,

    /// Maximum request body size in bytes (default: 10MB).
    #[pyo3(get, set)]
    pub max_body_size: usize,

    /// Maximum concurrent connections (default: 10000).
    #[pyo3(get, set)]
    pub max_connections: usize,

    /// Maximum requests per connection (keep-alive) (default: 100).
    #[pyo3(get, set)]
    pub max_requests_per_connection: usize,

    /// Maximum concurrent connections per IP (default: 100).
    #[pyo3(get, set)]
    pub max_connections_per_ip: usize,

    /// Maximum request line size in bytes (default: 8KB).
    #[pyo3(get, set)]
    pub max_request_line_size: usize,
}

#[pymethods]
impl LimitsConfig {
    #[new]
    #[pyo3(signature = (
        max_header_size=None,
        max_body_size=None,
        max_connections=None,
        max_requests_per_connection=None,
        max_connections_per_ip=None,
        max_request_line_size=None
    ))]
    pub fn new(
        max_header_size: Option<usize>,
        max_body_size: Option<usize>,
        max_connections: Option<usize>,
        max_requests_per_connection: Option<usize>,
        max_connections_per_ip: Option<usize>,
        max_request_line_size: Option<usize>,
    ) -> Self {
        Self {
            max_header_size: max_header_size.unwrap_or(8 * 1024),
            max_body_size: max_body_size.unwrap_or(10 * 1024 * 1024),
            max_connections: max_connections.unwrap_or(10000),
            max_requests_per_connection: max_requests_per_connection.unwrap_or(100),
            max_connections_per_ip: max_connections_per_ip.unwrap_or(100),
            max_request_line_size: max_request_line_size.unwrap_or(8 * 1024),
        }
    }

    /// Parse a size string like "10mb", "8kb", "1024".
    #[staticmethod]
    pub fn parse_size(size: &str) -> PyResult<usize> {
        parse_size_string(size)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(format!("Invalid size: {}", size)))
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_header_size: 8 * 1024,
            max_body_size: 10 * 1024 * 1024,
            max_connections: 10000,
            max_requests_per_connection: 100,
            max_connections_per_ip: 100,
            max_request_line_size: 8 * 1024,
        }
    }
}

/// Parse size string like "10mb", "8kb", "1024".
pub fn parse_size_string(s: &str) -> Option<usize> {
    let s = s.trim().to_lowercase();

    if let Ok(n) = s.parse::<usize>() {
        return Some(n);
    }

    let (num, unit) = if s.ends_with("kb") || s.ends_with("k") {
        let num = s.trim_end_matches(|c| c == 'k' || c == 'b');
        (num, 1024usize)
    } else if s.ends_with("mb") || s.ends_with("m") {
        let num = s.trim_end_matches(|c| c == 'm' || c == 'b');
        (num, 1024 * 1024)
    } else if s.ends_with("gb") || s.ends_with("g") {
        let num = s.trim_end_matches(|c| c == 'g' || c == 'b');
        (num, 1024 * 1024 * 1024)
    } else if s.ends_with('b') {
        let num = s.trim_end_matches('b');
        (num, 1)
    } else {
        return None;
    };

    num.trim().parse::<usize>().ok().map(|n| n * unit)
}

/// Per-route timeout overrides.
#[derive(Debug, Clone, Default)]
pub struct RouteTimeouts {
    /// Handler execution timeout override.
    pub handler_timeout_ms: Option<u64>,
    /// Body read timeout override.
    pub read_body_timeout_ms: Option<u64>,
    /// Write timeout override.
    pub write_timeout_ms: Option<u64>,
    /// Maximum body size override.
    pub max_body_size: Option<usize>,
}

impl RouteTimeouts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_handler_timeout(mut self, ms: u64) -> Self {
        self.handler_timeout_ms = Some(ms);
        self
    }

    pub fn with_body_timeout(mut self, ms: u64) -> Self {
        self.read_body_timeout_ms = Some(ms);
        self
    }

    pub fn with_max_body_size(mut self, size: usize) -> Self {
        self.max_body_size = Some(size);
        self
    }

    /// Get effective handler timeout.
    pub fn handler_timeout(&self, default: &TimeoutConfig) -> Duration {
        Duration::from_millis(self.handler_timeout_ms.unwrap_or(default.handler_timeout_ms))
    }

    /// Get effective body read timeout.
    pub fn read_body_timeout(&self, default: &TimeoutConfig) -> Duration {
        Duration::from_millis(self.read_body_timeout_ms.unwrap_or(default.read_body_timeout_ms))
    }

    /// Get effective max body size.
    pub fn max_body_size(&self, default: &LimitsConfig) -> usize {
        self.max_body_size.unwrap_or(default.max_body_size)
    }
}

/// Timeout error types.
#[derive(Debug, Clone)]
pub enum TimeoutError {
    /// Reading headers took too long.
    ReadHeaderTimeout,
    /// Reading body took too long.
    ReadBodyTimeout,
    /// Writing response took too long.
    WriteTimeout,
    /// Handler execution took too long.
    HandlerTimeout,
    /// Connection was idle for too long.
    IdleTimeout,
}

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeoutError::ReadHeaderTimeout => write!(f, "Read header timeout"),
            TimeoutError::ReadBodyTimeout => write!(f, "Read body timeout"),
            TimeoutError::WriteTimeout => write!(f, "Write timeout"),
            TimeoutError::HandlerTimeout => write!(f, "Handler execution timeout"),
            TimeoutError::IdleTimeout => write!(f, "Idle connection timeout"),
        }
    }
}

impl std::error::Error for TimeoutError {}

/// Limit error types.
#[derive(Debug, Clone)]
pub enum LimitError {
    /// Request body exceeds size limit.
    BodyTooLarge { limit: usize, actual: usize },
    /// Request headers exceed size limit.
    HeadersTooLarge { limit: usize },
    /// Too many concurrent connections.
    TooManyConnections { limit: usize },
    /// Too many connections from this IP.
    TooManyConnectionsFromIp { ip: String, limit: usize },
    /// Read error during body collection.
    ReadError(String),
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LimitError::BodyTooLarge { limit, actual } => {
                write!(f, "Request body too large: {} bytes (limit: {})", actual, limit)
            }
            LimitError::HeadersTooLarge { limit } => {
                write!(f, "Request headers too large (limit: {} bytes)", limit)
            }
            LimitError::TooManyConnections { limit } => {
                write!(f, "Too many concurrent connections (limit: {})", limit)
            }
            LimitError::TooManyConnectionsFromIp { ip, limit } => {
                write!(f, "Too many connections from IP {} (limit: {})", ip, limit)
            }
            LimitError::ReadError(msg) => write!(f, "Read error: {}", msg),
        }
    }
}

impl std::error::Error for LimitError {}

/// Connection tracker for enforcing limits.
pub struct ConnectionTracker {
    /// Total active connections.
    total_connections: AtomicUsize,
    /// Connections per IP.
    connections_per_ip: dashmap::DashMap<String, AtomicUsize>,
    /// Limits configuration.
    limits: LimitsConfig,
}

impl ConnectionTracker {
    /// Create a new connection tracker.
    pub fn new(limits: LimitsConfig) -> Self {
        Self {
            total_connections: AtomicUsize::new(0),
            connections_per_ip: dashmap::DashMap::new(),
            limits,
        }
    }

    /// Try to acquire a connection slot.
    pub fn try_acquire(&self, ip: &str) -> Result<ConnectionGuard<'_>, LimitError> {
        // Check total connections
        let current = self.total_connections.fetch_add(1, Ordering::SeqCst);
        if current >= self.limits.max_connections {
            self.total_connections.fetch_sub(1, Ordering::SeqCst);
            return Err(LimitError::TooManyConnections {
                limit: self.limits.max_connections,
            });
        }

        // Check per-IP connections
        let ip_count = self
            .connections_per_ip
            .entry(ip.to_string())
            .or_insert_with(|| AtomicUsize::new(0));
        let ip_current = ip_count.fetch_add(1, Ordering::SeqCst);
        if ip_current >= self.limits.max_connections_per_ip {
            ip_count.fetch_sub(1, Ordering::SeqCst);
            self.total_connections.fetch_sub(1, Ordering::SeqCst);
            return Err(LimitError::TooManyConnectionsFromIp {
                ip: ip.to_string(),
                limit: self.limits.max_connections_per_ip,
            });
        }

        Ok(ConnectionGuard {
            tracker: self,
            ip: ip.to_string(),
        })
    }

    /// Get current connection count.
    pub fn connection_count(&self) -> usize {
        self.total_connections.load(Ordering::SeqCst)
    }

    /// Get connection count for an IP.
    pub fn connection_count_for_ip(&self, ip: &str) -> usize {
        self.connections_per_ip
            .get(ip)
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    fn release(&self, ip: &str) {
        self.total_connections.fetch_sub(1, Ordering::SeqCst);
        if let Some(count) = self.connections_per_ip.get(ip) {
            count.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// RAII guard for connection tracking.
pub struct ConnectionGuard<'a> {
    tracker: &'a ConnectionTracker,
    ip: String,
}

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        self.tracker.release(&self.ip);
    }
}

/// Request cancellation token wrapper.
#[derive(Clone)]
pub struct RequestCancellation {
    token: CancellationToken,
}

impl RequestCancellation {
    /// Create a new cancellation token.
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// Cancel the request.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Check if the request is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Create a child token.
    pub fn child_token(&self) -> Self {
        Self {
            token: self.token.child_token(),
        }
    }

    /// Get the inner cancellation token.
    pub fn inner(&self) -> &CancellationToken {
        &self.token
    }

    /// Wait until cancelled.
    pub async fn cancelled(&self) {
        self.token.cancelled().await
    }
}

impl Default for RequestCancellation {
    fn default() -> Self {
        Self::new()
    }
}

/// Python-exposed cancellation token.
#[pyclass]
#[derive(Clone)]
pub struct PyCancellation {
    inner: RequestCancellation,
}

#[pymethods]
impl PyCancellation {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: RequestCancellation::new(),
        }
    }

    /// Cancel the operation.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.inner.is_cancelled()
    }
}

impl Default for PyCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl PyCancellation {
    pub fn inner(&self) -> &RequestCancellation {
        &self.inner
    }
}

/// Server metrics for monitoring.
#[derive(Default)]
pub struct ServerMetrics {
    /// Total requests processed.
    pub total_requests: AtomicU64,
    /// Active connections.
    pub active_connections: AtomicU64,
    /// Total bytes received.
    pub bytes_received: AtomicU64,
    /// Total bytes sent.
    pub bytes_sent: AtomicU64,
    /// Total errors.
    pub error_count: AtomicU64,
    /// Timeout count.
    pub timeout_count: AtomicU64,
}

impl ServerMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn increment_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_errors(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_timeouts(&self) {
        self.timeout_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn set_active_connections(&self, count: u64) {
        self.active_connections.store(count, Ordering::Relaxed);
    }

    /// Get metrics as a snapshot.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            timeout_count: self.timeout_count.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of server metrics.
#[derive(Debug, Clone)]
#[pyclass]
pub struct MetricsSnapshot {
    #[pyo3(get)]
    pub total_requests: u64,
    #[pyo3(get)]
    pub active_connections: u64,
    #[pyo3(get)]
    pub bytes_received: u64,
    #[pyo3(get)]
    pub bytes_sent: u64,
    #[pyo3(get)]
    pub error_count: u64,
    #[pyo3(get)]
    pub timeout_count: u64,
}

#[pymethods]
impl MetricsSnapshot {
    fn __repr__(&self) -> String {
        format!(
            "MetricsSnapshot(requests={}, connections={}, errors={}, timeouts={})",
            self.total_requests, self.active_connections, self.error_count, self.timeout_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size_string() {
        assert_eq!(parse_size_string("1024"), Some(1024));
        assert_eq!(parse_size_string("8kb"), Some(8 * 1024));
        assert_eq!(parse_size_string("8KB"), Some(8 * 1024));
        assert_eq!(parse_size_string("10mb"), Some(10 * 1024 * 1024));
        assert_eq!(parse_size_string("1gb"), Some(1024 * 1024 * 1024));
        assert_eq!(parse_size_string("100b"), Some(100));
        assert_eq!(parse_size_string("invalid"), None);
    }

    #[test]
    fn test_timeout_config_defaults() {
        let config = TimeoutConfig::default();
        assert_eq!(config.read_header_timeout_ms, 5000);
        assert_eq!(config.handler_timeout_ms, 30000);
    }

    #[test]
    fn test_limits_config_defaults() {
        let config = LimitsConfig::default();
        assert_eq!(config.max_header_size, 8 * 1024);
        assert_eq!(config.max_body_size, 10 * 1024 * 1024);
        assert_eq!(config.max_connections, 10000);
    }

    #[test]
    fn test_route_timeouts() {
        let default = TimeoutConfig::default();
        let route = RouteTimeouts::new().with_handler_timeout(60000);

        assert_eq!(route.handler_timeout(&default), Duration::from_millis(60000));
        assert_eq!(route.read_body_timeout(&default), Duration::from_millis(30000)); // Falls back to default
    }

    #[test]
    fn test_cancellation_token() {
        let token = RequestCancellation::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_server_metrics() {
        let metrics = ServerMetrics::new();
        metrics.increment_requests();
        metrics.increment_requests();
        metrics.increment_errors();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_requests, 2);
        assert_eq!(snapshot.error_count, 1);
    }
}
