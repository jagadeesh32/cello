---
title: "Tutorial: Microservices"
description: Guide to building microservices with Cello using blueprints, health checks, and circuit breakers
---

# Tutorial: Microservices

In this tutorial you will decompose a monolithic application into microservices using Cello. You will learn how to use blueprints for service boundaries, add health check endpoints, communicate between services, protect against cascading failures with the circuit breaker, share middleware, and plan for deployment.

---

## Architecture Overview

We will build two services:

| Service | Port | Responsibility |
|---------|------|----------------|
| **User Service** | 8001 | User registration, profiles |
| **Order Service** | 8002 | Order creation, calls User Service to validate users |

Both services are standalone Cello applications that communicate over HTTP.

---

## Step 1: Shared Middleware Module

Create a shared module that both services import. This keeps cross-cutting concerns consistent.

```python
# shared.py
from cello import App, Response, JwtConfig, RateLimitConfig, HealthCheckConfig

def configure_service(app: App, service_name: str):
    """Apply standard middleware to any service."""

    # Structured logging
    app.enable_logging()

    # CORS for API consumers
    app.enable_cors(origins=["*"])

    # Rate limiting
    app.enable_rate_limit(RateLimitConfig.token_bucket(
        requests=200,
        window=60,
    ))

    # Health checks at /health/live, /health/ready, /health
    app.enable_health_checks(HealthCheckConfig(
        base_path="/health",
        include_system_info=True,
    ))

    # Prometheus metrics
    app.enable_prometheus(
        endpoint="/metrics",
        namespace=service_name,
    )
```

---

## Step 2: User Service

Create `user_service.py`.

```python
# user_service.py
from cello import App, Blueprint, Response
from shared import configure_service

app = App()
configure_service(app, "user_service")

# --- Data store ---
users = {}
next_id = 1

# --- Routes ---
user_bp = Blueprint("/users")

@user_bp.get("/")
def list_users(request):
    return {"users": list(users.values())}

@user_bp.get("/{id}")
def get_user(request):
    uid = int(request.params["id"])
    user = users.get(uid)
    if not user:
        return Response.json({"error": "User not found"}, status=404)
    return user

@user_bp.post("/")
def create_user(request):
    global next_id
    data = request.json()
    user = {"id": next_id, "name": data["name"], "email": data["email"]}
    users[next_id] = user
    next_id += 1
    return Response.json(user, status=201)

app.register_blueprint(user_bp)

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8001)
```

---

## Step 3: Order Service with Inter-Service Communication

Create `order_service.py`. This service calls the User Service to validate that a user exists before creating an order.

```python
# order_service.py
import urllib.request
import json
from cello import App, Blueprint, Response
from shared import configure_service

app = App()
configure_service(app, "order_service")

USER_SERVICE_URL = "http://127.0.0.1:8001"

orders = {}
next_id = 1

def fetch_user(user_id: int) -> dict | None:
    """Call the User Service to retrieve a user."""
    try:
        url = f"{USER_SERVICE_URL}/users/{user_id}"
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=5) as resp:
            return json.loads(resp.read())
    except Exception:
        return None

# --- Routes ---
order_bp = Blueprint("/orders")

@order_bp.post("/")
def create_order(request):
    global next_id
    data = request.json()
    user_id = data.get("user_id")

    # Validate user exists in User Service
    user = fetch_user(user_id)
    if not user:
        return Response.json(
            {"error": f"User {user_id} not found in User Service"},
            status=400,
        )

    order = {
        "id": next_id,
        "user_id": user_id,
        "user_name": user.get("name"),
        "items": data.get("items", []),
        "status": "created",
    }
    orders[next_id] = order
    next_id += 1
    return Response.json(order, status=201)

@order_bp.get("/")
def list_orders(request):
    return {"orders": list(orders.values())}

app.register_blueprint(order_bp)

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8002)
```

---

## Step 4: Circuit Breaker for Resilience

If the User Service goes down, the Order Service should not hang or crash. Enable the circuit breaker to fail fast.

```python
# Add to order_service.py after configure_service()

app.enable_circuit_breaker(
    failure_threshold=5,     # Open circuit after 5 consecutive failures
    reset_timeout=30,        # Wait 30 seconds before retrying
    half_open_target=3,      # Require 3 successes to close the circuit
    failure_codes=[500, 502, 503, 504],
)
```

When the circuit is open, requests to failing upstream services return immediately with a `503 Service Unavailable` response instead of blocking.

---

## Step 5: Health Checks

Both services already have health checks from `configure_service`. Verify them:

```bash
# Liveness probe -- is the process running?
curl http://127.0.0.1:8001/health/live

# Readiness probe -- is it ready to serve traffic?
curl http://127.0.0.1:8001/health/ready

# Full health report
curl http://127.0.0.1:8001/health
```

Expected response:

```json
{
  "status": "healthy",
  "checks": {
    "liveness": "ok",
    "readiness": "ok"
  },
  "system": {
    "uptime_seconds": 42,
    "memory_mb": 28
  }
}
```

---

## Step 6: Service Discovery Pattern

For dynamic environments, externalize service URLs through environment variables.

```python
import os

USER_SERVICE_URL = os.environ.get("USER_SERVICE_URL", "http://127.0.0.1:8001")
```

In Kubernetes or Docker Compose, inject the variable at deployment time:

```yaml
# docker-compose.yml snippet
services:
  order-service:
    environment:
      USER_SERVICE_URL: http://user-service:8001
```

---

## Step 7: Shared Authentication

Use the same JWT secret across services so a token issued by one service is valid everywhere.

```python
# Add to shared.py
import os

SHARED_JWT_CONFIG = JwtConfig(
    secret=os.environ.get("JWT_SECRET", "shared-dev-secret"),
    algorithm="HS256",
    expiration=3600,
)

def configure_service(app: App, service_name: str):
    # ... existing middleware ...
    app.enable_jwt(SHARED_JWT_CONFIG)
```

---

## Step 8: Running Both Services

Open two terminals:

```bash
# Terminal 1
python user_service.py

# Terminal 2
python order_service.py
```

Test the flow:

```bash
# Create a user
curl -X POST http://127.0.0.1:8001/users/ \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "email": "alice@example.com"}'

# Create an order referencing that user
curl -X POST http://127.0.0.1:8002/orders/ \
  -H "Content-Type: application/json" \
  -d '{"user_id": 1, "items": ["Book", "Pen"]}'

# Attempt an order for a nonexistent user
curl -X POST http://127.0.0.1:8002/orders/ \
  -H "Content-Type: application/json" \
  -d '{"user_id": 999, "items": ["Ghost"]}'
```

---

## Deployment Considerations

| Concern | Recommendation |
|---------|---------------|
| **Containerization** | Package each service as a Docker image (see [Docker guide](../../enterprise/deployment/docker.md)) |
| **Orchestration** | Use Kubernetes Deployments with liveness/readiness probes pointing at `/health/live` and `/health/ready` |
| **Observability** | Enable [OpenTelemetry](../../enterprise/observability/opentelemetry.md) for distributed tracing across services |
| **Configuration** | Use environment variables or ConfigMaps; never hard-code URLs or secrets |
| **Scaling** | Use `--workers` flag or Kubernetes HPA based on Prometheus metrics from `/metrics` |

---

## Next Steps

- Read the [Docker deployment guide](../../enterprise/deployment/docker.md) to containerize your services.
- Add [distributed tracing](../../enterprise/observability/tracing.md) to follow requests across service boundaries.
- Explore the [Service Mesh guide](../../enterprise/deployment/service-mesh.md) for mTLS and traffic management.
