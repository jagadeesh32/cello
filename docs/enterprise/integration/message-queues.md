---
title: Message Queue Integration
description: Kafka, RabbitMQ, and SQS support in Cello Framework
---

# Message Queue Integration

Cello provides first-class support for message queues with decorator-based consumers, producers, and configuration for Kafka, RabbitMQ, and AWS SQS.

## Quick Start

```python
from cello import App, KafkaConfig, RabbitMQConfig, SqsConfig
from cello.messaging import kafka_consumer, kafka_producer, Message, MessageResult

app = App()
app.enable_messaging(KafkaConfig(brokers="localhost:9092", group_id="my-app"))

@kafka_consumer(topic="orders", group="order-processor")
async def process_order(message: Message):
    order = message.json()
    await fulfill_order(order)
    return MessageResult.ACK

@app.post("/orders")
@kafka_producer(topic="order-events")
def create_order(request):
    return {"order_id": 1, "status": "created"}

app.run()
```

## Kafka

### Consumer Decorator

Subscribe to Kafka topics with the `@kafka_consumer` decorator:

```python
from cello.messaging import kafka_consumer, Message, MessageResult

@kafka_consumer(topic="user-events", group="processors", auto_commit=True)
async def handle_user_event(message: Message):
    data = message.json()
    print(f"Received: {data}")
    return MessageResult.ACK
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `topic` | Required | Kafka topic to consume from |
| `group` | `"default"` | Consumer group ID |
| `auto_commit` | `True` | Auto-commit offsets |

### Producer Decorator

Auto-publish handler return values to a Kafka topic:

```python
from cello.messaging import kafka_producer

@app.post("/events")
@kafka_producer(topic="app-events")
def publish_event(request):
    return {"type": "user_signup", "user_id": 42}
```

### KafkaConfig

```python
from cello import KafkaConfig

# Full configuration
config = KafkaConfig(
    brokers="broker1:9092,broker2:9092",
    group_id="my-service",
    client_id="cello-app",
    auto_commit=True,
    session_timeout_ms=30000,
    max_poll_records=500
)

# Local development
config = KafkaConfig.local()  # localhost:9092

app.enable_messaging(config)
```

### Manual Producer/Consumer

```python
from cello.messaging import Producer, Consumer, Message

# Producer
producer = await Producer.connect(config)
await producer.send("my-topic", {"key": "value"})
await producer.send_batch([
    {"topic": "t1", "value": "msg1"},
    {"topic": "t1", "value": "msg2"},
])
await producer.close()

# Consumer
consumer = await Consumer.connect(config)
await consumer.subscribe(["topic1", "topic2"])
messages = await consumer.poll(timeout_ms=1000)
for msg in messages:
    print(msg.text)
    await consumer.commit(msg)
await consumer.close()
```

## Message Class

Wrapper for consumed messages with convenient accessors:

```python
message = Message(id="1", topic="orders", key="order-1", value='{"id": 1}')

# Access properties
message.id        # "1"
message.topic     # "orders"
message.key       # "order-1"
message.text      # '{"id": 1}' (string)
message.json()    # {"id": 1} (parsed dict)

# Acknowledgment
message.ack()     # Acknowledge message
message.nack()    # Negative acknowledge
```

## MessageResult

Constants for consumer return values:

```python
from cello.messaging import MessageResult

MessageResult.ACK          # "ack" - Successfully processed
MessageResult.NACK         # "nack" - Processing failed
MessageResult.REJECT       # "reject" - Reject permanently
MessageResult.REQUEUE      # "requeue" - Requeue for retry
MessageResult.DEAD_LETTER  # "dead_letter" - Send to DLQ
```

## RabbitMQ

```python
from cello import RabbitMQConfig

config = RabbitMQConfig(
    url="amqp://guest:guest@localhost:5672/",
    vhost="/",
    prefetch_count=10,
    heartbeat=60
)

# Local development
config = RabbitMQConfig.local()  # amqp://guest:guest@localhost:5672

app.enable_rabbitmq(config)
```

| Option | Default | Description |
|--------|---------|-------------|
| `url` | `amqp://localhost` | AMQP connection URL |
| `vhost` | `/` | Virtual host |
| `prefetch_count` | `10` | Prefetch count for consumers |
| `heartbeat` | `60` | Heartbeat interval in seconds |

## AWS SQS

```python
from cello import SqsConfig

config = SqsConfig(
    region="us-east-1",
    queue_url="https://sqs.us-east-1.amazonaws.com/123456789/my-queue",
    max_messages=10,
    wait_time_secs=20
)

# Local development (LocalStack)
config = SqsConfig.local(queue_url="http://localhost:4566/000000000000/test-queue")

app.enable_sqs(config)
```

| Option | Default | Description |
|--------|---------|-------------|
| `region` | `us-east-1` | AWS region |
| `queue_url` | Required | SQS queue URL |
| `endpoint_url` | `None` | Custom endpoint (for LocalStack) |
| `max_messages` | `10` | Max messages per poll |
| `wait_time_secs` | `20` | Long poll wait time |

## API Reference

| Class/Function | Description |
|---------------|-------------|
| `kafka_consumer` | Decorator to subscribe handler to Kafka topic |
| `kafka_producer` | Decorator to auto-publish handler returns |
| `KafkaConfig` | Kafka broker configuration (Rust-backed) |
| `RabbitMQConfig` | RabbitMQ connection configuration (Rust-backed) |
| `SqsConfig` | AWS SQS configuration (Rust-backed) |
| `Message` | Consumed message wrapper |
| `MessageResult` | Consumer result constants |
| `Producer` | Manual message producer |
| `Consumer` | Manual message consumer |
