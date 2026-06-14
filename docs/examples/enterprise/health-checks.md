---
title: Health Checks & Observability
description: Production-ready health endpoints for Kubernetes and monitoring
---

# :material-heart-pulse: Health Checks & Observability

Running Cello in Kubernetes means the kubelet, load balancers, and monitoring systems all need reliable signals about your application's state. This example wires up `/health` (liveness) and `/ready` (readiness) endpoints that check every critical dependency — database, Redis, and background queues — and exposes Prometheus metrics on `/metrics` together with OpenTelemetry distributed traces and structured JSON logging so you have full observability out of the box.

## Complete Example

```python
"""
enterprise/health-checks.py

Production-ready health endpoints for Kubernetes, Prometheus metrics,
OpenTelemetry traces, and structured JSON logging.

Requirements:
    pip install cello redis asyncpg prometheus-client \
                opentelemetry-sdk opentelemetry-exporter-otlp-proto-grpc \
                python-json-logger
"""
from __future__ import annotations

import asyncio
import json
import logging
import os
import time
from dataclasses import dataclass, field
from typing import Any

import asyncpg
import redis.asyncio as aioredis
from prometheus_client import (
    CONTENT_TYPE_LATEST,
    Counter,
    Gauge,
    Histogram,
    generate_latest,
)
from pythonjsonlogger import jsonlogger
from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.resources import SERVICE_NAME, Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

import cello
from cello import Request, Response

# ---------------------------------------------------------------------------
# Structured JSON logging
# ---------------------------------------------------------------------------
def _configure_logging() -> logging.Logger:
    handler = logging.StreamHandler()
    formatter = jsonlogger.JsonFormatter(
        fmt="%(asctime)s %(levelname)s %(name)s %(message)s",
        datefmt="%Y-%m-%dT%H:%M:%S",
    )
    handler.setFormatter(formatter)
    root = logging.getLogger()
    root.addHandler(handler)
    root.setLevel(logging.INFO)
    return logging.getLogger("health_checks")


log = _configure_logging()

# ---------------------------------------------------------------------------
# OpenTelemetry tracing
# ---------------------------------------------------------------------------
def _configure_tracing(service_name: str = "cello-demo") -> trace.Tracer:
    resource = Resource(attributes={SERVICE_NAME: service_name})
    provider = TracerProvider(resource=resource)
    otlp_endpoint = os.getenv("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317")
    exporter = OTLPSpanExporter(endpoint=otlp_endpoint, insecure=True)
    provider.add_span_processor(BatchSpanProcessor(exporter))
    trace.set_tracer_provider(provider)
    log.info("OpenTelemetry tracing configured", extra={"endpoint": otlp_endpoint})
    return trace.get_tracer(service_name)


tracer = _configure_tracing()

# ---------------------------------------------------------------------------
# Prometheus metrics
# ---------------------------------------------------------------------------
REQUEST_LATENCY = Histogram(
    "http_request_duration_seconds",
    "HTTP request latency in seconds",
    ["method", "path"],
    buckets=[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5],
)
REQUESTS_TOTAL = Counter(
    "http_requests_total",
    "Total HTTP requests",
    ["method", "path", "status"],
)
DB_POOL_CONNECTIONS = Gauge(
    "db_pool_connections",
    "Current number of connections in the PostgreSQL pool",
    ["state"],  # idle | active
)
DEPENDENCY_UP = Gauge(
    "dependency_up",
    "Whether a dependency is reachable (1 = up, 0 = down)",
    ["name"],
)

# ---------------------------------------------------------------------------
# Dependency health checks
# ---------------------------------------------------------------------------
@dataclass
class CheckResult:
    name: str
    status: str          # "ok" | "degraded" | "down"
    latency_ms: float
    detail: str = ""
    error: str = ""


async def check_postgres(pool: asyncpg.Pool) -> CheckResult:
    """Verify the PostgreSQL pool can execute a trivial query."""
    start = time.monotonic()
    try:
        async with pool.acquire() as conn:
            await conn.fetchval("SELECT 1")
        latency = (time.monotonic() - start) * 1000
        DB_POOL_CONNECTIONS.labels(state="idle").set(pool.get_idle_size())
        DB_POOL_CONNECTIONS.labels(state="active").set(
            pool.get_size() - pool.get_idle_size()
        )
        DEPENDENCY_UP.labels(name="postgres").set(1)
        return CheckResult("postgres", "ok", round(latency, 2))
    except Exception as exc:
        DEPENDENCY_UP.labels(name="postgres").set(0)
        log.error("Postgres health check failed", extra={"error": str(exc)})
        return CheckResult("postgres", "down", 0, error=str(exc))


async def check_redis(client: aioredis.Redis) -> CheckResult:
    """Verify Redis responds to PING."""
    start = time.monotonic()
    try:
        pong = await client.ping()
        latency = (time.monotonic() - start) * 1000
        DEPENDENCY_UP.labels(name="redis").set(1 if pong else 0)
        return CheckResult(
            "redis",
            "ok" if pong else "down",
            round(latency, 2),
            detail="PONG" if pong else "no response",
        )
    except Exception as exc:
        DEPENDENCY_UP.labels(name="redis").set(0)
        log.error("Redis health check failed", extra={"error": str(exc)})
        return CheckResult("redis", "down", 0, error=str(exc))


async def check_queue(redis_client: aioredis.Redis, queue_name: str = "jobs") -> CheckResult:
    """Check the background job queue depth and signal degraded if too long."""
    start = time.monotonic()
    try:
        depth: int = await redis_client.llen(queue_name)
        latency = (time.monotonic() - start) * 1000
        status = "degraded" if depth > 10_000 else "ok"
        DEPENDENCY_UP.labels(name="queue").set(1)
        return CheckResult(
            "queue",
            status,
            round(latency, 2),
            detail=f"depth={depth}",
        )
    except Exception as exc:
        DEPENDENCY_UP.labels(name="queue").set(0)
        log.error("Queue health check failed", extra={"error": str(exc)})
        return CheckResult("queue", "down", 0, error=str(exc))


def _results_to_body(
    results: list[CheckResult],
    overall: str,
) -> tuple[bytes, int]:
    payload = {
        "status": overall,
        "checks": {
            r.name: {
                "status": r.status,
                "latency_ms": r.latency_ms,
                **({"detail": r.detail} if r.detail else {}),
                **({"error": r.error} if r.error else {}),
            }
            for r in results
        },
    }
    http_status = 200 if overall == "ok" else (207 if overall == "degraded" else 503)
    return json.dumps(payload).encode(), http_status


# ---------------------------------------------------------------------------
# Application
# ---------------------------------------------------------------------------
app = cello.App()

# Shared dependency clients (initialised at startup)
_pg_pool: asyncpg.Pool | None = None
_redis_client: aioredis.Redis | None = None


@app.on_startup
async def startup() -> None:
    global _pg_pool, _redis_client
    _pg_pool = await asyncpg.create_pool(
        dsn=os.getenv("DATABASE_URL", "postgresql://user:pass@localhost/demo"),
        min_size=2,
        max_size=10,
    )
    _redis_client = aioredis.from_url(
        os.getenv("REDIS_URL", "redis://localhost:6379/0"),
        decode_responses=True,
    )
    log.info("Application started", extra={"event": "startup"})


@app.on_shutdown
async def shutdown() -> None:
    if _pg_pool:
        await _pg_pool.close()
    if _redis_client:
        await _redis_client.close()
    log.info("Application stopped", extra={"event": "shutdown"})


# ------------------------------------------------------------------
# /health  –  liveness probe
# Kubernetes restarts the pod if this returns non-2xx.
# Keep it fast: only check that the process is alive and event loop
# is responsive. Do NOT check external dependencies here.
# ------------------------------------------------------------------
@app.get("/health")
async def liveness(request: Request) -> Response:
    with tracer.start_as_current_span("health.liveness") as span:
        span.set_attribute("probe", "liveness")
        body = json.dumps({"status": "ok", "probe": "liveness"}).encode()
        log.info("Liveness probe", extra={"probe": "liveness", "status": "ok"})
        return Response(
            status=200,
            body=body,
            headers={"Content-Type": "application/json"},
        )


# ------------------------------------------------------------------
# /ready  –  readiness probe
# Kubernetes stops sending traffic if this returns non-2xx.
# Check every dependency that the application needs to serve requests.
# ------------------------------------------------------------------
@app.get("/ready")
async def readiness(request: Request) -> Response:
    with tracer.start_as_current_span("health.readiness") as span:
        span.set_attribute("probe", "readiness")

        results = await asyncio.gather(
            check_postgres(_pg_pool),
            check_redis(_redis_client),
            check_queue(_redis_client),
        )

        statuses = {r.status for r in results}
        if "down" in statuses:
            overall = "down"
        elif "degraded" in statuses:
            overall = "degraded"
        else:
            overall = "ok"

        span.set_attribute("overall_status", overall)
        body, http_status = _results_to_body(list(results), overall)

        log.info(
            "Readiness probe",
            extra={
                "probe": "readiness",
                "overall": overall,
                "checks": {r.name: r.status for r in results},
            },
        )

        return Response(
            status=http_status,
            body=body,
            headers={"Content-Type": "application/json"},
        )


# ------------------------------------------------------------------
# /metrics  –  Prometheus scrape endpoint
# ------------------------------------------------------------------
@app.get("/metrics")
async def metrics(request: Request) -> Response:
    data = generate_latest()
    return Response(
        status=200,
        body=data,
        headers={"Content-Type": CONTENT_TYPE_LATEST},
    )


# ------------------------------------------------------------------
# Instrumentation middleware – records latency & request count
# ------------------------------------------------------------------
@app.middleware
async def instrument(request: Request, next_handler) -> Response:
    start = time.monotonic()
    with tracer.start_as_current_span(
        f"{request.method} {request.path}",
        kind=trace.SpanKind.SERVER,
    ) as span:
        span.set_attribute("http.method", request.method)
        span.set_attribute("http.route", request.path)

        response = await next_handler(request)

        duration = time.monotonic() - start
        REQUEST_LATENCY.labels(
            method=request.method, path=request.path
        ).observe(duration)
        REQUESTS_TOTAL.labels(
            method=request.method,
            path=request.path,
            status=response.status,
        ).inc()

        span.set_attribute("http.status_code", response.status)
        log.info(
            "Request handled",
            extra={
                "method": request.method,
                "path": request.path,
                "status": response.status,
                "duration_ms": round(duration * 1000, 2),
            },
        )
        return response


# ------------------------------------------------------------------
# Example application endpoint
# ------------------------------------------------------------------
@app.get("/api/users/{user_id}")
async def get_user(request: Request, user_id: str) -> Response:
    with tracer.start_as_current_span("db.get_user") as span:
        span.set_attribute("user.id", user_id)
        async with _pg_pool.acquire() as conn:
            row = await conn.fetchrow(
                "SELECT id, email, created_at FROM users WHERE id = $1", user_id
            )
        if row is None:
            return Response(
                status=404,
                body=b'{"error": "user not found"}',
                headers={"Content-Type": "application/json"},
            )
        return Response(
            status=200,
            body=json.dumps(dict(row)).encode(),
            headers={"Content-Type": "application/json"},
        )


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Key Concepts

- **Liveness vs. readiness separation** — `/health` is the *liveness* probe: it only checks that the process is alive and the event loop is responsive. Kubernetes restarts the container if liveness fails. `/ready` is the *readiness* probe: it checks every external dependency. Kubernetes removes the pod from the load balancer if readiness fails, preventing traffic from reaching an unhealthy instance without triggering a restart loop.

- **Dependency checks run concurrently** — `asyncio.gather` fans out the Postgres, Redis, and queue checks in parallel. Total probe latency equals the slowest dependency check, not the sum of all checks.

- **Partial degradation with HTTP 207** — If some checks are degraded (e.g. the job queue is deep) but not fully down, the endpoint returns `207 Multi-Status` instead of `503`. This lets monitoring systems distinguish between "completely broken" and "running hot but serving traffic".

- **Prometheus metrics** — Three metric families are registered: `http_request_duration_seconds` (histogram with meaningful buckets for SLO calculation), `http_requests_total` (counter for error-rate alerts), and `dependency_up` (gauge per dependency for dashboards). Kubernetes scrapes `/metrics` directly; no push gateway needed.

- **OpenTelemetry distributed tracing** — Every request and every DB call is wrapped in a span that is batch-exported to an OTEL collector via gRPC. Traces are correlated with logs via `trace_id` injection (add `python-json-logger` + OTEL log bridge for full correlation).

- **Structured JSON logging** — All log statements emit JSON objects with consistent fields (`method`, `path`, `status`, `duration_ms`, `error`). This makes logs trivially queryable in Loki, CloudWatch Insights, or Datadog Log Management without regex parsing.

- **Graceful startup / shutdown** — `@app.on_startup` and `@app.on_shutdown` hooks create and close the connection pool cleanly. Kubernetes sends `SIGTERM` before removing the pod from the load balancer, giving in-flight requests time to finish.

## Running This Example

```bash
# 1. Start dependencies
docker compose up -d postgres redis

# 2. (Optional) Start an OpenTelemetry collector
docker run -d --name otel-collector \
  -p 4317:4317 \
  otel/opentelemetry-collector-contrib:latest

# 3. Install dependencies
pip install cello asyncpg redis prometheus-client \
            opentelemetry-sdk opentelemetry-exporter-otlp-proto-grpc \
            python-json-logger uvicorn

# 4. Run the application
DATABASE_URL=postgresql://user:pass@localhost/demo \
REDIS_URL=redis://localhost:6379/0 \
python examples/enterprise/health-checks.py

# 5. Test the probes
curl http://localhost:8000/health       # liveness  → 200 {"status":"ok"}
curl http://localhost:8000/ready        # readiness → 200 or 503 depending on deps
curl http://localhost:8000/metrics      # Prometheus exposition format

# 6. Kubernetes probe configuration (add to your Deployment manifest)
# livenessProbe:
#   httpGet: { path: /health, port: 8000 }
#   initialDelaySeconds: 5
#   periodSeconds: 10
# readinessProbe:
#   httpGet: { path: /ready, port: 8000 }
#   initialDelaySeconds: 10
#   periodSeconds: 15
#   failureThreshold: 3
```
