---
title: Adaptive Rate Limiting
description: Automatically adjust request rate limits based on error rates to protect your service under load.
---

# :material-speedometer: Adaptive Rate Limiting

Cello's adaptive rate limiter dynamically tightens capacity when error rates climb above a configurable threshold and relaxes it again once the service recovers. This helps protect downstream services without requiring manual tuning during traffic spikes.

The example below starts with a bucket of 100 tokens, refills at 100 tokens per second, and shrinks to a minimum capacity of 5 when errors exceed 20 % of requests.

## Features Demonstrated

- Token-bucket rate limiter configured with `RateLimitConfig.adaptive()`
- Dynamic capacity adjustment driven by real-time error rate
- `min_capacity` floor to preserve a small throughput window even under high error conditions
- `error_threshold` controlling when adaptive reduction kicks in
- Enabling the limiter globally via `app.enable_rate_limit(config)`

## Complete Source Code

```python
from cello import App, RateLimitConfig, Response
import time
import threading

app = App()

config = RateLimitConfig.adaptive(
    capacity=100,
    refill_rate=100,
    min_capacity=5,
    error_threshold=0.20
)
app.enable_rate_limit(config)

@app.get("/")
def home(request):
    return {"status": "ok", "message": "System is healthy"}

@app.get("/trigger-errors")
def trigger_errors(request):
    return Response.json({"error": "Simulated Warning"}, status=500)

if __name__ == "__main__":
    app.run(port=8080)
```

## Running This Example

```bash
python examples/adaptive_rate_limit.py
```

```bash
# Check healthy endpoint
curl http://localhost:8080/

# Trigger errors to drive up the error rate
curl http://localhost:8080/trigger-errors

# Flood the server — watch for 429 responses as capacity shrinks
for i in $(seq 1 120); do curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8080/; done
```

## Key Concepts

- **Token bucket algorithm** — each request consumes one token; tokens refill at `refill_rate` per second up to `capacity`
- **Adaptive reduction** — when the proportion of 5xx responses exceeds `error_threshold`, the effective capacity is reduced toward `min_capacity`
- **Automatic recovery** — as the error rate falls back below the threshold the limiter gradually restores full capacity
- **HTTP 429 Too Many Requests** — clients receive this status code when the bucket is exhausted, allowing them to back off and retry
- **`min_capacity` safety floor** — prevents capacity from reaching zero so the service never becomes completely unavailable
