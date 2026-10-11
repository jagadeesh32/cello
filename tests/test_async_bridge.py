"""The Rust-future → asyncio bridge (``src/async_bridge.rs``) and the async
handler path (``src/async_loop.rs`` ``submit_coroutine``).

Covers result/exception mapping, cancellation, concurrency, use from several
event loops, and exit safety. Database/Redis tests are skipped when the servers
are unreachable (same env vars as ``test_native_db.py``).
"""

import asyncio
import os
import subprocess
import sys
import textwrap
import threading
import time

import pytest

from cello import App, DatabaseConfig, RedisConfig

DSN = os.environ.get(
    "CELLO_TEST_DATABASE_URL",
    "postgresql://postgres:postgres@127.0.0.1:5432/cello_test",
)
REDIS_URL = os.environ.get("CELLO_TEST_REDIS_URL", "redis://127.0.0.1:6379")


def _run(coro):
    return asyncio.new_event_loop().run_until_complete(coro)


def _pg_available():
    async def check():
        app = App()
        app.enable_database(DatabaseConfig(url=DSN, pool_size=1))
        await app.database.ping()

    try:
        _run(check())
        return True
    except Exception:
        return False


def _redis_available():
    async def check():
        app = App()
        app.enable_redis(RedisConfig(url=REDIS_URL))
        await app.redis.ping()

    try:
        _run(check())
        return True
    except Exception:
        return False


PG = _pg_available()
REDIS = _redis_available()
needs_pg = pytest.mark.skipif(not PG, reason="Postgres not reachable")
needs_redis = pytest.mark.skipif(not REDIS, reason="Redis not reachable")


def _db(pool_size=2):
    app = App()
    app.enable_database(DatabaseConfig(url=DSN, pool_size=pool_size))
    return app.database


# ── Bridge semantics ─────────────────────────────────────────────────────────


@needs_pg
def test_results_and_types_resolve():
    db = _db()

    async def go():
        assert await db.fetchval("SELECT $1::int", 7) == 7
        assert await db.fetchval("SELECT NULL::int") is None
        assert await db.fetchrow("SELECT 1 AS a, 'x' AS b") == {"a": 1, "b": "x"}
        assert await db.fetch("SELECT g FROM generate_series(1, 3) g") == [
            {"g": 1},
            {"g": 2},
            {"g": 3},
        ]
        assert await db.execute("SELECT 1") == 1

    _run(go())


@needs_pg
def test_errors_raise_from_await():
    db = _db()

    async def go():
        with pytest.raises(RuntimeError, match="db error"):
            await db.fetchval("SELECT * FROM no_such_table_bridge")
        # The pool is still healthy afterwards.
        assert await db.fetchval("SELECT 2") == 2

    _run(go())


def test_requires_running_loop():
    app = App()
    app.enable_database(DatabaseConfig(url="postgresql://u:p@127.0.0.1:1/x", pool_size=1))
    with pytest.raises(RuntimeError):
        app.database.fetchval("SELECT 1")


@needs_pg
def test_cancellation_aborts_and_pool_recovers():
    db = _db(pool_size=1)

    async def go():
        with pytest.raises(asyncio.TimeoutError):
            await asyncio.wait_for(db.fetchval("SELECT pg_sleep(0.3)"), 0.05)
        # A cancelled future is never resolved behind our back.
        fut = db.fetchval("SELECT 1")
        fut.cancel()
        with pytest.raises(asyncio.CancelledError):
            await fut
        await asyncio.sleep(0.4)
        assert await db.fetchval("SELECT $1::int", 5) == 5

    _run(go())


@needs_pg
def test_concurrent_awaits_resolve_to_their_own_results():
    db = _db(pool_size=4)

    async def go():
        vals = await asyncio.gather(*(db.fetchval("SELECT $1::int", i) for i in range(200)))
        assert vals == list(range(200))

    _run(go())


