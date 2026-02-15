//! Event Sourcing Support for Cello Framework.
//!
//! Provides a complete event sourcing implementation with:
//! - Immutable event log with append-only storage
//! - Aggregate state reconstruction from events
//! - Snapshot support for performance optimization
//! - In-memory event store for development and testing
//! - Configurable snapshot intervals and event TTL
//!
//! # Example
//! ```python
//! from cello import App, EventSourcingConfig
//!
//! config = EventSourcingConfig.memory()
//!
//! @app.on_startup
//! async def setup():
//!     app.enable_event_sourcing(config)
//!
//! @app.post("/accounts/{id}/deposit")
//! async def deposit(request):
//!     data = request.json()
//!     event = {
//!         "type": "MoneyDeposited",
//!         "amount": data["amount"],
//!         "aggregate_id": request.params["id"],
//!     }
//!     # Append event to the store
//!     return {"event": event, "status": "persisted"}
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

/// Event sourcing configuration.
///
/// Controls how the event store behaves, including the storage backend,
/// snapshot intervals, and event retention policies.
#[derive(Clone, Debug)]
pub struct EventSourcingConfig {
    /// Storage backend type (e.g., "memory", "postgresql").
    pub store_type: String,
    /// Number of events between automatic snapshots.
    pub snapshot_interval: u32,
    /// Whether snapshot creation is enabled.
    pub enable_snapshots: bool,
    /// Maximum number of events allowed per aggregate.
    pub max_events_per_aggregate: usize,
    /// Event time-to-live in seconds (0 = infinite).
    pub event_ttl_secs: u64,
    /// Optional connection URL for persistent stores.
    pub connection_url: Option<String>,
}

impl Default for EventSourcingConfig {
    fn default() -> Self {
        Self {
            store_type: "memory".to_string(),
            snapshot_interval: 100,
            enable_snapshots: true,
            max_events_per_aggregate: 10000,
            event_ttl_secs: 0,
            connection_url: None,
        }
    }
}

impl EventSourcingConfig {
    /// Create a new in-memory event sourcing configuration.
    pub fn memory() -> Self {
        Self {
            store_type: "memory".to_string(),
            ..Default::default()
        }
    }

    /// Create a PostgreSQL-backed event sourcing configuration.
    pub fn postgresql(url: &str) -> Self {
        Self {
            store_type: "postgresql".to_string(),
            connection_url: Some(url.to_string()),
            ..Default::default()
        }
    }

    /// Set the snapshot interval (number of events between snapshots).
    pub fn with_snapshot_interval(mut self, interval: u32) -> Self {
        self.snapshot_interval = interval;
        self
    }

    /// Enable or disable snapshot creation.
    pub fn with_snapshots(mut self, enabled: bool) -> Self {
        self.enable_snapshots = enabled;
        self
    }

    /// Set the maximum number of events per aggregate.
    pub fn with_max_events(mut self, max: usize) -> Self {
        self.max_events_per_aggregate = max;
        self
    }

    /// Set the event time-to-live in seconds (0 = infinite).
    pub fn with_event_ttl(mut self, ttl_secs: u64) -> Self {
        self.event_ttl_secs = ttl_secs;
        self
    }
}

// ============================================================================
// Event Types
// ============================================================================

/// An immutable domain event persisted in the event store.
///
/// Events represent state changes and are the source of truth in an
/// event-sourced system. Each event is associated with an aggregate
/// and carries a monotonically increasing version number.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Unique event identifier.
    pub id: String,
    /// Identifier of the aggregate this event belongs to.
    pub aggregate_id: String,
    /// The type of event (e.g., "OrderCreated", "ItemAdded").
    pub event_type: String,
    /// Event payload data.
    pub data: JsonValue,
    /// Additional metadata (e.g., user ID, correlation ID).
    pub metadata: HashMap<String, String>,
    /// Sequential version number within the aggregate.
    pub version: u64,
    /// Unix timestamp (seconds) when the event was created.
    pub timestamp: u64,
}

