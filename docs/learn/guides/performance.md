---
title: Performance
description: Understanding Cello's performance architecture and optimizing your application
---

# Performance

Cello is built for speed. Its Rust core handles TCP, HTTP parsing, routing, JSON serialization, and middleware entirely outside of Python. This guide explains why Cello is fast and how to keep your application running at peak performance.

---

## Architecture Advantages

### Rust Owns the Hot Path

```
Request --> Rust HTTP Engine --> Python Handler --> Rust Response
                |                     |
                +- SIMD JSON          +- Return dict or Response
                +- Radix routing      +- Python business logic only
                +- Middleware (Rust)
```

Every request touches Python only for business logic. Everything else runs in compiled Rust:

| Component | Implementation | Benefit |
|-----------|---------------|---------|
| TCP accept loop | Tokio (Rust) | Zero-copy, epoll/kqueue |
| HTTP parsing | Hyper (Rust) | Streaming, zero-alloc |
| Routing | matchit radix tree | O(log n) lookup |
| JSON serialization | simd-json (Rust) | SIMD-accelerated, 5-10x faster |
| Middleware | Rust trait chain | No Python overhead per request |
| Response building | Rust | Direct byte assembly |

### Key Numbers

On typical hardware (8-core), Cello handles:

- **170,000+ requests/sec** for simple JSON endpoints (4 workers, wrk 12t/400c)
- **1.9x faster** than BlackSheep+Granian, **3.1x faster** than FastAPI+Granian
- **Sub-millisecond** routing and JSON serialization
- **50-70% lower memory** than equivalent Python frameworks

---

## Benchmarking

Use a tool like `wrk`, `hey`, or `oha` to measure throughput.

```bash
# Install wrk (Ubuntu)
sudo apt install wrk

# Benchmark a simple endpoint
wrk -t4 -c100 -d30s http://127.0.0.1:8000/

# With more detail
wrk -t4 -c100 -d30s --latency http://127.0.0.1:8000/
```

### Benchmark Tips

- Always run with `--env production` and `--workers $(nproc)`.
- Disable logging during benchmarks (`--no-logs`).
- Run the benchmark tool on a separate machine to avoid resource contention.
- Warm up the server with a few hundred requests before measuring.

---

## Profiling

### Python Profiling

To find bottlenecks in your handler code:

```python
import cProfile

@app.get("/debug/profile")
def profiled_endpoint(request):
    profiler = cProfile.Profile()
    profiler.enable()

    result = your_business_logic()

    profiler.disable()
    profiler.print_stats(sort="cumtime")
    return result
```

### Rust-side Metrics

Enable Prometheus metrics to measure request latency at the framework level:

```python
app.enable_prometheus(endpoint="/metrics")
```

Check the histogram `cello_http_request_duration_seconds` to see where time is spent.

---

## Optimization Tips

### 1. Return Dicts Instead of Response Objects

Returning a plain `dict` lets Cello serialize JSON entirely in Rust using SIMD instructions. Creating a `Response` object adds a Python allocation.

```python
# Fast -- Rust handles serialization
@app.get("/users")
def list_users(request):
    return {"users": get_users()}

# Slower -- Python creates the Response object first
@app.get("/users")
def list_users(request):
    return Response.json({"users": get_users()})
```

Only use `Response.json()` when you need a custom status code or additional headers.

### 2. Use Path Parameters Over Query Parameters

Path parameters are extracted during routing in the Rust radix tree. Query parameters are parsed from the URL string at runtime.

```python
# Faster -- resolved during routing
@app.get("/users/{id}")
def get_user(request):
    return find_user(request.params["id"])

# Slower -- parsed at request time
@app.get("/users")
def get_user(request):
    return find_user(request.query["id"])
```

### 3. Enable Compression

For responses larger than 1 KB, gzip compression reduces transfer size and improves perceived latency for clients.

```python
app.enable_compression(min_size=1024)
```

### 4. Use Lazy Body Parsing

Cello parses request bodies lazily. If your handler does not call `request.json()` or `request.body()`, the body is never read from the socket. Design read-only endpoints to avoid parsing the body.

### 5. Cache Expensive Responses

Use the `@cache` decorator for endpoints that return data that changes infrequently.

```python
from cello import cache

@app.get("/reports/summary")
@cache(ttl=300, tags=["reports"])
def summary(request):
    return compute_expensive_report()
```

### 6. Avoid Blocking Calls in Async Handlers

Never use synchronous I/O inside an `async def` handler. This blocks the Tokio runtime thread.

```python
# Bad -- blocks the event loop
@app.get("/data")
async def get_data(request):
    import time
    time.sleep(1)  # DO NOT do this
    return {"data": "value"}

# Good -- use async I/O
@app.get("/data")
async def get_data(request):
    import asyncio
    await asyncio.sleep(1)
    return {"data": "value"}
```

---

## Connection Pooling

For database-heavy applications, use connection pooling to avoid the overhead of creating a new connection per request.

```python
from cello import App, DatabaseConfig

app = App()
app.enable_database(DatabaseConfig(
    url="postgresql://user:pass@localhost/mydb",
    pool_size=20,
    max_lifetime_secs=1800,
))
```

The pool is managed in Rust and shared across all worker threads.

---

## Cluster Mode

For multi-process scaling, enable cluster mode to fork multiple processes, each with its own set of worker threads.

```python
from cello import ClusterConfig

app.run(
    host="0.0.0.0",
    port=8000,
    workers=4,
    cluster=ClusterConfig(processes=4),
)
```

This creates 4 processes x 4 threads = 16 concurrent execution contexts.

---

## Performance Checklist

| Area | Action |
|------|--------|
| **Handlers** | Return `dict` instead of `Response` when possible |
| **Routing** | Prefer path parameters over query parameters |
| **Compression** | Enable for responses > 1 KB |
| **Caching** | Use `@cache` for read-heavy endpoints |
| **Async** | Use `async def` for I/O-bound handlers |
| **Blocking** | Never use `time.sleep()` or sync HTTP in async handlers |
| **Workers** | Set to CPU count (`--workers $(nproc)`) |
| **Logging** | Disable request logging in production benchmarks |
| **Connection pool** | Use database connection pooling |
| **Monitoring** | Enable Prometheus metrics to detect regressions |

---

## Next Steps

- See the [Deployment guide](deployment.md) for production configuration.
- See the [Server configuration reference](../../reference/config/server.md) for all tuning options.
- Enable [Prometheus metrics](../../enterprise/observability/metrics.md) for continuous performance monitoring.
