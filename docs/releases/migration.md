---
title: Migration Guide
---

# Migration Guide

This guide helps you migrate between major versions of Cello.

## 0.9.x to 0.10.x {#09x-to-010x}

### New Features

Version 0.10.0 adds advanced pattern features:

- Event Sourcing with Aggregate roots, event replay, and snapshots
- CQRS with Command/Query separation, dedicated buses, and event-driven sync
- Saga Pattern for distributed transactions with compensation logic

### Breaking Changes

No breaking changes from v0.9.0. All existing code continues to work.

### New Imports

```python
from cello import EventStoreConfig, CqrsConfig, SagaConfig
```

### New Python Modules

```python
from cello.eventsourcing import Aggregate, Event, event_handler, EventStore, EventStoreConfig, Snapshot
from cello.cqrs import Command, Query, CommandBus, QueryBus, command_handler, query_handler, CqrsConfig
from cello.saga import Saga, SagaStep, SagaConfig, SagaResult
```

### New App Methods

```python
app = App()

# Event Sourcing
app.enable_event_store()

# CQRS
app.enable_cqrs()

# Saga
app.enable_sagas()
```

### External Dependencies

| Feature          | External Dependency Required          |
|------------------|---------------------------------------|
| Event Sourcing   | PostgreSQL (or in-memory for dev)     |
| CQRS             | None                                  |
| Saga Pattern     | PostgreSQL (or in-memory for dev)     |

### New APIs (v0.10.0)

```python
from cello import App, EventStoreConfig, CqrsConfig, SagaConfig
from cello.eventsourcing import Aggregate, Event, event_handler
from cello.cqrs import Command, Query, command_handler, query_handler
from cello.saga import Saga, SagaStep

app = App()

# Event Sourcing
class OrderCreated(Event):
    order_id: str
    customer_id: str

class Order(Aggregate):
    @event_handler(OrderCreated)
    def on_created(self, event):
        self.id = event.order_id
        self.status = "created"

app.enable_event_store(EventStoreConfig(
    storage="postgresql://localhost/events",
    snapshot_interval=100,
))

# CQRS
class CreateOrderCommand(Command):
    customer_id: str
    items: list

@command_handler(CreateOrderCommand)
async def handle_create(command, db):
    order = Order.create(command.customer_id, command.items)
    await db.save(order)
    return order.id

class GetOrderQuery(Query):
    order_id: str

@query_handler(GetOrderQuery)
async def handle_get(query, read_db):
    return await read_db.get_order(query.order_id)

app.enable_cqrs(CqrsConfig(enable_event_sync=True))

# Saga
async def reserve(ctx):
    return await inventory.reserve(ctx["order_id"])

async def release(ctx):
    await inventory.release(ctx["reservation_id"])

class OrderSaga(Saga):
    steps = [
        SagaStep("reserve_inventory", reserve, compensate=release),
    ]

app.enable_sagas(SagaConfig(
    storage="postgresql://localhost/sagas",
    max_retries=3,
))
```

---

## 0.8.x to 0.9.x {#08x-to-09x}

### New Features

Version 0.9.0 adds API protocol features:

- GraphQL support with Query, Mutation, Subscription decorators and DataLoader
- gRPC support with GrpcService base class, @grpc_method decorator, and bidirectional streaming
- Message queue adapters for Kafka, RabbitMQ, and AWS SQS

### Breaking Changes

No breaking changes from v0.8.0. All existing code continues to work.

### New Imports

```python
from cello import GrpcConfig, KafkaConfig, RabbitMQConfig, SqsConfig
```

### New Python Modules

```python
from cello.graphql import Query, Mutation, Subscription, Schema, DataLoader
from cello.grpc import GrpcService, grpc_method, GrpcResponse, GrpcServer, GrpcChannel, GrpcError
from cello.messaging import kafka_consumer, kafka_producer, Message, MessageResult, Producer, Consumer
```

### New App Methods

```python
app = App()

# gRPC
app.enable_grpc()
app.add_grpc_service(UserService())

# Messaging
app.enable_messaging()       # Kafka (default)
app.enable_rabbitmq()        # RabbitMQ
app.enable_sqs()             # AWS SQS
```

