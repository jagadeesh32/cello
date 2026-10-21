---
title: Session Management
description: Secure cookie-based session management in Cello Framework
---

# Session Management

Cello provides cookie-based sessions implemented in Rust. The cookie holds only a
random 256-bit session id; the data lives server-side (in-memory store, per
process). Cookies use secure defaults (`HttpOnly`, `Secure`, `SameSite=Lax`).

## Quick Start

```python
from cello import App, Response, SessionConfig

app = App()
app.enable_session(SessionConfig(cookie_name="session_id", max_age=86400))

@app.post("/login")
def login(request):
    request.session["user_id"] = "123"      # first login key -> new session id
    request.session["username"] = "alice"
    return {"logged_in": True}

@app.get("/profile")
def profile(request):
    user_id = request.session.get("user_id")
    if not user_id:
        return Response.json({"error": "Not logged in"}, status=401)
    return {"user_id": user_id}

@app.post("/logout")
def logout(request):
    request.session.destroy()               # delete server-side, expire cookie
    return {"logged_out": True}
```

---

## SessionConfig

```python
config = SessionConfig(
    cookie_name="session_id",
    cookie_path="/",
    cookie_domain=None,
    cookie_secure=True,
    cookie_http_only=True,
    cookie_same_site="Lax",
    max_age=86400,
    max_sessions=100_000,
    max_session_size=64 * 1024,
    auth_keys=None,
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `cookie_name` | `str` | `"session_id"` | Name of the session cookie |
| `cookie_path` | `str` | `"/"` | Cookie `Path` |
| `cookie_domain` | `str \| None` | `None` | Cookie `Domain` (omit for host-only) |
| `cookie_secure` | `bool` | `True` | Send cookie only over HTTPS |
| `cookie_http_only` | `bool` | `True` | Prevent JavaScript access to cookie |
| `cookie_same_site` | `str` | `"Lax"` | `Strict`, `Lax` or `None` |
| `max_age` | `int` | `86400` | Session lifetime in seconds (sliding: refreshed on each request) |
| `max_sessions` | `int` | `100000` | Sessions kept in memory per process; when full, the least recently used sixteenth is evicted (`0` = unlimited) |
| `max_session_size` | `int` | `65536` | Largest serialized session in bytes; a write past it raises `ValueError` (`0` = unlimited) |
| `auth_keys` | `list[str] \| None` | `None` | Keys whose change issues a new session id. `None` uses `SessionConfig.default_auth_keys()` |

---

## The `request.session` object

`request.session` is available in every handler once sessions are enabled
(`AttributeError` otherwise). It behaves like a dict of JSON-compatible values;
changes are saved when the request finishes.

| Operation | Effect |
|-----------|--------|
| `session[k]`, `session.get(k, default=None)` | Read a value |
| `session[k] = v`, `session.set(k, v)` | Write a value |
| `del session[k]`, `session.pop(k, default=None)` | Remove a value |
| `k in session`, `len(session)`, `session.keys()` | Inspect |
| `session.clear()` | Remove all values (same id) |
| `session.regenerate()` | Issue a new id at the end of the request, keeping the data |
| `session.destroy()` | Delete the session and expire the cookie |
| `session.is_new` | `True` if this request created the session |

Behavior worth knowing (since 1.4.1):

- **Lazy creation.** A new session that nothing wrote to is neither stored nor
  sent as a cookie, so clients without a cookie do not fill the store.
- **Fixation protection.** When a login key appears or changes, the session id
  is replaced automatically. The default keys (`SessionConfig.default_auth_keys()`)
  are `user_id`, `userid`, `uid`, `user`, `username`, `account_id`, `email`,
  `auth`, `authenticated`, `is_authenticated`, `logged_in`, `role` and `roles`.
  Set `auth_keys=[...]` to the keys your app uses. Calling `regenerate()` after
  login is always the reliable choice.
- **Multiple cookies.** Session and CSRF cookies are sent as separate
  `Set-Cookie` headers.
- **Per process.** The in-memory store is not shared between workers, so
  `app.run(workers=N)` with N > 1 refuses to start when sessions are enabled
  (a login on one worker would be unknown to the others). Pass
  `allow_per_worker_state=True` only if every client is pinned to one worker.
  Expired sessions are swept periodically and the store is capped
  (`max_sessions`).

---

## Cookie Security Flags

### HttpOnly

When `cookie_http_only=True` (default), the session cookie is inaccessible to JavaScript:

```
Set-Cookie: session_id=abc123; HttpOnly
```

This prevents XSS attacks from stealing session tokens.

### Secure

When `cookie_secure=True` (default), the cookie is only sent over HTTPS:

```
Set-Cookie: session_id=abc123; Secure
```

!!! warning "Development Mode"
    Set `cookie_secure=False` during local development if you are not using HTTPS. Always set it to `True` in production.

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
    cookie_same_site="Strict"
)

# Lax -- good balance of security and usability (default)
config = SessionConfig(
    cookie_same_site="Lax"
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
    # Delete the session server-side and expire the cookie
    request.session.destroy()
    return {"logged_out": True}
```

---

## Session Expiration

Sessions automatically expire after `max_age` seconds:

```python
# Session expires after 1 hour
config = SessionConfig(
    max_age=3600
)

# Session expires after 7 days
config = SessionConfig(
    max_age=604800
)
```

After expiration (no request within `max_age`), the old id is unknown, `request.session` starts empty and the client must log in again.

---

## Example: Login/Logout Flow

```python
from cello import App, Response, SessionConfig

app = App()
app.enable_session(SessionConfig(
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
    request.session.destroy()
    return {"logged_out": True}
```

---

## Sessions vs JWT

| Feature | Sessions | JWT |
|---------|----------|-----|
| Storage | Server-side (cookie is just an ID) | Client-side (token contains claims) |
| Stateless | No | Yes |
| Revocation | Immediate (`session.destroy()`) | Requires blacklist |
| Scalability | Requires shared store for multi-server | No shared state needed |
| Best for | Server-rendered apps, admin panels | APIs, SPAs, mobile apps |

!!! tip "When to Use Sessions"
    Sessions are best for traditional web applications with server-rendered HTML where you need immediate revocation (logout). For stateless APIs serving SPAs or mobile clients, prefer [JWT](jwt.md).

---

## Security Considerations

- Session ids are 256-bit random values from the OS RNG; nothing to configure
- Always enable `HttpOnly` to prevent XSS session theft
- Enable `Secure` in production (HTTPS only)
- Use `SameSite=Lax` or `Strict` to mitigate CSRF
- Set appropriate `max_age` -- shorter is more secure
- Combine with [CSRF protection](csrf.md) for form-based applications
- Sessions live in process memory: use one worker, or sticky routing, until a shared store is configured

---

## Next Steps

- [CSRF Protection](csrf.md) - Protect session-based forms
- [Authentication](authentication.md) - All authentication methods
- [JWT](jwt.md) - Stateless token authentication
- [Security Headers](headers.md) - Additional browser protections
