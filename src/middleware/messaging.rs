//! Message Queue Adapter Support.
//!
//! Provides unified message queue adapters for various brokers with:
//! - Kafka, RabbitMQ, and SQS configuration
//! - Producer and consumer traits
//! - In-memory mock implementations for testing
//! - Messaging statistics and monitoring
//!
//! # Example
//! ```python
//! from cello import App
//! from cello.messaging import MessageQueueConfig, KafkaConfig
//!
//! config = MessageQueueConfig.kafka(
//!     brokers=["localhost:9092"],
//!     group_id="my-consumer-group",
//! )
//!
//! @app.on_startup
//! async def setup():
//!     app.state.producer = await MessageProducer.connect(config)
//!
//! @app.post("/events")
//! async def publish_event(request):
//!     data = request.json()
//!     await app.state.producer.send("events", key=data["id"], value=data)
//!     return {"published": True}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

// ============================================================================
// Configuration Types
// ============================================================================

/// Top-level message queue configuration enum.
///
/// Supports multiple message broker backends through a unified configuration
/// interface. Each variant wraps a backend-specific configuration struct.
#[derive(Clone, Debug)]
pub enum MessageQueueConfig {
    /// Apache Kafka configuration.
    Kafka(KafkaConfig),
    /// RabbitMQ (AMQP) configuration.
    RabbitMQ(RabbitMQConfig),
    /// Amazon SQS configuration.
    Sqs(SqsConfig),
}

/// Kafka broker configuration.
#[derive(Clone, Debug)]
pub struct KafkaConfig {
    /// List of broker addresses (e.g., ["localhost:9092"]).
    pub brokers: Vec<String>,
    /// Consumer group identifier.
    pub group_id: Option<String>,
    /// Client identifier for broker connections.
    pub client_id: Option<String>,
    /// Enable automatic offset commits.
    pub auto_commit: bool,
    /// Session timeout in milliseconds for consumer group membership.
    pub session_timeout_ms: u64,
    /// Maximum number of records returned per poll.
    pub max_poll_records: usize,
    /// Security protocol (e.g., "PLAINTEXT", "SSL", "SASL_PLAINTEXT", "SASL_SSL").
    pub security_protocol: String,
}

impl Default for KafkaConfig {
    fn default() -> Self {
        Self {
            brokers: vec!["localhost:9092".to_string()],
            group_id: None,
            client_id: None,
            auto_commit: true,
            session_timeout_ms: 10000,
            max_poll_records: 500,
            security_protocol: "PLAINTEXT".to_string(),
        }
    }
}

impl KafkaConfig {
    /// Create a KafkaConfig preset for a local development broker at localhost:9092.
    pub fn local() -> Self {
        Self {
            brokers: vec!["localhost:9092".to_string()],
            group_id: Some("cello-local-group".to_string()),
            client_id: Some("cello-local-client".to_string()),
            auto_commit: true,
            session_timeout_ms: 10000,
            max_poll_records: 500,
            security_protocol: "PLAINTEXT".to_string(),
        }
    }

    /// Set the broker addresses.
    pub fn with_brokers(mut self, brokers: Vec<String>) -> Self {
        self.brokers = brokers;
        self
    }

    /// Set the consumer group ID.
    pub fn with_group_id(mut self, group_id: &str) -> Self {
        self.group_id = Some(group_id.to_string());
        self
    }

    /// Set the client ID.
    pub fn with_client_id(mut self, client_id: &str) -> Self {
        self.client_id = Some(client_id.to_string());
        self
    }
}

/// RabbitMQ (AMQP) broker configuration.
#[derive(Clone, Debug)]
pub struct RabbitMQConfig {
    /// AMQP connection URL (e.g., "amqp://guest:guest@localhost:5672").
    pub url: String,
    /// Virtual host to connect to.
    pub vhost: String,
    /// Prefetch count for consumer QoS.
    pub prefetch_count: u16,
    /// Heartbeat interval in seconds.
    pub heartbeat_secs: u16,
    /// Connection timeout in seconds.
    pub connection_timeout_secs: u16,
}

impl Default for RabbitMQConfig {
    fn default() -> Self {
        Self {
            url: "amqp://localhost".to_string(),
            vhost: "/".to_string(),
            prefetch_count: 10,
            heartbeat_secs: 60,
            connection_timeout_secs: 30,
        }
    }
}

