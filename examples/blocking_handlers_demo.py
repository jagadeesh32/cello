"""Blocking sync handlers and the threadpool.

Sync ``def`` handlers run inline on the server thread, which is the fastest path
for a handler that just builds a value. A handler that *blocks* would hold that
thread for its whole duration and stall every other connection, so Cello times
sync handlers and moves the blocking ones onto a bounded threadpool.

Run:
    python examples/blocking_handlers_demo.py

Then compare, with any HTTP load generator:

    wrk -t4 -c50 -d8s http://127.0.0.1:8000/blocking     # offloaded
    wrk -t4 -c50 -d8s http://127.0.0.1:8000/pinned       # inline, serialised
    wrk -t4 -c50 -d8s http://127.0.0.1:8000/fast         # inline, full speed

On a single worker, ``/blocking`` sustains thousands of req/s while ``/pinned``
sits at the one-request-at-a-time ceiling of ~100 req/s.
"""

import threading
import time

from cello import App, ThreadPoolConfig

app = App()

# Optional — these are the defaults.
app.set_threadpool(ThreadPoolConfig(
    size=64,
    offload_threshold_ms=1,
    adaptive=True,
))


@app.get("/fast")
def fast(request):
    """Cheap handler: stays inline, no threadpool overhead."""
    return {"thread": threading.get_ident(), "mode": "inline"}


@app.get("/blocking")
def blocking(request):
    """Blocking handler: auto-detected after two slow calls, then pooled.

    ``time.sleep`` releases the GIL, so pooled calls genuinely overlap. The same
    is true of synchronous database drivers and HTTP clients.
    """
    time.sleep(0.010)
    return {"thread": threading.get_ident(), "mode": "auto-offloaded"}


@app.get("/marked", blocking=True)
def marked(request):
    """Known to block — pooled from the very first request, no learning phase."""
    time.sleep(0.010)
    return {"thread": threading.get_ident(), "mode": "blocking=True"}


@app.get("/pinned", blocking=False)
def pinned(request):
    """Opted out of offloading. Shows the cost: this route serialises."""
    time.sleep(0.010)
    return {"thread": threading.get_ident(), "mode": "blocking=False"}


@app.get("/cpu")
def cpu(request):
    """CPU-bound Python does NOT benefit — it never releases the GIL.

    It will still be detected as slow and offloaded, but the GIL serialises it
    anyway. Use multiple workers (``app.run(workers=N)``) for CPU-bound work.
    """
    total = sum(i * i for i in range(200_000))
    return {"thread": threading.get_ident(), "total": total}


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
