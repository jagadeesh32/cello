<p align="center">
  <img src="https://raw.githubusercontent.com/jagadeesh32/cello/main/docs/assets/logo.svg" alt="Cello" width="400">
</p>

<p align="center">
  <strong>The World's Fastest Python Web Framework</strong><br>
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
  <a href="https://jagadeesh32.github.io/cello/">Documentation</a>
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
# 🐍 Cello v0.7.0 server starting at http://127.0.0.1:8000
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

### Enterprise Features (v0.7.0)

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

### Protocol Support

| Feature | Description |
|---------|-------------|
| 🔒 **TLS/SSL** | Native HTTPS with rustls |
| ⚡ **HTTP/2** | Multiplexed connections with h2 |
| 🚀 **HTTP/3** | QUIC protocol support with quinn |
| 🏭 **Cluster Mode** | Multi-worker process deployment |

---

## 📘 Examples

### Enterprise Features (v0.7.0)

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
    return {"status": "ok", "version": "0.7.0"}

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

## 📚 Documentation

Full documentation available at: **[jagadeesh32.github.io/cello](https://jagadeesh32.github.io/cello/)**

- 📖 [Getting Started](https://jagadeesh32.github.io/cello/getting-started/)
- ✨ [Features](https://jagadeesh32.github.io/cello/features/)
- 📘 [API Reference](https://jagadeesh32.github.io/cello/reference/)
- 🏢 [Enterprise Guide](https://jagadeesh32.github.io/cello/enterprise/)
- 📝 [Examples](https://jagadeesh32.github.io/cello/examples/)

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
