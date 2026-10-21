"""Live-HTTP regression tests for the 1.4.0 architecture/security review
(CODE_REVIEW.md). Each test names the finding it pins.
"""

import base64
import gzip
import os
import socket
import struct
import subprocess
import sys
import textwrap
import threading
import time

import pytest

requests = pytest.importorskip("requests")
pyjwt = pytest.importorskip("jwt")

from cello import (
    App,
    GraphQLConfig,
    HealthCheckConfig,
    JwtConfig,
    LimitsConfig,
    RateLimitConfig,
    Response,
    SessionConfig,
    TimeoutConfig,
)

_PORT = [17900]
SECRET = "r" * 32


def _next_port():
    _PORT[0] += 1
    return _PORT[0]


def _serve(app, ready_path="/__ping__"):
    port = _next_port()
    threading.Thread(target=lambda: app.run(host="127.0.0.1", port=port), daemon=True).start()
    base = f"http://127.0.0.1:{port}"
    deadline = time.time() + 10
    while time.time() < deadline:
        try:
            requests.get(base + ready_path, timeout=0.5)
            return base
        except Exception:
            time.sleep(0.05)
    raise RuntimeError("server never came up")


def _app():
    app = App()

    @app.get("/__ping__")
    def ping(request):
        return {"ok": True}

    return app


def _token(**claims):
    claims.setdefault("sub", "u1")
    claims.setdefault("exp", int(time.time()) + 3600)
    return pyjwt.encode(claims, SECRET, algorithm="HS256")


# ── S1: unrouted endpoints run the auth chain ───────────────────────────────

def test_unrouted_endpoints_require_auth():
    app = _app()
    app.enable_jwt(JwtConfig(secret=SECRET), skip_paths=["/__ping__", "/health/live"])
    app.enable_health_checks(HealthCheckConfig())
    app.enable_graphql(GraphQLConfig())
    app.enable_prometheus()
    url = _serve(app)

    for path in ("/health", "/health/ready", "/metrics", "/graphql"):
        r = requests.get(url + path)
        assert r.status_code == 401, (path, r.status_code)
    # An explicit skip path opens a probe endpoint deliberately.
    assert requests.get(url + "/health/live").status_code == 200

    auth = {"Authorization": f"Bearer {_token()}"}
    assert requests.get(url + "/health", headers=auth).status_code == 200
    assert requests.get(url + "/metrics", headers=auth).status_code == 200
    q = requests.post(url + "/graphql", json={"query": "{ __typename }"}, headers=auth)
    assert 200 <= q.status_code < 300
    # A genuine miss still 404s before auth (no 404->401 route-existence leak).
    assert requests.get(url + "/nope").status_code == 404


def test_graphql_playground_and_introspection_off_by_default():
    app = _app()
    app.enable_graphql(GraphQLConfig())
    url = _serve(app)
    r = requests.get(url + "/graphql", headers={"Accept": "text/html"})
    assert "<html" not in r.text.lower()


# ── S2: rate limiting / client IP use the socket peer ───────────────────────

def test_forwarded_for_cannot_dodge_rate_limit():
    app = _app()
    app.enable_rate_limit(RateLimitConfig(algorithm="sliding_window", capacity=3, window_secs=60))

    @app.get("/limited")
    def limited(request):
        return {"ip": request.client_ip()}

    url = _serve(app)
    codes = [
        requests.get(url + "/limited", headers={"X-Forwarded-For": f"10.0.0.{i}"}).status_code
        for i in range(6)
    ]
    assert codes.count(429) >= 2, codes


def test_client_ip_ignores_untrusted_forwarded_headers():
    app = _app()

    @app.get("/ip")
    def ip(request):
        return {"ip": request.client_ip(), "secure": request.is_secure(), "host": request.host()}

    url = _serve(app)
    body = requests.get(
        url + "/ip",
        headers={"X-Forwarded-For": "6.6.6.6", "X-Forwarded-Proto": "https", "X-Forwarded-Host": "evil"},
    ).json()
    assert body["ip"] == "127.0.0.1"
    assert body["secure"] is False
    assert body["host"].startswith("127.0.0.1")


