# API Reference

Complete API reference for Cello v0.4.0.

## Core Classes

### App

The main application class.

```python
from cello import App

app = App()
```

#### Methods

| Method | Description |
|--------|-------------|
| `get(path)` | Register GET route decorator |
| `post(path)` | Register POST route decorator |
| `put(path)` | Register PUT route decorator |
| `delete(path)` | Register DELETE route decorator |
| `patch(path)` | Register PATCH route decorator |
| `options(path)` | Register OPTIONS route decorator |
| `head(path)` | Register HEAD route decorator |
| `websocket(path)` | Register WebSocket route decorator |
| `route(path, methods)` | Register multi-method route decorator |
| `register_blueprint(bp)` | Register a blueprint |
| `enable_cors(origins)` | Enable CORS middleware |
| `enable_logging()` | Enable request logging |
| `enable_compression(min_size)` | Enable gzip compression |
| `run(host, port, **kwargs)` | Start the server |

---

### Blueprint

Group routes with a common prefix.

```python
from cello import Blueprint

bp = Blueprint("/api", name="api")
```

#### Constructor

```python
Blueprint(prefix: str, name: str = None)
```

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `prefix` | str | URL prefix |
| `name` | str | Blueprint name |

#### Methods

| Method | Description |
|--------|-------------|
| `get(path)` | Register GET route |
| `post(path)` | Register POST route |
| `put(path)` | Register PUT route |
| `delete(path)` | Register DELETE route |
| `patch(path)` | Register PATCH route |
| `register(blueprint)` | Register nested blueprint |
| `get_all_routes()` | Get list of (method, path, handler) tuples |

---

### Request

HTTP request object passed to handlers.

```python
def handler(request):
    method = request.method
    path = request.path
```

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `method` | str | HTTP method (GET, POST, etc.) |
| `path` | str | Request path |
| `params` | dict | Path parameters |
| `query` | dict | Query parameters |

#### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `get_param(name, default)` | str | Get path parameter |
| `get_query_param(name, default)` | str | Get query parameter |
| `get_header(name)` | str | Get header value |
| `body()` | bytes | Get raw body |
| `text()` | str | Get body as text |
| `json()` | dict | Parse JSON body |
| `form()` | dict | Parse form data |
| `is_json()` | bool | Check if JSON content type |
| `is_form()` | bool | Check if form content type |

---

### Response

HTTP response object.

```python
from cello import Response

response = Response.json({"ok": True})
```

#### Static Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `json(data, status=200)` | Response | JSON response |
| `text(text, status=200)` | Response | Plain text response |
| `html(html, status=200)` | Response | HTML response |
| `binary(data, status=200)` | Response | Binary response |
| `redirect(url, permanent=False)` | Response | Redirect response |
| `no_content()` | Response | 204 No Content |
| `file(path)` | Response | File response |

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `status` | int | HTTP status code |
| `headers` | dict | Response headers |

#### Methods

| Method | Description |
|--------|-------------|
| `set_header(name, value)` | Set response header |
| `content_type()` | Get content type |

---

### SseEvent

Server-Sent Event object.

```python
from cello import SseEvent

event = SseEvent.data("Hello")
event = SseEvent.with_event("update", '{"count": 1}')
```

#### Static Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `data(message)` | SseEvent | Create data-only event |
| `with_event(name, data)` | SseEvent | Create named event |

#### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `to_sse_string()` | str | Format as SSE string |

---

### SseStream

Container for SSE events.

```python
from cello import SseStream

stream = SseStream()
stream.add_data("Hello")
return stream
```

#### Methods

| Method | Description |
|--------|-------------|
| `add(event)` | Add SseEvent to stream |
| `add_data(message)` | Add data-only event |
| `add_event(name, data)` | Add named event |
| `len()` | Get event count |
| `is_empty()` | Check if empty |

---

### WebSocket

WebSocket connection object.

```python
@app.websocket("/ws")
def handler(ws):
    ws.send_text("Hello")
```

#### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `recv()` | WebSocketMessage | Receive message |
| `send_text(text)` | None | Send text message |
| `send_binary(data)` | None | Send binary message |
| `close()` | None | Close connection |

---

### WebSocketMessage

WebSocket message object.

```python
from cello import WebSocketMessage

msg = WebSocketMessage.text("Hello")
```

