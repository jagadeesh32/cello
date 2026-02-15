---
title: CORS Middleware
description: Cross-Origin Resource Sharing (CORS) configuration in Cello Framework
---

# CORS Middleware

Cross-Origin Resource Sharing (CORS) controls which external domains can make requests to your API. Cello's CORS middleware is implemented in Rust and handles preflight `OPTIONS` requests automatically.

## Quick Start

```python
from cello import App

app = App()

# Allow all origins (development)
app.enable_cors()

# Allow specific origins (production)
app.enable_cors(origins=["https://example.com", "https://app.example.com"])
```

---

## How CORS Works

When a browser makes a cross-origin request, it follows this flow:

```
Browser                         Cello Server
  │                                  │
  │── Preflight OPTIONS ──────────→  │
  │                                  │ ← Rust CORS middleware checks origin
  │←── 200 + CORS headers ────────  │
  │                                  │
  │── Actual GET/POST request ────→  │
  │                                  │ ← CORS headers added to response
  │←── Response + CORS headers ───  │
```

!!! info "Preflight Requests"
    Browsers send a preflight `OPTIONS` request for non-simple requests (those with custom headers, non-standard methods, or JSON content type). Cello handles these automatically in Rust without reaching your Python handler.

---

## Configuration

### Default Configuration

```python
# Allows all origins, all standard methods, and common headers
app.enable_cors()
```

Default behavior:

| Setting | Default |
|---------|---------|
| Origins | `["*"]` (all origins) |
| Methods | `GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD` |
| Headers | `Content-Type, Authorization, Accept, Origin, X-Requested-With` |
| Credentials | `false` |
| Max Age | `86400` seconds (24 hours) |

### Restricted Origins

```python
# Only allow specific domains
app.enable_cors(origins=[
    "https://example.com",
    "https://app.example.com",
    "https://admin.example.com"
])
```

!!! warning "Wildcard and Credentials"
    When `origins` is set to `["*"]`, browsers will not send cookies or authorization headers. If your API requires credentials, you must list specific origins.

---

## Preflight Handling

Cello automatically responds to `OPTIONS` preflight requests in Rust. The response includes:

- `Access-Control-Allow-Origin` -- the matched origin or `*`
- `Access-Control-Allow-Methods` -- allowed HTTP methods
- `Access-Control-Allow-Headers` -- allowed request headers
- `Access-Control-Max-Age` -- how long browsers should cache the preflight result

```
OPTIONS /api/users HTTP/1.1
Origin: https://example.com
Access-Control-Request-Method: POST
Access-Control-Request-Headers: Content-Type, Authorization

HTTP/1.1 200 OK
Access-Control-Allow-Origin: https://example.com
Access-Control-Allow-Methods: GET, POST, PUT, DELETE, PATCH, OPTIONS, HEAD
Access-Control-Allow-Headers: Content-Type, Authorization
Access-Control-Max-Age: 86400
```

---

## Common Patterns

### API with Frontend SPA

```python
app = App()

# Allow the SPA origin
app.enable_cors(origins=["https://myapp.example.com"])

@app.get("/api/data")
def get_data(request):
    return {"items": [1, 2, 3]}
```

### Public API

```python
app = App()

# Allow any origin to call the API
app.enable_cors()

@app.get("/api/public/status")
def status(request):
    return {"status": "ok"}
```

### Multiple Environments

```python
import os

app = App()

if os.environ.get("ENV") == "production":
    app.enable_cors(origins=[
        "https://app.example.com",
        "https://admin.example.com"
    ])
else:
    # Development -- allow everything
    app.enable_cors()
```

---

## Blueprint-Level CORS

Apply CORS to specific route groups using blueprints:

```python
from cello import Blueprint

# Public API -- open CORS
public = Blueprint("/api/public")

# Internal API -- no CORS (same-origin only)
internal = Blueprint("/api/internal")

app.register_blueprint(public)
app.register_blueprint(internal)

# CORS only applies to public routes when configured at the blueprint level
```

---

## Performance

CORS processing in Cello is extremely efficient:

| Operation | Time |
|-----------|------|
| Origin check | ~10ns |
| Preflight response | ~50ns |
| Header injection | ~20ns |

Preflight responses are handled entirely in Rust and never reach your Python handlers.

---

## Next Steps

- [Middleware Overview](overview.md) - Full middleware system
- [Security Headers](../security/headers.md) - Additional security headers
- [Authentication](../security/authentication.md) - Auth middleware
