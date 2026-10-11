# Blocking Handlers & the Threadpool

Sync `def` handlers run **inline** on Cello's server thread. That is the fastest
possible path for a handler that just builds a value and returns it — there is no
thread hop, no queue, no handoff.

It is the *wrong* path for a handler that **blocks**. A synchronous database
driver, `requests.get()`, `time.sleep()`, or a blocking file read holds the server
thread for its entire duration, and while it is held nothing else can be accepted,
parsed, or answered. Throughput collapses to one request at a time: a handler that
blocks for 10 ms caps the whole worker at ~100 requests/second.

Cello solves this **automatically**. Sync handlers are timed, and any handler that
repeatedly exceeds the offload threshold is permanently moved onto a bounded pool
of OS threads, where blocking calls overlap instead of queueing.

```python
from cello import App
import time

app = App()

@app.get("/fast")
def fast(request):
    return {"ok": True}          # stays inline — no threadpool overhead

@app.get("/slow")
def slow(request):
    time.sleep(0.010)            # detected as blocking, moved to the pool
    return {"ok": True}
```

Measured on the same 10 ms handler, single worker, 50 concurrent connections:

| | Requests/sec | p50 latency |
|---|---|---|
| Inline (blocking, no offload) | 94 | 508 ms |
| Offloaded to the threadpool | **3,735** | 11.7 ms |

Trivial handlers are unaffected — the inline fast path is preserved exactly, at
~26,000 req/s on the same machine.

## How the decision is made

Each sync handler is timed **inside the GIL**, measuring only the handler call
itself. When a call exceeds `offload_threshold_ms`, a counter increments; after two
**consecutive** slow calls the handler is marked blocking and every later request
goes to the pool.

The decision is:

- **Sticky** — once promoted, a handler never returns to the inline path, so it
  cannot flap under changing load.
- **Two-sample** — a handler's very first call pays one-time warmup that can exceed
  the threshold on its own. Requiring two consecutive slow calls stops that from
  misclassifying cheap handlers. In exchange, at most the first two requests to a
  genuinely blocking handler run inline.
- **Per handler** — not per route pattern, not global.

Async `def` handlers are not affected by any of this; they already run on the
persistent asyncio event loop.

## Taking control with `blocking=`

Every route decorator accepts `blocking=`, on both `App` and `Blueprint`:

```python
@app.get("/db", blocking=True)
def query(request):
    return db.execute("SELECT ...")   # pooled from the very first request

@app.get("/tiny", blocking=False)
def tiny(request):
    return {"ok": True}               # pinned inline, never promoted
```

Use `blocking=True` when you already know a handler blocks and you do not want the
first two requests to run inline. Use `blocking=False` to opt a handler out of
timing entirely.

## Configuration

```python
from cello import App, ThreadPoolConfig

app = App()
app.set_threadpool(ThreadPoolConfig(
    size=64,                 # max blocking threads
    offload_threshold_ms=1,  # a sync call slower than this counts as blocking
    adaptive=True,           # False -> only blocking=True handlers are pooled
))
```

Call `set_threadpool()` before `run()` — the pool size is fixed when the runtime is
built.

| Option | Default | Meaning |
|---|---|---|
| `size` | `64` | Maximum concurrent blocking threads |
| `offload_threshold_ms` | `1` | Duration above which a sync call counts as slow |
| `adaptive` | `True` | When `False`, only `blocking=True` handlers are pooled |

Sizing: the ceiling for a handler that blocks for `T` seconds is roughly
`size / T` requests per second. At the default 64 threads, a 10 ms handler tops out
around 6,400 req/s per worker. Raising `size` costs memory — each OS thread
reserves stack space — and increases GIL contention.

## Things worth knowing

**Only GIL-releasing work benefits.** `time.sleep`, socket I/O, database drivers,
and most C extensions release the GIL while they wait, so they genuinely run in
parallel. Pure-Python CPU-bound work does **not** release the GIL and will not go
faster on the pool — for that, use more workers (`app.run(workers=N)`), which is
Cello's model for using multiple cores.

**The pool is shared.** Offloaded sync handlers, async coroutine waits, and
background tasks all draw from the same pool. Size it for the total.

**Handler timeouts do not reclaim threads.** `handler_timeout` (see
[`set_timeouts`](../../reference/api/plugins.md)) returns `504` to the client, but a
handler that is genuinely stuck keeps its pool thread until it returns. A handler
that can hang forever should have its own internal timeout.

**Handlers may run on different threads.** Do not rely on thread-local state
persisting across requests, and guard any shared mutable state you touch from a
handler that may be offloaded.
