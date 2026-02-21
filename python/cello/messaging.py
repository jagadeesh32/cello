"""
Cello Message Queue Adapter Module.

Provides Python-friendly wrappers for message queue operations
with Kafka, RabbitMQ, and AWS SQS support. Includes producer/consumer
patterns, decorator-based message handling, and configuration classes.

Example (Kafka):
    from cello import App
    from cello.messaging import KafkaConfig, Producer, Consumer, Message

    app = App()
    kafka_config = KafkaConfig(brokers=["localhost:9092"], group_id="my-group")

    @app.on_event("startup")
    async def setup():
        app.state.producer = await Producer.connect(kafka_config)
        app.state.consumer = await Consumer.connect(kafka_config)
        await app.state.consumer.subscribe(["orders", "events"])

    @app.post("/publish")
    async def publish(request):
        data = request.json()
        await app.state.producer.send("orders", value=data)
        return {"published": True}

    @app.on_event("shutdown")
    async def teardown():
        await app.state.producer.close()
        await app.state.consumer.close()

Example (RabbitMQ):
    from cello.messaging import RabbitMQConfig, Producer, Consumer

    rabbit_config = RabbitMQConfig(url="amqp://guest:guest@localhost", prefetch_count=20)

    producer = await Producer.connect(rabbit_config)
    await producer.send("tasks", value={"action": "process", "id": 42})

Example (SQS):
    from cello.messaging import SqsConfig, Producer, Consumer

    sqs_config = SqsConfig(region="us-west-2", queue_url="https://sqs.us-west-2.amazonaws.com/123/my-queue")

    producer = await Producer.connect(sqs_config)
    await producer.send("my-queue", value={"event": "order_created"})

Example (Decorators):
    from cello.messaging import kafka_consumer, kafka_producer

    @kafka_consumer(topic="orders", group="order-processor")
    async def process_order(message):
        order = message.json()
        print(f"Processing order {order['id']}")
        return MessageResult.ACK

    @kafka_producer(topic="events")
    async def create_event(request):
        return {"event": "user_signup", "user_id": 123}
        # Return value is automatically published to "events" topic
"""

import json
import time
import uuid
from functools import wraps
from typing import Any, Callable, Optional


def _is_async(func: Callable) -> bool:
    """Check if a function is async."""
    import inspect
    return inspect.iscoroutinefunction(func)


def kafka_consumer(topic: str, group: str = None, auto_commit: bool = True) -> Callable:
    """
    Decorator that wraps an async handler to consume messages from a Kafka topic.

    The decorated function receives a Message object and should return a
    MessageResult indicating how the message should be acknowledged.

    Args:
        topic: Kafka topic to consume from.
        group: Consumer group ID. If None, a unique group is generated.
        auto_commit: Whether to automatically commit offsets (default: True).

    Returns:
        Decorator function for the message handler.

    Example:
        @kafka_consumer(topic="orders", group="processor")
        async def handle_order(message):
            order = message.json()
            await process(order)
            return MessageResult.ACK
    """
    def decorator(func: Callable) -> Callable:
        @wraps(func)
        async def wrapper(*args, **kwargs):
            # In a real implementation, this wrapper is registered with the
            # Rust consumer loop which calls it for each incoming message.
            result = await func(*args, **kwargs) if _is_async(func) else func(*args, **kwargs)
            return result

        # Attach metadata for the Rust runtime to discover
        wrapper._cello_consumer = True
        wrapper._cello_consumer_topic = topic
        wrapper._cello_consumer_group = group
        wrapper._cello_consumer_auto_commit = auto_commit
        return wrapper
    return decorator


