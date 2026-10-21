"""End-to-end verification of the ``App.enable_*`` plugins.

Every plugin that has an observable HTTP effect is driven against a real Cello
server (daemon thread) and asserted on status codes / headers / bodies. The
"announce-only" plugins (gRPC, messaging, RabbitMQ, SQS, event-sourcing, CQRS,
saga) print their configuration but do not wire runtime behaviour into the HTTP
app — the real functionality lives in the ``cello.grpc`` / ``cello.messaging`` /
``cello.cqrs`` / ``cello.saga`` / ``cello.eventsourcing`` Python modules — so we
only assert that enabling them does not break the server.

Regression coverage for bugs fixed in v1.3.0:
  * health checks / GraphQL 404 (fast-404 ran before the middleware chain)
  * BasicAuth 401 missing ``WWW-Authenticate``
  * JWT rejecting a token without ``iat``
  * ``enable_security_headers`` not accepting a ``SecurityHeadersConfig``
"""

import threading
import time

import pytest

requests = pytest.importorskip("requests")
pyjwt = pytest.importorskip("jwt")

from cello import (
    App,
    Response,
    JwtConfig,
    SessionConfig,
    SecurityHeadersConfig,
    HealthCheckConfig,
    GraphQLConfig,
    OpenTelemetryConfig,
    GrpcConfig,
    KafkaConfig,
    RabbitMQConfig,
    SqsConfig,
)

_PORT = [17400]


def _next_port():
    _PORT[0] += 1
    return _PORT[0]


def _serve(app, *, ready_path="/__ping__", auth=None):
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
            requests.get(base + ready_path, timeout=0.5, auth=auth)
            return base
        except Exception as e:  # not up yet
            last = e
            time.sleep(0.1)
    raise RuntimeError(f"server on :{port} never came up: {last}")


def _ping_app():
    app = App()

    @app.get("/__ping__")
    def ping(request):
        return {"ok": True}

    return app


# ── JWT ─────────────────────────────────────────────────────────────────────

def test_jwt_auth_flow():
    app = _ping_app()
    app.enable_jwt(JwtConfig(secret="s" * 32), skip_paths=["/__ping__", "/open"])

    @app.get("/secure")
    def secure(request):
        return {"secure": True}

    @app.get("/open")
    def open_(request):
        return {"open": True}

    url = _serve(app)
    assert requests.get(f"{url}/secure").status_code == 401
    assert requests.get(f"{url}/open").status_code == 200  # skip_path bypass

    token = pyjwt.encode(
        {"sub": "u1", "exp": int(time.time()) + 3600}, "s" * 32, algorithm="HS256"
    )
    ok = requests.get(f"{url}/secure", headers={"Authorization": f"Bearer {token}"})
    assert ok.status_code == 200

    assert requests.get(
        f"{url}/secure", headers={"Authorization": "Bearer nonsense"}
    ).status_code == 401


def test_jwt_token_without_iat_is_accepted():
    """Regression: a standard ``sub``+``exp`` token (no ``iat``) must decode."""
    app = _ping_app()
    app.enable_jwt(JwtConfig(secret="k" * 32), skip_paths=["/__ping__"])

    @app.get("/secure")
    def secure(request):
        return {"ok": True}

    url = _serve(app)
    token = pyjwt.encode(
        {"sub": "u", "exp": int(time.time()) + 3600}, "k" * 32, algorithm="HS256"
    )
    r = requests.get(f"{url}/secure", headers={"Authorization": f"Bearer {token}"})
    assert r.status_code == 200, r.text


# ── Basic auth ──────────────────────────────────────────────────────────────

def test_basic_auth_challenge_and_flow():
    app = _ping_app()
    app.enable_basic_auth({"admin": "secret"}, realm="Test")

    @app.get("/data")
    def data(request):
        return {"data": 1}

    url = _serve(app)
    r = requests.get(f"{url}/data")
    assert r.status_code == 401
    # Regression: the WWW-Authenticate challenge must be present on the 401.
    assert r.headers.get("www-authenticate", "").startswith('Basic realm="Test"')
    assert requests.get(f"{url}/data", auth=("admin", "secret")).status_code == 200
    assert requests.get(f"{url}/data", auth=("admin", "nope")).status_code == 401


# ── API key ─────────────────────────────────────────────────────────────────

def test_api_key_auth_flow():
    app = _ping_app()
    app.enable_api_key({"key-123": "client-a"}, header="X-API-Key")

    @app.get("/data")
    def data(request):
        return {"data": 1}

    url = _serve(app, ready_path="/data")  # ping is protected too; any resp is fine
    assert requests.get(f"{url}/data").status_code == 401
    assert requests.get(
        f"{url}/data", headers={"X-API-Key": "key-123"}
    ).status_code == 200
    assert requests.get(
        f"{url}/data", headers={"X-API-Key": "bad"}
    ).status_code == 401