def test_trusted_proxy_forwarded_for_is_honoured():
    app = _app()
    app.set_trusted_proxies(["127.0.0.1"])

    @app.get("/ip")
    def ip(request):
        return {"ip": request.client_ip(), "secure": request.is_secure()}

    try:
        url = _serve(app)
        body = requests.get(
            url + "/ip",
            headers={"X-Forwarded-For": "1.1.1.1, 9.9.9.9", "X-Forwarded-Proto": "https"},
        ).json()
        # Right-most untrusted hop; the left-most is client-supplied.
        assert body["ip"] == "9.9.9.9"
        assert body["secure"] is True
    finally:
        app.set_trusted_proxies([])


def test_rate_limit_key_by_header():
    app = _app()
    app.enable_rate_limit(
        RateLimitConfig(algorithm="sliding_window", capacity=2, window_secs=60, key_by="header:X-API-Key")
    )

    @app.get("/k")
    def k(request):
        return {}

    url = _serve(app)
    a = [requests.get(url + "/k", headers={"X-API-Key": "a"}).status_code for _ in range(3)]
    b = requests.get(url + "/k", headers={"X-API-Key": "b"}).status_code
    assert a == [200, 200, 429]
    assert b == 200


def test_rate_limit_rejects_unknown_key_by():
    app = _app()
    with pytest.raises(ValueError):
        app.enable_rate_limit(RateLimitConfig(key_by="bogus"))


# ── S3: response cache ──────────────────────────────────────────────────────

def test_cache_never_shares_authenticated_responses():
    app = _app()
    app.enable_caching(ttl=60)
    app.enable_security_headers()
    hits = {"n": 0}

    @app.get("/me")
    def me(request):
        hits["n"] += 1
        return {"auth": request.get_header("Authorization")}

    @app.get("/public")
    def public(request):
        hits["n"] += 1
        return {"public": True}

    url = _serve(app)
    alice = requests.get(url + "/me", headers={"Authorization": "alice"}).json()
    bob = requests.get(url + "/me", headers={"Authorization": "bob"}).json()
    assert alice["auth"] == "alice" and bob["auth"] == "bob"

    first = requests.get(url + "/public")
    second = requests.get(url + "/public")
    assert second.headers.get("x-cache") == "HIT"
    # No server-TTL leak into browser/shared caches.
    assert "cache-control" not in {k.lower() for k in second.headers}
    # A HIT still goes through the after hooks.
    assert second.headers.get("x-content-type-options") == "nosniff"
    assert first.json() == second.json()


# ── S4: sessions persist, regenerate, and coexist with other cookies ────────

def _cookie_value(set_cookie, name):
    for part in set_cookie.split(","):
        part = part.strip()
        if part.startswith(name + "="):
            return part.split(";", 1)[0].split("=", 1)[1]
    return None


def test_session_mutations_persist_and_login_regenerates():
    app = _app()
    app.enable_session(SessionConfig(cookie_secure=False))

    @app.get("/visit")
    def visit(request):
        n = request.session.get("n", 0) + 1
        request.session["n"] = n
        return {"n": n}

    @app.get("/login")
    def login(request):
        request.session["user_id"] = 42
        return {"ok": True}

    @app.get("/whoami")
    def whoami(request):
        return {"user": request.session.get("user_id"), "n": request.session.get("n")}

    @app.get("/logout")
    def logout(request):
        request.session.destroy()
        return {"ok": True}

    url = _serve(app)
    r = requests.get(url + "/visit")
    sid = _cookie_value(r.headers["set-cookie"], "session_id")
    assert sid and r.json() == {"n": 1}

    cookie = {"Cookie": f"session_id={sid}"}
    assert requests.get(url + "/visit", headers=cookie).json() == {"n": 2}
    assert requests.get(url + "/visit", headers=cookie).json() == {"n": 3}

    r = requests.get(url + "/login", headers=cookie)
    new_sid = _cookie_value(r.headers.get("set-cookie", ""), "session_id")
    assert new_sid and new_sid != sid, "login must issue a fresh session id"
    # The pre-login id is dead; the new one carries the data.
    assert requests.get(url + "/whoami", headers=cookie).json()["user"] is None
    new_cookie = {"Cookie": f"session_id={new_sid}"}
    assert requests.get(url + "/whoami", headers=new_cookie).json() == {"user": 42, "n": 3}

    r = requests.get(url + "/logout", headers=new_cookie)
    assert "max-age=0" in r.headers.get("set-cookie", "").lower()
    assert requests.get(url + "/whoami", headers=new_cookie).json()["user"] is None


