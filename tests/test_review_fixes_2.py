"""Live-HTTP regression tests for the second review of the 1.4.1 fixes
(findings N1-N3, R1-R8) and for the handler-response bugs found while
checking them. Each test names the finding it pins.
"""

import base64
import os
import socket
import struct
import threading
import time

import pytest

requests = pytest.importorskip("requests")

from cello import App, GraphQLConfig, LimitsConfig, Response, SessionConfig, ThreadPoolConfig
from cello.graphql import Mutation, Query, Schema

_PORT = [18400]


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


# ── WebSocket helpers ───────────────────────────────────────────────────────

def _ws_frame(payload, opcode=0x1):
    if isinstance(payload, str):
        payload = payload.encode()
    mask = os.urandom(4)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    n = len(payload)
    if n < 126:
        head = bytes([0x80 | opcode, 0x80 | n])
    elif n < 65536:
        head = bytes([0x80 | opcode, 0x80 | 126]) + struct.pack(">H", n)
    else:
        head = bytes([0x80 | opcode, 0x80 | 127]) + struct.pack(">Q", n)
    return head + mask + masked


def _recv_exact(sock, n):
    data = b""
    while len(data) < n:
        chunk = sock.recv(n - len(data))
        if not chunk:
            raise ConnectionError("closed")
        data += chunk
    return data


def _ws_read(sock):
    hdr = _recv_exact(sock, 2)
    opcode, length = hdr[0] & 0x0F, hdr[1] & 0x7F
    if length == 126:
        length = struct.unpack(">H", _recv_exact(sock, 2))[0]
    elif length == 127:
        length = struct.unpack(">Q", _recv_exact(sock, 8))[0]
    return opcode, _recv_exact(sock, length)


def _ws_open(url, path="/ws", origin=None, host="127.0.0.1"):
    port = int(url.rsplit(":", 1)[1])
    s = socket.create_connection(("127.0.0.1", port))
    s.settimeout(5)
    key = base64.b64encode(os.urandom(16)).decode()
    extra = f"Origin: {origin}\r\n" if origin else ""
    s.sendall(
        (
            f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n"
            f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n"
            f"Sec-WebSocket-Version: 13\r\n{extra}\r\n"
        ).encode()
    )
    resp = b""
    while b"\r\n\r\n" not in resp:
        chunk = s.recv(1)
        if not chunk:
            break
        resp += chunk
    return s, resp


# ── Handler-returned Response objects (found during the R7 check) ───────────

def test_handler_response_keeps_binary_body_files_and_repeated_cookies(tmp_path):
    small = tmp_path / "small.bin"
    small.write_bytes(bytes(range(256)) * 8)
    app = _app()

    @app.get("/bin")
    def binary(request):
        return Response.binary(bytes([0xFF, 0xFE, 0x00, 0x01]))

    @app.get("/file")
    def file(request):
        return Response.sendfile(str(small))

    @app.get("/cookies")
    def cookies(request):
        r = Response.text("x")
        r.append_header("Set-Cookie", "a=1")
        r.append_header("Set-Cookie", "b=2")
        return r

    url = _serve(app)
    assert requests.get(url + "/bin").content == bytes([0xFF, 0xFE, 0x00, 0x01])
    r = requests.get(url + "/file")
    assert r.content == small.read_bytes()
    # The server's file path is internal; it must not reach the client.
    assert not any(k.lower().startswith("x-sendfile") for k in r.headers)
    raw = requests.get(url + "/cookies").raw.headers.getlist("Set-Cookie")
    assert raw == ["a=1", "b=2"], raw


# ── R7: large files are streamed, not buffered ──────────────────────────────

def test_large_file_and_range_are_streamed_intact(tmp_path):
    big = tmp_path / "big.bin"
    blob = os.urandom(3 * 1024 * 1024 + 17)  # over the 1 MiB stream threshold
    big.write_bytes(blob)
    app = _app()
    app.enable_compression()

    @app.get("/big")
    def whole(request):
        return Response.sendfile(str(big))

    @app.get("/part")
    def part(request):
        return Response.file_range(str(big), "bytes=1000-2000999")

    url = _serve(app)
    r = requests.get(url + "/big", headers={"Accept-Encoding": "gzip"})
    assert r.status_code == 200
    assert r.headers["Content-Length"] == str(len(blob))
    assert "Content-Encoding" not in r.headers
    assert r.content == blob
    p = requests.get(url + "/part")
    assert p.status_code == 206
    assert p.content == blob[1000:2001000]
    assert p.headers["Content-Length"] == str(2_000_000)


# ── R8: bounded defaults ────────────────────────────────────────────────────

def test_default_body_cap_is_limits_config_default():
    app = _app()

    @app.post("/upload")
    def upload(request):
        return {"n": len(request.body())}

    url = _serve(app)
    assert LimitsConfig().max_body_size == 10 * 1024 * 1024
    port = int(url.rsplit(":", 1)[1])
    with socket.create_connection(("127.0.0.1", port)) as s:
        s.settimeout(5)
        s.sendall(
            b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: 20000000\r\n\r\n"
        )
        assert b" 413 " in s.recv(4096).split(b"\r\n", 1)[0]
    assert requests.post(url + "/upload", data=b"x" * 1000).json() == {"n": 1000}


