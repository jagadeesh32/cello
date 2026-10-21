# CLAUDE.md - Cello Framework Project Intelligence

## Project Overview

**Cello** is an ultra-fast, Rust-powered Python async web framework designed to achieve C-level performance on the hot path while maintaining Python's developer experience. It combines a modern feature set with a pure Rust implementation for maximum performance.

**Version:** 1.4.1
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
│   │   ├── exception_handler.rs   # Global error handling
│   │   └── redis.rs               # Redis integration (v0.8.0)
│   ├── routing/                   # Route constraints
│   ├── server/                    # Server modes (cluster, TLS)
│   ├── blueprint.rs               # Modular route grouping
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
│   ├── database.py                # Database & Redis wrappers (v0.8.0)
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
│   ├── comprehensive_demo.py      # All v0.7.0 features
│   ├── database_demo.py           # Database & Redis (v0.8.0)
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
- SIMD JSON is 10x faster than Python JSON (with serde_json fallback on ARM)
- Zero-copy routing eliminates allocations
- Async I/O without Python overhead
- Cross-platform: Linux (fork + SO_REUSEPORT), Windows (subprocess re-execution)

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
        type_uri="/errors/validation",
        title="Validation Error",
        status=400,
        detail=str(exc),
        instance=request.path
    )
```

## Version History

- **v1.4.1**: Review fixes — every finding in `CODE_REVIEW.md` (S1–S8, B1–B7, P1–P3) addressed. Full notes and the list of deliberate behavior changes: `docs/releases/v1.4.1.md`. Verification: full suite **1325 pass** (`pytest --asyncio-mode=auto`; new `tests/test_review_fixes.py`). `cargo test --lib` now links with `RUSTFLAGS="-C link-arg=-L<libdir> -C link-arg=-lpython3.13 -C link-arg=-Wl,-rpath,<libdir>"` (libdir from `sysconfig LIBDIR`): 362 pass, 6 pre-existing failures (also fail on the 1.4.0 tree). Perf (paired, same interpreter): control route ≈ −5% (cost = new default 30 s header-read timer; −3%/noise without it), ORM/validate within noise; overflow checks measured free. Python guard rejections now 401/403 (were 500 + exception text).
  - **Request pipeline** (`src/server/mod.rs`): unrouted owned paths (`/health*`, `/metrics`, `/graphql`) run the full chain with path owners deferred (`execute_before_deferring_owners`); genuine misses still fast-404. Early responses (stop/error/guard/cache HIT/unrouted) go through `finish_early` → `execute_after_lenient` (all sync `after` hooks) and are tagged `EARLY_RESPONSE_KEY` so the circuit breaker ignores them. `build_hyper_response` drops `Content-Length`/`Transfer-Encoding` (hyper frames `Full` bodies) and emits `Response::extra_headers` (repeatable `Set-Cookie` via `append_header`). Duplicate request headers are joined. Static/sendfile bodies are deferred (`Response::defer_file_body`) and read with `tokio::fs` in `open_deferred_body` (streamed as `FileBody` over 1 MiB).
  - **Client identity**: `src/server/proxy.rs` (`Cidr`, per-app `TrustedProxies::resolve_client`); `Request.remote_addr`/`client_addr`/`from_trusted_proxy`; `X-Forwarded-*` honoured only from trusted peers (`App.set_trusted_proxies`). `RateLimitConfig.key_by` honoured (`ip`/`user`/`api_key`/`header:<n>`); stores swept every 4096 checks.
  - **Sessions**: `request.session` (`PySession`, shared `SessionHandle` Arc between handler and `after`), lazy creation, auto-regenerate on first login key, `destroy()`; all `SessionConfig` cookie fields applied. **CSRF**: form-body tokens (urlencoded/multipart), no query tokens, session-bound synchronizer token, signed tokens must match cookie, TRACE not safe, cookie reused. **Cache**: skips `Authorization`/`Cookie` (opt-in `cache_authenticated`), no `Cache-Control` emitted, store capped. **CORS**: no credentials with wildcard, `Mirror` refuses `null`, `domain_matches` label boundary, max-age 600. **Auth**: constant-time digest compare for Basic dict/API keys, JWT `issuer`/`audience`, generic JWT error.
  - **Defaults / process**: Rust `EXPOSE_ERROR_DETAILS` defaults false; `App.run` exposes details only for explicit development / `--debug` / `CELLO_ENV` / TTY; dev defaults to 1 worker. Fork parent is a supervisor (exactly N workers); children call `_reset_after_fork` (fresh PG pool, Redis manager); `async_bridge` I/O runtime is pid-tagged so a forked child starts its own. Windows workers get `CELLO_HOST/PORT/EXPOSE_ERRORS/LOGS`. Header-read timeout 30 s default; `LimitsConfig` fully applied; `idle_timeout` watchdog. GraphQL playground/introspection off by default. `overflow-checks = true` in release.
  - **WebSocket**: real HTTP/1.1 upgrade (`websocket_upgrade` + `websocket::serve_socket`, connections served `with_upgrades()`); sync handler on the blocking pool, `ws.recv(timeout)`/`send_*`/`close`/`request`; payload via `msg.content` (`msg.text` is the constructor). HTTP/2, HTTP/3, TLS configs are documented as records only.
  - **Second review pass** (N1–N3, R1–R8; `tests/test_review_fixes_2.py`, suite **1337 pass**, Rust 374 pass + same 6 pre-existing failures). WebSocket: inbound budget 4 MiB/socket via a semaphore (reader stops reading when the handler lags), `async def` handlers run on the persistent loop (`submit_coroutine`) with awaitable `arecv`/`asend*` (blocking `recv`/`send` raise on an event-loop thread), Origin check (same-origin or `@app.websocket(path, allowed_origins=[...])`, else 403). `App.run` refuses `workers>1` with sessions/rate limiting unless `allow_per_worker_state=True` (defaulted count falls back to 1; cache only warns; `Cello.per_process_state()`). GraphQL: real parser `src/middleware/graphql_plan.rs` (`async-graphql-parser`; depth/complexity/fragment cycles/introspection/variables/@skip) used by the Rust endpoint and by Python via `_cello.graphql_plan`; `GraphQL.execute` resolves only selected root fields (it used to run every mutation); `enable_graphql(config, schema=Schema|GraphQL)` registers a POST route. Trusted proxies per app (`ServerConfig.trusted_proxies`, read per connection), `X-Real-IP` only with `trust_x_real_ip=True`. CSRF default cookie `__Host-csrf`; token bound to a session in the CSRF `after` hook (runs before session `after`). Sessions: `SessionConfig(max_sessions, max_session_size, auth_keys)`, LRU-ish batch eviction. Large file bodies (>1 MiB) streamed (`src/server/body.rs`, `ResBody = Either<Full, FileBody>`); ETag/cache/compression skip deferred files; static index re-checked against root. Defaults = `LimitsConfig()` (10 MiB body, 16 KiB head, 1000 req/conn) + 30 s body/handler timeouts; idle watchdog stays off (header-read timeout covers it). Paired bench old vs new defaults: −0.2%. Also fixed: handler-returned `Response` objects now pass through natively (`HandlerResult::Response`) — binary bodies, `sendfile`, appended `Set-Cookie` were lost and `X-Sendfile-Path` leaked.
  - **Known follow-ups**: `write_timeout` unenforced; request bodies collected before the handler; sessions/rate limits/cache have no shared (Redis) backend; GraphQL endpoint resolves root fields only (no type system); `examples/api_protocols_demo.py` still fails in its gRPC section (pre-existing); dependency refresh / `cargo audit` not done.
- **v1.4.0**: The Native Core — one model declaration compiled once into a Rust schema registry drives the ORM, validation, serialization and OpenAPI. Full notes: `docs/releases/v1.4.0.md` (design: `v1.4.0-plan.md`). Verification: full suite **1300 pass** (`pytest --asyncio-mode=auto`); perf numbers come only from `benchmarks/orm/paired.py` (paired, simultaneous servers).
  - **Speed**: prepared-statement cache (`prepare_cached`, DDL-change retry); new `src/async_bridge.rs` replaces `pyo3_asyncio::future_into_py` (single `cello-io` current-thread runtime, never takes the GIL, pipe-wakeup of the asyncio loop) and async handlers are submitted to the persistent loop directly (no parked blocking thread) — fixed the interpreter-exit "panic in a function that cannot unwind" and lost Ctrl-C; response serializer (`src/json.rs`) dispatches on concrete types first (the old extract chain raised ~4 Python exceptions per string; ~5.5× on a 200-row response); Phase 02 row pipeline — a returned queryset (`return {"users": qs}`) or `await qs.json()` / `db.fetch_json()` is encoded straight from Postgres wire rows to JSON on the I/O thread (zero per-row Python objects, byte-identical to `to_dict()`), native `to_dict()` and native hydration. Native query compiler deliberately **skipped**: query building measured <3% of request time.
  - **Security**: identifier injection in `filter/exclude/order_by/update` closed — every identifier resolves through the registry (undeclared → `FieldError`); LIKE wildcards escaped; int range checks; `write_only`/`read_only` fields; `body=Model` rejects U+0000 and ignores client-sent pk/read-only fields (no mass assignment); Postgres TLS with libpq `sslmode` (default `prefer`; `verify-full` recommended) for pool and migrations (`src/db/tls.rs`). Production-safe 500s: with `debug` off (incl. `env="production"`) an unhandled handler error returns a generic `Internal Server Error` and logs the details (global `EXPOSE_ERROR_DETAILS` in `src/server/mod.rs`, set by `App.run()` via `set_expose_errors`) — database messages carry constraint names/row values and must never reach clients. `async def` shutdown hooks now run on Ctrl-C (pending `KeyboardInterrupt` consumed via `py.check_signals()` before hooks; `tests/test_shutdown_hooks.py`).
  - **Simplicity / correctness**: Rust schema registry (`src/schema/`); value layer rewrite (`src/db/value.rs`: NUMERIC exact decimal strings, arrays, interval/inet/…, ISO/datetime/UUID binding, `DatabaseError` hierarchy in `src/db/error.rs` subclassing `RuntimeError`); ORM semantics fixes documented as deliberate breaks (defaults, create/save, FK assignment, `=None`→IS NULL, null-safe exclude, ambient transactions + `TransactionAborted`); native `body=Model` validation (`src/schema/validate.rs`, 400 `{"detail": [...]}` frozen); OpenAPI request schemas from the registry; Phase 04 Q objects, aggregates/annotate, `select_related`/`prefetch_related`/reverse accessors, `bulk_create`/`bulk_update`; Phase 05 experimental `python -m cello migrate --experimental {make,apply,status,sql}`.
  - **Tests**: ORM compatibility suite `tests/test_orm_compat_*.py` pins observable behaviour (changed pins carry a "1.4.0 deliberate break" comment); `test_orm_fastpath.py` (byte parity), `test_native_validation.py`, `test_orm_advanced.py`, `test_migrations.py`, `test_db_tls.py` (throwaway TLS cluster), `test_async_bridge.py`.
  - **Known follow-ups**: lookups across relations (`filter(author__name=…)`), `HAVING`, `annotate()` without `values()`; migrations don't handle PK/identity changes, CHECK constraints or indexes; returned querysets run after the handler returns (outside its transaction — use `await qs.json()` inside one).
- **v1.3.0**: The Speed/Simplicity/Security release — async rework (below), native data layer + ORM, a full `enable_*` plugin audit, and three-pillar upgrades. Native data layer + ORM resolves issue #5 ("how do I use the database pool and Redis client?"); the previous DB/Redis layer was **mock scaffolding** (methods returned `[]`/`None`, never connected) and is now real.
  - **Native Postgres pool** (`src/db/postgres.rs`, `PyDatabase`): backed by `deadpool-postgres` + `tokio-postgres`. `request.database` / `app.database` expose `fetch` (→`list[dict]`), `fetchrow` (→`dict|None`), `fetchval`, `execute` (→rows affected), and `transaction()`. Positional `$1` params via a `SqlParam`/`ToSql` bridge (`src/db/value.rs`) that dispatches on the target column type; rows decode common pg types incl. `jsonb`→nested Python.
  - **Native Redis client** (`src/db/redis_client.rs`, `PyRedis`): `redis` crate `aio::ConnectionManager`. `request.redis` / `app.redis` expose get/set/del/expire/incr/hget/hset/lpush/lrange/sadd/publish/eval/evalsha/script_load/ping/… (real, verified against a live server).
  - **Transactions**: `async with request.database.transaction() as tx` (explicit BEGIN/COMMIT/ROLLBACK on a held pooled connection), plus a rewritten `@transactional` decorator (async-only; injects a `tx` argument; auto commit/rollback).
  - **Built-in ORM** (`python/cello/orm.py`): `Model` + typed fields (`AutoField/IntegerField/CharField/TextField/BooleanField/FloatField/JSONField/DateTimeField/ForeignKey`), a chainable async `QuerySet` (`filter/exclude/order_by/limit/offset` with field lookups; `get/first/all/count/exists/values/create/update/delete`), `create_table`/`drop_table`. Intentionally lightweight (no migration diffing, lazy reverse relations, `select_related`, signals/admin) — documented as such.
  - **Async bridge**: all methods return `pyo3_asyncio::tokio::future_into_py` awaitables (same proven pattern as `AsyncClient`); verified to resolve on the persistent asyncio loop.
  - **Wiring**: `lib.rs` `enable_database`/`enable_redis` build real pools; `app.state` namespace added; `request.database`/`request.db`/`request.redis` injected per request. Stub `Database`/`Redis`/`Transaction` in `python/cello/database.py` removed — native classes exported from `cello`.
  - **Deps**: `deadpool-postgres`/`tokio-postgres` made non-optional (with `with-chrono-0_4`/`with-uuid-1`/`with-serde_json-1`); added `redis = 0.25`. `postgres` feature kept as an empty alias.
  - **Tests**: `tests/test_native_db.py` — live-server integration for raw queries/types, transaction commit+rollback, Redis commands, ORM CRUD/filters/FK (auto-skips when PG/Redis unreachable). Obsolete mock-based unit tests removed. Docs: `docs/data-layer.md`, `docs/issue-5-answer.md`, `examples/database_orm_demo.py`.
  - **Middleware verification + Prometheus fix**: verified the 7 core `enable_*` middleware end-to-end (cors, logging, compression, caching, rate_limit, circuit_breaker, prometheus). Six worked; **`enable_prometheus` was broken** — `/metrics` returned 404 because `handle_request` (`src/server/mod.rs`) does "route match FIRST, fast-return 404" before the prometheus middleware runs, and `/metrics` isn't a registered route. Fixed via `PrometheusMiddleware::try_serve(path)` (`src/middleware/prometheus.rs`), called in the routing-miss branch before returning 404. Tests: `tests/test_middleware.py`; docs: `docs/middleware.md`; example: `examples/middleware_full_demo.py`. Known middleware notes (documented): CORS preflight `OPTIONS` to an *unrouted* path still 404s (same fast-404 cause; register an `OPTIONS` route); a cache HIT serves the stored body uncompressed (bypasses compression); rate-limit + circuit-breaker are global.
  - **Full `enable_*` plugin audit (all 27)**: verified every plugin end-to-end over live HTTP (extends the 7-core pass above). Bugs found & fixed:
    - **Health checks & GraphQL returned 404** — identical root cause to the prometheus bug (fast-404 runs before the middleware chain; `/health*` and `/graphql` aren't registered routes). Fixed generally: added a `serves_unrouted(method, path)` hook to the `Middleware`/`AsyncMiddleware` traits (default `false`; overridden in `src/middleware/health.rs` and `graphql.rs`), plus `MiddlewareChain::has_unrouted` / `execute_before_unrouted` / `execute_before_async_unrouted` (`src/middleware/mod.rs`) and a `serve_unrouted()` helper in the router-miss branch (`src/server/mod.rs`). **Only** path-owning middleware run on a miss, so unknown paths under auth still 404 (no 404→401 route-existence leak).
    - **BasicAuth 401 lacked `WWW-Authenticate`** — `before` returned `Err`, so the server short-circuited and the `after` hook that set the header never ran. Fixed by returning `Stop(challenge())` with the header set inline (`src/middleware/auth.rs`).
    - **JWT rejected tokens without `iat`** — a standard `sub`+`exp` token failed with "missing field iat". Fixed with `#[serde(default)]` on `JwtClaims.iat`. `exp` stays required (secure default).
    - **`enable_security_headers` only accepted a `bool`** while docs/examples pass a `SecurityHeadersConfig` (TypeError). Now accepts `None` | `bool` | `SecurityHeadersConfig` via `Option<&PyAny>` + `build_security_headers_mw()` (`src/lib.rs`).
    - **`set_timeouts` panicked** — hyper 1.10 requires `builder.timer(TokioTimer::new())` when `header_read_timeout` is set ("timeout set, but no timer set"); was missing. Fixed in `src/server/mod.rs` (unblocks the 3 timeout/limit tests in `tests/test_v130_fixes.py`).
    - **Announce-only plugins** (documented, not "broken"): `enable_grpc`, `enable_messaging`, `enable_rabbitmq`, `enable_sqs`, `enable_event_sourcing`, `enable_cqrs`, `enable_saga` print config but do not wire runtime behaviour into the HTTP app — real functionality is in `cello.grpc/messaging/cqrs/saga/eventsourcing`.
    - Tests: `tests/test_plugins.py` (20 live-HTTP checks); example: `examples/plugins_demo.py`; docs: `docs/reference/api/plugins.md` (+ mkdocs nav). Full suite: **433 pass** with `pytest --asyncio-mode=auto` (`pytest-asyncio` IS required for `test_cello.py`'s `async def` tests).
  - **Three-pillar upgrades (Speed / Simplicity / Security)**:
    - **Security — full headers**: `SecurityHeadersConfig` now exposes `csp` (a `CSP` builder), `permissions_policy` (`{feature: [origins]}`), and `coep`/`coop`/`corp` (string values); `build_security_headers_mw` in `src/lib.rs` bridges them onto the Rust `SecurityHeadersMiddleware` (which already emitted them). `SecurityHeadersConfig.secure()` now includes cross-origin isolation (`require-corp` / `same-origin`).
    - **Security — Redis TLS**: added rustls features (`tls-rustls`, `tls-rustls-webpki-roots`, `tokio-rustls-comp`) to the `redis` crate; `rediss://` URLs now work (`src/db/redis_client.rs` unchanged — `Client::open` handles the scheme). Resolves the v1.4.0 "TLS for Redis" follow-up.
    - **Speed — compressed cache**: a cache HIT short-circuits the pipeline so the compression middleware never ran on it (was served uncompressed). `CacheMiddleware` now gzips the HIT inline for `Accept-Encoding: gzip` clients (`compress`/`compress_min_size` on `CacheConfig`, default on, min 1 KB), sets `Vary: Accept-Encoding`, and serves identity to non-gzip clients. `enable_caching(..., compress=True)` toggles it. Cache stores identity because async-`after` (store) runs before sync-`after` (compress).
    - **Simplicity — honest stubs**: the 7 announce-only `enable_*` methods (`grpc`, `messaging`, `rabbitmq`, `sqs`, `event_sourcing`, `cqrs`, `saga`) no longer print a misleading "enabled" banner — they emit a clear "records config only — use the `cello.X` module" note (Rust + Python docstrings). No behavior change; just honest.
    - **DX + Security — request validation**: `@app.{get,post,...}(path, body=DTO)` (App + Blueprint) parses/validates the JSON body and returns **400 `{"detail": [...]}`** before the handler runs, injecting the validated instance (`wrap_handler_with_body` in `python/cello/validation.py`). Works with Pydantic, dataclasses, and plain classes. Complements the pre-existing type-hint validation (returns 422).
    - Tests: `tests/test_upgrades.py` (11 checks). Example: `examples/three_pillars_demo.py`. Docs: `docs/reference/api/plugins.md`, `docs/features/advanced/dto-validation.md`. Full suite: **464 pass** (`pytest --asyncio-mode=auto`).
  - **Blocking-handler threadpool (Speed)**: sync `def` handlers were called **inline, under the GIL, on the single-threaded Tokio runtime** (`src/handler.rs` Phase 1), so one blocking handler (`time.sleep`, sync DB driver, `requests`) pinned both the GIL *and* the thread that accepts connections and parses HTTP — throughput collapsed to the one-at-a-time ceiling (**94 rps** measured on a 10 ms handler). Only the coroutine path escaped to `spawn_blocking`. Fixed with **adaptive offload**: sync handlers are timed *inside the GIL* (wall-clock around `Python::with_gil` also counts GIL-acquisition wait and misclassifies cheap handlers under concurrency), and after **two consecutive** calls over `offload_threshold_ms` the handler is stickily promoted to the Tokio blocking pool. Two samples are required because a handler's first call pays one-time warmup that alone exceeds the threshold. Paired benchmark (both builds running simultaneously, alternating `wrk` runs — sequential runs on this box are too noisy to compare): **94 → 3,735 rps, 39.9×**, with the trivial-handler inline path unchanged (**+0.4%**, within noise).
    - `src/handler.rs`: `HandlerMeta.offload`/`slow_streak`/`offload_policy`; Phase 1 and Phase 3 extracted to `call_handler()` / `serialize()` so the offloaded path does call+drive+serialize in **one** `spawn_blocking`; sync handlers serialize inline without awaiting `drive_and_serialize` (no extra state machine on the hot path).
    - `src/lib.rs`: `ThreadPoolConfig` (`size=64`, `offload_threshold_ms=1`, `adaptive=True`) + `App.set_threadpool()`; `.max_blocking_threads()` on the runtime builder.
    - `blocking=True|False` kwarg on every `App`/`Blueprint` verb decorator (incl. `options`/`head`/`route`), plumbed via a `__cello_blocking__` attribute read in `HandlerRegistry::register` — no Rust signature churn.
    - Caveats (documented): only GIL-releasing work benefits (CPU-bound Python still serialises — use `workers=N`); the pool is shared with async waits and background tasks; `handler_timeout` returns 504 but does not reclaim a stuck pool thread.
    - Tests: `tests/test_threadpool.py` (10 live-HTTP checks, inline-vs-pooled asserted via `threading.get_ident()`). Docs: `docs/features/advanced/threadpool.md` (+ mkdocs nav). Example: `examples/blocking_handlers_demo.py`. Full suite: **474 pass**.
  - **DI fix (pre-existing, found during the above)**: `HandlerRegistry::set_has_dependencies()` was **never called anywhere**, so `has_dependencies` stayed `false` and `Depends(...)` parameters were never resolved — handlers received the raw `Depends` marker object (`TypeError: 'Depends' object is not subscriptable`, or a 500 on serialization). `register_singleton` (`src/lib.rs`) now sets the flag. Unrelated to the threadpool work; covered by `test_dependency_injection_survives_offload`.
  - **Known follow-up**: `numeric`/`timestamptz` params need an explicit `$1::type` cast; `cargo test --lib` can't link libpython (pre-existing pyo3 `extension-module` limitation) — Python integration tests are the verification path.
  - **Async runtime rework, security hardening & DoS protection** (the async-rework portion of v1.3.0):
  - **Async runtime rework** (`src/async_loop.rs`, `handler.rs`, `lib.rs`): async `def` handlers now run on a single **persistent asyncio loop** (dedicated daemon thread) via `run_coroutine_threadsafe` instead of a fresh `asyncio.run()` per request. Loop-bound resources (aiohttp/asyncpg pools, `asyncio.Lock`/`Queue`) survive across requests, and the GIL is released while a coroutine awaits I/O (async handlers no longer serialize on the GIL). Async startup/shutdown hooks now actually run (previously the `pyo3_asyncio::into_future` path failed silently).
  - **Security**: CSRF Origin/Referer validation rewritten to exact-authority matching (`example.com.evil.com` no longer bypasses); all middleware skip/exclude paths use `path_matches_skip` (no prefix bypass); CORS adds `Vary: Origin` on reflected responses; BasicAuth uses non-short-circuit comparison (no username-timing oracle); SSE `id`/`event` strip CR/LF (no stream injection).
  - **DoS hardening**: `max_body_size` enforced (413, default 100 MB, `Limited`-capped streaming) via `App.set_limits()`; `App.set_timeouts()` wires header/body read timeouts (Slowloris) and a handler timeout (504). Previously `LimitsConfig`/`TimeoutConfig` were inert.
  - **Correctness**: large Python ints (`> i64::MAX`, up to `u64::MAX`) serialize exactly instead of becoming lossy floats; `Range` header no longer underflows on 0-byte files; DELETE/OPTIONS request bodies are read; query `+` decodes to space in keys as well as values.
  - **Python API**: `CsrfConfig` passed to `app.use()` is now honored (`cookie_name`/`header_name`/`allowed_origins`); `options()`/`head()`/`route()` apply the same validation + redis-injection wrapping as other verbs.
  - **Tests**: new Rust `#[cfg(test)]` units (CSRF authority, `path_matches_skip`, Range, SSE) + `tests/test_v130_fixes.py` integration suite.
  - **Known follow-up**: per-process Tokio runtime remains current-thread (multicore via the existing multi-process model); `AsyncClient` still uses `pyo3_asyncio::future_into_py`.
- **v1.2.4**: Critical fix — async handlers broken since v1.2.1; `pyo3_asyncio::tokio::into_future` failed silently after server startup switched to `py.allow_threads + block_on` (v1.2.1), causing all `async def` handlers to return 500; fixed by driving coroutines via `tokio::task::spawn_blocking + asyncio.run()` (`handler.rs`)
- **v1.2.3**: Full middleware Python API — `cello.middleware` module with `JwtAuth`, `BasicAuth`, `ApiKeyAuth`, `CsrfConfig`, `AdaptiveRateLimitConfig`; `app.use()` dispatcher; 6 new `enable_*` methods on App (`enable_jwt`, `enable_session`, `enable_security_headers`, `enable_csrf`, `enable_basic_auth`, `enable_api_key`); all docs import paths corrected (`from cello import RoleGuard` not `cello.guards`)
- **v1.2.2**: Security & bug fixes — CSRF `HttpOnly` on double-submit cookie (critical, broke all AJAX CSRF); all middleware `skip_path` prefix bypass fixed via `path_matches_skip()` helper; `FixedWindowStore` window_start never updated after reset; unused `mut` cleaned in minijinja tests
- **v1.2.1**: Bug fixes — server port never bound (`pyo3_asyncio` replaced with native `py.allow_threads` + `tokio::block_on`); `ProblemDetails` was missing from Python module export; `And`/`Or` guards now accept both `*args` and list styles; CSRF `HttpOnly` removed from double-submit cookie (JS must read it); `FixedWindowStore` window_start never updated after reset; all middleware `skip_path` used raw `starts_with` allowing prefix bypass; doc corrections (`type_uri` not `type_url`)
- **v1.2.0**: Bug fixes (shutdown coroutine never awaited, KeyboardInterrupt in shutdown handler, `request.redis` AttributeError); Redis Lua scripting (`eval`, `evalsha`, `script_load`); Rust-native `AsyncClient` backed by `reqwest + Tokio` — GIL never held during HTTP I/O, HTTP/2, gzip, rustls
- **v1.1.0**: MiniJinja Jinja2-compatible template engine (`MiniJinjaEngine`, `App.enable_templates()`, `App.render()`, `App.render_string()`); minijinja 2 Rust crate; HTML auto-escaping; globals; 47 new tests; 6 examples
- **v1.0.1**: Cross-platform fixes (Windows multi-worker, signal handling, UNC paths; ARM JSON fallback; Linux-only CPU affinity), async compatibility fixes (handler validation, guards, cache decorator, blueprints), guards and database exports in `__all__`
- **v1.0.0**: Production-ready stable release, performance optimizations, API stability guarantees
- **v0.10.0**: Advanced patterns (Event Sourcing, CQRS, Saga Pattern)
- **v0.9.0**: API protocols (GraphQL, gRPC), message queue adapters (Kafka, RabbitMQ)
- **v0.8.0**: Database connection pooling (enhanced), Redis integration, transaction support
- **v0.7.0**: OpenTelemetry, health checks, GraphQL support, structured logging
- **v0.6.0**: Smart caching, adaptive rate limiting, DTO validation, circuit breaker
- **v0.5.0**: Dependency injection, guards (RBAC), Prometheus metrics, OpenAPI
- **v0.4.0**: JWT auth, rate limiting, sessions, security headers, cluster mode
- **v0.3.0**: WebSocket, SSE, multipart, blueprints
- **v0.2.0**: Middleware system, CORS, logging, compression
- **v0.1.0**: Initial release with basic HTTP routing

## Roadmap (Post-1.0.1 Features)

### Planned for v1.1.0+
- OAuth2/OIDC Provider
- Service mesh integration (Istio/Envoy)
- Admin dashboard (real-time monitoring UI)
- Multi-tenancy support

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

### Cross-Platform Notes (v1.0.1)

- **Windows multi-worker**: Uses subprocess re-execution (`CELLO_WORKER=1` env var) instead of `os.fork()`
- **Windows signals**: `SIGTERM` is not available; Cello handles this gracefully with try/except
- **Windows static files**: UNC paths are normalized automatically
- **CPU affinity**: Only supported on Linux (`os.sched_setaffinity`); a warning is emitted on other platforms
- **ARM/non-SIMD**: JSON falls back to `serde_json` when SIMD instructions are unavailable

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