def kafka_producer(topic: str) -> Callable:
    """
    Decorator that wraps a function to auto-publish its return value to a Kafka topic.

    The return value of the decorated function is serialized as JSON and
    sent to the specified topic. If the return value is None, no message
    is published.

    Args:
        topic: Kafka topic to publish to.

    Returns:
        Decorator function for the producer handler.

    Example:
        @kafka_producer(topic="events")
        async def emit_event(request):
            return {"event": "user_signup", "user_id": 123}
            # Return value is automatically published to "events" topic
    """
    def decorator(func: Callable) -> Callable:
        @wraps(func)
        async def wrapper(*args, **kwargs):
            result = await func(*args, **kwargs) if _is_async(func) else func(*args, **kwargs)

            # Auto-publish return value if not None
            if result is not None:
                # The Rust runtime intercepts this metadata to perform the
                # actual publish. In the placeholder implementation we
                # attach the publish intent to the wrapper.
                wrapper._cello_last_publish = {
                    "topic": topic,
                    "value": result,
                }
            return result

        # Attach metadata for the Rust runtime to discover
        wrapper._cello_producer = True
        wrapper._cello_producer_topic = topic
        return wrapper
    return decorator


class KafkaConfig:
    """
    Configuration for Kafka connections.

    Provides broker addresses, consumer group settings, and tuning parameters
    for both producers and consumers.

    Example:
        config = KafkaConfig(
            brokers=["kafka1:9092", "kafka2:9092"],
            group_id="order-service",
            client_id="cello-app",
            session_timeout_ms=30000,
            max_poll_records=500,
        )
    """

    def __init__(
        self,
        brokers: list[str] = None,
        group_id: str = None,
        client_id: str = None,
        auto_commit: bool = True,
        session_timeout_ms: int = 30000,
        max_poll_records: int = 500,
    ):
        """
        Initialize Kafka configuration.

        Args:
            brokers: List of broker addresses (default: ["localhost:9092"]).
            group_id: Consumer group ID (default: None).
            client_id: Client identifier (default: None).
            auto_commit: Automatically commit offsets (default: True).
            session_timeout_ms: Session timeout in milliseconds (default: 30000).
            max_poll_records: Maximum records per poll (default: 500).
        """
        self.brokers = brokers or ["localhost:9092"]
        self.group_id = group_id
        self.client_id = client_id
        self.auto_commit = auto_commit
        self.session_timeout_ms = session_timeout_ms
        self.max_poll_records = max_poll_records

    @classmethod
    def local(cls) -> "KafkaConfig":
        """
        Create a KafkaConfig for local development with localhost defaults.

        Returns:
            KafkaConfig configured for localhost:9092.

        Example:
            config = KafkaConfig.local()
        """
        return cls(
            brokers=["localhost:9092"],
            group_id="cello-local",
            client_id="cello-dev",
        )


class RabbitMQConfig:
    """
    Configuration for RabbitMQ connections.

    Provides connection URL, virtual host, and consumer tuning parameters.

    Example:
        config = RabbitMQConfig(
            url="amqp://user:pass@rabbitmq.example.com:5672",
            vhost="/production",
            prefetch_count=20,
            heartbeat=60,
        )
    """

    def __init__(
        self,
        url: str = "amqp://localhost",
        vhost: str = "/",
        prefetch_count: int = 10,
        heartbeat: int = 60,
    ):
        """
        Initialize RabbitMQ configuration.

        Args:
            url: AMQP connection URL (default: "amqp://localhost").
            vhost: Virtual host (default: "/").
            prefetch_count: Number of prefetched messages per consumer (default: 10).
            heartbeat: Heartbeat interval in seconds (default: 60).
        """
        self.url = url
        self.vhost = vhost
        self.prefetch_count = prefetch_count
        self.heartbeat = heartbeat

    @classmethod
    def local(cls) -> "RabbitMQConfig":
        """
        Create a RabbitMQConfig for local development with localhost defaults.

        Returns:
            RabbitMQConfig configured for amqp://localhost.

        Example:
            config = RabbitMQConfig.local()
        """
        return cls(
            url="amqp://localhost",
            vhost="/",
            prefetch_count=10,
            heartbeat=60,
        )


