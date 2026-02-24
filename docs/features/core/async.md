---
title: Async Support
description: Async and sync handler support in Cello Framework
---

# Async Support

Cello supports both synchronous (`def`) and asynchronous (`async def`) handlers. The Rust runtime (Tokio) manages the async event loop, so you get true async I/O without needing to configure an event loop in Python.

## Sync vs Async Handlers

### Sync Handlers

Use regular `def` for simple, CPU-bound operations that do not perform I/O:

```python
from cello import App

app = App()

@app.get("/hello")
def hello(request):
    return {"message": "Hello, World!"}

@app.get("/compute")
def compute(request):
    result = sum(range(1000))
    return {"result": result}
```

### Async Handlers

Use `async def` for operations that involve I/O -- database queries, HTTP calls, file reads, or any awaitable operation:

```python
import aiohttp

@app.get("/users")
async def get_users(request):
    async with aiohttp.ClientSession() as session:
        async with session.get("https://api.example.com/users") as resp:
            data = await resp.json()
    return {"users": data}

@app.get("/user/{id}")
async def get_user(request):
    user_id = request.params["id"]
    user = await database.fetch_one("SELECT * FROM users WHERE id = $1", user_id)
    return {"user": user}
```

!!! tip "When to Use Async"
    Use `async def` whenever your handler performs I/O: database queries, HTTP requests, file operations, or calls to external services. Use plain `def` for pure computation or simple dict returns.

---

## How It Works

Cello's async model is powered by Tokio, Rust's high-performance async runtime:

```
Incoming Request
    │
    ▼
Tokio Async Runtime (Rust)
    │
    ├─ sync handler  → Executed directly on worker thread
    │
    └─ async handler → Scheduled on Python asyncio event loop
                       managed by Tokio via pyo3-asyncio
```

Key points:

- **Sync handlers** run on Tokio worker threads with minimal overhead.
- **Async handlers** are dispatched to a Python asyncio event loop that Tokio manages.
- You never need to call `asyncio.run()` or create an event loop yourself.

---

## Async Lifecycle Hooks

Register startup and shutdown hooks that run async initialization or cleanup:

```python
@app.on_event("startup")
async def on_startup():
    # Initialize database connection pool
    app.db = await create_database_pool(
        "postgresql://user:pass@localhost/mydb"
    )
    print("Database pool initialized")

@app.on_event("shutdown")
async def on_shutdown():
    # Close database connections
    await app.db.close()
    print("Database pool closed")
```

Sync lifecycle hooks are also supported:

```python
@app.on_event("startup")
def on_startup():
    print("App starting up")

@app.on_event("shutdown")
def on_shutdown():
    print("App shutting down")
```

---

## Async Dependency Injection

Dependencies can be async functions, which is useful for database connections and external service clients:

```python
from cello import App, Depends

async def get_db():
    """Provide a database connection from the pool."""
    conn = await app.db.acquire()
    try:
        yield conn
    finally:
        await conn.release()

async def get_current_user(request, db=Depends(get_db)):
    """Extract and validate the current user."""
    token = request.headers.get("authorization", "").replace("Bearer ", "")
    user = await db.fetch_one("SELECT * FROM users WHERE token = $1", token)
    if not user:
        raise ValueError("Invalid token")
    return user

@app.get("/profile")
async def profile(request, user=Depends(get_current_user)):
    return {"name": user["name"], "email": user["email"]}
```

---

## Async Background Tasks

Run work after the response has been sent to the client:

```python
from cello import App, BackgroundTasks

@app.post("/orders")
async def create_order(request):
    data = request.json()
    order_id = await save_order(data)

    tasks = BackgroundTasks()
    tasks.add_task(send_confirmation_email, order_id)
    tasks.add_task(update_inventory, data["items"])

    return Response.json(
        {"order_id": order_id, "status": "created"},
        status=201
    )

async def send_confirmation_email(order_id):
    """Runs after the 201 response has been sent."""
    await email_service.send(
        template="order_confirmation",
        order_id=order_id
    )

async def update_inventory(items):
    """Runs after the 201 response has been sent."""
    for item in items:
        await inventory_service.decrement(item["sku"], item["qty"])
```

