---
title: Guards API
description: Role-based access control with Guards in Cello
---

# Guards API

Guards provide declarative access control for routes. A guard is a callable that inspects the request and either allows it to proceed or raises an error. Guards are checked before the handler executes.

---

## Importing

Guards are exported directly from `cello` (preferred):

```python
from cello import (
    Guard,
    RoleGuard,
    PermissionGuard,
    Authenticated,
    And,
    Or,
    Not,
    GuardError,
    ForbiddenError,
    UnauthorizedError,
)
```

The legacy import style still works but is no longer preferred:

```python
# Still works -- Role and Permission are the underlying class names
from cello.guards import Role, Permission, Authenticated, And, Or, Not
```

---

## Base Class: `Guard`

All guards extend the `Guard` base class.

```python
class Guard:
    def __call__(self, request) -> bool | None | str:
        raise NotImplementedError
```

A guard must either:

- Return `True` to allow the request.
- Raise `GuardError` (or a subclass) to deny the request.

---

## Built-in Guards

### `Authenticated(user_key="user")`

Ensures the request has an authenticated user in `request.context`.

```python
from cello import Authenticated

@app.get("/me", guards=[Authenticated()])
def profile(request):
    return request.context["user"]
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `user_key` | `str` | `"user"` | Key to look up in `request.context` |

Raises `UnauthorizedError` (HTTP 401) if the user is not present.

---

### `RoleGuard(roles, require_all=False, user_key="user", role_key="roles")`

Checks that the authenticated user has the required roles. Exported as `RoleGuard` from `cello` (underlying class name is `Role` in `cello.guards`).

```python
from cello import RoleGuard

@app.get("/admin", guards=[RoleGuard(["admin"])])
def admin(request):
    return {"admin": True}

# Require ALL roles
@app.get("/super", guards=[RoleGuard(["admin", "superuser"], require_all=True)])
def super_admin(request):
    return {"super": True}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `roles` | `list[str]` | Required | Required role names |
| `require_all` | `bool` | `False` | If `True`, user must have all listed roles. If `False`, any one is sufficient. |
| `user_key` | `str` | `"user"` | Context key for the user object |
| `role_key` | `str` | `"roles"` | Key within the user object containing the roles list |

Raises `UnauthorizedError` (401) if no user, `ForbiddenError` (403) if roles are insufficient.

---

### `PermissionGuard(permissions, require_all=True, user_key="user", perm_key="permissions")`

Checks that the authenticated user has the required permissions. Exported as `PermissionGuard` from `cello` (underlying class name is `Permission` in `cello.guards`).

```python
from cello import PermissionGuard

@app.delete("/users/{id}", guards=[PermissionGuard(["users:delete"])])
def delete_user(request):
    return {"deleted": True}
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `permissions` | `list[str]` | Required | Required permission strings |
| `require_all` | `bool` | `True` | If `True`, user must have all permissions |
| `user_key` | `str` | `"user"` | Context key for the user object |
| `perm_key` | `str` | `"permissions"` | Key within the user object for the permissions list |

---

## Composite Guards

### `And(guards)`

Passes only if **all** guards pass.

```python
from cello import And, Authenticated, RoleGuard

@app.get("/secure", guards=[And([Authenticated(), RoleGuard(["admin"])])])
def secure(request):
    return {"secure": True}
```

### `Or(guards)`

Passes if **any** guard passes.

```python
from cello import Or, RoleGuard

@app.get("/content", guards=[Or([RoleGuard(["editor"]), RoleGuard(["admin"])])])
def content(request):
    return {"content": True}
```

### `Not(guard)`

Inverts a guard. Passes if the inner guard **fails**.

```python
from cello import Not, RoleGuard

@app.get("/public", guards=[Not(RoleGuard(["banned"]))])
def public(request):
    return {"public": True}
```

---

## Custom Guards

Create a custom guard by subclassing `Guard`.

```python
from cello import Guard, ForbiddenError

class IPWhitelist(Guard):
    def __init__(self, allowed_ips: list[str]):
        self.allowed_ips = set(allowed_ips)

    def __call__(self, request):
        client_ip = request.get_header("X-Real-IP") or request.remote_addr
        if client_ip not in self.allowed_ips:
            raise ForbiddenError(f"IP {client_ip} is not whitelisted")
        return True

@app.get("/internal", guards=[IPWhitelist(["10.0.0.1", "10.0.0.2"])])
def internal(request):
    return {"internal": True}
```

---

## Error Classes

### `GuardError(message, status_code=403)`

Base exception for guard failures.

### `ForbiddenError(message)`

Raised when the user is authenticated but lacks permissions. Status code: `403`.

### `UnauthorizedError(message="Authentication required")`

Raised when the user is not authenticated. Status code: `401`.

---

## `verify_guards(guards, request)`

Helper function that runs a list of guards with AND logic. Used internally by Cello when processing the `guards` parameter on route decorators.

```python
from cello.guards import verify_guards, Authenticated, Role

def my_middleware(request):
    verify_guards([Authenticated(), Role(["user"])], request)
```

Raises `GuardError` if any guard fails.

---

## Using Guards on Routes

Pass guards to the route decorator via the `guards` parameter. Guards work with both sync and async handlers.

```python
@app.get("/admin", guards=[RoleGuard(["admin"])])
def admin(request):
    return {"admin": True}

@app.post("/data", guards=[Authenticated(), PermissionGuard(["data:write"])])
def write_data(request):
    return {"written": True}

# Async handlers are also supported
@app.get("/async-data", guards=[Authenticated()])
async def async_data(request):
    return {"data": []}
```

---

## Using Guards on Blueprints

Apply guards at the blueprint level to protect all routes in the group.

```python
from cello import Blueprint, RoleGuard

admin_bp = Blueprint("/admin", guards=[RoleGuard(["admin"])])

@admin_bp.get("/dashboard")
def dashboard(request):
    return {"admin": True}
```

Guards can also be applied to individual blueprint routes via the `guards` parameter on route decorators:

```python
from cello import Blueprint, Authenticated, PermissionGuard

api_bp = Blueprint("/api")

@api_bp.get("/public")
def public(request):
    return {"public": True}

@api_bp.get("/private", guards=[Authenticated()])
def private(request):
    return {"private": True}

@api_bp.post("/data", guards=[PermissionGuard(["data:write"])])
async def create_data(request):
    return {"created": True}
```

---

## Summary

| Guard | Purpose |
|-------|---------|
| `Authenticated()` | User must be present in context |
| `RoleGuard(roles)` | User must have at least one of the roles |
| `PermissionGuard(perms)` | User must have all specified permissions |
| `And(guards)` | All guards must pass |
| `Or(guards)` | At least one guard must pass |
| `Not(guard)` | Guard must fail |
| Custom `Guard` subclass | Application-specific access rules |
