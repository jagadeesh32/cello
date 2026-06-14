---
title: Advanced Middleware — DI, Guards & Prometheus
description: Explore role-based access control guards, dependency injection via Depends, and Prometheus metrics scraping in a single Cello app.
---

# :material-shield-star: Advanced Middleware — DI, Guards & Prometheus

Cello's middleware story goes well beyond simple request/response transformations. This example combines three advanced features in one application: a Prometheus metrics exporter, an RBAC guard that intercepts requests before they reach any handler, and Cello's dependency-injection system that resolves named singletons at call time. Together they illustrate how to build production-grade cross-cutting concerns without touching individual route handlers.

## Features Demonstrated

- `app.enable_prometheus()` — exposes a `/metrics` endpoint compatible with any Prometheus scraper
- `app.register_singleton("name", value)` — registers a shared object in Cello's DI container
- `app.add_guard(fn)` — attaches a request guard that runs before every route handler
- Role-based access control (RBAC) implemented as a plain guard function
- `Depends("name")` — injects a registered singleton as a default argument into a handler
- Path-prefix inspection inside a guard for route-level authorization
- Simulated admin and moderator role checks without external auth libraries

## Complete Source Code

```python
"""
Advanced Middleware Example for Cello Framework
Run with: python examples/advanced_middleware.py
Then visit: http://localhost:8000/docs
"""
from cello import App, Response, Depends

app = App()
app.enable_prometheus()
app.register_singleton("database", {"url": "postgres://localhost:5432/cello"})

def rbac_guard(request):
    request.set_context("user", {"roles": ["user"]})
    path = request.path
    if path.startswith("/admin"):
        user = request.get_context("user") or {}
        if "admin" not in user.get("roles", []):
            return "Admin access required"
    return True

app.add_guard(rbac_guard)

@app.get("/")
def home(request):
    return {"message": "Welcome to Cello with Advanced Middleware!"}

@app.get("/health")
def health(request):
    return {"status": "healthy"}

@app.get("/user")
def user_endpoint(request):
    return {"message": "User endpoint (requires authentication in production)"}

@app.get("/admin")
def admin_endpoint(request):
    return {"message": "Admin endpoint (requires admin role in production)"}

@app.get("/admin-or-moderator")
def admin_or_moderator(request):
    return {"message": "Admin or Moderator endpoint"}

@app.get("/users/{user_id}")
def get_user(request, db=Depends("database")):
    user_id = request.params.get("user_id")
    return {"user_id": user_id, "database_info": db, "note": "Injected via Dependency Injection!"}

@app.post("/users")
def create_user(request):
    data = request.json()
    return {"message": "User created (simulated)", "data": data}

@app.get("/api/stats")
def api_stats(request):
    return {"message": "Visit /metrics for Prometheus metrics",
            "endpoints": {"/": "Home", "/user": "Protected", "/admin": "Admin", "/metrics": "Prometheus"}}

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Running This Example

```bash
python examples/advanced_middleware.py
# Test:
curl http://127.0.0.1:8000/
```

## Key Concepts

- **Guards return `True` to allow or a string to deny** — when a guard function returns anything other than `True`, Cello short-circuits the handler and sends a `403 Forbidden` response containing the returned string as the error message.
- **`request.set_context` / `get_context`** — guards and handlers share per-request state through a context dictionary, avoiding global variables and keeping request data properly scoped.
- **`register_singleton`** — stores any Python object (a DB config dict, a connection pool, a service class) under a string key so it can be resolved by name later without passing it through every call frame.
- **`Depends("database")`** — tells Cello's DI system to look up the `"database"` singleton and pass it as the `db` argument; the handler remains a plain function with no imports from the DI layer beyond the annotation.
- **`enable_prometheus()`** — instruments every route automatically; counters and histograms are updated on each request so the `/metrics` endpoint always reflects real traffic without any per-handler boilerplate.
