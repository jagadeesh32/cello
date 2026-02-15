---
title: Event-Driven Pattern
description: Building event-driven architectures with Cello using message queues and event sourcing
---

# Event-Driven Pattern

Event-driven architecture decouples services by communicating through events rather than direct calls. A service publishes an event when something happens; other services subscribe and react independently. This pattern improves scalability, resilience, and extensibility.

---

## Core Concepts

| Term | Description |
|------|-------------|
| **Event** | An immutable record of something that happened (e.g., `UserCreated`) |
| **Publisher** | The service that emits the event |
| **Subscriber** | A service that listens for and reacts to events |
| **Broker** | The infrastructure that delivers events (Kafka, RabbitMQ, SQS) |

---

## In-Process Event Bus

For a single-service application, start with a simple in-process event bus.

```python
# events/bus.py
from typing import Callable, Dict, List
import asyncio

class EventBus:
    def __init__(self):
        self._subscribers: Dict[str, List[Callable]] = {}

    def subscribe(self, event_type: str, handler: Callable):
        self._subscribers.setdefault(event_type, []).append(handler)

    async def publish(self, event_type: str, data: dict):
        handlers = self._subscribers.get(event_type, [])
        for handler in handlers:
            await handler(data)
```

### Usage

```python
from cello import App
from events.bus import EventBus

app = App()
bus = EventBus()

# Subscribe
async def on_user_created(data):
    print(f"Welcome email sent to {data['email']}")

async def on_user_created_audit(data):
    print(f"Audit log: user {data['id']} created")

bus.subscribe("user.created", on_user_created)
bus.subscribe("user.created", on_user_created_audit)

# Publish from a handler
@app.post("/users")
async def create_user(request):
    data = request.json()
    user = {"id": 1, **data}
    await bus.publish("user.created", user)
    return user
```

---

## Using Kafka for Distributed Events

For microservices, use Kafka to deliver events between services.

### Publisher Service

```python
from cello import App, KafkaConfig

app = App()
app.enable_messaging(KafkaConfig(
    brokers=["localhost:9092"],
    group_id="user-service",
))

@app.post("/users")
async def create_user(request):
    data = request.json()
    user = save_user(data)

    # Publish event to Kafka topic
    await app.publish_message("user.events", {
        "type": "UserCreated",
        "user_id": user["id"],
        "email": user["email"],
        "timestamp": time.time(),
    })

    return user
```

### Subscriber Service

```python
from cello import App, KafkaConfig

app = App()
app.enable_messaging(KafkaConfig(
    brokers=["localhost:9092"],
    group_id="notification-service",
))

@app.on_message("user.events")
async def handle_user_event(message):
    if message["type"] == "UserCreated":
        await send_welcome_email(message["email"])
    elif message["type"] == "UserDeleted":
        await cleanup_user_data(message["user_id"])
```

---

## Using RabbitMQ

```python
from cello import App, RabbitMQConfig

app = App()
app.enable_rabbitmq(RabbitMQConfig(
    url="amqp://localhost",
    prefetch_count=20,
))

# Publish
@app.post("/orders")
async def create_order(request):
    order = process_order(request.json())
    await app.publish_message("order.created", order)
    return order

# Subscribe
@app.on_message("order.created")
async def handle_order_created(message):
    await update_inventory(message["items"])
```

---

## Event Design Guidelines

### Event Structure

Define a consistent schema for all events.

```python
{
    "type": "OrderPlaced",           # What happened
    "version": 1,                    # Schema version
    "timestamp": "2026-01-15T...",   # When it happened
    "source": "order-service",       # Who emitted it
    "data": {                        # Domain payload
        "order_id": 42,
        "user_id": 7,
        "total": 99.99
    }
}
```

### Best Practices

| Guideline | Reason |
|-----------|--------|
| Events are immutable | Once published, they should never be modified |
| Use past tense names | `OrderPlaced`, not `PlaceOrder` (that is a command) |
| Include a version field | Allows subscribers to handle schema changes |
| Keep payloads small | Include IDs; subscribers can fetch full data if needed |
| Make subscribers idempotent | Messages may be delivered more than once |

---

## Event Sourcing Overview

Event sourcing stores the sequence of events as the source of truth rather than the current state.

```
UserCreated -> NameChanged -> EmailChanged -> AccountDeactivated
```

The current state is derived by replaying events. This approach provides a complete audit trail and enables time-travel debugging.

```python
class UserAggregate:
    def __init__(self):
        self.events = []
        self.state = {}

    def apply(self, event: dict):
        self.events.append(event)
        if event["type"] == "UserCreated":
            self.state = {"id": event["data"]["id"], "name": event["data"]["name"]}
        elif event["type"] == "NameChanged":
            self.state["name"] = event["data"]["new_name"]

    def get_state(self) -> dict:
        return self.state
```

!!! info
    Event sourcing adds complexity. Use it when you need a full audit trail or when the event history itself is a business requirement (finance, compliance, etc.).

---

## Decoupling Services

The event-driven pattern lets services evolve independently.

```
User Service --publishes--> [user.created] --subscribes--> Notification Service
                                           --subscribes--> Analytics Service
                                           --subscribes--> Billing Service
```

Adding a new subscriber (e.g., Analytics Service) does not require any changes to the User Service.

---

## Error Handling

### Dead Letter Queues

When a subscriber fails to process an event, route it to a dead letter queue (DLQ) for later inspection.

```python
app.enable_messaging(KafkaConfig(
    brokers=["localhost:9092"],
    group_id="notification-service",
    dead_letter_topic="dlq.notifications",
    max_retries=3,
))
```

### Retry Strategies

- **Immediate retry** -- Retry the message immediately (up to a limit).
- **Exponential backoff** -- Wait progressively longer between retries.
- **Dead letter** -- After max retries, send to DLQ for manual review.

---

## Next Steps

- See [CQRS pattern](cqrs.md) for separating reads and writes with events.
- See [Circuit Breaker](../../reference/api/middleware.md) for protecting against cascading failures.
- See the [Microservices tutorial](../tutorials/microservices.md) for a complete multi-service example.
