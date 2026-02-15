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
| `processes` | `int` | `1` | Number of worker processes to fork |

Cluster mode forks multiple processes, each running its own Tokio runtime. Combined with the `workers` parameter, this provides `processes x workers` concurrent execution contexts.

```python
app.run(
    host="0.0.0.0",
    port=8000,
    workers=4,
    cluster=ClusterConfig(processes=4),
)
# Total: 4 processes x 4 threads = 16 concurrent contexts
```

---

## Timeout Configuration

```python
from cello import TimeoutConfig

timeout = TimeoutConfig(
    request_timeout=30,
    response_timeout=30,
    keep_alive_timeout=75,
)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `request_timeout` | `int` | `30` | Maximum seconds to receive the full request |
| `response_timeout` | `int` | `30` | Maximum seconds for the handler to produce a response |
| `keep_alive_timeout` | `int` | `75` | Seconds to keep idle connections open |

---

## Limits Configuration

```python
from cello import LimitsConfig

limits = LimitsConfig(
    max_connections=10000,
    max_request_size=10 * 1024 * 1024,
    max_headers=100,
)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_connections` | `int` | `10000` | Maximum concurrent connections |
| `max_request_size` | `int` | `10485760` | Maximum request body size in bytes (10 MB) |
| `max_headers` | `int` | `100` | Maximum number of request headers |

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
