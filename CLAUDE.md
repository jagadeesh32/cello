# CLAUDE.md - Cello Framework Project Intelligence

## Project Overview

**Cello** is an ultra-fast, Rust-powered Python async web framework designed to achieve C-level performance on the hot path while maintaining Python's developer experience. It's the successor to frameworks like FastAPI, Robyn, and Litestar, combining their best features with pure Rust implementation for maximum performance.

**Version:** 0.6.0 (Beta)
**License:** MIT
**Python Requirement:** 3.12+
**Author:** Jagadeesh Katla

## Architecture Philosophy

### Core Principle: Rust Owns the Hot Path

```
Request → Rust HTTP Engine → Python Handler → Rust Response
              │                    │
              ├─ SIMD JSON         ├─ Return dict or Response
              ├─ Radix routing     └─ Python business logic only
              └─ Middleware (Rust)
```

**Key Rules:**
- Python = Developer Experience (DX) / DSL
- Rust = Runtime & Execution Engine
- Async-first design
- Zero-copy data flow
- Minimal Python involvement per request

### What Rust Owns (MUST stay in Rust)
- TCP accept loop
- HTTP parsing
- Routing (radix tree)
- All middleware
- JSON serialization (SIMD)
- Response building

### What Python Does (ONLY)
- Route registration
- Handler function pointers
- Business logic
- Returns minimal data structures

## Project Structure

```
/home/vrinda/cello/
├── src/                           # Rust source (23K+ lines, 45 files)
│   ├── lib.rs                     # PyO3 module entry point
│   ├── router.rs                  # Radix-tree routing (matchit)
│   ├── handler.rs                 # Handler registry & caching
│   ├── request/                   # HTTP request handling
│   │   ├── mod.rs                 # Request struct
│   │   ├── body.rs                # Lazy body parsing
│   │   └── multipart.rs           # Multipart form handling
│   ├── response/                  # Response types
│   │   ├── mod.rs                 # Response struct
│   │   ├── streaming.rs           # Streaming responses
│   │   └── xml.rs                 # XML responses
│   ├── middleware/                # Middleware suite (16 files)
│   │   ├── mod.rs                 # Middleware chain & traits
│   │   ├── auth.rs                # JWT, Basic, API Key auth
│   │   ├── rate_limit.rs          # Token bucket, sliding window
│   │   ├── cache.rs               # Smart caching with TTL
│   │   ├── session.rs             # Secure cookie sessions
│   │   ├── security.rs            # CSP, HSTS, security headers
│   │   ├── guards.rs              # RBAC with composable guards
│   │   ├── cors.rs                # CORS handling
│   │   ├── csrf.rs                # CSRF protection
│   │   ├── etag.rs                # ETag caching
│   │   ├── body_limit.rs          # Request size limits
│   │   ├── static_files.rs        # Static file serving
│   │   ├── request_id.rs          # UUID request tracing
│   │   ├── prometheus.rs          # Metrics collection
│   │   ├── circuit_breaker.rs     # Fault tolerance
│   │   └── exception_handler.rs   # Global error handling
│   ├── routing/                   # Route constraints
│   ├── server/                    # Server modes (cluster, TLS)
│   ├── blueprint.rs               # Flask-like route grouping
│   ├── websocket.rs               # WebSocket support
│   ├── sse.rs                     # Server-Sent Events
│   ├── json.rs                    # SIMD JSON parsing
│   ├── arena.rs                   # Arena allocators
│   ├── context.rs                 # Request context & DI
│   ├── dependency.rs              # Dependency injection
│   ├── error.rs                   # RFC 7807 errors
│   ├── lifecycle.rs               # Startup/shutdown hooks
│   ├── timeout.rs                 # Timeout config
│   ├── dto.rs                     # Data Transfer Objects
│   ├── openapi.rs                 # OpenAPI generation
│   ├── background.rs              # Background tasks
│   └── template.rs                # Jinja2 templates
│
├── python/cello/                  # Python API wrapper
│   ├── __init__.py                # Public Python API
│   ├── guards.py                  # RBAC guard classes
│   └── validation.py              # DTO validation
│
├── tests/                         # Test suite
│   ├── test_cello.py              # Main integration tests
│   └── verify_*.py                # Feature verification tests
│
├── examples/                      # 20 example applications
│   ├── hello.py                   # Basic hello world
│   ├── simple_api.py              # REST API with OpenAPI
│   ├── comprehensive_demo.py      # All v0.6.0 features
│   ├── guards.py                  # RBAC examples
│   └── ...
│
├── docs/                          # Documentation
│   ├── README.md                  # Doc index
│   ├── getting-started.md         # Installation & basics
│   ├── api-reference.md           # Complete API docs
│   └── ...
│
├── Cargo.toml                     # Rust dependencies
├── pyproject.toml                 # Python packaging
└── maturin build config
```