impl RabbitMQConfig {
    /// Create a RabbitMQConfig preset for a local development broker at localhost.
    pub fn local() -> Self {
        Self {
            url: "amqp://guest:guest@localhost:5672".to_string(),
            vhost: "/".to_string(),
            prefetch_count: 10,
            heartbeat_secs: 60,
            connection_timeout_secs: 30,
        }
    }
}

/// Amazon SQS queue configuration.
#[derive(Clone, Debug)]
pub struct SqsConfig {
    /// AWS region (e.g., "us-east-1").
    pub region: String,
    /// Custom endpoint URL (e.g., for LocalStack).
    pub endpoint_url: Option<String>,
    /// SQS queue URL.
    pub queue_url: String,
    /// Maximum number of messages to receive per request (1-10).
    pub max_messages: i32,
    /// Long-poll wait time in seconds (0-20).
    pub wait_time_secs: i32,
    /// Visibility timeout in seconds for received messages.
    pub visibility_timeout_secs: i32,
}

impl Default for SqsConfig {
    fn default() -> Self {
        Self {
            region: "us-east-1".to_string(),
            endpoint_url: None,
            queue_url: String::new(),
            max_messages: 10,
            wait_time_secs: 20,
            visibility_timeout_secs: 30,
        }
    }
}

impl SqsConfig {
    /// Create an SqsConfig preset for LocalStack local development.
    pub fn local(queue_url: &str) -> Self {
        Self {
            region: "us-east-1".to_string(),
            endpoint_url: Some("http://localhost:4566".to_string()),
            queue_url: queue_url.to_string(),
            max_messages: 10,
            wait_time_secs: 5,
            visibility_timeout_secs: 30,
        }
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// A message received from or sent to a message queue.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier.
    pub id: String,
    /// Topic or queue name the message belongs to.
    pub topic: String,
    /// Optional message key for partitioning.
    pub key: Option<String>,
    /// Raw message payload bytes.
    pub value: Vec<u8>,
    /// Message headers/metadata.
    pub headers: HashMap<String, String>,
    /// Message timestamp (Unix epoch milliseconds).
    pub timestamp: u64,
    /// Kafka partition number (if applicable).
    pub partition: Option<i32>,
    /// Kafka offset within the partition (if applicable).
    pub offset: Option<i64>,
}

impl Message {
    /// Attempt to interpret the message value as a UTF-8 string.
    pub fn value_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.value).ok()
    }

    /// Attempt to parse the message value as a JSON value.
    pub fn value_json(&self) -> Option<JsonValue> {
        serde_json::from_slice(&self.value).ok()
    }
}

/// Result of processing a message, indicating desired acknowledgement behaviour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageResult {
    /// Acknowledge successful processing; remove from queue.
    Ack,
    /// Negative acknowledgement; message may be redelivered.
    Nack,
    /// Reject the message; discard without redelivery.
    Reject,
    /// Requeue the message for later processing.
    Requeue,
    /// Route the message to a dead-letter queue.
    DeadLetter,
}

// ============================================================================
// Producer Configuration & Trait
// ============================================================================

/// Configuration for a message producer.
#[derive(Clone, Debug)]
pub struct ProducerConfig {
    /// Default topic to produce messages to.
    pub topic: String,
    /// Serializer name for message keys.
    pub key_serializer: String,
    /// Serializer name for message values.
    pub value_serializer: String,
    /// Acknowledgement mode (e.g., "all", "1", "0").
    pub acks: String,
    /// Number of send retries on transient failures.
    pub retries: u32,
    /// Maximum batch size in bytes before flushing.
    pub batch_size: usize,
    /// Time in milliseconds to wait for additional messages before flushing.
    pub linger_ms: u64,
}

impl Default for ProducerConfig {
    fn default() -> Self {
        Self {
            topic: String::new(),
            key_serializer: "string".to_string(),
            value_serializer: "bytes".to_string(),
            acks: "all".to_string(),
            retries: 3,
            batch_size: 16384,
            linger_ms: 5,
        }
    }
}

/// Trait for message producers capable of sending messages to a broker.
pub trait MessageProducer: Send + Sync {
    /// Send a single message to the specified topic.
    fn send(&self, topic: &str, key: Option<&str>, value: &[u8]) -> Result<(), MessagingError>;

