---
title: Multi-Tenant SaaS
description: Multi-tenant SaaS example with tenant middleware, tenant-aware routing, and data isolation
---

# Multi-Tenant SaaS Example

This example demonstrates building a multi-tenant SaaS application with Cello. Tenants are identified by a subdomain or header, and all data access is scoped to the authenticated tenant.

---

## Architecture

```
                    ┌────────────────────────────┐
                    │     Tenant Middleware       │
                    │  Extract tenant from header │
                    │  or subdomain, validate,    │
                    │  inject into request context │
                    └──────────┬─────────────────┘
                               |
              ┌────────────────┼────────────────┐
              |                |                |
    ┌─────────▼──────┐  ┌─────▼──────┐  ┌──────▼─────────┐
    │  Tenant Admin  │  │  Tenant    │  │  Shared Admin  │
    │  /api/settings │  │  /api/data │  │  /admin/tenants│
    └────────────────┘  └────────────┘  └────────────────┘
```

- **Tenant Middleware** extracts and validates the tenant on every request
- **Scoped Data** ensures each tenant only sees its own records
- **Admin API** manages tenant registration and provisioning

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
Multi-tenant SaaS application with Cello.

Tenants are identified by the X-Tenant-ID header. Each tenant has
isolated data, rate limits, and configuration.
"""

from cello import App, Blueprint, Response
import json
import time

app = App()
app.enable_cors()
app.enable_logging()

# ===================================================================
# Tenant Registry (in-memory for demonstration)
# ===================================================================

tenants = {
    "acme": {
        "id": "acme",
        "name": "Acme Corporation",
        "plan": "enterprise",
        "active": True,
        "created_at": 1700000000,
        "settings": {"max_users": 100, "features": ["analytics", "api", "export"]},
    },
    "globex": {
        "id": "globex",
        "name": "Globex Inc.",
        "plan": "starter",
        "active": True,
        "created_at": 1700100000,
        "settings": {"max_users": 10, "features": ["api"]},
    },
}

# Tenant-scoped data stores
tenant_data = {
    "acme": {
        "users": {
            "1": {"id": "1", "name": "Alice", "role": "admin"},
            "2": {"id": "2", "name": "Bob", "role": "member"},
        },
        "next_id": 3,
    },
    "globex": {
        "users": {
            "1": {"id": "1", "name": "Charlie", "role": "admin"},
        },
        "next_id": 2,
    },
}


# ===================================================================
# Tenant Middleware
# ===================================================================

@app.before_request
def tenant_middleware(request):
    """Extract and validate tenant from every request."""
    # Skip for admin endpoints
    if request.path.startswith("/admin"):
        return None

    tenant_id = request.get_header("X-Tenant-ID")
    if not tenant_id:
        return Response.json(
            {"error": "Missing X-Tenant-ID header"},
            status=400,
        )

    tenant = tenants.get(tenant_id)
    if not tenant:
        return Response.json(
            {"error": f"Unknown tenant: {tenant_id}"},
            status=404,
        )

    if not tenant["active"]:
        return Response.json(
            {"error": "Tenant account is suspended"},
            status=403,
        )

    # Store tenant in request context for downstream handlers
    request.context["tenant_id"] = tenant_id
    request.context["tenant"] = tenant
    return None


# ===================================================================
# Tenant-Scoped API
# ===================================================================

api = Blueprint("/api")

@api.get("/me")
def tenant_info(request):
    """Return the current tenant's profile."""
    return request.context["tenant"]

@api.get("/settings")
def get_settings(request):
    """Return the current tenant's settings."""
    tenant = request.context["tenant"]
    return {"tenant": tenant["id"], "settings": tenant["settings"]}

@api.put("/settings")
def update_settings(request):
    """Update the current tenant's settings."""
    tenant_id = request.context["tenant_id"]
    data = request.json()
    tenants[tenant_id]["settings"].update(data)
    return {"updated": True, "settings": tenants[tenant_id]["settings"]}

@api.get("/users")
def list_users(request):
    """List users scoped to the current tenant."""
    tenant_id = request.context["tenant_id"]
    store = tenant_data.get(tenant_id, {"users": {}})
    users = list(store["users"].values())
    return {"tenant": tenant_id, "users": users, "count": len(users)}

@api.get("/users/{id}")
def get_user(request):
    """Get a user by ID within the current tenant."""
    tenant_id = request.context["tenant_id"]
    user_id = request.params["id"]
    store = tenant_data.get(tenant_id, {"users": {}})
    user = store["users"].get(user_id)
    if not user:
        return Response.json({"error": "User not found"}, status=404)
    return user