## Technology Stack

### Rust Dependencies (Critical)

| Component | Crate | Purpose |
|-----------|-------|---------|
| Python Bindings | `pyo3 0.20` | Python-Rust FFI (abi3-py312) |
| Async Runtime | `tokio 1.x` | Full-featured async runtime |
| HTTP Server | `hyper 1.x` | HTTP/1.1 server |
| HTTP/2 | `h2 0.4` | HTTP/2 support |
| HTTP/3 | `quinn 0.10` | QUIC protocol |
| TLS | `rustls 0.22` | TLS implementation |
| JSON | `simd-json 0.13` | SIMD-accelerated parsing |
| Serialization | `serde 1` | Rust serialization |
| Routing | `matchit 0.7` | Radix tree routing |
| Concurrency | `dashmap 5` | Lock-free HashMaps |
| Memory | `bumpalo 3` | Arena allocators |
| JWT | `jsonwebtoken 9` | JWT authentication |
| Security | `subtle 2` | Constant-time comparison |
| Metrics | `prometheus 0.13` | Prometheus metrics |
| WebSocket | `tokio-tungstenite 0.21` | WebSocket support |
| Multipart | `multer 3` | Form parsing |

## Coding Conventions

### Rust Code Style

1. **Error Handling**: Use `thiserror` for custom errors, return `Result<T, CelloError>`
2. **Async**: All I/O operations must be async using Tokio
3. **Memory**: Prefer zero-copy operations, use `Bytes` for buffers
4. **Concurrency**: Use `DashMap` for concurrent access, `parking_lot` for locks
5. **Traits**: Implement `Send + Sync` for all middleware and handlers

```rust
// Good: Async with proper error handling
pub async fn handle_request(&self, req: Request) -> Result<Response, CelloError> {
    let body = req.body().await?;
    let json: Value = simd_json::from_slice(&body)?;
    Ok(Response::json(json))
}

// Bad: Blocking I/O in async context
pub async fn bad_handler(&self, req: Request) -> Result<Response, CelloError> {
    let data = std::fs::read_to_string("file.txt")?; // BLOCKING!
    Ok(Response::text(data))
}
```

### Python Code Style

1. **Type Hints**: Always use type hints for public APIs
2. **Decorators**: Route decorators should be clean and intuitive
3. **Returns**: Handlers return `dict`, `Response`, or async equivalents

```python
# Good: Clean, typed handler
@app.get("/users/{id}")
def get_user(request: Request) -> dict:
    user_id = request.params["id"]
    return {"id": user_id, "name": "John"}

# Good: Explicit Response with status
@app.post("/users")
def create_user(request: Request) -> Response:
    data = request.json()
    return Response.json({"created": True, **data}, status=201)
```

### Middleware Pattern

All middleware must implement the `Middleware` trait:

```rust
#[async_trait]
pub trait Middleware: Send + Sync {
    async fn process(
        &self,
        request: &mut Request,
        response: &mut Response,
        context: &mut Context,
    ) -> Result<MiddlewareResult, CelloError>;

    fn priority(&self) -> i32 { 0 }
}

pub enum MiddlewareResult {
    Continue,           // Proceed to next middleware/handler
    Stop,               // Stop processing, return current response
    Error(CelloError),  // Return error response
}
```

## Building & Testing

### Development Setup

```bash
# Clone and setup
git clone https://github.com/jagadeesh32/cello.git
cd cello
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest requests

# Build Rust extensions
maturin develop

# Run tests
pytest tests/ -v

# Rust checks
cargo clippy --all-targets
cargo fmt --check
cargo test
```

