//! Database Connection Pooling Support.
//!
//! Provides connection pooling for various databases with:
//! - Async connection management
//! - Health monitoring
//! - Connection lifecycle management
//! - Query statistics
//!
//! # Example
//! ```python
//! from cello import App, Depends
//! from cello.database import DatabasePool, DatabaseConfig
//!
//! config = DatabaseConfig(
//!     url="postgresql://localhost/mydb",
//!     pool_size=20,
//!     max_lifetime=1800
//! )
//!
//! @app.on_startup
//! async def setup():
//!     app.state.db = await DatabasePool.connect(config)
//!
//! @app.get("/users")
//! async def get_users(request, db=Depends("database")):
//!     rows = await db.fetch_all("SELECT * FROM users")
//!     return {"users": rows}
//! ```

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Database configuration.
#[derive(Clone, Debug)]
pub struct DatabaseConfig {
    /// Connection URL (e.g., postgresql://user:pass@host:port/db)
    pub url: String,
    /// Maximum number of connections in the pool
    pub pool_size: usize,
    /// Minimum number of idle connections
    pub min_idle: usize,
    /// Maximum lifetime of a connection
    pub max_lifetime: Duration,
    /// Maximum time to wait for a connection
    pub connection_timeout: Duration,
    /// Idle timeout before closing a connection
    pub idle_timeout: Duration,
    /// Enable statement caching
    pub statement_cache_size: usize,
    /// Application name for connection identification
    pub application_name: Option<String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            pool_size: 10,
            min_idle: 1,
            max_lifetime: Duration::from_secs(1800), // 30 minutes
            connection_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            statement_cache_size: 100,
            application_name: None,
        }
    }
}

impl DatabaseConfig {
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

    pub fn min_idle(mut self, min: usize) -> Self {
        self.min_idle = min;
        self
    }

    pub fn max_lifetime(mut self, lifetime: Duration) -> Self {
        self.max_lifetime = lifetime;
        self
    }

    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    pub fn application_name(mut self, name: &str) -> Self {
        self.application_name = Some(name.to_string());
        self
    }
}

/// Database connection statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatabaseStats {
    /// Total connections created
    pub total_connections: u64,
    /// Currently active connections
    pub active_connections: usize,
    /// Currently idle connections
    pub idle_connections: usize,
    /// Total queries executed
    pub total_queries: u64,
    /// Total query errors
    pub total_errors: u64,
    /// Average query time in milliseconds
    pub avg_query_time_ms: f64,
    /// Peak active connections
    pub peak_active_connections: usize,
    /// Connection timeouts
    pub connection_timeouts: u64,
}

/// Connection pool metrics.
pub struct PoolMetrics {
    total_connections: AtomicU64,
    active_connections: AtomicUsize,
    idle_connections: AtomicUsize,
    total_queries: AtomicU64,
    total_errors: AtomicU64,
    query_time_sum_ms: AtomicU64,
    peak_active: AtomicUsize,
    connection_timeouts: AtomicU64,
}

impl Default for PoolMetrics {
    fn default() -> Self {
        Self {
            total_connections: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            idle_connections: AtomicUsize::new(0),
            total_queries: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            query_time_sum_ms: AtomicU64::new(0),
            peak_active: AtomicUsize::new(0),
            connection_timeouts: AtomicU64::new(0),
        }
    }
}

