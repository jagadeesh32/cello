---
title: Redis Caching
description: Cache responses using Redis with TTL management
tags:
  - Redis
  - Caching
  - Performance
  - TTL
  - Examples
---

# :material-database-clock: Redis Caching

Speed up your Cello API by caching expensive responses in Redis. This example demonstrates the cache-aside pattern using a `@cache` decorator with configurable TTLs, manual invalidation on mutating requests, and a cache-warming strategy so your most popular routes are hot on startup.

## Complete Example

```python
import json
import time
import asyncio
import hashlib
from functools import wraps
from typing import Any, Callable, Optional

import redis.asyncio as aioredis
import cello
from cello import Request, Response, route, on_startup

# ---------------------------------------------------------------------------
# Redis client setup
# ---------------------------------------------------------------------------

REDIS_URL = "redis://localhost:6379/0"
DEFAULT_TTL = 60  # seconds

redis_client: Optional[aioredis.Redis] = None


@on_startup
async def connect_redis():
    """Open a connection pool to Redis when the app starts."""
    global redis_client
    redis_client = await aioredis.from_url(
        REDIS_URL,
        encoding="utf-8",
        decode_responses=True,
        max_connections=20,
    )
    print(f"[startup] Connected to Redis at {REDIS_URL}")
    await warm_cache()


# ---------------------------------------------------------------------------
# @cache decorator — cache-aside pattern
# ---------------------------------------------------------------------------

def cache(ttl: int = DEFAULT_TTL, prefix: str = "cello"):
    """
    Decorator that wraps a route handler with Redis caching.

    The cache key is built from ``prefix``, the route path, and a SHA-256
    hash of the query string so that ``/products?page=2`` is cached
    separately from ``/products?page=1``.

    Usage::

        @route("/products")
        @cache(ttl=120, prefix="shop")
        async def list_products(req: Request) -> Response:
            ...
    """
    def decorator(handler: Callable):
        @wraps(handler)
        async def wrapper(req: Request, *args, **kwargs) -> Response:
            # Build a deterministic cache key
            qs_hash = hashlib.sha256(req.query_string.encode()).hexdigest()[:8]
            key = f"{prefix}:{req.path}:{qs_hash}"

            # --- Cache HIT path ---
            cached = await redis_client.get(key)
            if cached is not None:
                data = json.loads(cached)
                return Response.json(data, headers={"X-Cache": "HIT"})

            # --- Cache MISS path: call the real handler ---
            response: Response = await handler(req, *args, **kwargs)

            # Only cache successful JSON responses
            if response.status_code == 200 and response.content_type == "application/json":
                await redis_client.setex(key, ttl, response.body)

            response.headers["X-Cache"] = "MISS"
            return response

        return wrapper
    return decorator


# ---------------------------------------------------------------------------
# Manual cache invalidation helper
# ---------------------------------------------------------------------------

async def invalidate(pattern: str) -> int:
    """
    Delete all Redis keys matching *pattern*.

    Example: ``await invalidate("shop:/products:*")``

    Returns the number of keys deleted.
    """
    keys = await redis_client.keys(pattern)
    if not keys:
        return 0
    return await redis_client.delete(*keys)


# ---------------------------------------------------------------------------
# Cache warming — pre-populate hot keys on startup
# ---------------------------------------------------------------------------

WARM_TARGETS = [
    ("/products", {}),
    ("/products?featured=true", {"featured": "true"}),
    ("/categories", {}),
]


async def warm_cache():
    """Seed Redis with responses for the most-requested endpoints."""
    print("[cache] Warming cache …")
    for path, params in WARM_TARGETS:
        # Build a synthetic request so we can reuse the handler logic
        qs = "&".join(f"{k}={v}" for k, v in params.items())
        qs_hash = hashlib.sha256(qs.encode()).hexdigest()[:8]
        key = f"shop:{path.split('?')[0]}:{qs_hash}"

        # Skip keys that are already warm
        if await redis_client.exists(key):
            print(f"[cache]   {key!r} already warm — skipping")
            continue

        # Fetch fresh data and store it
        data = await _fetch_products(params)
        await redis_client.setex(key, DEFAULT_TTL, json.dumps(data))
        print(f"[cache]   Warmed {key!r} ({DEFAULT_TTL}s TTL)")


# ---------------------------------------------------------------------------
# Simulated data layer (replace with your real DB calls)
# ---------------------------------------------------------------------------

PRODUCTS = [
    {"id": 1, "name": "Widget Alpha",  "price": 9.99,  "featured": True,  "category": "widgets"},
    {"id": 2, "name": "Widget Beta",   "price": 14.99, "featured": False, "category": "widgets"},
    {"id": 3, "name": "Gadget Pro",    "price": 49.99, "featured": True,  "category": "gadgets"},
    {"id": 4, "name": "Gadget Lite",   "price": 24.99, "featured": False, "category": "gadgets"},
    {"id": 5, "name": "Doohickey Max", "price": 4.99,  "featured": False, "category": "misc"},
]


async def _fetch_products(filters: dict) -> list[dict]:
    """Simulate a slow DB query (200 ms latency)."""
    await asyncio.sleep(0.2)
    items = PRODUCTS
    if filters.get("featured") == "true":
        items = [p for p in items if p["featured"]]
    if cat := filters.get("category"):
        items = [p for p in items if p["category"] == cat]
    return items


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------

app = cello.App()


@app.route("/products", methods=["GET"])
@cache(ttl=120, prefix="shop")
async def list_products(req: Request) -> Response:
    """
    List products with optional filtering.

    Query params:
      - featured=true   – only featured products
      - category=<name> – filter by category
    """
    data = await _fetch_products(dict(req.query_params))
    return Response.json({"products": data, "count": len(data)})


@app.route("/products/{product_id}", methods=["GET"])
@cache(ttl=300, prefix="shop")
async def get_product(req: Request, product_id: int) -> Response:
    """Fetch a single product by ID (cached for 5 minutes)."""
    product = next((p for p in PRODUCTS if p["id"] == product_id), None)
    if product is None:
        return Response.json({"error": "Product not found"}, status=404)
    return Response.json(product)


@app.route("/products", methods=["POST"])
async def create_product(req: Request) -> Response:
    """
    Create a product and **invalidate** all cached product listings so
    readers immediately see the new item.
    """
    body = await req.json()

    new_product = {
        "id": max(p["id"] for p in PRODUCTS) + 1,
        "name": body["name"],
        "price": float(body["price"]),
        "featured": body.get("featured", False),
        "category": body.get("category", "misc"),
    }
    PRODUCTS.append(new_product)

    # Bust every cached listing (but leave individual product caches intact)
    deleted = await invalidate("shop:/products:*")
    print(f"[cache] POST /products — invalidated {deleted} listing key(s)")

    return Response.json(new_product, status=201)


@app.route("/products/{product_id}", methods=["DELETE"])
async def delete_product(req: Request, product_id: int) -> Response:
    """
    Delete a product and invalidate both the listing cache **and** the
    specific product's cache entry.
    """
    global PRODUCTS
    product = next((p for p in PRODUCTS if p["id"] == product_id), None)
    if product is None:
        return Response.json({"error": "Product not found"}, status=404)

    PRODUCTS = [p for p in PRODUCTS if p["id"] != product_id]

    # Invalidate all listing caches and the specific product entry
    listing_deleted = await invalidate("shop:/products:*")
    item_deleted    = await invalidate(f"shop:/products/{product_id}:*")
    total = listing_deleted + item_deleted
    print(f"[cache] DELETE /products/{product_id} — invalidated {total} key(s)")

    return Response.json({"deleted": product_id})


@app.route("/categories", methods=["GET"])
@cache(ttl=600, prefix="shop")   # categories change rarely — 10-minute TTL
async def list_categories(req: Request) -> Response:
    cats = sorted({p["category"] for p in PRODUCTS})
    return Response.json({"categories": cats})


@app.route("/cache/stats", methods=["GET"])
async def cache_stats(req: Request) -> Response:
    """Inspect live Redis stats useful for monitoring."""
    info = await redis_client.info("stats")
    keys = await redis_client.dbsize()
    return Response.json({
        "total_keys": keys,
        "hits":   info.get("keyspace_hits", 0),
        "misses": info.get("keyspace_misses", 0),
        "hit_rate": (
            round(
                info["keyspace_hits"] /
                max(1, info["keyspace_hits"] + info["keyspace_misses"]) * 100,
                2,
            )
            if "keyspace_hits" in info else None
        ),
    })


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **`@cache` decorator** — wraps any route handler transparently; the decorator checks Redis first and only calls the real handler on a cache miss, then stores the result.
- **Cache key design** — the key encodes the URL path and a hash of the query string, so different query parameter combinations are cached independently without key collisions.
- **TTL management** — each route specifies its own TTL (`ttl=120` for listings, `ttl=300` for individual items, `ttl=600` for stable data like categories) matching how often that data actually changes.
- **Cache-aside pattern** — the application code is responsible for both reading from and writing to Redis; the data store is never written through the cache layer.
- **Manual invalidation on POST/DELETE** — mutating routes call `invalidate(pattern)` with a glob pattern to bust all affected keys immediately, preventing stale reads.
- **Cache warming** — `warm_cache()` runs at startup via `@on_startup`, pre-populating the highest-traffic keys so the very first real requests are already served from Redis.
- **`X-Cache` header** — every response includes `X-Cache: HIT` or `X-Cache: MISS`, making it easy to verify cache behaviour during development and in monitoring dashboards.

## Running This Example

```bash
# Start a local Redis instance (Docker)
docker run -d -p 6379:6379 redis:7-alpine

# Install dependencies
pip install cello redis

# Run the server
python examples/advanced/redis-caching.py
```

Make a few requests to see the cache in action:

```bash
# First request → MISS; subsequent requests → HIT
curl -i http://localhost:8000/products

# Creates a product and busts listing caches
curl -s -X POST http://localhost:8000/products \
  -H "Content-Type: application/json" \
  -d '{"name":"Super Widget","price":19.99,"featured":true,"category":"widgets"}'

# Inspect live hit/miss counters
curl -s http://localhost:8000/cache/stats | python -m json.tool
```
