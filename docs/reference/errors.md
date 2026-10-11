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
| `400` | Bad Request | Invalid input, malformed JSON, `body=Model` validation failure |
| `401` | Unauthorized | Missing or invalid JWT token, `UnauthorizedError` from guards |
| `403` | Forbidden | Insufficient permissions, `ForbiddenError` from guards |
| `404` | Not Found | No matching route, resource not found |
| `405` | Method Not Allowed | Route exists but not for this HTTP method |
| `409` | Conflict | Duplicate resource (your handler decides, for example on `UniqueViolation`) |
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

## Validation Errors for `body=Model` (400)

When a route declares `body=Model` with a Cello ORM model, the request body is
validated natively before the handler runs. A rejected body returns `400` with
`{"detail": [...]}`. Every field is checked and **all** errors are returned, not
just the first. Each item has exactly the keys `loc`, `msg` and `type`, using
the same vocabulary as the Pydantic `body=` path. Input values are never echoed
back, so a rejected password is not reflected.

```json
{
    "detail": [
        {"loc": ["name"], "msg": "String should have at most 5 characters", "type": "string_too_long"},
        {"loc": ["age"], "msg": "Input should be a valid integer, unable to parse string as an integer", "type": "int_parsing"},
        {"loc": ["at"], "msg": "Input should be a valid datetime, unable to parse input", "type": "datetime_parsing"}
    ]
}
```

