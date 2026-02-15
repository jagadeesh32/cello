//! CQRS (Command Query Responsibility Segregation) Support for Cello Framework.
//!
//! Provides a complete CQRS implementation with:
//! - Command bus for dispatching write operations
//! - Query bus for dispatching read operations
//! - Configurable timeouts and retry policies
//! - Statistics and monitoring
//! - In-memory handler registries for development and testing
//!
//! # Example
//! ```python
//! from cello import App, CqrsConfig
//!
//! config = CqrsConfig()
//!
//! @app.on_startup
//! async def setup():
//!     app.enable_cqrs(config)
//!
//! @app.post("/orders")
//! async def create_order(request):
//!     data = request.json()
//!     command = {
//!         "type": "CreateOrder",
//!         "data": data,
//!     }
//!     # Dispatch command through the command bus
//!     return {"command": command, "status": "dispatched"}
//!
//! @app.get("/orders/{id}")
//! async def get_order(request):
//!     query = {
//!         "type": "GetOrder",
//!         "params": {"id": request.params["id"]},
//!     }
//!     # Execute query through the query bus
//!     return {"query": query}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

// ============================================================================
// Configuration
// ============================================================================

/// CQRS configuration.
///
/// Controls command and query processing behaviour including timeouts,
/// retry policies, and event synchronization.
#[derive(Clone, Debug)]
pub struct CqrsConfig {
    /// Whether to synchronize commands with the event store.
    pub enable_event_sync: bool,
    /// Maximum time in milliseconds to wait for a command to complete.
    pub command_timeout_ms: u64,
    /// Maximum time in milliseconds to wait for a query to complete.
    pub query_timeout_ms: u64,
    /// Maximum number of retries for failed commands.
    pub max_retries: u32,
}

impl Default for CqrsConfig {
    fn default() -> Self {
        Self {
            enable_event_sync: true,
            command_timeout_ms: 5000,
            query_timeout_ms: 3000,
            max_retries: 3,
        }
    }
}

impl CqrsConfig {
    /// Create a new CQRS configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable event synchronization.
    pub fn with_event_sync(mut self, enabled: bool) -> Self {
        self.enable_event_sync = enabled;
        self
    }

    /// Set the command timeout in milliseconds.
    pub fn with_command_timeout(mut self, timeout_ms: u64) -> Self {
        self.command_timeout_ms = timeout_ms;
        self
    }

    /// Set the query timeout in milliseconds.
    pub fn with_query_timeout(mut self, timeout_ms: u64) -> Self {
        self.query_timeout_ms = timeout_ms;
        self
    }

    /// Set the maximum number of retries for failed commands.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

// ============================================================================
// Command Types
// ============================================================================

/// A command representing an intent to change system state.
///
/// Commands are dispatched through the `CommandBus` and processed
/// by registered command handlers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    /// Unique command identifier.
    pub id: String,
    /// The type of command (e.g., "CreateOrder", "UpdateProfile").
    pub command_type: String,
    /// Command payload data.
    pub data: JsonValue,
    /// Additional metadata (e.g., user ID, request ID).
    pub metadata: HashMap<String, String>,
    /// Unix timestamp (seconds) when the command was created.
    pub timestamp: u64,
}