### Running Examples

```bash
# Basic example
python examples/hello.py

# Full feature demo
python examples/comprehensive_demo.py

# With options
python examples/simple_api.py --port 8080 --workers 4
```

## Key Design Decisions

### 1. Why Rust for Hot Path?
- Python's GIL limits concurrency
- SIMD JSON is 10x faster than Python JSON
- Zero-copy routing eliminates allocations
- Async I/O without Python overhead

### 2. Why PyO3 with abi3?
- Single binary works across Python versions
- Minimal FFI overhead
- Native async support via `pyo3-asyncio`

### 3. Why matchit for Routing?
- O(log n) radix tree lookup
- Compile-time route optimization
- Support for path parameters and wildcards

### 4. Why DashMap over RwLock<HashMap>?
- Lock-free concurrent reads
- Fine-grained locking for writes
- Better performance under contention

## Performance Guidelines

### DO:
- Return `dict` directly (Rust handles JSON serialization)
- Use path parameters over query parameters (cached in router)
- Enable compression for responses > 1KB
- Use connection pooling for external services
- Leverage lazy body parsing

### DON'T:
- Parse JSON in Python (use `request.json()` from Rust)
- Use Python middleware on hot paths
- Block async handlers with sync I/O
- Create Response objects unnecessarily
- Hold references across await points

## Common Patterns

### Dependency Injection

```python
from cello import App, Depends

def get_db():
    return DatabaseConnection()

def get_current_user(request, db=Depends(get_db)):
    token = request.get_header("Authorization")
    return db.get_user_by_token(token)

@app.get("/profile")
def profile(request, user=Depends(get_current_user)):
    return {"user": user.name}
```

### Guards (RBAC)

```python
from cello import App
from cello.guards import RoleGuard, PermissionGuard

admin_only = RoleGuard(["admin"])
can_write = PermissionGuard(["write"])

@app.get("/admin", guards=[admin_only])
def admin_panel(request):
    return {"admin": True}

@app.post("/data", guards=[can_write])
def write_data(request):
    return {"written": True}
```

### Error Handling (RFC 7807)

```python
from cello import App, ProblemDetails

@app.exception_handler(ValueError)
def handle_value_error(request, exc):
    return ProblemDetails(
        type_url="/errors/validation",
        title="Validation Error",
        status=400,
        detail=str(exc),
        instance=request.path
    )
```

## Version History

- **v0.6.0**: Smart caching, adaptive rate limiting, DTO validation, circuit breaker
- **v0.5.0**: Dependency injection, guards (RBAC), Prometheus metrics, OpenAPI
- **v0.4.0**: JWT auth, rate limiting, sessions, security headers, cluster mode
- **v0.3.0**: WebSocket, SSE, multipart, blueprints
- **v0.2.0**: Middleware system, CORS, logging, compression
- **v0.1.0**: Initial release with basic HTTP routing

## Roadmap (Enterprise Features)

### Planned for v0.7.0+
- OpenTelemetry integration (distributed tracing)
- GraphQL support
- gRPC support
- Database connection pooling
- Health check endpoints (/health, /ready, /live)
- Service mesh integration
- Event sourcing patterns
- Message queue adapters (Kafka, RabbitMQ)
- Admin dashboard

## Troubleshooting

### Build Issues

```bash
# Missing Rust toolchain
rustup default stable

# PyO3 version mismatch
pip install --upgrade maturin
maturin develop --release

# Linker errors on Linux
sudo apt install build-essential pkg-config libssl-dev
```

### Runtime Issues

```bash
# Import errors
maturin develop  # Rebuild extensions

# Performance issues
python app.py --env production --workers $(nproc)

# Debug mode
python app.py --debug --env development
```

## Contributing Guidelines

1. **Rust Changes**: Run `cargo clippy` and `cargo fmt` before committing
2. **Python Changes**: Follow PEP 8, use type hints
3. **Tests**: Add tests for new features in `tests/`
4. **Docs**: Update relevant documentation
5. **Examples**: Add example if feature is user-facing

## Contact & Resources

- **Repository**: https://github.com/jagadeesh32/cello
- **Documentation**: See `docs/` directory
- **Issues**: GitHub Issues
- **License**: MIT