`loc` is `[field]` (or `[column]` when a ForeignKey's column key was sent), and
`["body"]` for whole-body errors. Unknown keys, `AutoField` and `read_only`
fields are ignored on input rather than rejected.

| `type` | `msg` | When |
|--------|-------|------|
| `missing` | Field required | A required field is absent. |
| `int_type` | Input should be a valid integer | Wrong JSON type for an integer field. |
| `int_parsing` | Input should be a valid integer, unable to parse string as an integer | Non-numeric string for an integer. |
| `int_from_float` | Input should be a valid integer, got a number with a fractional part | `1.5` for an integer. |
| `less_than_equal` | Input should be less than or equal to N | Above the column's range (for example `2147483647` for `INTEGER`). |
| `greater_than_equal` | Input should be greater than or equal to N | Below the column's range. |
| `float_type` / `float_parsing` | Input should be a valid number... | Wrong type / unparsable string for a float. |
| `bool_type` / `bool_parsing` | Input should be a valid boolean... | Wrong type / uninterpretable string (`"maybe"`). |
| `string_type` | Input should be a valid string | Wrong JSON type for a text field, including `null` for a non-null text field. |
| `string_too_long` | String should have at most N character(s) | Longer than `max_length`. |
| `string_null_byte` | String should not contain null characters (U+0000) | U+0000 in a string field or anywhere in a `JSONField` value. Postgres refuses it, so it is a 400 here instead of a database error later. |
| `datetime_type` / `datetime_parsing` | Input should be a valid datetime... | Wrong type / unparsable datetime. |
| `value_error` | Input should not be null | `null` for a non-nullable `JSONField` or custom-typed field. (For other types the type-specific error above is used, for example `string_type`.) |
| `model_type` | Input should be a valid dictionary or instance of Model | The body is valid JSON but not an object (for example `[1]`). |
| `value_error.json` | Invalid JSON body | Malformed JSON, invalid UTF-8, trailing data, or nesting deeper than 128 levels. `loc` is `["body"]`. |

The first thirteen rows come from `src/schema/validate.rs`; `model_type` was
observed from a live app. Coercion follows Pydantic's lax mode for JSON input
(numeric strings for integers, `"true"` for booleans, Unix timestamps and bare
dates for datetimes). See [DTO Validation](../features/advanced/dto-validation.md)
for the full coercion table. Pydantic, dataclass and plain-class `body=` keep
their own messages, and type-hint validation still returns `422`.

---

## Database Errors

Database failures raise `cello.DatabaseError` subclasses (all also
`RuntimeError`): `UniqueViolation`, `NotNullViolation`, `ForeignKeyViolation`,
`CheckViolation`, `DataError`, `OperationalError`, `ProgrammingError` and the
`Parameter*Error` classes. The full tree, SQLSTATE mapping and the
`.sqlstate` / `.detail` / `.constraint` / `.table` / `.column` attributes are in
the [Database API](api/database.md#exception-hierarchy). The ORM's
`DoesNotExist`, `MultipleObjectsReturned` and `FieldError` are described in the
[ORM API](api/orm.md#exceptions).

### What happens when one escapes a handler

Cello adds no automatic mapping from database errors to HTTP statuses. An
unhandled exception in an `async` handler returns `500` with a JSON body that
names the exception class and includes the message when the server runs in
development (debug) mode. Observed with a live app:

```text
GET /dup      (second INSERT violates a unique index)
HTTP/1.1 500 Internal Server Error
{"error":"Async handler error: UniqueViolation: db error: ERROR: duplicate key value violates unique constraint \"ref_users_email_key\"\nDETAIL: Key (email)=(x) already exists.","status":500}

GET /syntax   (malformed SQL)
HTTP/1.1 500 Internal Server Error
{"error":"Async handler error: ProgrammingError: db error: ERROR: syntax error at or near \"selec\"","status":500}

GET /nf       (Model.objects.get() with no match)
HTTP/1.1 500 Internal Server Error
{"error":"Async handler error: DoesNotExist: RefUser matching query does not exist.","status":500}
```

Because the body carries the database's message (constraint names, `DETAIL`
lines with the offending values), do not let database errors escape to clients.
Catch the specific class and return the status you want:

```python
from cello import Response, UniqueViolation

@app.post("/users")
async def create(request):
    try:
        user = await User.objects.create(**request.json())
    except UniqueViolation:
        return Response.json({"error": "email already registered"}, status=409)
    return user

@app.get("/users/{id}")
async def one(request):
    try:
        return await User.objects.get(id=int(request.params["id"]))
    except User.DoesNotExist:
        return Response.json({"error": "Not Found"}, status=404)
```

Suggested mapping (your choice, not framework behaviour):

| Error | Typical response |
|-------|------------------|
| `UniqueViolation` | `409 Conflict` |
| `ForeignKeyViolation`, `NotNullViolation`, `CheckViolation` | `400` or `409`, depending on whether the client caused it |
| `Model.DoesNotExist` | `404` |
| `MultipleObjectsReturned` | `500` (a data or query bug) |
| `FieldError`, `ParameterTypeError`, `ParameterValueError`, `ParameterRangeError` | `400` if the value came from the client, otherwise `500` |
| `OperationalError` | `503`, with a retry hint |
| `ProgrammingError` | `500` (a bug) |

You can also register one handler per class with `@app.exception_handler(...)`
(see below). A queryset that is *returned* from a handler runs after the
handler returns, so its error becomes a `500` the handler cannot catch; use
`await qs.json()` inside the handler to catch it.

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

| Behavior | Development | Production |
|----------|-------------|------------|
| Error detail | Full stack trace | Generic message |
| Status code | Accurate | Accurate |
| Exception type | Included | Hidden |
| Request context | Included | Hidden |

In production (`--env production`), Cello returns a generic `500 Internal Server Error` for unhandled exceptions to avoid leaking implementation details.

!!! note "Database messages never reach clients in production"
    With `debug` off (including `env="production"` / `--env production`), an
    unhandled exception returns only `{"error": "Internal Server Error", "status": 500}`.
    The real error, with the request method and path, is written to the server log.
    This matters most for database errors, whose messages carry constraint names and
    row values (for example `DETAIL: Key (email)=(x) already exists`). In development
    the body describes the error, as in the example above. Catch the exception in the
    handler when you want a specific status (see [Database Errors](#database-errors)).

---

## See Also

- [Error Handling Guide](../learn/guides/error-handling.md) for patterns and best practices
- [Guards API](api/guards.md) for access control error handling
