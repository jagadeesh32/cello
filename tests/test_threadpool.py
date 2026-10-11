"""Blocking-handler threadpool offload.

Sync ``def`` handlers run inline on the single-threaded server runtime, which is
the fastest path for a handler that just builds a value. A handler that *blocks*
would hold that thread for its whole duration and stall every other connection —
throughput collapses to one-request-at-a-time. Cello times sync handlers and
permanently moves any that exceed the offload threshold onto a bounded blocking
threadpool, so blocking calls overlap instead of serialising.

Inline vs pooled is observed directly via ``threading.get_ident()``: an inline
handler always runs on the one server thread, a pooled handler is spread across
pool threads.
"""

import threading
import time
from concurrent.futures import ThreadPoolExecutor

import pytest

requests = pytest.importorskip("requests")

from cello import App, Depends, ThreadPoolConfig

_PORT = [17600]

# A handler slow enough to be unambiguously "blocking" (threshold defaults to 1ms).
SLEEP = 0.010
CONCURRENCY = 20

# Tests that compare elapsed time against the serialised time use a longer sleep
# so the gap (seconds) dwarfs the HTTP client's own per-request overhead, which
# dominates a 10 ms sleep on slow shared CI runners.
TIMING_SLEEP = 0.050


def _next_port():
    _PORT[0] += 1
    return _PORT[0]


def _serve(app):
    """Start ``app`` on a daemon thread and wait until it answers."""
    port = _next_port()
    threading.Thread(
        target=lambda: app.run(host="127.0.0.1", port=port), daemon=True
    ).start()
    base = f"http://127.0.0.1:{port}"
    deadline = time.time() + 10
    last = None
    while time.time() < deadline:
        try:
            requests.get(base + "/__ping__", timeout=0.5)
            return base
        except Exception as e:  # not up yet
            last = e
            time.sleep(0.1)
    raise RuntimeError(f"server on :{port} never came up: {last}")


def _app():
    app = App()

    @app.get("/__ping__")
    def ping(request):
        return {"ok": True}

    return app


def _hammer(base, path, n, concurrency=CONCURRENCY):
    """Fire ``n`` requests at ``concurrency``; return (elapsed, [thread ids])."""
    session = requests.Session()
    adapter = requests.adapters.HTTPAdapter(
        pool_connections=concurrency, pool_maxsize=concurrency
    )
    session.mount("http://", adapter)
    url = base + path

    def one(_):
        return session.get(url, timeout=30).json().get("tid")

    start = time.perf_counter()
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        tids = list(pool.map(one, range(n)))
    return time.perf_counter() - start, tids


# ── Adaptive promotion ──────────────────────────────────────────────────────

def test_blocking_handler_is_offloaded_and_runs_concurrently():
    """A blocking sync handler must not serialise the server."""
    app = _app()

    @app.get("/slow")
    def slow(request):
        time.sleep(TIMING_SLEEP)
        return {"tid": threading.get_ident()}

    base = _serve(app)

    # First call is inline and pays the cost; it promotes the handler.
    requests.get(base + "/slow", timeout=30)

    n = 60
    elapsed, tids = _hammer(base, "/slow", n)

    # Serialised, 60 x 50ms would take 3 s; concurrently it is a fraction of that.
    serial = n * TIMING_SLEEP
    assert elapsed < serial / 2, (
        f"{n} blocking requests took {elapsed:.2f}s; "
        f"serialised would be ~{serial:.2f}s — handler was not offloaded"
    )
    # Spread across the pool, not pinned to the server thread.
    assert len(set(tids)) > 1


def test_fast_handler_stays_inline():
    """Trivial handlers keep the zero-hop path — no threadpool tax."""
    app = _app()

    @app.get("/fast")
    def fast(request):
        return {"tid": threading.get_ident()}

    base = _serve(app)
    _, tids = _hammer(base, "/fast", 200)

    assert len(set(tids)) == 1, (
        f"fast handler was offloaded to {len(set(tids))} threads; "
        "it should stay inline on the server thread"
    )


# ── Explicit blocking= override ─────────────────────────────────────────────