def test_session_and_csrf_cookies_both_sent():
    app = _app()
    app.enable_session(SessionConfig(cookie_secure=False))
    app.enable_csrf()

    @app.get("/form")
    def form(request):
        request.session["seen"] = True
        return {"ok": True}

    url = _serve(app)
    r = requests.get(url + "/form")
    raw = r.raw.headers.getlist("Set-Cookie")
    names = {c.split("=", 1)[0] for c in raw}
    assert {"session_id", "__Host-csrf"} <= names, raw


# ── S5: CSRF token from form bodies, not the query string ───────────────────

def test_csrf_accepts_form_field_and_rejects_query_token():
    app = _app()
    app.enable_csrf()

    @app.get("/form")
    def form(request):
        return {"token": request.get_context("csrf_token")}

    @app.post("/submit")
    def submit(request):
        return {"submitted": True}

    url = _serve(app)
    r = requests.get(url + "/form")
    token = r.json()["token"]
    cookie = {"Cookie": f"__Host-csrf={token}"}

    ok = requests.post(url + "/submit", data={"_csrf": token, "x": "1"}, headers=cookie)
    assert ok.status_code == 200, ok.text
    multi = requests.post(url + "/submit", files={"_csrf": (None, token)}, headers=cookie)
    assert multi.status_code == 200, multi.text
    assert requests.post(url + f"/submit?_csrf={token}", headers=cookie).status_code == 403
    assert requests.request("TRACE", url + "/submit", headers=cookie).status_code in (403, 404, 405)

    # The cookie is reused instead of rotated on every page view.
    again = requests.get(url + "/form", headers=cookie)
    assert again.json()["token"] == token
    assert "set-cookie" not in {k.lower() for k in again.headers}


# ── S6: early responses get the after hooks ─────────────────────────────────

def test_rejected_requests_still_get_security_headers():
    app = _app()
    app.enable_security_headers()
    app.enable_jwt(JwtConfig(secret=SECRET), skip_paths=["/__ping__"])

    @app.get("/secure")
    def secure(request):
        return {}

    url = _serve(app)
    r = requests.get(url + "/secure")
    assert r.status_code == 401
    assert r.headers.get("x-content-type-options") == "nosniff"
    # JWT library error text is not echoed.
    bad = requests.get(url + "/secure", headers={"Authorization": f"Bearer {_token(exp=1)}"})
    assert bad.status_code == 401 and "ExpiredSignature" not in bad.text


def test_jwt_issuer_and_audience_enforced():
    app = _app()
    app.enable_jwt(JwtConfig(secret=SECRET, issuer="me", audience="api"), skip_paths=["/__ping__"])

    @app.get("/secure")
    def secure(request):
        return {}

    url = _serve(app)
    good = _token(iss="me", aud="api")
    assert requests.get(url + "/secure", headers={"Authorization": f"Bearer {good}"}).status_code == 200
    for bad in (_token(iss="other", aud="api"), _token(iss="me", aud="other")):
        assert requests.get(url + "/secure", headers={"Authorization": f"Bearer {bad}"}).status_code == 401


# ── B3: compression framing ─────────────────────────────────────────────────

