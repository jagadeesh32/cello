# Plugins (`app.enable_*`)

Cello ships its batteries as **plugins** you switch on with a single
`app.enable_*()` call. Each one either installs Rust middleware on the hot path
or serves a built-in endpoint. This page is the authoritative list — every
plugin here is verified end-to-end by `tests/test_plugins.py`, and a runnable
showcase lives in [`examples/plugins_demo.py`](../../examples/index.md).

```python
from cello import App, RateLimitConfig

app = App()
app.enable_logging()
app.enable_compression(min_size=256)
app.enable_rate_limit(RateLimitConfig.token_bucket(capacity=100, refill_rate=20))
app.enable_prometheus(endpoint="/metrics")
```

!!! note "Order & scope"
    Middleware run in priority order (auth early, compression late), not
    registration order, so `enable_*` calls can appear in any order. Auth,
    rate-limit, CSRF and circuit-breaker are **global** — they apply to every
    route. Call `enable_openapi()` **after** your routes so it can introspect
    them.

---

## Observability & traffic

| Plugin | Signature | Effect |
| --- | --- | --- |
| `enable_logging` | `enable_logging()` | Structured request/response logs |
| `enable_compression` | `enable_compression(min_size=None)` | gzip responses larger than `min_size` bytes when the client sends `Accept-Encoding: gzip` |
| `enable_caching` | `enable_caching(ttl=300, methods=None, exclude_paths=None, compress=True)` | In-memory response cache with TTL; a HIT is gzipped inline for gzip clients (`compress=True`); invalidate with `app.invalidate_cache([...])` |
| `enable_rate_limit` | `enable_rate_limit(config)` | Token-bucket / sliding-window / adaptive limiting |
| `enable_circuit_breaker` | `enable_circuit_breaker(failure_threshold=5, reset_timeout=30, half_open_target=3, failure_codes=None)` | Trips open after repeated failures |
| `enable_prometheus` | `enable_prometheus(endpoint="/metrics", namespace=None, subsystem=None)` | Serves Prometheus metrics at `endpoint` |
| `enable_telemetry` | `enable_telemetry(OpenTelemetryConfig(...))` | OpenTelemetry traces/metrics |

```python
from cello import RateLimitConfig, OpenTelemetryConfig

app.enable_caching(ttl=30, exclude_paths=["/metrics"])
app.enable_rate_limit(RateLimitConfig.sliding_window(max_requests=5, window_secs=60))
app.enable_prometheus(endpoint="/metrics", namespace="myapp")
app.enable_telemetry(OpenTelemetryConfig(service_name="myapp"))
```

!!! note "Cache + compression"
    A cache **HIT** short-circuits the pipeline (the compression middleware
    never runs on it), so the cache gzips the HIT **inline** for clients that
    send `Accept-Encoding: gzip` when `compress=True` (the default) and the body
    is ≥ 1 KB. It sets `Vary: Accept-Encoding`, and non-gzip clients still get
    identity. Pass `compress=False` to disable.

---

## Security & auth

All auth plugins are **global**. Use `skip_paths=` (JWT) or register public
routes appropriately for health/metrics/docs.

| Plugin | Signature | Effect |
| --- | --- | --- |
| `enable_cors` | `enable_cors(origins=None)` | Reflects listed origins (adds `Vary: Origin`); `None` = `*`, never with credentials |
| `enable_security_headers` | `enable_security_headers(config=None)` | Adds HSTS, `X-Frame-Options`, `X-Content-Type-Options`, `Referrer-Policy`, … |
| `enable_jwt` | `enable_jwt(JwtConfig(...), skip_paths=None)` | Validates `Bearer` tokens (header/cookie), stores claims in context |
| `enable_basic_auth` | `enable_basic_auth(credentials, realm=None)` | HTTP Basic; issues a `WWW-Authenticate` challenge on 401 |
| `enable_api_key` | `enable_api_key(keys, header=None)` | API-key auth via header (default `X-API-Key`) |
| `enable_session` | `enable_session(SessionConfig()=None)` | Server-side sessions behind a random cookie id; `request.session` |
| `enable_csrf` | `enable_csrf(cookie_name=None, header_name=None, allowed_origins=None)` | Double-submit-cookie CSRF protection |

`enable_security_headers` accepts three forms:

```python
from cello import SecurityHeadersConfig, CSP

app.enable_security_headers()                          # sensible defaults
app.enable_security_headers(True)                      # strict preset (CSP, HSTS, cross-origin isolation)
app.enable_security_headers(SecurityHeadersConfig(     # explicit — full control
    x_frame_options="DENY",
    referrer_policy="strict-origin-when-cross-origin",
    hsts_max_age=31_536_000,
    hsts_include_subdomains=True,
    # Content-Security-Policy via the CSP builder:
    csp=CSP().default_src(["'self'"]).img_src(["'self'", "data:"]),
    # Permissions-Policy as {feature: [allowed-origins]} ([] = disabled):
    permissions_policy={"geolocation": [], "camera": ["'self'"]},
    # Cross-origin isolation:
    coep="require-corp",            # unsafe-none | require-corp | credentialless
    coop="same-origin",             # unsafe-none | same-origin | same-origin-allow-popups
    corp="same-origin",             # same-site | same-origin | cross-origin
))
```

