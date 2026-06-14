---
title: Lifecycle Hooks
description: Demonstrates how to run setup and teardown logic at application startup and shutdown using Cello's on_event decorator.
---

# :material-refresh: Lifecycle Hooks

Production APIs almost always need to establish shared resources — database pools, cache clients, background workers — before serving traffic, and release them cleanly on shutdown. Cello exposes `@app.on_event("startup")` and `@app.on_event("shutdown")` decorators for exactly this purpose, and supports both `async def` and plain `def` callbacks.

## Features Demonstrated

- Registering an `async def` startup handler with `@app.on_event("startup")`
- Registering a synchronous `def` shutdown handler with `@app.on_event("shutdown")`
- Sharing state initialised during startup with route handlers via a module-level variable
- Simulating async I/O (e.g., connecting to a database) with `asyncio.sleep()`

## Complete Source Code

```python
from cello import App
import asyncio
import time

app = App()

DB_CONNECTION = None

@app.on_event("startup")
async def connect_db():
    global DB_CONNECTION
    print("Connecting to database (simulated)...")
    await asyncio.sleep(0.5)
    DB_CONNECTION = "Connected"
    print("Database Connected")

@app.on_event("shutdown")
def close_db():
    print("Closing database connection...")
    time.sleep(0.1)
    print("Database Closed")

@app.get("/")
def home(request):
    return {"db_status": DB_CONNECTION or "Disconnected"}

if __name__ == "__main__":
    app.run(port=8080)
```

## Running This Example

```bash
python examples/lifecycle_hooks.py
# then test it:
curl http://127.0.0.1:8080/
```

## Key Concepts

- **`@app.on_event("startup")`** — The decorated function is awaited (or called synchronously) once, before the server begins accepting connections, making it safe to perform blocking or async initialisation.
- **`@app.on_event("shutdown")`** — Called when the server receives a shutdown signal (e.g., `SIGINT`/`SIGTERM`), guaranteeing cleanup code runs even if the process is interrupted.
- **Async and sync callbacks** — Both `async def` and plain `def` callbacks are accepted for each event; Cello handles the dispatch automatically.
- **Shared state via module globals** — A simple module-level variable (like `DB_CONNECTION`) is a lightweight way to share resources between lifecycle hooks and route handlers, suitable for single-process deployments.
