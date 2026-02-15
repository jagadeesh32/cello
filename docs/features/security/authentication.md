---
title: Authentication
description: Authentication methods in Cello Framework - JWT, Basic Auth, API Key
---

# Authentication

Cello supports three authentication methods, all implemented in Rust with constant-time comparison to prevent timing attacks. Choose the method that fits your use case, or combine multiple methods for layered security.

## Authentication Methods

| Method | Best For | Stateless | Token Location |
|--------|----------|-----------|----------------|
| **JWT** | APIs, SPAs, mobile apps | Yes | `Authorization: Bearer <token>` |
| **Basic Auth** | Internal tools, simple auth | Yes | `Authorization: Basic <base64>` |
| **API Key** | Service-to-service, third-party access | Yes | Custom header (e.g., `X-API-Key`) |

---

## JWT Authentication

JSON Web Tokens are the recommended authentication method for most APIs. Tokens are validated entirely in Rust using the `jsonwebtoken` crate.

```python
from cello import App, JwtConfig
from cello.middleware import JwtAuth

app = App()

jwt_config = JwtConfig(
    secret=b"your-secret-key-minimum-32-bytes-long",
    algorithm="HS256",
    expiration=3600  # 1 hour
)

jwt_auth = JwtAuth(jwt_config)
jwt_auth.skip_path("/login")
jwt_auth.skip_path("/public")

app.use(jwt_auth)

@app.get("/protected")
def protected(request):
    claims = request.context.get("jwt_claims")
    return {"user": claims["sub"]}
```

!!! tip
    For detailed JWT configuration including token creation, refresh, and blacklisting, see the [JWT documentation](jwt.md).

---

## Basic Authentication

HTTP Basic Auth encodes credentials as `username:password` in Base64. Best for simple internal tools or admin panels.

```python
from cello.middleware import BasicAuth

def verify_credentials(username: str, password: str) -> bool:
    """Return True if the credentials are valid."""
    # In production, check against a database with hashed passwords
    return username == "admin" and password == "secret"

app.use(BasicAuth(verify_credentials))

@app.get("/admin")
def admin(request):
    # request.context["user"] contains the authenticated username
    return {"admin": True, "user": request.context.get("user")}
```

The middleware:

1. Extracts the `Authorization: Basic <base64>` header.
2. Decodes the Base64 credentials.
3. Calls your `verify_credentials` function.
4. Returns `401 Unauthorized` if validation fails.

!!! warning "Security"
    Basic Auth transmits credentials in every request (Base64-encoded, not encrypted). Always use HTTPS when using Basic Auth in production.

---

## API Key Authentication

API keys are ideal for service-to-service communication and third-party integrations.

```python
from cello.middleware import ApiKeyAuth

# Define valid API keys mapped to service names
valid_keys = {
    "sk_live_abc123": "payment-service",
    "sk_live_def456": "analytics-service",
    "sk_live_ghi789": "partner-api"
}

api_auth = ApiKeyAuth(
    keys=valid_keys,
    header="X-API-Key"  # Header name to check
)

app.use(api_auth)

@app.get("/api/data")
def get_data(request):
    # request.context["api_key_name"] contains the service name
    service = request.context.get("api_key_name")
    return {"data": [], "requested_by": service}
```

### Request Format

```bash
curl -H "X-API-Key: sk_live_abc123" https://api.example.com/api/data
```

---

## Skip Paths

All authentication middleware supports skipping certain paths (public endpoints, login pages, health checks):

```python
# JWT -- skip specific paths
jwt_auth = JwtAuth(jwt_config)
jwt_auth.skip_path("/login")
jwt_auth.skip_path("/register")
jwt_auth.skip_path("/health")
jwt_auth.skip_path("/docs")
jwt_auth.skip_path("/public")

app.use(jwt_auth)
```

---

## Choosing an Authentication Method

```
Do you need stateless authentication?
  │
  Yes ──→ Do clients have API keys?
  │         │
  │      Yes│  No
  │         ▼    ▼
  │     API Key  JWT
  │
  No ──→ Is this an internal tool?
           │
        Yes│  No
           ▼    ▼
       Basic   Sessions
       Auth    (see Sessions docs)
```

### Common Combinations

```python
# Public API with JWT for users + API keys for services
jwt_auth = JwtAuth(jwt_config)
jwt_auth.skip_path("/public")
jwt_auth.skip_path("/api/v1/webhook")  # Webhook uses API key

api_auth = ApiKeyAuth(keys=valid_keys, header="X-API-Key")

# Apply JWT first, then API key for webhook routes
app.use(jwt_auth)
```

---

## Error Responses

Authentication failures return standard HTTP error responses:

| Status | Meaning | When |
|--------|---------|------|
| `401 Unauthorized` | Missing or invalid credentials | No token, bad token, expired token |
| `403 Forbidden` | Valid credentials but insufficient permissions | Use with [Guards](guards.md) |

```json
{
    "error": "Unauthorized",
    "detail": "Invalid or expired token",
    "status": 401
}
```

---

## Security Best Practices

- Always use HTTPS in production
- Store secrets in environment variables, never in code
- Use strong secrets (minimum 32 bytes)
- Set short token expiration times (1 hour for JWT)
- Implement token refresh for long-lived sessions
- Log authentication failures for security monitoring
- Rate limit authentication endpoints to prevent brute force

---

## Next Steps

- [JWT](jwt.md) - Detailed JWT configuration
- [Sessions](sessions.md) - Cookie-based session management
- [Guards](guards.md) - Authorization and RBAC
- [Security Headers](headers.md) - Protecting against common attacks
