---
title: Smart Caching with Tag Invalidation
description: Cache responses with per-endpoint TTLs and invalidate groups of cached entries using semantic tags.
---

# :material-cached: Smart Caching with Tag Invalidation

Cello's caching layer lets you attach a time-to-live to any route with a single decorator and group related entries under named tags so they can all be purged at once. This removes the need for brittle key-based invalidation logic and keeps stale data out of your API responses.

The example below enables a global 60-second cache, overrides TTL on individual endpoints, and demonstrates tag-based invalidation triggered by a POST request.

## Features Demonstrated

- Global cache configuration via `app.enable_caching(ttl=60)`
- Per-route TTL override with `@cache(ttl=10)`
- Tag-based grouping with `@cache(tags=["users", "list"])`
- On-demand invalidation of tagged entries via `app.invalidate_cache(["users"])`
- Observing cache hits vs. misses through the `timestamp` field

## Complete Source Code

```python
from cello import App, cache, Response
import time

app = App()
app.enable_caching(ttl=60)

@app.get("/")
def home(request):
    return {"message": "Hello! Try /cached or /slow"}

@app.get("/cached")
@cache(ttl=10)
def cached_endpoint(request):
    return {"timestamp": time.time(), "note": "Refreshes every 10s"}

@app.get("/tagged")
@cache(tags=["users", "list"])
def tagged_endpoint(request):
    return {"data": "User List", "timestamp": time.time()}

@app.post("/invalidate")
def invalidate(request):
    app.invalidate_cache(["users"])
    return {"status": "Cache invalidated for tag 'users'"}

if __name__ == "__main__":
    app.run(port=8080)
```

## Running This Example

```bash
python examples/smart_caching.py
```

```bash
# First request — response is computed and cached
curl http://localhost:8080/cached

# Second request within 10 s — timestamp is identical (cache hit)
curl http://localhost:8080/cached

# Fetch the tagged user list
curl http://localhost:8080/tagged

# Invalidate all entries tagged "users"
curl -X POST http://localhost:8080/invalidate

# Next request recomputes the response — new timestamp
curl http://localhost:8080/tagged
```

## Key Concepts

- **Global TTL** — `app.enable_caching(ttl=60)` sets the default expiry for all cached routes; individual routes may override it
- **`@cache(ttl=N)`** — decorator that caches the response for `N` seconds, regardless of the global setting
- **Cache tags** — arbitrary string labels attached to cache entries; a single entry can belong to multiple tags
- **`app.invalidate_cache(tags)`** — purges every entry matching any of the supplied tags, enabling coarse-grained invalidation without knowing individual cache keys
- **Cache key derivation** — the cache key is derived from the request path and query string, so distinct query parameters produce distinct entries
