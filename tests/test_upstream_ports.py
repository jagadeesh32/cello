"""
Ports taken from upstream cello v1.4.0 ("The Native Core") and its follow-up
release commit, because each one fixes real behaviour in this fork.

* **Production-safe 500s** — an *unhandled* handler error used to be described to
  the client in every environment. The message of such an error is the Python
  exception text, and can be a database error carrying constraint names and row
  values, so outside debug mode the client now gets a generic Problem Details
  body while the detail is written to the server log. Handlers registered with
  ``@app.exception_handler(...)`` still receive the untouched error, so redaction
  can never silence a handler the user wrote.
* **Ctrl-C ran no shutdown hooks** — SIGINT is delivered to CPython's own handler
  as well as to Tokio's, which leaves a ``KeyboardInterrupt`` pending on the
  interpreter. The first Python call after ``server.run()`` raised it, so an
  ``async def`` shutdown hook's coroutine was dropped un-awaited and the hook
  never ran.
* **Postgres prepared statements** — every query used to be parsed, described and
  bound on each call (two round trips); repeated SQL is now prepared once per
  pooled connection. Needs a live server, so that test skips without one.

The first two are process-wide state (the debug flag, the interpreter's pending
signal), so each scenario runs a real server script in a subprocess.
"""

import os
import signal
import subprocess
import sys
import textwrap
import time

import pytest

requests = pytest.importorskip("requests")

from cello import App

_PORT = [18700]

#: The text a handler leaks on purpose; it stands in for a database message with
#: a constraint name and a row value.
SECRET = "secret-constraint users_pkey bob@example.com"


def _next_port():
    _PORT[0] += 1
    return _PORT[0]


def _wait_up(base, proc, deadline=20):
    """Wait until `proc` answers on `base`, or fail with its output."""
    last = None
    end = time.time() + deadline
    while time.time() < end:
        if proc.poll() is not None:
            out, err = proc.communicate()
            raise AssertionError(
                f"server exited early (rc={proc.returncode})\nstdout:\n{out}\nstderr:\n{err}"
            )
        try:
            requests.get(base + "/__ping__", timeout=0.5)
            return
        except Exception as e:  # not up yet
            last = e
            time.sleep(0.1)
    proc.kill()
    raise AssertionError(f"server never came up: {last}")


def _spawn_server(tmp_path, body, **subs):
    """Write `body` as a server script, start it, return (proc, base)."""
    script = tmp_path / "server_app.py"
    script.write_text(textwrap.dedent(body).format(**subs))
    proc = subprocess.Popen(
        [sys.executable, str(script)],
        cwd=str(tmp_path),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env={**os.environ, "PYTHONUNBUFFERED": "1"},
    )
    return proc, f"http://127.0.0.1:{subs['port']}"


def _interrupt(proc, timeout=20):
    """SIGINT `proc`, then return its (stdout, stderr) once it exits."""
    if proc.poll() is None:
        proc.send_signal(signal.SIGINT)
    try:
        return proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        out, err = proc.communicate()
        raise AssertionError(f"server ignored SIGINT\nstdout:\n{out}\nstderr:\n{err}")


# ── Production-safe 500s ─────────────────────────────────────────────────────

# `workers=1` keeps the child single-process; with `debug=False` the default is
# one worker per CPU, and the debug flag would then be set in another process.
_BOOM_APP = """
    from cello import App

    PORT = {port}
    DEBUG = {debug}
    SECRET = {secret!r}

    app = App()

    @app.get("/__ping__")
    def ping(request):
        return {{"ok": True}}

    @app.get("/boom")
    def boom(request):
        raise ValueError(SECRET)

    app.run(host="127.0.0.1", port=PORT, debug=DEBUG, env="production", workers=1)
"""


def test_unhandled_error_is_redacted_in_production(tmp_path):
    port = _next_port()
    proc, base = _spawn_server(
        tmp_path, _BOOM_APP, port=port, debug=False, secret=SECRET
    )
    _wait_up(base, proc)
    r = requests.get(base + "/boom", timeout=5)
    _, err = _interrupt(proc)

    assert r.status_code == 500
    # The internals never reach the client ...
    assert "secret-constraint" not in r.text
    assert "users_pkey" not in r.text
    assert "bob@example.com" not in r.text
    # ... the body is still a Problem Details document, just without the detail
    doc = r.json()
    assert doc["status"] == 500
    assert doc["title"] == "Internal Server Error"
    assert "detail" not in doc
    assert "exception_type" not in doc
    # ... and the detail went to the server log instead.
    assert "secret-constraint" in err


