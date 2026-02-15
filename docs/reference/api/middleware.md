---
title: Middleware API
description: Built-in middleware, configuration, and the middleware chain in Cello
---

# Middleware API

Cello's middleware system runs entirely in Rust for maximum performance. Middleware intercepts requests before they reach handlers and responses before they are sent to clients. All built-in middleware is enabled via `app.enable_*()` methods.

---

## Middleware Chain

Middleware executes in a defined order based on priority. The chain processes requests from highest to lowest priority, then processes responses in reverse order.

```
Request --> [Security Headers] --> [CORS] --> [Rate Limit] --> [Auth] --> Handler
Response <-- [Security Headers] <-- [CORS] <-- [Compression] <-- [Logging] <-- Handler
```

---

## Built-in Middleware

### CORS

Enable Cross-Origin Resource Sharing.

```python
app.enable_cors(origins=["https://app.example.com"])
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `origins` | `list[str]` | `["*"]` | Allowed origin domains |

---

### Logging

Log every request with method, path, status code, and latency.

```python
app.enable_logging()
```

No configuration parameters. Automatically disabled when `--no-logs` is passed.

---

### Compression

Gzip-compress responses above a size threshold.

```python
app.enable_compression(min_size=1024)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `min_size` | `int` | `1024` | Minimum response size in bytes to compress |

---

### Rate Limiting

Limit the number of requests per client.

```python
from cello import RateLimitConfig

# Token bucket algorithm
app.enable_rate_limit(RateLimitConfig.token_bucket(
    requests=100,
    window=60,
))

# Sliding window algorithm
app.enable_rate_limit(RateLimitConfig.sliding_window(
    requests=100,
    window=60,
))
```

Returns `429 Too Many Requests` when the limit is exceeded, with a `Retry-After` header.

---

### JWT Authentication

Validate JWT tokens from the `Authorization` header.

```python
from cello import JwtConfig

app.enable_jwt(JwtConfig(
    secret="your-secret-key",
    algorithm="HS256",
    expiration=3600,
    issuer="my-app",
    header_name="Authorization",
    header_prefix="Bearer",
))
```

Populates `request.context["user"]` with the decoded token payload.

---

### Session Management

Cookie-based session management.

```python
from cello import SessionConfig

app.enable_session(SessionConfig(
    secret="session-secret",
    cookie_name="session_id",
    max_age=86400,
    httponly=True,
    secure=True,
))
```

Session data is available at `request.context["session"]`.

---

### Security Headers

Add security headers to every response.

```python
from cello import SecurityHeadersConfig

app.enable_security_headers(SecurityHeadersConfig(
    hsts=True,
    hsts_max_age=31536000,
    x_frame_options="DENY",
    x_content_type_options=True,
    x_xss_protection=True,
))
```

---

### CSRF Protection

Protect against Cross-Site Request Forgery.

```python
app.enable_csrf()
```

Generates and validates CSRF tokens for state-changing requests (POST, PUT, DELETE, PATCH).

---

### Static Files

Serve static files from a directory.

```python
from cello import StaticFilesConfig

app.enable_static_files(StaticFilesConfig(
    directory="./static",
    prefix="/static",
))
```

---

### Prometheus Metrics

Expose Prometheus-compatible metrics.

```python
app.enable_prometheus(
    endpoint="/metrics",
    namespace="cello",
    subsystem="http",
)
```

---

### Request ID

Add a unique ID to each request for tracing.

```python
app.enable_request_id()
```

Sets `request.context["request_id"]` and adds `X-Request-ID` to the response.

---

### Caching

Cache GET and HEAD responses.

```python
app.enable_caching(ttl=300, methods=["GET", "HEAD"], exclude_paths=["/health"])
```

---

### Circuit Breaker

Protect against cascading failures from downstream services.

```python
app.enable_circuit_breaker(
    failure_threshold=5,
    reset_timeout=30,
    half_open_target=3,
    failure_codes=[500, 502, 503, 504],
)
```

---

### Body Limit

Restrict maximum request body size.

```python
app.enable_body_limit(max_size=10 * 1024 * 1024)  # 10 MB
```

---

### ETag

Automatic ETag generation for response caching.

```python
app.enable_etag()
```

---

## Middleware Configuration Classes

Each middleware has a corresponding configuration class. See the [Middleware Configuration Reference](../config/middleware.md) for all fields and defaults.

---

## Middleware Execution Order

Middleware runs in priority order. Built-in middleware priorities (lower number = runs first):

| Priority | Middleware |
|----------|-----------|
| 1 | Request ID |
| 2 | Security Headers |
| 3 | CORS |
| 4 | Body Limit |
| 5 | Rate Limiting |
| 6 | CSRF |
| 7 | Authentication (JWT) |
| 8 | Session |
| 9 | Caching |
| 10 | Compression |
| 11 | Logging |
| 12 | Prometheus |

---

## Summary

| Middleware | Method | Purpose |
|-----------|--------|---------|
| CORS | `enable_cors()` | Cross-origin requests |
| Logging | `enable_logging()` | Request/response logging |
| Compression | `enable_compression()` | Gzip responses |
| Rate Limit | `enable_rate_limit()` | Throttle clients |
| JWT | `enable_jwt()` | Token authentication |
| Session | `enable_session()` | Cookie sessions |
| Security | `enable_security_headers()` | HSTS, CSP, etc. |
| CSRF | `enable_csrf()` | CSRF protection |
| Static Files | `enable_static_files()` | Serve files |
| Prometheus | `enable_prometheus()` | Metrics endpoint |
| Request ID | `enable_request_id()` | Trace IDs |
| Caching | `enable_caching()` | Response cache |
| Circuit Breaker | `enable_circuit_breaker()` | Fault tolerance |
| Body Limit | `enable_body_limit()` | Size restriction |
| ETag | `enable_etag()` | Conditional requests |