impl Event {
    /// Create a new event.
    pub fn new(
        aggregate_id: &str,
        event_type: &str,
        data: JsonValue,
        version: u64,
    ) -> Self {
        Self {
            id: format!("evt-{}-{}", aggregate_id, version),
            aggregate_id: aggregate_id.to_string(),
            event_type: event_type.to_string(),
            data,
            metadata: HashMap::new(),
            version,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create a new event with metadata.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Get the event type.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Get the aggregate ID.
    pub fn aggregate_id(&self) -> &str {
        &self.aggregate_id
    }

    /// Get the event version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Check if this event has a specific metadata key.
    pub fn has_metadata(&self, key: &str) -> bool {
        self.metadata.contains_key(key)
    }

    /// Get a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}

// ============================================================================
// Aggregate State
// ============================================================================

/// Represents the current state of an aggregate, reconstructed from events.
///
/// An aggregate is a cluster of domain objects treated as a single unit.
/// Its state is derived by replaying all events in order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AggregateState {
    /// Unique identifier for this aggregate instance.
    pub id: String,
    /// The type of aggregate (e.g., "Order", "Account").
    pub aggregate_type: String,
    /// Current version (number of applied events).
    pub version: u64,
    /// Current computed state as JSON.
    pub state: JsonValue,
    /// Uncommitted events pending persistence.
    #[serde(skip)]
    pub events: Vec<Event>,
}

impl AggregateState {
    /// Create a new aggregate with an initial empty state.
    pub fn new(id: &str, aggregate_type: &str) -> Self {
        Self {
            id: id.to_string(),
            aggregate_type: aggregate_type.to_string(),
            version: 0,
            state: JsonValue::Object(serde_json::Map::new()),
            events: Vec::new(),
        }
    }

    /// Create an aggregate from a snapshot.
    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
        Self {
            id: snapshot.aggregate_id.clone(),
            aggregate_type: String::new(),
            version: snapshot.version,
            state: snapshot.state.clone(),
            events: Vec::new(),
        }
    }

    /// Apply an event to update the aggregate state.
    ///
    /// The event is added to the uncommitted events list and the
    /// version is incremented. The caller is responsible for updating
    /// the `state` field based on domain logic.
    pub fn apply_event(&mut self, event: Event) {
        self.version = event.version;

        // Merge event data into current state
        if let (Some(state_map), Some(event_map)) =
            (self.state.as_object_mut(), event.data.as_object())
        {
            for (k, v) in event_map {
                state_map.insert(k.clone(), v.clone());
            }
        }

        self.events.push(event);
    }

    /// Get the current version of this aggregate.
    pub fn get_version(&self) -> u64 {
        self.version
    }

    /// Get a reference to uncommitted events.
    pub fn get_uncommitted_events(&self) -> &[Event] {
        &self.events
    }

    /// Clear uncommitted events after persistence.
    pub fn clear_uncommitted_events(&mut self) {
        self.events.clear();
    }

    /// Check if there are uncommitted events.
    pub fn has_uncommitted_events(&self) -> bool {
        !self.events.is_empty()
    }

    /// Get the number of uncommitted events.
    pub fn uncommitted_count(&self) -> usize {
        self.events.len()
    }
}

// ============================================================================
// Snapshot
// ============================================================================

/// A snapshot of an aggregate's state at a specific version.
///
/// Snapshots are used to optimize event replay by providing a
/// starting point closer to the current version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    /// Identifier of the aggregate this snapshot belongs to.
    pub aggregate_id: String,
    /// The aggregate version at the time of the snapshot.
    pub version: u64,
    /// The serialized aggregate state.
    pub state: JsonValue,
    /// Unix timestamp (seconds) when the snapshot was created.
    pub timestamp: u64,
}