def test_unhandled_error_is_described_in_debug(tmp_path):
    port = _next_port()
    proc, base = _spawn_server(
        tmp_path, _BOOM_APP, port=port, debug=True, secret=SECRET
    )
    _wait_up(base, proc)
    r = requests.get(base + "/boom", timeout=5)
    _interrupt(proc)

    assert r.status_code == 500
    # In development the detail is the point of the response.
    assert "secret-constraint" in r.text
    assert r.json()["detail"].startswith("ValueError")


def _serve(app, port):
    import threading

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


def test_exception_handler_still_sees_the_error_when_redaction_is_on():
    """Redaction only shapes the *default* 500 — a registered handler still wins."""
    app = App()
    app._app.set_expose_errors(False)
    try:

        @app.exception_handler(ValueError)
        def on_value_error(request, exc):
            return {"handled": str(exc)}

        @app.get("/__ping__")
        def ping(request):
            return {"ok": True}

        @app.get("/boom")
        def boom(request):
            raise ValueError("visible to the handler")

        base = _serve(app, _next_port())
        r = requests.get(base + "/boom", timeout=5)
    finally:
        app._app.set_expose_errors(True)

    assert r.status_code == 200
    assert r.json() == {"handled": "visible to the handler"}


# ── Ctrl-C / shutdown hooks ──────────────────────────────────────────────────

_SHUTDOWN_APP = """
    from cello import App

    PORT = {port}
    LOG = {log!r}

    app = App()

    @app.get("/__ping__")
    def ping(request):
        return {{"ok": True}}

    @app.on_event("shutdown")
    async def bye():
        with open(LOG, "a") as f:
            f.write("shutdown\\n")

    app.run(host="127.0.0.1", port=PORT)
"""


def test_sigint_runs_async_shutdown_hooks(tmp_path):
    port = _next_port()
    log = tmp_path / "shutdown.log"
    proc, base = _spawn_server(tmp_path, _SHUTDOWN_APP, port=port, log=str(log))
    _wait_up(base, proc)
    assert requests.get(base + "/__ping__", timeout=5).status_code == 200

    _interrupt(proc)

    assert log.exists(), "the shutdown hook never ran"
    assert "shutdown" in log.read_text()


# ── Postgres prepared-statement cache ────────────────────────────────────────

DSN = os.environ.get(
    "CELLO_TEST_DATABASE_URL",
    "postgresql://postgres:postgres@127.0.0.1:5432/cello_test",
)


def _pg_available():
    import asyncio

    from cello import DatabaseConfig

    async def check():
        app = App()
        app.enable_database(DatabaseConfig(url=DSN, pool_size=1))
        await app.database.ping()
        await app.database.close()

    try:
        asyncio.new_event_loop().run_until_complete(check())
        return True
    except Exception:
        return False


def _run(coro):
    import asyncio

    return asyncio.new_event_loop().run_until_complete(coro)


@pytest.mark.skipif(not _pg_available(), reason="PostgreSQL not reachable")
def test_prepared_statement_cache_returns_the_same_rows():
    """A query repeated on the pooled connection reuses its prepared statement."""
    from cello import DatabaseConfig

    async def main():
        app = App()
        app.enable_database(DatabaseConfig(url=DSN, pool_size=1))
        db = app.database
        try:
            await db.execute("DROP TABLE IF EXISTS t_stmt_cache")
            await db.execute("CREATE TABLE t_stmt_cache (id int, name text)")
            await db.execute(
                "INSERT INTO t_stmt_cache (id, name) VALUES ($1, $2)", 1, "alice"
            )
            await db.execute(
                "INSERT INTO t_stmt_cache (id, name) VALUES ($1, $2)", 2, "bob"
            )

            # First call prepares (Parse/Describe/Bind); the rest reuse the
            # cached statement. All of them must return the same rows.
            first = await db.fetch("SELECT id, name FROM t_stmt_cache ORDER BY id")
            again = await db.fetch("SELECT id, name FROM t_stmt_cache ORDER BY id")
            assert first == again == [{"id": 1, "name": "alice"}, {"id": 2, "name": "bob"}]

            row = await db.fetchrow("SELECT name FROM t_stmt_cache WHERE id = $1", 2)
            assert row == {"name": "bob"}
            assert await db.fetchval("SELECT count(*) FROM t_stmt_cache") == 2

            async with db.transaction() as tx:
                await tx.execute(
                    "INSERT INTO t_stmt_cache (id, name) VALUES ($1, $2)", 3, "carol"
                )
                names = await tx.fetch("SELECT name FROM t_stmt_cache ORDER BY id")
                assert [r["name"] for r in names] == ["alice", "bob", "carol"]
        finally:
            await db.execute("DROP TABLE IF EXISTS t_stmt_cache")
            await db.close()

    _run(main())
