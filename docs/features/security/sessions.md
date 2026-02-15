---
title: Session Management
description: Secure cookie-based session management in Cello Framework
---

# Session Management

Cello provides secure cookie-based sessions implemented in Rust. Session data is cryptographically signed to prevent tampering, and cookies are configured with secure defaults (`HttpOnly`, `Secure`, `SameSite`).

## Quick Start

```python
from cello import App, SessionConfig

app = App()

config = SessionConfig(
    secret=b"session-secret-minimum-32-bytes-long",
    cookie_name="session_id",
    max_age=86400  # 24 hours
)

app.enable_sessions(config)

@app.post("/login")
def login(request):
    request.session["user_id"] = "123"
    request.session["username"] = "alice"
    return {"logged_in": True}

@app.get("/profile")
def profile(request):
    user_id = request.session.get("user_id")
    if not user_id:
        return Response.json({"error": "Not logged in"}, status=401)
    return {"user_id": user_id}
```

---

## SessionConfig

```python
config = SessionConfig(
    secret=b"session-secret-minimum-32-bytes-long",
    cookie_name="session_id",
    max_age=86400,
    http_only=True,
    secure=True,
    same_site="Lax"
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `secret` | `bytes` | required | Signing secret (minimum 32 bytes) |
| `cookie_name` | `str` | `"session_id"` | Name of the session cookie |
| `max_age` | `int` | `86400` | Session lifetime in seconds (24 hours) |
| `http_only` | `bool` | `True` | Prevent JavaScript access to cookie |
| `secure` | `bool` | `True` | Send cookie only over HTTPS |
| `same_site` | `str` | `"Lax"` | SameSite cookie attribute |

---

## Cookie Security Flags

### HttpOnly

When `http_only=True` (default), the session cookie is inaccessible to JavaScript:

```
Set-Cookie: session_id=abc123; HttpOnly
```

This prevents XSS attacks from stealing session tokens.

### Secure

When `secure=True` (default), the cookie is only sent over HTTPS:

```
Set-Cookie: session_id=abc123; Secure
```

!!! warning "Development Mode"
    Set `secure=False` during local development if you are not using HTTPS. Always set it to `True` in production.

### SameSite

Controls when the cookie is sent with cross-site requests:

| Value | Behavior |
|-------|----------|
| `"Strict"` | Cookie only sent for same-site requests |
| `"Lax"` | Cookie sent for same-site requests and top-level navigation (default) |
| `"None"` | Cookie sent for all requests (requires `Secure=True`) |

```python
# Strict -- maximum security, may break some OAuth flows
config = SessionConfig(
    secret=b"secret-key-minimum-32-bytes-long!",
    same_site="Strict"
)

# Lax -- good balance of security and usability (default)
config = SessionConfig(
    secret=b"secret-key-minimum-32-bytes-long!",
    same_site="Lax"
)
```

---

## Reading and Writing Session Data

### Setting Values

```python
@app.post("/login")
def login(request):
    data = request.json()
    user = authenticate(data["username"], data["password"])
    if user:
        request.session["user_id"] = str(user["id"])
        request.session["username"] = user["name"]
        request.session["role"] = user["role"]
        return {"logged_in": True}
    return Response.json({"error": "Invalid credentials"}, status=401)
```

### Reading Values

```python
@app.get("/dashboard")
def dashboard(request):
    user_id = request.session.get("user_id")
    username = request.session.get("username")
    if not user_id:
        return Response.redirect("/login")
    return {"user_id": user_id, "username": username}
```

### Deleting Values

```python
@app.post("/logout")
def logout(request):
    # Clear all session data
    request.session.clear()
    return {"logged_out": True}
```

---

## Session Expiration

Sessions automatically expire after `max_age` seconds:

```python
# Session expires after 1 hour
config = SessionConfig(
    secret=b"secret-key-minimum-32-bytes-long!",
    max_age=3600
)

# Session expires after 7 days
config = SessionConfig(
    secret=b"secret-key-minimum-32-bytes-long!",
    max_age=604800
)
```

After expiration, `request.session` returns an empty dictionary and the client must log in again.

---

## Example: Login/Logout Flow

```python
from cello import App, Response, SessionConfig

app = App()
app.enable_sessions(SessionConfig(
    secret=b"session-secret-minimum-32-bytes-long",
    max_age=86400
))

@app.post("/login")
def login(request):
    data = request.json()
    if data.get("username") == "admin" and data.get("password") == "secret":
        request.session["user_id"] = "1"
        request.session["role"] = "admin"
        return {"logged_in": True}
    return Response.json({"error": "Invalid credentials"}, status=401)

@app.get("/me")
def me(request):
    user_id = request.session.get("user_id")
    if not user_id:
        return Response.json({"error": "Not authenticated"}, status=401)
    return {
        "user_id": user_id,
        "role": request.session.get("role")
    }

@app.post("/logout")
def logout(request):
    request.session.clear()
    return {"logged_out": True}
```

---

## Sessions vs JWT

| Feature | Sessions | JWT |
|---------|----------|-----|
| Storage | Server-side (cookie is just an ID) | Client-side (token contains claims) |
| Stateless | No | Yes |
| Revocation | Immediate (clear session) | Requires blacklist |
| Scalability | Requires shared store for multi-server | No shared state needed |
| Best for | Server-rendered apps, admin panels | APIs, SPAs, mobile apps |

!!! tip "When to Use Sessions"
    Sessions are best for traditional web applications with server-rendered HTML where you need immediate revocation (logout). For stateless APIs serving SPAs or mobile clients, prefer [JWT](jwt.md).

---

## Security Considerations

- Use a strong, random secret (minimum 32 bytes)
- Always enable `HttpOnly` to prevent XSS session theft
- Enable `Secure` in production (HTTPS only)
- Use `SameSite=Lax` or `Strict` to mitigate CSRF
- Set appropriate `max_age` -- shorter is more secure
- Combine with [CSRF protection](csrf.md) for form-based applications
- Store secrets in environment variables

---

## Next Steps

- [CSRF Protection](csrf.md) - Protect session-based forms
- [Authentication](authentication.md) - All authentication methods
- [JWT](jwt.md) - Stateless token authentication
- [Security Headers](headers.md) - Additional browser protections
