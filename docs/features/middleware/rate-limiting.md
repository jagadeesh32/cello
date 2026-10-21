---
title: Rate Limiting
description: Request rate limiting in Cello Framework
---

# Rate Limiting

Cello provides rate limiting middleware implemented in Rust with lock-free counters. It supports multiple algorithms, per-IP tracking, custom key extraction, and adaptive limiting based on server load.

## Quick Start

```python
from cello import App, RateLimitConfig

app = App()

# 100 requests per minute using token bucket
app.enable_rate_limit(RateLimitConfig.token_bucket(
    requests=100,
    window=60
))
```

When a client exceeds the limit, they receive a `429 Too Many Requests` response with `Retry-After` and `X-RateLimit-*` headers.

---

## Algorithms

### Token Bucket

The token bucket algorithm allows bursts of traffic while enforcing an average rate. Tokens are added at a constant rate, and each request consumes one token.

```python
from cello import RateLimitConfig

config = RateLimitConfig.token_bucket(
    requests=100,     # Bucket capacity (max burst size)
    window=60         # Refill window in seconds
)

app.enable_rate_limit(config)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `requests` | `int` | Maximum number of tokens (burst capacity) |
| `window` | `int` | Time window in seconds for full refill |

!!! tip "When to Use Token Bucket"
    Token bucket is ideal for APIs where occasional bursts are acceptable (e.g., a user rapidly paginating through results) but you want to enforce a long-term average rate.

### Sliding Window

The sliding window algorithm provides a stricter, more uniform rate limit. It counts requests in a rolling time window.

```python
config = RateLimitConfig.sliding_window(
    requests=100,     # Max requests in the window
    window=60         # Window size in seconds
)

app.enable_rate_limit(config)
```

!!! tip "When to Use Sliding Window"
    Sliding window is better for APIs that need a strict, uniform rate -- for example, a billing API where you must guarantee no more than N requests per minute.

### Adaptive Rate Limiting

Adaptive rate limiting automatically adjusts the allowed rate based on server load (CPU, memory, latency). Under heavy load, limits decrease to protect the server.

```python
config = RateLimitConfig.adaptive(
    base_requests=100,       # Normal rate
    window=60,
    cpu_threshold=0.8,       # Reduce limits above 80% CPU
    memory_threshold=0.9,    # Reduce limits above 90% memory
    latency_threshold=100    # Reduce limits if latency > 100ms
)

app.enable_rate_limit(config)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `base_requests` | `int` | Normal request limit |
| `window` | `int` | Time window in seconds |
| `cpu_threshold` | `float` | CPU usage threshold (0.0 - 1.0) |
| `memory_threshold` | `float` | Memory usage threshold (0.0 - 1.0) |
| `latency_threshold` | `int` | Latency threshold in milliseconds |

---

## Response Headers

When rate limiting is enabled, every response includes rate limit headers:

```
HTTP/1.1 200 OK
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 87
X-RateLimit-Reset: 1705312800

HTTP/1.1 429 Too Many Requests
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1705312800
Retry-After: 23
```

| Header | Description |
|--------|-------------|
| `X-RateLimit-Limit` | Maximum requests allowed in the window |
| `X-RateLimit-Remaining` | Remaining requests in the current window |
| `X-RateLimit-Reset` | Unix timestamp when the window resets |
| `Retry-After` | Seconds until the client can retry (only on 429) |

---

## Per-IP Rate Limiting

By default, rate limiting is tracked per client IP address: the address of the
TCP peer. `X-Forwarded-For` and `X-Real-IP` are **ignored** unless the peer is a
proxy you configured — otherwise any client could pick a fresh bucket per
request, or push everyone into one shared bucket:

```python
# Behind a load balancer / reverse proxy on 10.0.0.0/8
app.set_trusted_proxies(["10.0.0.0/8"])
```

With trusted proxies, the client is the right-most `X-Forwarded-For` entry that
is not itself a trusted proxy. The same resolution backs `request.client_ip()`.

---

## Key Selection (`key_by`)

```python
app.enable_rate_limit(RateLimitConfig(capacity=100, refill_rate=10, key_by="api_key"))
```

| `key_by` | Bucket per |
|----------|-----------|
| `"ip"` (default) | Client IP (see above) |
| `"user"` | `request.context["user"]["id"]` or the JWT `sub` claim; client IP for anonymous requests |
| `"api_key"` | `X-API-Key` header value; client IP when absent |
| `"header:<Name>"` | Value of any header; client IP when absent |

Any other value raises `ValueError`. Buckets that carry no state are swept
periodically, so the store does not grow with every client ever seen.

---

## Exempt Paths

Exclude paths from rate limiting (health checks, metrics, etc.):

```python
config = RateLimitConfig.token_bucket(
    requests=100,
    window=60,
    exempt_paths=["/health", "/metrics", "/docs"]
)

app.enable_rate_limit(config)
```

---

## Example: API with Tiered Limits

```python
from cello import App, RateLimitConfig

app = App()

# Global rate limit: 100 requests per minute
app.enable_rate_limit(RateLimitConfig.token_bucket(
    requests=100,
    window=60
))

@app.get("/api/search")
def search(request):
    return {"results": []}

@app.post("/api/upload")
def upload(request):
    return {"uploaded": True}

@app.get("/health")
def health(request):
    return {"status": "ok"}
```

---

## Performance

Rate limiting uses lock-free atomic counters in Rust:

| Operation | Overhead |
|-----------|----------|
| Token check | ~100ns |
| Counter update | ~50ns |
| Key lookup (DashMap) | ~100ns |
| Total per request | ~250ns |

---

## Next Steps

- [Middleware Overview](overview.md) - Full middleware system
- [Circuit Breaker](circuit-breaker.md) - Fault tolerance
- [Caching](caching.md) - Response caching
- [Security Overview](../security/overview.md) - Security features