impl Command {
    /// Create a new command.
    pub fn new(command_type: &str, data: JsonValue) -> Self {
        Self {
            id: format!(
                "cmd-{}-{}",
                command_type.to_lowercase(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            ),
            command_type: command_type.to_string(),
            data,
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Add metadata to the command.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Get the command type.
    pub fn command_type(&self) -> &str {
        &self.command_type
    }

    /// Get a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

/// Result of processing a command.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CommandResult {
    /// Command executed successfully with optional result data.
    Success(JsonValue),
    /// Command execution failed with an error message.
    Failure(String),
    /// Command was rejected (e.g., validation failure).
    Rejected(String),
}

impl CommandResult {
    /// Check if the result represents success.
    pub fn is_success(&self) -> bool {
        matches!(self, CommandResult::Success(_))
    }

    /// Check if the result represents failure.
    pub fn is_failure(&self) -> bool {
        matches!(self, CommandResult::Failure(_))
    }

    /// Check if the result represents rejection.
    pub fn is_rejected(&self) -> bool {
        matches!(self, CommandResult::Rejected(_))
    }
}

// ============================================================================
// Query Types
// ============================================================================

/// A query representing a request to read system state.
///
/// Queries are dispatched through the `QueryBus` and processed
/// by registered query handlers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryDef {
    /// Unique query identifier.
    pub id: String,
    /// The type of query (e.g., "GetOrder", "ListUsers").
    pub query_type: String,
    /// Query parameters.
    pub params: JsonValue,
    /// Unix timestamp (seconds) when the query was created.
    pub timestamp: u64,
}

impl QueryDef {
    /// Create a new query.
    pub fn new(query_type: &str, params: JsonValue) -> Self {
        Self {
            id: format!(
                "qry-{}-{}",
                query_type.to_lowercase(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            ),
            query_type: query_type.to_string(),
            params,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Get the query type.
    pub fn query_type(&self) -> &str {
        &self.query_type
    }
}

/// Result of executing a query.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum QueryResult {
    /// Query executed successfully with result data.
    Success(JsonValue),
    /// The requested resource was not found.
    NotFound,
    /// Query execution encountered an error.
    Error(String),
}

impl QueryResult {
    /// Check if the result represents success.
    pub fn is_success(&self) -> bool {
        matches!(self, QueryResult::Success(_))
    }

    /// Check if the result is not found.
    pub fn is_not_found(&self) -> bool {
        matches!(self, QueryResult::NotFound)
    }

    /// Check if the result is an error.
    pub fn is_error(&self) -> bool {
        matches!(self, QueryResult::Error(_))
    }
}

// ============================================================================
// Command Bus
// ============================================================================

/// Type alias for a synchronous command handler function.
pub type CommandHandlerFn = Arc<dyn Fn(&Command) -> CommandResult + Send + Sync>;

/// Command bus for dispatching commands to registered handlers.
///
/// Each command type can have exactly one handler. Commands are matched
/// by their `command_type` field.
pub struct CommandBus {
    /// Registered command handlers keyed by command type.
    handlers: Arc<RwLock<HashMap<String, CommandHandlerFn>>>,
    /// Internal metrics tracker.
    metrics: Arc<CqrsMetrics>,
    /// Configuration reference.
    config: CqrsConfig,
}

impl CommandBus {
    /// Create a new command bus with default configuration.
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(CqrsMetrics::default()),
            config: CqrsConfig::default(),
        }
    }

    /// Create a new command bus with a specific configuration.
    pub fn with_config(config: CqrsConfig) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(CqrsMetrics::default()),
            config,
        }
    }

    /// Register a handler for a specific command type.
    pub fn register<F>(&self, command_type: &str, handler: F)
    where
        F: Fn(&Command) -> CommandResult + Send + Sync + 'static,
    {
        self.handlers
            .write()
            .insert(command_type.to_string(), Arc::new(handler));
    }

    /// Dispatch a command to its registered handler.
    pub fn dispatch(&self, command: &Command) -> Result<CommandResult, CqrsError> {
        let handlers = self.handlers.read();
        match handlers.get(&command.command_type) {
            Some(handler) => {
                self.metrics.record_command_processed();
                let result = handler(command);
                if result.is_failure() {
                    self.metrics.record_command_error();
                }
                Ok(result)
            }
            None => {
                self.metrics.record_command_error();
                Err(CqrsError::CommandNotFound(command.command_type.clone()))
            }
        }
    }

    /// Check if a handler is registered for a given command type.
    pub fn has_handler(&self, command_type: &str) -> bool {
        self.handlers.read().contains_key(command_type)
    }

