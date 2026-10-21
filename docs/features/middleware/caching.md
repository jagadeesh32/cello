---
title: Caching Middleware
description: Smart response caching in Cello Framework
---

# Caching Middleware

Cello provides smart caching middleware implemented in Rust. It caches HTTP responses in memory with configurable TTL, tag-based invalidation, and ETag support for conditional requests.

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
    exclude_paths=["/api/realtime", "/ws"], # Paths to skip
    compress=True,                        # gzip HITs for gzip clients
    cache_authenticated=False,            # see "What Is Not Cached"
    max_entries=10000,                    # store cap
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ttl` | `int` | `300` | Server-side time-to-live in seconds |
| `methods` | `list[str]` | `["GET", "HEAD"]` | HTTP methods to cache |
| `exclude_paths` | `list[str]` | `["/health", "/metrics"]` | Paths excluded from caching |
| `compress` | `bool` | `True` | Gzip a HIT for clients that accept gzip |
| `cache_authenticated` | `bool` | `False` | Also cache requests with `Authorization`/`Cookie`; those headers then join the key so users never share an entry |
| `max_entries` | `int` | `10000` | Maximum stored responses (oldest evicted) |

---

## Per-Route Caching with the `@cache` Decorator

Override the default TTL or add cache tags on individual routes. The `@cache` decorator supports both sync and async handlers -- it automatically detects the handler type and wraps it correctly, so async handlers are properly awaited.

```python
from cello import App, cache

app = App()
app.enable_caching(ttl=300)

# Cache this route for 1 hour (sync handler)
@app.get("/api/config")
@cache(ttl=3600)
def get_config(request):
    return {"theme": "dark", "language": "en"}

# Cache with tags for targeted invalidation (sync handler)
@app.get("/api/users")
@cache(ttl=600, tags=["users"])
def list_users(request):
    return {"users": ["Alice", "Bob"]}

# Cache an async handler -- works identically
@app.get("/api/posts")
@cache(ttl=600, tags=["posts"])
async def list_posts(request):
    posts = await fetch_posts_from_db()
    return {"posts": posts}
```

The `@cache` decorator sets `X-Cache-TTL` and `X-Cache-Tags` headers that the Rust caching middleware reads. When applied to an async handler, the decorator awaits the coroutine before setting headers, so there is no risk of returning an unawaited coroutine.

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
| `X-Cache` | `HIT`, `MISS` or `BYPASS` |
| `ETag` | Entity tag for conditional requests |
| `Last-Modified` | When the entry was stored |

The cache is server-side only: it does **not** add `Cache-Control`. Browsers
and shared caches follow whatever `Cache-Control` the handler sets. A HIT still
passes through the `after` hooks, so security and CORS headers are applied.

---

## What Is Not Cached

The middleware skips caching for:

- Non-GET/HEAD methods (POST, PUT, DELETE, PATCH)
- Paths listed in `exclude_paths`
- Requests carrying `Authorization` or `Cookie` (unless `cache_authenticated=True`)
- Responses that set a cookie (`Set-Cookie`)
- Responses with `Cache-Control: no-store`, `no-cache` or `private`, or `Vary: *`
- Responses with a status other than 200, 203 or 204
- Bodies larger than 1 MiB

---

## Example: Full Caching Setup

```python
from cello import App, cache

app = App()
app.enable_caching(
    ttl=300,
    exclude_paths=["/api/auth", "/health", "/metrics"]
)

# Short-lived cache for frequently changing data (async)
@app.get("/api/feed")
@cache(ttl=30, tags=["feed"])
async def feed(request):
    posts = await get_recent_posts()
    return {"posts": posts}

# Long-lived cache for static configuration (sync)
@app.get("/api/settings")
@cache(ttl=3600, tags=["settings"])
def settings(request):
    return {"maintenance_mode": False}

# Invalidate on write
@app.post("/api/posts")
async def create_post(request):
    data = request.json()
    await save_post(data)
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
