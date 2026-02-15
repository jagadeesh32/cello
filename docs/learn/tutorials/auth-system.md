---
title: "Tutorial: Authentication System"
description: Step-by-step guide to building a JWT authentication system with Cello
---

# Tutorial: Authentication System

In this tutorial you will build a complete JWT-based authentication system. You will learn how to configure JWT middleware, create login and registration endpoints, protect routes with tokens, implement token refresh, handle logout with a token blacklist, and add role-based access control.

---

## Prerequisites

- Python 3.12 or later
- Cello installed (`pip install cello-framework`)

---

## Step 1: Project Setup

```bash
mkdir auth-demo && cd auth-demo
python -m venv .venv
source .venv/bin/activate
pip install cello-framework
touch app.py
```

---

## Step 2: Configure JWT

Initialize the application and enable JWT authentication.

```python
from cello import App, Response, JwtConfig
from cello.guards import Role, Authenticated

app = App()

# Configure JWT middleware
jwt_config = JwtConfig(
    secret="your-secret-key-change-in-production",
    algorithm="HS256",
    expiration=3600,          # Access tokens expire in 1 hour
    issuer="auth-demo",
    header_name="Authorization",
    header_prefix="Bearer",
)
app.enable_jwt(jwt_config)
```

!!! warning
    Never hard-code secrets in production. Use environment variables or a secrets manager.

---

## Step 3: In-Memory User Store

For this tutorial we use a simple dictionary. Replace this with a real database in production.

```python
import hashlib
import time

users_db = {}
refresh_tokens = {}   # token -> {"user_id": str, "expires": float}
blacklisted_tokens = set()

def hash_password(password: str) -> str:
    return hashlib.sha256(password.encode()).hexdigest()

def verify_password(password: str, hashed: str) -> bool:
    return hash_password(password) == hashed
```

---

## Step 4: Registration Endpoint

```python
@app.post("/auth/register")
def register(request):
    """Register a new user account."""
    data = request.json()

    email = data.get("email", "").strip().lower()
    password = data.get("password", "")
    name = data.get("name", "")

    if not email or not password:
        return Response.json({"error": "Email and password are required"}, status=400)

    if email in users_db:
        return Response.json({"error": "Email already registered"}, status=409)

    users_db[email] = {
        "email": email,
        "name": name,
        "password": hash_password(password),
        "roles": ["user"],
    }

    return Response.json({"message": "Registration successful", "email": email}, status=201)
```

---

## Step 5: Login Endpoint

On successful login, return both an access token and a refresh token.

```python
import secrets

@app.post("/auth/login")
def login(request):
    """Authenticate and return JWT tokens."""
    data = request.json()
    email = data.get("email", "").strip().lower()
    password = data.get("password", "")

    user = users_db.get(email)
    if not user or not verify_password(password, user["password"]):
        return Response.json({"error": "Invalid credentials"}, status=401)

    # Build the JWT payload
    now = time.time()
    payload = {
        "sub": email,
        "name": user["name"],
        "roles": user["roles"],
        "iat": int(now),
        "exp": int(now + jwt_config.expiration),
    }

    access_token = app.create_jwt(payload)

    # Create a refresh token (opaque string)
    refresh_token = secrets.token_urlsafe(48)
    refresh_tokens[refresh_token] = {
        "user_id": email,
        "expires": now + 86400 * 7,  # 7 days
    }

    return {
        "access_token": access_token,
        "refresh_token": refresh_token,
        "token_type": "Bearer",
        "expires_in": jwt_config.expiration,
    }
```

---

## Step 6: Protected Routes

Use the `guards` parameter to require authentication on specific routes.

```python
@app.get("/me", guards=[Authenticated()])
def get_profile(request):
    """Return the authenticated user's profile."""
    user_data = request.context.get("user", {})
    email = user_data.get("sub")
    user = users_db.get(email, {})

    return {
        "email": email,
        "name": user.get("name"),
        "roles": user.get("roles", []),
    }
```

Any request to `/me` without a valid `Authorization: Bearer <token>` header will receive a `401 Unauthorized` response.

---