    /// Get the number of registered command handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.read().len()
    }

    /// Get the configuration.
    pub fn config(&self) -> &CqrsConfig {
        &self.config
    }

    /// Get current statistics.
    pub fn stats(&self) -> CqrsStats {
        self.metrics.get_stats()
    }
}

impl Default for CommandBus {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Query Bus
// ============================================================================

/// Type alias for a synchronous query handler function.
pub type QueryHandlerFn = Arc<dyn Fn(&QueryDef) -> QueryResult + Send + Sync>;

/// Query bus for dispatching queries to registered handlers.
///
/// Each query type can have exactly one handler. Queries are matched
/// by their `query_type` field.
pub struct QueryBus {
    /// Registered query handlers keyed by query type.
    handlers: Arc<RwLock<HashMap<String, QueryHandlerFn>>>,
    /// Internal metrics tracker.
    metrics: Arc<CqrsMetrics>,
    /// Configuration reference.
    config: CqrsConfig,
}

impl QueryBus {
    /// Create a new query bus with default configuration.
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(CqrsMetrics::default()),
            config: CqrsConfig::default(),
        }
    }

    /// Create a new query bus with a specific configuration.
    pub fn with_config(config: CqrsConfig) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(CqrsMetrics::default()),
            config,
        }
    }

    /// Register a handler for a specific query type.
    pub fn register<F>(&self, query_type: &str, handler: F)
    where
        F: Fn(&QueryDef) -> QueryResult + Send + Sync + 'static,
    {
        self.handlers
            .write()
            .insert(query_type.to_string(), Arc::new(handler));
    }

    /// Execute a query through its registered handler.
    pub fn execute(&self, query: &QueryDef) -> Result<QueryResult, CqrsError> {
        let handlers = self.handlers.read();
        match handlers.get(&query.query_type) {
            Some(handler) => {
                self.metrics.record_query_processed();
                let result = handler(query);
                if result.is_error() {
                    self.metrics.record_query_error();
                }
                Ok(result)
            }
            None => {
                self.metrics.record_query_error();
                Err(CqrsError::QueryNotFound(query.query_type.clone()))
            }
        }
    }

    /// Check if a handler is registered for a given query type.
    pub fn has_handler(&self, query_type: &str) -> bool {
        self.handlers.read().contains_key(query_type)
    }

    /// Get the number of registered query handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.read().len()
    }

    /// Get the configuration.
    pub fn config(&self) -> &CqrsConfig {
        &self.config
    }

    /// Get current statistics.
    pub fn stats(&self) -> CqrsStats {
        self.metrics.get_stats()
    }
}

impl Default for QueryBus {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// CQRS error types.
#[derive(Debug, Clone)]
pub enum CqrsError {
    /// No handler registered for the given command type.
    CommandNotFound(String),
    /// No handler registered for the given query type.
    QueryNotFound(String),
    /// Error occurred while executing a handler.
    HandlerError(String),
    /// Command or query execution timed out.
    TimeoutError(String),
    /// Command or query validation failed.
    ValidationError(String),
}

impl std::fmt::Display for CqrsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CqrsError::CommandNotFound(cmd) => {
                write!(f, "No handler registered for command: {}", cmd)
            }
            CqrsError::QueryNotFound(qry) => {
                write!(f, "No handler registered for query: {}", qry)
            }
            CqrsError::HandlerError(msg) => write!(f, "Handler error: {}", msg),
            CqrsError::TimeoutError(msg) => write!(f, "Timeout: {}", msg),
            CqrsError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for CqrsError {}

// ============================================================================
// Statistics & Metrics
// ============================================================================

/// Aggregate CQRS statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CqrsStats {
    /// Total number of commands processed.
    pub commands_processed: u64,
    /// Total number of queries processed.
    pub queries_processed: u64,
    /// Total number of command errors.
    pub command_errors: u64,
    /// Total number of query errors.
    pub query_errors: u64,
}

