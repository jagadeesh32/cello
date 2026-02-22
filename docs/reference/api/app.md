---
title: App Reference
description: Cello App class API reference — complete method documentation
---

# App

The `App` class is the main entry point for Cello applications.

## Constructor

```python
from cello import App

app = App()
```

The `App` constructor takes no arguments. The application name, debug mode, and environment are configured at runtime via `app.run()`.

### Example

```python
from cello import App

app = App()
```

---

## Route Decorators

### `@app.get(path, **options)`

Register a GET route.

> *Since v0.1.0*

```python
@app.get("/users")
def list_users(request):
    return {"users": []}

@app.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern (e.g., `"/users/{id}"`) |
| `tags` | `list[str]` | `None` | OpenAPI tags for grouping |
| `summary` | `str` | `None` | OpenAPI summary |
| `description` | `str` | `None` | OpenAPI description |
| `guards` | `list[Guard]` | `None` | Authorization guards |

### `@app.post(path, **options)`

Register a POST route.

> *Since v0.1.0*

```python
@app.post("/users")
def create_user(request):
    data = request.json()
    return Response.json({"id": 1, **data}, status=201)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern |
| `tags` | `list[str]` | `None` | OpenAPI tags for grouping |
| `summary` | `str` | `None` | OpenAPI summary |
| `description` | `str` | `None` | OpenAPI description |
| `guards` | `list[Guard]` | `None` | Authorization guards |

### `@app.put(path, **options)`

Register a PUT route.

> *Since v0.1.0*

```python
@app.put("/users/{id}")
def update_user(request):
    data = request.json()
    return {"id": request.params["id"], **data}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern |
| `tags` | `list[str]` | `None` | OpenAPI tags for grouping |
| `summary` | `str` | `None` | OpenAPI summary |
| `description` | `str` | `None` | OpenAPI description |
| `guards` | `list[Guard]` | `None` | Authorization guards |

### `@app.patch(path, **options)`

Register a PATCH route.

> *Since v0.1.0*

```python
@app.patch("/users/{id}")
def patch_user(request):
    data = request.json()
    return {"updated": True}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern |
| `tags` | `list[str]` | `None` | OpenAPI tags for grouping |
| `summary` | `str` | `None` | OpenAPI summary |
| `description` | `str` | `None` | OpenAPI description |
| `guards` | `list[Guard]` | `None` | Authorization guards |

### `@app.delete(path, **options)`

Register a DELETE route.

> *Since v0.1.0*

```python
@app.delete("/users/{id}")
def delete_user(request):
    return Response.no_content()
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern |
| `tags` | `list[str]` | `None` | OpenAPI tags for grouping |
| `summary` | `str` | `None` | OpenAPI summary |
| `description` | `str` | `None` | OpenAPI description |
| `guards` | `list[Guard]` | `None` | Authorization guards |

### `@app.options(path, **options)`

Register an OPTIONS route.

> *Since v0.1.0*

```python
@app.options("/resource")
def resource_options(request):
    return Response.json({"methods": ["GET", "POST", "PUT", "DELETE"]})
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern |
| `guards` | `list[Guard]` | `None` | Authorization guards |

!!! tip
    CORS preflight requests are handled automatically by the CORS middleware. Use `@app.options()` only for custom OPTIONS responses.

### `@app.head(path, **options)`

Register a HEAD route.

> *Since v0.1.0*

```python
@app.head("/health")
def health_check(request):
    return Response("", status=200)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern |
| `guards` | `list[Guard]` | `None` | Authorization guards |

### `@app.route(path, methods, **options)`

Register a route that handles multiple HTTP methods.

> *Since v0.1.0*

```python
@app.route("/resource", methods=["GET", "POST"])
def resource_handler(request):
    if request.method == "GET":
        return {"action": "list"}
    return {"action": "create"}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `path` | `str` | *required* | URL path pattern |
| `methods` | `list[str]` | `["GET"]` | List of HTTP methods (e.g., `["GET", "POST"]`) |

### Route Options

All route decorators (`get`, `post`, `put`, `patch`, `delete`) accept the following shared options:

| Option | Type | Description |
|--------|------|-------------|
| `guards` | `list[Guard]` | Authorization guards |
| `tags` | `list[str]` | OpenAPI tags |
| `summary` | `str` | OpenAPI summary |
| `description` | `str` | OpenAPI description |

---

## WebSocket

### `@app.websocket(path)`

Register a WebSocket route.

> *Since v0.3.0*

```python
@app.websocket("/ws")
def websocket_handler(ws):
    ws.send_text("Connected!")
    while True:
        message = ws.recv()
        if message is None:
            break
        ws.send_text(f"Echo: {message.text}")
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | `str` | URL path for WebSocket endpoint |

---

## Middleware

### `app.enable_cors(origins)`

Enable CORS middleware.

> *Since v0.2.0*

```python
# Allow all origins (default)
app.enable_cors()