@api.post("/users")
def create_user(request):
    """Create a user within the current tenant."""
    tenant_id = request.context["tenant_id"]
    tenant = request.context["tenant"]
    store = tenant_data.setdefault(tenant_id, {"users": {}, "next_id": 1})

    # Enforce plan limits
    max_users = tenant["settings"].get("max_users", 10)
    if len(store["users"]) >= max_users:
        return Response.json(
            {"error": f"User limit reached ({max_users} users on {tenant['plan']} plan)"},
            status=403,
        )

    data = request.json()
    user_id = str(store["next_id"])
    user = {
        "id": user_id,
        "name": data["name"],
        "role": data.get("role", "member"),
    }
    store["users"][user_id] = user
    store["next_id"] += 1
    return Response.json(user, status=201)


# ===================================================================
# Admin API (Platform-Level)
# ===================================================================

admin = Blueprint("/admin")

@admin.get("/tenants")
def list_tenants(request):
    """List all registered tenants (admin only)."""
    return {"tenants": list(tenants.values()), "count": len(tenants)}

@admin.get("/tenants/{id}")
def get_tenant(request):
    """Get a specific tenant's details."""
    tenant = tenants.get(request.params["id"])
    if not tenant:
        return Response.json({"error": "Tenant not found"}, status=404)
    store = tenant_data.get(request.params["id"], {"users": {}})
    return {**tenant, "user_count": len(store["users"])}

@admin.post("/tenants")
def create_tenant(request):
    """Register a new tenant."""
    data = request.json()
    tenant_id = data["id"]
    if tenant_id in tenants:
        return Response.json({"error": "Tenant already exists"}, status=409)

    tenant = {
        "id": tenant_id,
        "name": data["name"],
        "plan": data.get("plan", "starter"),
        "active": True,
        "created_at": int(time.time()),
        "settings": {"max_users": 10, "features": ["api"]},
    }
    tenants[tenant_id] = tenant
    tenant_data[tenant_id] = {"users": {}, "next_id": 1}
    return Response.json(tenant, status=201)

@admin.put("/tenants/{id}/suspend")
def suspend_tenant(request):
    """Suspend a tenant account."""
    tenant = tenants.get(request.params["id"])
    if not tenant:
        return Response.json({"error": "Tenant not found"}, status=404)
    tenant["active"] = False
    return {"suspended": True, "tenant": tenant["id"]}

@admin.put("/tenants/{id}/activate")
def activate_tenant(request):
    """Reactivate a suspended tenant account."""
    tenant = tenants.get(request.params["id"])
    if not tenant:
        return Response.json({"error": "Tenant not found"}, status=404)
    tenant["active"] = True
    return {"activated": True, "tenant": tenant["id"]}


# ===================================================================
# Register Blueprints
# ===================================================================

app.register_blueprint(api)
app.register_blueprint(admin)

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Testing with curl

### Tenant API (requires `X-Tenant-ID` header)

```bash
# Get tenant profile
curl -H "X-Tenant-ID: acme" http://127.0.0.1:8000/api/me

# List tenant users
curl -H "X-Tenant-ID: acme" http://127.0.0.1:8000/api/users

# Create a user (subject to plan limits)
curl -X POST -H "X-Tenant-ID: acme" \
  -H "Content-Type: application/json" \
  -d '{"name": "Diana", "role": "member"}' \
  http://127.0.0.1:8000/api/users

# Missing header returns 400
curl http://127.0.0.1:8000/api/me
```

### Admin API (no tenant header needed)

```bash
# List all tenants
curl http://127.0.0.1:8000/admin/tenants

# Register a new tenant
curl -X POST http://127.0.0.1:8000/admin/tenants \
  -H "Content-Type: application/json" \
  -d '{"id": "initech", "name": "Initech LLC", "plan": "starter"}'

# Suspend a tenant
curl -X PUT http://127.0.0.1:8000/admin/tenants/globex/suspend
```

---

## Key Patterns

### Tenant Identification

The `@app.before_request` middleware extracts the `X-Tenant-ID` header and stores it in `request.context`. All downstream handlers access the tenant from context rather than parsing headers themselves.

In production, you might identify tenants by subdomain (`acme.yourapp.com`), JWT claim, or API key instead.

### Data Isolation

Each tenant has its own key in the `tenant_data` dictionary. Handlers always scope queries by `tenant_id` from the request context, ensuring one tenant cannot access another's data.

### Plan-Based Limits

The `create_user` handler checks the tenant's plan settings before allowing new records. This pattern extends to rate limiting, feature flags, and storage quotas.

---

## Next Steps

- [API Gateway](api-gateway.md) - Add auth, rate limiting, and circuit breaking
- [Event Sourcing](event-sourcing.md) - Event-driven architecture patterns
- [Microservices](../advanced/microservices.md) - Split into separate services