class SqsConfig:
    """
    Configuration for AWS SQS connections.

    Provides region, queue URL, and polling parameters. Supports
    localstack for local development.

    Example:
        config = SqsConfig(
            region="us-west-2",
            queue_url="https://sqs.us-west-2.amazonaws.com/123456789/my-queue",
            max_messages=10,
            wait_time_secs=20,
        )
    """

    def __init__(
        self,
        region: str = "us-east-1",
        queue_url: str = "",
        endpoint_url: str = None,
        max_messages: int = 10,
        wait_time_secs: int = 20,
    ):
        """
        Initialize SQS configuration.

        Args:
            region: AWS region (default: "us-east-1").
            queue_url: SQS queue URL (default: "").
            endpoint_url: Custom endpoint URL for localstack or compatible services (default: None).
            max_messages: Maximum messages per receive call (default: 10).
            wait_time_secs: Long polling wait time in seconds (default: 20).
        """
        self.region = region
        self.queue_url = queue_url
        self.endpoint_url = endpoint_url
        self.max_messages = max_messages
        self.wait_time_secs = wait_time_secs

    @classmethod
    def local(cls, queue_url: str) -> "SqsConfig":
        """
        Create an SqsConfig for local development with localstack defaults.

        Args:
            queue_url: The SQS queue URL on localstack.

        Returns:
            SqsConfig configured for localstack on localhost:4566.

        Example:
            config = SqsConfig.local("http://localhost:4566/000000000000/my-queue")
        """
        return cls(
            region="us-east-1",
            queue_url=queue_url,
            endpoint_url="http://localhost:4566",
            max_messages=10,
            wait_time_secs=5,
        )


class Message:
    """
    Represents a message consumed from a queue.

    Provides access to the message payload, headers, and methods
    for acknowledging or rejecting the message.

    Example:
        @kafka_consumer(topic="orders", group="processor")
        async def handle(message):
            print(message.topic)         # "orders"
            print(message.text)          # raw string value
            data = message.json()        # parsed JSON
            print(data["order_id"])
            message.ack()
    """

    def __init__(
        self,
        id: str = None,
        topic: str = "",
        key: str = None,
        value: Any = None,
        headers: dict = None,
        timestamp: float = None,
    ):
        """
        Initialize a Message.

        Args:
            id: Unique message identifier (default: auto-generated UUID).
            topic: Topic or queue the message originated from (default: "").
            key: Message key for partitioning (default: None).
            value: Message payload (default: None).
            headers: Message headers as a dictionary (default: None).
            timestamp: Message timestamp as Unix epoch (default: None).
        """
        self.id = id or str(uuid.uuid4())
        self.topic = topic
        self.key = key
        self.value = value
        self.headers = headers or {}
        self.timestamp = timestamp or time.time()
        self._acked = False
        self._nacked = False

    @property
    def text(self) -> str:
        """
        Return the message value as a string.

        Returns:
            String representation of the message value.
        """
        if self.value is None:
            return ""
        if isinstance(self.value, bytes):
            return self.value.decode("utf-8")
        return str(self.value)

    # Maximum size in bytes for JSON deserialization to prevent
    # denial-of-service via extremely large payloads.
    MAX_JSON_SIZE: int = 10 * 1024 * 1024  # 10 MB

    def json(self) -> Any:
        """
        Parse the message value as JSON.

        SECURITY: Enforces a size limit (MAX_JSON_SIZE, default 10 MB) on the
        raw payload before parsing to prevent denial-of-service via
        oversized messages. The parsed result must be a dict or list;
        other JSON top-level types (strings, numbers, booleans, null)
        are rejected to reduce the risk of type-confusion bugs.

        Returns:
            Parsed JSON value (dict or list).

        Raises:
            json.JSONDecodeError: If the value is not valid JSON.
            ValueError: If the payload exceeds MAX_JSON_SIZE or the parsed
                        result is not a dict or list.
        """
        raw = self.value

        # Already a dict/list -- return as-is
        if isinstance(raw, (dict, list)):
            return raw

        if isinstance(raw, bytes):
            if len(raw) > self.MAX_JSON_SIZE:
                raise ValueError(
                    f"Message payload size ({len(raw)} bytes) exceeds "
                    f"maximum allowed ({self.MAX_JSON_SIZE} bytes)"
                )
            raw = raw.decode("utf-8")
        elif isinstance(raw, str):
            if len(raw.encode("utf-8")) > self.MAX_JSON_SIZE:
                raise ValueError(
                    f"Message payload size exceeds "
                    f"maximum allowed ({self.MAX_JSON_SIZE} bytes)"
                )
        else:
            raw = str(raw)

        parsed = json.loads(raw)

        if not isinstance(parsed, (dict, list)):
            raise ValueError(
                f"Expected JSON object or array, got {type(parsed).__name__}"
            )

        return parsed

    def ack(self) -> None:
        """
        Acknowledge the message.

        Signals to the broker that the message has been successfully processed
        and can be removed from the queue.
        """
        self._acked = True

    def nack(self) -> None:
        """
        Negatively acknowledge the message.

        Signals to the broker that the message was not successfully processed
        and should be redelivered or moved to a dead-letter queue.
        """
        self._nacked = True