impl Snapshot {
    /// Create a new snapshot from an aggregate state.
    pub fn from_aggregate(aggregate: &AggregateState) -> Self {
        Self {
            aggregate_id: aggregate.id.clone(),
            version: aggregate.version,
            state: aggregate.state.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

// ============================================================================
// Event Store Trait
// ============================================================================

/// Trait defining the interface for event persistence backends.
///
/// Implementations must be thread-safe (`Send + Sync`) to support
/// concurrent access from multiple request handlers.
pub trait EventStore: Send + Sync {
    /// Append one or more events to the store for a given aggregate.
    ///
    /// Returns an error if a concurrency conflict is detected (i.e.,
    /// the expected version does not match the current version).
    fn append_events(
        &self,
        aggregate_id: &str,
        events: &[Event],
        expected_version: u64,
    ) -> Result<(), EventSourcingError>;

    /// Retrieve all events for an aggregate, optionally starting from a version.
    fn get_events(
        &self,
        aggregate_id: &str,
        from_version: Option<u64>,
    ) -> Result<Vec<Event>, EventSourcingError>;

    /// Retrieve the latest snapshot for an aggregate, if one exists.
    fn get_snapshot(
        &self,
        aggregate_id: &str,
    ) -> Result<Option<Snapshot>, EventSourcingError>;

    /// Persist a snapshot for an aggregate.
    fn save_snapshot(
        &self,
        snapshot: &Snapshot,
    ) -> Result<(), EventSourcingError>;
}

// ============================================================================
// In-Memory Event Store
// ============================================================================

/// In-memory event store for development and testing.
///
/// All data is stored in memory and will be lost when the process exits.
/// Use this for local development, unit tests, and prototyping.
pub struct InMemoryEventStore {
    /// Events stored per aggregate ID.
    events: Arc<RwLock<HashMap<String, Vec<Event>>>>,
    /// Latest snapshot per aggregate ID.
    snapshots: Arc<RwLock<HashMap<String, Snapshot>>>,
    /// Internal metrics tracker.
    metrics: Arc<EventSourcingMetrics>,
    /// Configuration reference.
    config: EventSourcingConfig,
}

impl InMemoryEventStore {
    /// Create a new in-memory event store with default configuration.
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(EventSourcingMetrics::default()),
            config: EventSourcingConfig::default(),
        }
    }

    /// Create a new in-memory event store with a specific configuration.
    pub fn with_config(config: EventSourcingConfig) -> Self {
        Self {
            events: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(EventSourcingMetrics::default()),
            config,
        }
    }

    /// Get the total number of events across all aggregates.
    pub fn total_events(&self) -> usize {
        self.events.read().values().map(|v| v.len()).sum()
    }

    /// Get the number of tracked aggregates.
    pub fn total_aggregates(&self) -> usize {
        self.events.read().len()
    }

    /// Get the number of stored snapshots.
    pub fn total_snapshots(&self) -> usize {
        self.snapshots.read().len()
    }

    /// Get aggregate statistics.
    pub fn stats(&self) -> EventSourcingStats {
        self.metrics.get_stats(&self.events.read(), &self.snapshots.read())
    }

    /// Clear all stored events and snapshots.
    pub fn clear(&self) {
        self.events.write().clear();
        self.snapshots.write().clear();
    }
}

impl Default for InMemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EventStore for InMemoryEventStore {
    fn append_events(
        &self,
        aggregate_id: &str,
        events: &[Event],
        expected_version: u64,
    ) -> Result<(), EventSourcingError> {
        let mut store = self.events.write();
        let aggregate_events = store.entry(aggregate_id.to_string()).or_insert_with(Vec::new);

        // Check for concurrency conflicts.
        let current_version = aggregate_events.last().map(|e| e.version).unwrap_or(0);
        if current_version != expected_version {
            return Err(EventSourcingError::ConcurrencyConflict {
                aggregate_id: aggregate_id.to_string(),
                expected: expected_version,
                actual: current_version,
            });
        }

        // Check max events per aggregate.
        if aggregate_events.len() + events.len() > self.config.max_events_per_aggregate {
            return Err(EventSourcingError::StoreError(format!(
                "Aggregate '{}' would exceed max events limit of {}",
                aggregate_id, self.config.max_events_per_aggregate
            )));
        }

        for event in events {
            aggregate_events.push(event.clone());
            self.metrics.record_event_appended();
        }

        // Auto-snapshot if enabled and interval reached.
        if self.config.enable_snapshots && self.config.snapshot_interval > 0 {
            let new_version = aggregate_events.last().map(|e| e.version).unwrap_or(0);
            if new_version > 0 && new_version % (self.config.snapshot_interval as u64) == 0 {
                // Build a minimal aggregate state for the snapshot.
                let mut state = JsonValue::Object(serde_json::Map::new());
                for evt in aggregate_events.iter() {
                    if let (Some(state_map), Some(event_map)) =
                        (state.as_object_mut(), evt.data.as_object())
                    {
                        for (k, v) in event_map {
                            state_map.insert(k.clone(), v.clone());
                        }
                    }
                }

                let snapshot = Snapshot {
                    aggregate_id: aggregate_id.to_string(),
                    version: new_version,
                    state,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                };

                drop(store); // Release events lock before acquiring snapshots lock.
                self.snapshots.write().insert(aggregate_id.to_string(), snapshot);
                self.metrics.record_snapshot_created();
            }
        }

        Ok(())
    }