# ── R4: trusted proxies are per app; X-Real-IP is opt-in ────────────────────

def test_trusted_proxies_are_per_app_and_real_ip_is_opt_in():
    trusting, plain, real_ip = _app(), _app(), _app()
    trusting.set_trusted_proxies(["127.0.0.1"])
    real_ip.set_trusted_proxies(["127.0.0.1"], trust_x_real_ip=True)
    for app in (trusting, plain, real_ip):

        @app.get("/ip")
        def ip(request):
            return {"ip": request.client_ip()}

    urls = [_serve(a) for a in (trusting, plain, real_ip)]
    xff = {"X-Forwarded-For": "9.9.9.9"}
    assert requests.get(urls[0] + "/ip", headers=xff).json()["ip"] == "9.9.9.9"
    # Another App in the same process does not inherit the list.
    assert requests.get(urls[1] + "/ip", headers=xff).json()["ip"] == "127.0.0.1"
    rip = {"X-Real-IP": "8.8.8.8"}
    assert requests.get(urls[0] + "/ip", headers=rip).json()["ip"] == "127.0.0.1"
    assert requests.get(urls[2] + "/ip", headers=rip).json()["ip"] == "8.8.8.8"


# ── N3: WebSocket Origin check ──────────────────────────────────────────────

def test_websocket_rejects_cross_site_origin():
    app = _app()

    @app.websocket("/ws")
    def ws(ws):
        ws.close()

    @app.websocket("/open", allowed_origins=["https://app.example.com"])
    def open_ws(ws):
        ws.close()

    url = _serve(app)
    port = int(url.rsplit(":", 1)[1])
    cases = [
        ("/ws", None, b" 101 "),  # non-browser client
        ("/ws", f"http://127.0.0.1:{port}", b" 101 "),  # same origin
        ("/ws", "https://evil.example", b" 403 "),
        ("/open", "https://app.example.com", b" 101 "),
        ("/open", "https://other.example.com", b" 403 "),
    ]
    for path, origin, expected in cases:
        s, resp = _ws_open(url, path, origin)
        s.close()
        assert expected in resp.split(b"\r\n", 1)[0], (path, origin, resp[:60])


# ── N2: async WebSocket handlers run on the event loop, not a pool thread ───

def test_async_websocket_handlers_share_the_event_loop():
    app = _app()
    # Two pool threads: under the old model (asyncio.run per socket on the
    # blocking pool) the third concurrent async socket would never start.
    app.set_threadpool(ThreadPoolConfig(size=2))

    @app.websocket("/ws")
    async def echo(ws):
        try:
            ws.recv(timeout=0.01)
        except RuntimeError:
            await ws.asend_text("blocking-refused")
        await ws.asend_text(threading.current_thread().name)
        while (msg := await ws.arecv()) is not None:
            await ws.asend_text(f"Echo: {msg.content}")

    url = _serve(app)
    socks = []
    try:
        for _ in range(5):
            s, resp = _ws_open(url)
            assert b" 101 " in resp.split(b"\r\n", 1)[0]
            socks.append(s)
        for s in socks:
            assert _ws_read(s) == (1, b"blocking-refused")
            assert _ws_read(s) == (1, b"cello-asyncio")
        for i, s in enumerate(socks):
            s.sendall(_ws_frame(f"m{i}"))
        for i, s in enumerate(socks):
            assert _ws_read(s) == (1, f"Echo: m{i}".encode())
    finally:
        for s in socks:
            s.close()


# ── N1: inbound frames are bounded (the server stops reading) ───────────────

def test_websocket_inbound_queue_applies_backpressure():
    app = _app()
    release = threading.Event()

    @app.websocket("/ws")
    def slow(ws):
        release.wait(20)  # a handler that does not read

    url = _serve(app)
    s, resp = _ws_open(url)
    assert b" 101 " in resp.split(b"\r\n", 1)[0]
    frame = _ws_frame(b"x" * (1024 * 1024), opcode=0x2)
    sent = 0
    s.settimeout(3)
    try:
        for _ in range(128):  # 128 MiB if nothing pushed back
            s.sendall(frame)
            sent += 1
    except socket.timeout:
        pass
    finally:
        release.set()
        s.close()
    # The connection task holds at most the inbound budget plus one message;
    # the rest sits in socket buffers. Unbounded queueing would take all 128.
    assert sent < 64, sent


# ── R1: per-process state refuses multiple workers ──────────────────────────

def test_workers_refused_with_in_memory_sessions_or_rate_limits():
    from cello import RateLimitConfig

    for enable in (
        lambda a: a.enable_session(SessionConfig(cookie_secure=False)),
        lambda a: a.enable_rate_limit(RateLimitConfig.token_bucket(10, 1)),
    ):
        app = _app()
        enable(app)
        with pytest.raises(RuntimeError, match="allow_per_worker_state"):
            app.run(host="127.0.0.1", port=_next_port(), workers=2)


