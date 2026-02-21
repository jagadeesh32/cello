---
title: Cello - Ultra-Fast Python Web Framework
description: Enterprise-grade, Rust-powered Python async web framework with C-level performance
hide:
  - navigation
  - toc
---

<style>
  /* Hide default title */
  .md-typeset h1 { display: none; }

  /* Hero section styling */
  .hero-section {
    text-align: center;
    padding: 2rem 1rem 1rem 1rem;
  }
  .hero-section .hero-title {
    font-size: 18px;
    font-weight: 800;
    letter-spacing: -0.02em;
    margin-bottom: 0.25rem;
    color: #BF360C;
    line-height: 1.1;
  }
  .hero-section .hero-tagline {
    font-size: 16px;
    font-weight: 700;
    margin: 0.5rem 0 0.25rem 0;
    color: #1A1A1A;
  }
  .hero-section .hero-subtitle {
    font-size: 16px;
    color: #424242;
    margin-bottom: 1.5rem;
  }
  .hero-badges {
    display: flex;
    gap: 0.5rem;
    justify-content: center;
    flex-wrap: wrap;
    margin-top: 1rem;
    margin-bottom: 0.5rem;
  }
  .hero-badges code {
    padding: 0.3em 0.8em;
    border-radius: 999px;
    font-size: 0.8rem;
    font-weight: 600;
  }
  .badge-version {
    background: #1A1A1A !important;
    color: #FF9100 !important;
    border: 1px solid #424242 !important;
  }
  .badge-tests {
    background: #1A1A1A !important;
    color: #4caf50 !important;
    border: 1px solid #424242 !important;
  }
  .badge-license {
    background: #1A1A1A !important;
    color: #42a5f5 !important;
    border: 1px solid #424242 !important;
  }
  .badge-python {
    background: #1A1A1A !important;
    color: #ab47bc !important;
    border: 1px solid #424242 !important;
  }

  /* Performance banner */
  .perf-banner {
    text-align: center;
    padding: 1.5rem;
    margin: 1rem 0;
    border-radius: 12px;
    background: linear-gradient(135deg, rgba(255, 109, 0, 0.08), rgba(255, 193, 7, 0.08));
    border: 1px solid rgba(255, 145, 0, 0.25);
  }
  .perf-number {
    font-size: 18px;
    font-weight: 900;
    color: #BF360C;
    line-height: 1.2;
    margin-bottom: 0.25rem;
  }
  .perf-label {
    font-size: 13px;
    font-weight: 600;
    color: #424242;
  }
  .perf-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 1.5rem;
    margin-top: 1.5rem;
    text-align: center;
  }
  .perf-item {
    padding: 1rem;
    border-radius: 8px;
    background: #FFFFFF;
    border: 1px solid #E0E0E0;
  }
  .perf-item .fw-name {
    font-weight: 700;
    font-size: 1rem;
    margin-bottom: 0.25rem;
    color: #1A1A1A;
  }
  .perf-item .fw-rps {
    font-size: 18px;
    font-weight: 800;
  }
  .perf-item .fw-rps.cello-rps {
    color: #BF360C;
  }
  .perf-item .fw-rps.other-rps {
    color: #424242;
  }
  .perf-item .fw-detail {
    font-size: 0.8rem;
    color: #616161;
  }

  /* Architecture section */
  .arch-explain {
    text-align: center;
    max-width: 700px;
    margin: 0 auto;
    color: #424242;
    font-size: 14px;
  }

  /* What's new banner */
  .whats-new-box {
    padding: 1.5rem 2rem;
    border-radius: 12px;
    background: linear-gradient(135deg, rgba(255, 109, 0, 0.06), rgba(156, 39, 176, 0.06));
    border: 1px solid rgba(255, 145, 0, 0.2);
    margin: 1rem 0;
  }
  .whats-new-box h3 {
    margin-top: 0;
  }

  /* Footer tagline */
  .footer-tagline {
    text-align: center;
    padding: 2rem 0 1rem 0;
    font-size: 14px;
    color: #1A1A1A;
  }
  .footer-tagline .star-btn {
    margin-top: 1rem;
  }

  /* Responsive */
  @media screen and (max-width: 768px) {
    .hero-section .hero-title { font-size: 16px; }
    .hero-section .hero-tagline { font-size: 14px; }
    .perf-grid { grid-template-columns: repeat(2, 1fr); }
    .perf-number { font-size: 16px; }
  }
  @media screen and (max-width: 480px) {
    .perf-grid { grid-template-columns: 1fr; }
  }