@needs_pg
def test_same_pool_from_several_loops_and_threads():
    db = _db(pool_size=2)

    async def go(base):
        return [await db.fetchval("SELECT $1::int", base + i) for i in range(20)]

    # Sequential loops on this thread (each gets its own completion pipe).
    for base in (0, 100, 200):
        assert _run(go(base)) == list(range(base, base + 20))

    # Loops on other threads at the same time.
    out = {}

    def worker(base):
        out[base] = asyncio.run(go(base))

    threads = [threading.Thread(target=worker, args=(b,)) for b in (1000, 2000, 3000)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(30)
    for base in (1000, 2000, 3000):
        assert out[base] == list(range(base, base + 20))


@needs_pg
def test_many_short_lived_loops_do_not_leak_descriptors():
    db = _db(pool_size=1)

    def fd_count():
        return len(os.listdir("/proc/self/fd"))

    if not os.path.isdir("/proc/self/fd"):
        pytest.skip("needs /proc")

    async def one():
        return await db.fetchval("SELECT 1")

    for _ in range(5):
        asyncio.run(one())
    import gc

    gc.collect()
    before = fd_count()
    for _ in range(50):
        asyncio.run(one())
    gc.collect()
    assert fd_count() - before < 10


@needs_redis
def test_redis_round_trip():
    app = App()
    app.enable_redis(RedisConfig(url=REDIS_URL))
    r = app.redis

    async def go():
        await r.set("cello:bridge:k", "v")
        assert await r.get("cello:bridge:k") == "v"
        assert await r.delete("cello:bridge:k") == 1
        assert await r.get("cello:bridge:k") is None

    _run(go())


# ── Exit safety ──────────────────────────────────────────────────────────────

_EXIT_SCRIPT = textwrap.dedent(
    """
    import asyncio, sys
    sys.setswitchinterval(1e-6)  # maximise thread switches near shutdown
    from cello import App, DatabaseConfig

    async def main():
        app = App()
        app.enable_database(DatabaseConfig(url=%r, pool_size=4))
        db = app.database
        for i in range(20):
            await db.fetchval("SELECT $1::int", i)
        await asyncio.gather(*(db.fetchval("SELECT pg_sleep(0.01)") for _ in range(4)))

    asyncio.new_event_loop().run_until_complete(main())
    """
)


@needs_pg
@pytest.mark.skipif(not sys.platform.startswith("linux"), reason="Linux /proc check")
def test_no_native_thread_enters_python_after_use():
    # Native I/O threads must never take the GIL: CPython terminates a thread
    # that asks for it during finalization, and that forced unwind through
    # Rust frames aborts the process ("panic in a function that cannot
    # unwind"). The old bridge ran completions on `tokio-rt-worker` threads.
    # Checked in a fresh process: servers started by other tests own
    # blocking-pool threads with that default name.
    script = textwrap.dedent(
        """
        import asyncio, os
        from cello import App, DatabaseConfig
        app = App()
        app.enable_database(DatabaseConfig(url=%r, pool_size=1))
        async def main():
            return await app.database.fetchval("SELECT 1")
        assert asyncio.run(main()) == 1
        names = set()
        for tid in os.listdir("/proc/self/task"):
            try:
                with open(f"/proc/self/task/{tid}/comm") as f:
                    names.add(f.read().strip())
            except OSError:
                pass
        print(sorted(names))
        assert "tokio-rt-worker" not in names, names
        assert "cello-io" in names, names
        """
    )
    proc = subprocess.run(
        [sys.executable, "-c", script % DSN], capture_output=True, text=True, timeout=60
    )
    assert proc.returncode == 0, proc.stdout + proc.stderr[-2000:]


@needs_pg
def test_scripts_exit_cleanly_after_db_use():
    for _ in range(5):
        proc = subprocess.run(
            [sys.executable, "-c", _EXIT_SCRIPT % DSN],
            capture_output=True,
            text=True,
            timeout=60,
        )
        assert proc.returncode == 0, proc.stderr[-2000:]
        assert "cannot unwind" not in proc.stderr


# ── Async handlers over HTTP ─────────────────────────────────────────────────

requests = pytest.importorskip("requests")
_PORT = [17950]


def _serve(app):
    _PORT[0] += 1
    port = _PORT[0]
    threading.Thread(target=lambda: app.run(host="127.0.0.1", port=port), daemon=True).start()
    base = f"http://127.0.0.1:{port}"
    deadline = time.time() + 10
    while time.time() < deadline:
        try:
            requests.get(base + "/__ping__", timeout=0.5)
            return base
        except Exception:
            time.sleep(0.1)
    raise RuntimeError("server never came up")


def test_async_handler_results_errors_and_loop_state():
    app = App()

    @app.get("/__ping__")
    def ping(request):
        return {"ok": True}

    state = {"lock": None}

    @app.get("/value")
    async def value(request):
        await asyncio.sleep(0)
        return {"v": 1, "loop": id(asyncio.get_running_loop())}

    @app.get("/boom")
    async def boom(request):
        await asyncio.sleep(0)
        raise ValueError("kaboom")

    @app.get("/lock")
    async def lock(request):
        # Loop-bound objects survive across requests (persistent loop).
        if state["lock"] is None:
            state["lock"] = asyncio.Lock()
        async with state["lock"]:
            return {"ok": True}

    base = _serve(app)
    a = requests.get(base + "/value", timeout=5).json()
    b = requests.get(base + "/value", timeout=5).json()
    assert a["v"] == 1 and a["loop"] == b["loop"]
    assert requests.get(base + "/boom", timeout=5).status_code == 500
    for _ in range(3):
        assert requests.get(base + "/lock", timeout=5).json() == {"ok": True}


@needs_pg
def test_async_handler_awaiting_database():
    app = App()
    app.enable_database(DatabaseConfig(url=DSN, pool_size=2))

    @app.get("/__ping__")
    def ping(request):
        return {"ok": True}

    @app.get("/q/{n}")
    async def q(request):
        n = int(request.params["n"])
        return {"n": await request.database.fetchval("SELECT $1::int", n)}

    base = _serve(app)
    for n in (1, 2, 3):
        assert requests.get(f"{base}/q/{n}", timeout=5).json() == {"n": n}