    /// Send a batch of messages. Each tuple contains (topic, optional key, value).
    fn send_batch(
        &self,
        messages: Vec<(String, Option<String>, Vec<u8>)>,
    ) -> Result<(), MessagingError>;
}

/// Trait for message consumers capable of receiving messages from a broker.
pub trait MessageConsumer: Send + Sync {
    /// Subscribe to the given list of topics.
    fn subscribe(&self, topics: &[&str]) -> Result<(), MessagingError>;

    /// Poll the broker for new messages.
    fn poll(&self) -> Result<Vec<Message>, MessagingError>;

    /// Commit (acknowledge) a successfully processed message.
    fn commit(&self, message: &Message) -> Result<(), MessagingError>;
}

// ============================================================================
// Mock Implementations
// ============================================================================

/// In-memory mock producer for testing.
///
/// All sent messages are stored in an internal buffer and can be inspected
/// through the `sent_messages` method.
pub struct MockProducer {
    messages: Arc<RwLock<Vec<Message>>>,
    stats: Arc<MessagingMetrics>,
}

impl MockProducer {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(MessagingMetrics::default()),
        }
    }

    /// Return a snapshot of all messages sent through this producer.
    pub fn sent_messages(&self) -> Vec<Message> {
        self.messages.read().clone()
    }

    /// Get messaging statistics.
    pub fn stats(&self) -> MessagingStats {
        self.stats.get_stats()
    }

    /// Clear all sent messages.
    pub fn clear(&self) {
        self.messages.write().clear();
    }
}

impl Default for MockProducer {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageProducer for MockProducer {
    fn send(&self, topic: &str, key: Option<&str>, value: &[u8]) -> Result<(), MessagingError> {
        let message = Message {
            id: format!("msg-{}", self.messages.read().len()),
            topic: topic.to_string(),
            key: key.map(|k| k.to_string()),
            value: value.to_vec(),
            headers: HashMap::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            partition: None,
            offset: None,
        };
        self.messages.write().push(message);
        self.stats.record_sent();
        Ok(())
    }

    fn send_batch(
        &self,
        messages: Vec<(String, Option<String>, Vec<u8>)>,
    ) -> Result<(), MessagingError> {
        let mut store = self.messages.write();
        let base_idx = store.len();
        for (i, (topic, key, value)) in messages.into_iter().enumerate() {
            let message = Message {
                id: format!("msg-{}", base_idx + i),
                topic,
                key,
                value,
                headers: HashMap::new(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                partition: None,
                offset: None,
            };
            store.push(message);
            self.stats.record_sent();
        }
        Ok(())
    }
}

/// In-memory mock consumer for testing.
///
/// Messages can be enqueued via `enqueue` and will be returned by `poll`.
/// Committed message IDs are tracked for verification.
pub struct MockConsumer {
    subscriptions: Arc<RwLock<Vec<String>>>,
    pending: Arc<RwLock<Vec<Message>>>,
    committed: Arc<RwLock<Vec<String>>>,
    stats: Arc<MessagingMetrics>,
}

impl MockConsumer {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(Vec::new())),
            pending: Arc::new(RwLock::new(Vec::new())),
            committed: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(MessagingMetrics::default()),
        }
    }

    /// Add a message to the pending queue (simulates broker delivery).
    pub fn enqueue(&self, message: Message) {
        self.pending.write().push(message);
    }

    /// Return a list of subscribed topics.
    pub fn subscriptions(&self) -> Vec<String> {
        self.subscriptions.read().clone()
    }

    /// Return a list of committed message IDs.
    pub fn committed_ids(&self) -> Vec<String> {
        self.committed.read().clone()
    }

    /// Get messaging statistics.
    pub fn stats(&self) -> MessagingStats {
        self.stats.get_stats()
    }
}

impl Default for MockConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageConsumer for MockConsumer {
    fn subscribe(&self, topics: &[&str]) -> Result<(), MessagingError> {
        let mut subs = self.subscriptions.write();
        for topic in topics {
            if !subs.contains(&topic.to_string()) {
                subs.push(topic.to_string());
            }
        }
        self.stats.set_active_consumers(subs.len());
        Ok(())
    }

    fn poll(&self) -> Result<Vec<Message>, MessagingError> {
        let mut pending = self.pending.write();
        let subscriptions = self.subscriptions.read();

        // Only return messages for subscribed topics.
        let (matching, remaining): (Vec<_>, Vec<_>) = pending
            .drain(..)
            .partition(|msg| subscriptions.contains(&msg.topic));

        *pending = remaining;

        for _ in &matching {
            self.stats.record_received();
        }

        Ok(matching)
    }