def test_blocking_true_offloads_from_first_request():
    """``blocking=True`` skips the learning request entirely."""
    app = _app()

    @app.get("/marked")
    def marked(request):
        time.sleep(TIMING_SLEEP)
        return {"tid": threading.get_ident()}

    base = _serve(app)

    # No warmup: concurrency must be present immediately.
    n = 40
    elapsed, tids = _hammer(base, "/marked", n)
    assert elapsed < (n * TIMING_SLEEP) / 2, (
        f"{n} blocking=True requests took {elapsed:.2f}s; "
        f"serialised would be ~{n * TIMING_SLEEP:.2f}s"
    )
    assert len(set(tids)) > 1


def test_blocking_false_pins_handler_inline():
    """``blocking=False`` opts out of adaptive promotion."""
    app = _app()

    @app.get("/pinned", blocking=False)
    def pinned(request):
        time.sleep(SLEEP)
        return {"tid": threading.get_ident()}

    base = _serve(app)
    requests.get(base + "/pinned", timeout=30)

    n = 20
    elapsed, tids = _hammer(base, "/pinned", n)

    assert len(set(tids)) == 1, "blocking=False handler was offloaded anyway"
    # Pinned inline means it really does serialise.
    assert elapsed >= n * SLEEP * 0.8


def test_blocking_true_on_fast_handler_still_correct():
    """Marking a cheap handler blocking is legal, just slower."""
    app = _app()

    @app.get("/cheap", blocking=True)
    def cheap(request):
        return {"tid": threading.get_ident(), "value": 42}

    base = _serve(app)
    body = requests.get(base + "/cheap", timeout=10).json()
    assert body["value"] == 42


# ── The refactored call path must not break anything ────────────────────────

def test_async_handler_still_works():
    """Guards the extraction of the shared call path."""
    app = _app()

    @app.get("/async")
    async def async_handler(request):
        import asyncio

        await asyncio.sleep(0.001)
        return {"async": True}

    base = _serve(app)
    assert requests.get(base + "/async", timeout=10).json() == {"async": True}


def test_dependency_injection_survives_offload():
    """DI resolution must behave identically on the pooled path."""
    app = _app()

    app.register_singleton("service", {"name": "svc"})

    @app.get("/di", blocking=True)
    def with_di(request, service=Depends("service")):
        time.sleep(SLEEP)
        return {"service": service["name"]}

    base = _serve(app)
    assert requests.get(base + "/di", timeout=10).json() == {"service": "svc"}


def test_path_params_and_post_body_survive_offload():
    """The Request is moved across a thread boundary — it must arrive intact."""
    app = _app()

    @app.post("/echo/{name}", blocking=True)
    def echo(request):
        time.sleep(SLEEP)
        return {"name": request.params["name"], "body": request.json()}

    base = _serve(app)
    resp = requests.post(base + "/echo/ada", json={"k": "v"}, timeout=10)
    assert resp.json() == {"name": "ada", "body": {"k": "v"}}


# ── Configuration ───────────────────────────────────────────────────────────

def test_threadpool_config_accepted():
    cfg = ThreadPoolConfig(size=8, offload_threshold_ms=5, adaptive=True)
    assert (cfg.size, cfg.offload_threshold_ms, cfg.adaptive) == (8, 5, True)
    assert ThreadPoolConfig().size == 64  # documented default

    app = _app()
    app.set_threadpool(cfg)

    @app.get("/ok")
    def ok(request):
        return {"ok": True}

    base = _serve(app)
    assert requests.get(base + "/ok", timeout=10).json() == {"ok": True}


def test_adaptive_false_disables_promotion():
    """With adaptive off, only explicitly marked handlers are pooled."""
    app = _app()
    app.set_threadpool(ThreadPoolConfig(adaptive=False))

    @app.get("/slow")
    def slow(request):
        time.sleep(SLEEP)
        return {"tid": threading.get_ident()}

    base = _serve(app)
    requests.get(base + "/slow", timeout=30)
    _, tids = _hammer(base, "/slow", 20)

    assert len(set(tids)) == 1, "handler was promoted despite adaptive=False"
