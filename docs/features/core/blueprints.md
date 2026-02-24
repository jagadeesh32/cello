---
title: Blueprints
description: Flask-like route grouping with Blueprints in Cello Framework
---

# Blueprints

Blueprints provide Flask-like route grouping with URL prefixes. They let you organize your application into modular components, each with its own routes, and optionally nest them for hierarchical URL structures.

## Creating a Blueprint

```python
from cello import App, Blueprint

# Create a blueprint with a URL prefix
api = Blueprint("/api")

@api.get("/users")
def list_users(request):
    return {"users": []}

@api.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}

# Register it with the app
app = App()
app.register_blueprint(api)

# Routes are now:
# GET /api/users
# GET /api/users/{id}
```

---

## Blueprint Constructor

```python
Blueprint(prefix: str, name: str = None)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `prefix` | `str` | URL prefix for all routes (e.g., `/api/v1`) |
| `name` | `str` | Optional name for identification |

```python
users_bp = Blueprint("/users", name="users")
print(users_bp.prefix)  # "/users"
print(users_bp.name)    # "users"
```

---

## HTTP Method Decorators

Blueprints support all the same HTTP method decorators as the `App`. Both sync and async handlers work interchangeably -- guards and validation wrappers applied by blueprint decorators automatically detect and handle async handlers:

```python
api = Blueprint("/api")

@api.get("/items")
def list_items(request):
    return {"items": []}

@api.post("/items")
async def create_item(request):
    data = request.json()
    item = await db.insert(data)
    return Response.json({"id": item["id"], **data}, status=201)

@api.put("/items/{id}")
def replace_item(request):
    return {"updated": request.params["id"]}

@api.patch("/items/{id}")
async def patch_item(request):
    await db.update(request.params["id"], request.json())
    return {"patched": request.params["id"]}

@api.delete("/items/{id}")
def delete_item(request):
    return Response.no_content()
```

---

## Nested Blueprints

Blueprints can be nested to create hierarchical URL structures:

```python
from cello import App, Blueprint

# Inner blueprint
users_bp = Blueprint("/users")

@users_bp.get("/")
def list_users(request):
    return {"users": []}

@users_bp.get("/{id}")
def get_user(request):
    return {"id": request.params["id"]}

@users_bp.post("/")
def create_user(request):
    return Response.json(request.json(), status=201)

# Inner blueprint for posts
posts_bp = Blueprint("/posts")

@posts_bp.get("/")
def list_posts(request):
    return {"posts": []}

@posts_bp.get("/{id}")
def get_post(request):
    return {"id": request.params["id"]}

# Outer blueprint
api_v1 = Blueprint("/api/v1")
api_v1.register(users_bp)
api_v1.register(posts_bp)

# Register with app
app = App()
app.register_blueprint(api_v1)

# Routes are now:
# GET  /api/v1/users/
# GET  /api/v1/users/{id}
# POST /api/v1/users/
# GET  /api/v1/posts/
# GET  /api/v1/posts/{id}
```

!!! tip "Nesting Depth"
    You can nest blueprints as deeply as needed. Prefixes are concatenated at registration time, so there is no runtime overhead from nesting.

---

## API Versioning with Blueprints

Blueprints are ideal for managing multiple API versions side by side:

```python
from cello import App, Blueprint

# Version 1
v1 = Blueprint("/api/v1")

@v1.get("/users")
def users_v1(request):
    return {"version": 1, "users": ["Alice", "Bob"]}

# Version 2 with different response format
v2 = Blueprint("/api/v2")

@v2.get("/users")
def users_v2(request):
    return {
        "version": 2,
        "data": {
            "users": [
                {"id": 1, "name": "Alice"},
                {"id": 2, "name": "Bob"}
            ]
        },
        "meta": {"total": 2}
    }

app = App()
app.register_blueprint(v1)
app.register_blueprint(v2)
```

---

## Blueprint Middleware

Apply middleware to all routes within a blueprint. Async handlers work seamlessly with blueprint-level middleware and per-route guards:

```python
from cello import App, Blueprint
from cello.guards import RoleGuard, Authenticated

# Public blueprint -- no auth required
public_bp = Blueprint("/public")

@public_bp.get("/status")
def status(request):
    return {"status": "ok"}

# Admin blueprint -- guards on individual routes
admin_bp = Blueprint("/admin")

@admin_bp.get("/dashboard", guards=[Authenticated()])
async def dashboard(request):
    stats = await fetch_dashboard_stats()
    return {"admin": True, "stats": stats}

@admin_bp.get("/users", guards=[RoleGuard(["admin"])])
async def admin_users(request):
    users = await db.fetch_all("SELECT * FROM users")
    return {"users": users, "admin": True}

app = App()
app.register_blueprint(public_bp)
app.register_blueprint(admin_bp)
```

---

## Inspecting Routes

Retrieve all registered routes from a blueprint, including routes from nested blueprints:

```python
api = Blueprint("/api")

@api.get("/health")
def health(request):
    return {"ok": True}

users = Blueprint("/users")

@users.get("/")
def list_users(request):
    return {"users": []}

api.register(users)

routes = api.get_all_routes()
# Returns: [("/api/health", "GET"), ("/api/users/", "GET")]
```

---

## Organizing a Large Application

A recommended project structure using blueprints:

```
myapp/
  __init__.py
  app.py           # App creation and blueprint registration
  routes/
    __init__.py
    users.py       # users_bp = Blueprint("/users")
    posts.py       # posts_bp = Blueprint("/posts")
    admin.py       # admin_bp = Blueprint("/admin")
```

```python
# myapp/routes/users.py
from cello import Blueprint, Response

users_bp = Blueprint("/users")

@users_bp.get("/")
def list_users(request):
    return {"users": []}

@users_bp.get("/{id}")
def get_user(request):
    return {"id": request.params["id"]}

@users_bp.post("/")
def create_user(request):
    data = request.json()
    return Response.json(data, status=201)
```

```python
# myapp/app.py
from cello import App, Blueprint
from myapp.routes.users import users_bp
from myapp.routes.posts import posts_bp
from myapp.routes.admin import admin_bp

app = App()

api = Blueprint("/api/v1")
api.register(users_bp)
api.register(posts_bp)

app.register_blueprint(api)
app.register_blueprint(admin_bp)

app.run()
```

---

## Next Steps

- [Routing](routing.md) - Route patterns and parameters
- [Async Support](async.md) - Using async handlers with blueprints
- [Middleware Overview](../middleware/overview.md) - Applying middleware
