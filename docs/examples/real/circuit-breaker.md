---
title: Circuit Breaker Pattern
description: Protect your service from cascading failures by automatically opening a circuit when error thresholds are exceeded.
---

# :material-electric-switch: Circuit Breaker Pattern

A circuit breaker monitors outgoing calls and, when failures accumulate beyond a threshold, opens the circuit so subsequent requests fail fast instead of piling up. After a configurable timeout the breaker moves to a *half-open* state, allows a probe request through, and either closes or trips again based on the result.

This example wires Cello's built-in circuit breaker to a deliberately flaky endpoint so you can observe all three states — closed, open, and half-open — through curl commands.

## Features Demonstrated

- Circuit breaker enabled globally with `app.enable_circuit_breaker()`
- `failure_threshold` — number of consecutive failures before the circuit opens
- `reset_timeout` — seconds to wait before moving to the half-open probe state
- `half_open_target` — number of successful probe requests needed to close the circuit again
- Simulated failures using `Response.json` with a 500 status
- A clean recovery route to demonstrate circuit re-closure

## Complete Source Code

```python
from cello import App, Response
import time

app = App()
app.enable_circuit_breaker(failure_threshold=3, reset_timeout=5, half_open_target=1)

@app.get("/")
def home(request):
    return {"status": "ok", "message": "System is healthy"}

@app.get("/flaky")
def flaky(request):
    resp = Response.json({"error": "Simulated Failure"})
    resp.set_status(500)
    return resp

@app.get("/recover")
def recover(request):
    return {"status": "recovered"}

@app.get("/test_cb")
def test_cb(request):
    if "fail" in request.query_params and request.query_params["fail"] == "true":
        resp = Response.json({"error": "Simulated Failure"})
        resp.set_status(500)
        return resp
    return {"status": "ok"}

if __name__ == "__main__":
    app.run(port=8082)
```

## Running This Example

```bash
python examples/circuit_breaker.py
```

```bash
# Verify the service is healthy (circuit CLOSED)
curl http://localhost:8082/

# Trigger 3 failures to open the circuit
curl http://localhost:8082/flaky
curl http://localhost:8082/flaky
curl http://localhost:8082/flaky

# Circuit is now OPEN — requests should fail fast with 503
curl http://localhost:8082/

# Wait for reset_timeout (5 s) then send a probe — circuit moves to HALF-OPEN
sleep 5
curl http://localhost:8082/recover

# Circuit is CLOSED again — normal traffic resumes
curl http://localhost:8082/

# Use query param to control failures programmatically
curl "http://localhost:8082/test_cb?fail=true"
curl "http://localhost:8082/test_cb?fail=false"
```

## Key Concepts

- **Closed state** — normal operation; every request passes through and failures are counted
- **Open state** — triggered after `failure_threshold` consecutive errors; all requests immediately receive a 503 without hitting the handler
- **Half-open state** — entered after `reset_timeout` seconds; `half_open_target` successful probe requests are required to re-close the circuit
- **Fail-fast behaviour** — open circuit responses are returned in microseconds, preventing thread exhaustion and downstream overload
- **`reset_timeout`** — controls how long the breaker waits before attempting recovery, giving downstream services time to stabilise
- **Query-parameter driven testing** — the `/test_cb?fail=true` pattern lets you script deterministic failure/recovery sequences in automated tests