class MessageResult:
    """
    Result constants for message processing outcomes.

    Used as return values from consumer handlers to indicate how
    the broker should handle the message after processing.

    Example:
        @kafka_consumer(topic="orders", group="processor")
        async def handle(message):
            try:
                process(message.json())
                return MessageResult.ACK
            except TransientError:
                return MessageResult.REQUEUE
            except PermanentError:
                return MessageResult.DEAD_LETTER
    """

    ACK: str = "ack"
    """Message processed successfully; remove from queue."""

    NACK: str = "nack"
    """Message processing failed; do not acknowledge."""

    REJECT: str = "reject"
    """Message is invalid; reject and discard."""

    REQUEUE: str = "requeue"
    """Message processing failed; requeue for retry."""

    DEAD_LETTER: str = "dead_letter"
    """Message processing permanently failed; move to dead-letter queue."""


class Producer:
    """
    Message producer for publishing messages to a queue or topic.

    Wraps the Rust-powered producer with a Pythonic async API.
    Supports Kafka, RabbitMQ, and SQS backends depending on the
    config type passed to connect().

    Example:
        producer = await Producer.connect(KafkaConfig.local())
        await producer.send("events", value={"type": "user_signup"}, key="user-123")
        await producer.send_batch([
            {"topic": "events", "value": {"type": "a"}},
            {"topic": "events", "value": {"type": "b"}},
        ])
        await producer.close()
    """

    def __init__(self, config):
        """
        Initialize producer wrapper.

        Args:
            config: A KafkaConfig, RabbitMQConfig, or SqsConfig instance.
        """
        self._config = config
        self._connected = False

    @classmethod
    async def connect(cls, config) -> "Producer":
        """
        Connect to the message broker and create a producer.

        Args:
            config: A KafkaConfig, RabbitMQConfig, or SqsConfig instance.

        Returns:
            Connected Producer instance.

        Example:
            producer = await Producer.connect(KafkaConfig.local())
        """
        instance = cls(config)
        # In a real implementation, this would call Rust to create the producer
        instance._connected = True  # Placeholder
        return instance

    async def send(
        self,
        topic: str,
        value: Any,
        key: str = None,
        headers: dict = None,
    ) -> bool:
        """
        Send a single message to a topic or queue.

        The value is automatically serialized as JSON if it is a dict or list.

        Args:
            topic: Target topic or queue name.
            value: Message payload (dict, list, str, or bytes).
            key: Optional message key for partitioning.
            headers: Optional message headers.

        Returns:
            True if the message was sent successfully, False otherwise.

        Example:
            success = await producer.send(
                "orders",
                value={"order_id": 42, "total": 99.99},
                key="order-42",
                headers={"source": "api"},
            )
        """
        # Placeholder - real implementation calls Rust producer
        return True

    async def send_batch(self, messages: list[dict]) -> int:
        """
        Send a batch of messages.

        Each message in the list should be a dictionary with at least a
        "topic" and "value" key. Optional keys: "key", "headers".

        Args:
            messages: List of message dictionaries.

        Returns:
            Number of messages successfully sent.

        Example:
            count = await producer.send_batch([
                {"topic": "events", "value": {"type": "a"}, "key": "k1"},
                {"topic": "events", "value": {"type": "b"}},
            ])
        """
        # Placeholder - real implementation calls Rust batch producer
        return len(messages)

    async def close(self) -> None:
        """
        Close the producer connection and flush pending messages.

        Should be called during application shutdown to ensure all
        buffered messages are delivered.
        """
        self._connected = False


