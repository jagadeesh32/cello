---
title: Background Tasks
description: Post-response task execution in Cello Framework
---

# Background Tasks

Cello provides a `BackgroundTasks` system that lets you schedule work to run **after** the HTTP response has been sent to the client. This is useful for operations that the client does not need to wait for, such as sending emails, writing audit logs, or updating caches.

---

## Overview

```
Client Request
       |
   Handler runs
       |
   Response sent to client  <-- client connection closes here
       |
   Background tasks execute  <-- tasks run after the response
```

Background tasks are executed in order, and each task is isolated so that a failure in one task does not prevent subsequent tasks from running.

---

## Basic Usage

### The `BackgroundTasks` Class

Import `BackgroundTasks` and create an instance to queue tasks:

```python
from cello import App, BackgroundTasks

app = App()

def send_welcome_email(email: str, name: str):
    """Simulate sending an email (runs after response)."""
    print(f"Sending welcome email to {email} for {name}")

@app.post("/users")
def create_user(request):
    data = request.json()

    # Create the user in the database
    user = {"id": 1, "name": data["name"], "email": data["email"]}

    # Schedule the email to be sent after the response
    tasks = BackgroundTasks()
    tasks.add_task(send_welcome_email, [data["email"], data["name"]])

    return {"created": True, "user": user}
```

The client receives the response immediately. The `send_welcome_email` function runs afterward.

---

## Adding Tasks

### `add_task(func, args)`

Queue a Python callable with positional arguments:

```python
tasks = BackgroundTasks()

# Single argument
tasks.add_task(log_event, ["user_created"])

# Multiple arguments
tasks.add_task(send_notification, [user_id, "Welcome!", channel])

# No arguments
tasks.add_task(cleanup_temp_files, [])
```

### Multiple Tasks

You can add several tasks to the same queue. They execute in the order they were added:

```python
@app.post("/orders")
def create_order(request):
    data = request.json()
    order = save_order(data)

    tasks = BackgroundTasks()
    tasks.add_task(send_confirmation_email, [order["id"], data["email"]])
    tasks.add_task(update_inventory, [order["items"]])
    tasks.add_task(notify_warehouse, [order["id"]])

    return {"order_id": order["id"], "status": "created"}
```

Execution order: `send_confirmation_email` -> `update_inventory` -> `notify_warehouse`.

---

## Task Execution Order

Tasks are stored in a FIFO queue (first in, first out). When the response is sent, the queue is drained and tasks execute sequentially:

```python
tasks = BackgroundTasks()
tasks.add_task(step_one, [])    # Runs first
tasks.add_task(step_two, [])    # Runs second
tasks.add_task(step_three, [])  # Runs third
```

!!! info "Sequential Execution"
    Tasks run sequentially within a single request's queue. This guarantees ordering when tasks depend on each other. If you need parallelism, spawn your own threads or use `asyncio.gather` inside a single task.

---

## Error Handling in Tasks

Background tasks are wrapped in panic-safe execution. If a task raises an exception or panics, the error is logged and the remaining tasks continue:

```python
def risky_task():
    raise ValueError("Something went wrong")

def safe_task():
    print("This still runs")

tasks = BackgroundTasks()
tasks.add_task(risky_task, [])   # Fails, error is logged
tasks.add_task(safe_task, [])    # Still executes
```

The error output for a failed task:

```
Background task 'python_task' failed: ValueError: Something went wrong
```

!!! warning "No Retry"
    Failed background tasks are not retried automatically. If you need retry logic, implement it within your task function or use a dedicated task queue like Celery.

---

## Checking the Queue

You can inspect the queue before execution:

```python
tasks = BackgroundTasks()
tasks.add_task(fn_a, [])
tasks.add_task(fn_b, [])

# Check pending count
print(tasks.pending_count())  # 2

# Execute all pending tasks
tasks.run_all()

print(tasks.pending_count())  # 0
```

---

## Practical Examples

### Audit Logging

```python
import json
from datetime import datetime

def write_audit_log(user_id: str, action: str, details: str):
    log_entry = {
        "timestamp": datetime.utcnow().isoformat(),
        "user_id": user_id,
        "action": action,
        "details": details,
    }
    with open("audit.log", "a") as f:
        f.write(json.dumps(log_entry) + "\n")

@app.delete("/users/{id}")
def delete_user(request):
    user_id = request.params["id"]

    # Delete the user
    db.delete_user(user_id)

    # Log the action after response
    tasks = BackgroundTasks()
    tasks.add_task(write_audit_log, [user_id, "DELETE", "User deleted"])

    return {"deleted": True}
```

### Cache Warming

```python
def warm_cache(keys: list):
    """Pre-populate cache entries after an update."""
    for key in keys:
        data = db.fetch(key)
        cache.set(key, data, ttl=300)

@app.put("/products/{id}")
def update_product(request):
    product_id = request.params["id"]
    data = request.json()
    db.update_product(product_id, data)

    tasks = BackgroundTasks()
    tasks.add_task(warm_cache, [["product:" + product_id, "product_list"]])

    return {"updated": True}
```

### Webhook Delivery

```python
import urllib.request
import json

def deliver_webhook(url: str, payload: dict):
    """POST a webhook payload to an external URL."""
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        urllib.request.urlopen(req, timeout=10)
    except Exception as e:
        print(f"Webhook delivery failed: {e}")

@app.post("/events")
def create_event(request):
    event = request.json()
    db.save_event(event)

    tasks = BackgroundTasks()
    for subscriber in get_webhook_subscribers(event["type"]):
        tasks.add_task(deliver_webhook, [subscriber["url"], event])

    return {"event_id": event["id"]}
```

---

## Performance

Background tasks are executed using Tokio's `spawn_blocking`, which offloads work to a thread pool. This avoids blocking the async event loop:

| Operation | Overhead |
|-----------|----------|
| Queuing a task | ~100ns (lock-free push) |
| Task dispatch | ~1us (channel send) |
| Panic recovery | ~500ns per task |

---

## Next Steps

- [Dependency Injection](dependency-injection.md) - Inject services into handlers and tasks
- [Lifecycle Hooks](../../getting-started/configuration.md) - Run setup/teardown at startup and shutdown
- [Templates](templates.md) - Render templates in background tasks (e.g., email templates)
