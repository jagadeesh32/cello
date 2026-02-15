---
title: Caching Middleware
description: Smart response caching in Cello Framework
---

# Caching Middleware

Cello v0.6.0 introduces smart caching middleware implemented in Rust. It caches HTTP responses in memory with configurable TTL, tag-based invalidation, and ETag support for conditional requests.

## Quick Start

```python
from cello import App

app = App()

# Cache GET responses for 5 minutes
app.enable_caching(ttl=300)
```

---

## How It Works

```
Request arrives
     │
     ▼
  Is method cacheable? (GET/HEAD)
     │
  Yes│  No
     ▼   ▼
  Cache   Skip to handler
  lookup
     │
  Hit│  Miss
     ▼     ▼
  Return   Run handler
  cached   Cache response
  response Return response
```

The middleware stores responses in a Rust `DashMap` (lock-free concurrent HashMap) keyed by the request method, path, and query string.

---

## Configuration

```python
app.enable_caching(
    ttl=300,                              # Default TTL in seconds
    methods=["GET", "HEAD"],              # HTTP methods to cache
    exclude_paths=["/api/realtime", "/ws"] # Paths to skip
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ttl` | `int` | `300` | Time-to-live in seconds |
| `methods` | `list[str]` | `["GET", "HEAD"]` | HTTP methods to cache |
| `exclude_paths` | `list[str]` | `[]` | Paths excluded from caching |

---

## Per-Route Caching with the `@cache` Decorator

Override the default TTL or add cache tags on individual routes:

```python
from cello import App, cache

app = App()
app.enable_caching(ttl=300)

# Cache this route for 1 hour
@app.get("/api/config")
@cache(ttl=3600)
def get_config(request):
    return {"theme": "dark", "language": "en"}

# Cache with tags for targeted invalidation
@app.get("/api/users")
@cache(ttl=600, tags=["users"])
def list_users(request):
    return {"users": ["Alice", "Bob"]}

@app.get("/api/posts")
@cache(ttl=600, tags=["posts"])
def list_posts(request):
    return {"posts": []}
```

The `@cache` decorator sets `X-Cache-TTL` and `X-Cache-Tags` headers that the Rust caching middleware reads.

---

## Cache Invalidation

### Tag-Based Invalidation

Invalidate all cached responses that share a tag:

```python
@app.post("/api/users")
def create_user(request):
    data = request.json()
    # ... create user in database ...

    # Invalidate all responses tagged with "users"
    app.invalidate_cache(tags=["users"])

    return Response.json({"id": 1, **data}, status=201)
```

### Time-Based Expiration

Cached entries automatically expire after their TTL:

```python
# Entries expire after 300 seconds (5 minutes)
app.enable_caching(ttl=300)

# Per-route: this entry expires after 60 seconds
@app.get("/api/stock-price")
@cache(ttl=60)
def stock_price(request):
    return {"price": 142.50}
```

---

## ETag Support

Cello automatically generates ETags for cached responses, enabling conditional requests:

```
# First request -- full response
GET /api/users HTTP/1.1

HTTP/1.1 200 OK
ETag: "a1b2c3d4"
Content-Type: application/json
{"users": [...]}

# Subsequent request -- conditional
GET /api/users HTTP/1.1
If-None-Match: "a1b2c3d4"

HTTP/1.1 304 Not Modified
ETag: "a1b2c3d4"
```

The `304 Not Modified` response saves bandwidth by not re-sending the body.

---

## Cache Headers

Cached responses include diagnostic headers:

| Header | Description |
|--------|-------------|
| `X-Cache` | `HIT` or `MISS` |
| `X-Cache-TTL` | Remaining TTL in seconds |
| `ETag` | Entity tag for conditional requests |
| `Cache-Control` | Standard HTTP cache directives |

---

## What Is Not Cached

The middleware skips caching for:

- Non-GET/HEAD methods (POST, PUT, DELETE, PATCH)
- Paths listed in `exclude_paths`
- Responses with `Cache-Control: no-store` or `no-cache`
- Responses with status codes outside the 2xx range

---

## Example: Full Caching Setup

```python
from cello import App, cache

app = App()
app.enable_caching(
    ttl=300,
    exclude_paths=["/api/auth", "/health", "/metrics"]
)

# Short-lived cache for frequently changing data
@app.get("/api/feed")
@cache(ttl=30, tags=["feed"])
def feed(request):
    return {"posts": get_recent_posts()}

# Long-lived cache for static configuration
@app.get("/api/settings")
@cache(ttl=3600, tags=["settings"])
def settings(request):
    return {"maintenance_mode": False}

# Invalidate on write
@app.post("/api/posts")
def create_post(request):
    data = request.json()
    save_post(data)
    app.invalidate_cache(tags=["feed"])
    return Response.json(data, status=201)
```

---

## Performance

| Operation | Overhead |
|-----------|----------|
| Cache lookup (hit) | ~100ns |
| Cache lookup (miss) | ~100ns |
| Cache store | ~200ns |
| ETag comparison | ~50ns |
| Tag invalidation | ~1us per tag |

!!! tip "Memory Management"
    Cached responses are stored in memory. For applications with many unique URLs, set a reasonable TTL and use `exclude_paths` to avoid caching large or dynamic responses.

---

## Next Steps

- [Middleware Overview](overview.md) - Full middleware system
- [Circuit Breaker](circuit-breaker.md) - Fault tolerance
- [Compression](compression.md) - Compress cached responses
- [Rate Limiting](rate-limiting.md) - Throttle requests
