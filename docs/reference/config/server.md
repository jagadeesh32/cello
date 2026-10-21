---
title: Server Configuration
description: All configuration options for the Cello application server
---

# Server Configuration

This reference covers all options for configuring the Cello application server, including the `App` constructor, `app.run()` parameters, TLS, HTTP/2, HTTP/3, and cluster mode.

---

## App Constructor

```python
from cello import App

app = App()
```

The `App()` constructor takes no arguments. All configuration is applied through method calls and the `app.run()` parameters.

---

## `app.run()` Parameters

```python
app.run(
    host="127.0.0.1",
    port=8000,
    debug=None,
    env=None,
    workers=None,
    reload=False,
    logs=None,
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `host` | `str` | `"127.0.0.1"` | IP address to bind to. Use `"0.0.0.0"` for all interfaces. |
| `port` | `int` | `8000` | TCP port to listen on |
| `debug` | `bool` | `True` in dev, `False` in prod | Enable debug mode with verbose errors |
| `env` | `str` | `"development"` | `"development"` or `"production"` |
| `workers` | `int` | CPU count | Number of Tokio worker threads |
| `reload` | `bool` | `False` | Enable hot reload (watches `.py` files) |
| `logs` | `bool` | `True` in dev | Enable request logging |

---

## Command-Line Overrides

All `app.run()` parameters can be overridden from the command line.

```bash
python app.py --host 0.0.0.0 --port 8080 --env production --workers 8 --debug --reload --no-logs
```

| Flag | Maps To |
|------|---------|
| `--host HOST` | `host` parameter |
| `--port PORT` | `port` parameter |
| `--env ENV` | `env` parameter |
| `--workers N` | `workers` parameter |
| `--debug` | `debug=True` |
| `--reload` | `reload=True` |
| `--no-logs` | `logs=False` |

Command-line arguments take precedence over values passed to `app.run()`.

---

## TLS Configuration

```python
from cello import TlsConfig

tls = TlsConfig(
    cert_path="/path/to/cert.pem",
    key_path="/path/to/key.pem",
)
```

| Field | Type | Description |
|-------|------|-------------|
| `cert_path` | `str` | Path to the TLS certificate file (PEM format) |
| `key_path` | `str` | Path to the private key file (PEM format) |

Cello uses Rustls, a modern TLS implementation written in Rust. No OpenSSL dependency is required.

---

## HTTP/2 Configuration

```python
from cello import Http2Config

http2 = Http2Config(
    enabled=True,
    max_concurrent_streams=100,
    initial_window_size=65535,
    max_frame_size=16384,
)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `True` | Enable HTTP/2 support |
| `max_concurrent_streams` | `int` | `100` | Maximum concurrent streams per connection |
| `initial_window_size` | `int` | `65535` | Initial flow-control window size |
| `max_frame_size` | `int` | `16384` | Maximum frame size in bytes |

HTTP/2 requires TLS. Enable both `TlsConfig` and `Http2Config` together.

---

## HTTP/3 Configuration

```python
from cello import Http3Config

http3 = Http3Config(
    enabled=True,
    max_idle_timeout=30,
)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | `bool` | `False` | Enable HTTP/3 (QUIC) support |
| `max_idle_timeout` | `int` | `30` | Idle timeout in seconds |

HTTP/3 uses the QUIC protocol via the `quinn` crate. It requires TLS and runs over UDP.

---

## Cluster Mode

```python
from cello import ClusterConfig

cluster = ClusterConfig(
    processes=4,
)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `processes` | `int` | `1` | Number of worker processes |

Cluster mode spawns multiple processes, each running its own Tokio runtime. Combined with the `workers` parameter, this provides `processes x workers` concurrent execution contexts.

```python
app.run(
    host="0.0.0.0",
    port=8000,
    workers=4,
    cluster=ClusterConfig(processes=4),
)
# Total: 4 processes x 4 threads = 16 concurrent contexts
```

### Platform-Specific Worker Behavior

Cello uses different strategies depending on the operating system:

| Platform | Strategy | Details |
|----------|----------|---------|
| Unix/Linux/macOS | `os.fork()` + `SO_REUSEPORT` | Workers inherit the listening socket from the parent. Best performance. |
| Windows | Subprocess re-execution | Each worker re-executes the user's script with `CELLO_WORKER=1` env var set, ensuring all routes and middleware are registered. Same pattern as Gunicorn/Uvicorn. |

The strategy is selected automatically. Application code does not need to change between platforms.

> **Reserved environment variable:** `CELLO_WORKER=1` is set internally by Cello on Windows worker subprocesses. Do not set this variable manually.

---

## Timeout Configuration

```python
from cello import TimeoutConfig

app.set_timeouts(TimeoutConfig(
    read_header=5,   # seconds to receive the request head (Slowloris guard)
    read_body=30,    # seconds to receive the body -> 408
    write=30,        # accepted, not enforced
    idle=60,         # close keep-alive connections idle this long
    handler=30,      # handler time limit -> 504
))
```

