# Configuration

This document covers all configuration options available in Cello v0.4.0.

## Server Configuration

### CLI Arguments

```bash
python app.py [options]

Options:
  --host HOST           Host to bind to (default: 127.0.0.1)
  --port PORT           Port to bind to (default: 8000)
  --env ENV             Environment: development or production
  --workers N           Number of worker threads
  --reload              Enable hot reload (development only)
  --debug               Enable debug logging
  --no-logs             Disable request logging
```

### Code Configuration

```python
app.run(
    host="127.0.0.1",     # Host to bind
    port=8000,            # Port to bind
    workers=4,            # Worker threads
    env="production",     # Environment mode
    debug=False,          # Debug mode
    reload=False,         # Hot reload
)
```

## Enterprise Configuration Classes

All configuration classes are available from the main `cello` module:

```python
from cello import (
    TimeoutConfig,
    LimitsConfig,
    ClusterConfig,
    TlsConfig,
    Http2Config,
    Http3Config,
    JwtConfig,
    RateLimitConfig,
    SessionConfig,
    SecurityHeadersConfig,
    CSP,
    StaticFilesConfig,
)
```

---

## TimeoutConfig

Controls request/response timeouts.

```python
from cello import TimeoutConfig

config = TimeoutConfig(
    read_header=5,      # Seconds to read headers (default: 5)
    read_body=30,       # Seconds to read body (default: 30)
    write=30,           # Seconds to write response (default: 30)
    idle=60,            # Idle connection timeout (default: 60)
    handler=30,         # Handler execution timeout (default: 30)
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `read_header` | 5 | Time to read request headers |
| `read_body` | 30 | Time to read request body |
| `write` | 30 | Time to write response |
| `idle` | 60 | Idle connection timeout |
| `handler` | 30 | Handler execution timeout |

---

## LimitsConfig

Controls connection and request limits.

```python
from cello import LimitsConfig