# ── R5: CSRF token bound to the session created on login ────────────────────

def test_csrf_token_binds_to_session_created_by_login():
    app = _app()
    app.enable_session(SessionConfig(cookie_secure=False))
    app.enable_csrf()

    @app.get("/form")
    def form(request):
        return {"token": request.get_context("csrf_token")}

    @app.post("/login")
    def login(request):
        request.session["user_id"] = 1
        return {"ok": True}

    @app.post("/action")
    def action(request):
        return {"done": True}

    url = _serve(app)
    r = requests.get(url + "/form")
    token = r.json()["token"]
    names = {c.split("=", 1)[0] for c in r.raw.headers.getlist("Set-Cookie")}
    assert names == {"__Host-csrf"}, names  # no session for an anonymous GET
    csrf = {"Cookie": f"__Host-csrf={token}", "X-CSRF-Token": token}
    login = requests.post(url + "/login", headers=csrf)
    assert login.status_code == 200, login.text
    sid = login.cookies.get("session_id")
    assert sid

    ok = requests.post(
        url + "/action",
        headers={"Cookie": f"session_id={sid}; __Host-csrf={token}", "X-CSRF-Token": token},
    )
    assert ok.status_code == 200, ok.text
    # A planted cookie/header pair no longer passes once the session holds
    # the token (it would have under plain double-submit).
    planted = "f" * 64
    bad = requests.post(
        url + "/action",
        headers={"Cookie": f"session_id={sid}; __Host-csrf={planted}", "X-CSRF-Token": planted},
    )
    assert bad.status_code == 403


# ── R6: configurable rotation keys and session size cap ─────────────────────

def test_session_auth_keys_and_size_cap():
    app = _app()
    app.enable_session(
        SessionConfig(cookie_secure=False, auth_keys=["member"], max_session_size=1024)
    )

    @app.post("/seed")
    def seed(request):
        request.session["cart"] = []
        return {"ok": True}

    @app.post("/login")
    def login(request):
        request.session["member"] = "m1"
        return {"ok": True}

    @app.post("/big")
    def big(request):
        try:
            request.session["blob"] = "x" * 4096
        except ValueError:
            return {"refused": True}
        return {"refused": False}

    url = _serve(app)
    sid = requests.post(url + "/seed").cookies["session_id"]
    rotated = requests.post(url + "/login", headers={"Cookie": f"session_id={sid}"})
    new_sid = rotated.cookies.get("session_id")
    assert new_sid and new_sid != sid
    assert requests.post(url + "/big", headers={"Cookie": f"session_id={new_sid}"}).json() == {
        "refused": True
    }
    assert "uid" in SessionConfig.default_auth_keys()


# ── R2: GraphQL is parsed, limited, and dispatched by operation type ────────

def test_builtin_graphql_endpoint_uses_a_real_parser():
    app = _app()
    app.enable_graphql(GraphQLConfig(max_depth=3, max_complexity=5))
    url = _serve(app)

    def gql(query, **extra):
        return requests.post(url + "/graphql", json={"query": query, **extra})

    assert gql("{ __typename }").json() == {"data": {"__typename": "Query"}}
    # A commented or named mutation is a mutation (this schema has none).
    r = gql("# note\nmutation Create { createUser { id } }")
    assert r.status_code == 400 and "mutation" in r.text
    assert gql("{ a { b { c { d } } } }").status_code == 400
    assert gql("{ a b c d e f }").status_code == 400
    r = gql("{ ...F } fragment F on Query { __schema { types { name } } }")
    assert r.status_code == 400 and "Introspection" in r.text
    assert gql("{ a ").status_code == 400


def test_python_graphql_schema_endpoint_runs_only_selected_fields():
    calls = []

    @Query
    def users(info) -> list:
        calls.append("users")
        return [{"id": 1, "name": "Ada", "password": "secret"}]

    @Mutation
    def delete_all(info) -> bool:
        calls.append("delete_all")
        return True

    app = _app()
    app.enable_graphql(
        GraphQLConfig(max_depth=4), schema=Schema().query(users).mutation(delete_all)
    )
    url = _serve(app)
    r = requests.post(url + "/graphql", json={"query": "{ users { id name } }"})
    assert r.status_code == 200
    assert r.json() == {"data": {"users": [{"id": 1, "name": "Ada"}]}}
    assert calls == ["users"], calls  # a query never runs a mutation
    r = requests.post(
        url + "/graphql",
        json={"query": "mutation Wipe { deleteAll }", "operationName": "Wipe"},
    )
    assert r.json() == {"data": {"deleteAll": True}}
    assert calls == ["users", "delete_all"]
    assert requests.post(url + "/graphql", json={"query": "{ a { b { c { d { e } } } } }"}).status_code == 400
