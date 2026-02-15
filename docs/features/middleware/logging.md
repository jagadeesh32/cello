---
title: Logging Middleware
description: Request and response logging in Cello Framework
---

# Logging Middleware

Cello provides built-in request logging implemented in Rust. The logging middleware records HTTP method, path, status code, and response time for every request.

## Quick Start

```python
from cello import App

app = App()

# Enable request logging
app.enable_logging()
```

When enabled, each request produces a log line:

```
[2025-01-15 10:23:45] GET /api/users → 200 (2.3ms)
[2025-01-15 10:23:46] POST /api/users → 201 (5.1ms)
[2025-01-15 10:23:47] GET /api/users/999 → 404 (0.8ms)
```

---

## Automatic Logging in Development

When you run your application with `app.run()`, logging is enabled automatically in development mode:

```python
app = App()

# Logging enabled automatically (env defaults to "development")
app.run()

# Explicit development mode
app.run(env="development")

# Disable auto-logging in development
app.run(logs=False)

# Production mode -- no auto-logging
app.run(env="production")
```

| Environment | Auto-Logging |
|------------|--------------|
| `development` | Enabled |
| `production` | Disabled |

You can always explicitly enable logging regardless of environment:

```python
app.enable_logging()
app.run(env="production")  # Logging still active
```

---

## Log Format

Each log entry contains:

| Field | Description | Example |
|-------|-------------|---------|
| Timestamp | ISO 8601 format | `2025-01-15 10:23:45` |
| Method | HTTP method | `GET`, `POST`, `DELETE` |
| Path | Request path | `/api/users/123` |
| Status | Response status code | `200`, `404`, `500` |
| Duration | Processing time | `2.3ms` |

### Example Output

```
[2025-01-15 10:23:45] GET    /                → 200 (0.1ms)
[2025-01-15 10:23:45] GET    /api/users       → 200 (3.2ms)
[2025-01-15 10:23:46] POST   /api/users       → 201 (5.7ms)
[2025-01-15 10:23:46] GET    /api/users/1     → 200 (1.1ms)
[2025-01-15 10:23:47] DELETE /api/users/1     → 204 (2.0ms)
[2025-01-15 10:23:48] GET    /api/users/999   → 404 (0.5ms)
[2025-01-15 10:23:49] POST   /api/login       → 401 (1.8ms)
```

---

## Status Code Colors

In terminal output, status codes are color-coded for quick visual identification:

| Status Range | Color | Meaning |
|-------------|-------|---------|
| 2xx | Green | Success |
| 3xx | Cyan | Redirect |
| 4xx | Yellow | Client error |
| 5xx | Red | Server error |

---

## Filtering

### Exclude Paths

Common paths like health checks and metrics are often excluded from logs to reduce noise:

```python
# These paths are typically excluded by the logging middleware:
# /health, /metrics, /favicon.ico
```

!!! tip "Reducing Log Noise"
    If your application has a health check endpoint that is polled frequently (e.g., by Kubernetes), the default logging middleware keeps logs clean by focusing on application routes.

---

## Performance

The logging middleware is implemented in Rust with asynchronous I/O:

| Operation | Overhead |
|-----------|----------|
| Log entry creation | ~100ns |
| Async write to stdout | ~200ns |
| Total per request | ~300ns |

The middleware uses buffered, non-blocking writes so logging does not slow down request processing.

---

## Combining with Other Middleware

Place the logging middleware after authentication but before compression so that logs include authenticated user context:

```python
app = App()

# Recommended order
app.enable_cors()
app.use(JwtAuth(jwt_config))   # Auth runs first
app.enable_logging()            # Log with auth context
app.enable_compression()        # Compress after logging
```

---

## CLI Options

Control logging from the command line:

```bash
# Enable logging (default in development)
python app.py

# Disable logging
python app.py --no-logs

# Debug mode (always enables logging)
python app.py --debug
```

---

## Next Steps

- [Middleware Overview](overview.md) - Full middleware system
- [Compression](compression.md) - Response compression
- [Rate Limiting](rate-limiting.md) - Request throttling
