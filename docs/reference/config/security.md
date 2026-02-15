---
title: Security Configuration
description: Configuration classes for JWT, sessions, security headers, rate limiting, and CSP
---

# Security Configuration

This reference covers all security-related configuration classes in Cello: JWT authentication, session management, security headers, rate limiting, and Content Security Policy.

---

## JwtConfig

Configure JWT (JSON Web Token) authentication.

```python
from cello import JwtConfig

config = JwtConfig(
    secret="your-secret-key",
    algorithm="HS256",
    expiration=3600,
    issuer="my-app",
    header_name="Authorization",
    header_prefix="Bearer",
)
app.enable_jwt(config)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `secret` | `str` | Required | HMAC secret or RSA/EC key |
| `algorithm` | `str` | `"HS256"` | Signing algorithm (`HS256`, `HS384`, `HS512`, `RS256`, `RS384`, `RS512`, `ES256`, `ES384`) |
| `expiration` | `int` | `3600` | Token lifetime in seconds |
| `issuer` | `str` | `None` | Expected `iss` claim value |
| `header_name` | `str` | `"Authorization"` | Request header containing the token |
| `header_prefix` | `str` | `"Bearer"` | Prefix before the token value |

---

## SessionConfig

Configure cookie-based session management.

```python
from cello import SessionConfig

config = SessionConfig(
    secret="session-secret-key",
    cookie_name="session_id",
    max_age=86400,
    httponly=True,
    secure=True,
    same_site="Lax",
)
app.enable_session(config)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `secret` | `str` | Required | Secret key for signing session cookies |
| `cookie_name` | `str` | `"session_id"` | Name of the session cookie |
| `max_age` | `int` | `86400` | Cookie lifetime in seconds (24 hours) |
| `httponly` | `bool` | `True` | Prevent JavaScript access to the cookie |
| `secure` | `bool` | `False` | Require HTTPS for the cookie |
| `same_site` | `str` | `"Lax"` | SameSite attribute (`"Strict"`, `"Lax"`, `"None"`) |
| `path` | `str` | `"/"` | Cookie path |
| `domain` | `str` | `None` | Cookie domain |

---

## SecurityHeadersConfig

Configure security response headers.

```python
from cello import SecurityHeadersConfig

config = SecurityHeadersConfig(
    hsts=True,
    hsts_max_age=31536000,
    hsts_include_subdomains=True,
    hsts_preload=False,
    x_frame_options="DENY",
    x_content_type_options=True,
    x_xss_protection=True,
    referrer_policy="strict-origin-when-cross-origin",
)
app.enable_security_headers(config)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `hsts` | `bool` | `True` | Enable `Strict-Transport-Security` header |
| `hsts_max_age` | `int` | `31536000` | HSTS max-age in seconds (1 year) |
| `hsts_include_subdomains` | `bool` | `True` | Include subdomains in HSTS |
| `hsts_preload` | `bool` | `False` | Add `preload` directive to HSTS |
| `x_frame_options` | `str` | `"DENY"` | `"DENY"`, `"SAMEORIGIN"`, or `None` |
| `x_content_type_options` | `bool` | `True` | Add `X-Content-Type-Options: nosniff` |
| `x_xss_protection` | `bool` | `True` | Add `X-XSS-Protection: 1; mode=block` |
| `referrer_policy` | `str` | `"strict-origin-when-cross-origin"` | Referrer-Policy header value |

---

## RateLimitConfig

Configure rate limiting with different algorithms.

### Token Bucket

```python
from cello import RateLimitConfig

config = RateLimitConfig.token_bucket(
    requests=100,
    window=60,
)
app.enable_rate_limit(config)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `requests` | `int` | Required | Maximum requests per window |
| `window` | `int` | Required | Time window in seconds |

### Sliding Window

```python
config = RateLimitConfig.sliding_window(
    requests=100,
    window=60,
)
```

### Adaptive Rate Limiting

Dynamically adjusts limits based on server load.

```python
config = RateLimitConfig.adaptive(
    base_requests=100,
    window=60,
    cpu_threshold=0.8,
    memory_threshold=0.9,
    latency_threshold=100,
    min_requests=10,
)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_requests` | `int` | Required | Base requests per window at normal load |
| `window` | `int` | Required | Time window in seconds |
| `cpu_threshold` | `float` | `0.8` | Reduce limits above this CPU usage (0.0-1.0) |
| `memory_threshold` | `float` | `0.9` | Reduce limits above this memory usage |
| `latency_threshold` | `int` | `100` | Reduce limits above this latency in ms |
| `min_requests` | `int` | `10` | Minimum requests per window (floor) |

---

## CSP (Content Security Policy)

Build a Content Security Policy header.

```python
from cello import CSP

csp = CSP()
csp.default_src("'self'")
csp.script_src("'self'", "https://cdn.example.com")
csp.style_src("'self'", "'unsafe-inline'")
csp.img_src("'self'", "data:", "https:")
csp.connect_src("'self'", "https://api.example.com")
csp.font_src("'self'", "https://fonts.googleapis.com")
csp.frame_ancestors("'none'")
```

### Directives

| Method | Directive | Description |
|--------|-----------|-------------|
| `default_src(*sources)` | `default-src` | Fallback for other directives |
| `script_src(*sources)` | `script-src` | Allowed script sources |
| `style_src(*sources)` | `style-src` | Allowed stylesheet sources |
| `img_src(*sources)` | `img-src` | Allowed image sources |
| `connect_src(*sources)` | `connect-src` | Allowed XHR/WebSocket origins |
| `font_src(*sources)` | `font-src` | Allowed font sources |
| `media_src(*sources)` | `media-src` | Allowed media sources |
| `object_src(*sources)` | `object-src` | Allowed plugin sources |
| `frame_src(*sources)` | `frame-src` | Allowed iframe sources |
| `frame_ancestors(*sources)` | `frame-ancestors` | Who can embed this page |
| `form_action(*sources)` | `form-action` | Allowed form submission targets |
| `base_uri(*sources)` | `base-uri` | Allowed `<base>` element URIs |

---

## Putting It All Together

```python
from cello import App, JwtConfig, SessionConfig, SecurityHeadersConfig, RateLimitConfig, CSP

app = App()

# JWT
app.enable_jwt(JwtConfig(
    secret=os.environ["JWT_SECRET"],
    algorithm="HS256",
    expiration=900,  # 15 minutes
))

# Security headers with CSP
csp = CSP()
csp.default_src("'self'")
csp.script_src("'self'")

app.enable_security_headers(SecurityHeadersConfig(
    hsts=True,
    x_frame_options="DENY",
    csp=csp,
))

# Rate limiting
app.enable_rate_limit(RateLimitConfig.token_bucket(requests=100, window=60))

# Sessions (optional, for web apps)
app.enable_session(SessionConfig(
    secret=os.environ["SESSION_SECRET"],
    secure=True,
))
```

---

## Summary

| Config Class | Purpose |
|-------------|---------|
| `JwtConfig` | JWT token authentication |
| `SessionConfig` | Cookie-based sessions |
| `SecurityHeadersConfig` | HSTS, X-Frame-Options, etc. |
| `RateLimitConfig` | Request throttling |
| `CSP` | Content Security Policy builder |
