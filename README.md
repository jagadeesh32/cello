<p align="center">
  <img src="https://cello-framework.vercel.app/logo-full.png" alt="Cello" width="400">
</p>

<p align="center">
  <strong>Ultra-Fast Python Web Framework</strong><br>
  <em>Rust-powered performance with Python simplicity</em>
</p>

<p align="center">
  <a href="https://github.com/jagadeesh32/cello/actions/workflows/ci.yml"><img src="https://github.com/jagadeesh32/cello/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://pypi.org/project/cello-framework/"><img src="https://img.shields.io/pypi/v/cello-framework.svg" alt="PyPI"></a>
  <a href="https://pypi.org/project/cello-framework/"><img src="https://img.shields.io/pypi/pyversions/cello-framework.svg" alt="Python"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License"></a>
</p>

<p align="center">
  <a href="#-installation">Installation</a> •
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-features">Features</a> •
  <a href="#-examples">Examples</a> •
  <a href="https://cello-framework.vercel.app/">Documentation</a>
</p>

---

## Why Cello?

Cello is an **enterprise-grade Python web framework** that combines Python's developer experience with Rust's raw performance. All HTTP handling, routing, JSON serialization, and middleware execute in native Rust while Python handles your business logic.

```
┌─────────────────────────────────────────────────────────────────┐
│  Request → Rust HTTP Engine → Python Handler → Rust Response   │
│                  │                    │                         │
│                  ├─ SIMD JSON         ├─ Return dict            │
│                  ├─ Radix routing     └─ Return Response        │
│                  └─ Middleware (Rust)                           │
└─────────────────────────────────────────────────────────────────┘
```

---

## ⚡ Performance

Cello is the **fastest Python web framework** — benchmarked with `wrk` on the same machine, same worker count, same settings.

### Benchmark Results (4 workers, 5 processes each, wrk -t12 -c400 -d10s)

| Framework | Server | Req/sec | Avg Latency | p99 Latency | Relative |
|-----------|--------|---------|-------------|-------------|----------|
| **Cello** | **Built-in (Rust/Tokio)** | **170,000+** | **2.76ms** | **15.34ms** | **1.0x (fastest)** |
| BlackSheep + Granian | Granian (Rust) | ~92,000 | 4.31ms | 12.63ms | 1.9x slower |
| FastAPI + Granian | Granian (Rust) | ~55,000 | 7.14ms | 16.86ms | 3.1x slower |
| Robyn | Built-in (Rust) | ~29,000 | 14.21ms | 37.91ms | 5.9x slower |

> **How to reproduce**: See [`benchmarks/compare/`](benchmarks/compare/) for the automated comparison runner. All frameworks use the same JSON endpoint, same process count, and same `wrk` settings for a fair comparison.

---

## 📦 Installation

```bash
pip install cello-framework
```

**From source:**
```bash
git clone https://github.com/jagadeesh32/cello.git
cd cello
pip install maturin
maturin develop
```

**Requirements:** Python 3.12+

---

## 🚀 Quick Start

```python
from cello import App, Response

app = App()

@app.get("/")
def home(request):
    return {"message": "Hello, Cello! 🎸"}

@app.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"], "name": "John Doe"}

@app.post("/users")
def create_user(request):
    data = request.json()
    return Response.json({"id": 1, **data}, status=201)

if __name__ == "__main__":
    app.run()
```

```bash
python app.py
# 🐍 Cello v1.0.0 server starting at http://127.0.0.1:8000
```

---

## ✨ Features

### Core Features

| Feature | Description |
|---------|-------------|
| 🚀 **Blazing Fast** | Tokio + Hyper async HTTP engine in pure Rust |
| 📦 **SIMD JSON** | SIMD-accelerated JSON parsing with `simd-json` |
| 🛤️ **Radix Routing** | Ultra-fast route matching with `matchit` |
| 🔄 **Async/Sync** | Support for both `async def` and regular `def` handlers |
| 🛡️ **Middleware** | Built-in CORS, logging, compression, rate limiting |
| 📐 **Blueprints** | Flask-like route grouping and modular apps |
| 🌐 **WebSocket** | Real-time bidirectional communication |
| 📡 **SSE** | Server-Sent Events for streaming |
| 📁 **Multipart** | File uploads and form data handling |

### Security Features

| Feature | Description |
|---------|-------------|
| 🔐 **JWT Authentication** | JSON Web Token with constant-time validation |
| 🛡️ **CSRF Protection** | Double-submit cookie and signed token patterns |
| ⏱️ **Rate Limiting** | Token bucket, sliding window, and adaptive algorithms |
| 🍪 **Sessions** | Secure cookie-based session management |
| 🔒 **Security Headers** | CSP, HSTS, X-Frame-Options, Referrer-Policy |
| 🔑 **API Key Auth** | Header and query parameter authentication |

