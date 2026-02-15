---
title: Security Headers
description: Security headers configuration in Cello Framework - CSP, HSTS, X-Frame-Options
---

# Security Headers

Cello's security headers middleware adds protective HTTP headers to every response. These headers instruct browsers to enable built-in security features that defend against XSS, clickjacking, MIME sniffing, and other common attacks.

## Quick Start

```python
from cello import App

app = App()

# Enable with secure defaults
app.enable_security_headers()
```

This adds the following headers to every response:

```
Content-Security-Policy: default-src 'self'
Strict-Transport-Security: max-age=31536000; includeSubDomains
X-Frame-Options: DENY
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
```

---

## SecurityHeadersConfig

For full control, provide a `SecurityHeadersConfig`:

```python
from cello import App, SecurityHeadersConfig, CSP

app = App()

config = SecurityHeadersConfig(
    csp=CSP(
        default_src=["'self'"],
        script_src=["'self'", "https://cdn.example.com"],
        style_src=["'self'", "'unsafe-inline'"],
        img_src=["'self'", "data:", "https:"],
        connect_src=["'self'", "https://api.example.com"]
    ),
    hsts_max_age=31536000,
    hsts_include_subdomains=True,
    hsts_preload=True,
    x_frame_options="DENY",
    x_content_type_options="nosniff",
    referrer_policy="strict-origin-when-cross-origin",
    permissions_policy="geolocation=(), microphone=()"
)

app.enable_security_headers(config)
```

---

## Content Security Policy (CSP)

CSP controls which resources the browser is allowed to load, preventing XSS and data injection attacks.

### CSP Builder

```python
from cello import CSP

csp = CSP(
    default_src=["'self'"],
    script_src=["'self'", "https://cdn.example.com"],
    style_src=["'self'", "'unsafe-inline'"],
    img_src=["'self'", "data:", "https:"],
    font_src=["'self'", "https://fonts.googleapis.com"],
    connect_src=["'self'", "https://api.example.com"],
    media_src=["'self'"],
    frame_src=["'none'"],
    object_src=["'none'"],
    base_uri=["'self'"]
)
```

### CSP Directives

| Directive | Controls | Example |
|-----------|----------|---------|
| `default_src` | Fallback for all resource types | `["'self'"]` |
| `script_src` | JavaScript sources | `["'self'", "https://cdn.example.com"]` |
| `style_src` | CSS sources | `["'self'", "'unsafe-inline'"]` |
| `img_src` | Image sources | `["'self'", "data:", "https:"]` |
| `font_src` | Font sources | `["'self'", "https://fonts.googleapis.com"]` |
| `connect_src` | Fetch/XHR/WebSocket origins | `["'self'", "https://api.example.com"]` |
| `media_src` | Audio/video sources | `["'self'"]` |
| `frame_src` | Iframe sources | `["'none'"]` |
| `object_src` | Plugin sources (Flash, etc.) | `["'none'"]` |
| `base_uri` | Allowed `<base>` URLs | `["'self'"]` |

### Generated Header

```
Content-Security-Policy: default-src 'self'; script-src 'self' https://cdn.example.com; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' https://api.example.com; frame-src 'none'; object-src 'none'
```

!!! warning "unsafe-inline and unsafe-eval"
    Avoid `'unsafe-inline'` for scripts and `'unsafe-eval'` whenever possible. These directives weaken CSP and allow most XSS attacks. Use nonces or hashes for inline scripts instead.

---

## HTTP Strict Transport Security (HSTS)

HSTS instructs browsers to always use HTTPS for your domain:

```python
config = SecurityHeadersConfig(
    hsts_max_age=31536000,          # 1 year
    hsts_include_subdomains=True,   # Include all subdomains
    hsts_preload=True               # Submit to browser preload lists
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `hsts_max_age` | `int` | `31536000` | Duration in seconds to remember HTTPS preference |
| `hsts_include_subdomains` | `bool` | `True` | Apply to all subdomains |
| `hsts_preload` | `bool` | `True` | Enable HSTS preload list inclusion |

Generated header:

```
Strict-Transport-Security: max-age=31536000; includeSubDomains; preload
```

!!! warning "HSTS Preload"
    Once your domain is on the HSTS preload list, all browsers will refuse HTTP connections to your domain. Only enable preload when you are certain all subdomains support HTTPS.

---

## X-Frame-Options

Prevents your pages from being embedded in iframes (clickjacking protection):

| Value | Behavior |
|-------|----------|
| `"DENY"` | Page cannot be displayed in any iframe |
| `"SAMEORIGIN"` | Page can be displayed in iframes on the same origin |

```python
config = SecurityHeadersConfig(
    x_frame_options="DENY"  # Strictest setting
)
```

---

## X-Content-Type-Options

Prevents browsers from MIME type sniffing, which can lead to XSS:

```
X-Content-Type-Options: nosniff
```

This is always set to `nosniff` and should not be changed.

---

## Referrer-Policy

Controls how much referrer information is sent with requests:

| Value | Behavior |
|-------|----------|
| `"no-referrer"` | Never send referrer |
| `"same-origin"` | Send referrer only for same-origin requests |
| `"strict-origin"` | Send origin only (not path) for cross-origin HTTPS requests |
| `"strict-origin-when-cross-origin"` | Full referrer for same-origin, origin only for cross-origin (default) |

```python
config = SecurityHeadersConfig(
    referrer_policy="strict-origin-when-cross-origin"
)
```

---

## Permissions-Policy

Controls which browser features your application can use:

```python
config = SecurityHeadersConfig(
    permissions_policy="geolocation=(), microphone=(), camera=()"
)
```

Common permissions:

| Feature | Description |
|---------|-------------|
| `geolocation=()` | Disable geolocation API |
| `microphone=()` | Disable microphone access |
| `camera=()` | Disable camera access |
| `payment=()` | Disable Payment Request API |
| `usb=()` | Disable WebUSB API |

---

## Common Configurations

### API Server (JSON only, no browser features)

```python
config = SecurityHeadersConfig(
    csp=CSP(default_src=["'none'"]),
    x_frame_options="DENY",
    hsts_max_age=31536000,
    permissions_policy="geolocation=(), microphone=(), camera=()"
)
```

### Web Application with CDN

```python
config = SecurityHeadersConfig(
    csp=CSP(
        default_src=["'self'"],
        script_src=["'self'", "https://cdn.example.com"],
        style_src=["'self'", "https://cdn.example.com", "'unsafe-inline'"],
        img_src=["'self'", "https://cdn.example.com", "data:"],
        font_src=["'self'", "https://fonts.googleapis.com", "https://fonts.gstatic.com"],
        connect_src=["'self'", "https://api.example.com"]
    ),
    x_frame_options="SAMEORIGIN",
    hsts_max_age=31536000,
    hsts_preload=True
)
```

---

## Header Summary

| Header | Protection Against |
|--------|-------------------|
| `Content-Security-Policy` | XSS, data injection, clickjacking |
| `Strict-Transport-Security` | Protocol downgrade, cookie hijacking |
| `X-Frame-Options` | Clickjacking |
| `X-Content-Type-Options` | MIME type confusion, XSS |
| `Referrer-Policy` | Information leakage via referrer |
| `Permissions-Policy` | Unauthorized browser feature access |

---

## Next Steps

- [Security Overview](overview.md) - Full security reference
- [CSRF Protection](csrf.md) - Cross-Site Request Forgery
- [Authentication](authentication.md) - Auth configuration
- [Guards](guards.md) - Role-based access control