</style>

<!-- ===== HERO SECTION ===== -->

# Cello Framework

<div class="hero-section" markdown>

<div style="text-align: center; margin-bottom: 1rem;">
  <img src="https://cello-framework.vercel.app/logo-full.png" alt="Cello Framework" style="max-width: 320px; width: 100%; height: auto;">
</div>

<div class="hero-title">🔥 Cello Framework 🚀</div>

<div class="hero-tagline">Ultra-Fast Python Web Framework</div>

<div class="hero-subtitle">Rust-powered performance meets Python simplicity</div>

[:material-rocket-launch: Get Started](getting-started/index.md){ .md-button .md-button--primary .md-button--lg }
[:material-github: GitHub](https://github.com/jagadeesh32/cello){ .md-button }
[:material-package-variant: PyPI](https://pypi.org/project/cello-framework/){ .md-button }

<div class="hero-badges">
  <code class="badge-version">v1.0.0</code>
  <code class="badge-tests">394 tests passing</code>
  <code class="badge-license">MIT License</code>
  <code class="badge-python">Python 3.12+</code>
</div>

</div>

---

<!-- ===== PERFORMANCE BANNER ===== -->

<div class="perf-banner">
  <div class="perf-number">150,000+ req/sec</div>
  <div class="perf-label">Benchmarked on 4 cores / 8 GB RAM with wrk (12 threads, 400 connections)</div>

  <div class="perf-grid">
    <div class="perf-item">
      <div class="fw-name">🔥 Cello</div>
      <div class="fw-rps cello-rps">150,000+</div>
      <div class="fw-detail">p50 &lt;1ms &middot; p99 &lt;5ms</div>
    </div>
    <div class="perf-item">
      <div class="fw-name">Robyn</div>
      <div class="fw-rps other-rps">40,000</div>
      <div class="fw-detail">p50 ~2ms &middot; p99 ~10ms</div>
    </div>
    <div class="perf-item">
      <div class="fw-name">BlackSheep + Granian</div>
      <div class="fw-rps other-rps">30,000</div>
      <div class="fw-detail">p50 ~2ms &middot; p99 ~10ms</div>
    </div>
    <div class="perf-item">
      <div class="fw-name">FastAPI + Granian</div>
      <div class="fw-rps other-rps">25,000</div>
      <div class="fw-detail">p50 ~3ms &middot; p99 ~12ms</div>
    </div>
  </div>
</div>

---

<!-- ===== WHY CELLO ===== -->

## :material-lightning-bolt: Why Cello?

<div class="grid cards" markdown>

-   :material-lightning-bolt:{ .lg .middle } **Blazing Fast**

    ---

    The entire hot path runs in **native Rust** -- SIMD JSON parsing, radix-tree routing, and middleware execution. Python only touches your business logic.

    [:octicons-arrow-right-24: Performance benchmarks](#performance-banner)

-   :material-shield-lock:{ .lg .middle } **Enterprise Security**

    ---

    Production-grade JWT authentication, RBAC guards, CSRF protection, rate limiting with sliding windows, and security headers with **constant-time comparison**.

    [:octicons-arrow-right-24: Security features](features/security/overview.md)

-   :material-access-point-network:{ .lg .middle } **Modern Protocols**

    ---

    First-class support for **HTTP/2**, **HTTP/3** (QUIC), **WebSocket**, and **Server-Sent Events** -- all powered by Rust's async runtime.

    [:octicons-arrow-right-24: Real-time features](features/realtime/websocket.md)

-   :material-puzzle:{ .lg .middle } **Developer Experience**

    ---

    FastAPI-style **dependency injection**, Flask-like **blueprints**, auto-generated **OpenAPI/Swagger** docs, and full **type hint** support.

    [:octicons-arrow-right-24: Quick start](getting-started/quickstart.md)

-   :material-graphql:{ .lg .middle } **API Protocols**

    ---

    Native **GraphQL** with subscriptions, **gRPC** with streaming, and message queue adapters for **Kafka**, **RabbitMQ**, and **SQS**.

    [:octicons-arrow-right-24: API protocols](enterprise/integration/graphql.md)

-   :material-source-branch:{ .lg .middle } **Advanced Patterns** :material-new-box:{ .lg }

    ---

    **Event Sourcing**, **CQRS**, and **Saga** orchestration built right into the framework. Enterprise architecture, zero boilerplate. _Stable in v1.0.0_.

    [:octicons-arrow-right-24: Event Sourcing](examples/enterprise/event-sourcing.md)

</div>

---

<!-- ===== QUICK START ===== -->

## :material-rocket-launch: Quick Start

=== ":material-download: Installation"

    ```bash
    # Install from PyPI
    pip install cello-framework

    # Or with optional dependencies
    pip install cello-framework[graphql,grpc,kafka]

    # Verify installation
    python -c "import cello; print(cello.__version__)"
    ```

=== ":material-hand-wave: Hello World"

    ```python
    from cello import App

    app = App()

    @app.get("/")
    def home(request):
        return {"message": "Hello, Cello!"}

    @app.get("/users/{id}")
    def get_user(request):
        user_id = request.params["id"]
        return {"id": user_id, "name": "Alice"}

    if __name__ == "__main__":
        app.run()
    # => Cello running at http://127.0.0.1:8000
    ```

=== ":material-api: REST API"

    ```python
    from cello import App, Response, Blueprint
    from cello.guards import RoleGuard

    app = App()
    api = Blueprint("api", url_prefix="/api/v1")

    admin_only = RoleGuard(["admin"])

    @api.get("/users")
    def list_users(request):
        return {"users": [{"id": 1, "name": "Alice"}]}

    @api.post("/users", guards=[admin_only])
    def create_user(request):
        data = request.json()
        return Response.json({"created": True, **data}, status=201)

    @api.get("/users/{id}")
    def get_user(request):
        return {"id": request.params["id"]}

    app.register_blueprint(api)

    if __name__ == "__main__":
        app.run(port=8000, workers=4)
    ```

=== ":material-source-branch: Event Sourcing"

    ```python
    from cello import App
    from cello.eventsourcing import EventStore, Event, AggregateRoot

    app = App()
    store = EventStore()

    class OrderCreated(Event):
        order_id: str
        customer: str
        total: float

    class Order(AggregateRoot):
        def create(self, customer: str, total: float):
            self.apply(OrderCreated(
                order_id=self.id,
                customer=customer,
                total=total
            ))

        def on_order_created(self, event: OrderCreated):
            self.customer = event.customer
            self.total = event.total

    @app.post("/orders")
    async def create_order(request):
        data = request.json()
        order = Order()
        order.create(data["customer"], data["total"])
        await store.save(order)
        return {"order_id": order.id, "status": "created"}
    ```

---

<!-- ===== ARCHITECTURE ===== -->

## :material-hammer-wrench: Architecture

```mermaid
graph LR
    A["<b>Request</b><br/>HTTP/1.1, HTTP/2, HTTP/3"] --> B["<b>Rust HTTP Engine</b><br/>Tokio + Hyper"]
    B --> C{"<b>Radix Router</b><br/>matchit O(log n)"}
    C --> D["<b>Middleware Chain</b><br/>Auth, CORS, Rate Limit"]
    D --> E["<b>Python Handler</b><br/>Your Business Logic"]
    E --> F["<b>Rust Response Builder</b><br/>SIMD JSON Serialization"]
    F --> G["<b>Response</b><br/>Zero-Copy Output"]

    style A fill:#1A1A1A,stroke:#E65100,color:#FF9100
    style B fill:#1A1A1A,stroke:#E65100,color:#FF9100
    style C fill:#1A1A1A,stroke:#E65100,color:#FF9100
    style D fill:#1A1A1A,stroke:#E65100,color:#FF9100
    style E fill:#1A1A1A,stroke:#424242,color:#42a5f5
    style F fill:#1A1A1A,stroke:#E65100,color:#FF9100
    style G fill:#1A1A1A,stroke:#E65100,color:#FF9100
```

<div class="arch-explain" markdown>

**Rust owns the hot path.** Every TCP connection, HTTP parse, route lookup, middleware check, and JSON serialization happens in native Rust. Python is invoked _only_ for your handler function -- then Rust takes over again to build and send the response. The result: **C-level throughput** with **Python-level simplicity**.

</div>

| Component | Technology | Why It Matters |
|-----------|------------|----------------|
| **HTTP Server** | Tokio + Hyper | Async I/O with zero Python overhead |
| **JSON Parsing** | simd-json | 10x faster than Python's `json` module |
| **Routing** | matchit (radix tree) | O(log n) lookup with compile-time optimization |
| **Middleware** | Pure Rust pipeline | No GIL contention, zero-copy data flow |
| **TLS** | rustls | Memory-safe TLS without OpenSSL |
| **WebSocket** | tokio-tungstenite | Native async WebSocket in Rust |

---

<!-- ===== FEATURE MATRIX ===== -->

## :material-chart-bar: Feature Comparison

How Cello stacks up against popular Python web frameworks (4 workers, wrk 12t/400c):

| Feature | **Cello** | Robyn | BlackSheep+Granian | FastAPI+Granian |
|---------|:---------:|:-----:|:------------------:|:---------------:|
| **Requests/sec** | **150K+** | 40K | 30K | 25K |
| **Async Native** | :material-check-circle: | :material-check-circle: | :material-check-circle: | :material-check-circle: |
| **Rust Core** | :material-check-circle: | :material-check-circle: | :material-check-circle: | :material-check-circle: |
| **SIMD JSON** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **WebSocket** | :material-check-circle: | :material-check-circle: | :material-check-circle: | :material-check-circle: |
| **HTTP/2** | :material-check-circle: | :material-close-circle: | :material-check-circle: | :material-check-circle: |
| **HTTP/3 (QUIC)** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **SSE** | :material-check-circle: | :material-close-circle: | :material-check-circle: | :material-check-circle: |
| **GraphQL** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **gRPC** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **Message Queues** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **Dependency Injection** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-check-circle: |
| **RBAC Guards** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **OpenAPI Auto-Gen** | :material-check-circle: | :material-check-circle: | :material-close-circle: | :material-check-circle: |
| **Event Sourcing** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **CQRS** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |
| **Saga Pattern** | :material-check-circle: | :material-close-circle: | :material-close-circle: | :material-close-circle: |

---

<!-- ===== TECH STACK ===== -->

## :material-package-variant: Tech Stack

<div class="grid cards" markdown>

-   :material-language-rust:{ .lg .middle } **Rust Core**

    ---

    - **Tokio** -- async runtime
    - **Hyper** -- HTTP/1.1 & HTTP/2
    - **simd-json** -- hardware-accelerated JSON
    - **matchit** -- radix tree routing
    - **bumpalo** -- arena allocators

-   :material-language-python:{ .lg .middle } **Python API**

    ---

    - **PyO3** -- zero-overhead FFI
    - **abi3-py312** -- single binary for all versions
    - **Type hints** -- full IDE support
    - **Async/await** -- native coroutines
    - **Pydantic** -- optional validation

-   :material-shield-lock:{ .lg .middle } **Security**

    ---

    - **jsonwebtoken** -- JWT auth
    - **subtle** -- constant-time comparison
    - **rustls** -- memory-safe TLS
    - **hmac + sha2** -- hashing
    - **DashMap** -- lock-free concurrency

-   :material-server-network:{ .lg .middle } **Protocols**

    ---

    - **h2** -- HTTP/2 support
    - **quinn** -- HTTP/3 / QUIC
    - **tokio-tungstenite** -- WebSocket
    - **SSE** -- Server-Sent Events
    - **rustls** -- native HTTPS

-   :material-api:{ .lg .middle } **API & Messaging**

    ---

    - **async-graphql** -- GraphQL engine
    - **tonic** -- gRPC framework
    - **rdkafka** -- Apache Kafka
    - **lapin** -- RabbitMQ (AMQP)
    - **SQS** -- AWS message queue

</div>

---

<!-- ===== WHAT'S NEW ===== -->

## :material-creation: What's New in v1.0.0

<div class="whats-new-box" markdown>

!!! tip "v1.0.0 -- Production Ready"

    Cello reaches **stable release** with a complete feature set, major performance optimizations, and a semantic versioning commitment. The API is now frozen -- no breaking changes until v2.0.

    - :material-rocket-launch: **Production Stable** -- Battle-tested API with semantic versioning guarantees. Ready for enterprise deployments.

    - :material-speedometer: **Performance Optimizations** -- Handler metadata caching, lazy body parsing, zero-copy responses, TCP_NODELAY, and optimized release builds deliver peak throughput.

    - :material-feature-search: **Complete Feature Set** -- Routing, middleware, auth, WebSocket, SSE, DI, guards, caching, rate limiting, OpenTelemetry, GraphQL, gRPC, Event Sourcing, CQRS, and Saga patterns -- all in one framework.

    [:material-tag: Full Release Notes](releases/v1.0.0.md){ .md-button .md-button--primary }
    [:material-book-open-variant: Migration Guide](releases/migration.md){ .md-button }

</div>

---

<!-- ===== DOCUMENTATION SECTIONS ===== -->

## :material-book-open-variant: Documentation

<div class="grid cards" markdown>

-   :material-rocket-launch:{ .lg .middle } **Getting Started**

    ---

    Installation, quick start guide, your first application, project structure, and configuration.

    [:octicons-arrow-right-24: Get started](getting-started/index.md)

-   :material-feature-search:{ .lg .middle } **Features**

    ---

    Routing, middleware, security, real-time, dependency injection, templates, and file uploads.

    [:octicons-arrow-right-24: Explore features](features/index.md)

-   :material-school:{ .lg .middle } **Learn**

    ---

    Step-by-step tutorials for REST APIs, chat apps, auth systems, and microservices.

    [:octicons-arrow-right-24: Tutorials & guides](learn/index.md)

-   :material-book-open-page-variant:{ .lg .middle } **API Reference**

    ---

    Complete API documentation for App, Request, Response, Blueprint, Middleware, Guards, and Context.

    [:octicons-arrow-right-24: Reference docs](reference/index.md)

-   :material-code-tags:{ .lg .middle } **Examples**

    ---

    Ready-to-run examples from hello world to full-stack apps, microservices, and event sourcing.

    [:octicons-arrow-right-24: Browse examples](examples/index.md)

-   :material-office-building:{ .lg .middle } **Enterprise**

    ---

    OpenTelemetry, GraphQL, gRPC, message queues, Docker, Kubernetes, and service mesh deployment.

    [:octicons-arrow-right-24: Enterprise docs](enterprise/index.md)

</div>

---

<!-- ===== COMMUNITY ===== -->

## :material-handshake: Community & Contributing

We welcome contributions of all kinds. Whether it is a bug report, feature request, documentation improvement, or code contribution -- every bit helps.

<div class="grid cards" markdown>

-   :material-bug:{ .lg .middle } **Report Issues**

    ---

    Found a bug or have a feature request? Open an issue on GitHub and help us improve Cello.

    [:octicons-issue-opened-24: Open an Issue](https://github.com/jagadeesh32/cello/issues){ .md-button }

-   :material-source-pull:{ .lg .middle } **Submit Pull Requests**

    ---

    Want to contribute code? Check out the contributing guide, pick an issue, and submit a PR.

    [:octicons-git-pull-request-24: Contributing Guide](community/contributing.md){ .md-button }

-   :material-chat:{ .lg .middle } **Join the Community**

    ---

    Questions, ideas, or just want to chat? Join our Discord server and connect with other developers.

    [:material-discord: Join Discord](https://discord.gg/cello){ .md-button }

</div>

---

<!-- ===== FOOTER TAGLINE ===== -->

<div class="footer-tagline" markdown>

**Made with 🐍 Python and 🦀 Rust**

Designed for developers who refuse to compromise between performance and productivity.

<div class="star-btn" markdown>

[:material-star: Star on GitHub](https://github.com/jagadeesh32/cello){ .md-button .md-button--primary }
[:material-tag: Release Notes](releases/index.md){ .md-button }

</div>

</div>
