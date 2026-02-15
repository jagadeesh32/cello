---
title: CSRF Protection
description: Cross-Site Request Forgery protection in Cello Framework
---

# CSRF Protection

Cello provides CSRF (Cross-Site Request Forgery) protection using the double-submit cookie pattern with cryptographically signed tokens. The middleware is implemented in Rust for secure, constant-time token validation.

## Quick Start

```python
from cello import App
from cello.middleware import CsrfConfig

app = App()

config = CsrfConfig(
    secret=b"csrf-secret-minimum-32-bytes-long",
    cookie_name="_csrf",
    header_name="X-CSRF-Token"
)

app.enable_csrf(config)
```

---

## How CSRF Protection Works

The double-submit cookie pattern works as follows:

```
1. Client requests a page (GET)
   ← Server sets a CSRF cookie and provides a token

2. Client submits a form (POST)
   → Sends CSRF cookie (automatic) + CSRF token in header/body
   ← Server verifies cookie matches token

3. Attacker tries cross-site POST
   → Cannot read the CSRF cookie (SameSite/HttpOnly)
   → Cannot provide the matching token
   ← Server rejects the request (403 Forbidden)
```

!!! info "Why Double-Submit?"
    An attacker on a different origin can cause the browser to send cookies, but cannot read them. By requiring the token to also appear in the request body or header, the server ensures the request originated from a page that could read the cookie.

---

## CsrfConfig

```python
config = CsrfConfig(
    secret=b"csrf-secret-minimum-32-bytes-long",
    cookie_name="_csrf",
    header_name="X-CSRF-Token",
    safe_methods=["GET", "HEAD", "OPTIONS"]
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `secret` | `bytes` | required | Signing secret for token generation |
| `cookie_name` | `str` | `"_csrf"` | Name of the CSRF cookie |
| `header_name` | `str` | `"X-CSRF-Token"` | Header name for token submission |
| `safe_methods` | `list[str]` | `["GET", "HEAD", "OPTIONS"]` | Methods that skip CSRF validation |

---

## Safe Methods

By default, `GET`, `HEAD`, and `OPTIONS` requests are considered safe and do not require a CSRF token. Only state-changing methods (`POST`, `PUT`, `DELETE`, `PATCH`) are validated.

---

## HTML Form Protection

Include the CSRF token as a hidden field in HTML forms:

```python
@app.get("/form")
def get_form(request):
    csrf_token = request.csrf_token
    return Response.html(f'''
        <form method="POST" action="/submit">
            <input type="hidden" name="_csrf" value="{csrf_token}">
            <label>Name: <input type="text" name="name"></label>
            <button type="submit">Submit</button>
        </form>
    ''')

@app.post("/submit")
def submit_form(request):
    # CSRF token is validated automatically by middleware
    data = request.form()
    return {"submitted": data.get("name")}
```

---

## JavaScript / SPA Protection

For single-page applications, send the CSRF token in a request header:

```javascript
// Read the token from the cookie
function getCsrfToken() {
    const match = document.cookie.match(/(?:^|;\s*)_csrf=([^;]*)/);
    return match ? decodeURIComponent(match[1]) : '';
}

// Include in fetch requests
fetch('/api/data', {
    method: 'POST',
    headers: {
        'Content-Type': 'application/json',
        'X-CSRF-Token': getCsrfToken()
    },
    body: JSON.stringify({ key: 'value' })
});
```

---

## Exempt Paths

Exclude specific paths from CSRF validation (e.g., API endpoints that use JWT or webhook receivers):

```python
config = CsrfConfig(
    secret=b"csrf-secret-minimum-32-bytes-long",
    exempt_paths=[
        "/api/webhook",      # External webhook (uses API key)
        "/api/v1/",          # API routes (use JWT)
    ]
)

app.enable_csrf(config)
```

!!! warning
    Only exempt paths that have their own authentication mechanism (JWT, API key). Never exempt form submission endpoints.

---

## Token Lifecycle

1. **Generation**: A signed CSRF token is generated when the client first visits the site.
2. **Cookie**: The token is stored in a cookie (default: `_csrf`).
3. **Submission**: The client includes the token in the request header or body.
4. **Validation**: The Rust middleware verifies the submitted token matches the cookie using constant-time comparison.
5. **Rotation**: A new token can be generated after each successful validation for added security.

---

## Error Responses

When CSRF validation fails, the middleware returns:

```
HTTP/1.1 403 Forbidden
Content-Type: application/json

{"error": "CSRF token validation failed"}
```

Common causes:

| Error | Cause |
|-------|-------|
| Missing token | Form does not include the hidden `_csrf` field |
| Token mismatch | Token in header/body does not match cookie |
| Expired token | Token has expired (if expiration is configured) |
| Missing cookie | Cookie was cleared or blocked |

---

## Combining with Sessions

CSRF protection is typically used with session-based authentication:

```python
from cello import App, SessionConfig
from cello.middleware import CsrfConfig

app = App()

# Enable sessions
app.enable_sessions(SessionConfig(
    secret=b"session-secret-minimum-32-bytes-long",
    max_age=86400
))

# Enable CSRF protection
app.enable_csrf(CsrfConfig(
    secret=b"csrf-secret-minimum-32-bytes-long"
))

@app.get("/login")
def login_form(request):
    csrf_token = request.csrf_token
    return Response.html(f'''
        <form method="POST" action="/login">
            <input type="hidden" name="_csrf" value="{csrf_token}">
            <input type="text" name="username" placeholder="Username">
            <input type="password" name="password" placeholder="Password">
            <button type="submit">Log In</button>
        </form>
    ''')

@app.post("/login")
def login(request):
    form = request.form()
    # CSRF validated automatically, session set on success
    request.session["user"] = form.get("username")
    return Response.redirect("/dashboard")
```

---

## Security Considerations

- Use a strong, random secret (minimum 32 bytes, different from session secret)
- Always use HTTPS in production so cookies are not intercepted
- Set `SameSite=Lax` or `Strict` on session cookies for additional protection
- Do not exempt form submission endpoints from CSRF
- Rotate tokens after each use for maximum security

---

## Next Steps

- [Sessions](sessions.md) - Session management for stateful apps
- [Security Headers](headers.md) - Additional browser protections
- [Authentication](authentication.md) - Authentication methods
- [Security Overview](overview.md) - Full security reference