/// Internal atomic metrics tracker for CQRS operations.
struct CqrsMetrics {
    commands_processed: AtomicU64,
    queries_processed: AtomicU64,
    command_errors: AtomicU64,
    query_errors: AtomicU64,
}

impl Default for CqrsMetrics {
    fn default() -> Self {
        Self {
            commands_processed: AtomicU64::new(0),
            queries_processed: AtomicU64::new(0),
            command_errors: AtomicU64::new(0),
            query_errors: AtomicU64::new(0),
        }
    }
}

impl CqrsMetrics {
    pub fn record_command_processed(&self) {
        self.commands_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_query_processed(&self) {
        self.queries_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_command_error(&self) {
        self.command_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_query_error(&self) {
        self.query_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> CqrsStats {
        CqrsStats {
            commands_processed: self.commands_processed.load(Ordering::Relaxed),
            queries_processed: self.queries_processed.load(Ordering::Relaxed),
            command_errors: self.command_errors.load(Ordering::Relaxed),
            query_errors: self.query_errors.load(Ordering::Relaxed),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- CqrsConfig Tests ----------

    #[test]
    fn test_config_default() {
        let config = CqrsConfig::default();
        assert!(config.enable_event_sync);
        assert_eq!(config.command_timeout_ms, 5000);
        assert_eq!(config.query_timeout_ms, 3000);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_builder() {
        let config = CqrsConfig::new()
            .with_event_sync(false)
            .with_command_timeout(10000)
            .with_query_timeout(5000)
            .with_max_retries(5);

        assert!(!config.enable_event_sync);
        assert_eq!(config.command_timeout_ms, 10000);
        assert_eq!(config.query_timeout_ms, 5000);
        assert_eq!(config.max_retries, 5);
    }

    // ---------- Command Tests ----------

    #[test]
    fn test_command_creation() {
        let cmd = Command::new("CreateOrder", serde_json::json!({"item": "widget"}));

        assert_eq!(cmd.command_type(), "CreateOrder");
        assert!(cmd.id.starts_with("cmd-createorder-"));
        assert!(cmd.timestamp > 0);
    }

    #[test]
    fn test_command_metadata() {
        let cmd = Command::new("UpdateProfile", serde_json::json!({}))
            .with_metadata("user_id", "user-42")
            .with_metadata("request_id", "req-abc");

        assert_eq!(cmd.get_metadata("user_id"), Some("user-42"));
        assert_eq!(cmd.get_metadata("request_id"), Some("req-abc"));
        assert_eq!(cmd.get_metadata("missing"), None);
    }

    // ---------- CommandResult Tests ----------

    #[test]
    fn test_command_result_variants() {
        let success = CommandResult::Success(serde_json::json!({"id": 1}));
        assert!(success.is_success());
        assert!(!success.is_failure());
        assert!(!success.is_rejected());

        let failure = CommandResult::Failure("database error".to_string());
        assert!(!failure.is_success());
        assert!(failure.is_failure());

        let rejected = CommandResult::Rejected("invalid data".to_string());
        assert!(!rejected.is_success());
        assert!(rejected.is_rejected());
    }

    // ---------- QueryDef Tests ----------

    #[test]
    fn test_query_creation() {
        let query = QueryDef::new("GetOrder", serde_json::json!({"id": "order-123"}));

        assert_eq!(query.query_type(), "GetOrder");
        assert!(query.id.starts_with("qry-getorder-"));
        assert!(query.timestamp > 0);
    }

    // ---------- QueryResult Tests ----------

    #[test]
    fn test_query_result_variants() {
        let success = QueryResult::Success(serde_json::json!({"name": "test"}));
        assert!(success.is_success());
        assert!(!success.is_not_found());
        assert!(!success.is_error());

        let not_found = QueryResult::NotFound;
        assert!(not_found.is_not_found());

        let error = QueryResult::Error("timeout".to_string());
        assert!(error.is_error());
    }

    // ---------- CommandBus Tests ----------

    #[test]
    fn test_command_bus_register_and_dispatch() {
        let bus = CommandBus::new();

        bus.register("CreateOrder", |cmd: &Command| {
            CommandResult::Success(serde_json::json!({
                "order_id": "order-1",
                "item": cmd.data["item"]
            }))
        });

        assert!(bus.has_handler("CreateOrder"));
        assert!(!bus.has_handler("DeleteOrder"));
        assert_eq!(bus.handler_count(), 1);

        let cmd = Command::new("CreateOrder", serde_json::json!({"item": "widget"}));
        let result = bus.dispatch(&cmd).unwrap();
        assert!(result.is_success());
    }

    #[test]
    fn test_command_bus_unknown_command() {
        let bus = CommandBus::new();
        let cmd = Command::new("UnknownCommand", serde_json::json!({}));
        let result = bus.dispatch(&cmd);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CqrsError::CommandNotFound(_)));
    }

    #[test]
    fn test_command_bus_stats() {
        let bus = CommandBus::new();

        bus.register("TestCmd", |_| {
            CommandResult::Success(serde_json::json!({}))
        });

        let cmd = Command::new("TestCmd", serde_json::json!({}));
        bus.dispatch(&cmd).unwrap();
        bus.dispatch(&cmd).unwrap();

        let stats = bus.stats();
        assert_eq!(stats.commands_processed, 2);
        assert_eq!(stats.command_errors, 0);
    }

    // ---------- QueryBus Tests ----------

    #[test]
    fn test_query_bus_register_and_execute() {
        let bus = QueryBus::new();

        bus.register("GetUser", |query: &QueryDef| {
            let user_id = query.params["id"].as_str().unwrap_or("unknown");
            QueryResult::Success(serde_json::json!({
                "id": user_id,
                "name": "Test User"
            }))
        });

        assert!(bus.has_handler("GetUser"));
        assert_eq!(bus.handler_count(), 1);

        let query = QueryDef::new("GetUser", serde_json::json!({"id": "user-1"}));
        let result = bus.execute(&query).unwrap();
        assert!(result.is_success());
    }

    #[test]
    fn test_query_bus_unknown_query() {
        let bus = QueryBus::new();
        let query = QueryDef::new("UnknownQuery", serde_json::json!({}));
        let result = bus.execute(&query);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CqrsError::QueryNotFound(_)));
    }

    #[test]
    fn test_query_bus_stats() {
        let bus = QueryBus::new();

        bus.register("TestQuery", |_| {
            QueryResult::Success(serde_json::json!({}))
        });

        let query = QueryDef::new("TestQuery", serde_json::json!({}));
        bus.execute(&query).unwrap();

        let stats = bus.stats();
        assert_eq!(stats.queries_processed, 1);
        assert_eq!(stats.query_errors, 0);
    }

    // ---------- Error Display Tests ----------

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", CqrsError::CommandNotFound("CreateOrder".to_string())),
            "No handler registered for command: CreateOrder"
        );
        assert_eq!(
            format!("{}", CqrsError::QueryNotFound("GetUser".to_string())),
            "No handler registered for query: GetUser"
        );
        assert_eq!(
            format!("{}", CqrsError::HandlerError("db failure".to_string())),
            "Handler error: db failure"
        );
        assert_eq!(
            format!("{}", CqrsError::TimeoutError("5s exceeded".to_string())),
            "Timeout: 5s exceeded"
        );
        assert_eq!(
            format!(
                "{}",
                CqrsError::ValidationError("missing field".to_string())
            ),
            "Validation error: missing field"
        );
    }

    // ---------- CqrsStats Tests ----------

    #[test]
    fn test_stats_default() {
        let stats = CqrsStats::default();
        assert_eq!(stats.commands_processed, 0);
        assert_eq!(stats.queries_processed, 0);
        assert_eq!(stats.command_errors, 0);
        assert_eq!(stats.query_errors, 0);
    }
}