| Argument | Default | Description |
|----------|---------|-------------|
| `read_header` | `5` | Time to receive the request line and headers; also bounds the wait for the next request on a keep-alive connection |
| `read_body` | `30` | Time to receive the full body (`408` when exceeded) |
| `write` | `30` | Accepted for compatibility; not enforced |
| `idle` | `60` | Keep-alive connections with no request in flight are closed after this long |
| `handler` | `30` | Handler time limit (`504`; the pool thread is not reclaimed) |

`0` disables a timeout. Without `set_timeouts()` the server uses: header read
**30 s**, body read **30 s**, handler **30 s**, and no idle watchdog (the
header-read timeout already closes a keep-alive connection that sends nothing
for 30 s). A
handler that legitimately runs longer (a report, a long poll) needs
`set_timeouts(TimeoutConfig(handler=0))` or a larger value.

---

## Limits Configuration

```python
from cello import LimitsConfig

app.set_limits(LimitsConfig(
    max_header_size=16 * 1024,
    max_body_size=10 * 1024 * 1024,
    max_connections=10000,
    max_requests_per_connection=1000,
))
```

| Argument | Default | Description |
|----------|---------|-------------|
| `max_header_size` | `16384` | Request head size in bytes (minimum 8 KiB); larger heads are rejected with `431` |
| `max_body_size` | `10485760` | Body size in bytes; larger bodies get `413` before buffering |
| `max_connections` | `10000` | Concurrent connections; excess connections are closed on accept |
| `max_requests_per_connection` | `1000` | Responses per connection before `Connection: close` |

`0` means unlimited. Without `set_limits()` the server uses exactly these
defaults (since 1.4.1; before, the body cap was 100 MB and headers and
requests per connection were unbounded).

---

## Trusted Proxies

```python
app.set_trusted_proxies(["10.0.0.0/8", "127.0.0.1"])
```

`X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host` are honoured only
when the TCP peer is in this list (IP addresses or CIDR networks). By default
none is trusted, and `request.client_ip()`, `request.is_secure()`,
`request.host()` and the rate limiter use the socket peer and the `Host` header.

The list belongs to this `App`; another `App` in the same process keeps its
own. A change made while the server runs applies to new connections.

`X-Real-IP` carries a single address with no hop chain, so a proxy that passes
the client's copy through would let the client choose its own address. It is
ignored unless you opt in, which you should do only when every trusted proxy
overwrites it:

```python
app.set_trusted_proxies(["10.0.0.0/8"], trust_x_real_ip=True)
```

---

## Database Configuration

`app.enable_database(DatabaseConfig(...))` creates the native Postgres pool. Full
reference: [Database API](../api/database.md#databaseconfig).

```python
from cello import App, DatabaseConfig

app = App()
app.enable_database(DatabaseConfig(
    url="postgresql://app:secret@db.internal:5432/app",
    pool_size=10,
    sslmode="verify-full",
    sslrootcert="/etc/cello/db-ca.pem",
))
```

| Option | Default | Description |
|--------|---------|-------------|
| `url` | required | Postgres URL or libpq DSN. libpq query keys such as `?sslmode=...` are honoured. |
| `pool_size` | `10` | Maximum pooled connections. |
| `sslmode` | `None` (behaves as `prefer`) | `disable`, `prefer`, `require`, `verify-ca`, `verify-full`. Overrides an `sslmode` in the URL. |
| `sslrootcert` | `None` | PEM CA file to trust. When omitted or `"system"`, the bundled Mozilla roots are used. |
| `sslcert` / `sslkey` | `None` | PEM client certificate and unencrypted private key; both are required together. |

**The default `sslmode` is `prefer`**: the connection is encrypted when the server
offers TLS and plaintext otherwise, and nothing is verified. A server with TLS
enabled therefore now gets an encrypted connection where 1.3.x used plaintext.
Use `sslmode="verify-full"` in production, or `sslmode="disable"` to force
plaintext. `app.database.sslmode` reports the effective mode.

The `min_idle`, `max_lifetime_secs`, `connection_timeout_secs`,
`idle_timeout_secs` and `application_name` options are accepted but not applied
by the native Postgres pool in 1.4.0.

---

## Environment Behavior

| Setting | Development | Production |
|---------|-------------|------------|
| Debug mode | On | Off |
| Request logging | On | Off |
| Error details | Verbose (stack trace) | Generic message |
| Hot reload | Available | Disabled |

---

## Complete Example

```python
from cello import App, TlsConfig, Http2Config, ClusterConfig, TimeoutConfig

app = App()

# Middleware
app.enable_cors(origins=["https://example.com"])
app.enable_compression(min_size=1024)
app.enable_logging()

# Routes
@app.get("/")
def index(request):
    return {"status": "ok"}

# Run with full production configuration
app.run(
    host="0.0.0.0",
    port=443,
    env="production",
    workers=8,
    tls=TlsConfig(cert_path="/etc/ssl/cert.pem", key_path="/etc/ssl/key.pem"),
    http2=Http2Config(enabled=True),
    cluster=ClusterConfig(processes=4),
    timeout=TimeoutConfig(request_timeout=60),
)
```
