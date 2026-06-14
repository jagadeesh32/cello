---
title: Rate Limiting & DDoS Protection
description: Sliding window rate limiting with Redis and IP filtering
tags:
  - Rate Limiting
  - DDoS
  - Security
  - Redis
  - Middleware
  - Examples
---

# :material-speedometer: Rate Limiting & DDoS Protection

Protect your Cello application from abuse and DDoS attacks by combining a Redis-backed sliding window rate limiter with IP allowlists and blocklists. This example shows how to build a reusable `RateLimiter` middleware that returns standards-compliant `429 Too Many Requests` responses including a `Retry-After` header, and how to wire up Prometheus alerting rules so your on-call team is paged before traffic becomes an outage.

## Complete Example

```python
"""
enterprise/rate-limiting.py

Sliding-window rate limiting with Redis, IP allow/block lists,
429 responses with Retry-After headers, and Prometheus alerting.

Requirements:
    pip install cello redis prometheus-client
"""
from __future__ import annotations

import ipaddress
import logging
import math
import time
from dataclasses import dataclass, field
from typing import Callable

import redis
from prometheus_client import Counter, Gauge, start_http_server

import cello
from cello import Request, Response

# ---------------------------------------------------------------------------
# Logging
# ---------------------------------------------------------------------------
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s – %(message)s",
)
log = logging.getLogger("rate_limiter")

# ---------------------------------------------------------------------------
# Prometheus metrics
# ---------------------------------------------------------------------------
REQUESTS_TOTAL = Counter(
    "http_requests_total",
    "Total HTTP requests received",
    ["method", "path", "status"],
)
RATE_LIMITED_TOTAL = Counter(
    "http_rate_limited_total",
    "Total requests rejected by the rate limiter",
    ["reason"],
)
ACTIVE_CONNECTIONS = Gauge(
    "http_active_connections",
    "Number of currently active HTTP connections",
)

# ---------------------------------------------------------------------------
# IP allow / block lists
# ---------------------------------------------------------------------------
# Networks listed here are always allowed through, regardless of rate limits.
ALLOWLIST: list[ipaddress.IPv4Network | ipaddress.IPv6Network] = [
    ipaddress.ip_network("10.0.0.0/8"),       # internal services
    ipaddress.ip_network("172.16.0.0/12"),     # Docker / k8s pod CIDR
    ipaddress.ip_network("192.168.0.0/16"),    # LAN / staging
]

# IPs / networks listed here are immediately rejected with 403.
BLOCKLIST: list[ipaddress.IPv4Network | ipaddress.IPv6Network] = [
    ipaddress.ip_network("198.51.100.0/24"),   # known bad actor range
    ipaddress.ip_network("203.0.113.42/32"),   # single abusive host
]


def _parse_ip(addr: str) -> ipaddress.IPv4Address | ipaddress.IPv6Address:
    """Parse a raw IP string, stripping port information if present."""
    host = addr.rsplit(":", 1)[0].strip("[]")
    return ipaddress.ip_address(host)


def ip_is_allowed(addr: str) -> bool:
    """Return True when the IP is explicitly whitelisted."""
    ip = _parse_ip(addr)
    return any(ip in net for net in ALLOWLIST)


def ip_is_blocked(addr: str) -> bool:
    """Return True when the IP is explicitly blacklisted."""
    ip = _parse_ip(addr)
    return any(ip in net for net in BLOCKLIST)


# ---------------------------------------------------------------------------
# Redis sliding-window counter (atomic Lua script)
# ---------------------------------------------------------------------------
SLIDING_WINDOW_SCRIPT = """
-- KEYS[1] = rate-limit key
-- ARGV[1] = now (ms)   ARGV[2] = window (ms)   ARGV[3] = limit
local key    = KEYS[1]
local now    = tonumber(ARGV[1])
local window = tonumber(ARGV[2])
local limit  = tonumber(ARGV[3])

redis.call('ZREMRANGEBYSCORE', key, '-inf', now - window)
local count = redis.call('ZCARD', key)

if count < limit then
    redis.call('ZADD', key, now, now .. math.random())
    redis.call('PEXPIRE', key, window)
    return count + 1  -- calls so far, including this one
else
    return -1         -- limit exceeded
end
"""


@dataclass
class RateLimitConfig:
    """Per-route rate-limit configuration."""
    requests: int = 100          # max requests …
    window_seconds: float = 60   # … per this many seconds
    key_prefix: str = "rl"       # Redis key namespace


@dataclass
class RateLimiter:
    """
    Cello middleware that enforces sliding-window rate limits via Redis.

    Usage::

        app = cello.App()
        limiter = RateLimiter(
            redis_url="redis://localhost:6379/0",
            default=RateLimitConfig(requests=200, window_seconds=60),
        )
        app.use(limiter)
    """

    redis_url: str = "redis://localhost:6379/0"
    default: RateLimitConfig = field(default_factory=RateLimitConfig)
    # Map path prefixes to custom configs, e.g. {"/api/auth": RateLimitConfig(10, 60)}
    overrides: dict[str, RateLimitConfig] = field(default_factory=dict)

    def __post_init__(self) -> None:
        self._redis: redis.Redis = redis.from_url(
            self.redis_url, decode_responses=True
        )
        self._script = self._redis.register_script(SLIDING_WINDOW_SCRIPT)
        log.info("RateLimiter connected to %s", self.redis_url)

    async def __call__(self, request: Request, next_handler: Callable) -> Response:
        client_ip: str = (
            request.headers.get("X-Forwarded-For", request.remote_addr or "")
            .split(",")[0]
            .strip()
        )

        # 1. Block listed IPs immediately
        if ip_is_blocked(client_ip):
            log.warning("Blocked IP %s denied", client_ip)
            RATE_LIMITED_TOTAL.labels(reason="blocklist").inc()
            return Response(
                status=403,
                body=b'{"error": "forbidden"}',
                headers={"Content-Type": "application/json"},
            )

        # 2. Allowlisted IPs skip rate limiting entirely
        if ip_is_allowed(client_ip):
            return await next_handler(request)

        # 3. Pick the most-specific override config
        cfg = self._config_for(request.path)

        # 4. Evaluate the sliding window atomically in Redis
        key = f"{cfg.key_prefix}:{client_ip}:{request.path}"
        window_ms = int(cfg.window_seconds * 1000)
        now_ms = int(time.time() * 1000)

        try:
            result: int = self._script(
                keys=[key],
                args=[now_ms, window_ms, cfg.requests],
            )
        except redis.RedisError as exc:
            # Fail open: log the error but allow the request through
            log.error("Redis error in rate limiter: %s", exc)
            return await next_handler(request)

        if result == -1:
            retry_after = math.ceil(cfg.window_seconds)
            log.info(
                "Rate limit exceeded for %s on %s (limit=%d/%ds)",
                client_ip, request.path, cfg.requests, cfg.window_seconds,
            )
            RATE_LIMITED_TOTAL.labels(reason="rate_limit").inc()
            return Response(
                status=429,
                body=(
                    f'{{"error": "too many requests", "retry_after": {retry_after}}}'
                ).encode(),
                headers={
                    "Content-Type": "application/json",
                    "Retry-After": str(retry_after),
                    "X-RateLimit-Limit": str(cfg.requests),
                    "X-RateLimit-Window": str(cfg.window_seconds),
                },
            )

        response = await next_handler(request)
        remaining = max(cfg.requests - result, 0)
        response.headers["X-RateLimit-Limit"] = str(cfg.requests)
        response.headers["X-RateLimit-Remaining"] = str(remaining)
        return response

    def _config_for(self, path: str) -> RateLimitConfig:
        """Return the most specific matching override, or the default."""
        for prefix in sorted(self.overrides, key=len, reverse=True):
            if path.startswith(prefix):
                return self.overrides[prefix]
        return self.default


# ---------------------------------------------------------------------------
# Application
# ---------------------------------------------------------------------------
app = cello.App()

limiter = RateLimiter(
    redis_url="redis://localhost:6379/0",
    default=RateLimitConfig(requests=100, window_seconds=60),
    overrides={
        # Auth endpoints get a much stricter limit to prevent brute-force
        "/api/auth": RateLimitConfig(requests=10, window_seconds=60, key_prefix="rl:auth"),
        # Public search is more generous
        "/api/search": RateLimitConfig(requests=300, window_seconds=60, key_prefix="rl:search"),
    },
)

app.use(limiter)


@app.get("/api/data")
async def get_data(request: Request) -> Response:
    REQUESTS_TOTAL.labels(method="GET", path="/api/data", status=200).inc()
    return Response(body=b'{"data": "hello"}', headers={"Content-Type": "application/json"})


@app.post("/api/auth/login")
async def login(request: Request) -> Response:
    payload = await request.json()
    username = payload.get("username", "")
    # ... real auth logic here ...
    REQUESTS_TOTAL.labels(method="POST", path="/api/auth/login", status=200).inc()
    return Response(
        body=f'{{"token": "eyJ...demo", "user": "{username}"}}'.encode(),
        headers={"Content-Type": "application/json"},
    )


@app.get("/api/search")
async def search(request: Request) -> Response:
    query = request.query.get("q", "")
    REQUESTS_TOTAL.labels(method="GET", path="/api/search", status=200).inc()
    return Response(
        body=f'{{"query": "{query}", "results": []}}'.encode(),
        headers={"Content-Type": "application/json"},
    )


# ---------------------------------------------------------------------------
# Prometheus alerting rules
# Paste into: prometheus/rules/rate_limit.yml
# ---------------------------------------------------------------------------
PROMETHEUS_RULES = """
groups:
  - name: rate_limiting
    rules:

      # Fire when > 5 % of all requests are being rate-limited
      - alert: HighRateLimitRejectionRate
        expr: |
          rate(http_rate_limited_total[5m])
            /
          rate(http_requests_total[5m]) > 0.05
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "High rate-limit rejection rate ({{ $value | humanizePercentage }})"
          description: >
            More than 5 % of requests have been rejected by the rate limiter
            over the last 5 minutes. This may indicate a DDoS attempt or a
            misconfigured client.

      # Fire when a burst of blocklist hits is detected
      - alert: BlocklistSpike
        expr: rate(http_rate_limited_total{reason="blocklist"}[1m]) > 10
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Blocklist spike – possible DDoS"
          description: >
            More than 10 requests per second are being rejected from blocklisted
            IPs. Inspect firewall logs immediately.
"""


if __name__ == "__main__":
    # Expose Prometheus metrics on a dedicated port
    start_http_server(9090)
    log.info("Prometheus metrics available at http://localhost:9090")
    log.info("Starting Cello app on http://localhost:8000")
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **Sliding window algorithm** — Unlike a fixed window, which can allow a burst at window boundaries, the sliding window tracks the precise timestamp of every request in a Redis sorted-set (`ZADD`). Stale entries are pruned with `ZREMRANGEBYSCORE` before counting, giving an accurate, burst-safe view of the last *N* seconds at all times.

- **Atomic Lua script** — The check-and-increment is executed as a single atomic Lua script on the Redis server. This eliminates the TOCTOU (time-of-check / time-of-use) race condition that would exist with separate `ZCARD` and `ZADD` calls from application code.

- **IP allowlist / blocklist** — Two in-memory CIDR lists are evaluated *before* hitting Redis. Allowlisted subnets (internal services, k8s pods) bypass the counter entirely. Blocklisted IPs receive an immediate `403 Forbidden` without consuming any Redis budget.

- **Per-route overrides** — The `RateLimiter` accepts an `overrides` dict keyed by path prefix. The most-specific matching prefix wins, enabling tighter limits on sensitive endpoints (e.g. `/api/auth`) without touching your default policy.

- **Standards-compliant `429` response** — Rejected requests return HTTP `429 Too Many Requests` with a `Retry-After` header (seconds until the window resets) plus `X-RateLimit-Limit` and `X-RateLimit-Window` so well-behaved clients can back off gracefully.

- **Fail-open on Redis errors** — If Redis is unavailable the middleware logs the error and falls through to the actual handler. This avoids turning a Redis outage into an application-wide denial of service. Switch to fail-closed (`return Response(status=503, ...)`) if your threat model requires it.

- **Prometheus alerting** — Two alert rules are provided: one fires when the fraction of rejected requests exceeds 5 % (early-warning DDoS indicator), the other fires when blocklist hits spike (active attack signal). Both include `for:` clauses to suppress transient spikes.

## Running This Example

```bash
# 1. Start Redis
docker run -d --name redis -p 6379:6379 redis:7-alpine

# 2. Install dependencies
pip install cello redis prometheus-client

# 3. Run the application
python examples/enterprise/rate-limiting.py

# 4. Test the limiter — send 15 rapid requests against the strict /api/auth limit (10/60 s)
for i in $(seq 1 15); do
  curl -s -o /dev/null -w "%{http_code}\n" \
    -X POST http://localhost:8000/api/auth/login \
    -H 'Content-Type: application/json' \
    -d '{"username":"alice","password":"secret"}'
done
# Expected output: 200 200 200 ... 429 429 429

# 5. Inspect Prometheus metrics
curl -s http://localhost:9090/metrics | grep http_rate_limited
```