    fn commit(&self, message: &Message) -> Result<(), MessagingError> {
        self.committed.write().push(message.id.clone());
        Ok(())
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Messaging error types.
#[derive(Debug, Clone)]
pub enum MessagingError {
    /// Failed to connect to the message broker.
    Connection,
    /// Failed to serialize or deserialize a message.
    Serialization,
    /// Operation timed out waiting for broker response.
    Timeout,
    /// The specified queue or topic was not found.
    QueueNotFound,
    /// The message broker is unavailable.
    BrokerUnavailable,
    /// Authentication or authorization failure.
    Authentication,
    /// An unclassified error with a description.
    Unknown(String),
}

impl std::fmt::Display for MessagingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessagingError::Connection => write!(f, "Messaging connection error"),
            MessagingError::Serialization => write!(f, "Messaging serialization error"),
            MessagingError::Timeout => write!(f, "Messaging operation timed out"),
            MessagingError::QueueNotFound => write!(f, "Queue or topic not found"),
            MessagingError::BrokerUnavailable => write!(f, "Message broker unavailable"),
            MessagingError::Authentication => write!(f, "Messaging authentication error"),
            MessagingError::Unknown(msg) => write!(f, "Messaging error: {msg}"),
        }
    }
}

impl std::error::Error for MessagingError {}

// ============================================================================
// Statistics & Metrics
// ============================================================================

/// Aggregate messaging statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagingStats {
    /// Total number of messages successfully sent.
    pub messages_sent: u64,
    /// Total number of messages successfully received.
    pub messages_received: u64,
    /// Total number of message operations that failed.
    pub messages_failed: u64,
    /// Average message processing latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Number of currently active consumer subscriptions.
    pub active_consumers: usize,
}

/// Internal atomic metrics tracker for messaging operations.
struct MessagingMetrics {
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    messages_failed: AtomicU64,
    latency_sum_ms: AtomicU64,
    total_operations: AtomicU64,
    active_consumers: AtomicU64,
}

impl Default for MessagingMetrics {
    fn default() -> Self {
        Self {
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            messages_failed: AtomicU64::new(0),
            latency_sum_ms: AtomicU64::new(0),
            total_operations: AtomicU64::new(0),
            active_consumers: AtomicU64::new(0),
        }
    }
}

