---
title: App Reference
description: Cello App class API reference
---

# App

The `App` class is the main entry point for Cello applications.

## Constructor

```python
from cello import App

app = App(
    name: str = "cello",
    debug: bool = False,
    env: str = "development"
)
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `name` | `str` | `"cello"` | Application name (used in logging) |
| `debug` | `bool` | `False` | Enable debug mode |
| `env` | `str` | `"development"` | Environment name |

### Example

```python
from cello import App

# Basic initialization
app = App()

# With configuration
app = App(
    name="my-api",
    debug=True,
    env="development"
)
```

---

## Route Decorators

### `@app.get(path, **options)`

Register a GET route.

```python
@app.get("/users")
def list_users(request):
    return {"users": []}

@app.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}
```

### `@app.post(path, **options)`

Register a POST route.

```python
@app.post("/users")
def create_user(request):
    data = request.json()
    return Response.json({"id": 1, **data}, status=201)
```

### `@app.put(path, **options)`

Register a PUT route.

```python
@app.put("/users/{id}")
def update_user(request):
    data = request.json()
    return {"id": request.params["id"], **data}
```

### `@app.patch(path, **options)`

Register a PATCH route.

```python
@app.patch("/users/{id}")
def patch_user(request):
    data = request.json()
    return {"updated": True}
```

### `@app.delete(path, **options)`

Register a DELETE route.

```python
@app.delete("/users/{id}")
def delete_user(request):
    return Response.no_content()
```

### `@app.route(path, methods, **options)`

Register a route for multiple HTTP methods.

```python
@app.route("/resource", methods=["GET", "POST"])
def resource_handler(request):
    if request.method == "GET":
        return {"action": "list"}
    return {"action": "create"}
```

### Route Options

| Option | Type | Description |
|--------|------|-------------|
| `guards` | `list[Guard]` | Authorization guards |
| `constraints` | `dict` | Path parameter constraints |
| `tags` | `list[str]` | OpenAPI tags |
| `summary` | `str` | OpenAPI summary |
| `description` | `str` | OpenAPI description |

---

## WebSocket

### `@app.websocket(path)`

Register a WebSocket route.

```python
@app.websocket("/ws")
def websocket_handler(ws):
    ws.send_text("Connected!")
    while True:
        message = ws.recv()
        if message is None:
            break
        ws.send_text(f"Echo: {message.text}")
```

---

## Middleware

### `app.use(middleware)`

Add middleware to the application.

```python
from cello.middleware import JwtAuth, RateLimit

app.use(JwtAuth(config))
app.use(RateLimit(config))
```

### `app.enable_cors(**options)`

Enable CORS middleware.

```python
# Default settings
app.enable_cors()

# Custom configuration
app.enable_cors(
    origins=["https://example.com"],
    methods=["GET", "POST", "PUT", "DELETE"],
    allow_headers=["Authorization", "Content-Type"],
    expose_headers=["X-Request-Id"],
    allow_credentials=True,
    max_age=3600
)
```

### `app.enable_logging(**options)`

Enable request logging.

```python
app.enable_logging()
app.enable_logging(
    format="combined",
    exclude_paths=["/health"]
)
```

### `app.enable_compression(**options)`

Enable response compression.

```python
app.enable_compression()
app.enable_compression(
    min_size=1024,  # Only compress if > 1KB
    level=6         # Compression level (1-9)
)
```

### `app.enable_rate_limit(**options)`

Enable rate limiting.

```python
app.enable_rate_limit(requests=100, window=60)
```

### `app.enable_security_headers(**options)`

Enable security headers.

```python
app.enable_security_headers()
```

### `app.enable_sessions(**options)`

Enable session management.

```python
app.enable_sessions(secret=b"your-secret-key")
```

### `app.enable_csrf(**options)`

Enable CSRF protection.

```python
app.enable_csrf(secret=b"csrf-secret-key")
```

---

## Blueprints

### `app.register_blueprint(blueprint)`

Register a blueprint with the application.

```python
from cello import Blueprint

api = Blueprint("/api/v1")

@api.get("/users")
def list_users(request):
    return {"users": []}

app.register_blueprint(api)
```

---

## Lifecycle Hooks

### `@app.on_startup`

Execute code at application startup.

```python
@app.on_startup
async def startup():
    print("Application starting...")
    app.state.db = await Database.connect()
```

### `@app.on_shutdown`

Execute code at application shutdown.

```python
@app.on_shutdown
async def shutdown():
    print("Application shutting down...")
    await app.state.db.disconnect()
```

---

## Exception Handlers

### `@app.exception_handler(exception_type)`

Register a global exception handler.

```python
from cello import ProblemDetails

@app.exception_handler(ValueError)
def handle_value_error(request, exc):
    return ProblemDetails(
        type_url="/errors/validation",
        title="Validation Error",
        status=400,
        detail=str(exc)
    )

@app.exception_handler(Exception)
def handle_all_errors(request, exc):
    return Response.json(
        {"error": "Internal server error"},
        status=500
    )
```

---

## Running the Application

### `app.run(**options)`

Start the application server.

```python
app.run()

# With options
app.run(
    host="0.0.0.0",
    port=8080,
    workers=4,
    env="production",
    reload=False,
    debug=False
)
```

### Run Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `host` | `str` | `"127.0.0.1"` | Host address |
| `port` | `int` | `8000` | Port number |
| `workers` | `int` | CPU count | Worker processes |
| `env` | `str` | `"development"` | Environment |
| `reload` | `bool` | `False` | Hot reload |
| `debug` | `bool` | `False` | Debug mode |

---

## Application State

### `app.state`

Store application-level state.

```python
@app.on_startup
async def startup():
    app.state.db = await Database.connect()
    app.state.cache = await Redis.connect()

@app.get("/users")
def list_users(request):
    db = request.app.state.db
    return {"users": db.get_users()}
```

---

## Configuration

### `app.configure(**options)`

Configure multiple settings at once.

```python
app.configure(
    timeout=30,
    max_body_size=10 * 1024 * 1024,  # 10MB
    json_encoder=custom_encoder
)
```