!!! note
    Background tasks run after the response is delivered. If a background task fails, the client has already received a success response. Use background tasks for non-critical operations like sending emails, logging analytics, or updating caches.

---

## Mixing Sync and Async

You can freely mix sync and async handlers in the same application:

```python
from cello import App

app = App()

# Sync -- simple computation, no I/O
@app.get("/health")
def health(request):
    return {"status": "ok"}

# Async -- database query
@app.get("/users")
async def list_users(request):
    users = await db.fetch_all("SELECT * FROM users")
    return {"users": users}

# Sync -- returns static data
@app.get("/version")
def version(request):
    return {"version": "1.0.1"}

# Async -- calls external API
@app.post("/webhooks")
async def webhook(request):
    data = request.json()
    await notify_external_service(data)
    return {"received": True}
```

---

## Async Middleware

Cello's middleware system is inherently async. All built-in middleware (CORS, rate limiting, JWT, etc.) runs asynchronously in Rust without blocking the event loop:

```python
# All middleware runs asynchronously in Rust
app.enable_cors()
app.enable_logging()
app.enable_rate_limit(RateLimitConfig.token_bucket(
    requests=100, window=60
))
```

The middleware chain is:

```
Request → [Rust Async Middleware Chain] → Python Handler → [Rust Response]
              │
              ├─ CORS (async, Rust)
              ├─ Logging (async, Rust)
              ├─ Rate Limit (async, Rust)
              ├─ Auth (async, Rust)
              └─ ... all in Rust, zero Python overhead
```

---

## Async-Compatible Decorators and Wrappers

All Python-side decorators and wrappers in Cello automatically detect whether a handler is sync or async and wrap it accordingly. You never need to worry about unawaited coroutines when combining these features with `async def` handlers:

| Decorator / Wrapper | Async Support | Notes |
|---------------------|--------------|-------|
| `@cache(ttl=...)` | Yes | Awaits the handler, then sets cache headers |
| `guards=[...]` | Yes | Runs guard checks, then awaits the handler |
| Pydantic validation | Yes | Validates the request body, then awaits the handler |

```python
from cello import App, cache
from cello.guards import RoleGuard
from pydantic import BaseModel

app = App()

class CreateItem(BaseModel):
    name: str
    price: float

# All three features combined with an async handler
@app.post("/items", guards=[RoleGuard(["editor"])])
async def create_item(request, item: CreateItem):
    result = await db.insert(item.model_dump())
    return {"id": result["id"], "name": item.name}

# @cache with an async handler
@app.get("/items")
@cache(ttl=120, tags=["items"])
async def list_items(request):
    items = await db.fetch_all("SELECT * FROM items")
    return {"items": items}
```

Each wrapper uses `inspect.iscoroutinefunction()` to choose the right strategy at decoration time, so there is zero overhead from runtime type checking on each request.

---

## Performance Considerations

| Pattern | Recommendation |
|---------|---------------|
| Simple JSON return | Use `def` -- minimal overhead |
| Database query | Use `async def` -- non-blocking I/O |
| External HTTP call | Use `async def` with `aiohttp` or `httpx` |
| File read (small) | Either works; `def` is simpler |
| File read (large) | Use `async def` with `aiofiles` |
| CPU-heavy computation | Use `def` -- avoid blocking the async loop |

!!! warning "Avoid Blocking in Async Handlers"
    Never use blocking I/O (e.g., `requests.get()`, `open().read()`, `time.sleep()`) inside an `async def` handler. This blocks the event loop and degrades performance for all concurrent requests. Use async alternatives like `aiohttp`, `aiofiles`, or `asyncio.sleep()`.

---

## Next Steps

- [Request Handling](requests.md) - Accessing request data
- [Response Types](responses.md) - Building responses
- [Middleware Overview](../middleware/overview.md) - Async middleware chain