class Consumer:
    """
    Message consumer for receiving messages from a queue or topic.

    Wraps the Rust-powered consumer with a Pythonic async API.
    Supports Kafka, RabbitMQ, and SQS backends depending on the
    config type passed to connect().

    Example:
        consumer = await Consumer.connect(KafkaConfig.local())
        await consumer.subscribe(["orders", "events"])

        messages = await consumer.poll(timeout_ms=1000)
        for msg in messages:
            print(msg.topic, msg.json())
            await consumer.commit(msg)

        await consumer.close()
    """

    def __init__(self, config):
        """
        Initialize consumer wrapper.

        Args:
            config: A KafkaConfig, RabbitMQConfig, or SqsConfig instance.
        """
        self._config = config
        self._connected = False
        self._subscriptions: list[str] = []

    @classmethod
    async def connect(cls, config) -> "Consumer":
        """
        Connect to the message broker and create a consumer.

        Args:
            config: A KafkaConfig, RabbitMQConfig, or SqsConfig instance.

        Returns:
            Connected Consumer instance.

        Example:
            consumer = await Consumer.connect(KafkaConfig.local())
        """
        instance = cls(config)
        # In a real implementation, this would call Rust to create the consumer
        instance._connected = True  # Placeholder
        return instance

    async def subscribe(self, topics: list[str]) -> None:
        """
        Subscribe to one or more topics or queues.

        Args:
            topics: List of topic or queue names to subscribe to.

        Example:
            await consumer.subscribe(["orders", "events", "notifications"])
        """
        self._subscriptions = list(topics)

    async def poll(self, timeout_ms: int = 1000) -> list[Message]:
        """
        Poll for new messages from subscribed topics.

        Blocks up to timeout_ms milliseconds waiting for messages.
        Returns an empty list if no messages are available.

        Args:
            timeout_ms: Maximum time to wait in milliseconds (default: 1000).

        Returns:
            List of Message objects received from the broker.

        Example:
            messages = await consumer.poll(timeout_ms=2000)
            for msg in messages:
                data = msg.json()
                process(data)
        """
        # Placeholder - real implementation calls Rust consumer
        return []

    async def commit(self, message: Message = None) -> None:
        """
        Commit consumer offsets.

        If a message is provided, commits the offset for that specific message.
        If no message is provided, commits all pending offsets.

        Args:
            message: Optional specific message to commit (default: None).

        Example:
            # Commit a specific message
            await consumer.commit(msg)

            # Commit all pending offsets
            await consumer.commit()
        """
        # Placeholder - real implementation calls Rust consumer commit
        if message is not None:
            message.ack()

    async def close(self) -> None:
        """
        Close the consumer connection.

        Commits any pending offsets and disconnects from the broker.
        Should be called during application shutdown.
        """
        self._connected = False
        self._subscriptions = []
