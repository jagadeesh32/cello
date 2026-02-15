# Changelog

All notable changes to Cello are documented in this file.

## [1.0.0] - February 2026

### Added
- First stable release with semantic versioning commitment
- Handler metadata caching (async detection and DI params cached per handler)
- Lazy query parsing and body reading for bodyless HTTP methods
- Pre-allocated headers HashMap with known capacity
- Fast-path skip for empty middleware chains, guards, and lifecycle hooks
- Atomic `has_py_singletons` check (replaces RwLock)
- TCP_NODELAY on accepted connections
- HTTP/1.1 keep-alive and pipeline flush
- VecDeque ring buffer for O(1) latency tracking
- Zero-copy response body building
- Thread-local cached regex in OpenAPI generation
- Optimized release profile: LTO fat, panic=abort, strip, overflow-checks=false

### Fixed
- Handler introspection overhead (per-request `inspect` module import eliminated)
- O(n) latency tracking with `Vec::remove(0)` (replaced with VecDeque)
- Async middleware chain cloning entire vector per request
- DI container RwLock on every request for singleton check
- GIL acquisition for empty lifecycle hook lists
- `println!` on circuit breaker hot path (replaced with `tracing`)

---

## [0.10.0] - February 2026

### Added
- Event Sourcing with Aggregate base class and @event_handler decorator
- Event base class for typed domain events with automatic serialization
- EventStore for persisting and retrieving events with configurable backends
- EventStoreConfig for storage URL, snapshot interval, and replay settings
- Snapshot support for optimized aggregate loading at scale
- Event replay to rebuild aggregate state from the event log
- CQRS with Command and Query base classes
- CommandBus and QueryBus for dispatching operations to registered handlers
- @command_handler and @query_handler decorators for handler registration
- CqrsConfig for timeout settings and event synchronization
- Saga Pattern with Saga base class and SagaStep definitions
- SagaConfig for storage, retries, delay, and timeout settings
- SagaResult with success status, completed steps, and error details
- Automatic compensation (rollback) when any saga step fails
- Persistent saga state for crash recovery
- Event browser in development mode at /events
- Saga dashboard in development mode at /sagas

### Fixed
- GraphQL subscription disconnects under high message throughput
- gRPC reflection service not listing all methods after hot reload
- Kafka consumer group rebalancing causing duplicate message processing
- Health check endpoint returning 200 when event store is unreachable
- Improved error message when saga compensation fails with non-retryable error

---

## [0.9.0] - February 2026

### Added
- GraphQL support with Query, Mutation, Subscription decorators
- DataLoader for N+1 query prevention with batching and caching
- Schema builder with fluent API for composing GraphQL schemas
- gRPC support with GrpcService base class and @grpc_method decorator
- GrpcServer and GrpcChannel for server/client communication
- GrpcConfig with reflection, gRPC-Web, and keepalive support
- Kafka integration with @kafka_consumer and @kafka_producer decorators
- RabbitMQ integration with RabbitMQConfig
- AWS SQS integration with SqsConfig and LocalStack support
- Message class with text, json, ack, nack methods
- MessageResult constants (ACK, NACK, REJECT, REQUEUE, DEAD_LETTER)
- Producer and Consumer classes for manual message control
- GrpcError with all standard gRPC status codes

### Fixed
- Database connection pool not releasing connections on handler timeout
- Redis hgetall returning empty dict for non-existent keys
- @transactional decorator not propagating exceptions in nested async calls
- Health check endpoint returning 200 when database is down
- Memory leak in long-running WebSocket connections
- Improved error message when maturin develop is not run before import

---

## [0.8.0] - 2026-02-15

### Added

#### Features
- **Database Connection Pooling (Enhanced)**:
  - High-performance async database connections with improved pool management.
  - Support for PostgreSQL, MySQL, and SQLite.
  - Configurable pool size, max lifetime, and idle timeout.
- **Redis Integration**:
  - Async Redis client with connection pooling.
  - Pub/Sub support for real-time messaging.
  - Cluster mode and Sentinel support.
- **Transaction Support**:
  - Context-managed database transactions.
  - Automatic rollback on failure.
  - Nested transaction support via savepoints.

### Fixed
- **CORS Origins**: Fixed handling of multiple allowed origins in CORS middleware.
- **Logs Typo**: Corrected "loogs" typo in logging middleware output.
- **Response.error**: Fixed `Response.error` method to properly set error status codes.

---

## [0.6.0] - 2025-12-25

### Added

#### Features
- **Smart Caching System**: 
  - `@cache` decorator for route-specific caching.
  - TTL support and tag-based invalidation (`invalidate_cache`).
  - Async middleware implementation for high performance.
