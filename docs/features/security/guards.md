---
title: Guards (RBAC)
description: Role-based access control with composable guards in Cello Framework
---

# Guards (RBAC)

Guards provide role-based and permission-based access control in Cello. They are composable Python classes that run before your handler to verify authorization. If a guard fails, the request is rejected with a `403 Forbidden` or `401 Unauthorized` response before the handler executes.

## Quick Start

```python
from cello import App
from cello.guards import Role, Permission, Authenticated

app = App()

@app.get("/admin", guards=[Role(["admin"])])
def admin_panel(request):
    return {"admin": True}

@app.post("/articles", guards=[Permission(["articles:write"])])
def create_article(request):
    return {"created": True}

@app.get("/profile", guards=[Authenticated()])
def profile(request):
    return {"user": request.context.get("user")}
```

---

## Built-in Guards

### Authenticated

Ensures a user is present in the request context (i.e., authentication middleware has run):

```python
from cello.guards import Authenticated

@app.get("/dashboard", guards=[Authenticated()])
def dashboard(request):
    user = request.context.get("user")
    return {"user": user}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `user_key` | `str` | `"user"` | Key in `request.context` for the user object |

### Role

Checks if the user has one (or all) of the required roles:

```python
from cello.guards import Role

# User must have the "admin" role
admin_only = Role(["admin"])

# User must have "admin" OR "editor" (any one)
admin_or_editor = Role(["admin", "editor"])

# User must have BOTH "admin" AND "editor"
admin_and_editor = Role(["admin", "editor"], require_all=True)

@app.get("/admin", guards=[admin_only])
def admin(request):
    return {"admin": True}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `roles` | `list[str]` | required | Required role names |
| `require_all` | `bool` | `False` | If `True`, user must have ALL roles |
| `user_key` | `str` | `"user"` | Key in `request.context` for user object |
| `role_key` | `str` | `"roles"` | Key in user object for roles list |

### Permission

Checks if the user has the required permissions:

```python
from cello.guards import Permission

# User must have "articles:write" permission
can_write = Permission(["articles:write"])

# User must have ALL listed permissions (default behavior)
can_manage = Permission(["articles:write", "articles:delete"])

# User must have ANY of the listed permissions
can_access = Permission(["articles:read", "articles:write"], require_all=False)

@app.post("/articles", guards=[can_write])
def create_article(request):
    return {"created": True}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `permissions` | `list[str]` | required | Required permission strings |
| `require_all` | `bool` | `True` | If `True`, user must have ALL permissions |
| `user_key` | `str` | `"user"` | Key in `request.context` for user object |
| `perm_key` | `str` | `"permissions"` | Key in user object for permissions list |

---

## Composable Guards

Guards can be combined using logical operators:

### And

All guards must pass:

```python
from cello.guards import And, Role, Permission

# Must be admin AND have write permission
admin_writer = And([Role(["admin"]), Permission(["write"])])

@app.delete("/data", guards=[admin_writer])
def delete_data(request):
    return {"deleted": True}
```

### Or

At least one guard must pass:

```python
from cello.guards import Or, Role

# Must be admin OR moderator
admin_or_mod = Or([Role(["admin"]), Role(["moderator"])])

@app.post("/moderate", guards=[admin_or_mod])
def moderate(request):
    return {"moderated": True}
```

### Not

Inverts a guard's result:

```python
from cello.guards import Not, Role

# Must NOT be a "banned" user
not_banned = Not(Role(["banned"]))

@app.post("/comment", guards=[not_banned])
def comment(request):
    return {"commented": True}
```

### Complex Compositions

```python
from cello.guards import And, Or, Not, Role, Permission, Authenticated

# (admin OR editor) AND has write permission AND is NOT suspended
complex_guard = And([
    Or([Role(["admin"]), Role(["editor"])]),
    Permission(["write"]),
    Not(Role(["suspended"]))
])

@app.put("/articles/{id}", guards=[complex_guard])
def update_article(request):
    return {"updated": True}
