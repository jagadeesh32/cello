"""Cello plugin showcase — every ``app.enable_*`` plugin, verified working.

Run:
    python examples/plugins_demo.py         # starts on http://127.0.0.1:8080

Then try:
    curl http://127.0.0.1:8080/                     # JSON (compressed if large)
    curl http://127.0.0.1:8080/health               # health report
    curl http://127.0.0.1:8080/health/live          # liveness probe
    curl http://127.0.0.1:8080/metrics              # Prometheus metrics
    curl http://127.0.0.1:8080/docs                 # Swagger UI (HTML)
    curl http://127.0.0.1:8080/openapi.json         # OpenAPI schema
    curl http://127.0.0.1:8080/graphql -H 'Accept: text/html'   # playground
    curl -XPOST http://127.0.0.1:8080/graphql -d '{"query":"{ __typename }"}'
    curl -i http://127.0.0.1:8080/                  # see security headers

The auth plugins (JWT / Basic / API-key / CSRF) are global — enabling them would
lock down every route in this single-app demo — so they are shown as commented
recipes at the bottom rather than switched on here. See ``examples/security.py``
for a full auth walk-through.
"""

from cello import (
    App,
    Response,
    RateLimitConfig,
    SecurityHeadersConfig,
    HealthCheckConfig,
    GraphQLConfig,
    OpenTelemetryConfig,
)

app = App()

# ── Observability & traffic management ───────────────────────────────────────
app.enable_logging()                                  # structured request logs
app.enable_compression(min_size=256)                  # gzip responses > 256 bytes
app.enable_caching(ttl=30, exclude_paths=["/metrics"])  # cache GETs for 30s
app.enable_rate_limit(RateLimitConfig.token_bucket(capacity=100, refill_rate=20))
app.enable_circuit_breaker(failure_threshold=5, reset_timeout=30)
app.enable_prometheus(endpoint="/metrics", namespace="demo")
app.enable_telemetry(OpenTelemetryConfig(service_name="plugins-demo"))

# ── Security ─────────────────────────────────────────────────────────────────
app.enable_cors(origins=["http://localhost:3000"])
app.enable_security_headers(
    SecurityHeadersConfig(
        x_frame_options="DENY",
        referrer_policy="strict-origin-when-cross-origin",
        hsts_max_age=31_536_000,
        hsts_include_subdomains=True,
    )
)

# ── Endpoints that Cello serves for you (not registered routes) ──────────────
app.enable_health_checks(HealthCheckConfig())         # /health, /health/live, ...
app.enable_graphql(GraphQLConfig())                   # /graphql + playground


# ── Application routes ───────────────────────────────────────────────────────
@app.get("/")
def index(request):
    return {"framework": "cello", "plugins": "see /docs"}


@app.get("/users/{id}")
def get_user(request) -> dict:
    return {"id": request.params["id"], "name": "Ada Lovelace"}


@app.post("/echo")
def echo(request) -> Response:
    return Response.json({"you_sent": request.json()}, status=201)


# OpenAPI last so it can introspect the routes above.
app.enable_openapi(title="Cello Plugins Demo", version="1.4.0")


# ── Auth recipes (global — uncomment ONE to lock the app down) ───────────────
#
# from cello import JwtConfig, SessionConfig
#
# app.enable_jwt(JwtConfig(secret="change-me-32-bytes-minimum-secret"),
#                skip_paths=["/health", "/metrics", "/docs"])
# app.enable_basic_auth({"admin": "secret"}, realm="Demo")
# app.enable_api_key({"key-123": "client-a"}, header="X-API-Key")
# app.enable_session(SessionConfig())
# app.enable_csrf(allowed_origins=["http://localhost:3000"])


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8080)
