---
title: Context API
description: Request context for storing and accessing per-request data in Cello
---

# Context API

The request context provides a per-request key-value store that passes data between middleware, guards, and handlers. It is the primary mechanism for middleware to communicate with downstream handlers.

---

## Overview

Every incoming request carries a `context` dictionary. Middleware can write to it (e.g., storing the authenticated user), and handlers can read from it.

```
Request --> Auth Middleware (writes context["user"]) --> Handler (reads context["user"])
```

---

## Accessing Context in Handlers

The context is available as a property on the `Request` object.

```python
from cello import App

app = App()

@app.get("/me")
def profile(request):
    user = request.context.get("user")
    if not user:
        return Response.json({"error": "Not authenticated"}, status=401)
    return {"name": user["name"], "email": user["email"]}
```

---

## Context Properties

### `request.context`

| Property | Type | Description |
|----------|------|-------------|
| `request.context` | `dict` | Mutable dictionary scoped to the current request |

The context dictionary supports all standard `dict` operations:

```python
# Read a value
user = request.context.get("user")

# Read with default
role = request.context.get("role", "anonymous")

# Check existence
if "user" in request.context:
    ...

# Iterate
for key, value in request.context.items():
    ...
```

---

## Writing to Context from Middleware

Middleware writes to `context` by modifying it during request processing. The Rust middleware chain passes the context through each layer.

### JWT Middleware

When JWT authentication is enabled, the JWT middleware automatically populates `context["user"]` with the decoded token payload.

```python
from cello import App, JwtConfig

app = App()
app.enable_jwt(JwtConfig(secret="my-secret", algorithm="HS256", expiration=3600))

@app.get("/me")
def profile(request):
    # context["user"] is set by JWT middleware
    user = request.context.get("user", {})
    return {
        "sub": user.get("sub"),
        "roles": user.get("roles", []),
    }
```

### Session Middleware

When sessions are enabled, `context["session"]` contains the session data.

```python
from cello import App, SessionConfig

app = App()
app.enable_session(SessionConfig(secret="session-secret"))

@app.get("/dashboard")
def dashboard(request):
    session = request.context.get("session", {})
    visits = session.get("visits", 0)
    return {"visits": visits}
```

---

## Custom Context Data

You can store arbitrary data in the context. Use it to pass computed values from middleware to handlers without repeating work.

```python
import time

@app.before_request
def add_timing(request):
    request.context["request_start"] = time.time()

@app.get("/slow")
def slow_endpoint(request):
    # ... do work ...
    elapsed = time.time() - request.context["request_start"]
    return {"elapsed_ms": round(elapsed * 1000, 2)}
```

---

## Context with Guards

Guards access the context to make authorization decisions. The JWT middleware populates `context["user"]`, and guards inspect it.

```python
from cello.guards import Role

@app.get("/admin", guards=[Role(["admin"])])
def admin_panel(request):
    # Role guard already verified context["user"]["roles"] contains "admin"
    return {"admin": True}
```

If the guard fails (e.g., the user lacks the required role), it raises a `GuardError` before the handler executes.

---

## Context with Dependency Injection

Singletons registered with `app.register_singleton()` are injected via `Depends`, not via context. Use context for per-request data and `Depends` for application-scoped dependencies.

| Use Case | Mechanism |
|----------|-----------|
| Authenticated user | `request.context["user"]` |
| Session data | `request.context["session"]` |
| Request ID | `request.context["request_id"]` |
| Database connection | `Depends("database")` |
| Service instances | `Depends("user_service")` |

---

## Request ID in Context

When request ID middleware is enabled, each request gets a unique identifier.

```python
app.enable_request_id()

@app.get("/debug")
def debug(request):
    return {"request_id": request.context.get("request_id")}
```

The same ID is included in the `X-Request-ID` response header for end-to-end tracing.

---

## Summary

| Feature | Description |
|---------|-------------|
| `request.context` | Per-request mutable `dict` |
| Populated by | Middleware (JWT, sessions, request ID, custom) |
| Read by | Handlers and guards |
| Scope | Single request lifetime |
| Thread safety | Each request has its own context instance |
