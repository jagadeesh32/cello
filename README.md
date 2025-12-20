<p align="center">
  <img src="docs/logo-full.png" alt="Cello" width="400">
</p>

<p align="center">
  <strong>The World's Fastest Python Web Framework</strong>
</p>

<p align="center">
  <a href="https://github.com/jagadeesh32/cello/actions/workflows/ci.yml"><img src="https://github.com/jagadeesh32/cello/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://pypi.org/project/cello-framework/"><img src="https://img.shields.io/pypi/v/cello-framework.svg" alt="PyPI"></a>
  <a href="https://pypi.org/project/cello-framework/"><img src="https://img.shields.io/pypi/pyversions/cello-framework.svg" alt="Python"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License"></a>
</p>

<p align="center">
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-features">Features</a> •
  <a href="#-examples">Examples</a> •
  <a href="docs/README.md">Documentation</a>
</p>

---

## Why Cello?

Cello is the **fastest Python web framework** — combining Python's developer experience with Rust's raw performance. All HTTP handling, routing, JSON serialization, and middleware execute in native Rust. Python handles only your business logic.

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
# 🚀 Cello running at http://127.0.0.1:8000
```

---

## ✨ Features

### Core Features

| Feature | Description |
|---------|-------------|
| 🚀 **Blazing Fast** | Tokio + Hyper async HTTP engine in pure Rust |
| 📦 **SIMD JSON** | SIMD-accelerated JSON parsing with `simd-json` |
| �️ **Radix Routing** | Ultra-fast route matching with `matchit` |
| 🔄 **Async/Sync** | Support for both `async def` and regular `def` handlers |
| �🛡️ **Middleware** | Built-in CORS, logging, compression, rate limiting |
| � **Blueprints** | Flask-like route grouping and modular apps |
| 🌐 **WebSocket** | Real-time bidirectional communication |
| 📡 **SSE** | Server-Sent Events for streaming |
| 📁 **Multipart** | File uploads and form data handling |

### Advanced Features

| Feature | Description |
|---------|-------------|
| 🔐 **Authentication** | JWT, Basic Auth, API Key with constant-time validation |
| 🛡️ **CSRF Protection** | Double-submit cookie and signed token patterns |
| ⏱️ **Rate Limiting** | Token bucket and sliding window algorithms |
| 🍪 **Sessions** | Secure cookie-based session management |
| 🛡️ **Security Headers** | CSP, HSTS, X-Frame-Options, Referrer-Policy |
| 🏭 **Cluster Mode** | Multi-worker process deployment |
| 🔒 **TLS/SSL** | Native HTTPS with rustls |
| ⚡ **HTTP/2 & HTTP/3** | Modern protocol support including QUIC |
| ⏰ **Timeouts** | Request/response timeout protection |
| 🆔 **Request ID** | Automatic request tracing with UUID |
| 📏 **Body Limits** | Request size validation and protection |
| 📂 **Static Files** | Efficient static file serving with caching |
| 🏷️ **ETag/Caching** | HTTP caching with ETag support |
| ⚠️ **Exception Handling** | Global error handlers with RFC 7807 support |
| 🔄 **Lifecycle Hooks** | Startup/shutdown events for app initialization |
| 📦 **DTOs** | Data Transfer Objects with field filtering |

### 🆕 New in v0.5.0 - Best of FastAPI, Litestar, Robyn & Django!

| Feature | Inspired By | Description |
|---------|-------------|-------------|
| 💉 **Dependency Injection** | FastAPI | Type-safe DI with Singleton/Request/Transient scopes |
| 🛡️ **Guards (RBAC)** | Litestar | Role & permission-based access control with composable guards |
| 📊 **Prometheus Metrics** | Litestar | Production-ready metrics with automatic `/metrics` endpoint |
| 📄 **OpenAPI/Swagger** | FastAPI | Auto-generated API documentation at `/docs` |
| 🎯 **Background Tasks** | FastAPI | Execute tasks after response is sent |
| 📝 **Template Rendering** | Django | Jinja2-compatible template support |

> All implemented in **pure Rust** for maximum performance! See [docs/new-middleware-features.md](docs/new-middleware-features.md) for details.

---

## � Examples

#### 🎯 Advanced Features Demo (v0.5.0)

See `examples/comprehensive_demo.py` for a complete demonstration of all new features:
- Dependency Injection
- Guards/RBAC
- Prometheus Metrics
- Exception Handling
- Advanced Caching
- DTO System

```bash
python examples/comprehensive_demo.py
```

### Blueprints (Route Grouping)

```python
from cello import App, Blueprint

# Create versioned API blueprint
api_v1 = Blueprint("/api/v1")

@api_v1.get("/users")
def list_users(request):
    return {"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}

@api_v1.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}

@api_v1.post("/users")
def create_user(request):
    return Response.json(request.json(), status=201)

# Mount blueprint
app = App()
app.register_blueprint(api_v1)
app.run()
```

### Async Handlers

```python
@app.get("/sync")
def sync_handler(request):
    """Sync handler for simple operations"""
    return {"message": "Hello from sync!"}

@app.get("/async")
async def async_handler(request):
    """Async handler for I/O operations"""
    users = await database.fetch_users()
    return {"users": users}

