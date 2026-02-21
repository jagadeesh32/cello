---
title: Circuit Breaker
description: Circuit breaker middleware for fault tolerance in Cello Framework
---

# Circuit Breaker

Cello includes a circuit breaker middleware implemented in Rust. It monitors response failures and temporarily stops processing requests when a failure threshold is exceeded, giving downstream services time to recover.

## Quick Start

```python
from cello import App

app = App()

app.enable_circuit_breaker(
    failure_threshold=5,     # Open after 5 failures
    reset_timeout=30,        # Wait 30 seconds before half-open
    half_open_target=3       # 3 successes to close again
)
```

---

## The Circuit Breaker Pattern

The circuit breaker operates in three states:

```
                ┌──────────────────────┐
                │       CLOSED         │
                │  (normal operation)  │
                └──────────┬───────────┘
                           │
              failure_threshold reached
                           │
                           ▼
                ┌──────────────────────┐
                │        OPEN          │
                │ (reject all requests)│
                │  Returns 503        │
                └──────────┬───────────┘
                           │
                  reset_timeout expires
                           │
                           ▼
                ┌──────────────────────┐
                │     HALF-OPEN        │
                │ (allow test requests)│
                └─────┬──────────┬─────┘
                      │          │
           success    │          │  failure
           count met  │          │
                      ▼          ▼
                   CLOSED      OPEN
```

### States

| State | Behavior |
|-------|----------|
| **Closed** | Normal operation. Requests pass through. Failures are counted. |
| **Open** | All requests are rejected immediately with `503 Service Unavailable`. No load on downstream services. |
| **Half-Open** | A limited number of test requests are allowed through. If they succeed, the circuit closes. If they fail, it opens again. |

---

## Configuration

```python
app.enable_circuit_breaker(
    failure_threshold=5,
    reset_timeout=30,
    half_open_target=3,
    failure_codes=[500, 502, 503, 504]
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `failure_threshold` | `int` | `5` | Number of failures before opening the circuit |
| `reset_timeout` | `int` | `30` | Seconds to wait in Open state before moving to Half-Open |
| `half_open_target` | `int` | `3` | Successful requests needed in Half-Open to close the circuit |
| `failure_codes` | `list[int]` | `[500, 502, 503, 504]` | HTTP status codes considered failures |

---

## How Failures Are Detected

The circuit breaker counts responses with status codes in the `failure_codes` list. By default, these are server errors:

- `500 Internal Server Error`
- `502 Bad Gateway`
- `503 Service Unavailable`
- `504 Gateway Timeout`

Client errors (4xx) are **not** counted as failures because they indicate bad input, not a system problem.

### Custom Failure Codes

```python
# Include 408 Request Timeout as a failure
app.enable_circuit_breaker(
    failure_threshold=5,
    reset_timeout=30,
    failure_codes=[408, 500, 502, 503, 504]
)
```

---

## Response When Open

When the circuit is open, all requests receive:

```
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
Retry-After: 25

{"error": "Service temporarily unavailable", "retry_after": 25}
```

The `Retry-After` header tells clients how many seconds until the circuit enters Half-Open and may start accepting requests again.

---

## Example: Protecting a Fragile Endpoint

```python
from cello import App

app = App()

# Circuit breaker protects all routes
app.enable_circuit_breaker(
    failure_threshold=3,      # Open after just 3 failures
    reset_timeout=60,         # Wait 1 minute before retrying
    half_open_target=2        # 2 successes to fully recover
)

@app.get("/api/external-data")
async def external_data(request):
    # If this endpoint fails 3 times in a row,
    # the circuit opens and returns 503 for 60 seconds
    data = await fetch_from_external_api()
    return {"data": data}

@app.get("/health")
def health(request):
    return {"status": "ok"}
```

---

## Recovery Flow

A typical failure and recovery sequence:

```
Time 0s   - Request 1: 500 (failure count: 1)
Time 1s   - Request 2: 500 (failure count: 2)
Time 2s   - Request 3: 500 (failure count: 3)
Time 3s   - Request 4: 500 (failure count: 4)
Time 4s   - Request 5: 500 (failure count: 5 → CIRCUIT OPENS)
Time 5s   - Request 6: 503 (circuit open, rejected)
Time 10s  - Request 7: 503 (circuit open, rejected)
Time 34s  - Circuit enters HALF-OPEN
Time 35s  - Request 8: 200 (success count: 1)
Time 36s  - Request 9: 200 (success count: 2)
Time 37s  - Request 10: 200 (success count: 3 → CIRCUIT CLOSES)
Time 38s  - Request 11: 200 (normal operation)
```

---

## Combining with Other Middleware

The circuit breaker works well alongside rate limiting and caching:

```python
app = App()

# Rate limiting prevents abuse
app.enable_rate_limit(RateLimitConfig.token_bucket(requests=100, window=60))

# Circuit breaker protects against cascading failures
app.enable_circuit_breaker(failure_threshold=5, reset_timeout=30)

# Caching reduces load on recovering services
app.enable_caching(ttl=60)
```

!!! tip "Defense in Depth"
    Rate limiting protects your server from external abuse. The circuit breaker protects your server from downstream failures. Together they provide comprehensive fault tolerance.

---

## Performance

The circuit breaker uses atomic state checks in Rust:

| Operation | Overhead |
|-----------|----------|
| State check (Closed) | ~50ns |
| State check (Open, reject) | ~20ns |
| Failure counter update | ~50ns |

When the circuit is open, requests are rejected in under 20 nanoseconds -- faster than any other middleware because no processing occurs at all.

---

## Next Steps

- [Middleware Overview](overview.md) - Full middleware system
- [Rate Limiting](rate-limiting.md) - Request throttling
- [Caching](caching.md) - Response caching
