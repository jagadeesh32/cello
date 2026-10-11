---
title: Changelog
description: Complete changelog for all Cello Framework releases
---

# Changelog

All notable changes to the Cello Framework are documented here. Each version links to its full release notes for detailed descriptions and code examples.

---

## v1.4.0 -- The Native Core

One model declaration, compiled once in Rust, drives the ORM, request validation, response serialization and OpenAPI.

**Speed:**
- Prepared-statement cache for every Postgres query (with automatic recovery after schema changes)
- New async bridge: database and Redis calls no longer hop through extra threads; async handlers run directly on the persistent loop
- Response serializer dispatches on concrete Python types first (~5.5x on a 200-row response)
- Row to JSON pipeline: a returned queryset (`return {"users": qs}`), `await qs.json()` and `db.fetch_json()` are encoded in Rust straight from Postgres rows, byte-identical to `to_dict()`
- Paired benchmark against 1.3.0: 200-row list 19.7x, `get` 5.4x, `create` 4.6x (see the release notes for method and caveats)

**Security:**
- ORM identifier injection closed: every identifier resolves through the schema registry (undeclared names raise `FieldError`)
- `write_only` / `read_only` model fields; request bodies reject U+0000 and ignore client-sent primary keys
- Postgres TLS with libpq-style `sslmode` (`verify-full` recommended) for the pool and migrations
- Fixed an exit-time crash and Ctrl-C being ignored under load
- Production (`debug` off) 500 responses no longer include the exception text; it is logged instead
- `async def` shutdown hooks now run when the server is stopped with Ctrl-C

**Simplicity:**
- Rust schema registry; native `body=Model` validation (400 `{"detail": [...]}` unchanged) and OpenAPI request schemas generated from the model
- Value layer rewrite: exact NUMERIC, arrays, intervals, datetime/UUID binding, `DatabaseError` hierarchy
- Q objects, aggregates / `annotate`, `select_related` / `prefetch_related`, reverse accessors, `bulk_create` / `bulk_update`
- Experimental `python -m cello migrate --experimental`

**Breaking changes:** undeclared column names raise `FieldError`; out-of-range integers raise instead of wrapping; `fetchrow` / `fetchval` return the first row; `exclude()` is null-safe; `filter(x=None)` means `IS NULL`; default Postgres `sslmode` is `prefer`. Each is listed with its migration in the release notes.

[Full release notes](v1.4.0.md)

---

## v1.3.0 -- Async Rework, Native Data Layer & Three-Pillar Upgrades

- `async def` handlers run on one persistent asyncio loop (loop-bound resources survive across requests; GIL released during I/O)
- Native Postgres pool and async Redis client (including `rediss://` TLS), plus a built-in async ORM, replacing the old mock layer
- Full audit of every `enable_*` plugin: fixed health-check / GraphQL / Prometheus 404s, BasicAuth challenge, JWT `iat`, security-headers config and a timeout panic
- Full security headers (CSP, Permissions-Policy, cross-origin isolation), gzip-compressed cache hits, declarative `body=` request validation
- Adaptive threadpool offload for blocking sync handlers (94 to 3,735 rps on a 10 ms handler)
- CSRF / skip-path / CORS / BasicAuth / SSE hardening; request body-size limits and header / body / handler timeouts

[Full release notes](v1.3.0.md)

---

## v1.2.4 -- Critical Async Fix

- Fixed every `async def` handler returning 500 since v1.2.1 (coroutines now driven correctly after the server-startup change)

[Full release notes](v1.2.4.md)

---

## v1.2.3 -- Full Middleware Python API

- `cello.middleware` module (`JwtAuth`, `BasicAuth`, `ApiKeyAuth`, `CsrfConfig`, `AdaptiveRateLimitConfig`), `app.use()` dispatcher
- Six new `enable_*` methods (`enable_jwt`, `enable_session`, `enable_security_headers`, `enable_csrf`, `enable_basic_auth`, `enable_api_key`)
- All documentation import paths corrected

[Full release notes](v1.2.3.md)

---

## v1.2.2 -- Security & Bug Fixes

- CSRF double-submit cookie `HttpOnly` fix (it broke all AJAX apps)
- Middleware `skip_path` prefix-bypass fixed via `path_matches_skip()`
- Rate limiter fixed-window start now resets correctly