impl MessagingMetrics {
    pub fn record_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.total_operations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.total_operations.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn record_failed(&self) {
        self.messages_failed.fetch_add(1, Ordering::Relaxed);
        self.total_operations.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn record_latency(&self, latency_ms: u64) {
        self.latency_sum_ms.fetch_add(latency_ms, Ordering::Relaxed);
    }

    pub fn set_active_consumers(&self, count: usize) {
        self.active_consumers.store(count as u64, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> MessagingStats {
        let total_ops = self.total_operations.load(Ordering::Relaxed);
        let latency_sum = self.latency_sum_ms.load(Ordering::Relaxed);

        MessagingStats {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            messages_failed: self.messages_failed.load(Ordering::Relaxed),
            avg_latency_ms: if total_ops > 0 {
                latency_sum as f64 / total_ops as f64
            } else {
                0.0
            },
            active_consumers: self.active_consumers.load(Ordering::Relaxed) as usize,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- KafkaConfig Tests ----------

    #[test]
    fn test_kafka_config_default() {
        let config = KafkaConfig::default();
        assert_eq!(config.brokers, vec!["localhost:9092".to_string()]);
        assert!(config.group_id.is_none());
        assert!(config.client_id.is_none());
        assert!(config.auto_commit);
        assert_eq!(config.session_timeout_ms, 10000);
        assert_eq!(config.max_poll_records, 500);
        assert_eq!(config.security_protocol, "PLAINTEXT");
    }

    #[test]
    fn test_kafka_config_local() {
        let config = KafkaConfig::local();
        assert_eq!(config.brokers, vec!["localhost:9092".to_string()]);
        assert_eq!(config.group_id, Some("cello-local-group".to_string()));
        assert_eq!(config.client_id, Some("cello-local-client".to_string()));
        assert!(config.auto_commit);
    }

    #[test]
    fn test_kafka_config_builder() {
        let config = KafkaConfig::default()
            .with_brokers(vec!["broker1:9092".to_string(), "broker2:9092".to_string()])
            .with_group_id("my-group")
            .with_client_id("my-client");

        assert_eq!(config.brokers.len(), 2);
        assert_eq!(config.group_id, Some("my-group".to_string()));
        assert_eq!(config.client_id, Some("my-client".to_string()));
    }

    // ---------- RabbitMQConfig Tests ----------

    #[test]
    fn test_rabbitmq_config_default() {
        let config = RabbitMQConfig::default();
        assert_eq!(config.url, "amqp://localhost");
        assert_eq!(config.vhost, "/");
        assert_eq!(config.prefetch_count, 10);
        assert_eq!(config.heartbeat_secs, 60);
        assert_eq!(config.connection_timeout_secs, 30);
    }

    #[test]
    fn test_rabbitmq_config_local() {
        let config = RabbitMQConfig::local();
        assert_eq!(config.url, "amqp://guest:guest@localhost:5672");
        assert_eq!(config.vhost, "/");
        assert_eq!(config.prefetch_count, 10);
    }

    // ---------- SqsConfig Tests ----------

    #[test]
    fn test_sqs_config_default() {
        let config = SqsConfig::default();
        assert_eq!(config.region, "us-east-1");
        assert!(config.endpoint_url.is_none());
        assert_eq!(config.queue_url, "");
        assert_eq!(config.max_messages, 10);
        assert_eq!(config.wait_time_secs, 20);
        assert_eq!(config.visibility_timeout_secs, 30);
    }

    #[test]
    fn test_sqs_config_local() {
        let config = SqsConfig::local("http://localhost:4566/000000000000/my-queue");
        assert_eq!(config.region, "us-east-1");
        assert_eq!(
            config.endpoint_url,
            Some("http://localhost:4566".to_string())
        );
        assert_eq!(
            config.queue_url,
            "http://localhost:4566/000000000000/my-queue"
        );
        assert_eq!(config.wait_time_secs, 5);
    }

    // ---------- MessageQueueConfig Tests ----------

    #[test]
    fn test_message_queue_config_variants() {
        let kafka = MessageQueueConfig::Kafka(KafkaConfig::local());
        assert!(matches!(kafka, MessageQueueConfig::Kafka(_)));

        let rabbit = MessageQueueConfig::RabbitMQ(RabbitMQConfig::local());
        assert!(matches!(rabbit, MessageQueueConfig::RabbitMQ(_)));

        let sqs = MessageQueueConfig::Sqs(SqsConfig::local("http://localhost:4566/q"));
        assert!(matches!(sqs, MessageQueueConfig::Sqs(_)));
    }

    // ---------- Message Tests ----------

    #[test]
    fn test_message_value_str() {
        let msg = Message {
            id: "msg-1".to_string(),
            topic: "test-topic".to_string(),
            key: Some("key-1".to_string()),
            value: b"hello world".to_vec(),
            headers: HashMap::new(),
            timestamp: 1700000000000,
            partition: Some(0),
            offset: Some(42),
        };

        assert_eq!(msg.value_str(), Some("hello world"));
    }

    #[test]
    fn test_message_value_json() {
        let json_bytes = br#"{"name": "cello", "version": 7}"#;
        let msg = Message {
            id: "msg-2".to_string(),
            topic: "json-topic".to_string(),
            key: None,
            value: json_bytes.to_vec(),
            headers: HashMap::new(),
            timestamp: 1700000000000,
            partition: None,
            offset: None,
        };

        let json = msg.value_json().expect("should parse JSON");
        assert_eq!(json["name"], "cello");
        assert_eq!(json["version"], 7);
    }

    #[test]
    fn test_message_value_invalid_utf8() {
        let msg = Message {
            id: "msg-3".to_string(),
            topic: "binary-topic".to_string(),
            key: None,
            value: vec![0xFF, 0xFE, 0xFD],
            headers: HashMap::new(),
            timestamp: 0,
            partition: None,
            offset: None,
        };

        assert!(msg.value_str().is_none());
        assert!(msg.value_json().is_none());
    }

    // ---------- MessageResult Tests ----------

    #[test]
    fn test_message_result_variants() {
        assert_eq!(MessageResult::Ack, MessageResult::Ack);
        assert_ne!(MessageResult::Ack, MessageResult::Nack);
        assert_ne!(MessageResult::Reject, MessageResult::Requeue);
        assert_eq!(MessageResult::DeadLetter, MessageResult::DeadLetter);
    }

    // ---------- ProducerConfig Tests ----------

    #[test]
    fn test_producer_config_default() {
        let config = ProducerConfig::default();
        assert_eq!(config.topic, "");
        assert_eq!(config.key_serializer, "string");
        assert_eq!(config.value_serializer, "bytes");
        assert_eq!(config.acks, "all");
        assert_eq!(config.retries, 3);
        assert_eq!(config.batch_size, 16384);
        assert_eq!(config.linger_ms, 5);
    }

    // ---------- MockProducer Tests ----------

    #[test]
    fn test_mock_producer_send() {
        let producer = MockProducer::new();

        producer
            .send("events", Some("key-1"), b"payload-1")
            .unwrap();
        producer.send("events", None, b"payload-2").unwrap();

        let messages = producer.sent_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].topic, "events");
        assert_eq!(messages[0].key, Some("key-1".to_string()));
        assert_eq!(messages[0].value, b"payload-1");
        assert_eq!(messages[1].key, None);
        assert_eq!(messages[1].value, b"payload-2");
    }

    #[test]
    fn test_mock_producer_send_batch() {
        let producer = MockProducer::new();

        let batch = vec![
            (
                "topic-a".to_string(),
                Some("k1".to_string()),
                b"v1".to_vec(),
            ),
            ("topic-b".to_string(), None, b"v2".to_vec()),
            (
                "topic-a".to_string(),
                Some("k3".to_string()),
                b"v3".to_vec(),
            ),
        ];

        producer.send_batch(batch).unwrap();

        let messages = producer.sent_messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].topic, "topic-a");
        assert_eq!(messages[1].topic, "topic-b");
        assert_eq!(messages[2].key, Some("k3".to_string()));
    }

    #[test]
    fn test_mock_producer_clear() {
        let producer = MockProducer::new();

        producer.send("topic", None, b"data").unwrap();
        assert_eq!(producer.sent_messages().len(), 1);

        producer.clear();
        assert_eq!(producer.sent_messages().len(), 0);
    }

    #[test]
    fn test_mock_producer_stats() {
        let producer = MockProducer::new();

        producer.send("t1", None, b"v1").unwrap();
        producer.send("t2", None, b"v2").unwrap();
        producer.send("t3", None, b"v3").unwrap();

        let stats = producer.stats();
        assert_eq!(stats.messages_sent, 3);
    }

    // ---------- MockConsumer Tests ----------

    #[test]
    fn test_mock_consumer_subscribe_and_poll() {
        let consumer = MockConsumer::new();

        consumer.subscribe(&["events", "logs"]).unwrap();

        let subs = consumer.subscriptions();
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&"events".to_string()));
        assert!(subs.contains(&"logs".to_string()));