#### Static Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `text(message)` | WebSocketMessage | Create text message |
| `binary(data)` | WebSocketMessage | Create binary message |
| `close()` | WebSocketMessage | Create close message |

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `msg_type` | str | Message type |
| `text` | str | Text content (if text) |
| `data` | bytes | Binary content (if binary) |

#### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `is_text()` | bool | Check if text message |
| `is_binary()` | bool | Check if binary message |
| `is_close()` | bool | Check if close message |

---

## Configuration Classes

### TimeoutConfig

```python
TimeoutConfig(
    read_header=5,      # Seconds
    read_body=30,
    write=30,
    idle=60,
    handler=30,
)
```

### LimitsConfig

```python
LimitsConfig(
    max_header_size=8192,       # Bytes
    max_body_size=10485760,     # 10MB
    max_connections=10000,
    max_requests_per_connection=1000,
)
```

### ClusterConfig

```python
ClusterConfig(
    workers=None,               # Auto-detect
    cpu_affinity=False,
    max_restarts=5,
    graceful_shutdown=True,
    shutdown_timeout=30,
)

ClusterConfig.auto()            # Factory method
```

### TlsConfig

```python
TlsConfig(
    cert_path="/path/to/cert.pem",
    key_path="/path/to/key.pem",
    ca_path=None,
    min_version="1.2",
    max_version="1.3",
    require_client_cert=False,
)
```

### Http2Config

```python
Http2Config(
    max_concurrent_streams=100,
    initial_window_size=1048576,
    max_frame_size=16384,
    enable_push=False,
)
```

### Http3Config

```python
Http3Config(
    max_idle_timeout=30,
    max_udp_payload_size=1350,
    initial_max_streams_bidi=100,
    enable_0rtt=False,
)
```

### JwtConfig

```python
JwtConfig(
    secret="your-secret-key",
    algorithm="HS256",
    header_name="Authorization",
    cookie_name=None,
    leeway=0,
)
```

### RateLimitConfig

```python
RateLimitConfig(
    algorithm="token_bucket",
    capacity=100,
    refill_rate=10,
    window_secs=60,
    key_by="ip",
)

RateLimitConfig.token_bucket(capacity, refill_rate)
RateLimitConfig.sliding_window(max_requests, window_secs)
```

### SessionConfig

```python
SessionConfig(
    cookie_name="session_id",
    cookie_path="/",
    cookie_domain=None,
    cookie_secure=True,
    cookie_http_only=True,
    cookie_same_site="Lax",
    max_age=86400,
)
```

### SecurityHeadersConfig

```python
SecurityHeadersConfig(
    x_frame_options="DENY",
    x_content_type_options=True,
    x_xss_protection="1; mode=block",
    referrer_policy="strict-origin-when-cross-origin",
    hsts_max_age=None,
    hsts_include_subdomains=False,
    hsts_preload=False,
)

SecurityHeadersConfig.secure()  # Factory method
```

### CSP

```python
csp = CSP()
csp.default_src(["'self'"])
csp.script_src(["'self'"])
csp.style_src(["'self'"])
csp.img_src(["'self'"])
header = csp.build()
```

### StaticFilesConfig

```python
StaticFilesConfig(
    root="./static",
    prefix="/static",
    index_file="index.html",
    enable_etag=True,
    enable_last_modified=True,
    cache_control=None,
    directory_listing=False,
)
```

---

## Module Structure

```
cello/
├── App               # Main application
├── Blueprint         # Route grouping
├── Request           # Request object
├── Response          # Response object
├── WebSocket         # WebSocket connection
├── WebSocketMessage  # WebSocket message
├── SseEvent         # SSE event
├── SseStream        # SSE stream
├── FormData         # Multipart form data
├── UploadedFile     # Uploaded file
├── TimeoutConfig    # Timeout settings
├── LimitsConfig     # Limit settings
├── ClusterConfig    # Cluster settings
├── TlsConfig        # TLS settings
├── Http2Config      # HTTP/2 settings
├── Http3Config      # HTTP/3 settings
├── JwtConfig        # JWT settings
├── RateLimitConfig  # Rate limit settings
├── SessionConfig    # Session settings
├── SecurityHeadersConfig  # Security headers
├── CSP              # CSP builder
└── StaticFilesConfig     # Static files
```