[Full release notes](v1.2.2.md)

---

## v1.2.1 -- Critical Bug Fixes

- Server port was never bound; fixed
- `ProblemDetails` was missing from the Python module export
- `And` / `Or` guards accept both `*args` and list styles

[Full release notes](v1.2.1.md)

---

## v1.2.0 -- Redis Lua Scripting & Rust-Native AsyncClient

- Shutdown coroutines are now awaited; `KeyboardInterrupt` no longer leaks into shutdown output; `request.redis` no longer raises `AttributeError`
- Redis Lua scripting: `eval`, `evalsha`, `script_load`
- `AsyncClient` rewritten on `reqwest` + Tokio: GIL never held during HTTP I/O, HTTP/2, gzip, rustls

[Full release notes](v1.2.0.md)

---

## v1.1.0 -- MiniJinja Templates

- Built-in Jinja2-compatible template engine (`MiniJinjaEngine`, `app.enable_templates()`, `app.render()`, `app.render_string()`)
- HTML auto-escaping, global variables, full template syntax

[Full release notes](v1.1.0.md)

---

## v1.0.1 -- Cross-Platform & Compatibility Patch

**Cross-Platform Fixes:**
- Windows multi-worker: subprocess re-execution (`CELLO_WORKER=1`) instead of broken `multiprocessing.Process`
- Windows signal handling: `SIGTERM` wrapped in `try/except`, platform validation for signals
- Windows static files: UNC path normalization fix
- Linux-only CPU affinity: gated with warning on other platforms
- ARM JSON: `serde_json` fallback for non-SIMD architectures

**Compatibility Fixes:**
- Async handler validation: `wrap_handler_with_validation` now supports async handlers
- Async guard wrappers: `_apply_guards` creates async/sync wrappers based on handler type
- Async cache decorator: `cache()` now supports async handlers
- Blueprint validation & guards: Blueprint route decorators now support `guards` parameter and validation
- Guards exported in `__all__`: `RoleGuard`, `PermissionGuard`, `Authenticated`, `And`, `Or`, `Not`, `GuardError`, `ForbiddenError`, `UnauthorizedError`
- Database exports: `Database`, `Redis`, `Transaction` added to `__all__`

[Full release notes](v1.0.1.md)

---

## v1.0.0 -- Production Ready

- First stable release with semantic versioning guarantees
- Major performance optimizations: handler metadata caching, lazy body parsing, zero-copy responses
- TCP_NODELAY, HTTP keep-alive, and pipeline flush for lower latency
- VecDeque ring buffer for O(1) latency tracking (was O(n))
- Atomic fast-path checks for DI, guards, middleware, and lifecycle hooks
- Optimized release build: LTO fat, panic=abort, strip, overflow-checks=false
- API stability commitment: no breaking changes until v2.0
- All 394 tests passing

[Full release notes](v1.0.0.md)

---

## v0.10.0 -- Advanced Patterns

- Added Event Sourcing with Aggregate base class, event replay, and snapshots
- Added CQRS with Command/Query separation, dedicated buses, and event-driven sync
- Added Saga Pattern for distributed transaction coordination with compensation logic
- Added EventSourcingConfig, CqrsConfig, and SagaConfig configuration classes
- Fixed GraphQL subscription disconnects under high throughput
- Fixed gRPC reflection not listing methods after hot reload
- Fixed Kafka consumer group rebalancing causing duplicate processing
- Updated version references across all modules

[Full release notes](v0.10.0.md)

---

## v0.9.0 -- GraphQL, gRPC, and Messaging

- Added GraphQL support with schema-first and code-first approaches
- Added gRPC server and client with Protocol Buffers integration
- Added message queue adapters for Kafka, RabbitMQ, and AWS SQS
- Added pub/sub patterns and event-driven architecture primitives
- Added streaming RPC support for bidirectional communication
- Updated version references across all modules

[Full release notes](v0.9.0.md)

---

## v0.8.0 -- Database and Caching Infrastructure

- Added database connection pooling for PostgreSQL, MySQL, and SQLite
- Added Redis integration with connection pooling and pub/sub
- Added query builder with parameterized queries
- Added transaction management with automatic rollback
- Added database health checks and connection monitoring
- Updated version references across all modules