# Restrict to specific origins
app.enable_cors(origins=["https://example.com", "https://app.example.com"])
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `origins` | `list[str]` | `None` (allows all) | List of allowed origins |

### `app.enable_logging()`

Enable request/response logging middleware.

> *Since v0.2.0*

```python
app.enable_logging()
```

This method takes no parameters.

### `app.enable_compression(min_size)`

Enable gzip response compression.

> *Since v0.2.0*

```python
# Default minimum size (1024 bytes)
app.enable_compression()

# Custom minimum size
app.enable_compression(min_size=512)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `min_size` | `int` | `None` (1024) | Minimum response size in bytes to compress |

### `app.enable_rate_limit(config)`

Enable rate limiting middleware.

> *Since v0.4.0*

```python
from cello import RateLimitConfig

config = RateLimitConfig.token_bucket(max_requests=100, window_secs=60)
app.enable_rate_limit(config)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `config` | `RateLimitConfig` | Rate limit configuration. Use `RateLimitConfig.token_bucket()`, `.sliding_window()`, or `.adaptive()` to create. |

### `app.enable_caching(ttl, methods, exclude_paths)`

Enable smart response caching with TTL and tag-based invalidation.

> *Since v0.6.0*

```python
# Default caching (300s TTL, GET and HEAD only)
app.enable_caching()

# Custom configuration
app.enable_caching(
    ttl=600,
    methods=["GET"],
    exclude_paths=["/api/realtime", "/ws"]
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ttl` | `int` | `300` | Default time-to-live in seconds |
| `methods` | `list[str]` | `["GET", "HEAD"]` | HTTP methods to cache |
| `exclude_paths` | `list[str]` | `None` | Paths to exclude from caching |

### `app.enable_circuit_breaker(failure_threshold, reset_timeout, half_open_target, failure_codes)`

Enable circuit breaker for fault tolerance.

> *Since v0.6.0*