```

---

## verify_guards()

The `verify_guards()` helper runs a list of guards with AND logic:

```python
from cello.guards import verify_guards, Role, Permission

# Equivalent to And([Role(["admin"]), Permission(["write"])])
@app.post("/data", guards=[Role(["admin"]), Permission(["write"])])
def create_data(request):
    # Both guards must pass
    return {"created": True}
```

When multiple guards are passed in the `guards=[]` list, they all must pass (AND logic). Use `Or()` explicitly if you need OR logic.

---

## User Context

Guards expect user data in `request.context`. This is typically set by authentication middleware:

```python
# JWT middleware sets request.context["jwt_claims"]
# which includes "sub", "roles", "permissions", etc.

# To work with guards, your auth middleware should set:
# request.context["user"] = {
#     "id": "123",
#     "roles": ["admin", "editor"],
#     "permissions": ["read", "write", "delete"]
# }
```

### Custom User Key

If your middleware stores user data under a different key:

```python
# If user data is in request.context["current_user"]
admin = Role(["admin"], user_key="current_user")
can_edit = Permission(["edit"], user_key="current_user")
```

---

## Error Handling

Guards raise typed exceptions that produce appropriate HTTP responses:

| Exception | Status Code | When |
|-----------|-------------|------|
| `UnauthorizedError` | `401` | No user found in context (not authenticated) |
| `ForbiddenError` | `403` | User lacks required roles or permissions |

```python
from cello.guards import GuardError, ForbiddenError, UnauthorizedError

# Guards automatically raise these exceptions.
# The framework catches them and returns the appropriate HTTP response.
```

Error response body:

```json
{
    "error": "Forbidden",
    "detail": "Missing required roles: admin",
    "status": 403
}
```

---

## Custom Guards

Create custom guards by subclassing `Guard`:

```python
from cello.guards import Guard, ForbiddenError

class IpWhitelist(Guard):
    """Allow only requests from whitelisted IPs."""

    def __init__(self, allowed_ips: list):
        self.allowed_ips = set(allowed_ips)

    def __call__(self, request):
        client_ip = request.headers.get("x-forwarded-for", "unknown")
        if client_ip not in self.allowed_ips:
            raise ForbiddenError(f"IP {client_ip} is not allowed")
        return True

class TimeBasedGuard(Guard):
    """Allow access only during business hours."""

    def __call__(self, request):
        import datetime
        hour = datetime.datetime.now().hour
        if not (9 <= hour < 17):
            raise ForbiddenError("Access only during business hours (9-17)")
        return True

# Usage
@app.get("/internal", guards=[IpWhitelist(["10.0.0.1", "10.0.0.2"])])
def internal(request):
    return {"internal": True}

@app.post("/batch-job", guards=[TimeBasedGuard()])
def batch_job(request):
    return {"started": True}
```

---

## Full Example

```python
from cello import App, JwtConfig
from cello.middleware import JwtAuth
from cello.guards import Role, Permission, Authenticated, Or

app = App()

# Set up JWT authentication
jwt_config = JwtConfig(secret=b"your-secret-key-minimum-32-bytes-long")
jwt_auth = JwtAuth(jwt_config)
jwt_auth.skip_path("/login")
app.use(jwt_auth)

# Public
@app.post("/login")
def login(request):
    return {"token": "..."}

# Any authenticated user
@app.get("/profile", guards=[Authenticated()])
def profile(request):
    return {"user": request.context.get("user")}

# Admin only
@app.get("/admin", guards=[Role(["admin"])])
def admin(request):
    return {"admin": True}

# Admin or editor
@app.get("/content", guards=[Or([Role(["admin"]), Role(["editor"])])])
def content(request):
    return {"content": []}

# Specific permission
@app.delete("/users/{id}", guards=[Permission(["users:delete"])])
def delete_user(request):
    return {"deleted": request.params["id"]}

app.run()
```

---

## Next Steps

- [Authentication](authentication.md) - Setting up auth middleware
- [JWT](jwt.md) - JWT token configuration
- [Security Overview](overview.md) - Full security reference
