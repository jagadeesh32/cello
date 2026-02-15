---
title: API Gateway
description: API gateway example with rate limiting, authentication, circuit breaker, and request routing
---

# API Gateway Example

This example builds an API gateway that sits in front of multiple backend services. It handles authentication, rate limiting, circuit breaking, and request routing -- all using Cello's built-in middleware.

---

## Architecture

```
                        ┌────────────────────────┐
    Client ────────────>│      API Gateway       │
                        │                        │
                        │  1. Rate Limiting       │
                        │  2. JWT Authentication  │
                        │  3. Circuit Breaker     │
                        │  4. Route to Service    │
                        └───┬────────┬────────┬──┘
                            |        |        |
                   ┌────────▼──┐ ┌───▼────┐ ┌─▼────────┐
                   │  Users    │ │ Orders │ │ Products │
                   │  Service  │ │ Service│ │ Service  │
                   └───────────┘ └────────┘ └──────────┘
```

- **Rate Limiting** protects backends from traffic spikes (token bucket, 100 req/min)
- **JWT Authentication** validates Bearer tokens and injects claims into context
- **Circuit Breaker** stops forwarding requests to failing services
- **Routing** maps gateway paths to the correct backend service

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
API Gateway with Cello.

Demonstrates rate limiting, JWT auth, circuit breaking, and
routing to multiple backend services -- all in a single process.
"""

from cello import App, Blueprint, Response
import json
import time
import hashlib
import hmac
import base64

app = App()
app.enable_cors()
app.enable_logging()
app.enable_compression()

# ===================================================================
# Configuration
# ===================================================================

JWT_SECRET = "super-secret-gateway-key"
RATE_LIMIT = 100  # requests per window
RATE_WINDOW = 60  # seconds

# ===================================================================
# In-Memory Rate Limiter
# ===================================================================

rate_limit_store = {}

def check_rate_limit(client_id):
    """Token bucket rate limiter. Returns (allowed, remaining, reset_at)."""
    now = time.time()
    window_start = int(now / RATE_WINDOW) * RATE_WINDOW
    reset_at = window_start + RATE_WINDOW
    key = f"{client_id}:{window_start}"

    if key not in rate_limit_store:
        rate_limit_store[key] = 0
        # Cleanup old windows
        for k in list(rate_limit_store):
            if not k.endswith(f":{window_start}"):
                del rate_limit_store[k]

    rate_limit_store[key] += 1
    count = rate_limit_store[key]
    remaining = max(0, RATE_LIMIT - count)
    return count <= RATE_LIMIT, remaining, int(reset_at)


# ===================================================================
# Simple JWT Helpers
# ===================================================================

def create_token(user_id, role="user"):
    """Create a simple JWT-like token for demonstration."""
    payload = json.dumps({"sub": user_id, "role": role, "exp": int(time.time()) + 3600})
    encoded = base64.urlsafe_b64encode(payload.encode()).decode()
    sig = hmac.new(JWT_SECRET.encode(), encoded.encode(), hashlib.sha256).hexdigest()[:16]
    return f"{encoded}.{sig}"

def verify_token(token):
    """Verify and decode a token. Returns claims dict or None."""
    try:
        parts = token.split(".")
        if len(parts) != 2:
            return None
        encoded, sig = parts
        expected_sig = hmac.new(JWT_SECRET.encode(), encoded.encode(), hashlib.sha256).hexdigest()[:16]
        if not hmac.compare_digest(sig, expected_sig):
            return None
        payload = json.loads(base64.urlsafe_b64decode(encoded))
        if payload.get("exp", 0) < time.time():
            return None
        return payload
    except Exception:
        return None


# ===================================================================
# Circuit Breaker
# ===================================================================

circuit_state = {}  # service -> {"failures": int, "open_since": float | None}
FAILURE_THRESHOLD = 5
RESET_TIMEOUT = 30  # seconds

def circuit_check(service_name):
    """Check if circuit is open. Returns (allowed, state_name)."""
    state = circuit_state.setdefault(service_name, {"failures": 0, "open_since": None})

    if state["open_since"] is not None:
        elapsed = time.time() - state["open_since"]
        if elapsed < RESET_TIMEOUT:
            return False, "open"
        # Half-open: allow one probe
        return True, "half-open"

    return True, "closed"

def circuit_record_success(service_name):
    """Record a successful response, closing the circuit."""
    circuit_state[service_name] = {"failures": 0, "open_since": None}

def circuit_record_failure(service_name):
    """Record a failure, potentially opening the circuit."""
    state = circuit_state.setdefault(service_name, {"failures": 0, "open_since": None})
    state["failures"] += 1
    if state["failures"] >= FAILURE_THRESHOLD:
        state["open_since"] = time.time()


# ===================================================================
# Backend Service Simulators
# ===================================================================

users_db = {
    "1": {"id": "1", "name": "Alice", "email": "alice@example.com"},
    "2": {"id": "2", "name": "Bob", "email": "bob@example.com"},
}

orders_db = {
    "1": {"id": "1", "user_id": "1", "product": "Widget", "total": 29.99, "status": "shipped"},
    "2": {"id": "2", "user_id": "2", "product": "Gadget", "total": 49.99, "status": "pending"},
}

products_db = {
    "1": {"id": "1", "name": "Widget", "price": 29.99, "stock": 150},
    "2": {"id": "2", "name": "Gadget", "price": 49.99, "stock": 75},
}


# ===================================================================
# Gateway Middleware
# ===================================================================

@app.before_request
def gateway_middleware(request):
    """Apply rate limiting and auth to all /api/* requests."""
    # Skip non-API paths
    if not request.path.startswith("/api/"):
        return None

    # Skip auth for token endpoint
    if request.path == "/api/token":
        return None

    # --- Rate Limiting ---
    client_ip = request.get_header("X-Forwarded-For") or "unknown"
    allowed, remaining, reset_at = check_rate_limit(client_ip)

    if not allowed:
        resp = Response.json(
            {"error": "Rate limit exceeded", "retry_after": reset_at - int(time.time())},
            status=429,
        )
        resp.set_header("X-RateLimit-Limit", str(RATE_LIMIT))
        resp.set_header("X-RateLimit-Remaining", "0")
        resp.set_header("X-RateLimit-Reset", str(reset_at))
        resp.set_header("Retry-After", str(reset_at - int(time.time())))
        return resp

    # Store rate limit info for after_request
    request.context["rate_limit_remaining"] = remaining
    request.context["rate_limit_reset"] = reset_at

    # --- JWT Authentication ---
    auth_header = request.get_header("Authorization")
    if not auth_header or not auth_header.startswith("Bearer "):
        return Response.json({"error": "Missing Bearer token"}, status=401)

    claims = verify_token(auth_header[7:])
    if not claims:
        return Response.json({"error": "Invalid or expired token"}, status=401)

    request.context["user_id"] = claims["sub"]
    request.context["role"] = claims.get("role", "user")
    return None


# ===================================================================
# Token Endpoint (Public)
# ===================================================================

@app.post("/api/token")
def get_token(request):
    """Issue a JWT token (simplified, no password check)."""
    data = request.json()
    user_id = data.get("user_id", "anonymous")
    role = data.get("role", "user")
    token = create_token(user_id, role)
    return {"token": token, "expires_in": 3600}


# ===================================================================
# Service Routes (Gateway -> Backend)
# ===================================================================

svc = Blueprint("/api/v1")

@svc.get("/users")
def route_users(request):
    """Route to user service."""
    allowed, state = circuit_check("users")
    if not allowed:
        return Response.json({"error": "User service unavailable", "circuit": state}, status=503)
    circuit_record_success("users")
    return {"users": list(users_db.values())}

@svc.get("/users/{id}")
def route_user(request):
    """Route to user service for a single user."""
    allowed, state = circuit_check("users")
    if not allowed:
        return Response.json({"error": "User service unavailable", "circuit": state}, status=503)
    user = users_db.get(request.params["id"])
    if not user:
        circuit_record_success("users")
        return Response.json({"error": "User not found"}, status=404)
    circuit_record_success("users")
    return user

@svc.get("/orders")
def route_orders(request):
    """Route to order service."""
    allowed, state = circuit_check("orders")
    if not allowed:
        return Response.json({"error": "Order service unavailable", "circuit": state}, status=503)
    # Scope to authenticated user (unless admin)
    user_id = request.context.get("user_id")
    role = request.context.get("role")
    if role == "admin":
        results = list(orders_db.values())
    else:
        results = [o for o in orders_db.values() if o["user_id"] == user_id]
    circuit_record_success("orders")
    return {"orders": results}

@svc.get("/products")
def route_products(request):
    """Route to product service."""
    allowed, state = circuit_check("products")
    if not allowed:
        return Response.json({"error": "Product service unavailable", "circuit": state}, status=503)
    circuit_record_success("products")
    return {"products": list(products_db.values())}

@svc.get("/products/{id}")
def route_product(request):
    """Route to product service for a single product."""
    allowed, state = circuit_check("products")
    if not allowed:
        return Response.json({"error": "Product service unavailable", "circuit": state}, status=503)
    product = products_db.get(request.params["id"])
    if not product:
        circuit_record_success("products")
        return Response.json({"error": "Product not found"}, status=404)
    circuit_record_success("products")
    return product


# ===================================================================
# Health & Status
# ===================================================================

@app.get("/health")
def health(request):
    """Gateway health check."""
    services = {}
    for name in ["users", "orders", "products"]:
        _, state = circuit_check(name)
        services[name] = state
    return {"status": "healthy", "circuits": services}

@app.get("/")
def gateway_index(request):
    """Gateway service discovery."""
    return {
        "gateway": "Cello API Gateway",
        "endpoints": [
            {"path": "/api/token", "method": "POST", "auth": False},
            {"path": "/api/v1/users", "method": "GET", "auth": True},
            {"path": "/api/v1/orders", "method": "GET", "auth": True},
            {"path": "/api/v1/products", "method": "GET", "auth": True},
            {"path": "/health", "method": "GET", "auth": False},
        ],
    }


# ===================================================================
# Register and Run
# ===================================================================

app.register_blueprint(svc)

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Testing with curl

### Get a Token

```bash
# Obtain an access token
curl -X POST http://127.0.0.1:8000/api/token \
  -H "Content-Type: application/json" \
  -d '{"user_id": "1", "role": "admin"}'
```

### Authenticated Requests

```bash
# Replace TOKEN with the value from the /api/token response
TOKEN="<paste-token-here>"

# List users
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8000/api/v1/users

# Get a specific user
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8000/api/v1/users/1

# List orders (admin sees all; regular user sees own orders)
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8000/api/v1/orders

# List products
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8000/api/v1/products
```

### Health Check

```bash
curl http://127.0.0.1:8000/health
```

---

## Key Patterns

### Rate Limiting

The gateway enforces a fixed-window rate limit per client IP. Standard `X-RateLimit-*` headers are included in responses. When the limit is exceeded, a `429 Too Many Requests` response is returned with a `Retry-After` header.

In production, use Cello's built-in `RateLimitConfig` with the token bucket or sliding window algorithm for more precise control.

### JWT Authentication

All `/api/v1/*` requests require a `Bearer` token. The gateway verifies the signature and expiration, then injects the user ID and role into `request.context` for downstream handlers. The `/api/token` endpoint is excluded from auth checks.

### Circuit Breaker

Each backend service has independent circuit breaker state. After 5 consecutive failures the circuit opens for 30 seconds, returning `503 Service Unavailable` immediately. After the timeout, one probe request is allowed (half-open state). A successful probe closes the circuit.

### Service Routing

The gateway maps public paths (`/api/v1/users`) to internal service logic. In a distributed deployment, these handlers would forward the request to separate microservices via HTTP or gRPC.

---

## Next Steps

- [Multi-Tenant SaaS](multi-tenant.md) - Add tenant isolation
- [Event Sourcing](event-sourcing.md) - Event-driven architecture patterns
- [Microservices](../advanced/microservices.md) - Deploy services independently