config = LimitsConfig(
    max_header_size=8192,           # 8KB (default)
    max_body_size=10485760,         # 10MB (default)
    max_connections=10000,          # Default
    max_requests_per_connection=1000,  # Default
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_header_size` | 8192 | Maximum request header size in bytes |
| `max_body_size` | 10485760 | Maximum request body size in bytes (10MB) |
| `max_connections` | 10000 | Maximum concurrent connections |
| `max_requests_per_connection` | 1000 | Max requests per keep-alive connection |

---

## ClusterConfig

Controls multi-worker deployment.

```python
from cello import ClusterConfig

# Manual configuration
config = ClusterConfig(
    workers=4,                  # Number of workers
    cpu_affinity=False,         # Pin workers to CPU cores
    max_restarts=5,             # Max worker restarts
    graceful_shutdown=True,     # Enable graceful shutdown
    shutdown_timeout=30,        # Shutdown grace period
)

# Auto-detect configuration
config = ClusterConfig.auto()
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `workers` | CPU count | Number of worker processes |
| `cpu_affinity` | False | Pin workers to CPU cores |
| `max_restarts` | 5 | Max worker restart attempts |
| `graceful_shutdown` | True | Enable graceful shutdown |
| `shutdown_timeout` | 30 | Seconds to wait for graceful shutdown |

---

## TlsConfig

Controls TLS/SSL settings.

```python
from cello import TlsConfig

config = TlsConfig(
    cert_path="/path/to/cert.pem",   # Required
    key_path="/path/to/key.pem",     # Required
    ca_path=None,                    # CA certificate (optional)
    min_version="1.2",               # Minimum TLS version
    max_version="1.3",               # Maximum TLS version
    require_client_cert=False,       # Require client certificate
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `cert_path` | Required | Path to certificate file |
| `key_path` | Required | Path to private key file |
| `ca_path` | None | Path to CA certificate |
| `min_version` | "1.2" | Minimum TLS version |
| `max_version` | "1.3" | Maximum TLS version |
| `require_client_cert` | False | Require client certificate |

---

## Http2Config

Controls HTTP/2 settings.

```python
from cello import Http2Config

config = Http2Config(
    max_concurrent_streams=100,     # Default
    initial_window_size=1048576,    # 1MB (default)
    max_frame_size=16384,           # Default
    enable_push=False,              # Server push (default: disabled)
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_concurrent_streams` | 100 | Max concurrent streams per connection |
| `initial_window_size` | 1048576 | Initial flow control window (1MB) |
| `max_frame_size` | 16384 | Maximum frame size |
| `enable_push` | False | Enable server push |

---

## Http3Config

Controls HTTP/3 (QUIC) settings.

```python
from cello import Http3Config

config = Http3Config(
    max_idle_timeout=30,            # Seconds (default)
    max_udp_payload_size=1350,      # Bytes (default)
    initial_max_streams_bidi=100,   # Default
    enable_0rtt=False,              # 0-RTT resumption (default: disabled)
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_idle_timeout` | 30 | Max idle timeout in seconds |
| `max_udp_payload_size` | 1350 | Max UDP payload size |
| `initial_max_streams_bidi` | 100 | Initial max bidirectional streams |
| `enable_0rtt` | False | Enable 0-RTT resumption |

---

## JwtConfig

Controls JWT authentication settings.

```python
from cello import JwtConfig

config = JwtConfig(
    secret="your-secret-key",       # Required
    algorithm="HS256",              # Default
    header_name="Authorization",    # Default
    cookie_name=None,               # Also check cookies
    leeway=0,                       # Clock skew tolerance
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `secret` | Required | JWT secret key |
| `algorithm` | "HS256" | JWT algorithm (HS256, HS384, HS512, RS256, etc.) |
| `header_name` | "Authorization" | Header to check for token |
| `cookie_name` | None | Cookie to check for token |
| `leeway` | 0 | Clock skew tolerance in seconds |

---

## RateLimitConfig

Controls rate limiting.

```python
from cello import RateLimitConfig

# Token bucket algorithm
config = RateLimitConfig.token_bucket(
    capacity=100,       # Max tokens
    refill_rate=10,     # Tokens per second
)

# Sliding window algorithm
config = RateLimitConfig.sliding_window(
    max_requests=100,   # Max requests in window
    window_secs=60,     # Window duration
)

# Custom configuration
config = RateLimitConfig(
    algorithm="token_bucket",
    capacity=100,
    refill_rate=10,
    window_secs=60,
    key_by="ip",        # "ip", "user", or "api_key"
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `algorithm` | "token_bucket" | Algorithm: token_bucket or sliding_window |
| `capacity` | 100 | Max capacity |
| `refill_rate` | 10 | Refill rate (token bucket) |
| `window_secs` | 60 | Window duration (sliding window) |
| `key_by` | "ip" | Rate limit key |

---

## SessionConfig

Controls cookie-based session settings.

```python
from cello import SessionConfig

config = SessionConfig(
    cookie_name="session_id",       # Default
    cookie_path="/",                # Default
    cookie_domain=None,             # Current domain
    cookie_secure=True,             # Require HTTPS
    cookie_http_only=True,          # No JavaScript access
    cookie_same_site="Lax",         # SameSite policy
    max_age=86400,                  # 24 hours (default)
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `cookie_name` | "session_id" | Session cookie name |
| `cookie_path` | "/" | Cookie path |
| `cookie_domain` | None | Cookie domain |
| `cookie_secure` | True | Require HTTPS |
| `cookie_http_only` | True | Not accessible via JavaScript |
| `cookie_same_site` | "Lax" | SameSite policy |
| `max_age` | 86400 | Session duration in seconds |

---

## SecurityHeadersConfig

Controls security headers.

```python
from cello import SecurityHeadersConfig

# Custom configuration
config = SecurityHeadersConfig(
    x_frame_options="DENY",
    x_content_type_options=True,
    x_xss_protection="1; mode=block",
    referrer_policy="strict-origin-when-cross-origin",
    hsts_max_age=31536000,  # 1 year
    hsts_include_subdomains=True,
    hsts_preload=False,
)

# Pre-configured secure defaults
config = SecurityHeadersConfig.secure()
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `x_frame_options` | "DENY" | X-Frame-Options header |
| `x_content_type_options` | True | X-Content-Type-Options: nosniff |
| `x_xss_protection` | "1; mode=block" | X-XSS-Protection header |
| `referrer_policy` | "strict-origin-when-cross-origin" | Referrer-Policy header |
| `hsts_max_age` | None | HSTS max-age in seconds |
| `hsts_include_subdomains` | False | Include subdomains in HSTS |
| `hsts_preload` | False | HSTS preload flag |

---

## CSP (Content Security Policy)

Build Content Security Policy headers.

```python
from cello import CSP

csp = CSP()
csp.default_src(["'self'"])
csp.script_src(["'self'", "https://cdn.example.com"])
csp.style_src(["'self'", "'unsafe-inline'"])
csp.img_src(["'self'", "data:", "https:"])

header_value = csp.build()
# "default-src 'self'; script-src 'self' https://cdn.example.com; ..."
```

Available directives:
- `default_src(sources)`
- `script_src(sources)`
- `style_src(sources)`
- `img_src(sources)`

---

## StaticFilesConfig

Controls static file serving.

```python
from cello import StaticFilesConfig

config = StaticFilesConfig(
    root="./static",            # Root directory
    prefix="/static",           # URL prefix (default)
    index_file="index.html",    # Index file (default)
    enable_etag=True,           # ETag caching (default)
    enable_last_modified=True,  # Last-Modified header (default)
    cache_control=None,         # Custom Cache-Control
    directory_listing=False,    # Disable directory listing (default)
)
```

| Parameter | Default | Description |
|-----------|---------|-------------|
| `root` | Required | Static files root directory |
| `prefix` | "/static" | URL prefix for static files |
| `index_file` | "index.html" | Default index file |
| `enable_etag` | True | Enable ETag caching |
| `enable_last_modified` | True | Enable Last-Modified header |
| `cache_control` | None | Custom Cache-Control header |
| `directory_listing` | False | Enable directory listing |
