# Cello Documentation 📚

Welcome to the Cello documentation! Cello is an ultra-fast Rust-powered Python async web framework.

## Quick Navigation

| Document | Description |
|----------|-------------|
| [Getting Started](getting-started.md) | Installation and basic usage |
| [Configuration](configuration.md) | All configuration options |
| [Middleware](middleware.md) | Built-in and custom middleware |
| [Security](security.md) | Authentication, rate limiting, headers |
| [Advanced Features](advanced.md) | Cluster mode, protocols, lifecycle |
| [Routing](routing.md) | Routes, blueprints, constraints |
| [API Reference](api-reference.md) | Complete API documentation |
| [Deployment](deployment.md) | Production deployment guide |
| [Changelog](changelog.md) | Version history |

## What is Cello?

Cello is a high-performance web framework that combines **Python's developer experience** with **Rust's raw speed**. All HTTP handling, routing, and JSON serialization happen in Rust—Python handles only your business logic.

```
Request → Rust HTTP Engine → Python Handler → Rust Response
              │                    │
              ├─ SIMD JSON         ├─ Return dict
              ├─ Radix routing     └─ Return Response
              └─ Middleware
```

## Features at a Glance

### Core Features
- 🚀 **Blazing Fast** - Tokio + Hyper HTTP engine in pure Rust
- 📦 **SIMD JSON** - SIMD-accelerated JSON with simd-json
- 🛡️ **Middleware** - CORS, logging, compression, and more
- 🗺️ **Blueprints** - Flask-like route grouping
- 🌐 **WebSocket** - Real-time bidirectional communication
- 📡 **SSE** - Server-Sent Events streaming
- 📁 **File Uploads** - Multipart form data handling

### Advanced Features (v0.5.0)
- 🔐 **Authentication** - JWT, Basic Auth, API Key
- ⏱️ **Rate Limiting** - Token bucket, sliding window
- 🍪 **Sessions** - Secure cookie-based sessions
- 🛡️ **Security Headers** - CSP, HSTS, X-Frame-Options
- 🏭 **Cluster Mode** - Multi-worker deployment
- 🔒 **TLS/SSL** - Native TLS support
- 🌐 **HTTP/2 & HTTP/3** - Modern protocol support
- ⏰ **Timeouts & Limits** - Request protection
- 💉 **Dependency Injection** - FastAPI-style DI
- 🛡️ **Guards (RBAC)** - Role-based access control
- 📊 **Prometheus Metrics** - Production metrics
- 📄 **OpenAPI/Swagger** - Auto API documentation
- 🎯 **Background Tasks** - Post-response execution
- 📝 **Template Rendering** - Jinja2-style templates

## Quick Start

```python
from cello import App

app = App()

@app.get("/")
def home(request):
    return {"message": "Hello, Cello!"}

if __name__ == "__main__":
    app.run()
```

See [Getting Started](getting-started.md) for more details.

## Community

- **GitHub**: [github.com/jagadeesh32/cello](https://github.com/jagadeesh32/cello)
- **Issues**: [Report bugs](https://github.com/jagadeesh32/cello/issues)
- **Contributing**: [Contribution guide](../CONTRIBUTING.md)

## License

MIT License - see [LICENSE](../LICENSE)
