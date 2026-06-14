---
title: Auto-Generated OpenAPI Docs
description: Annotate routes with tags and summaries, then call enable_openapi() to get a fully interactive Swagger UI at /docs with zero extra code.
---

# :material-file-document-edit: Auto-Generated OpenAPI Docs

Cello can introspect your route table and generate a complete OpenAPI 3.x specification automatically. By adding `tags`, `summary`, and a docstring to each handler, you give the generator the metadata it needs to produce a well-organised, human-readable API reference. A single call to `app.enable_openapi()` then mounts an interactive Swagger UI at `/docs` and a raw JSON spec at `/openapi.json` — no separate configuration files or schema classes required.

## Features Demonstrated

- `tags=[…]` on route decorators — groups endpoints into named sections in the Swagger UI
- `summary="…"` on route decorators — provides a one-line description shown in the endpoint list
- Python docstrings on handler functions — used as the long-form description in the spec
- `app.enable_openapi(title=…, version=…)` — generates the spec and mounts `/docs` and `/openapi.json`
- Full CRUD route set (`GET`, `POST`, `PUT`, `DELETE`) across two resource types (`users`, `items`)
- Path parameters (`/users/{id}`) and query parameters (`?limit=`) reflected in the spec automatically
- `app.enable_cors(origins=["*"])` allowing the Swagger UI to make in-browser test requests

## Complete Source Code

```python
#!/usr/bin/env python3
"""
Cello Framework - Auto-Generated OpenAPI Demo
Run with: python examples/auto_openapi_demo.py
Then visit: http://127.0.0.1:8080/docs
"""

from cello import App, Response

app = App()
app.enable_cors(origins=["*"])
app.enable_logging()

@app.get("/", tags=["General"], summary="API Home")
def home(request):
    """Returns welcome message and API info."""
    return {"message": "Welcome to Cello API!", "version": "1.0.1", "docs": "/docs"}

@app.get("/health", tags=["General"], summary="Health Check")
def health_check(request):
    """Check server health status."""
    return {"status": "healthy", "framework": "cello"}

@app.get("/users", tags=["Users"], summary="List Users")
def list_users(request):
    """Get all users from the database."""
    return {"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}

@app.get("/users/{id}", tags=["Users"], summary="Get User")
def get_user(request):
    """Get a specific user by their ID."""
    user_id = request.params.get("id")
    return {"id": int(user_id), "name": f"User {user_id}"}

@app.post("/users", tags=["Users"], summary="Create User")
def create_user(request):
    """Create a new user in the database."""
    data = request.json()
    return {"message": "User created", "user": data, "id": 123}

@app.put("/users/{id}", tags=["Users"], summary="Update User")
def update_user(request):
    """Update an existing user."""
    user_id = request.params.get("id")
    data = request.json()
    return {"message": f"User {user_id} updated", "user": data}

@app.delete("/users/{id}", tags=["Users"], summary="Delete User")
def delete_user(request):
    """Delete a user from the database."""
    user_id = request.params.get("id")
    return {"message": f"User {user_id} deleted"}

@app.get("/items", tags=["Items"], summary="List Items")
def list_items(request):
    """Get all items with optional pagination."""
    limit = request.query.get("limit", "10")
    return {"items": [{"id": 1, "name": "Laptop", "price": 999.99}], "limit": int(limit)}

@app.post("/items", tags=["Items"], summary="Create Item")
def create_item(request):
    """Create a new item in the catalog."""
    data = request.json()
    return {"message": "Item created", "item": data}

app.enable_openapi(title="My Auto-Generated API", version="1.0.1")

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8080)
```

## Running This Example

```bash
python examples/auto_openapi_demo.py
# Test:
curl http://127.0.0.1:8000/
```

## Key Concepts

- **`enable_openapi()` must be called after all routes are registered** — the generator reads the route table at the moment it is called, so routes registered afterwards will not appear in the spec.
- **`tags` create sidebar groups in Swagger UI** — grouping related endpoints under the same tag (e.g., `"Users"`) makes large APIs navigable; endpoints with no tag land in a default group.
- **Docstrings become descriptions** — Cello reads `__doc__` from each handler function, so the same text that documents the function in your editor also appears in the published API reference without duplication.
- **Path parameters are inferred from route patterns** — `{id}` in `/users/{id}` is automatically promoted to a `path` parameter in the OpenAPI spec; you do not need to declare it separately.
- **`/openapi.json`** — the raw machine-readable spec is available alongside the UI, making it straightforward to import into Postman, generate client SDKs, or run contract tests against the spec.