    fn get_events(
        &self,
        aggregate_id: &str,
        from_version: Option<u64>,
    ) -> Result<Vec<Event>, EventSourcingError> {
        let store = self.events.read();
        match store.get(aggregate_id) {
            Some(events) => {
                let from = from_version.unwrap_or(0);
                let filtered: Vec<Event> = events
                    .iter()
                    .filter(|e| e.version > from)
                    .cloned()
                    .collect();
                Ok(filtered)
            }
            None => Err(EventSourcingError::AggregateNotFound(
                aggregate_id.to_string(),
            )),
        }
    }

    fn get_snapshot(
        &self,
        aggregate_id: &str,
    ) -> Result<Option<Snapshot>, EventSourcingError> {
        let snapshots = self.snapshots.read();
        Ok(snapshots.get(aggregate_id).cloned())
    }

    fn save_snapshot(
        &self,
        snapshot: &Snapshot,
    ) -> Result<(), EventSourcingError> {
        self.snapshots
            .write()
            .insert(snapshot.aggregate_id.clone(), snapshot.clone());
        self.metrics.record_snapshot_created();
        Ok(())
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Event sourcing error types.
#[derive(Debug, Clone)]
pub enum EventSourcingError {
    /// General storage error.
    StoreError(String),
    /// Optimistic concurrency conflict detected.
    ConcurrencyConflict {
        aggregate_id: String,
        expected: u64,
        actual: u64,
    },
    /// The requested event was not found.
    EventNotFound(String),
    /// The requested aggregate was not found.
    AggregateNotFound(String),
    /// Failed to serialize or deserialize event data.
    SerializationError(String),
    /// Error during snapshot creation or retrieval.
    SnapshotError(String),
}

impl std::fmt::Display for EventSourcingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventSourcingError::StoreError(msg) => write!(f, "Event store error: {}", msg),
            EventSourcingError::ConcurrencyConflict {
                aggregate_id,
                expected,
                actual,
            } => write!(
                f,
                "Concurrency conflict on aggregate '{}': expected version {}, actual {}",
                aggregate_id, expected, actual
            ),
            EventSourcingError::EventNotFound(id) => write!(f, "Event not found: {}", id),
            EventSourcingError::AggregateNotFound(id) => {
                write!(f, "Aggregate not found: {}", id)
            }
            EventSourcingError::SerializationError(msg) => {
                write!(f, "Event serialization error: {}", msg)
            }
            EventSourcingError::SnapshotError(msg) => write!(f, "Snapshot error: {}", msg),
        }
    }
}

impl std::error::Error for EventSourcingError {}

// ============================================================================
// Statistics & Metrics
// ============================================================================

