"""Unhandled handler errors must not leak details to clients in production.

Error text can carry database messages, constraint names and row values (for
example a unique violation's ``DETAIL: Key (email)=(a@b.c) already exists``).
In development the 500 body describes the error; with ``debug`` off (including
``env="production"``) the client gets a generic 500 and the details go to the
server log.
"""

import os
import socket
import subprocess
import sys
import textwrap
import time

import pytest

requests = pytest.importorskip("requests")

_APP = textwrap.dedent(
    """
    import sys
    from cello import App

    port, mode = int(sys.argv[1]), sys.argv[2]
    app = App()

    @app.get("/boom")
    def boom(request):
        raise RuntimeError("secret-detail Key (email)=(a@b.c) already exists")

    @app.get("/aboom")
    async def aboom(request):
        raise RuntimeError("secret-detail Key (email)=(a@b.c) already exists")

    @app.get("/ok")
    def ok(request):
        return {"ok": True}

    if mode == "production":
        app.run(host="127.0.0.1", port=port, env="production", workers=1)
    else:
        app.run(host="127.0.0.1", port=port, workers=1)
    """
)


def _free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _serve(tmp_path, mode):
    script = tmp_path / "app.py"
    script.write_text(_APP)
    port = _free_port()
    # A piped stdout defaults to the locale codec on Windows (cp1252), which
    # cannot encode the startup banner; force UTF-8 so the child can start.
    env = {**os.environ, "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8"}
    proc = subprocess.Popen(
        [sys.executable, str(script), str(port), mode],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
    )
    base = f"http://127.0.0.1:{port}"
    deadline = time.time() + 30
    while time.time() < deadline:
        if proc.poll() is not None:
            out, _ = proc.communicate()
            raise RuntimeError(f"server exited with {proc.returncode}:\n{out}")
        try:
            if requests.get(base + "/ok", timeout=0.5).ok:
                return proc, base
        except Exception:
            time.sleep(0.1)
    proc.kill()
    out, _ = proc.communicate()
    raise RuntimeError(f"server never came up:\n{out}")


def _stop(proc):
    proc.terminate()
    try:
        out, _ = proc.communicate(timeout=15)
    except subprocess.TimeoutExpired:
        proc.kill()
        out, _ = proc.communicate()
    return out


@pytest.mark.parametrize("path", ["/boom", "/aboom"])
def test_production_500_is_generic_and_logged(tmp_path, path):
    proc, base = _serve(tmp_path, "production")
    try:
        r = requests.get(base + path, timeout=10)
    finally:
        log = _stop(proc)

    assert r.status_code == 500
    assert "secret-detail" not in r.text
    assert "RuntimeError" not in r.text
    assert r.json() == {"error": "Internal Server Error", "status": 500}
    # The operator still sees what happened.
    assert "secret-detail" in log
    assert path in log


@pytest.mark.parametrize("path", ["/boom", "/aboom"])
def test_development_500_describes_the_error(tmp_path, path):
    proc, base = _serve(tmp_path, "development")
    try:
        r = requests.get(base + path, timeout=10)
    finally:
        _stop(proc)

    assert r.status_code == 500
    assert "secret-detail" in r.text