### Enterprise Features (v0.7.0+)

| Feature | Description |
|---------|-------------|
| 📊 **OpenTelemetry** | Distributed tracing with W3C Trace Context |
| 🏥 **Health Checks** | Kubernetes-compatible liveness/readiness probes |
| 🗄️ **Database Pooling** | Connection pool management with metrics |
| 🔷 **GraphQL** | GraphQL endpoint with Playground UI |
| 💉 **Dependency Injection** | Type-safe DI with Singleton/Request/Transient scopes |
| 🛡️ **Guards (RBAC)** | Role & permission-based access control |
| 📈 **Prometheus Metrics** | Production-ready metrics at `/metrics` |
| 🔌 **Circuit Breaker** | Fault tolerance with automatic recovery |

### Data Layer Features (v0.8.0)

| Feature | Description |
|---------|-------------|
| 🗄️ **Enhanced DB Pooling** | Async connection pool with health monitoring & reconnection |
| 🔴 **Redis Integration** | Async Redis client with pool, Pub/Sub, cluster mode |
| 🔄 **Transactions** | Automatic transaction management with decorator support |

### API Protocol Features (v0.9.0)

| Feature | Description |
|---------|-------------|
| 🔷 **GraphQL** | Query, Mutation, Subscription decorators with Schema builder |
| 📊 **DataLoader** | N+1 prevention with automatic batching and caching |
| 🔌 **gRPC** | Service-based gRPC with unary, streaming, and bidirectional support |
| 📨 **Kafka** | Consumer/producer decorators with automatic message routing |
| 🐰 **RabbitMQ** | AMQP messaging with topic exchanges and prefetch control |
| ☁️ **SQS/SNS** | AWS message queue integration with LocalStack support |

### Protocol Support

| Feature | Description |
|---------|-------------|
| 🔒 **TLS/SSL** | Native HTTPS with rustls |
| ⚡ **HTTP/2** | Multiplexed connections with h2 |
| 🚀 **HTTP/3** | QUIC protocol support with quinn |
| 🏭 **Cluster Mode** | Multi-worker process deployment |

---

## 📘 Examples

### Data Layer Features (v0.8.0)

```python
from cello import App, DatabaseConfig, RedisConfig
from cello.database import transactional

app = App()

# Enable database connection pooling
app.enable_database(DatabaseConfig(
    url="postgresql://user:pass@localhost/mydb",
    pool_size=20,
    max_lifetime_secs=1800
))

# Enable Redis connection
app.enable_redis(RedisConfig(
    url="redis://localhost:6379",
    pool_size=10
))

@app.post("/transfer")
@transactional
async def transfer(request):
    # Automatic transaction management
    return {"success": True}

@app.get("/")
def home(request):
    return {"status": "ok", "version": "1.0.0"}

app.run()
```

### API Protocol Features (v0.9.0)

```python
from cello import App, GrpcConfig, KafkaConfig, RabbitMQConfig
from cello.graphql import Query, Mutation, Schema, DataLoader, GraphQL
from cello.grpc import GrpcService, grpc_method, GrpcServer
from cello.messaging import kafka_consumer, kafka_producer, Producer, Consumer

app = App()

# --- GraphQL ---
@Query
def users(info):
    return [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]

@Mutation
def create_user(info, name: str, email: str):
    return {"id": 3, "name": name, "email": email}

schema = Schema().query(users).mutation(create_user).build()

# --- gRPC ---
class UserService(GrpcService):
    @grpc_method
    async def get_user(self, request):
        return {"id": request.data.get("id"), "name": "Alice"}

app.enable_grpc(GrpcConfig(port=50051, reflection=True))
app.add_grpc_service("UserService", ["GetUser", "ListUsers"])

# --- Kafka ---
app.enable_messaging(KafkaConfig(brokers="localhost:9092", group_id="my-app"))

@kafka_consumer(topic="user-events", group="processors")
async def handle_user_event(message):
    print(f"Received: {message.text}")

@app.post("/users")
@kafka_producer(topic="user-events")
def create_user_api(request):
    return {"id": 1, "name": request.json().get("name")}

# --- RabbitMQ ---
app.enable_rabbitmq(RabbitMQConfig(url="amqp://localhost:5672"))

@app.get("/")
def home(request):
    return {"status": "ok", "version": "1.0.0", "protocols": ["graphql", "grpc", "kafka", "rabbitmq"]}

app.run()
```