### External Dependencies

| Feature    | External Dependency Required         |
|------------|--------------------------------------|
| GraphQL    | None                                 |
| gRPC       | None                                 |
| Kafka      | Kafka broker needed                  |
| RabbitMQ   | RabbitMQ server needed               |
| SQS        | AWS account or LocalStack            |

### New APIs (v0.9.0)

```python
from cello import App, KafkaConfig
from cello.graphql import Query, Schema, DataLoader
from cello.grpc import GrpcService, grpc_method, GrpcResponse
from cello.messaging import kafka_consumer, Message, MessageResult

app = App()

# GraphQL
@Query
def users(info) -> list[dict]:
    return [{"id": 1, "name": "Alice"}]

schema = Schema().query(users).build()
app.mount("/graphql", schema)

# gRPC
class UserService(GrpcService):
    @grpc_method
    async def GetUser(self, request):
        return GrpcResponse.ok({"id": request.id, "name": "Alice"})

app.enable_grpc()
app.add_grpc_service(UserService())

# Kafka consumer
@kafka_consumer(topic="orders", group="processors")
async def process_order(message: Message):
    data = message.json()
    await handle_order(data)
    return MessageResult.ACK

app.enable_messaging()
```

---

## 0.7.x to 0.8.x {#07x-to-08x}

### New Features

Version 0.8.0 adds data layer features:

- Enhanced database connection pooling
- Redis integration with clustering support
- Transaction support for database operations
- Bug fixes (CORS origins, logs typo, Response.error)

### Breaking Changes

No breaking changes in 0.8.0. All existing code continues to work.

### New APIs (v0.8.0)

```python
from cello import App
from cello.database import DatabaseConfig, Database
from cello.cache import Redis

app = App()

# Database connection pooling (enhanced)
db = await Database.connect(DatabaseConfig(
    url="postgresql://localhost/mydb",
    pool_size=20,
    max_lifetime=1800
))

# Redis integration
redis = await Redis.connect("redis://localhost:6379")
await redis.set("key", "value", ttl=300)

# Transaction support
async with db.transaction() as tx:
    await tx.execute("INSERT INTO users (name) VALUES ($1)", "Alice")
    await tx.execute("INSERT INTO logs (action) VALUES ($1)", "user_created")
```

## 0.6.x to 0.7.x {#06x-to-07x}

### New Features

Version 0.7.0 adds enterprise features:

- OpenTelemetry distributed tracing
- Kubernetes-compatible health checks
- Database connection pooling
- GraphQL support

### Breaking Changes

No breaking changes in 0.7.0. All existing code continues to work.

### New APIs

```python
from cello import App, OpenTelemetryConfig, HealthCheckConfig, GraphQLConfig

app = App()

# Enable telemetry
app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317"
))

# Enable health checks
app.enable_health_checks(HealthCheckConfig(
    base_path="/health",
    include_details=True
))

# Enable GraphQL
app.enable_graphql(GraphQLConfig(
    path="/graphql",
    playground=True
))
```

## 0.5.x to 0.6.x {#05x-to-06x}

### New Features

Version 0.6.0 introduced:

- Guards and RBAC
- Rate limiting with multiple algorithms
- Circuit breaker pattern
- Prometheus metrics
- Caching middleware

### Breaking Changes

No breaking changes in 0.6.0.

### New APIs

```python
from cello import App, RateLimitConfig

app = App()

# Enable rate limiting
app.enable_rate_limit(RateLimitConfig.token_bucket(
    capacity=100,
    refill_rate=10
))

# Enable Prometheus metrics
app.enable_prometheus(endpoint="/metrics")

# Add guards
@app.add_guard
def require_auth(request):
    return request.headers.get("Authorization") is not None
```

## 0.4.x to 0.5.x {#04x-to-05x}

### New Features

Version 0.5.0 introduced:

- Background tasks
- Template engine
- OpenAPI documentation

### Breaking Changes

No breaking changes in 0.5.0.