/// Aggregate event sourcing statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventSourcingStats {
    /// Total number of events stored across all aggregates.
    pub total_events: u64,
    /// Total number of tracked aggregates.
    pub total_aggregates: u64,
    /// Total number of stored snapshots.
    pub total_snapshots: u64,
    /// Average number of events per aggregate.
    pub avg_events_per_aggregate: f64,
}

/// Internal atomic metrics tracker for event sourcing operations.
struct EventSourcingMetrics {
    events_appended: AtomicU64,
    snapshots_created: AtomicU64,
}

impl Default for EventSourcingMetrics {
    fn default() -> Self {
        Self {
            events_appended: AtomicU64::new(0),
            snapshots_created: AtomicU64::new(0),
        }
    }
}

impl EventSourcingMetrics {
    pub fn record_event_appended(&self) {
        self.events_appended.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_snapshot_created(&self) {
        self.snapshots_created.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_stats(
        &self,
        events: &HashMap<String, Vec<Event>>,
        snapshots: &HashMap<String, Snapshot>,
    ) -> EventSourcingStats {
        let total_events: u64 = events.values().map(|v| v.len() as u64).sum();
        let total_aggregates = events.len() as u64;
        let total_snapshots = snapshots.len() as u64;

        EventSourcingStats {
            total_events,
            total_aggregates,
            total_snapshots,
            avg_events_per_aggregate: if total_aggregates > 0 {
                total_events as f64 / total_aggregates as f64
            } else {
                0.0
            },
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- EventSourcingConfig Tests ----------

    #[test]
    fn test_config_default() {
        let config = EventSourcingConfig::default();
        assert_eq!(config.store_type, "memory");
        assert_eq!(config.snapshot_interval, 100);
        assert!(config.enable_snapshots);
        assert_eq!(config.max_events_per_aggregate, 10000);
        assert_eq!(config.event_ttl_secs, 0);
        assert!(config.connection_url.is_none());
    }

    #[test]
    fn test_config_memory() {
        let config = EventSourcingConfig::memory();
        assert_eq!(config.store_type, "memory");
        assert!(config.connection_url.is_none());
    }

    #[test]
    fn test_config_postgresql() {
        let config = EventSourcingConfig::postgresql("postgres://localhost/events");
        assert_eq!(config.store_type, "postgresql");
        assert_eq!(
            config.connection_url,
            Some("postgres://localhost/events".to_string())
        );
    }

    #[test]
    fn test_config_builder() {
        let config = EventSourcingConfig::memory()
            .with_snapshot_interval(50)
            .with_snapshots(false)
            .with_max_events(5000)
            .with_event_ttl(3600);

        assert_eq!(config.snapshot_interval, 50);
        assert!(!config.enable_snapshots);
        assert_eq!(config.max_events_per_aggregate, 5000);
        assert_eq!(config.event_ttl_secs, 3600);
    }

    // ---------- Event Tests ----------

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            "order-123",
            "OrderCreated",
            serde_json::json!({"item": "widget", "qty": 5}),
            1,
        );

        assert_eq!(event.aggregate_id(), "order-123");
        assert_eq!(event.event_type(), "OrderCreated");
        assert_eq!(event.version(), 1);
        assert!(event.timestamp > 0);
        assert_eq!(event.id, "evt-order-123-1");
    }

    #[test]
    fn test_event_metadata() {
        let event = Event::new("agg-1", "TestEvent", serde_json::json!({}), 1)
            .with_metadata("user_id", "user-42")
            .with_metadata("correlation_id", "corr-abc");

        assert!(event.has_metadata("user_id"));
        assert_eq!(event.get_metadata("user_id"), Some("user-42"));
        assert_eq!(event.get_metadata("correlation_id"), Some("corr-abc"));
        assert!(!event.has_metadata("missing_key"));
        assert_eq!(event.get_metadata("missing_key"), None);
    }

    // ---------- AggregateState Tests ----------

    #[test]
    fn test_aggregate_creation() {
        let aggregate = AggregateState::new("order-1", "Order");
        assert_eq!(aggregate.id, "order-1");
        assert_eq!(aggregate.aggregate_type, "Order");
        assert_eq!(aggregate.get_version(), 0);
        assert!(!aggregate.has_uncommitted_events());
    }

    #[test]
    fn test_aggregate_apply_event() {
        let mut aggregate = AggregateState::new("order-1", "Order");

        let event = Event::new(
            "order-1",
            "OrderCreated",
            serde_json::json!({"status": "created", "total": 100}),
            1,
        );

        aggregate.apply_event(event);

        assert_eq!(aggregate.get_version(), 1);
        assert!(aggregate.has_uncommitted_events());
        assert_eq!(aggregate.uncommitted_count(), 1);
        assert_eq!(aggregate.state["status"], "created");
        assert_eq!(aggregate.state["total"], 100);
    }

    #[test]
    fn test_aggregate_clear_uncommitted() {
        let mut aggregate = AggregateState::new("agg-1", "Test");

        let event = Event::new("agg-1", "TestEvent", serde_json::json!({"x": 1}), 1);
        aggregate.apply_event(event);
        assert_eq!(aggregate.uncommitted_count(), 1);

        aggregate.clear_uncommitted_events();
        assert!(!aggregate.has_uncommitted_events());
        assert_eq!(aggregate.uncommitted_count(), 0);
    }

    #[test]
    fn test_aggregate_from_snapshot() {
        let snapshot = Snapshot {
            aggregate_id: "agg-1".to_string(),
            version: 50,
            state: serde_json::json!({"balance": 500}),
            timestamp: 1700000000,
        };

        let aggregate = AggregateState::from_snapshot(&snapshot);
        assert_eq!(aggregate.id, "agg-1");
        assert_eq!(aggregate.get_version(), 50);
        assert_eq!(aggregate.state["balance"], 500);
    }

    // ---------- Snapshot Tests ----------

    #[test]
    fn test_snapshot_from_aggregate() {
        let mut aggregate = AggregateState::new("agg-1", "Account");
        let event = Event::new(
            "agg-1",
            "BalanceUpdated",
            serde_json::json!({"balance": 1000}),
            1,
        );
        aggregate.apply_event(event);

        let snapshot = Snapshot::from_aggregate(&aggregate);
        assert_eq!(snapshot.aggregate_id, "agg-1");
        assert_eq!(snapshot.version, 1);
        assert_eq!(snapshot.state["balance"], 1000);
        assert!(snapshot.timestamp > 0);
    }

    // ---------- InMemoryEventStore Tests ----------

    #[test]
    fn test_store_append_and_get_events() {
        let store = InMemoryEventStore::new();

        let events = vec![
            Event::new("order-1", "OrderCreated", serde_json::json!({"item": "A"}), 1),
            Event::new("order-1", "ItemAdded", serde_json::json!({"item": "B"}), 2),
        ];

        store.append_events("order-1", &events, 0).unwrap();

        let retrieved = store.get_events("order-1", None).unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved[0].event_type, "OrderCreated");
        assert_eq!(retrieved[1].event_type, "ItemAdded");
    }

    #[test]
    fn test_store_get_events_from_version() {
        let store = InMemoryEventStore::new();

        let events = vec![
            Event::new("agg-1", "E1", serde_json::json!({}), 1),
            Event::new("agg-1", "E2", serde_json::json!({}), 2),
            Event::new("agg-1", "E3", serde_json::json!({}), 3),
        ];

        store.append_events("agg-1", &events, 0).unwrap();

        let from_v2 = store.get_events("agg-1", Some(1)).unwrap();
        assert_eq!(from_v2.len(), 2);
        assert_eq!(from_v2[0].version, 2);
        assert_eq!(from_v2[1].version, 3);
    }

    #[test]
    fn test_store_concurrency_conflict() {
        let store = InMemoryEventStore::new();

        let events = vec![
            Event::new("agg-1", "E1", serde_json::json!({}), 1),
        ];
        store.append_events("agg-1", &events, 0).unwrap();

        // Try appending with wrong expected version.
        let more_events = vec![
            Event::new("agg-1", "E2", serde_json::json!({}), 2),
        ];
        let result = store.append_events("agg-1", &more_events, 0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EventSourcingError::ConcurrencyConflict { .. }
        ));
    }

