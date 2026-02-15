---
title: Migration Guide
---

# Migration Guide

This guide helps you migrate between major versions of Cello.

## 0.8.x to 0.9.x {#08x-to-09x}

### New Features

Version 0.9.0 adds API protocol features:

- GraphQL support with Playground UI
- gRPC support with bidirectional streaming
- Message queue adapters (Kafka, RabbitMQ)

### Breaking Changes

No breaking changes in 0.9.0. All existing code continues to work.

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
