---
title: Blueprint API
description: Route grouping and modular application structure with Blueprints
---

# Blueprint API

Blueprints group related routes under a common URL prefix. They enable modular application structure, similar to Flask's blueprints.

---

## Creating a Blueprint

```python
from cello import Blueprint

users_bp = Blueprint("/users", name="users")
```

### Constructor

```python
Blueprint(prefix: str, name: str = None)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `prefix` | `str` | Required | URL prefix for all routes in this blueprint |
| `name` | `str` | `None` | Optional name for identification and debugging |

---

## Properties

### `prefix`

```python
bp = Blueprint("/api/v1")
print(bp.prefix)  # "/api/v1"
```

### `name`

```python
bp = Blueprint("/api/v1", name="api_v1")
print(bp.name)  # "api_v1"
```

---

## Registering Routes

Blueprints support the same HTTP method decorators as `App`.

### `bp.get(path: str)`

```python
@users_bp.get("/")
def list_users(request):
    return {"users": []}

@users_bp.get("/{id}")
def get_user(request):
    return {"id": request.params["id"]}
```

### `bp.post(path: str)`

```python
@users_bp.post("/")
def create_user(request):
    data = request.json()
    return Response.json({"created": True}, status=201)
```

### `bp.put(path: str)`

```python
@users_bp.put("/{id}")
def update_user(request):
    return {"updated": True}
```

### `bp.delete(path: str)`

```python
@users_bp.delete("/{id}")
def delete_user(request):
    return {"deleted": True}
```

### `bp.patch(path: str)`

```python
@users_bp.patch("/{id}")
def patch_user(request):
    return {"patched": True}
```

---

## Registering with the App

Blueprints must be registered with the application to become active.

```python
from cello import App

app = App()
app.register_blueprint(users_bp)
```

After registration, all routes defined on `users_bp` are available under the `/users` prefix:

- `GET /users/` -- `list_users`
- `GET /users/{id}` -- `get_user`
- `POST /users/` -- `create_user`

---

## Nested Blueprints

Blueprints can contain other blueprints, forming a URL hierarchy.

```python
api_bp = Blueprint("/api")
v1_bp = Blueprint("/v1")
users_bp = Blueprint("/users")

@users_bp.get("/")
def list_users(request):
    return {"users": []}

# Nest: /api/v1/users/
v1_bp.register(users_bp)
api_bp.register(v1_bp)
app.register_blueprint(api_bp)
```

The resulting route is `GET /api/v1/users/`.

---

## Blueprint with Guards

Apply guards at the blueprint level so all routes in the blueprint require the guard to pass.

```python
from cello import Blueprint
from cello.guards import Role

admin_bp = Blueprint("/admin", guards=[Role(["admin"])])

@admin_bp.get("/dashboard")
def dashboard(request):
    return {"admin": True}

@admin_bp.get("/users")
def admin_users(request):
    return {"users": []}
```

Both `/admin/dashboard` and `/admin/users` require the `admin` role.

---

## Blueprint with Middleware

Apply middleware to a specific blueprint instead of the entire application.

```python
from cello import Blueprint

api_bp = Blueprint("/api")

# These middleware apply only to /api/* routes
api_bp.enable_cors(origins=["https://app.example.com"])
api_bp.enable_rate_limit(RateLimitConfig.token_bucket(requests=100, window=60))
```

---

## Getting All Routes

Retrieve all registered routes, including those from nested blueprints.

```python
bp = Blueprint("/api")

@bp.get("/health")
def health(request):
    return {"ok": True}

routes = bp.get_all_routes()
# Returns list of (method, path, handler) tuples
```

---

## Example: Modular Application

### blueprints/users.py

```python
from cello import Blueprint, Response

users_bp = Blueprint("/users")

@users_bp.get("/")
def list_users(request):
    return {"users": []}

@users_bp.post("/")
def create_user(request):
    return Response.json(request.json(), status=201)
```

### blueprints/orders.py

```python
from cello import Blueprint

orders_bp = Blueprint("/orders")

@orders_bp.get("/")
def list_orders(request):
    return {"orders": []}
```

### app.py

```python
from cello import App
from blueprints.users import users_bp
from blueprints.orders import orders_bp

app = App()
app.register_blueprint(users_bp)
app.register_blueprint(orders_bp)

if __name__ == "__main__":
    app.run()
```

---

## Summary

| Method | Description |
|--------|-------------|
| `Blueprint(prefix, name)` | Create a new blueprint |
| `bp.get(path)` | Register a GET route |
| `bp.post(path)` | Register a POST route |
| `bp.put(path)` | Register a PUT route |
| `bp.delete(path)` | Register a DELETE route |
| `bp.patch(path)` | Register a PATCH route |
| `bp.register(child_bp)` | Nest a child blueprint |
| `bp.get_all_routes()` | List all routes including nested |
| `app.register_blueprint(bp)` | Activate the blueprint in the application |
