---
title: Error Codes
description: HTTP status codes, RFC 7807 problem details, and common errors in Cello
---

# Error Codes

This reference documents the HTTP status codes used by Cello, the RFC 7807 error response format, and common errors you may encounter.

---

## HTTP Status Codes

### Success (2xx)

| Code | Name | Used By |
|------|------|---------|
| `200` | OK | Default for successful GET, PUT, PATCH, DELETE |
| `201` | Created | Successful resource creation (POST) |
| `204` | No Content | Successful request with no response body |

### Redirection (3xx)

| Code | Name | Used By |
|------|------|---------|
| `301` | Moved Permanently | `Response.redirect(url, status=301)` |
| `302` | Found | `Response.redirect(url)` (default) |
| `304` | Not Modified | ETag middleware when `If-None-Match` matches |

### Client Errors (4xx)

| Code | Name | Triggered By |
|------|------|-------------|
| `400` | Bad Request | Invalid input, malformed JSON |
| `401` | Unauthorized | Missing or invalid JWT token, `UnauthorizedError` from guards |
| `403` | Forbidden | Insufficient permissions, `ForbiddenError` from guards |
| `404` | Not Found | No matching route, resource not found |
| `405` | Method Not Allowed | Route exists but not for this HTTP method |
| `409` | Conflict | Duplicate resource |
| `413` | Payload Too Large | Body exceeds `body_limit` |
| `422` | Unprocessable Entity | Pydantic/DTO validation failure |
| `429` | Too Many Requests | Rate limit exceeded |

### Server Errors (5xx)

| Code | Name | Triggered By |
|------|------|-------------|
| `500` | Internal Server Error | Unhandled exception in handler |
| `503` | Service Unavailable | Circuit breaker open |
| `504` | Gateway Timeout | Request or response timeout exceeded |

---

## RFC 7807 Problem Details

Cello encourages the use of [RFC 7807](https://datatracker.ietf.org/doc/html/rfc7807) for structured error responses. This format is machine-readable and consistent across endpoints.

### Format

```json
{
    "type": "https://api.example.com/errors/validation",
    "title": "Validation Error",
    "status": 400,
    "detail": "The 'email' field is not a valid email address",
    "instance": "/users"
}
```

### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | `string` | Yes | URI identifying the error type (can be relative) |
| `title` | `string` | Yes | Short, human-readable summary |
| `status` | `integer` | Yes | HTTP status code |
| `detail` | `string` | No | Detailed explanation for this specific occurrence |
| `instance` | `string` | No | URI of the request that triggered the error |

Additional fields can be included for domain-specific information (e.g., `errors` array for validation).

---

## Common Error Responses

### Rate Limit Exceeded (429)

```json
{
    "type": "/errors/rate-limit",
    "title": "Rate Limit Exceeded",
    "status": 429,
    "detail": "You have exceeded the rate limit of 100 requests per minute",
    "retry_after": 45
}
```

The response also includes a `Retry-After` header with the number of seconds to wait.

### Validation Error (422)

Returned automatically when a Pydantic model fails validation:

```json
{
    "detail": [
        {
            "loc": ["body", "email"],
            "msg": "value is not a valid email address",
            "type": "value_error.email"
        },
        {
            "loc": ["body", "age"],
            "msg": "ensure this value is greater than 0",
            "type": "value_error.number.not_gt"
        }
    ]
}
```

### Guard Errors (401 / 403)

Returned by Cello's guard system:

```json
{
    "error": "Authentication required"
}
```

```json
{
    "error": "Missing required roles: admin"
}
```

### Route Not Found (404)

When no route matches the request path:

```json
{
    "error": "Not Found",
    "path": "/nonexistent"
}
```

### Circuit Breaker Open (503)

When the circuit breaker is in the open state:

```json
{
    "type": "/errors/circuit-breaker",
    "title": "Service Unavailable",
    "status": 503,
    "detail": "Circuit breaker is open, service temporarily unavailable"
}
```

---

## Built-in Error Classes

Cello ships exception classes that map onto the standard error responses and
work directly with `@app.exception_handler`:

| Class | Status | Meaning |
|-------|--------|---------|
| `ValidationError` | 422 | Request failed validation |
| `NotFoundError` | 404 | Resource does not exist |
| `AuthenticationError` | 401 | Caller not authenticated |
| `AuthorizationError` | 403 | Caller lacks permission |
| `BadRequestError` | 400 | Malformed request |
| `ConflictError` | 409 | State conflict |
| `RateLimitError` | 429 | Rate limit exceeded |
| `TimeoutError` | 504 | Operation timed out |
| `InternalServerError` | 500 | Unexpected failure |

```python
from cello import App, NotFoundError

app = App()

@app.get("/users/{id}")
def get_user(request):
    user = find_user(request.params["id"])
    if user is None:
        raise NotFoundError(f"User {request.params['id']} not found")
    return user
```

Raised built-in errors render as RFC 7807 Problem Details with the matching
status code by default; register an `@app.exception_handler(...)` to customize
the response.

---

## Customizing Error Responses

### Exception Handlers

Register handlers for specific exception types:

```python
@app.exception_handler(ValueError)
def handle_value_error(request, exc):
    return Response.json({
        "type": "/errors/validation",
        "title": "Validation Error",
        "status": 400,
        "detail": str(exc),
    }, status=400)
```

### Guard Error Customization

```python
from cello.guards import GuardError

@app.exception_handler(GuardError)
def handle_guard_error(request, exc):
    return Response.json({
        "type": "/errors/access-denied",
        "title": "Access Denied",
        "status": exc.status_code,
        "detail": exc.message,
    }, status=exc.status_code)
```

---

## Debug vs. Production Errors

| Behavior | Development (`debug=True`) | Production (`debug=False`) |
|----------|----------------------------|----------------------------|
| Error detail | The exception message | Generic message |
| Status code | Accurate | Accurate |
| Problem Details `type` / `title` | Included | Included |
| `exception_type` extension | Included | Hidden |
| Server log | — | The full error, with method and path |

The switch follows `debug`, not `env`: `app.run(debug=False)` (or a production
`env`, whose default is `debug=False`) redacts the details of an **unhandled**
error. The message of such an error is the Python exception's text — and, for a
database failure, a message that can carry constraint names and row values — so
the client gets the RFC 7807 skeleton with `detail` and the extensions dropped,
and the real error is written to the server log as
`[cello] unhandled 500 GET /path: ValueError: ...`.

Only the *default* 5xx response is redacted. A handler you register yourself is
never overridden — it still receives the original error object:

```python
@app.exception_handler(ValueError)
def on_value_error(request, exc):
    log.warning("handler failed: %s", exc)   # the real message
    return {"error": "something went wrong"}  # what the client sees
```

`raise NotFoundError("user 42 missing")` and the other 4xx classes are not
affected: their message is written for the caller and is sent as-is.

---

## See Also

- [Error Handling Guide](../learn/guides/error-handling.md) for patterns and best practices
- [Guards API](api/guards.md) for access control error handling
