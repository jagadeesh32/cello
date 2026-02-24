---
title: Configuration
description: Application configuration options in Cello Framework
---

# Configuration

Cello provides flexible configuration through the `App()` constructor, the `app.run()` method, command-line arguments, and dedicated configuration classes for features like TLS, clustering, and timeouts.

---

## App Constructor

Create the application instance:

```python
from cello import App

app = App()
```

The `App()` constructor takes no arguments. All configuration is applied through method calls and the `run()` method.

---

## Running the Server

### `app.run()` Parameters

```python
app.run(
    host="127.0.0.1",    # Bind address
    port=8000,            # Bind port
    debug=None,           # Debug mode (auto-detected from env)
    env=None,             # Environment: "development" or "production"
    workers=None,         # Worker thread count (default: CPU count)
    reload=False,         # Hot reload on file changes
    logs=None,            # Enable request logging
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `host` | `str` | `"127.0.0.1"` | Network interface to bind to |
| `port` | `int` | `8000` | TCP port to listen on |
| `debug` | `bool | None` | Auto | Enable debug mode (defaults to `True` in development) |
| `env` | `str | None` | `"development"` | Environment mode |
| `workers` | `int | None` | CPU count | Number of worker threads |
| `reload` | `bool` | `False` | Watch Python files and restart on changes |
| `logs` | `bool | None` | Auto | Enable request/response logging |

### Examples

```python
# Development (defaults)
app.run()

# Bind to all interfaces on port 3000
app.run(host="0.0.0.0", port=3000)

# Production with 8 workers
app.run(host="0.0.0.0", port=8080, env="production", workers=8)

# Development with hot reload
app.run(reload=True)

# Disable logs in development
app.run(logs=False)
```

---

## Environment Modes

Cello supports two environment modes that affect default behavior:

### Development (default)

```python
app.run(env="development")
```

- Debug mode is **enabled** by default
- Request logging is **enabled** by default
- Error responses include stack traces
- Hot reload is available

### Production

```python
app.run(env="production")
```

- Debug mode is **disabled** by default
- Request logging is **disabled** by default
- Error responses are generic (no stack traces)
- Workers default to CPU count

---

## Command-Line Arguments

When running your app directly, Cello accepts CLI arguments that override `app.run()` parameters:

```bash
python app.py --host 0.0.0.0 --port 8080 --env production --workers 4
python app.py --debug --reload
python app.py --no-logs
```

| Flag | Description |
|------|-------------|
| `--host HOST` | Bind address |
| `--port PORT` | Bind port |
| `--env ENV` | Environment mode (`development` or `production`) |
| `--debug` | Enable debug mode |
| `--reload` | Enable hot reload |
| `--workers N` | Number of worker threads |
| `--no-logs` | Disable request logging |

CLI arguments take precedence over values passed to `app.run()`.

---

## Debug Mode

When debug mode is enabled:

- Detailed error messages are returned to clients
- Stack traces appear in error responses
- Logging is more verbose

```python
# Explicitly enable debug
app.run(debug=True)

# Explicitly disable debug
app.run(debug=False)

# Auto-detect (True in development, False in production)
app.run(env="development")  # debug=True
app.run(env="production")   # debug=False
```

!!! warning "Production Security"
    Always disable debug mode in production. Debug responses can leak internal details like file paths, variable names, and stack traces.

---

## Enabling Features

Configure framework features through dedicated `enable_*` methods:

### Core Middleware

```python
app = App()

# CORS
app.enable_cors()                              # Allow all origins
app.enable_cors(["https://example.com"])       # Specific origins

# Logging
app.enable_logging()

# Compression
app.enable_compression()         # Default min_size: 1024 bytes
app.enable_compression(512)      # Custom min_size

# Rate Limiting
from cello import RateLimitConfig
app.enable_rate_limit(RateLimitConfig.token_bucket(100, 60))

# Caching
app.enable_caching(ttl=300)

# Circuit Breaker
app.enable_circuit_breaker(failure_threshold=5, reset_timeout=30)
```

### Security

```python
from cello import JwtConfig, SessionConfig, SecurityHeadersConfig

# JWT Authentication
jwt = JwtConfig(secret=b"your-secret-key-min-32-bytes-long")
app.use(JwtAuth(jwt))

# Sessions
app.enable_sessions(SessionConfig(
    secret=b"session-secret-minimum-32-bytes-long"
))