impl PoolMetrics {
    pub fn record_connection(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        let active = self.active_connections.fetch_add(1, Ordering::Relaxed) + 1;

        // Update peak if needed
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

    pub fn record_query(&self, duration_ms: u64, error: bool) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        self.query_time_sum_ms.fetch_add(duration_ms, Ordering::Relaxed);
        if error {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_timeout(&self) {
        self.connection_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> DatabaseStats {
        let total_queries = self.total_queries.load(Ordering::Relaxed);
        let query_time_sum = self.query_time_sum_ms.load(Ordering::Relaxed);

        DatabaseStats {
            total_connections: self.total_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            idle_connections: self.idle_connections.load(Ordering::Relaxed),
            total_queries,
            total_errors: self.total_errors.load(Ordering::Relaxed),
            avg_query_time_ms: if total_queries > 0 {
                query_time_sum as f64 / total_queries as f64
            } else {
                0.0
            },
            peak_active_connections: self.peak_active.load(Ordering::Relaxed),
            connection_timeouts: self.connection_timeouts.load(Ordering::Relaxed),
        }
    }
}

/// Generic database pool trait.
pub trait DatabasePool: Send + Sync {
    /// Get a connection from the pool.
    fn get_connection(&self) -> Result<Box<dyn DatabaseConnection>, DatabaseError>;

    /// Check if the pool is healthy.
    fn is_healthy(&self) -> bool;

    /// Get pool statistics.
    fn stats(&self) -> DatabaseStats;

    /// Close all connections.
    fn close(&self);
}

/// Generic database connection trait.
pub trait DatabaseConnection: Send + Sync {
    /// Execute a query that doesn't return rows.
    fn execute(&self, query: &str, params: &[&dyn ToSql]) -> Result<u64, DatabaseError>;

    /// Execute a query and return rows.
    fn query(&self, query: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>, DatabaseError>;

    /// Execute a query and return a single row.
    fn query_one(&self, query: &str, params: &[&dyn ToSql]) -> Result<Option<Row>, DatabaseError>;

    /// Begin a transaction.
    fn begin(&self) -> Result<Box<dyn Transaction>, DatabaseError>;

    /// Return the connection to the pool.
    fn release(self: Box<Self>);
}

/// Transaction trait.
pub trait Transaction: Send + Sync {
    /// Commit the transaction.
    fn commit(self: Box<Self>) -> Result<(), DatabaseError>;

    /// Rollback the transaction.
    fn rollback(self: Box<Self>) -> Result<(), DatabaseError>;

    /// Execute a query within the transaction.
    fn execute(&self, query: &str, params: &[&dyn ToSql]) -> Result<u64, DatabaseError>;

    /// Query within the transaction.
    fn query(&self, query: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>, DatabaseError>;
}

/// Trait for SQL parameter conversion.
pub trait ToSql: Send + Sync {
    fn to_sql(&self) -> SqlValue;
}

/// SQL value types.
#[derive(Debug, Clone)]
pub enum SqlValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Timestamp(chrono::DateTime<chrono::Utc>),
    Uuid(uuid::Uuid),
    Json(serde_json::Value),
}

impl ToSql for i32 {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Int(*self as i64)
    }
}

impl ToSql for i64 {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Int(*self)
    }
}

impl ToSql for f64 {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Float(*self)
    }
}

impl ToSql for String {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Text(self.clone())
    }
}

impl ToSql for &str {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Text(self.to_string())
    }
}

impl ToSql for bool {
    fn to_sql(&self) -> SqlValue {
        SqlValue::Bool(*self)
    }
}

impl<T: ToSql> ToSql for Option<T> {
    fn to_sql(&self) -> SqlValue {
        match self {
            Some(v) => v.to_sql(),
            None => SqlValue::Null,
        }
    }
}

/// Database row.
#[derive(Debug, Clone)]
pub struct Row {
    columns: HashMap<String, SqlValue>,
}

impl Row {
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
        }
    }

    pub fn with_column(mut self, name: &str, value: SqlValue) -> Self {
        self.columns.insert(name.to_string(), value);
        self
    }

    pub fn get<T: FromSql>(&self, column: &str) -> Option<T> {
        self.columns.get(column).and_then(|v| T::from_sql(v))
    }

    pub fn get_raw(&self, column: &str) -> Option<&SqlValue> {
        self.columns.get(column)
    }

    pub fn columns(&self) -> Vec<&str> {
        self.columns.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for converting SQL values to Rust types.
pub trait FromSql: Sized {
    fn from_sql(value: &SqlValue) -> Option<Self>;
}

impl FromSql for i32 {
    fn from_sql(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Int(v) => Some(*v as i32),
            _ => None,
        }
    }
}

impl FromSql for i64 {
    fn from_sql(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Int(v) => Some(*v),
            _ => None,
        }
    }
}

impl FromSql for f64 {
    fn from_sql(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Float(v) => Some(*v),
            SqlValue::Int(v) => Some(*v as f64),
            _ => None,
        }
    }
}

impl FromSql for String {
    fn from_sql(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Text(v) => Some(v.clone()),
            _ => None,
        }
    }
}

impl FromSql for bool {
    fn from_sql(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

impl<T: FromSql> FromSql for Option<T> {
    fn from_sql(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Null => Some(None),
            v => Some(T::from_sql(v)),
        }
    }
}

/// Database error types.
#[derive(Debug, Clone)]
pub enum DatabaseError {
    /// Connection error
    Connection(String),
    /// Query error
    Query(String),
    /// Pool exhausted
    PoolExhausted,
    /// Connection timeout
    Timeout,
    /// Transaction error
    Transaction(String),
    /// Configuration error
    Config(String),
    /// Unknown error
    Unknown(String),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::Connection(msg) => write!(f, "Connection error: {}", msg),
            DatabaseError::Query(msg) => write!(f, "Query error: {}", msg),
            DatabaseError::PoolExhausted => write!(f, "Connection pool exhausted"),
            DatabaseError::Timeout => write!(f, "Connection timeout"),
            DatabaseError::Transaction(msg) => write!(f, "Transaction error: {}", msg),
            DatabaseError::Config(msg) => write!(f, "Configuration error: {}", msg),
            DatabaseError::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for DatabaseError {}

/// In-memory mock database pool for testing.
pub struct MockDatabasePool {
    #[allow(dead_code)]
    config: DatabaseConfig,
    metrics: Arc<PoolMetrics>,
    data: Arc<RwLock<HashMap<String, Vec<Row>>>>,
    healthy: AtomicBool,
}

use std::sync::atomic::AtomicBool;

impl MockDatabasePool {
    pub fn new(config: DatabaseConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(PoolMetrics::default()),
            data: Arc::new(RwLock::new(HashMap::new())),
            healthy: AtomicBool::new(true),
        }
    }

    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::SeqCst);
    }

    pub fn insert_data(&self, table: &str, rows: Vec<Row>) {
        let mut data = self.data.write();
        data.insert(table.to_string(), rows);
    }
}

