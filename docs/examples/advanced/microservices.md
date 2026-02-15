---
title: Microservices
description: Microservices example with blueprints, health checks, and service communication
---

# Microservices Example

This example demonstrates building a microservices-style application using Cello. Each service is structured with its own blueprint, health checks, and configuration, while running in a single process for simplicity.

---

## Architecture

```
                    ┌─────────────────────────┐
                    │       API Gateway        │
                    │   /api/v1/* -> services  │
                    └──────┬──────────┬────────┘
                           |          |
              ┌────────────┘          └────────────┐
              |                                    |
    ┌─────────▼─────────┐             ┌────────────▼────────┐
    │   User Service     │             │   Product Service    │
    │   /api/v1/users    │             │   /api/v1/products   │
    └────────────────────┘             └─────────────────────┘
```

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
Microservices-style architecture with Cello.

Each service is a Blueprint with its own routes, health check, and data.
"""

from cello import App, Blueprint, Response
import time
import json

app = App()
app.enable_cors()
app.enable_logging()
app.enable_compression()

START_TIME = time.time()


# ===================================================================
# Service: User Management
# ===================================================================

user_service = Blueprint("/api/v1/users")
_users = {
    "1": {"id": "1", "name": "Alice", "email": "alice@example.com", "active": True},
    "2": {"id": "2", "name": "Bob", "email": "bob@example.com", "active": True},
}
_user_next_id = 3

@user_service.get("/")
def list_users(request):
    active_only = request.query.get("active", "false") == "true"
    result = list(_users.values())
    if active_only:
        result = [u for u in result if u["active"]]
    return {"users": result, "count": len(result)}

@user_service.get("/{id}")
def get_user(request):
    user = _users.get(request.params["id"])
    if not user:
        return Response.json({"error": "User not found"}, status=404)
    return user

@user_service.post("/")
def create_user(request):
    global _user_next_id
    data = request.json()
    user = {
        "id": str(_user_next_id),
        "name": data["name"],
        "email": data["email"],
        "active": True,
    }
    _users[user["id"]] = user
    _user_next_id += 1
    return Response.json(user, status=201)

@user_service.delete("/{id}")
def delete_user(request):
    uid = request.params["id"]
    if uid not in _users:
        return Response.json({"error": "User not found"}, status=404)
    _users[uid]["active"] = False
    return {"deactivated": True}


# ===================================================================
# Service: Product Catalog
# ===================================================================

product_service = Blueprint("/api/v1/products")
_products = {
    "1": {"id": "1", "name": "Cello Framework License", "price": 0.00, "stock": 999},
    "2": {"id": "2", "name": "Premium Support", "price": 99.00, "stock": 50},
}
_product_next_id = 3

@product_service.get("/")
def list_products(request):
    min_price = request.query.get("min_price")
    max_price = request.query.get("max_price")
    result = list(_products.values())
    if min_price:
        result = [p for p in result if p["price"] >= float(min_price)]
    if max_price:
        result = [p for p in result if p["price"] <= float(max_price)]
    return {"products": result, "count": len(result)}

@product_service.get("/{id}")
def get_product(request):
    product = _products.get(request.params["id"])
    if not product:
        return Response.json({"error": "Product not found"}, status=404)
    return product

@product_service.post("/")
def create_product(request):
    global _product_next_id
    data = request.json()
    product = {
        "id": str(_product_next_id),
        "name": data["name"],
        "price": float(data.get("price", 0)),
        "stock": int(data.get("stock", 0)),
    }
    _products[product["id"]] = product
    _product_next_id += 1
    return Response.json(product, status=201)


# ===================================================================
# Health Checks
# ===================================================================

health_bp = Blueprint("/health")

@health_bp.get("/live")
def liveness(request):
    """Kubernetes liveness probe."""
    return {"status": "alive"}

@health_bp.get("/ready")
def readiness(request):
    """Kubernetes readiness probe."""
    return {
        "status": "ready",
        "services": {
            "user_service": {"status": "up", "users": len(_users)},
            "product_service": {"status": "up", "products": len(_products)},
        },
    }

@health_bp.get("/")
def full_health(request):
    """Comprehensive health report."""
    uptime = time.time() - START_TIME
    return {
        "status": "healthy",
        "uptime_seconds": round(uptime, 2),
        "services": {
            "user_service": {
                "status": "up",
                "total_users": len(_users),
                "active_users": sum(1 for u in _users.values() if u["active"]),
            },
            "product_service": {
                "status": "up",
                "total_products": len(_products),
                "total_stock": sum(p["stock"] for p in _products.values()),
            },
        },
    }


# ===================================================================
# Service Discovery / Index
# ===================================================================

@app.get("/")
def service_index(request):
    return {
        "name": "Cello Microservices",
        "services": [
            {"name": "users", "base_url": "/api/v1/users"},
            {"name": "products", "base_url": "/api/v1/products"},
        ],
        "health": "/health",
    }


# ===================================================================
# Register All Services
# ===================================================================

app.register_blueprint(user_service)
app.register_blueprint(product_service)
app.register_blueprint(health_bp)

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Testing

```bash
# Service discovery
curl http://127.0.0.1:8000/

# Health checks
curl http://127.0.0.1:8000/health/live
curl http://127.0.0.1:8000/health/ready
curl http://127.0.0.1:8000/health/

# User service
curl http://127.0.0.1:8000/api/v1/users
curl http://127.0.0.1:8000/api/v1/users/1
curl -X POST http://127.0.0.1:8000/api/v1/users \
  -H "Content-Type: application/json" \
  -d '{"name": "Charlie", "email": "charlie@example.com"}'

# Product service
curl http://127.0.0.1:8000/api/v1/products
curl "http://127.0.0.1:8000/api/v1/products?max_price=50"
```

---

## Scaling to Separate Processes

To deploy each service as an independent process, extract each blueprint into its own `app.py`:

```python
# user_service/app.py
from cello import App
from routes import user_service, health_bp

app = App()
app.register_blueprint(user_service)
app.register_blueprint(health_bp)

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8001)
```

Use an API gateway or reverse proxy (nginx, Traefik) to route traffic to each service.

---

## Next Steps

- [API Gateway](../enterprise/api-gateway.md) - Add auth, rate limiting, and circuit breaking
- [Full-stack App](fullstack.md) - Combine services with a frontend
- [Project Structure](../../getting-started/project-structure.md) - Organize large codebases
