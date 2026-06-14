---
title: Async & Sync Handlers
description: Demonstrates how Cello transparently supports both synchronous and asynchronous route handlers side-by-side.
---

# :material-lightning-bolt: Async & Sync Handlers

Cello is built on top of an ASGI foundation, which means it can handle both regular `def` handlers and `async def` handlers without any extra configuration. This example shows both styles running in the same application, proving they coexist seamlessly.

## Features Demonstrated

- Defining a plain synchronous `def` route handler
- Defining a coroutine `async def` route handler with `await`
- Mixing sync and async handlers freely within one `App` instance
- Using `asyncio.sleep()` to simulate non-blocking I/O (e.g., database queries)

## Complete Source Code

```python
"""Test async and sync handlers work correctly."""
import asyncio
from cello import App

app = App()

@app.get("/sync")
def sync_handler(request):
    return {"type": "sync", "message": "Hello from sync handler!"}

@app.get("/async")
async def async_handler(request):
    await asyncio.sleep(0.1)
    return {"type": "async", "message": "Hello from async handler!"}

@app.get("/users/{id}")
async def get_user(request):
    user_id = request.params["id"]
    await asyncio.sleep(0.05)
    return {"id": user_id, "name": f"User {user_id}"}

if __name__ == "__main__":
    app.run()
```

## Running This Example

```bash
python examples/async_demo.py
# then test it:
curl http://127.0.0.1:8000/sync
curl http://127.0.0.1:8000/async
curl http://127.0.0.1:8000/users/42
```

## Key Concepts

- **ASGI under the hood** — Cello's server loop runs inside an event loop, so `async def` handlers are awaited natively without any thread-pool overhead.
- **Sync handler wrapping** — Plain `def` handlers are automatically run in a thread executor so they never block the event loop, keeping the server responsive.
- **`await asyncio.sleep()`** — A lightweight stand-in for real async I/O (database calls, HTTP requests); replace with actual `await`-able calls in production.
- **Transparent path params** — `request.params["id"]` works identically in both sync and async handlers — no API differences between the two styles.