# CSRF Protection
app.enable_csrf()

# Security Headers
app.enable_security_headers()
```

### Monitoring

```python
# Prometheus metrics at /metrics
app.enable_prometheus()
app.enable_prometheus(endpoint="/metrics", namespace="myapp")

# Health checks
app.enable_health_checks()

# OpenTelemetry
from cello import OpenTelemetryConfig
app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317",
))
```

### Documentation

```python
# OpenAPI/Swagger UI
app.enable_openapi(title="My API", version="1.0.1")
# Adds: /docs (Swagger UI), /redoc (ReDoc), /openapi.json
```

---

## TLS/SSL Configuration

Enable HTTPS with the `TlsConfig` class:

```python
from cello import TlsConfig

tls = TlsConfig(
    cert_path="certs/server.crt",
    key_path="certs/server.key",
)

app.enable_tls(tls)
app.run(host="0.0.0.0", port=443)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `cert_path` | `str` | Path to the TLS certificate file (PEM) |
| `key_path` | `str` | Path to the private key file (PEM) |

Cello uses `rustls` for TLS, providing native performance without OpenSSL dependencies.

---

## Worker Configuration

### Multi-Worker Mode

Cello can run multiple worker threads to utilize all CPU cores:

```python
# Auto-detect CPU count
app.run(workers=None)

# Specific worker count
app.run(workers=4)

# Single worker (development)
app.run(workers=1)
```

### Cluster Mode

For multi-process deployment:

```python
from cello import ClusterConfig

cluster = ClusterConfig(
    workers=4,              # Number of worker processes
    graceful_timeout=30,    # Seconds to wait for in-flight requests
)

app.enable_cluster(cluster)
```

---

## Timeout Configuration

Configure request and response timeouts:

```python
from cello import TimeoutConfig

timeout = TimeoutConfig(
    request_timeout=30,     # Max seconds for full request
    response_timeout=60,    # Max seconds for response generation
    keep_alive=75,          # Keep-alive timeout in seconds
)

app.enable_timeouts(timeout)
```

---

## Request Size Limits

Control maximum request body sizes:

```python
from cello import LimitsConfig

limits = LimitsConfig(
    max_body_size=10 * 1024 * 1024,    # 10 MB
    max_header_size=8192,               # 8 KB
)

app.enable_limits(limits)
```

Or use the simpler body limit:

```python
app.enable_body_limit(10 * 1024 * 1024)  # 10 MB
```

---

## HTTP/2 and HTTP/3

### HTTP/2

```python
from cello import Http2Config

h2 = Http2Config(
    enabled=True,
    max_concurrent_streams=100,
)

app.enable_http2(h2)
```

### HTTP/3 (QUIC)

```python
from cello import Http3Config

h3 = Http3Config(
    enabled=True,
    port=443,
)

app.enable_http3(h3)
```

---

## Lifecycle Hooks

Run code at application startup and shutdown:

```python
@app.on_event("startup")
def on_startup():
    print("Application starting...")
    # Initialize database connections, caches, etc.

@app.on_event("shutdown")
def on_shutdown():
    print("Application shutting down...")
    # Close connections, flush buffers, etc.
```

---

## Static Files

```python
from cello import StaticFilesConfig

app.enable_static_files(StaticFilesConfig("/static", "./public"))
```

See [Static Files](../features/advanced/static-files.md) for full configuration options.

---

## Complete Configuration Example

```python
from cello import App, RateLimitConfig, StaticFilesConfig

app = App()

# Middleware
app.enable_cors(["https://myapp.com"])
app.enable_logging()
app.enable_compression()
app.enable_rate_limit(RateLimitConfig.token_bucket(200, 60))
app.enable_caching(ttl=300)

# Security
app.enable_security_headers()
app.enable_csrf()

# Monitoring
app.enable_prometheus()
app.enable_health_checks()

# Documentation
app.enable_openapi(title="My Production API", version="2.0.0")

# Static files
app.enable_static_files(StaticFilesConfig("/static", "./public"))

# Routes
@app.get("/")
def home(request):
    return {"status": "running"}

if __name__ == "__main__":
    app.run(
        host="0.0.0.0",
        port=8080,
        env="production",
        workers=4,
    )
```

---

## Next Steps

- [First App](first-app.md) - Build your first application
- [Project Structure](project-structure.md) - Organize your codebase
- [Middleware](../features/middleware/overview.md) - Configure middleware in detail