def test_compression_content_length_matches_body():
    app = _app()
    app.enable_compression(min_size=100)

    @app.get("/big")
    def big(request):
        return {"data": "x" * 5000}

    url = _serve(app)
    with socket.create_connection(("127.0.0.1", int(url.rsplit(":", 1)[1]))) as s:
        s.sendall(b"GET /big HTTP/1.1\r\nHost: x\r\nAccept-Encoding: gzip\r\nConnection: close\r\n\r\n")
        raw = b""
        while chunk := s.recv(65536):
            raw += chunk
    head, body = raw.split(b"\r\n\r\n", 1)
    headers = {
        line.split(b":", 1)[0].strip().lower(): line.split(b":", 1)[1].strip()
        for line in head.split(b"\r\n")[1:]
    }
    assert headers[b"content-encoding"] == b"gzip"
    assert int(headers[b"content-length"]) == len(body)
    assert b"x" * 5000 in gzip.decompress(body)

    refused = requests.get(url + "/big", headers={"Accept-Encoding": "gzip;q=0"})
    assert "content-encoding" not in {k.lower() for k in refused.headers}


# ── B4: limits are applied ──────────────────────────────────────────────────

def test_limits_header_size_and_requests_per_connection():
    app = _app()
    app.set_limits(LimitsConfig(max_header_size=8192, max_requests_per_connection=2))
    url = _serve(app)

    port = int(url.rsplit(":", 1)[1])
    with socket.create_connection(("127.0.0.1", port)) as s:
        s.settimeout(5)
        s.sendall(b"GET /__ping__ HTTP/1.1\r\nHost: x\r\nX-Big: " + b"a" * 20000 + b"\r\n\r\n")
        try:
            reply = s.recv(4096)
        except ConnectionResetError:
            reply = b""
    # Rejected: either an explicit 431 or the connection is dropped, never served.
    assert reply == b"" or reply.startswith(b"HTTP/1.1 431"), reply[:60]

    with requests.Session() as s:
        first = s.get(url + "/__ping__")
        second = s.get(url + "/__ping__")
    assert first.headers.get("connection", "").lower() != "close"
    assert second.headers.get("connection", "").lower() == "close"


def test_idle_timeout_closes_keepalive_connection():
    app = _app()
    app.set_timeouts(TimeoutConfig(read_header=30, idle=1, handler=0, read_body=0))
    url = _serve(app)
    port = int(url.rsplit(":", 1)[1])
    with socket.create_connection(("127.0.0.1", port)) as s:
        s.sendall(b"GET /__ping__ HTTP/1.1\r\nHost: x\r\n\r\n")
        assert b"200" in s.recv(4096)
        s.settimeout(5)
        start = time.time()
        assert s.recv(4096) == b""  # server closed the idle connection
        assert time.time() - start < 4


# ── B6: WebSocket routes are actually served ────────────────────────────────

def _ws_frame(text):
    payload = text.encode()
    mask = os.urandom(4)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    assert len(payload) < 126
    return bytes([0x81, 0x80 | len(payload)]) + mask + masked


def _ws_read(sock):
    hdr = sock.recv(2)
    opcode, length = hdr[0] & 0x0F, hdr[1] & 0x7F
    if length == 126:
        length = struct.unpack(">H", sock.recv(2))[0]
    data = b""
    while len(data) < length:
        data += sock.recv(length - len(data))
    return opcode, data


def test_websocket_echo_round_trip():
    app = _app()

    @app.websocket("/ws")
    def echo(ws):
        while (msg := ws.recv()) is not None:
            ws.send_text(f"Echo: {msg.content}")

    url = _serve(app)
    port = int(url.rsplit(":", 1)[1])
    key = base64.b64encode(os.urandom(16)).decode()
    with socket.create_connection(("127.0.0.1", port)) as s:
        s.settimeout(5)
        s.sendall(
            (
                "GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
            ).encode()
        )
        resp = b""
        while b"\r\n\r\n" not in resp:
            resp += s.recv(1)
        assert resp.startswith(b"HTTP/1.1 101")
        for word in ("hi", "there"):
            s.sendall(_ws_frame(word))
            opcode, data = _ws_read(s)
            assert opcode == 1 and data == f"Echo: {word}".encode()


def test_websocket_upgrade_runs_auth():
    app = _app()
    app.enable_jwt(JwtConfig(secret=SECRET), skip_paths=["/__ping__"])

    @app.websocket("/ws")
    def echo(ws):
        ws.close()

    url = _serve(app)
    r = requests.get(
        url + "/ws",
        headers={
            "Upgrade": "websocket",
            "Connection": "Upgrade",
            "Sec-WebSocket-Key": base64.b64encode(os.urandom(16)).decode(),
            "Sec-WebSocket-Version": "13",
        },
    )
    assert r.status_code == 401


