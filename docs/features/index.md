---
title: Features
description: Comprehensive feature documentation for Cello Framework
---

# Features

Cello provides a comprehensive set of features for building modern web applications. All features are implemented in Rust for maximum performance.

## Feature Categories

<div class="grid cards" markdown>

-   :material-routes:{ .lg .middle } **Core Features**

    ---

    Routing, request handling, response types, blueprints, and async support

    [:octicons-arrow-right-24: Core](core/routing.md)

-   :material-middleware:{ .lg .middle } **Middleware**

    ---

    CORS, compression, logging, rate limiting, caching, and circuit breaker

    [:octicons-arrow-right-24: Middleware](middleware/overview.md)

-   :material-shield:{ .lg .middle } **Security**

    ---

    Authentication, authorization, JWT, sessions, CSRF, and security headers

    [:octicons-arrow-right-24: Security](security/overview.md)

-   :material-broadcast:{ .lg .middle } **Real-time**

    ---

    WebSocket and Server-Sent Events for real-time communication

    [:octicons-arrow-right-24: Real-time](realtime/websocket.md)

-   :material-cog:{ .lg .middle } **Advanced**

    ---

    Dependency injection, background tasks, templates, file uploads, DTOs

    [:octicons-arrow-right-24: Advanced](advanced/dependency-injection.md)

</div>

---

## Feature Matrix

### Core Features

| Feature | Description | Version |
|---------|-------------|---------|
| **Routing** | Radix-tree based routing with path parameters | v0.1.0 |
| **Request Handling** | Lazy body parsing, headers, query params | v0.1.0 |
| **Response Types** | JSON, HTML, text, binary, streaming | v0.1.0 |
| **Blueprints** | Flask-like route grouping | v0.2.0 |
| **Async Support** | Both sync and async handlers | v0.1.0 |
| **SIMD JSON** | Hardware-accelerated JSON parsing | v0.1.0 |

### Middleware

| Feature | Description | Version |
|---------|-------------|---------|
| **CORS** | Cross-Origin Resource Sharing | v0.2.0 |
| **Compression** | Gzip compression | v0.2.0 |
| **Logging** | Request/response logging | v0.2.0 |
| **Rate Limiting** | Token bucket & sliding window | v0.4.0 |
| **Caching** | Smart caching with TTL | v0.6.0 |
| **Circuit Breaker** | Fault tolerance | v0.6.0 |
| **Request ID** | UUID-based request tracing | v0.4.0 |
| **Body Limits** | Request size validation | v0.4.0 |

### Security

| Feature | Description | Version |
|---------|-------------|---------|
| **JWT Authentication** | JSON Web Token auth | v0.4.0 |
| **Basic Authentication** | HTTP Basic auth | v0.4.0 |
| **API Key Authentication** | API key validation | v0.4.0 |
| **Guards (RBAC)** | Role-based access control | v0.5.0 |
| **Sessions** | Secure cookie sessions | v0.4.0 |
| **CSRF Protection** | Double-submit cookies | v0.4.0 |
| **Security Headers** | CSP, HSTS, X-Frame-Options | v0.4.0 |

### Real-time

| Feature | Description | Version |
|---------|-------------|---------|
| **WebSocket** | Bidirectional real-time | v0.3.0 |
| **Server-Sent Events** | Server push streaming | v0.3.0 |

### Advanced

| Feature | Description | Version |
|---------|-------------|---------|
| **Dependency Injection** | FastAPI-style DI | v0.5.0 |
| **Background Tasks** | Post-response execution | v0.5.0 |
| **Templates** | Jinja2-compatible rendering | v0.5.0 |
| **Static Files** | Efficient file serving | v0.4.0 |
| **File Uploads** | Multipart form handling | v0.3.0 |
| **DTOs & Validation** | Data transfer objects | v0.6.0 |
| **OpenAPI/Swagger** | Auto-generated API docs | v0.5.0 |
| **Prometheus Metrics** | Production monitoring | v0.5.0 |

### Enterprise

| Feature | Description | Version |
|---------|-------------|---------|
| **Cluster Mode** | Multi-worker deployment | v0.4.0 |
| **TLS/SSL** | Native HTTPS (rustls) | v0.4.0 |
| **HTTP/2** | Modern protocol support | v0.4.0 |
| **HTTP/3 (QUIC)** | Next-gen protocol | v0.4.0 |
| **Lifecycle Hooks** | Startup/shutdown events | v0.4.0 |
| **Exception Handling** | RFC 7807 Problem Details | v0.5.0 |

---

## Implementation Philosophy

All features in Cello follow these principles:

### 1. Rust-First Implementation

Every feature is implemented in Rust for:

- **Performance**: No Python overhead on the hot path
- **Safety**: Memory safety without garbage collection
- **Concurrency**: True parallelism without GIL limitations

### 2. Python Developer Experience

Despite Rust internals, the API is Python-native:

```python
# Clean, intuitive Python API
@app.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}
```

### 3. Zero-Configuration Defaults

Features work with sensible defaults:

```python
# Just enable it - sensible defaults applied
app.enable_cors()
app.enable_logging()
app.enable_compression()
```

### 4. Full Configurability

Every feature can be customized:

```python
# Or configure in detail
app.enable_cors(
    origins=["https://example.com"],
    methods=["GET", "POST"],
    max_age=3600
)
```

---

## Quick Examples

### Routing with Parameters

```python
@app.get("/users/{user_id}/posts/{post_id}")
def get_post(request):
    return {
        "user_id": request.params["user_id"],
        "post_id": request.params["post_id"]
    }
```

### JWT Authentication

```python
from cello.middleware import JwtConfig, JwtAuth

jwt_config = JwtConfig(secret=b"your-secret-key-min-32-bytes-long")
app.use(JwtAuth(jwt_config))

@app.get("/protected")
def protected(request):
    claims = request.context.get("jwt_claims")
    return {"user": claims["sub"]}
```

### Rate Limiting

```python
# 100 requests per minute per IP
app.enable_rate_limit(requests=100, window=60)
```

### WebSocket

```python
@app.websocket("/ws/chat")
def chat(ws):
    ws.send_text("Welcome!")
    while True:
        msg = ws.recv()
        if msg is None:
            break
        ws.send_text(f"Echo: {msg.text}")
```

### Dependency Injection

```python
from cello import Depends

def get_db():
    return Database()

@app.get("/users")
def list_users(request, db=Depends(get_db)):
    return {"users": db.get_all_users()}
```

---

## Performance Characteristics

All features are optimized for performance:

| Feature | Overhead | Notes |
|---------|----------|-------|
| Routing | ~100ns | Radix tree lookup |
| JSON Parsing | ~1μs/KB | SIMD acceleration |
| JWT Validation | ~50μs | Constant-time comparison |
| Rate Limiting | ~100ns | Lock-free counters |
| Compression | ~1μs/KB | Native gzip |
| Middleware Chain | ~1μs | Zero-allocation |

---

## Next Steps

Start exploring specific feature categories:

- [:material-routes: Core Features](core/routing.md) - Routing, requests, responses
- [:material-middleware: Middleware](middleware/overview.md) - Built-in middleware
- [:material-shield: Security](security/overview.md) - Auth and security
- [:material-broadcast: Real-time](realtime/websocket.md) - WebSocket and SSE
- [:material-cog: Advanced](advanced/dependency-injection.md) - DI, tasks, templates