        // Enqueue messages.
        consumer.enqueue(Message {
            id: "msg-1".to_string(),
            topic: "events".to_string(),
            key: None,
            value: b"event-data".to_vec(),
            headers: HashMap::new(),
            timestamp: 1700000000000,
            partition: None,
            offset: None,
        });
        consumer.enqueue(Message {
            id: "msg-2".to_string(),
            topic: "unsubscribed-topic".to_string(),
            key: None,
            value: b"other-data".to_vec(),
            headers: HashMap::new(),
            timestamp: 1700000000000,
            partition: None,
            offset: None,
        });

        let polled = consumer.poll().unwrap();
        assert_eq!(polled.len(), 1);
        assert_eq!(polled[0].id, "msg-1");
        assert_eq!(polled[0].topic, "events");
    }

    #[test]
    fn test_mock_consumer_commit() {
        let consumer = MockConsumer::new();

        let msg = Message {
            id: "msg-42".to_string(),
            topic: "events".to_string(),
            key: None,
            value: b"data".to_vec(),
            headers: HashMap::new(),
            timestamp: 0,
            partition: None,
            offset: None,
        };

        consumer.commit(&msg).unwrap();

        let committed = consumer.committed_ids();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0], "msg-42");
    }

    #[test]
    fn test_mock_consumer_stats() {
        let consumer = MockConsumer::new();

        consumer.subscribe(&["topic-a"]).unwrap();

        consumer.enqueue(Message {
            id: "msg-1".to_string(),
            topic: "topic-a".to_string(),
            key: None,
            value: b"data".to_vec(),
            headers: HashMap::new(),
            timestamp: 0,
            partition: None,
            offset: None,
        });
        consumer.enqueue(Message {
            id: "msg-2".to_string(),
            topic: "topic-a".to_string(),
            key: None,
            value: b"data".to_vec(),
            headers: HashMap::new(),
            timestamp: 0,
            partition: None,
            offset: None,
        });

        let _ = consumer.poll().unwrap();

        let stats = consumer.stats();
        assert_eq!(stats.messages_received, 2);
        assert_eq!(stats.active_consumers, 1);
    }

    #[test]
    fn test_mock_consumer_duplicate_subscribe() {
        let consumer = MockConsumer::new();

        consumer.subscribe(&["events"]).unwrap();
        consumer.subscribe(&["events", "logs"]).unwrap();

        let subs = consumer.subscriptions();
        assert_eq!(subs.len(), 2); // "events" should not be duplicated
    }

    #[test]
    fn test_mock_consumer_empty_poll() {
        let consumer = MockConsumer::new();
        consumer.subscribe(&["events"]).unwrap();

        let polled = consumer.poll().unwrap();
        assert!(polled.is_empty());
    }

    // ---------- MessagingError Tests ----------

    #[test]
    fn test_messaging_error_display() {
        assert_eq!(
            format!("{}", MessagingError::Connection),
            "Messaging connection error"
        );
        assert_eq!(
            format!("{}", MessagingError::Serialization),
            "Messaging serialization error"
        );
        assert_eq!(
            format!("{}", MessagingError::Timeout),
            "Messaging operation timed out"
        );
        assert_eq!(
            format!("{}", MessagingError::QueueNotFound),
            "Queue or topic not found"
        );
        assert_eq!(
            format!("{}", MessagingError::BrokerUnavailable),
            "Message broker unavailable"
        );
        assert_eq!(
            format!("{}", MessagingError::Authentication),
            "Messaging authentication error"
        );
        assert_eq!(
            format!("{}", MessagingError::Unknown("custom".to_string())),
            "Messaging error: custom"
        );
    }

    // ---------- MessagingStats Tests ----------

    #[test]
    fn test_messaging_stats_default() {
        let stats = MessagingStats::default();
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.messages_failed, 0);
        assert_eq!(stats.avg_latency_ms, 0.0);
        assert_eq!(stats.active_consumers, 0);
    }

    // ---------- Integration / Round-trip Tests ----------

    #[test]
    fn test_producer_consumer_roundtrip() {
        let producer = MockProducer::new();
        let consumer = MockConsumer::new();

        // Subscribe to the target topic.
        consumer.subscribe(&["orders"]).unwrap();

        // Produce messages.
        producer
            .send("orders", Some("order-1"), b"{\"item\": \"widget\"}")
            .unwrap();
        producer
            .send("orders", Some("order-2"), b"{\"item\": \"gadget\"}")
            .unwrap();

        // Transfer messages from producer to consumer (simulating broker).
        for msg in producer.sent_messages() {
            consumer.enqueue(msg);
        }

        // Consume and commit.
        let received = consumer.poll().unwrap();
        assert_eq!(received.len(), 2);

        for msg in &received {
            assert!(msg.value_json().is_some());
            consumer.commit(msg).unwrap();
        }

        let committed = consumer.committed_ids();
        assert_eq!(committed.len(), 2);

        // Verify stats.
        let producer_stats = producer.stats();
        assert_eq!(producer_stats.messages_sent, 2);

        let consumer_stats = consumer.stats();
        assert_eq!(consumer_stats.messages_received, 2);
        assert_eq!(consumer_stats.active_consumers, 1);
    }

    #[test]
    fn test_message_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("correlation-id".to_string(), "abc-123".to_string());
        headers.insert("content-type".to_string(), "application/json".to_string());

        let msg = Message {
            id: "msg-h1".to_string(),
            topic: "events".to_string(),
            key: Some("key".to_string()),
            value: b"{}".to_vec(),
            headers,
            timestamp: 1700000000000,
            partition: Some(3),
            offset: Some(100),
        };

        assert_eq!(
            msg.headers.get("correlation-id"),
            Some(&"abc-123".to_string())
        );
        assert_eq!(msg.partition, Some(3));
        assert_eq!(msg.offset, Some(100));
    }
}