[Full release notes](v0.8.0.md)

---

## v0.7.0 -- Enterprise Features

- Added OpenTelemetry integration for distributed tracing
- Added health check endpoints (`/health/live`, `/health/ready`, `/health/startup`)
- Added structured logging with configurable outputs
- Improved Kubernetes deployment support with full manifest examples
- Added Docker multi-stage build documentation
- Added service mesh integration guide (Istio, Envoy)
- Updated version references across all modules

[Full release notes](v0.7.0.md)

---

## v0.6.0 -- Smart Caching and Validation

- Added `@cache` decorator with TTL and tag-based invalidation
- Added adaptive rate limiting based on server health metrics
- Added DTO validation with RFC 7807 Problem Details error responses
- Added circuit breaker middleware for fault tolerance
- Enhanced guards with controller-level and route-level support
- 15% faster JSON parsing through improved SIMD utilization
- 20% lower memory usage with optimized arena allocators
- Fixed WebSocket connection cleanup on client disconnect
- Fixed memory leak in long-running SSE connections
- Fixed race condition in concurrent session updates

[Full release notes](v0.6.0.md)

---

## v0.5.0 -- Dependency Injection and RBAC

- Added dependency injection via `Depends` with singleton and transient lifetimes
- Added composable guards: `RoleGuard`, `PermissionGuard`, `AuthenticatedGuard`
- Added guard composition with `AndGuard`, `OrGuard`, `NotGuard`
- Added built-in Prometheus metrics endpoint
- Added OpenAPI 3.0 schema generation with Swagger UI and ReDoc
- Added background tasks (fire-and-forget after response)
- Added Jinja2 template rendering via `minijinja`
- 12% faster middleware chain through pre-computed execution plans
- **Breaking:** Middleware priority values renumbered
- **Breaking:** `app.include_blueprint()` renamed to `app.register_blueprint()`
- Fixed JWT token refresh returning expired claims
- Fixed rate limiter not resetting window after full expiry

[Full release notes](v0.5.0.md)

---

## v0.4.0 -- Security and Production Readiness

- Added JWT authentication (HS256, HS384, HS512, RS256, RS384, RS512, ES256, ES384)
- Added rate limiting with token bucket and sliding window algorithms
- Added encrypted cookie session management with automatic rotation
- Added security headers middleware (CSP, HSTS, X-Frame-Options, etc.)
- Added cluster mode with multi-process workers via `SO_REUSEPORT`
- Added native TLS support via rustls (TLS 1.2 and 1.3)
- Added `--env` CLI flag for development/production mode switching
- 10% faster routing through improved radix tree traversal
- **Breaking:** Middleware now executes in registration order
- Fixed `OPTIONS` preflight not bypassing rate limiting
- Fixed WebSocket upgrade failing with compression middleware enabled

[Full release notes](v0.4.0.md)

---

## v0.3.0 -- Real-Time Communication

- Added WebSocket support via `tokio-tungstenite`
- Added Server-Sent Events (SSE) with async generators
- Added multipart form handling and file uploads via `multer`
- Added Blueprints for modular route organization with nesting
- Optimized radix tree with pre-compiled route patterns
- Zero-copy WebSocket frame passing through Rust layer
- Fixed route conflicts with overlapping path parameters
- Fixed `Content-Length` not set for empty responses

[Full release notes](v0.3.0.md)

---

## v0.2.0 -- Middleware System

- Added middleware system with composable chain execution
- Added CORS middleware with configurable origins, methods, and headers
- Added request/response logging middleware
- Added gzip and brotli compression middleware
- Added request timing and performance tracking
- Improved error handling with structured error responses

---

## v0.1.0 -- Initial Release

- Rust-powered HTTP server via hyper and tokio
- Python route registration with decorators (`@app.get`, `@app.post`, etc.)
- Radix tree routing via matchit with path parameters and wildcards
- SIMD-accelerated JSON parsing via simd-json
- Async handler support
- Static file serving
- Basic request/response API
- PyO3 abi3 bindings for Python 3.12+
- CLI with `--host`, `--port`, `--debug` flags
