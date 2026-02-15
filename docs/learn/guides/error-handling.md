---
title: Error Handling
description: Returning errors, status codes, exception handlers, and RFC 7807 ProblemDetails in Cello
---

# Error Handling

Cello provides multiple layers for dealing with errors: returning explicit error responses from handlers, registering global exception handlers, and using RFC 7807 Problem Details for structured error payloads.

---

## Returning Error Responses

The simplest approach is to return a `Response` with an appropriate HTTP status code directly from the handler.

```python
from cello import App, Response

app = App()

@app.get("/users/{id}")
def get_user(request):
    user_id = request.params["id"]
    user = find_user(user_id)

    if user is None:
        return Response.json(
            {"error": "User not found", "id": user_id},
            status=404,
        )

    return user
```

### Common Status Codes

| Code | Meaning | When to Use |
|------|---------|-------------|
| `400` | Bad Request | Missing or invalid input |
| `401` | Unauthorized | Missing or invalid authentication |
| `403` | Forbidden | Authenticated but lacking permissions |
| `404` | Not Found | Resource does not exist |
| `409` | Conflict | Duplicate resource (e.g., email already taken) |
| `422` | Unprocessable Entity | Validation failure on well-formed input |
| `429` | Too Many Requests | Rate limit exceeded |
| `500` | Internal Server Error | Unexpected server-side failure |

---

## Exception Handlers

Register global exception handlers with the `@app.exception_handler` decorator. When a handler raises an exception of the registered type, Cello catches it and calls your handler instead of returning a raw 500.

```python
@app.exception_handler(ValueError)
def handle_value_error(request, exc):
    return Response.json(
        {"error": "Invalid value", "detail": str(exc)},
        status=400,
    )

@app.exception_handler(PermissionError)
def handle_permission_error(request, exc):
    return Response.json(
        {"error": "Forbidden", "detail": str(exc)},
        status=403,
    )
```

### Catch-All Handler

Register a handler for `Exception` as a safety net. This should always return a generic message to avoid leaking internal details.

```python
@app.exception_handler(Exception)
def handle_unexpected(request, exc):
    import logging
    logging.exception("Unhandled error")
    return Response.json(
        {"error": "Internal server error"},
        status=500,
    )
```

!!! warning
    Exception handlers are matched from most specific to least specific. Register the `Exception` catch-all last.

---

## RFC 7807 Problem Details

[RFC 7807](https://datatracker.ietf.org/doc/html/rfc7807) defines a standard JSON format for HTTP error responses. Using it makes your API errors machine-readable and consistent.

### Format

```json
{
    "type": "https://api.example.com/errors/not-found",
    "title": "Resource Not Found",
    "status": 404,
    "detail": "User with ID 42 does not exist",
    "instance": "/users/42"
}
```

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `type` | Yes | URI identifying the error type |
| `title` | Yes | Short human-readable summary |
| `status` | Yes | HTTP status code |
| `detail` | No | Longer explanation specific to this occurrence |
| `instance` | No | URI of the request that caused the error |

### Using ProblemDetails in Cello

```python
from cello import App, Response

app = App()

def problem(type_url: str, title: str, status: int,
            detail: str = None, instance: str = None, **extra) -> Response:
    """Helper to build an RFC 7807 response."""
    body = {"type": type_url, "title": title, "status": status}
    if detail:
        body["detail"] = detail
    if instance:
        body["instance"] = instance
    body.update(extra)
    resp = Response.json(body, status=status)
    resp.set_header("Content-Type", "application/problem+json")
    return resp
```

Use the helper in handlers and exception handlers:

```python
@app.get("/users/{id}")
def get_user(request):
    user = find_user(request.params["id"])
    if not user:
        return problem(
            type_url="/errors/not-found",
            title="User Not Found",
            status=404,
            detail=f"No user with ID {request.params['id']}",
            instance=request.path,
        )
    return user

@app.exception_handler(ValueError)
def handle_validation(request, exc):
    return problem(
        type_url="/errors/validation",
        title="Validation Error",
        status=400,
        detail=str(exc),
        instance=request.path,
    )
```

---

## Validation Errors

When using DTO validation with Pydantic models, Cello automatically returns a `422` response with a list of field-level errors.

```python
from pydantic import BaseModel

class CreateUser(BaseModel):
    name: str
    email: str
    age: int

@app.post("/users")
def create_user(request, body: CreateUser):
    return {"name": body.name, "email": body.email}
```

If the request body is invalid, Cello responds with:

```json
{
    "detail": [
        {
            "loc": ["age"],
            "msg": "value is not a valid integer",
            "type": "type_error.integer"
        }
    ]
}
```

---

## Custom Error Types

Define application-specific exceptions and register handlers for them.

```python
class NotFoundError(Exception):
    def __init__(self, resource: str, resource_id: str):
        self.resource = resource
        self.resource_id = resource_id
        super().__init__(f"{resource} {resource_id} not found")

class ConflictError(Exception):
    def __init__(self, message: str):
        super().__init__(message)

@app.exception_handler(NotFoundError)
def handle_not_found(request, exc):
    return problem(
        type_url="/errors/not-found",
        title=f"{exc.resource} Not Found",
        status=404,
        detail=str(exc),
        instance=request.path,
    )

@app.exception_handler(ConflictError)
def handle_conflict(request, exc):
    return problem(
        type_url="/errors/conflict",
        title="Conflict",
        status=409,
        detail=str(exc),
    )
```

Then raise them in handlers:

```python
@app.get("/users/{id}")
def get_user(request):
    user = find_user(request.params["id"])
    if not user:
        raise NotFoundError("User", request.params["id"])
    return user
```

---

## Guard Errors

When a [Guard](../../reference/api/guards.md) denies access, it raises `GuardError` (or a subclass). Cello converts these into `401` or `403` responses automatically. You can customize the format:

```python
from cello.guards import GuardError

@app.exception_handler(GuardError)
def handle_guard_error(request, exc):
    return problem(
        type_url="/errors/access-denied",
        title="Access Denied",
        status=exc.status_code,
        detail=exc.message,
    )
```

---

## Summary

| Approach | Best For |
|----------|----------|
| `Response.json(..., status=4xx)` | Simple, inline error returns |
| `@app.exception_handler` | Centralizing error formatting |
| RFC 7807 Problem Details | Machine-readable, standardized errors |
| Custom exceptions | Domain-specific error semantics |
| DTO validation | Automatic input validation with Pydantic |
