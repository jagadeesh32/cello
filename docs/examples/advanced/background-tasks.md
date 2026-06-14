---
title: Background Tasks
description: Offload slow work to background workers with retries
tags:
  - Background Tasks
  - Async
  - Workers
  - Queue
  - Performance
  - Examples
---

# :material-cog-transfer: Background Tasks

Long-running operations such as PDF report generation, data exports, or third-party API calls should never block an HTTP response. This example shows how to enqueue tasks from a Cello route, poll their status via a dedicated endpoint, and automatically retry failed work with exponential back-off — all without an external broker by using Python's `asyncio` queue.

## Complete Example

```python
import asyncio
import uuid
import time
import random
import traceback
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable, Coroutine, Optional

import cello
from cello import Request, Response, on_startup, on_shutdown

# ---------------------------------------------------------------------------
# Task status model
# ---------------------------------------------------------------------------

class TaskStatus(str, Enum):
    PENDING   = "pending"
    RUNNING   = "running"
    SUCCESS   = "success"
    FAILED    = "failed"
    RETRYING  = "retrying"


@dataclass
class TaskRecord:
    id:          str
    kind:        str                    # human-readable task type, e.g. "report"
    payload:     dict
    status:      TaskStatus = TaskStatus.PENDING
    result:      Any        = None
    error:       str        = ""
    attempts:    int        = 0
    max_retries: int        = 3
    created_at:  float      = field(default_factory=time.time)
    updated_at:  float      = field(default_factory=time.time)
    started_at:  Optional[float] = None
    finished_at: Optional[float] = None

    def to_dict(self) -> dict:
        return {
            "id":          self.id,
            "kind":        self.kind,
            "status":      self.status,
            "attempts":    self.attempts,
            "max_retries": self.max_retries,
            "result":      self.result,
            "error":       self.error,
            "created_at":  self.created_at,
            "updated_at":  self.updated_at,
            "started_at":  self.started_at,
            "finished_at": self.finished_at,
        }


# ---------------------------------------------------------------------------
# In-process task queue
# ---------------------------------------------------------------------------

# In production swap this dict for Redis/Postgres so state survives restarts.
_task_store: dict[str, TaskRecord] = {}
_queue: asyncio.Queue[TaskRecord]  = asyncio.Queue()

# Registry of worker functions keyed by task kind
_handlers: dict[str, Callable[..., Coroutine]] = {}


def register_handler(kind: str):
    """Decorator: register an async function as the worker for *kind*."""
    def decorator(fn: Callable):
        _handlers[kind] = fn
        return fn
    return decorator


async def enqueue(kind: str, payload: dict, max_retries: int = 3) -> TaskRecord:
    """Create a ``TaskRecord``, store it, and push it onto the queue."""
    task = TaskRecord(
        id=str(uuid.uuid4()),
        kind=kind,
        payload=payload,
        max_retries=max_retries,
    )
    _task_store[task.id] = task
    await _queue.put(task)
    return task


# ---------------------------------------------------------------------------
# Worker loop — runs as a background asyncio task
# ---------------------------------------------------------------------------

async def _worker_loop(worker_id: int):
    """
    Continuously pull tasks from the queue and execute them.

    Retry policy
    ------------
    On failure the task is re-queued after an exponential back-off delay
    (2 ** attempt seconds, capped at 30 s).  After *max_retries* the task
    is marked FAILED and removed from the queue permanently.
    """
    print(f"[worker-{worker_id}] started")
    while True:
        task = await _queue.get()

        # Mark as running
        task.status     = TaskStatus.RUNNING
        task.attempts  += 1
        task.started_at = time.time()
        task.updated_at = time.time()
        print(f"[worker-{worker_id}] executing {task.kind!r} task {task.id} "
              f"(attempt {task.attempts}/{task.max_retries})")

        handler = _handlers.get(task.kind)
        if handler is None:
            task.status     = TaskStatus.FAILED
            task.error      = f"No handler registered for task kind {task.kind!r}"
            task.finished_at = time.time()
            task.updated_at  = time.time()
            _queue.task_done()
            continue

        try:
            task.result      = await handler(task.payload)
            task.status      = TaskStatus.SUCCESS
            task.finished_at = time.time()
            task.updated_at  = time.time()
            print(f"[worker-{worker_id}] ✓ {task.id} completed successfully")

        except Exception as exc:
            task.error      = traceback.format_exc()
            task.updated_at = time.time()

            if task.attempts < task.max_retries:
                delay = min(2 ** task.attempts, 30)
                task.status = TaskStatus.RETRYING
                print(f"[worker-{worker_id}] ✗ {task.id} failed "
                      f"(retrying in {delay}s): {exc}")
                await asyncio.sleep(delay)
                await _queue.put(task)          # re-enqueue for retry
            else:
                task.status      = TaskStatus.FAILED
                task.finished_at = time.time()
                print(f"[worker-{worker_id}] ✗ {task.id} permanently failed "
                      f"after {task.attempts} attempts: {exc}")

        finally:
            _queue.task_done()


# ---------------------------------------------------------------------------
# Lifecycle hooks
# ---------------------------------------------------------------------------

NUM_WORKERS = 3
_worker_tasks: list[asyncio.Task] = []


@on_startup
async def start_workers():
    """Spin up the worker pool when the Cello app starts."""
    for i in range(NUM_WORKERS):
        t = asyncio.create_task(_worker_loop(i))
        _worker_tasks.append(t)
    print(f"[queue] {NUM_WORKERS} background workers running")


@on_shutdown
async def stop_workers():
    """Drain the queue then cancel workers on graceful shutdown."""
    print("[queue] draining queue …")
    await _queue.join()          # wait for all in-flight tasks
    for t in _worker_tasks:
        t.cancel()
    print("[queue] all workers stopped")


# ---------------------------------------------------------------------------
# Task handlers (the actual slow work)
# ---------------------------------------------------------------------------

@register_handler("report")
async def generate_report(payload: dict) -> dict:
    """
    Simulate report generation: fetch data, crunch numbers, write PDF.
    Randomly fails ~20 % of the time so you can observe retry behaviour.
    """
    report_type = payload.get("type", "summary")
    filters     = payload.get("filters", {})

    print(f"  [report] building {report_type!r} report with filters={filters}")

    # Simulate variable-length work (2–5 s)
    await asyncio.sleep(random.uniform(2, 5))

    # Simulate transient failures
    if random.random() < 0.2:
        raise RuntimeError("Upstream data service timed out (simulated)")

    rows = random.randint(50, 500)
    return {
        "report_type": report_type,
        "rows_processed": rows,
        "download_url": f"https://storage.example.com/reports/{uuid.uuid4()}.pdf",
    }


@register_handler("email_blast")
async def send_email_blast(payload: dict) -> dict:
    """Simulate sending a batch of marketing e-mails."""
    recipients = payload.get("recipients", [])
    template   = payload.get("template", "default")

    print(f"  [email] sending {template!r} to {len(recipients)} recipients")
    await asyncio.sleep(len(recipients) * 0.05)   # 50 ms per recipient

    return {"sent": len(recipients), "template": template}


# ---------------------------------------------------------------------------
# Cello routes
# ---------------------------------------------------------------------------

app = cello.App()


@app.route("/reports", methods=["POST"])
async def create_report(req: Request) -> Response:
    """
    Enqueue a report generation job.

    Body (JSON)::

        {
            "type":    "monthly",
            "filters": {"department": "engineering", "month": "2026-05"}
        }

    Returns the task record immediately (status = "pending") so the client
    can start polling ``GET /reports/{id}``.
    """
    body = await req.json()
    task = await enqueue(
        kind="report",
        payload={
            "type":    body.get("type", "summary"),
            "filters": body.get("filters", {}),
        },
        max_retries=body.get("max_retries", 3),
    )
    return Response.json(task.to_dict(), status=202)


@app.route("/reports/{task_id}", methods=["GET"])
async def get_report_status(req: Request, task_id: str) -> Response:
    """
    Poll the status of an enqueued report task.

    The client should poll every few seconds until ``status`` is
    ``"success"`` or ``"failed"``.  A ``Retry-After`` header is returned
    while the task is still in progress.
    """
    task = _task_store.get(task_id)
    if task is None:
        return Response.json({"error": "Task not found"}, status=404)

    data = task.to_dict()
    headers = {}
    if task.status in (TaskStatus.PENDING, TaskStatus.RUNNING, TaskStatus.RETRYING):
        headers["Retry-After"] = "3"

    return Response.json(data, headers=headers)


@app.route("/reports", methods=["GET"])
async def list_reports(req: Request) -> Response:
    """List all report tasks (optionally filtered by status)."""
    status_filter = req.query_params.get("status")
    tasks = list(_task_store.values())
    if status_filter:
        tasks = [t for t in tasks if t.status == status_filter]
    # Most recent first
    tasks.sort(key=lambda t: t.created_at, reverse=True)
    return Response.json({
        "tasks": [t.to_dict() for t in tasks],
        "total": len(tasks),
    })


@app.route("/emails", methods=["POST"])
async def create_email_blast(req: Request) -> Response:
    """Enqueue a bulk e-mail send and return immediately."""
    body = await req.json()
    task = await enqueue(
        kind="email_blast",
        payload={
            "recipients": body.get("recipients", []),
            "template":   body.get("template", "default"),
        },
    )
    return Response.json(task.to_dict(), status=202)


@app.route("/queue/stats", methods=["GET"])
async def queue_stats(req: Request) -> Response:
    """Return a snapshot of queue health metrics."""
    counts: dict[str, int] = {s.value: 0 for s in TaskStatus}
    for task in _task_store.values():
        counts[task.status] += 1
    return Response.json({
        "queue_size":    _queue.qsize(),
        "total_tasks":   len(_task_store),
        "worker_count":  NUM_WORKERS,
        "status_counts": counts,
    })


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **Task queue setup** — an `asyncio.Queue` backed by an in-process `dict` store keeps the example self-contained; swap the dict for Redis/Postgres in production to survive restarts.
- **Handler registry** — `@register_handler("report")` decouples routing from execution; any coroutine can be registered as the worker for a named task kind.
- **Enqueue on `POST /reports`** — the HTTP handler returns `202 Accepted` with the task record immediately so the client is never blocked by the slow work.
- **Status check `GET /reports/{id}`** — a lightweight polling endpoint returns the live `TaskRecord`; a `Retry-After` header guides the client on how often to poll.
- **Retry logic** — on failure the worker re-queues the task (up to `max_retries`) with an exponential back-off delay (`2 ** attempt` seconds, capped at 30 s), preventing thundering herds.
- **Worker pool** — `NUM_WORKERS` concurrent asyncio tasks drain the queue in parallel; started via `@on_startup` and gracefully drained on `@on_shutdown` with `Queue.join()`.
- **Graceful shutdown** — `_queue.join()` ensures all in-flight tasks complete before the process exits, preventing data loss.

## Running This Example

```bash
# Install dependencies
pip install cello

# Run the server
python examples/advanced/background-tasks.py
```

Trigger a report job and poll until it finishes:

```bash
# Enqueue a report (returns immediately with status=pending)
TASK=$(curl -s -X POST http://localhost:8000/reports \
  -H "Content-Type: application/json" \
  -d '{"type":"monthly","filters":{"department":"engineering"}}')

echo $TASK | python -m json.tool
TASK_ID=$(echo $TASK | python -c "import sys,json; print(json.load(sys.stdin)['id'])")

# Poll until done
while true; do
  STATUS=$(curl -s http://localhost:8000/reports/$TASK_ID | python -c \
    "import sys,json; t=json.load(sys.stdin); print(t['status'])")
  echo "Status: $STATUS"
  [ "$STATUS" = "success" ] || [ "$STATUS" = "failed" ] && break
  sleep 3
done

# Inspect queue health
curl -s http://localhost:8000/queue/stats | python -m json.tool
```
