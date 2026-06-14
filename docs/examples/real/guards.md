---
title: Authorization Guards (RBAC)
description: Protect routes with composable role and permission guards, including custom guard classes for advanced access control.
---

# :material-shield-account: Authorization Guards (RBAC)

Cello's guard system lets you declaratively protect any route by attaching one or more guard objects to the `guards=` parameter. Guards are evaluated before the handler runs; if any guard raises a `ForbiddenError` the request is rejected with a 403. Guards compose with `And`, `Or`, and `Not` combinators for expressive, reusable access-control logic.

This example shows how to implement role-based and permission-based guards, combine them with boolean logic, and write a fully custom IP-allowlist guard as a callable class.

## Features Demonstrated

- `Role(["admin"])` guard — restricts access to users with a specific role
- `Permission(["users:write"])` guard — restricts access based on fine-grained permissions
- Stacking multiple guards (implicit `AND`) on a single route
- `Or([...])` combinator — allows access if any one guard passes
- Custom callable guard class (`IPAllowlist`) — arbitrary logic encapsulated as a guard
- `ForbiddenError` — raised inside guards to produce a 403 response
- Mock login endpoint that returns user context consumed by downstream guards

## Complete Source Code

```python
from cello import App, Request
from cello.guards import Role, Permission, Authenticated, And, Or, Not, ForbiddenError

app = App()

@app.post("/login/{username}")
def login(request: Request):
    username = request.params["username"]
    if username == "admin":
        user_data = {"id": 1, "username": "admin", "roles": ["admin"], "permissions": ["users:read", "users:write", "users:delete"]}
    elif username == "mod":
        user_data = {"id": 2, "username": "mod", "roles": ["moderator"], "permissions": ["users:read", "users:write"]}
    else:
        user_data = {"id": 3, "username": "user", "roles": ["user"], "permissions": ["users:read"]}
    return {"token": "mock-token", "user": user_data}

@app.get("/admin", guards=[Role(["admin"])])
def admin_only(request):
    return {"message": "Welcome Admin"}

@app.post("/users", guards=[Permission(["users:write"])])
def create_user(request):
    return {"message": "User created"}

@app.delete("/users/{id}", guards=[Role(["admin"]), Permission(["users:delete"])])
def delete_user(request):
    return {"message": f"User {request.params['id']} deleted"}

@app.get("/reports", guards=[Or([Role(["admin"]), Role(["moderator"])])])
def view_reports(request):
    return {"message": "Reports view"}

class IPAllowlist:
    def __init__(self, allowed_ips):
        self.allowed_ips = allowed_ips
    def __call__(self, request):
        client_ip = request.headers.get("X-Real-IP", "127.0.0.1")
        if client_ip not in self.allowed_ips:
            raise ForbiddenError(f"IP {client_ip} not allowed")
        return True

@app.get("/internal", guards=[IPAllowlist(["127.0.0.1"])])
def internal_api(request):
    return {"message": "Internal API access granted"}

if __name__ == "__main__":
    app.run(port=8080)
```

## Running This Example

```bash
python examples/guards.py
```

```bash
# Log in as admin and capture context
curl -X POST http://localhost:8080/login/admin

# Access admin-only route (succeeds with admin token)
curl http://localhost:8080/admin

# Access admin-only route as regular user (403 Forbidden)
curl http://localhost:8080/admin

# Create a user — requires users:write permission
curl -X POST http://localhost:8080/users

# Delete a user — requires admin role AND users:delete permission
curl -X DELETE http://localhost:8080/users/42

# Reports accessible to both admin and moderator roles
curl http://localhost:8080/reports

# Internal endpoint — accessible only from 127.0.0.1
curl http://localhost:8080/internal

# Simulate a blocked IP
curl -H "X-Real-IP: 10.0.0.1" http://localhost:8080/internal
```

## Key Concepts

- **Guard evaluation order** — guards in the `guards=[]` list are checked left-to-right; the first failure short-circuits and returns 403
- **Implicit AND** — listing multiple guards is equivalent to wrapping them in `And([...])`, requiring all to pass
- **`Or([...])` combinator** — grants access when at least one of the provided guards succeeds, enabling multi-role access to shared endpoints
- **`ForbiddenError`** — the standard way for any guard (built-in or custom) to signal that access should be denied
- **Custom callable guards** — any Python callable that accepts `request` and either returns `True` or raises `ForbiddenError` qualifies as a guard, enabling arbitrary logic (IP filtering, time-of-day restrictions, feature flags, etc.)
- **`Not([...])` combinator** — inverts a guard's result, useful for "deny if role is X" scenarios
- **`Authenticated` guard** — a built-in guard that checks whether the request carries a recognised authentication context