    #[test]
    fn test_store_aggregate_not_found() {
        let store = InMemoryEventStore::new();
        let result = store.get_events("nonexistent", None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EventSourcingError::AggregateNotFound(_)
        ));
    }

    #[test]
    fn test_store_save_and_get_snapshot() {
        let store = InMemoryEventStore::new();

        let snapshot = Snapshot {
            aggregate_id: "order-1".to_string(),
            version: 10,
            state: serde_json::json!({"total": 250}),
            timestamp: 1700000000,
        };

        store.save_snapshot(&snapshot).unwrap();

        let retrieved = store.get_snapshot("order-1").unwrap();
        assert!(retrieved.is_some());
        let snap = retrieved.unwrap();
        assert_eq!(snap.version, 10);
        assert_eq!(snap.state["total"], 250);
    }

    #[test]
    fn test_store_snapshot_not_found() {
        let store = InMemoryEventStore::new();
        let result = store.get_snapshot("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_store_stats() {
        let store = InMemoryEventStore::new();

        let events1 = vec![
            Event::new("agg-1", "E1", serde_json::json!({}), 1),
            Event::new("agg-1", "E2", serde_json::json!({}), 2),
        ];
        let events2 = vec![
            Event::new("agg-2", "E1", serde_json::json!({}), 1),
        ];

        store.append_events("agg-1", &events1, 0).unwrap();
        store.append_events("agg-2", &events2, 0).unwrap();

        let stats = store.stats();
        assert_eq!(stats.total_events, 3);
        assert_eq!(stats.total_aggregates, 2);
        assert_eq!(stats.avg_events_per_aggregate, 1.5);
    }

    #[test]
    fn test_store_clear() {
        let store = InMemoryEventStore::new();

        let events = vec![
            Event::new("agg-1", "E1", serde_json::json!({}), 1),
        ];
        store.append_events("agg-1", &events, 0).unwrap();
        assert_eq!(store.total_events(), 1);

        store.clear();
        assert_eq!(store.total_events(), 0);
        assert_eq!(store.total_aggregates(), 0);
    }

    // ---------- Error Display Tests ----------

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", EventSourcingError::StoreError("disk full".to_string())),
            "Event store error: disk full"
        );
        assert_eq!(
            format!(
                "{}",
                EventSourcingError::ConcurrencyConflict {
                    aggregate_id: "agg-1".to_string(),
                    expected: 5,
                    actual: 7,
                }
            ),
            "Concurrency conflict on aggregate 'agg-1': expected version 5, actual 7"
        );
        assert_eq!(
            format!("{}", EventSourcingError::EventNotFound("evt-1".to_string())),
            "Event not found: evt-1"
        );
        assert_eq!(
            format!("{}", EventSourcingError::AggregateNotFound("agg-1".to_string())),
            "Aggregate not found: agg-1"
        );
        assert_eq!(
            format!(
                "{}",
                EventSourcingError::SerializationError("invalid json".to_string())
            ),
            "Event serialization error: invalid json"
        );
        assert_eq!(
            format!(
                "{}",
                EventSourcingError::SnapshotError("corrupt data".to_string())
            ),
            "Snapshot error: corrupt data"
        );
    }

    // ---------- EventSourcingStats Tests ----------

    #[test]
    fn test_stats_default() {
        let stats = EventSourcingStats::default();
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.total_aggregates, 0);
        assert_eq!(stats.total_snapshots, 0);
        assert_eq!(stats.avg_events_per_aggregate, 0.0);
    }
}