### Enterprise Features (v0.7.0+)

```python
from cello import App, OpenTelemetryConfig, HealthCheckConfig, GraphQLConfig

app = App()

# Enable distributed tracing
app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-api",
    otlp_endpoint="http://collector:4317",
    sampling_rate=0.1
))

# Enable Kubernetes health checks
app.enable_health_checks(HealthCheckConfig(
    base_path="/health",
    include_details=True,
    include_system_info=True
))

# Enable GraphQL with Playground
app.enable_graphql(GraphQLConfig(
    path="/graphql",
    playground=True,
    introspection=True
))

# Enable Prometheus metrics
app.enable_prometheus(endpoint="/metrics")

@app.get("/")
def home(request):
    return {"status": "ok", "version": "1.0.0"}

app.run()
```

### Blueprints (Route Grouping)

```python
from cello import App, Blueprint

api_v1 = Blueprint("/api/v1")

@api_v1.get("/users")
def list_users(request):
    return {"users": [{"id": 1, "name": "Alice"}]}

@api_v1.post("/users")
def create_user(request):
    return Response.json(request.json(), status=201)

app = App()
app.register_blueprint(api_v1)
app.run()
```

### Guards (RBAC)

```python
from cello import App, RateLimitConfig

app = App()

# Role-based access control
@app.add_guard
def require_auth(request):
    return request.headers.get("Authorization") is not None

@app.add_guard
def require_admin(request):
    token = request.headers.get("Authorization", "")
    return "admin" in token

# Rate limiting
app.enable_rate_limit(RateLimitConfig.token_bucket(
    capacity=100,
    refill_rate=10
))

@app.get("/admin")
def admin_panel(request):
    return {"message": "Welcome, Admin!"}
```

### WebSocket

```python
@app.websocket("/ws/chat")
def chat_handler(ws):
    ws.send_text("Welcome to the chat!")

    while True:
        message = ws.recv()
        if message is None:
            break
        ws.send_json({"type": "echo", "content": message.text})
```

### Server-Sent Events

```python
from cello import SseStream

@app.get("/events")
def event_stream(request):
    stream = SseStream()
    stream.add_event("update", '{"count": 42}')
    stream.add_event("notification", '{"message": "New data"}')
    return stream
```

### Response Types

```python
from cello import Response

# JSON (default)
return {"data": "value"}

# Explicit JSON with status
return Response.json({"created": True}, status=201)

# Other response types
return Response.text("Hello, World!")
return Response.html("<h1>Welcome</h1>")
return Response.file("/path/to/document.pdf")
return Response.redirect("/new-location")
return Response.no_content()
```

---

## 🏗️ Tech Stack

| Component | Technology |
|-----------|------------|
| **Runtime** | Tokio (async Rust) |
| **HTTP Server** | Hyper 1.x |
| **JSON** | simd-json + serde |
| **Routing** | matchit (radix tree) |
| **Python Bindings** | PyO3 |
| **TLS/SSL** | rustls |
| **HTTP/2** | h2 |
| **HTTP/3** | quinn (QUIC) |
| **Tracing** | OpenTelemetry |
| **Metrics** | Prometheus |
| **JWT** | jsonwebtoken |
| **gRPC** | Custom Rust gRPC engine |
| **GraphQL** | Python engine with Rust serialization |
| **Messaging** | Kafka, RabbitMQ, SQS adapters |

---

## 🔒 Security

Cello is built with security as a priority:

- ✅ **Constant-time comparison** for passwords, API keys, and tokens
- ✅ **CSRF protection** with double-submit cookies and signed tokens
- ✅ **Security headers** (CSP, HSTS, X-Frame-Options, Referrer-Policy)
- ✅ **Rate limiting** with multiple algorithms
- ✅ **Session security** (Secure, HttpOnly, SameSite cookies)
- ✅ **Path traversal protection** in static file serving
- ✅ **JWT blacklisting** for token revocation

---

## 🛠️ Development

```bash
# Setup
git clone https://github.com/jagadeesh32/cello.git
cd cello
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest

# Build
maturin develop

# Test
pytest tests/ -v

# Lint
cargo clippy
cargo fmt
```

---

## 📋 Release History

### v1.0.0 — Production-Ready Stable Release (Feb 2026)