impl DatabasePool for MockDatabasePool {
    fn get_connection(&self) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        if !self.healthy.load(Ordering::SeqCst) {
            return Err(DatabaseError::Connection("Pool is unhealthy".to_string()));
        }
        self.metrics.record_connection();
        Ok(Box::new(MockConnection {
            data: Arc::clone(&self.data),
            metrics: Arc::clone(&self.metrics),
        }))
    }

    fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    fn stats(&self) -> DatabaseStats {
        self.metrics.get_stats()
    }

    fn close(&self) {
        // No-op for mock
    }
}

struct MockConnection {
    data: Arc<RwLock<HashMap<String, Vec<Row>>>>,
    metrics: Arc<PoolMetrics>,
}

impl DatabaseConnection for MockConnection {
    fn execute(&self, _query: &str, _params: &[&dyn ToSql]) -> Result<u64, DatabaseError> {
        let start = Instant::now();
        self.metrics.record_query(start.elapsed().as_millis() as u64, false);
        Ok(1)
    }

    fn query(&self, query: &str, _params: &[&dyn ToSql]) -> Result<Vec<Row>, DatabaseError> {
        let start = Instant::now();

        // Simple mock: extract table name from "SELECT * FROM table"
        let query_lower = query.to_lowercase();
        let table = query_lower
            .split("from")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or("");

        let data = self.data.read();
        let rows = data.get(table).cloned().unwrap_or_default();

        self.metrics.record_query(start.elapsed().as_millis() as u64, false);
        Ok(rows)
    }

    fn query_one(&self, query: &str, params: &[&dyn ToSql]) -> Result<Option<Row>, DatabaseError> {
        let rows = self.query(query, params)?;
        Ok(rows.into_iter().next())
    }

    fn begin(&self) -> Result<Box<dyn Transaction>, DatabaseError> {
        Ok(Box::new(MockTransaction {
            data: Arc::clone(&self.data),
            metrics: Arc::clone(&self.metrics),
        }))
    }

    fn release(self: Box<Self>) {
        self.metrics.release_connection();
    }
}

struct MockTransaction {
    data: Arc<RwLock<HashMap<String, Vec<Row>>>>,
    metrics: Arc<PoolMetrics>,
}

impl Transaction for MockTransaction {
    fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn execute(&self, _query: &str, _params: &[&dyn ToSql]) -> Result<u64, DatabaseError> {
        let start = Instant::now();
        self.metrics.record_query(start.elapsed().as_millis() as u64, false);
        Ok(1)
    }

    fn query(&self, query: &str, _params: &[&dyn ToSql]) -> Result<Vec<Row>, DatabaseError> {
        let start = Instant::now();

        let query_lower = query.to_lowercase();
        let table = query_lower
            .split("from")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or("");

        let data = self.data.read();
        let rows = data.get(table).cloned().unwrap_or_default();

        self.metrics.record_query(start.elapsed().as_millis() as u64, false);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = DatabaseConfig::new("postgresql://localhost/test")
            .pool_size(20)
            .min_idle(5)
            .application_name("test-app");

        assert_eq!(config.url, "postgresql://localhost/test");
        assert_eq!(config.pool_size, 20);
        assert_eq!(config.min_idle, 5);
        assert_eq!(config.application_name, Some("test-app".to_string()));
    }

    #[test]
    fn test_mock_pool() {
        let pool = MockDatabasePool::new(DatabaseConfig::new("mock://test"));

        // Insert test data
        pool.insert_data(
            "users",
            vec![
                Row::new()
                    .with_column("id", SqlValue::Int(1))
                    .with_column("name", SqlValue::Text("Alice".to_string())),
                Row::new()
                    .with_column("id", SqlValue::Int(2))
                    .with_column("name", SqlValue::Text("Bob".to_string())),
            ],
        );

        // Get connection and query
        let conn = pool.get_connection().unwrap();
        let rows = conn.query("SELECT * FROM users", &[]).unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<String>("name"), Some("Alice".to_string()));

        conn.release();

        // Check stats
        let stats = pool.stats();
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.total_queries, 1);
    }

    #[test]
    fn test_row_access() {
        let row = Row::new()
            .with_column("id", SqlValue::Int(42))
            .with_column("name", SqlValue::Text("Test".to_string()))
            .with_column("active", SqlValue::Bool(true));

        assert_eq!(row.get::<i64>("id"), Some(42));
        assert_eq!(row.get::<String>("name"), Some("Test".to_string()));
        assert_eq!(row.get::<bool>("active"), Some(true));
        assert_eq!(row.get::<String>("missing"), None);
    }
}