- **Intelligent Adaptive Rate Limiting**:
  - `Adaptive` algorithm that adjusts limits based on server load.
  - Monitors CPU, Memory, and Latency.
- **DTO Validation System**:
  - Pydantic integration for request payload validation.
  - Automatic 422 Unprocessable Entity responses with detailed error messages.
- **Circuit Breaker**:
  - Fault tolerance middleware to detect and isolate failing services.
  - Configurable failure threshold, reset timeout, and failure codes.
- **Lifecycle Hooks**:
  - `@app.on_event("startup")` and `@app.on_event("shutdown")` decorators.
  - Database connection management and cleanup support.

### Changed
- Refactored Middleware architecture to support fully async execution (`AsyncMiddleware`).
- Enhanced `CacheMiddleware` to support case-insensitive header checking.

---

## [0.4.0] - 2024-12-16

### Added

#### Enterprise Configuration Classes
- `TimeoutConfig` - Request/response timeout settings
- `LimitsConfig` - Connection and body size limits
- `ClusterConfig` - Multi-worker deployment configuration
- `TlsConfig` - TLS/SSL certificate configuration
- `Http2Config` - HTTP/2 protocol settings
- `Http3Config` - HTTP/3 (QUIC) protocol settings
- `JwtConfig` - JWT authentication configuration
- `RateLimitConfig` - Rate limiting with token bucket and sliding window
- `SessionConfig` - Cookie-based session management
- `SecurityHeadersConfig` - Security headers configuration
- `CSP` - Content Security Policy builder
- `StaticFilesConfig` - Static file serving configuration

#### Rust Modules
- `src/context.rs` - Request context and dependency injection container
- `src/error.rs` - RFC 7807 Problem Details error handling
- `src/lifecycle.rs` - Hooks and lifecycle events (startup, shutdown, signals)
- `src/timeout.rs` - Timeout and limits configuration
- `src/routing/` - Advanced routing with constraints (int, uuid, regex)
- `src/middleware/` - Complete middleware suite:
  - `auth.rs` - JWT, Basic, API Key authentication
  - `rate_limit.rs` - Token bucket, sliding window algorithms
  - `session.rs` - Cookie-based sessions
  - `static_files.rs` - Static file serving with caching
  - `security.rs` - CSP, HSTS, security headers
  - `body_limit.rs` - Request body size limits
  - `request_id.rs` - Unique request ID generation
  - `csrf.rs` - CSRF protection
  - `etag.rs` - ETag caching
  - `cors.rs` - CORS handling
- `src/response/` - Streaming responses, XML serialization
- `src/request/` - Lazy parsing, typed parameters, streaming multipart
- `src/server/` - Cluster mode, protocol support (TLS, HTTP/2, HTTP/3)

#### Dependencies (Cargo.toml)
- `jsonwebtoken` - JWT authentication
- `dashmap` - Concurrent HashMap for rate limiting
- `quick-xml` - XML serialization
- `quinn` - HTTP/3 (QUIC) support
- `tokio-rustls` - TLS support
- `rustls` - TLS implementation
- `tokio-util` - Cancellation tokens
- `uuid` - UUID generation
- `rand` - Random number generation
- `base64` - Base64 encoding
- `hmac`, `sha2` - HMAC and SHA2 hashing
- `regex` - Route constraints
- `h2` - HTTP/2 support

#### Documentation
- Complete `docs/` folder with guides
- API reference documentation
- Deployment guide
- Security documentation

#### Examples
- `examples/enterprise.py` - Enterprise configuration demo
- `examples/security.py` - Security features demo
- `examples/middleware_demo.py` - Middleware system demo
- `examples/cluster_demo.py` - Cluster mode demo
- `examples/streaming_demo.py` - Streaming responses demo

### Changed
- Updated to version 0.4.0
- Updated `python/cello/__init__.py` with new exports
- Updated examples to version 0.4.0

### Notes
The enterprise modules have some internal API compatibility issues that need follow-up work. These modules are structurally complete but require integration work.

---

## [0.3.0] - Previous Release

### Features
- SIMD-accelerated JSON parsing
- Middleware system (CORS, logging, compression)
- Blueprint-based routing
- WebSocket support
- Server-Sent Events (SSE)
- Multipart form handling
- Async handler support

---

## [0.2.0] - Earlier Release

### Features
- Basic HTTP routing
- Request/Response handling
- Path and query parameters
- JSON responses

---

## [0.1.0] - Initial Release

### Features
- Core HTTP server with Tokio/Hyper
- Basic routing
- PyO3 Python bindings
