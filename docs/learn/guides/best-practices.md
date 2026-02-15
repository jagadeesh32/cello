---
title: Best Practices
description: Recommended patterns and conventions for Cello applications
---

# Best Practices

This guide collects practical recommendations for building reliable, maintainable, and performant Cello applications.

---

## Project Structure

Organize code by feature rather than by layer. A clean layout for a medium-sized project:

```
myproject/
├── app.py                # Application entry point
├── config.py             # Configuration & environment
├── blueprints/
│   ├── users.py          # User routes
│   ├── orders.py         # Order routes
│   └── admin.py          # Admin routes
├── services/
│   ├── user_service.py   # Business logic
│   └── order_service.py
├── repositories/
│   ├── user_repo.py      # Data access
│   └── order_repo.py
├── middleware/
│   └── auth.py           # Custom middleware
├── guards/
│   └── permissions.py    # Custom guards
├── tests/
│   ├── test_users.py
│   └── test_orders.py
└── requirements.txt
```

!!! tip
    Use Blueprints to map each module in `blueprints/` to a URL prefix. Register them all in `app.py`.

---

## Error Handling Patterns

### Always return structured errors

Return RFC 7807-style problem details instead of plain strings.

```python
@app.exception_handler(ValueError)
def handle_value_error(request, exc):
    return Response.json({
        "type": "/errors/validation",
        "title": "Validation Error",
        "status": 400,
        "detail": str(exc),
    }, status=400)
```

### Fail fast on invalid input

Validate early in handlers. Return `400` immediately rather than letting bad data propagate.

```python
@app.post("/users")
def create_user(request):
    data = request.json()
    if not data.get("email"):
        return Response.json({"error": "email is required"}, status=400)
    # ... proceed with valid data
```

---

## Async Best Practices

### Use async handlers for I/O

When a handler calls a database, an external API, or reads a file, use `async def`.

```python
@app.get("/users")
async def list_users(request):
    users = await db.fetch_all("SELECT * FROM users")
    return {"users": users}
```

### Never block the event loop

Avoid calling `time.sleep()`, synchronous `requests.get()`, or `open()` inside an async handler. Use their async equivalents or offload to a thread.

### Keep handlers thin

Handlers should parse input, call a service, and return the result. Business logic belongs in a service layer.

```python
@app.post("/orders")
async def create_order(request):
    data = request.json()
    order = await order_service.create(data)
    return Response.json(order, status=201)
```

---

## Security Checklist

| Area | Action |
|------|--------|
| **Secrets** | Load from environment variables, never hard-code |
| **HTTPS** | Enable `TlsConfig` in production |
| **CORS** | Restrict `origins` to your actual domains |
| **Rate limiting** | Enable on authentication and public endpoints |
| **Headers** | Enable `SecurityHeadersConfig` for CSP, HSTS, X-Frame-Options |
| **JWT** | Use short-lived access tokens (15-60 min) with refresh tokens |
| **Input** | Validate all user input; use DTO validation or Pydantic models |
| **Dependencies** | Run `pip audit` and `cargo audit` regularly |

---

## Performance Tips

### Return dicts, not Response objects

Returning a plain `dict` lets Cello serialize JSON entirely in Rust via SIMD. Creating a `Response` object is only necessary when you need a custom status code or headers.

### Use path parameters over query parameters

Path parameters are resolved during routing in the Rust radix tree and are faster to access than query strings parsed at runtime.

### Enable compression

For responses larger than 1 KB, enable gzip compression to reduce bandwidth.

```python
app.enable_compression(min_size=1024)
```

### Use caching

Apply the `@cache` decorator to expensive read-only endpoints.

```python
from cello import cache

@app.get("/reports/daily")
@cache(ttl=600, tags=["reports"])
def daily_report(request):
    return generate_report()
```

---

## Testing Strategies

### Test routes in isolation

Use `pytest` with the `requests` library against a running instance, or mock the Cello request object.

### Use fixtures for the app

Create a `conftest.py` fixture that starts the server once per session.

### Test error paths

Every handler should have tests for both the happy path and expected error conditions (400, 404, 401, etc.).

### Separate unit and integration tests

- **Unit tests**: Test services and repositories with mocked dependencies.
- **Integration tests**: Test full HTTP round-trips including middleware.

---

## Logging Standards

### Enable structured logging in production

```python
app.enable_logging()
```

Cello logs request method, path, status code, and latency automatically. Add context to your own log messages:

```python
import logging
logger = logging.getLogger(__name__)

@app.post("/orders")
def create_order(request):
    logger.info("Creating order", extra={"user_id": request.context.get("user", {}).get("sub")})
    # ...
```

### Use request IDs for tracing

Enable the request ID middleware to correlate logs across a single request.

```python
app.enable_request_id()
```

Each response will include an `X-Request-ID` header that you can use in log filters.

---

## Configuration Management

### Use environment-based configuration

```python
import os

class Config:
    DEBUG = os.environ.get("CELLO_DEBUG", "false").lower() == "true"
    DATABASE_URL = os.environ.get("DATABASE_URL", "sqlite://dev.db")
    JWT_SECRET = os.environ.get("JWT_SECRET", "change-me")
    WORKERS = int(os.environ.get("WORKERS", "4"))
```

### Separate dev and production settings

Pass `--env production` on the command line or set `CELLO_ENV=production`. In production, Cello disables debug mode and verbose logging by default.

---

## Summary

| Do | Avoid |
|----|-------|
| Return `dict` from handlers | Creating `Response` objects unnecessarily |
| Use `async def` for I/O operations | Blocking calls in async handlers |
| Validate input early | Letting bad data reach business logic |
| Use Blueprints for route organization | Defining all routes in a single file |
| Load secrets from environment | Hard-coding credentials |
| Enable security middleware | Leaving CORS open to `*` in production |
| Write tests for error paths | Testing only the happy path |