@app.post("/data")
async def process_data(request):
    data = request.json()
    result = await external_api.process(data)
    return {"result": result}
```

### Request Object

```python
@app.post("/example")
def handler(request):
    # Request properties
    request.method              # "GET", "POST", etc.
    request.path                # "/users/123"
    request.params["id"]        # Path parameters
    request.query["search"]     # Query parameters (?search=value)
    request.get_header("Authorization")
    
    # Body parsing
    request.body()              # Raw bytes
    request.text()              # String body
    request.json()              # Parsed JSON dict
    request.form()              # Form data dict
    
    return {"received": True}
```

### Response Types

```python
from cello import Response

# JSON (default - just return a dict)
return {"data": "value"}

# Explicit JSON with status
return Response.json({"created": True}, status=201)

# Text response
return Response.text("Hello, World!")

# HTML response
return Response.html("<h1>Welcome</h1>")

# File download
return Response.file("/path/to/document.pdf")

# Redirect
return Response.redirect("/new-location")

# No content (204)
return Response.no_content()

# Binary data
return Response.binary(image_bytes, content_type="image/png")

# Custom headers
response = Response.json({"ok": True})
response.set_header("X-Custom", "value")
return response
```

### Middleware

```python
app = App()

# CORS - Cross-Origin Resource Sharing
app.enable_cors()
app.enable_cors(origins=["https://example.com", "https://app.example.com"])

# Request logging
app.enable_logging()

# Gzip compression
app.enable_compression()
app.enable_compression(min_size=1024)  # Only compress if > 1KB

# Security headers (CSP, HSTS, X-Frame-Options, etc.)
app.enable_security_headers()

# Rate limiting
app.enable_rate_limit(requests=100, window=60)  # 100 req/min
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
        
        # Echo back
        ws.send_text(f"You said: {message.text}")
        
        # Or send JSON
        ws.send_json({"type": "message", "content": message.text})
```

### Server-Sent Events (SSE)

```python
from cello import SseEvent, SseStream

@app.get("/events")
def event_stream(request):
    stream = SseStream()
    
    # Simple data
    stream.add_data("Connection established")
    
    # Named events with JSON
    stream.add_event("update", '{"count": 42}')
    stream.add_event("notification", '{"message": "New data available"}')
    
    return stream
```

### JWT Authentication

```python
from cello import App
from cello.middleware import JwtConfig, JwtAuth

# Configure JWT
jwt_config = JwtConfig(secret=b"your-secret-key-min-32-bytes-long")
jwt_auth = JwtAuth(jwt_config).skip_path("/public")

app = App()
app.use(jwt_auth)

@app.get("/protected")
def protected(request):
    claims = request.context.get("jwt_claims")
    return {"user": claims["sub"]}
```

---

## 🛠️ CLI & Configuration

```bash
# Development mode (hot reload + debug logs)
python app.py --env development --reload

# Production mode
python app.py --env production --workers 8 --port 8080 --no-logs
```

| Argument | Default | Description |
|----------|---------|-------------|
| `--host` | 127.0.0.1 | Host to bind to |
| `--port` | 8000 | Port to bind to |
| `--env` | development | Environment mode |
| `--reload` | False | Hot reload on file changes |
| `--workers` | CPU count | Number of worker threads |
| `--debug` | Auto | Enable debug logging |
| `--no-logs` | False | Disable request logging |

```python
# Programmatic configuration
app.run(
    host="0.0.0.0",
    port=8080,
    env="production",
    workers=4
)
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
| **Compression** | flate2 (gzip) |
| **JWT** | jsonwebtoken |
| **Concurrent Maps** | dashmap |
| **Security** | subtle (constant-time ops) |

---

## 🔒 Security

Cello is built with security as a priority:

- ✅ **Constant-time comparison** for passwords, API keys, and tokens
- ✅ **CSRF protection** with double-submit cookies and signed tokens
- ✅ **Security headers** (CSP, HSTS, X-Frame-Options, Referrer-Policy)
- ✅ **Rate limiting** to prevent abuse
- ✅ **Session security** (Secure, HttpOnly, SameSite cookies)
- ✅ **Path traversal protection** in static file serving
- ✅ **JWT blacklisting** for token revocation

---

## 📊 Benchmarks

Cello is designed to be the fastest Python web framework:

- **Zero-copy** request parsing
- **SIMD-accelerated** JSON
- **Arena allocation** for memory efficiency
- **Lock-free** concurrent data structures
- **Native Rust** HTTP handling

*Benchmark results coming soon*

---

## 🛠️ Development

```bash
# Setup
git clone https://github.com/jagadeesh32/cello.git
cd cello
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest requests

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

- 📖 [Getting Started](docs/getting-started.md)
- 🔧 [Configuration](docs/configuration.md)
- 🛡️ [Security Guide](docs/security.md)
- 📡 [WebSocket Guide](docs/websocket.md)
- 🏭 [Deployment](docs/deployment.md)

Full documentation: [docs/README.md](docs/README.md)

---

## 🤝 Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## 📄 License

MIT License - see [LICENSE](LICENSE)

---

## 👤 Author

**Jagadeesh Katla**

- GitHub: [@jagadeesh32](https://github.com/jagadeesh32)

---

<p align="center">
  Made with ❤️ using 🐍 Python and 🦀 Rust
</p>