## Step 7: Token Refresh

Allow clients to obtain a new access token using a valid refresh token.

```python
@app.post("/auth/refresh")
def refresh(request):
    """Issue a new access token using a refresh token."""
    data = request.json()
    token = data.get("refresh_token", "")

    entry = refresh_tokens.get(token)
    if not entry or entry["expires"] < time.time():
        refresh_tokens.pop(token, None)
        return Response.json({"error": "Invalid or expired refresh token"}, status=401)

    email = entry["user_id"]
    user = users_db.get(email)
    if not user:
        return Response.json({"error": "User not found"}, status=404)

    now = time.time()
    payload = {
        "sub": email,
        "name": user["name"],
        "roles": user["roles"],
        "iat": int(now),
        "exp": int(now + jwt_config.expiration),
    }

    new_access_token = app.create_jwt(payload)

    return {
        "access_token": new_access_token,
        "token_type": "Bearer",
        "expires_in": jwt_config.expiration,
    }
```

---

## Step 8: Logout and Token Blacklisting

Invalidate the current access token and its associated refresh token.

```python
@app.post("/auth/logout", guards=[Authenticated()])
def logout(request):
    """Invalidate the current tokens."""
    # Blacklist the access token
    token = request.get_header("Authorization", "").replace("Bearer ", "")
    blacklisted_tokens.add(token)

    # Revoke refresh token if provided
    data = request.json() or {}
    rt = data.get("refresh_token")
    if rt:
        refresh_tokens.pop(rt, None)

    return {"message": "Logged out successfully"}
```

!!! tip
    In production, store blacklisted tokens in Redis with a TTL matching the token's remaining lifetime so the set does not grow unbounded.

---

## Step 9: Role-Based Access Control

Restrict certain routes to users with specific roles.

```python
@app.get("/admin/dashboard", guards=[Role(["admin"])])
def admin_dashboard(request):
    """Admin-only dashboard."""
    return {
        "total_users": len(users_db),
        "active_sessions": len(refresh_tokens),
    }

@app.post("/admin/promote", guards=[Role(["admin"])])
def promote_user(request):
    """Promote a user to admin."""
    data = request.json()
    email = data.get("email", "").strip().lower()

    user = users_db.get(email)
    if not user:
        return Response.json({"error": "User not found"}, status=404)

    if "admin" not in user["roles"]:
        user["roles"].append("admin")

    return {"message": f"{email} promoted to admin", "roles": user["roles"]}
```

---

## Step 10: Run and Test

```python
if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

Test the full authentication flow with curl:

```bash
# Register
curl -X POST http://127.0.0.1:8000/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email": "alice@example.com", "password": "secret123", "name": "Alice"}'

# Login
curl -X POST http://127.0.0.1:8000/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "alice@example.com", "password": "secret123"}'
# Save the access_token from the response

# Access protected route
curl http://127.0.0.1:8000/me \
  -H "Authorization: Bearer <access_token>"

# Refresh the token
curl -X POST http://127.0.0.1:8000/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "<refresh_token>"}'

# Logout
curl -X POST http://127.0.0.1:8000/auth/logout \
  -H "Authorization: Bearer <access_token>" \
  -H "Content-Type: application/json" \
  -d '{"refresh_token": "<refresh_token>"}'
```

---

## Security Checklist

| Concern | Recommendation |
|---------|---------------|
| Secret key | Use a long random string from `secrets.token_urlsafe(64)` |
| Password hashing | Use `bcrypt` or `argon2` instead of SHA-256 in production |
| Token storage | Store refresh tokens in a database, not in memory |
| HTTPS | Always use TLS in production (`TlsConfig`) |
| Token lifetime | Keep access tokens short-lived (15-60 minutes) |
| Rate limiting | Enable `RateLimitConfig` on login endpoints |

---

## Next Steps

- Add [Guards](../../reference/api/guards.md) with `Permission` for fine-grained access control.
- Enable [Security Headers](../../reference/config/security.md) for HSTS, CSP, and other protections.
- See the [Deployment Guide](../guides/deployment.md) for production TLS configuration.