# ── S6 / B5: process model ──────────────────────────────────────────────────

_MP_APP = textwrap.dedent(
    """
    import sys
    from cello import App
    app = App()

    @app.get("/pid")
    def pid(request):
        import os
        return {"pid": os.getpid()}

    @app.get("/boom")
    def boom(request):
        raise RuntimeError("secret-detail")

    app.run(host="127.0.0.1", port=int(sys.argv[1]), env="production", workers=2)
    """
)


@pytest.mark.skipif(sys.platform == "win32" or not os.path.isdir("/proc"), reason="uses fork and /proc")
def test_fork_workers_are_exactly_n_and_parent_supervises(tmp_path):
    script = tmp_path / "mp_app.py"
    script.write_text(_MP_APP)
    port = _next_port()
    proc = subprocess.Popen(
        [sys.executable, str(script), str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env={**os.environ, "PYTHONUTF8": "1"},
    )
    try:
        base = f"http://127.0.0.1:{port}"
        deadline = time.time() + 20
        while time.time() < deadline:
            try:
                if requests.get(base + "/pid", timeout=0.5).ok:
                    break
            except Exception:
                time.sleep(0.1)
        children = open(f"/proc/{proc.pid}/task/{proc.pid}/children").read().split()
        assert len(children) == 2, children
        pids = {requests.get(base + "/pid", timeout=2).json()["pid"] for _ in range(30)}
        assert proc.pid not in pids, "the supervisor must not serve requests"
        r = requests.get(base + "/boom", timeout=2)
        assert r.status_code == 500 and "secret-detail" not in r.text
    finally:
        proc.terminate()
        proc.wait(timeout=15)
    for pid in children:
        assert not os.path.exists(f"/proc/{pid}") or "zombie" in open(f"/proc/{pid}/status").read().lower()


# ── Follow-ups from the independent review of these fixes ───────────────────

def test_cache_does_not_store_a_response_that_wrote_the_session():
    app = _app()
    app.enable_session(SessionConfig(cookie_secure=False))
    app.enable_caching(ttl=60)

    @app.get("/cart")
    def cart(request):
        request.session["cart"] = ["secret-item"]
        return {"cart": request.session["cart"]}

    url = _serve(app)
    first = requests.get(url + "/cart")
    assert "set-cookie" in {k.lower() for k in first.headers}
    second = requests.get(url + "/cart")
    assert second.headers.get("x-cache") != "HIT"
    assert "set-cookie" in {k.lower() for k in second.headers}


def test_switching_user_regenerates_session_id():
    app = _app()
    app.enable_session(SessionConfig(cookie_secure=False))

    @app.get("/as/{uid}")
    def login_as(request):
        request.session["user_id"] = int(request.params["uid"])
        return {}

    url = _serve(app)
    sid = _cookie_value(requests.get(url + "/as/1").headers["set-cookie"], "session_id")
    r = requests.get(url + "/as/2", headers={"Cookie": f"session_id={sid}"})
    new_sid = _cookie_value(r.headers.get("set-cookie", ""), "session_id")
    assert new_sid and new_sid != sid


def test_websocket_upgrade_runs_app_guards():
    from cello import RoleGuard

    app = _app()
    app.add_guard(RoleGuard(["admin"]))

    @app.websocket("/ws")
    def echo(ws):
        ws.close()

    url = _serve(app, ready_path="/__ping__")
    r = requests.get(
        url + "/ws",
        headers={
            "Upgrade": "websocket",
            "Connection": "Upgrade",
            "Sec-WebSocket-Key": base64.b64encode(os.urandom(16)).decode(),
            "Sec-WebSocket-Version": "13",
        },
    )
    assert r.status_code in (401, 403)


def test_offline_websocket_recv_timeout_does_not_abort():
    from cello import WebSocket

    ws = WebSocket()
    assert ws.recv(timeout=float("inf")) is None
    assert ws.recv(timeout=float("nan")) is None
