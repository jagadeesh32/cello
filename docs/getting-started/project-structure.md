---
title: Project Structure
description: Recommended project layout for Cello Framework applications
---

# Project Structure

As your Cello application grows beyond a single file, organizing code into a clear structure makes it easier to maintain, test, and scale. This guide covers recommended layouts from small projects to large multi-module applications.

---

## Small Project

For simple APIs and prototypes, a flat structure works well:

```
my-app/
    app.py              # Application entry point
    requirements.txt    # Python dependencies
    .env                # Environment variables (not in git)
    .gitignore
```

```python
# app.py
from cello import App

app = App()

@app.get("/")
def home(request):
    return {"message": "Hello!"}

if __name__ == "__main__":
    app.run()
```

---

## Medium Project

When you have multiple resources, use blueprints to split routes into separate modules:

```
my-app/
    app.py                  # Application entry point
    config.py               # Configuration
    routes/
        __init__.py
        users.py            # User routes
        products.py         # Product routes
        health.py           # Health check routes
    services/
        __init__.py
        user_service.py     # Business logic
        product_service.py
    tests/
        __init__.py
        test_users.py
        test_products.py
    requirements.txt
    .env
```

### Entry Point (`app.py`)

```python
from cello import App
from config import configure_app
from routes.users import users_bp
from routes.products import products_bp
from routes.health import health_bp

app = App()
configure_app(app)

# Register blueprints
app.register_blueprint(users_bp)
app.register_blueprint(products_bp)
app.register_blueprint(health_bp)

if __name__ == "__main__":
    app.run()
```

### Configuration (`config.py`)

```python
import os

class Config:
    DEBUG = os.getenv("DEBUG", "false") == "true"
    DATABASE_URL = os.getenv("DATABASE_URL", "sqlite://app.db")
    SECRET_KEY = os.getenv("SECRET_KEY", "change-me-in-production")
    CORS_ORIGINS = os.getenv("CORS_ORIGINS", "*").split(",")

def configure_app(app):
    """Apply configuration to the app."""
    config = Config()
    app.enable_cors(config.CORS_ORIGINS)
    app.enable_logging()
    app.enable_compression()
    app.register_singleton("config", config)
```

### Route Module (`routes/users.py`)

```python
from cello import Blueprint, Response, Depends

users_bp = Blueprint("/api/users")

@users_bp.get("/")
def list_users(request):
    """List all users."""
    return {"users": []}

@users_bp.get("/{id}")
def get_user(request):
    """Get a user by ID."""
    user_id = request.params["id"]
    return {"id": user_id, "name": f"User {user_id}"}

@users_bp.post("/")
def create_user(request):
    """Create a new user."""
    data = request.json()
    return Response.json({"id": 1, **data}, status=201)

@users_bp.delete("/{id}")
def delete_user(request):
    """Delete a user."""
    return {"deleted": True}
```

---

## Large Project

For full-scale applications with multiple domains, middleware, and infrastructure concerns:

```
my-app/
    app.py                      # Application entry point
    config/
        __init__.py
        settings.py             # Environment-based settings
        security.py             # Security configuration
        database.py             # Database configuration
    api/
        __init__.py
        v1/
            __init__.py
            users/
                __init__.py
                routes.py       # Route definitions
                service.py      # Business logic
                models.py       # Data models / DTOs
            products/
                __init__.py
                routes.py
                service.py
                models.py
            orders/
                __init__.py
                routes.py
                service.py
                models.py
        v2/
            __init__.py
            users/
                routes.py       # V2 user routes
    middleware/
        __init__.py
        auth.py                 # Custom auth middleware
        tenant.py               # Multi-tenant middleware
    templates/
        emails/
            welcome.html
            reset_password.html
        pages/
            index.html
    static/
        css/
        js/
        images/
    tests/
        __init__.py
        conftest.py             # Shared test fixtures
        api/
            test_users.py
            test_products.py
            test_orders.py
        integration/
            test_auth_flow.py
    scripts/
        seed_db.py              # Database seeding
        migrate.py              # Migrations
    Dockerfile
    docker-compose.yml
    requirements.txt
    pyproject.toml
    .env
    .env.example
```

### API Versioning with Blueprints

```python
# api/v1/__init__.py
from cello import Blueprint
from .users.routes import users_bp
from .products.routes import products_bp
from .orders.routes import orders_bp

v1 = Blueprint("/api/v1")
v1.register(users_bp)
v1.register(products_bp)
v1.register(orders_bp)
```

```python
# api/v2/__init__.py
from cello import Blueprint
from .users.routes import users_bp

v2 = Blueprint("/api/v2")
v2.register(users_bp)
```

```python
# app.py
from cello import App
from api.v1 import v1
from api.v2 import v2

app = App()
app.register_blueprint(v1)
app.register_blueprint(v2)

# Routes:
# /api/v1/users, /api/v1/products, /api/v1/orders
# /api/v2/users
```

---

## Separating Routes into Blueprints

### By Resource

Group all routes for a resource in a single blueprint:

```python
# api/v1/users/routes.py
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
    return Response.json(request.json(), status=201)
```

### By Function

Separate public and admin routes:

```python
# Public routes (no auth required)
public_bp = Blueprint("/public")

@public_bp.get("/status")
def status(request):
    return {"status": "ok"}

# Admin routes (auth required)
admin_bp = Blueprint("/admin")

@admin_bp.get("/dashboard")
def dashboard(request):
    return {"admin": True}

# Register both
app.register_blueprint(public_bp)
app.register_blueprint(admin_bp)
```

---

## Tests Directory

Organize tests to mirror your application structure:

```
tests/
    __init__.py
    conftest.py              # Shared fixtures
    test_health.py           # Health check tests
    api/
        __init__.py
        test_users.py        # User endpoint tests
        test_products.py     # Product endpoint tests
    integration/
        test_full_flow.py    # End-to-end tests
```

### Example Test

```python
# tests/api/test_users.py
import requests

BASE_URL = "http://127.0.0.1:8000"

def test_list_users():
    response = requests.get(f"{BASE_URL}/api/users")
    assert response.status_code == 200
    data = response.json()
    assert "users" in data

def test_create_user():
    response = requests.post(
        f"{BASE_URL}/api/users",
        json={"name": "Alice", "email": "alice@example.com"},
    )
    assert response.status_code == 201
    assert response.json()["name"] == "Alice"
```

---

## Configuration Files

### Environment Variables (`.env`)

```bash
# .env
DEBUG=true
DATABASE_URL=postgres://localhost/mydb
SECRET_KEY=your-secret-key-minimum-32-bytes
CORS_ORIGINS=http://localhost:3000,http://localhost:8080
WORKERS=4
```

### `.env.example`

Provide a template without secrets:

```bash
# .env.example
DEBUG=false
DATABASE_URL=postgres://localhost/mydb
SECRET_KEY=change-me
CORS_ORIGINS=*
WORKERS=4
```

### `.gitignore`

```
.env
__pycache__/
*.pyc
.venv/
target/
uploads/
*.egg-info/
```

---

## Best Practices

!!! tip "Recommendations"
    - **One blueprint per resource** -- keeps related routes together.
    - **Separate business logic from routes** -- put complex logic in service modules.
    - **Use `__init__.py`** to re-export blueprints for cleaner imports.
    - **Keep `app.py` thin** -- it should only wire things together.
    - **Store secrets in environment variables**, never in code.
    - **Mirror app structure in tests** for easy navigation.

---

## Next Steps

- [Configuration](configuration.md) - App configuration options
- [Routing](../features/core/routing.md) - Blueprint and route details
- [First App](first-app.md) - Step-by-step tutorial
