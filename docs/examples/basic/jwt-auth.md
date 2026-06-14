---
title: JWT Authentication
description: Secure routes with JSON Web Tokens
tags:
  - JWT
  - Authentication
  - Security
  - Token
  - Authorization
  - Examples
---

# :material-shield-key: JWT Authentication

Learn how to protect your Cello API routes using JSON Web Tokens (JWT). This example demonstrates token generation with PyJWT, a reusable `@requires_auth` decorator, a protected `/me` endpoint that returns the authenticated user's profile, and proper `401 Unauthorized` error handling for missing or invalid tokens.

## Complete Example

```python
import time
import jwt
from cello import Cello, Request, Response
from functools import wraps

app = Cello()

# -------------------------------------------------------------------
# Configuration
# -------------------------------------------------------------------
SECRET_KEY = "your-super-secret-key"  # In production, load from env
ALGORITHM = "HS256"
TOKEN_EXPIRY_SECONDS = 3600  # 1 hour


# -------------------------------------------------------------------
# Token helpers
# -------------------------------------------------------------------
def generate_token(user_id: int, username: str) -> str:
    """Create a signed JWT for the given user."""
    payload = {
        "sub": user_id,
        "username": username,
        "iat": int(time.time()),
        "exp": int(time.time()) + TOKEN_EXPIRY_SECONDS,
    }
    return jwt.encode(payload, SECRET_KEY, algorithm=ALGORITHM)


def decode_token(token: str) -> dict:
    """Decode and validate a JWT, raising on failure."""
    return jwt.decode(token, SECRET_KEY, algorithms=[ALGORITHM])


# -------------------------------------------------------------------
# Auth decorator
# -------------------------------------------------------------------
def requires_auth(handler):
    """Route guard: reject requests missing a valid Bearer token."""
    @wraps(handler)
    async def wrapper(request: Request, *args, **kwargs):
        auth_header = request.headers.get("Authorization", "")

        if not auth_header.startswith("Bearer "):
            return Response.json(
                {"error": "Missing or malformed Authorization header"},
                status=401,
            )

        token = auth_header.removeprefix("Bearer ").strip()

        try:
            payload = decode_token(token)
        except jwt.ExpiredSignatureError:
            return Response.json({"error": "Token has expired"}, status=401)
        except jwt.InvalidTokenError:
            return Response.json({"error": "Invalid token"}, status=401)

        # Attach decoded claims to the request for downstream handlers
        request.state.user = payload
        return await handler(request, *args, **kwargs)

    return wrapper


# -------------------------------------------------------------------
# Routes
# -------------------------------------------------------------------
@app.post("/login")
async def login(request: Request):
    """Issue a JWT in exchange for valid credentials."""
    body = await request.json()
    username = body.get("username")
    password = body.get("password")

    # Replace with a real user-lookup / password-hash check
    if username == "alice" and password == "secret":
        token = generate_token(user_id=1, username="alice")
        return Response.json({
            "access_token": token,
            "token_type": "bearer",
            "expires_in": TOKEN_EXPIRY_SECONDS,
        })

    return Response.json({"error": "Invalid credentials"}, status=401)


@app.get("/me")
@requires_auth
async def get_profile(request: Request):
    """Return the authenticated user's profile (protected endpoint)."""
    user = request.state.user
    return Response.json({
        "id": user["sub"],
        "username": user["username"],
        "token_issued_at": user["iat"],
        "token_expires_at": user["exp"],
    })


@app.get("/public")
async def public_route(request: Request):
    """An unprotected route — no token required."""
    return Response.json({"message": "Anyone can read this!"})


# -------------------------------------------------------------------
# Entry point
# -------------------------------------------------------------------
if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **`generate_token`** — builds a JWT payload with `sub` (subject/user ID), `iat` (issued-at), and `exp` (expiry) claims, then signs it with `HS256`.
- **`decode_token`** — verifies the signature and expiry in one call; raises typed exceptions (`ExpiredSignatureError`, `InvalidTokenError`) so errors can be reported precisely.
- **`@requires_auth` decorator** — a reusable guard that extracts the `Bearer` token from the `Authorization` header, validates it, and attaches the decoded claims to `request.state.user` before calling the real handler.
- **`401 Unauthorized`** — returned for three distinct cases: missing/malformed header, expired token, and invalid signature. Each returns a descriptive JSON error body.
- **`request.state`** — Cello's per-request state bag, used here to pass the decoded JWT payload from the decorator to the handler without re-decoding.
- **`@wraps(handler)`** — preserves the original function's name and docstring so Cello's router introspection still works correctly.

## Running This Example

```bash
# Install dependencies
pip install cello pyjwt

# Start the server
python examples/basic/jwt_auth.py
```

Then exercise the endpoints:

```bash
# 1. Get a token
curl -s -X POST http://localhost:8000/login \
  -H "Content-Type: application/json" \
  -d '{"username": "alice", "password": "secret"}' | jq .

# 2. Access the protected profile endpoint
TOKEN="<paste access_token here>"
curl -s http://localhost:8000/me \
  -H "Authorization: Bearer $TOKEN" | jq .

# 3. Try without a token — expect 401
curl -s http://localhost:8000/me | jq .
```