# ── Session ─────────────────────────────────────────────────────────────────

def test_session_sets_cookie():
    app = _ping_app()
    app.enable_session(SessionConfig())

    @app.get("/login")
    def login(request):
        request.session["user_id"] = 1
        return {"ok": True}

    url = _serve(app)
    cookie = requests.get(f"{url}/login").headers.get("set-cookie", "").lower()
    assert "session" in cookie or "cello" in cookie
    # 1.4.1 deliberate change: a session nothing wrote to is neither stored nor
    # sent, so cookieless clients cannot fill the session store.
    assert "set-cookie" not in {k.lower() for k in requests.get(f"{url}/__ping__").headers}


# ── Security headers ────────────────────────────────────────────────────────

def test_security_headers_bool():
    app = _ping_app()
    app.enable_security_headers(True)  # strict preset
    url = _serve(app)
    h = {k.lower() for k in requests.get(f"{url}/__ping__").headers}
    assert "x-content-type-options" in h
    assert "x-frame-options" in h


def test_security_headers_config_object():
    """Regression: enable_security_headers must accept a SecurityHeadersConfig."""
    app = _ping_app()
    app.enable_security_headers(SecurityHeadersConfig(x_frame_options="DENY"))
    url = _serve(app)
    r = requests.get(f"{url}/__ping__")
    assert r.headers.get("x-frame-options") == "DENY"
    assert r.headers.get("x-content-type-options", "").lower() == "nosniff"


def test_security_headers_default():
    app = _ping_app()
    app.enable_security_headers()  # no arg -> defaults
    url = _serve(app)
    h = {k.lower() for k in requests.get(f"{url}/__ping__").headers}
    assert "x-content-type-options" in h


# ── CSRF ────────────────────────────────────────────────────────────────────

def test_csrf_blocks_and_issues_cookie():
    app = _ping_app()
    app.enable_csrf()

    @app.get("/form")
    def form(request):
        return {"ok": True}

    @app.post("/submit")
    def submit(request):
        return {"submitted": True}

    url = _serve(app)
    assert requests.post(f"{url}/submit", json={}).status_code == 403
    assert "__Host-csrf=" in requests.get(f"{url}/form").headers.get("set-cookie", "")


# ── Health checks ───────────────────────────────────────────────────────────

def test_health_checks_endpoints():
    """Regression: /health* are not routes; must be served before the fast-404."""
    app = _ping_app()
    app.enable_health_checks(HealthCheckConfig())
    url = _serve(app)
    assert requests.get(f"{url}/health").status_code == 200
    assert requests.get(f"{url}/health/live").status_code == 200
    assert requests.get(f"{url}/health/ready").status_code == 200


# ── GraphQL ─────────────────────────────────────────────────────────────────

def test_graphql_playground_and_query():
    """Regression: /graphql is not a route; must be served before the fast-404."""
    app = _ping_app()
    app.enable_graphql(GraphQLConfig(playground=True, introspection=True))
    url = _serve(app)
    assert requests.get(f"{url}/graphql", headers={"Accept": "text/html"}).status_code == 200
    q = requests.post(f"{url}/graphql", json={"query": "{ __typename }"})
    assert 200 <= q.status_code < 300, q.text


# ── OpenAPI ─────────────────────────────────────────────────────────────────

def test_openapi_docs_and_spec():
    app = _ping_app()

    @app.get("/users")
    def users(request):
        return {"users": []}

    app.enable_openapi(title="Test API", version="9.9.9")
    url = _serve(app)
    docs = requests.get(f"{url}/docs")
    assert docs.status_code == 200 and "swagger" in docs.text.lower()
    spec = requests.get(f"{url}/openapi.json")
    assert spec.status_code == 200
    body = spec.json()
    assert "openapi" in body or "paths" in body


# ── Telemetry ───────────────────────────────────────────────────────────────

def test_telemetry_serves():
    app = _ping_app()
    app.enable_telemetry(OpenTelemetryConfig(service_name="verify-svc"))
    url = _serve(app)
    assert requests.get(f"{url}/__ping__").status_code == 200


# ── Announce-only plugins: enabling them must not break the server ───────────

@pytest.mark.parametrize(
    "enable",
    [
        lambda app: app.enable_grpc(GrpcConfig()),
        lambda app: app.enable_messaging(KafkaConfig(["localhost:9092"])),
        lambda app: app.enable_rabbitmq(RabbitMQConfig()),
        lambda app: app.enable_sqs(SqsConfig(region="us-east-1", queue_url="q")),
        lambda app: app.enable_event_sourcing(),
        lambda app: app.enable_cqrs(),
        lambda app: app.enable_saga(),
    ],
)
def test_announce_only_plugins_do_not_break_server(enable):
    app = _ping_app()
    enable(app)
    url = _serve(app)
    assert requests.get(f"{url}/__ping__").status_code == 200