- **170,000+ req/s** sustained throughput (fastest Python web framework)
- Handler metadata caching, lazy query parsing, zero-copy response building
- TCP_NODELAY, HTTP/1.1 keep-alive and pipeline flush optimization
- Pre-allocated headers, fast-path skip for empty middleware/guards
- Optimized release profile: LTO fat, panic abort, symbol stripping
- API stability guarantee under Semantic Versioning
- 394 tests passing, comprehensive security hardening

### v0.10.0 — Event Sourcing, CQRS & Saga Pattern

- **Event Sourcing**: Aggregate root pattern, event store, snapshot support, event versioning
- **CQRS**: Command/Query buses, separate read/write models, event-driven sync
- **Saga Pattern**: Distributed transaction coordination, compensation logic, persistent state, retry with backoff

### v0.9.0 — GraphQL, gRPC & Message Queues

- **GraphQL**: Query, Mutation, Subscription decorators, DataLoader for N+1 prevention, schema introspection
- **gRPC**: Protocol buffer integration, bidirectional streaming, gRPC-Web, reflection service
- **Kafka**: Consumer/producer decorators, consumer group management, dead letter queues
- **RabbitMQ**: AMQP messaging with topic exchanges and prefetch control
- **SQS/SNS**: AWS message queue integration with LocalStack support

### v0.8.0 — Database & Redis Integration

- Enhanced database connection pooling (PostgreSQL, MySQL, SQLite) with health monitoring
- Redis async client with connection pooling, Pub/Sub, and cluster mode
- Query builder with parameterized queries
- Transaction management with `@transactional` decorator and nested savepoints
- Pool metrics exposed via Prometheus

### v0.7.0 — Enterprise Observability

- OpenTelemetry distributed tracing with OTLP export
- Health check endpoints (`/health/live`, `/health/ready`, `/health/startup`)
- Structured JSON logging with trace context injection
- Kubernetes deployment support and Docker multi-stage builds

### v0.6.0 — Smart Caching & Validation

- `@cache` decorator with TTL and tag-based invalidation
- Adaptive rate limiting based on server health metrics
- DTO validation with RFC 7807 Problem Details errors
- Circuit breaker middleware for fault tolerance
- 15% faster JSON parsing, 20% lower memory usage

### v0.5.0 — Dependency Injection & RBAC

- Dependency injection via `Depends` with singleton and transient lifetimes
- Composable guards: `RoleGuard`, `PermissionGuard`, `AndGuard`, `OrGuard`, `NotGuard`
- Prometheus metrics endpoint (`/metrics`)
- OpenAPI 3.0 schema generation with Swagger UI and ReDoc
- Background tasks and Jinja2 template rendering

### v0.4.0 — Security & Cluster Mode

- JWT authentication (HS256/384/512, RS256/384/512, ES256/384)
- Rate limiting with token bucket and sliding window algorithms
- Encrypted cookie sessions with automatic rotation
- Security headers: CSP, HSTS, X-Frame-Options, X-Content-Type-Options
- Cluster mode with multi-process workers via SO_REUSEPORT
- Native TLS via rustls (TLS 1.2 and 1.3)

### v0.3.0 — Real-Time Communication

- WebSocket support via `tokio-tungstenite` with full-duplex communication
- Server-Sent Events (SSE) with async generators
- Multipart form handling and file uploads via `multer`
- Blueprints for modular route organization with nesting

### v0.2.0 — Middleware System

- Composable middleware chain execution
- CORS middleware with configurable origins, methods, and headers
- Request/response logging middleware
- Gzip and brotli compression middleware

### v0.1.0 — Initial Release

- Rust-powered HTTP server via Hyper and Tokio
- Python route registration with decorators (`@app.get`, `@app.post`, etc.)
- Radix tree routing via matchit with path parameters and wildcards
- SIMD-accelerated JSON parsing via simd-json
- Async handler support and static file serving
- PyO3 abi3 bindings for Python 3.12+

---

## 📚 Documentation

Full documentation available at: **[cello-framework.vercel.app](https://cello-framework.vercel.app/)**

- 📖 [Getting Started](https://cello-framework.vercel.app/getting-started/)
- ✨ [Features](https://cello-framework.vercel.app/features/)
- 📘 [API Reference](https://cello-framework.vercel.app/reference/)
- 🏢 [Enterprise Guide](https://cello-framework.vercel.app/enterprise/)
- 📝 [Examples](https://cello-framework.vercel.app/examples/)

---

## 🤝 Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## 📄 License

MIT License - see [LICENSE](LICENSE)

---

## 👤 Author

**Jagadeesh Katla** - [@jagadeesh32](https://github.com/jagadeesh32)

---

<p align="center">
  Made with ❤️ using 🐍 Python and 🦀 Rust
</p>
