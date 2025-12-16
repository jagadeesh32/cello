# Changelog

All notable changes to Cello are documented in this file.

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
