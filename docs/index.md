---
title: Cello - The World's Fastest Python Web Framework
description: Enterprise-grade, Rust-powered Python async web framework with C-level performance
hide:
  - navigation
  - toc
---

<style>
  .md-typeset h1 {
    display: none;
  }
</style>

<div align="center" markdown>

# :cello: **Cello Framework**

## The World's Fastest Python Web Framework

**Rust-powered performance** meets **Python simplicity**

[:material-rocket-launch: Get Started](getting-started/index.md){ .md-button .md-button--primary }
[:material-github: GitHub](https://github.com/jagadeesh32/cello){ .md-button }
[:material-package-variant: PyPI](https://pypi.org/project/cello-framework/){ .md-button }

</div>

---

## :zap: Why Cello?

<div class="grid cards" markdown>

-   :material-clock-fast:{ .lg .middle } **Blazing Fast**

    ---

    All HTTP handling, routing, JSON serialization, and middleware execute in **native Rust**.
    Python handles only your business logic.

    [:octicons-arrow-right-24: Performance benchmarks](#benchmarks)

-   :material-security:{ .lg .middle } **Enterprise Security**

    ---

    Built-in JWT authentication, RBAC guards, CSRF protection, rate limiting,
    and security headers with **constant-time comparison**.

    [:octicons-arrow-right-24: Security features](features/security/overview.md)

-   :material-api:{ .lg .middle } **Modern APIs**

    ---

    HTTP/2, HTTP/3 (QUIC), WebSocket, Server-Sent Events, OpenAPI/Swagger
    auto-generation, and Prometheus metrics.

    [:octicons-arrow-right-24: API features](features/index.md)

-   :material-puzzle:{ .lg .middle } **Developer Experience**

    ---

    FastAPI-style dependency injection, Flask-like blueprints, Django-inspired
    templates, and automatic API documentation.

    [:octicons-arrow-right-24: Quick start](getting-started/quickstart.md)

</div>

---

## :rocket: Quick Start

=== "Installation"

    ```bash
    pip install cello-framework
    ```

=== "Hello World"

    ```python
    from cello import App, Response

    app = App()

    @app.get("/")
    def home(request):
        return {"message": "Hello, Cello!"}

    @app.get("/users/{id}")
    def get_user(request):
        return {"id": request.params["id"]}

    if __name__ == "__main__":
        app.run()
    ```

=== "Run"

    ```bash
    python app.py
    # Cello running at http://127.0.0.1:8000
    ```

---

## :building_construction: Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Request → Rust HTTP Engine → Python Handler → Rust Response    │
│                  │                    │                         │
│                  ├─ SIMD JSON         ├─ Return dict            │
│                  ├─ Radix routing     └─ Return Response        │
│                  └─ Middleware (Rust)                           │
└─────────────────────────────────────────────────────────────────┘
```

**Key Principle**: Rust owns the hot path. Python is just a DSL for your business logic.

| Component | Technology | Benefit |
|-----------|------------|---------|
| **HTTP Server** | Tokio + Hyper | Async I/O with no Python overhead |
| **JSON** | simd-json | 10x faster than Python JSON |
| **Routing** | matchit (radix tree) | O(log n) lookup, compile-time optimization |
| **Middleware** | Pure Rust | No GIL contention, zero-copy |

---

## :sparkles: Feature Highlights

<div class="grid" markdown>

=== "Core Features"

    - :material-routes: **Radix Tree Routing** - Ultra-fast route matching
    - :material-code-json: **SIMD JSON** - Hardware-accelerated parsing
    - :material-sync: **Async/Sync** - Support both handler types
    - :material-puzzle-outline: **Blueprints** - Flask-like route grouping
    - :material-electric-switch: **WebSocket** - Real-time communication
    - :material-broadcast: **SSE** - Server-Sent Events

=== "Security"

    - :material-key: **JWT Authentication** - Secure token auth
    - :material-shield-account: **RBAC Guards** - Role-based access
    - :material-lock: **CSRF Protection** - Double-submit cookies
    - :material-speedometer: **Rate Limiting** - Token bucket algorithm
    - :material-cookie-lock: **Sessions** - Secure cookie sessions
    - :material-security: **Security Headers** - CSP, HSTS, etc.

=== "Enterprise"

    - :material-injection: **Dependency Injection** - FastAPI-style DI
    - :material-chart-line: **Prometheus Metrics** - Production monitoring
    - :material-api: **OpenAPI/Swagger** - Auto-generated docs
    - :material-server-network: **Cluster Mode** - Multi-worker deployment
    - :material-shield-lock: **TLS/SSL** - Native HTTPS (rustls)
    - :material-lightning-bolt: **HTTP/2 & HTTP/3** - Modern protocols

</div>

---

## :bar_chart: Benchmarks {#benchmarks}

Cello is designed to be the **fastest Python web framework**:

| Framework | Requests/sec | Latency p50 | Latency p99 |
|-----------|-------------|-------------|-------------|
| **Cello** | 150,000+ | <1ms | <5ms |
| Robyn | 100,000 | ~2ms | ~10ms |
| FastAPI | 30,000 | ~5ms | ~20ms |
| Flask | 5,000 | ~20ms | ~100ms |

!!! note "Benchmark Conditions"
    - 4 CPU cores, 8GB RAM
    - Simple JSON endpoint returning `{"message": "Hello"}`
    - Using `wrk` with 12 threads, 400 connections

---

## :package: Tech Stack

<div class="grid cards" markdown>

-   :material-language-rust:{ .lg } **Rust Core**

    ---

    - Tokio async runtime
    - Hyper HTTP server
    - simd-json parsing
    - matchit routing

-   :material-language-python:{ .lg } **Python API**

    ---

    - PyO3 bindings
    - Type hints
    - Async/await support
    - Pydantic integration

-   :material-shield:{ .lg } **Security**

    ---

    - jsonwebtoken (JWT)
    - subtle (constant-time)
    - rustls (TLS)
    - hmac + sha2

-   :material-server:{ .lg } **Protocols**

    ---

    - HTTP/1.1 (hyper)
    - HTTP/2 (h2)
    - HTTP/3 (quinn/QUIC)
    - WebSocket (tungstenite)

</div>

---

## :people_holding_hands: Trusted By

Used in production by teams building:

- High-frequency trading APIs
- Real-time gaming backends
- IoT data pipelines
- Microservices architectures
- AI/ML model serving

---

## :book: Documentation Sections

| Section | Description |
|---------|-------------|
| [:material-rocket-launch: Getting Started](getting-started/index.md) | Installation, quick start, first app |
| [:material-feature-search: Features](features/index.md) | Complete feature documentation |
| [:material-school: Learn](learn/index.md) | Tutorials, guides, patterns |
| [:material-book-open-page-variant: Reference](reference/index.md) | API reference, configuration |
| [:material-code-tags: Examples](examples/index.md) | Code examples and use cases |
| [:material-office-building: Enterprise](enterprise/index.md) | Enterprise features and deployment |
| [:material-tag: Release Notes](releases/index.md) | Version history and changelog |

---

## :handshake: Contributing

We welcome contributions! See our [Contributing Guide](community/contributing.md) for details.

<div class="grid cards" markdown>

-   :material-bug:{ .lg .middle } **Report Issues**

    ---

    Found a bug? Open an issue on GitHub.

    [:octicons-issue-opened-24: Open Issue](https://github.com/jagadeesh32/cello/issues)

-   :material-source-pull:{ .lg .middle } **Submit PRs**

    ---

    Want to contribute code? We'd love your help!

    [:octicons-git-pull-request-24: Pull Requests](https://github.com/jagadeesh32/cello/pulls)

-   :material-chat:{ .lg .middle } **Join Community**

    ---

    Questions? Join our Discord community.

    [:material-discord: Discord](https://discord.gg/cello)

</div>

---

<div align="center" markdown>

**Made with :heart: using :snake: Python and :crab: Rust**

[:material-star: Star on GitHub](https://github.com/jagadeesh32/cello){ .md-button }

</div>