`SecurityHeadersConfig.secure()` returns a hardened preset (DENY framing, 1-year
HSTS + subdomains, and `require-corp` / `same-origin` cross-origin isolation).

JWT / Basic / API-key:

```python
from cello import JwtConfig

app.enable_jwt(JwtConfig(secret="a-32-byte-minimum-secret-value!!"),
               skip_paths=["/health", "/metrics", "/docs"])
app.enable_basic_auth({"admin": "secret"}, realm="Admin Area")
app.enable_api_key({"key-123": "client-a"}, header="X-API-Key")
```

!!! info "JWT `exp` is required"
    Cello rejects tokens without an `exp` claim (a security-positive default —
    `jsonwebtoken` validates expiry). `iat` is optional. Adjust clock skew with
    `JwtConfig(leeway=...)`.

---

## Built-in endpoints

These plugins serve their own paths — you don't register a route for them. Cello
serves them even though they aren't in the router. Since 1.4.1 these endpoints
(and `/metrics`) run behind the full middleware chain: with JWT, Basic or
API-key auth enabled they require credentials, and rate limits apply. List
probe paths in `skip_paths` to keep them public, e.g.
`app.enable_jwt(cfg, skip_paths=["/health/live", "/health/ready"])`.

| Plugin | Signature | Serves |
| --- | --- | --- |
| `enable_health_checks` | `enable_health_checks(HealthCheckConfig()=None)` | `/health`, `/health/live`, `/health/ready`, `/health/startup` |
| `enable_openapi` | `enable_openapi(title=None, version=None)` | `/docs` (Swagger UI), `/redoc`, `/openapi.json` |
| `enable_graphql` | `enable_graphql(GraphQLConfig()=None)` | `/graphql` (POST queries; GET playground only with `playground=True`, off by default) |

```python
from cello import HealthCheckConfig, GraphQLConfig

app.enable_health_checks(HealthCheckConfig())
app.enable_graphql(GraphQLConfig())

@app.get("/users")
def users(request):
    return {"users": []}

app.enable_openapi(title="My API", version="1.0.0")   # call AFTER routes
```

!!! note "Kubernetes probes"
    Point liveness at `/health/live` and readiness at `/health/ready`. Toggle
    readiness at runtime from a startup/shutdown hook via the health middleware.

---

## Data layer

Real, native connections (see the [Data Layer guide](../../data-layer.md)).

| Plugin | Signature | Effect |
| --- | --- | --- |
| `enable_database` | `enable_database(DatabaseConfig(url=..., pool_size=...))` | Native Postgres pool at `app.database` / `request.database` |
| `enable_redis` | `enable_redis(RedisConfig(url=...))` | Native async Redis client at `app.redis` / `request.redis`; supports `redis://` and `rediss://` (TLS) |
| `enable_templates` | `enable_templates(...)` | MiniJinja (Jinja2-compatible) template engine |

```python
from cello import DatabaseConfig, RedisConfig

app.enable_database(DatabaseConfig(url="postgres://user:pass@localhost/db", pool_size=10))
app.enable_redis(RedisConfig(url="redis://localhost:6379"))
app.enable_redis(RedisConfig(url="rediss://user:pass@host:6380"))   # TLS (rustls)
```

---

## Enterprise patterns & protocols

!!! warning "Announce-only on the App"
    `enable_grpc`, `enable_messaging`, `enable_rabbitmq`, `enable_sqs`,
    `enable_event_sourcing`, `enable_cqrs` and `enable_saga` **print their
    configuration but do not wire runtime behaviour into the HTTP server** —
    they are convenience announcers. The real functionality lives in dedicated
    Python modules and is fully usable there:

    | Announcer | Use this instead |
    | --- | --- |
    | `enable_grpc` | `from cello.grpc import ...` |
    | `enable_messaging` / `enable_rabbitmq` / `enable_sqs` | `from cello.messaging import ...` |
    | `enable_event_sourcing` | `from cello.eventsourcing import ...` |
    | `enable_cqrs` | `from cello.cqrs import ...` |
    | `enable_saga` | `from cello.saga import ...` |

    Enabling them is harmless (the server keeps working), but don't expect a
    `/graphql`-style endpoint to appear from `enable_grpc()` alone.

---

## Verification

Every plugin above with an HTTP effect is asserted against a live server in
`tests/test_plugins.py`:

```bash
pytest tests/test_plugins.py -v
```

The announce-only plugins are covered by a test that asserts enabling them does
not break the server.