```python
# Default configuration
app.enable_circuit_breaker()

# Custom thresholds
app.enable_circuit_breaker(
    failure_threshold=5,
    reset_timeout=30,
    half_open_target=3,
    failure_codes=[500, 502, 503, 504]
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `int` | `5` | Failures before opening circuit |
| `reset_timeout` | `int` | `30` | Seconds to wait before half-open state |
| `half_open_target` | `int` | `3` | Successes needed to close circuit |
| `failure_codes` | `list[int]` | `[500, 502, 503, 504]` | Status codes considered failures |

### `app.enable_prometheus(endpoint, namespace, subsystem)`

Enable Prometheus metrics collection and exposition.

> *Since v0.5.0*

```python
app.enable_prometheus(
    endpoint="/metrics",
    namespace="cello",
    subsystem="http"
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `endpoint` | `str` | `"/metrics"` | Metrics endpoint path |
| `namespace` | `str` | `"cello"` | Metric namespace prefix |
| `subsystem` | `str` | `"http"` | Metric subsystem prefix |

---

## API Documentation

### `app.enable_openapi(title, version)`

Enable automatic OpenAPI schema generation and interactive documentation.

> *Since v0.5.0*

Adds the following endpoints:

- `GET /docs` -- Swagger UI
- `GET /redoc` -- ReDoc documentation
- `GET /openapi.json` -- OpenAPI JSON schema

```python
app.enable_openapi(
    title="My API",
    version="1.0.0"
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `title` | `str` | `"Cello API"` | API title |
| `version` | `str` | `"1.0.0"` | API version |

---

## Data Layer

### `app.enable_database(config)`

Enable database connection pooling.

> *Since v0.8.0*

Configures an async connection pool for PostgreSQL, MySQL, or SQLite. Supports connection health monitoring, automatic reconnection, and query statistics.

```python
from cello import App, DatabaseConfig

app = App()
app.enable_database(DatabaseConfig(
    url="postgresql://user:pass@localhost/mydb",
    pool_size=20,
    max_lifetime_secs=1800
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `DatabaseConfig` | `DatabaseConfig("sqlite://cello.db")` | Database configuration instance |

When `config` is `None`, a default SQLite database at `sqlite://cello.db` is used.

### `app.enable_redis(config)`

Enable Redis connection pooling.

> *Since v0.8.0*

Configures an async Redis client with connection pooling, supporting standard and cluster modes.

```python
from cello import App, RedisConfig

app = App()
app.enable_redis(RedisConfig(
    url="redis://localhost:6379",
    pool_size=10,
    cluster_mode=False
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `RedisConfig` | `RedisConfig()` | Redis configuration instance |

When `config` is `None`, defaults from `RedisConfig()` are used.

---

## Observability

### `app.enable_telemetry(config)`

Enable OpenTelemetry distributed tracing and metrics.

> *Since v0.7.0*

```python
from cello import App, OpenTelemetryConfig

app = App()
app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317",
    sampling_rate=0.1
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `OpenTelemetryConfig` | `OpenTelemetryConfig("cello-service")` | OpenTelemetry configuration instance |

When `config` is `None`, a default configuration with service name `"cello-service"` is used.

### `app.enable_health_checks(config)`

Enable Kubernetes-compatible health check endpoints.

> *Since v0.7.0*

Adds the following endpoints:

- `GET /health/live` -- Liveness probe
- `GET /health/ready` -- Readiness probe
- `GET /health/startup` -- Startup probe
- `GET /health` -- Full health report

```python
from cello import App, HealthCheckConfig

app = App()
app.enable_health_checks(HealthCheckConfig(
    base_path="/health",
    include_system_info=True
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `HealthCheckConfig` | `None` | Health check configuration instance |

---

## API Protocols

### `app.enable_graphql(config)`

Enable GraphQL endpoint with optional Playground.

> *Since v0.7.0*

```python
from cello import App, GraphQLConfig

app = App()
app.enable_graphql(GraphQLConfig(
    path="/graphql",
    playground=True,
    introspection=True
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `GraphQLConfig` | `GraphQLConfig()` | GraphQL configuration instance |

When `config` is `None`, defaults from `GraphQLConfig()` are used.

### `app.enable_grpc(config)`

Enable gRPC server support.

> *Since v0.9.0*

```python
from cello import App, GrpcConfig

app = App()
app.enable_grpc(GrpcConfig(
    address="[::]:50051",
    reflection=True,
    enable_web=True
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `GrpcConfig` | `GrpcConfig()` | gRPC configuration instance |

When `config` is `None`, defaults from `GrpcConfig()` are used.

### `app.add_grpc_service(name, methods)`

Register a gRPC service with the application.

> *Since v0.9.0*

```python
app.enable_grpc(GrpcConfig(address="[::]:50051"))
app.add_grpc_service("UserService", ["GetUser", "ListUsers"])
app.add_grpc_service("OrderService", ["CreateOrder", "GetOrder"])
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `name` | `str` | *required* | Service name |
| `methods` | `list[str]` | `None` | Optional list of method names |

---

## Messaging

### `app.enable_messaging(config)`

Enable Kafka message queue integration.

> *Since v0.9.0*

```python
from cello import App, KafkaConfig

app = App()
app.enable_messaging(KafkaConfig(
    brokers=["localhost:9092"],
    group_id="my-group"
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `KafkaConfig` | `KafkaConfig()` | Kafka configuration instance |

When `config` is `None`, defaults from `KafkaConfig()` are used.

### `app.enable_rabbitmq(config)`

Enable RabbitMQ message queue integration.

> *Since v0.9.0*

```python
from cello import App, RabbitMQConfig

app = App()
app.enable_rabbitmq(RabbitMQConfig(
    url="amqp://localhost",
    prefetch_count=20
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `RabbitMQConfig` | `RabbitMQConfig()` | RabbitMQ configuration instance |

When `config` is `None`, defaults from `RabbitMQConfig()` are used.

### `app.enable_sqs(config)`

Enable AWS SQS message queue integration.

> *Since v0.9.0*

```python
from cello import App, SqsConfig

app = App()
app.enable_sqs(SqsConfig(
    region="us-west-2",
    queue_url="https://sqs.us-west-2.amazonaws.com/123/queue"
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `SqsConfig` | `SqsConfig()` | SQS configuration instance |

When `config` is `None`, defaults from `SqsConfig()` are used.

---

## Advanced Patterns

### `app.enable_event_sourcing(config)`

Enable event sourcing support.

> *Since v0.10.0*

Configures the event sourcing subsystem with storage backend, snapshot support, and event retention settings. Returns the `App` instance for method chaining.

```python
from cello import App, EventSourcingConfig

app = App()
app.enable_event_sourcing(EventSourcingConfig(
    store_type="postgresql",
    snapshot_interval=100,
    enable_snapshots=True
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `EventSourcingConfig` | `EventSourcingConfig()` | Event sourcing configuration instance |

When `config` is `None`, defaults from `EventSourcingConfig()` are used.

### `app.enable_cqrs(config)`

Enable Command Query Responsibility Segregation.

> *Since v0.10.0*

Configures the CQRS subsystem with event synchronization, command/query timeouts, and retry settings. Returns the `App` instance for method chaining.

```python
from cello import App, CqrsConfig

app = App()
app.enable_cqrs(CqrsConfig(
    enable_event_sync=True,
    command_timeout_ms=10000
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `CqrsConfig` | `CqrsConfig()` | CQRS configuration instance |

When `config` is `None`, defaults from `CqrsConfig()` are used.

### `app.enable_saga(config)`

Enable saga orchestration pattern.

> *Since v0.10.0*

Configures the saga orchestration subsystem with retry behaviour, timeouts, and logging settings. Returns the `App` instance for method chaining.

```python
from cello import App, SagaConfig

app = App()
app.enable_saga(SagaConfig(
    max_retries=5,
    timeout_ms=60000
))
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `config` | `SagaConfig` | `SagaConfig()` | Saga configuration instance |

When `config` is `None`, defaults from `SagaConfig()` are used.

---

## Security

### `app.add_guard(guard)`

Add a global guard to all routes.

> *Since v0.5.0*

```python
from cello.guards import AuthenticatedGuard

app.add_guard(AuthenticatedGuard())
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `guard` | `Guard` | Guard instance or callable |

---

## Dependency Injection

### `app.register_singleton(name, value)`

Register a shared singleton dependency accessible across handlers.

> *Since v0.5.0*

```python
db = DatabaseConnection()
app.register_singleton("db", db)

# Access in handlers via Depends
from cello import Depends

@app.get("/users")
def get_users(db=Depends("db")):
    return db.query("SELECT * FROM users")
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `str` | Unique identifier for the dependency |
| `value` | `Any` | Singleton instance |

---

## Caching

### `app.invalidate_cache(tags)`

Invalidate cached responses by tags.

> *Since v0.6.0*

```python
@app.post("/users")
def create_user(request):
    user = create_in_db(request.json())
    app.invalidate_cache(["users"])
    return {"user": user}
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `tags` | `list[str]` | List of cache tags to invalidate |

---

## Blueprints

### `app.register_blueprint(blueprint)`

Register a blueprint with the application.

> *Since v0.3.0*

```python
from cello import Blueprint

api = Blueprint("/api/v1")

@api.get("/users")
def list_users(request):
    return {"users": []}

@api.post("/users")
def create_user(request):
    return {"created": True}

app.register_blueprint(api)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `blueprint` | `Blueprint` | Blueprint instance to register |

---

## Lifecycle Hooks

### `@app.on_event(event_name)`

Register a lifecycle event handler.

> *Since v0.5.0*

```python
@app.on_event("startup")
async def on_start():
    print("Starting up...")
    app.state.db = await Database.connect()

@app.on_event("shutdown")
async def on_stop():
    print("Shutting down...")
    await app.state.db.disconnect()
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `event_name` | `str` | Event name: `"startup"` or `"shutdown"` |

---

## Exception Handlers

### `@app.exception_handler(exception_type)`

Register a global exception handler.

> *Since v0.4.0*

```python
@app.exception_handler(ValueError)
def handle_value_error(request, exc):
    return Response.json({
        "type": "/errors/validation",
        "title": "Validation Error",
        "status": 400,
        "detail": str(exc)
    }, status=400)

@app.exception_handler(Exception)
def handle_all_errors(request, exc):
    return Response.json(
        {"error": "Internal server error"},
        status=500
    )
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `exception_type` | `type` | Exception class to handle (e.g., `ValueError`, `Exception`) |

---

## Running the Application

### `app.run(**options)`

Start the application server.

> *Since v0.1.0*

```python
# Simple development server
app.run()

# Production configuration
app.run(
    host="0.0.0.0",
    port=8080,
    env="production",
    workers=4,
    reload=False,
    debug=False
)
```

### Run Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `host` | `str` | `"127.0.0.1"` | Host address to bind to |
| `port` | `int` | `8000` | Port number to bind to |
| `debug` | `bool` | `None` | Enable debug mode (defaults to `True` in dev, `False` in prod) |
| `env` | `str` | `"development"` | Environment: `"development"` or `"production"` |
| `workers` | `int` | `None` | Number of worker processes (defaults to CPU count in production, 1 in debug) |
| `reload` | `bool` | `False` | Enable hot reload (watches `.py` files for changes) |
| `logs` | `bool` | `None` | Enable request logging (defaults to `True` in debug mode) |

CLI arguments `--host`, `--port`, `--env`, `--debug`, `--reload`, `--workers`, and `--no-logs` override the values passed to `app.run()`.

---

## Application State

### `app.state`

Store application-level state.

> *Since v0.1.0*

```python
@app.on_event("startup")
async def startup():
    app.state.db = await Database.connect()
    app.state.cache = await Redis.connect()

@app.get("/users")
def list_users(request):
    db = request.app.state.db
    return {"users": db.get_users()}
```
