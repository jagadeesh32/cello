"""Shutdown hooks must run however the server is stopped.

Ctrl-C (SIGINT) is delivered to CPython's own signal handler as well as the
server's, which used to leave a pending ``KeyboardInterrupt`` that was raised by
the first Python call made while running a shutdown hook. An ``async def`` hook's
coroutine was then dropped un-awaited ("coroutine ... was never awaited") and the
hook silently never ran. SIGTERM was unaffected.
"""

import os
import signal
import socket
import subprocess
import sys
import textwrap
import time

import pytest

requests = pytest.importorskip("requests")

pytestmark = pytest.mark.skipif(
    sys.platform == "win32", reason="POSIX signal semantics"
)

_APP = textwrap.dedent(
    """
    import sys
    from cello import App

    port, log = int(sys.argv[1]), sys.argv[2]
    app = App()

    def note(text):
        with open(log, "a") as fh:
            fh.write(text + "\\n")

    @app.on_event("startup")
    async def started():
        note("startup")

    @app.on_event("shutdown")
    async def stopped_async():
        note("shutdown-async")

    @app.on_event("shutdown")
    def stopped_sync():
        note("shutdown-sync")

    @app.get("/")
    def index(request):
        return {"ok": True}

    app.run(host="127.0.0.1", port=port)
    """
)


def _free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _run_and_stop(tmp_path, sig):
    script = tmp_path / "app.py"
    script.write_text(_APP)
    log = tmp_path / "events.log"
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, str(script), str(port), str(log)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    try:
        deadline = time.time() + 15
        while time.time() < deadline:
            try:
                if requests.get(f"http://127.0.0.1:{port}/", timeout=0.5).ok:
                    break
            except Exception:
                time.sleep(0.1)
        else:
            raise RuntimeError("server never came up")
        proc.send_signal(sig)
        output, _ = proc.communicate(timeout=20)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.communicate()
    events = log.read_text().split() if log.exists() else []
    return events, output


@pytest.mark.parametrize("sig", [signal.SIGINT, signal.SIGTERM], ids=["SIGINT", "SIGTERM"])
def test_shutdown_hooks_run_on_signal(tmp_path, sig):
    events, output = _run_and_stop(tmp_path, sig)

    assert events[0] == "startup"
    assert "shutdown-async" in events, f"async shutdown hook never ran: {events}\n{output}"
    assert "shutdown-sync" in events, f"sync shutdown hook never ran: {events}\n{output}"
    assert "never awaited" not in output, output
    assert "Traceback" not in output, output
