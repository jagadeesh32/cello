"""
Cello - Ultra-fast Rust-powered Python async web framework.

A high-performance async web framework with Rust core and Python developer experience.
All I/O, routing, and JSON serialization happen in Rust for maximum performance.

Features:
- Native async/await support (both sync and async handlers)
- SIMD-accelerated JSON parsing
- Middleware system with CORS, logging, compression
- Blueprint-based routing with inheritance
- WebSocket and SSE support
- File uploads and multipart form handling
- Enterprise features:
  - JWT, Basic, and API Key authentication
  - Rate limiting (token bucket, sliding window)
  - Session management
  - Security headers (CSP, HSTS, etc.)
  - Cluster mode with multiple workers
  - HTTP/2 and HTTP/3 (QUIC) support
  - TLS/SSL configuration
  - Request/response timeouts

Example:
    from cello import App, Blueprint

    app = App()

    # Enable built-in middleware
    app.enable_cors()
    app.enable_logging()

    # Sync handler (simple operations)
    @app.get("/")
    def home(request):
        return {"message": "Hello, Cello!"}

    # Async handler (for I/O operations like database calls)
    @app.get("/users")
    async def get_users(request):
        users = await database.fetch_all()
        return {"users": users}

    # Blueprint for route grouping
    api = Blueprint("/api")

    @api.get("/users/{id}")
    async def get_user(request):
        user = await database.fetch_user(request.params["id"])
        return user

    app.register_blueprint(api)

"""

import json

from .validation import wrap_handler_with_validation, wrap_handler_with_body


def _validate_wrap(func, body):
    """Apply explicit `body=` validation (400) when a model is given, else fall
    back to type-hint-based Pydantic validation (422)."""
    if body is not None:
        return wrap_handler_with_body(func, body)
    return wrap_handler_with_validation(func)


_VALIDATION_ERRORS_SCHEMA = {
    "type": "object",
    "properties": {
        "detail": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "loc": {"type": "array", "items": {"type": "string"}},
                    "msg": {"type": "string"},
                    "type": {"type": "string"},
                },
            },
        }
    },
}


def _component_name(model, components, owners):
    """A components/schemas name for ``model``: its class name, suffixed when a
    different model already uses that name. The same model always maps to the
    same component, so routes sharing a body model share one schema."""
    base = model.__name__
    name, n = base, 1
    while name in owners and owners[name] is not model:
        n += 1
        name = f"{base}{n}"
    return name


def _body_openapi_schema(model, components, owners):
    """OpenAPI schema (a ``$ref``) for a route's ``body=`` model, adding the
    model to ``components``. ``None`` when the model cannot describe itself
    (dataclasses, plain classes)."""
    if model is None:
        return None
    from cello._cello import ModelSpec, schema_openapi

    meta = getattr(model, "_meta", None)
    schema = getattr(meta, "schema", None)
    if isinstance(schema, ModelSpec):
        name = _component_name(model, components, owners)
        if name not in owners:
            owners[name] = model
            components[name] = schema_openapi(schema)
        return {"$ref": f"#/components/schemas/{name}"}
    json_schema = getattr(model, "model_json_schema", None)
    if callable(json_schema):
        name = _component_name(model, components, owners)
        if name not in owners:
            try:
                doc = json_schema(ref_template="#/components/schemas/{model}")
            except Exception:  # noqa: BLE001 - docs must never break the app
                return None
            for def_name, def_schema in doc.pop("$defs", {}).items():
                components.setdefault(def_name, def_schema)
            owners[name] = model
            components[name] = doc
        return {"$ref": f"#/components/schemas/{name}"}
    return None


def _mark_blocking(handler, blocking):
    """Record an explicit `blocking=` override for the offload policy.

    Sync handlers are normally timed and moved to the blocking threadpool only
    once observed to block. `True` pools the handler from the very first request;
    `False` pins it inline. Read by the Rust handler registry at registration.
    """
    if blocking is not None:
        try:
            handler.__cello_blocking__ = bool(blocking)
        except AttributeError:
            pass  # callable object with no writable __dict__; stays adaptive
    return handler
from .database import transactional, TransactionAborted
from .guards import (
    Guard,
    Role as RoleGuard,
    Permission as PermissionGuard,
    Authenticated,
    And,
    Or,
    Not,
    GuardError,
    ForbiddenError,
    UnauthorizedError,
)
from cello._cello import (
    Blueprint as _RustBlueprint,
)
from cello._cello import (
    FormData,
    Request,
    Response,
    SseEvent,
    SseStream,
    UploadedFile,
    Cello,
    WebSocket,
    WebSocketMessage,
)

# Advanced configuration classes
from cello._cello import (
    TimeoutConfig,
    LimitsConfig,
    ThreadPoolConfig,
    ClusterConfig,
    TlsConfig,
    Http2Config,
    Http3Config,
    JwtConfig,
    RateLimitConfig,
    SessionConfig,
    SecurityHeadersConfig,
    CSP,
    StaticFilesConfig,
)

# v0.5.0 - New features
from cello._cello import (
    PyBackgroundTasks as BackgroundTasks,
    PyTemplateEngine as TemplateEngine,
)

# v1.1.0 - MiniJinja template engine
from cello._cello import (
    MiniJinjaEngine,
)

# v0.7.0 - Enterprise features
from cello._cello import (
    OpenTelemetryConfig,
    HealthCheckConfig,
    DatabaseConfig,
    GraphQLConfig,
)

# v0.8.0 - Data Layer features
from cello._cello import (
    RedisConfig,
)

# v1.3.0 - Native async data layer (real Postgres pool + Redis client)
from cello._cello import (
    Database,
    Transaction,
    Redis,
)

# v1.4.0 - Pre-serialized query rows (`await queryset.json()`)
from cello._cello import RawJSON

# v1.4.0 - Database exceptions (SQLSTATE-mapped; DatabaseError subclasses RuntimeError)
from cello._cello import (
    DatabaseError,
    IntegrityError,
    UniqueViolation,
    NotNullViolation,
    ForeignKeyViolation,
    CheckViolation,
    DataError,
    ParameterTypeError,
    ParameterRangeError,
    ParameterValueError,
    OperationalError,
    ProgrammingError,
)

# v0.9.0 - API Protocol features
from cello._cello import (
    GrpcConfig,
    KafkaConfig,
    RabbitMQConfig,
    SqsConfig,
)

# v0.10.0 - Advanced Pattern features
from cello._cello import (
    EventSourcingConfig,
    CqrsConfig,
    SagaConfig,
)

# Rust-native async HTTP client (reqwest + Tokio, no GIL during I/O)
from cello._cello import (
    AsyncClient,
    HttpResponse,
)

# RFC 7807 Problem Details
from cello._cello import ProblemDetails

def validate_jwt_config(config: JwtConfig) -> JwtConfig:
    """Validate a JwtConfig instance.

    Args:
        config: JwtConfig to validate.

    Returns:
        The validated JwtConfig.

    Raises:
        ValueError: If the config has invalid values.
    """
    if not getattr(config, "secret", None):
        raise ValueError("JwtConfig: 'secret' must not be empty")
    return config


def validate_session_config(config: SessionConfig) -> SessionConfig:
    """Validate a SessionConfig instance.

    Args:
        config: SessionConfig to validate.

    Returns:
        The validated SessionConfig.

    Raises:
        ValueError: If the config has invalid values.
    """
    if not getattr(config, "cookie_name", None):
        raise ValueError("SessionConfig: 'cookie_name' must not be empty")
    return config


def validate_rate_limit_config(config: RateLimitConfig) -> RateLimitConfig:
    """Validate a RateLimitConfig instance.

    Args:
        config: RateLimitConfig to validate.

    Returns:
        The validated RateLimitConfig.

    Raises:
        ValueError: If the config has invalid values.
    """
    max_requests = getattr(config, "max_requests", None)
    if max_requests is not None and max_requests <= 0:
        raise ValueError("RateLimitConfig: 'max_requests' must be positive")
    window_secs = getattr(config, "window_secs", None)
    if window_secs is not None and window_secs <= 0:
        raise ValueError("RateLimitConfig: 'window_secs' must be positive")
    return config


def validate_tls_config(config: TlsConfig) -> TlsConfig:
    """Validate a TlsConfig instance.

    Args:
        config: TlsConfig to validate.

    Returns:
        The validated TlsConfig.

    Raises:
        ValueError: If the config has invalid values.
    """
    cert_path = getattr(config, "cert_path", None)
    key_path = getattr(config, "key_path", None)
    if not cert_path or not isinstance(cert_path, str):
        raise ValueError("TlsConfig: 'cert_path' must be a non-empty string")
    if not key_path or not isinstance(key_path, str):
        raise ValueError("TlsConfig: 'key_path' must be a non-empty string")
    return config


__all__ = [
    # Core
    "App",
    "Blueprint",
    "Request",
    "Response",
    "WebSocket",
    "WebSocketMessage",
    "SseEvent",
    "SseStream",
    "FormData",
    "UploadedFile",
    # Advanced Configuration
    "TimeoutConfig",
    "LimitsConfig",
    "ThreadPoolConfig",
    "ClusterConfig",
    "TlsConfig",
    "Http2Config",
    "Http3Config",
    "JwtConfig",
    "RateLimitConfig",
    "SessionConfig",
    "SecurityHeadersConfig",
    "CSP",
    "StaticFilesConfig",
    # v0.5.0 - New features
    "BackgroundTasks",
    "TemplateEngine",
    "Depends",
    "cache",
    # Async HTTP client
    "AsyncClient",
    "HttpResponse",
    # v1.1.0 - MiniJinja template engine
    "MiniJinjaEngine",
    # Guards (RBAC)
    "Guard",
    "RoleGuard",
    "PermissionGuard",
    "Authenticated",
    "And",
    "Or",
    "Not",
    "GuardError",
    "ForbiddenError",
    "UnauthorizedError",
    # v0.7.0 - Enterprise features
    "OpenTelemetryConfig",
    "HealthCheckConfig",
    "DatabaseConfig",
    "GraphQLConfig",
    # v0.8.0 - Data Layer features
    "RedisConfig",
    "Database",
    "Redis",
    "Transaction",
    "transactional",
    "TransactionAborted",
    "RawJSON",
    # v1.4.0 - Database exceptions
    "DatabaseError",
    "IntegrityError",
    "UniqueViolation",
    "NotNullViolation",
    "ForeignKeyViolation",
    "CheckViolation",
    "DataError",
    "ParameterTypeError",
    "ParameterRangeError",
    "ParameterValueError",
    "OperationalError",
    "ProgrammingError",
    # v0.9.0 - API Protocol features
    "GrpcConfig",
    "KafkaConfig",
    "RabbitMQConfig",
    "SqsConfig",
    # v0.10.0 - Advanced Pattern features
    "EventSourcingConfig",
    "CqrsConfig",
    "SagaConfig",
    # RFC 7807
    "ProblemDetails",
    # Config validators
    "validate_jwt_config",
    "validate_session_config",
    "validate_rate_limit_config",
    "validate_tls_config",
]
__version__ = "1.4.1"


class Blueprint:
    """
    Blueprint for grouping routes with a common prefix.

    Provides decorator syntax for route registration.
    """

    def __init__(self, prefix: str, name: str = None):
        """
        Create a new Blueprint.

        Args:
            prefix: URL prefix for all routes in this blueprint
            name: Optional name for the blueprint
        """
        self._bp = _RustBlueprint(prefix, name)

    @property
    def prefix(self) -> str:
        """Get the blueprint's URL prefix."""
        return self._bp.prefix

    @property
    def name(self) -> str:
        """Get the blueprint's name."""
        return self._bp.name

    def get(self, path: str, guards: list = None, body: type = None, blocking: bool = None):
        """Register a GET route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(func, body), guards), blocking)
            self._bp.get(path, wrapped)
            return wrapped
        return decorator

    def post(self, path: str, guards: list = None, body: type = None, blocking: bool = None):
        """Register a POST route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(func, body), guards), blocking)
            self._bp.post(path, wrapped)
            return wrapped
        return decorator

    def put(self, path: str, guards: list = None, body: type = None, blocking: bool = None):
        """Register a PUT route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(func, body), guards), blocking)
            self._bp.put(path, wrapped)
            return wrapped
        return decorator

    def delete(self, path: str, guards: list = None, body: type = None, blocking: bool = None):
        """Register a DELETE route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(func, body), guards), blocking)
            self._bp.delete(path, wrapped)
            return wrapped
        return decorator

    def patch(self, path: str, guards: list = None, body: type = None, blocking: bool = None):
        """Register a PATCH route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(func, body), guards), blocking)
            self._bp.patch(path, wrapped)
            return wrapped
        return decorator

    def register(self, blueprint: "Blueprint"):
        """Register a nested blueprint."""
        self._bp.register(blueprint._bp)

    def get_all_routes(self):
        """Get all routes including from nested blueprints."""
        return self._bp.get_all_routes()


def _worker_process_entry():
    """Placeholder - Windows workers use subprocess re-execution instead.

    On Windows (spawn-based multiprocessing), PyO3 Rust objects cannot be
    pickled/serialized across process boundaries. Instead, we use subprocess
    to re-execute the user's script with CELLO_WORKER=1 env var, which forces
    single-worker mode. This ensures all routes are properly registered in
    each worker process.
    """
    pass


def _apply_guards(handler, guards):
    """Wrap a handler with guard checks if guards are provided.

    Supports both sync and async handlers. Returns the handler unchanged
    if no guards are specified.
    """
    if not guards:
        return handler

    import functools
    import inspect
    from .guards import verify_guards

    if inspect.iscoroutinefunction(handler):
        @functools.wraps(handler)
        async def async_guard_wrapper(request, *args, **kwargs):
            verify_guards(guards, request)
            return await handler(request, *args, **kwargs)
        return async_guard_wrapper
    else:
        @functools.wraps(handler)
        def guard_wrapper(request, *args, **kwargs):
            verify_guards(guards, request)
            return handler(request, *args, **kwargs)
        return guard_wrapper


class _AppState:
    """A plain attribute namespace hanging off ``app.state``.

    A place to stash resources you create yourself (e.g. an external
    ``asyncpg``/``redis.asyncio`` pool, an ``aiohttp`` session) in a startup
    hook so they survive on Cello's persistent event loop across requests::

        @app.on_event("startup")
        async def startup():
            app.state.http = SomeClient()
    """

    __slots__ = ("__dict__",)

    def __repr__(self):
        keys = ", ".join(sorted(self.__dict__)) or "empty"
        return f"<App.state {keys}>"


class App:
    """
    The main Cello application class.

    Provides a clean API for defining routes and running the server.
    All heavy lifting is done in Rust for maximum performance.

    Enterprise Features:
        - JWT, Basic, and API Key authentication
        - Rate limiting with token bucket or sliding window
        - Session management with cookies
        - Security headers (CSP, HSTS, X-Frame-Options, etc.)
        - Cluster mode for multi-process scaling
        - HTTP/2 and HTTP/3 (QUIC) protocol support
        - TLS/SSL configuration
        - Request/response timeouts and limits
    """

    def __init__(self):
        """Create a new Cello application."""
        self._app = Cello()
        self._routes = []  # Track routes for OpenAPI generation
        self._template_engine: "MiniJinjaEngine | None" = None  # v1.1.0
        self._redis = None  # Native Redis client; set by enable_redis()
        self._database = None  # Native Postgres pool; set by enable_database()
        self.state = _AppState()  # Scratch namespace for user-held resources

    def _register_route(self, method: str, path: str, func, tags: list = None, summary: str = None, description: str = None, body: type = None):
        """Internal: Register a route and track metadata for OpenAPI."""
        # Extract docstring if no description provided
        doc = func.__doc__ or ""
        route_summary = summary or doc.split('\n')[0].strip() if doc else f"{method} {path}"
        route_description = description or doc.strip() if doc else None
        
        # Store route metadata
        self._routes.append({
            "method": method,
            "path": path,
            "handler": func.__name__,
            "summary": route_summary,
            "description": route_description,
            "tags": tags or [],
            "body": body,
        })

    def get(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None, body: type = None, blocking: bool = None):
        """
        Register a GET route.

        Args:
            path: URL path pattern (e.g., "/users/{id}")
            tags: OpenAPI tags for grouping
            summary: OpenAPI summary
            description: OpenAPI description
            guards: List of guard functions/classes

        Returns:
            Decorator function for the route handler.

        Example:
            @app.get("/hello/{name}", guards=[Authenticated()])
            def hello(request):
                return {"message": f"Hello, {request.params['name']}!"}
        """
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(self._make_redis_aware(func), body), guards), blocking)
            self._app.get(path, wrapped)
            self._register_route("GET", path, func, tags, summary, description, body=body)
            return wrapped
        return decorator

    def post(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None, body: type = None, blocking: bool = None):
        """Register a POST route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(self._make_redis_aware(func), body), guards), blocking)
            self._app.post(path, wrapped)
            self._register_route("POST", path, func, tags, summary, description, body=body)
            return wrapped
        return decorator

    def put(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None, body: type = None, blocking: bool = None):
        """Register a PUT route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(self._make_redis_aware(func), body), guards), blocking)
            self._app.put(path, wrapped)
            self._register_route("PUT", path, func, tags, summary, description, body=body)
            return wrapped
        return decorator

    def delete(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None, body: type = None, blocking: bool = None):
        """Register a DELETE route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(self._make_redis_aware(func), body), guards), blocking)
            self._app.delete(path, wrapped)
            self._register_route("DELETE", path, func, tags, summary, description, body=body)
            return wrapped
        return decorator

    def patch(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None, body: type = None, blocking: bool = None):
        """Register a PATCH route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(_validate_wrap(self._make_redis_aware(func), body), guards), blocking)
            self._app.patch(path, wrapped)
            self._register_route("PATCH", path, func, tags, summary, description, body=body)
            return wrapped
        return decorator

    def options(self, path: str, guards: list = None, blocking: bool = None):
        """Register an OPTIONS route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(wrap_handler_with_validation(self._make_redis_aware(func)), guards), blocking)
            self._app.options(path, wrapped)
            return wrapped
        return decorator

    def head(self, path: str, guards: list = None, blocking: bool = None):
        """Register a HEAD route."""
        def decorator(func):
            wrapped = _mark_blocking(_apply_guards(wrap_handler_with_validation(self._make_redis_aware(func)), guards), blocking)
            self._app.head(path, wrapped)
            return wrapped
        return decorator

    def websocket(self, path: str, allowed_origins: list = None):
        """
        Register a WebSocket route.

        Args:
            path: URL path for WebSocket endpoint
            allowed_origins: Browser origins (``"https://app.example.com"``)
                allowed to open this socket besides the server's own origin.
                ``["*"]`` allows any origin. A browser handshake from any other
                origin gets ``403`` (it could otherwise ride the user's cookies).
                Clients that send no ``Origin`` (non-browser) are not affected.

        Example:
            @app.websocket("/ws")
            def websocket_handler(ws):
                while True:
                    msg = ws.recv()          # None once the client closes
                    if msg is None:
                        break
                    ws.send_text(f"Echo: {msg.content}")

            @app.websocket("/ws/async")
            async def async_handler(ws):
                while (msg := await ws.arecv()) is not None:
                    await ws.asend_text(f"Echo: {msg.content}")

        A ``def`` handler runs on the blocking thread pool (``ws.recv()`` blocks
        without holding the GIL) and occupies one pool thread while the socket
        is open. An ``async def`` handler runs on the application's event loop,
        holds no thread while it awaits, and must use the awaitable methods
        (``arecv``, ``asend``, ``asend_text``, ``asend_binary``); the blocking
        ones raise ``RuntimeError`` there. The sync middleware chain (auth, rate
        limits) runs on the upgrade request; ``ws.request`` exposes it.
        """
        origins = list(allowed_origins) if allowed_origins is not None else None

        def decorator(func):
            self._app.websocket(path, func, origins)
            return func
        return decorator

    def route(self, path: str, methods: list = None, blocking: bool = None):
        """
        Register a route that handles multiple HTTP methods.

        Args:
            path: URL path pattern
            methods: List of HTTP methods (e.g., ["GET", "POST"])
        """
        if methods is None:
            methods = ["GET"]

        def decorator(func):
            wrapped = _mark_blocking(wrap_handler_with_validation(self._make_redis_aware(func)), blocking)
            for method in methods:
                method_upper = method.upper()
                if method_upper == "GET":
                    self._app.get(path, wrapped)
                elif method_upper == "POST":
                    self._app.post(path, wrapped)
                elif method_upper == "PUT":
                    self._app.put(path, wrapped)
                elif method_upper == "DELETE":
                    self._app.delete(path, wrapped)
                elif method_upper == "PATCH":
                    self._app.patch(path, wrapped)
                elif method_upper == "OPTIONS":
                    self._app.options(path, wrapped)
                elif method_upper == "HEAD":
                    self._app.head(path, wrapped)
            return func
        return decorator

    def register_blueprint(self, blueprint: Blueprint):
        """
        Register a blueprint with the application.

        Args:
            blueprint: Blueprint instance to register
        """
        self._app.register_blueprint(blueprint._bp)

    def enable_cors(self, origins: list = None):
        """
        Enable CORS middleware.

        Args:
            origins: List of allowed origins (default: ["*"])
        """
        self._app.enable_cors(origins)

    def enable_logging(self):
        """Enable request/response logging middleware."""
        self._app.enable_logging()

    def enable_compression(self, min_size: int = None):
        """
        Enable gzip compression middleware.

        Args:
            min_size: Minimum response size to compress (default: 1024)
        """
        self._app.enable_compression(min_size)

    def enable_prometheus(self, endpoint: str = "/metrics", namespace: str = "cello", subsystem: str = "http"):
        """
        Enable Prometheus metrics middleware.

        Args:
            endpoint: URL path for metrics (default: "/metrics")
            namespace: Prometheus namespace (default: "cello")
            subsystem: Prometheus subsystem (default: "http")
        """
        self._app.enable_prometheus(endpoint, namespace, subsystem)

    def enable_rate_limit(self, config: RateLimitConfig):
        """
        Enable rate limiting middleware.

        Args:
            config: RateLimitConfig instance. Use RateLimitConfig.token_bucket(), .sliding_window() or .adaptive() to create.
        """
        self._app.enable_rate_limit(config)

    def enable_caching(
        self,
        ttl: int = 300,
        methods: list = None,
        exclude_paths: list = None,
        compress: bool = True,
        cache_authenticated: bool = False,
        max_entries: int = 10000,
    ):
        """
        Enable smart caching middleware (server-side response cache).

        Only 200/203/204 responses without ``Set-Cookie`` and without
        ``Cache-Control: private/no-store/no-cache`` are stored. The TTL is not
        advertised to clients: ``Cache-Control`` is whatever the handler sent.

        Args:
            ttl: Default TTL in seconds (default: 300)
            methods: List of HTTP methods to cache (default: ["GET", "HEAD"])
            exclude_paths: List of paths to exclude from cache
            compress: Gzip a cache HIT inline for clients that send
                ``Accept-Encoding: gzip`` (default: True). Sets
                ``Vary: Accept-Encoding``.
            cache_authenticated: Also cache requests that carry
                ``Authorization`` or ``Cookie`` (default: False). When enabled
                those headers are part of the cache key, so users never share
                an entry.
            max_entries: Maximum number of stored responses (default: 10000).
        """
        self._app.enable_caching(ttl, methods, exclude_paths, compress, cache_authenticated, max_entries)

    def enable_circuit_breaker(self, failure_threshold: int = 5, reset_timeout: int = 30, half_open_target: int = 3, failure_codes: list = None):
        """
        Enable Circuit Breaker middleware.
        
        Args:
           failure_threshold: Failures before opening circuit.
           reset_timeout: Seconds to wait before Half-Open.
           half_open_target: Successes needed to Close.
           failure_codes: List of status codes considered failures (default: [500, 502, 503, 504]).
        """
        self._app.enable_circuit_breaker(failure_threshold, reset_timeout, half_open_target, failure_codes)

    def set_limits(self, config: "LimitsConfig"):
        """Configure request and connection limits. Every field is enforced:

        - ``max_body_size`` (bytes): larger bodies get ``413 Payload Too Large``
          before they are buffered. Default cap without ``set_limits``: 100 MB.
        - ``max_header_size`` (bytes, minimum 8 KiB): larger request heads get
          ``431``.
        - ``max_connections``: connections beyond this are closed on accept.
        - ``max_requests_per_connection``: the connection is closed (``Connection:
          close``) after this many responses.

        ``0`` means unlimited for any field.

        Args:
            config: LimitsConfig instance.
        """
        self._app.set_limits(config)
        return self

    def set_timeouts(self, config: "TimeoutConfig"):
        """Configure server timeouts (all values in seconds; 0 disables a timeout).

        Uses ``read_header_timeout`` (Slowloris guard for slow header delivery;
        also bounds idle keep-alive waits), ``read_body_timeout`` (slow body
        delivery → ``408``), ``handler_timeout`` (a handler exceeding the limit →
        ``504``) and ``idle_timeout`` (keep-alive connections with no request in
        flight are closed). ``write_timeout`` is accepted but not enforced.

        Defaults without ``set_timeouts``: header read 30 s, all others off.

        Args:
            config: TimeoutConfig instance.
        """
        self._app.set_timeouts(config)
        return self

    def set_trusted_proxies(self, proxies: list, trust_x_real_ip: bool = False):
        """Trust forwarding headers only from these reverse proxies.

        ``X-Forwarded-For``, ``X-Real-IP``, ``X-Forwarded-Proto`` and
        ``X-Forwarded-Host`` are client-controlled unless they were set by a
        proxy you run. By default no proxy is trusted: ``request.client_ip()``
        and the rate limiter use the TCP peer address. Pass the proxies'
        addresses or networks to honour the headers they add::

            app.set_trusted_proxies(["10.0.0.0/8", "127.0.0.1"])

        The client is then the right-most ``X-Forwarded-For`` entry that is not
        itself a trusted proxy. The list belongs to this app (other ``App``
        instances in the process keep their own).

        ``X-Real-IP`` is a single value with no hop chain, so it is ignored
        unless ``trust_x_real_ip=True``. Set that only when every trusted proxy
        overwrites the header (never passes the client's copy through).
        """
        self._app.set_trusted_proxies(list(proxies), trust_x_real_ip)
        return self

    def set_threadpool(self, config: "ThreadPoolConfig"):
        """Configure the blocking threadpool used for blocking sync handlers.

        Sync ``def`` handlers run inline on the server thread, which is the fastest
        path for handlers that simply build and return a value. A handler that
        *blocks* — ``time.sleep``, a synchronous database driver, a synchronous HTTP
        client — would otherwise hold that thread for its full duration and stall
        every other connection. Cello times sync handlers and permanently moves any
        that exceed ``offload_threshold_ms`` onto a bounded pool of OS threads, so
        blocking calls overlap instead of serialising.

        Only handlers that release the GIL benefit; pure-Python CPU-bound work is
        still serialised by the GIL no matter which thread it runs on.

        The pool is shared with async coroutine waits and background tasks, so size
        it for all three. Must be called before ``run()``.

        Args:
            config: ThreadPoolConfig instance.
        """
        self._app.set_threadpool(config)
        return self

    def enable_jwt(self, config: "JwtConfig", skip_paths: list = None):
        """Enable JWT authentication middleware.

        Args:
            config: JwtConfig instance with secret, algorithm, etc.
            skip_paths: List of paths to exclude from JWT validation.
        """
        self._app.enable_jwt(config, skip_paths)

    def enable_session(self, config: "SessionConfig" = None):
        """Enable session middleware (in-memory store).

        Args:
            config: SessionConfig instance (optional).
        """
        self._app.enable_session(config)

    def enable_security_headers(self, strict: bool = False):
        """Enable security headers middleware (HSTS, X-Frame-Options, etc.).

        Args:
            strict: Use stricter CSP and CORS settings (default: False).
        """
        self._app.enable_security_headers(strict)

    def enable_csrf(self, cookie_name: str = None, header_name: str = None,
                    allowed_origins: list = None):
        """Enable CSRF protection middleware (double-submit cookie pattern).

        Args:
            cookie_name: Name of the CSRF cookie (default: ``"__Host-csrf"``;
                the prefix is dropped when the cookie cannot be ``Secure``).
            header_name: Request header carrying the token (default: ``"X-CSRF-Token"``).
            allowed_origins: Optional explicit allow-list of full origins
                (e.g. ``["https://app.example.com"]``). When empty, requests are
                validated as same-origin against the ``Host`` header.
        """
        self._app.enable_csrf(cookie_name, header_name, allowed_origins)

    def enable_basic_auth(self, credentials: dict, realm: str = "Restricted"):
        """Enable HTTP Basic authentication middleware.

        Args:
            credentials: Dict of {username: password} pairs.
            realm: WWW-Authenticate realm string.
        """
        self._app.enable_basic_auth(credentials, realm)

    def enable_api_key(self, keys: dict, header: str = "X-API-Key"):
        """Enable API Key authentication middleware.

        Args:
            keys: Dict of {api_key: client_name} pairs.
            header: Header name that carries the key (default: ``"X-API-Key"``).
        """
        self._app.enable_api_key(keys, header)

    def use(self, middleware):
        """Apply a middleware instance to the application.

        Accepts any middleware object from ``cello.middleware``:

        - :class:`~cello.middleware.JwtAuth` → :meth:`enable_jwt`
        - :class:`~cello.middleware.BasicAuth` → :meth:`enable_basic_auth`
        - :class:`~cello.middleware.ApiKeyAuth` → :meth:`enable_api_key`
        - :class:`~cello.middleware.CsrfConfig` → :meth:`enable_csrf`
        - :class:`~cello.middleware.SessionConfig` → :meth:`enable_session`
        - :class:`~cello.middleware.SecurityHeadersConfig` → :meth:`enable_security_headers`

        Example::

            from cello import App, JwtConfig
            from cello.middleware import JwtAuth

            app = App()
            app.use(JwtAuth(JwtConfig(secret="your-secret", algorithm="HS256")))
        """
        # Import lazily to avoid circular imports
        from cello.middleware import JwtAuth, BasicAuth, ApiKeyAuth, CsrfConfig

        if isinstance(middleware, JwtAuth):
            self.enable_jwt(middleware.config, middleware.skip_paths or None)
        elif isinstance(middleware, BasicAuth):
            self.enable_basic_auth(middleware.credentials, middleware.realm)
        elif isinstance(middleware, ApiKeyAuth):
            self.enable_api_key(middleware.keys, middleware.header)
        elif isinstance(middleware, CsrfConfig):
            self.enable_csrf(
                cookie_name=middleware.cookie_name,
                header_name=middleware.header_name,
                allowed_origins=getattr(middleware, "allowed_origins", None),
            )
        else:
            # Try SessionConfig (from cello._cello)
            try:
                from cello._cello import SessionConfig, SecurityHeadersConfig
                if isinstance(middleware, SessionConfig):
                    self.enable_session(middleware)
                    return
                if isinstance(middleware, SecurityHeadersConfig):
                    self.enable_security_headers()
                    return
            except ImportError:
                pass
            raise TypeError(
                f"Unknown middleware type: {type(middleware).__name__}. "
                "Use one of: JwtAuth, BasicAuth, ApiKeyAuth, CsrfConfig, "
                "SessionConfig, SecurityHeadersConfig."
            )

    # -------------------------------------------------------------------------
    # v1.1.0 — MiniJinja template engine
    # -------------------------------------------------------------------------

    def enable_templates(
        self,
        template_dir: str = "templates",
        auto_escape: bool = True,
        globals: dict = None,
    ) -> "MiniJinjaEngine":
        """
        Attach a MiniJinja Jinja2-compatible template engine to this application.

        After calling this method, use ``app.render()`` inside handlers to produce
        HTML (or any text) from template files.

        Args:
            template_dir: Directory that contains ``.html`` / ``.txt`` templates
                (default: ``"templates"``).
            auto_escape: Enable HTML auto-escaping for ``.html``/``.htm``/``.xml``
                files to prevent XSS (default: ``True``).
            globals: Optional dictionary of variables that are available in
                *every* template rendered by this engine (e.g. app name, version).

        Returns:
            The configured :class:`MiniJinjaEngine` instance, in case you need
            direct access.

        Raises:
            RuntimeError: If ``enable_templates()`` has already been called on
                this application instance.

        Example:
            ::

                app = App()
                app.enable_templates(
                    template_dir="templates",
                    auto_escape=True,
                    globals={"app_name": "My Site", "year": 2026},
                )

                @app.get("/")
                def home(request):
                    html = app.render("index.html", {"title": "Welcome"})
                    return Response.html(html)
        """
        if self._template_engine is not None:
            raise RuntimeError(
                "enable_templates() has already been called on this App instance. "
                "Call it once during application setup."
            )
        engine = MiniJinjaEngine(template_dir=template_dir, auto_escape=auto_escape)
        if globals:
            engine.add_globals(globals)
        self._template_engine = engine
        return engine

    def render(self, template_name: str, context: dict = None) -> str:
        """
        Render a Jinja2 template and return the result as a string.

        Must call :meth:`enable_templates` before using this method.

        Args:
            template_name: Filename of the template relative to the configured
                ``template_dir`` (e.g. ``"index.html"``).
            context: Dictionary of variables passed to the template.
                Defaults to an empty dict.

        Returns:
            Rendered template string.

        Raises:
            RuntimeError: If :meth:`enable_templates` has not been called.
            ValueError: If the template file cannot be found or contains errors.

        Example:
            ::

                @app.get("/profile/{id}")
                def profile(request):
                    user = {"name": "Alice", "id": request.params["id"]}
                    html = app.render("profile.html", {"user": user})
                    return Response.html(html)
        """
        if self._template_engine is None:
            raise RuntimeError(
                "Template engine is not configured. "
                "Call app.enable_templates() before using app.render()."
            )
        return self._template_engine.render(template_name, context or {})

    def render_string(self, source: str, context: dict = None) -> str:
        """
        Render an inline Jinja2 template string and return the result.

        Useful for dynamic templates that are not stored on disk.

        Args:
            source: Jinja2 template source string.
            context: Dictionary of variables. Defaults to an empty dict.

        Returns:
            Rendered string.

        Raises:
            RuntimeError: If :meth:`enable_templates` has not been called.

        Example:
            ::

                msg = app.render_string(
                    "Hello, {{ name }}! You have {{ count }} new messages.",
                    {"name": "Bob", "count": 3},
                )
        """
        if self._template_engine is None:
            raise RuntimeError(
                "Template engine is not configured. "
                "Call app.enable_templates() before using app.render_string()."
            )
        return self._template_engine.render_string(source, context or {})

    def on_event(self, event_type: str):
        """
        Register a lifecycle event handler.
        
        Args:
            event_type: "startup" or "shutdown"
        """
        def decorator(func):
            if event_type == "startup":
                self._app.on_startup(func)
            elif event_type == "shutdown":
                self._app.on_shutdown(func)
            else:
                raise ValueError(f"Invalid event type: {event_type}")
            return func
        return decorator

    def invalidate_cache(self, tags: list):
        """
        Invalidate cache by tags.
        
        Args:
            tags: List of tags to invalidate.
        """
        self._app.invalidate_cache(tags)

    def enable_openapi(self, title: str = "Cello API", version: str = "1.4.1"):
        """
        Enable OpenAPI documentation endpoints.

        This adds:
        - GET /docs - Swagger UI
        - GET /redoc - ReDoc documentation
        - GET /openapi.json - OpenAPI JSON schema

        Args:
            title: API title (default: "Cello API")
            version: API version (default: "1.4.1")
        """
        # Store for closure
        api_title = title
        api_version = version

        # Create handlers in Python directly
        @self.get("/docs")
        def docs_handler(request):
            html = f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{api_title} - Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
    <style>
        body {{ margin: 0; padding: 0; }}
        .swagger-ui .topbar {{ display: none; }}
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = () => {{
            window.ui = SwaggerUIBundle({{
                url: "/openapi.json",
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [SwaggerUIBundle.presets.apis, SwaggerUIStandalonePreset],
                layout: "StandaloneLayout"
            }});
        }};
    </script>
</body>
</html>'''
            return Response.html(html)

        @self.get("/redoc")
        def redoc_handler(request):
            html = f'''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{api_title} - ReDoc</title>
    <link href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700" rel="stylesheet">
    <style>body {{ margin: 0; padding: 0; }}</style>
</head>
<body>
    <redoc spec-url="/openapi.json"></redoc>
    <script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
</body>
</html>'''
            return Response.html(html)

        # Store reference to self for closure
        app_ref = self
        
        @self.get("/openapi.json")
        def openapi_handler(request):
            # Auto-generate paths from registered routes
            paths = {}
            components = {}
            owners = {}  # component name -> model it documents (dedup)
            
            for route in app_ref._routes:
                path = route["path"]
                method = route["method"].lower()
                
                # Skip internal routes
                if path in ["/docs", "/redoc", "/openapi.json"]:
                    continue
                
                # Extract path parameters
                import re
                param_pattern = re.compile(r'\{([^}]+)\}')
                params = param_pattern.findall(path)
                
                # Build operation object
                operation = {
                    "summary": route["summary"],
                    "operationId": f"{method}_{route['handler']}",
                    "responses": {
                        "200": {
                            "description": "Successful response",
                            "content": {
                                "application/json": {
                                    "schema": {"type": "object"}
                                }
                            }
                        }
                    }
                }
                
                if route["description"]:
                    operation["description"] = route["description"]
                
                if route["tags"]:
                    operation["tags"] = route["tags"]
                
                # Add path parameters
                if params:
                    operation["parameters"] = [
                        {
                            "name": p,
                            "in": "path",
                            "required": True,
                            "schema": {"type": "string"}
                        }
                        for p in params
                    ]
                
                # A `body=` model documents its schema (ORM models from the
                # compiled registry, Pydantic models from their JSON schema).
                body_schema = _body_openapi_schema(route.get("body"), components, owners)
                if body_schema is not None:
                    operation["requestBody"] = {
                        "required": True,
                        "content": {"application/json": {"schema": body_schema}},
                    }
                    operation["responses"]["400"] = {
                        "description": "Request body failed validation",
                        "content": {"application/json": {"schema": {"$ref": "#/components/schemas/ValidationErrors"}}},
                    }
                    components.setdefault("ValidationErrors", _VALIDATION_ERRORS_SCHEMA)
                # Add request body for POST/PUT/PATCH
                # Add request body for POST/PUT/PATCH
                elif method in ["post", "put", "patch"]:
                    operation["requestBody"] = {
                        "content": {
                            "application/json": {
                                "schema": {"type": "object"}
                            }
                        }
                    }
                
                # Add to paths
                if path not in paths:
                    paths[path] = {}
                paths[path][method] = operation
            
            return {
                "openapi": "3.0.3",
                "info": {
                    "title": api_title,
                    "version": api_version,
                    "description": f"{api_title} - Powered by Cello Framework"
                },
                "paths": paths,
                **({"components": {"schemas": components}} if components else {}),
            }

        print("📚 OpenAPI docs enabled:")
        print("   Swagger UI: /docs")
        print("   ReDoc:      /redoc")
        print("   OpenAPI:    /openapi.json")

    # ========================================================================
    # Enterprise Features (v0.7.0+)
    # ========================================================================

    # ========================================================================
    # Data Layer Features (v0.8.0+)
    # ========================================================================

    def enable_database(self, config: "DatabaseConfig" = None):
        """
        Enable database connection pooling.

        Configures an async connection pool for PostgreSQL (native, deadpool-backed).
        Supports connection health monitoring, automatic reconnection, and
        query statistics.

        Args:
            config: DatabaseConfig instance

        Example:
            from cello import App, DatabaseConfig

            app = App()
            app.enable_database(DatabaseConfig(
                url="postgresql://user:pass@localhost/mydb",
                pool_size=20,
                max_lifetime_secs=1800
            ))

        TLS follows libpq's ``sslmode`` (URL query key or ``DatabaseConfig``
        option): ``disable``, ``prefer`` (default), ``require``, ``verify-ca``,
        ``verify-full`` (recommended for production), plus ``sslrootcert``,
        ``sslcert`` and ``sslkey``. Bad certificate paths raise ``ValueError``
        here. See docs/data-layer.md#tls-encrypted-connections.
        """
        if config is None:
            raise ValueError(
                "enable_database() requires a DatabaseConfig with a Postgres URL, e.g. "
                'DatabaseConfig(url="postgresql://user:pass@localhost:5432/mydb", pool_size=10)'
            )
        self._app.enable_database(config)
        # Expose the live native pool as app.database / request.database.
        self._database = self._app.database

    def enable_redis(self, config: "RedisConfig" = None):
        """
        Enable Redis connection pooling.

        Configures an async Redis client with connection pooling,
        supporting standard and cluster modes. After calling this,
        handlers can access the client via ``request.redis``.

        Args:
            config: RedisConfig instance

        Example:
            from cello import App, RedisConfig

            app = App()
            app.enable_redis(RedisConfig(
                url="redis://localhost:6379",
                pool_size=10,
                cluster_mode=False
            ))
        """
        if config is None:
            config = RedisConfig()
        self._app.enable_redis(config)
        # Expose the live native client as app.redis / request.redis.
        self._redis = self._app.redis

    @property
    def database(self):
        """The native Postgres pool (or None). Available after enable_database().

        Use inside ``on_event("startup")`` to create tables or warm caches::

            @app.on_event("startup")
            async def startup():
                await app.database.execute("CREATE TABLE IF NOT EXISTS ...")
        """
        return self._database

    @property
    def redis(self):
        """The native Redis client (or None). Available after enable_redis()."""
        return self._redis

    def _make_redis_aware(self, func):
        """Wrap a handler so the native Redis client and Database pool are
        injected onto the request (``request.redis`` / ``request.database``)
        before dispatch."""
        import inspect
        from functools import wraps
        app = self

        def _inject(request):
            if app._redis is not None:
                request._inject_redis(app._redis)
            if app._database is not None:
                request._inject_database(app._database)

        if inspect.iscoroutinefunction(func):
            @wraps(func)
            async def async_wrapper(request, *args, **kwargs):
                _inject(request)
                return await func(request, *args, **kwargs)
            return async_wrapper
        else:
            @wraps(func)
            def sync_wrapper(request, *args, **kwargs):
                _inject(request)
                return func(request, *args, **kwargs)
            return sync_wrapper

    # ========================================================================
    # End Data Layer Features
    # ========================================================================

    # ========================================================================
    # API Protocol Features (v0.9.0+)
    # ========================================================================

    def enable_grpc(self, config: "GrpcConfig" = None):
        """
        Record gRPC configuration (config only).

        NOTE: this does not start a gRPC server. The gRPC runtime is provided by
        the ``cello.grpc`` module; this call validates and records config.

        Args:
            config: GrpcConfig instance

        Example:
            from cello import App, GrpcConfig

            app = App()
            app.enable_grpc(GrpcConfig(
                address="[::]:50051",
                reflection=True,
                enable_web=True
            ))
        """
        if config is None:
            config = GrpcConfig()
        self._app.enable_grpc(config)

    def add_grpc_service(self, name: str, methods: list = None):
        """
        Record a gRPC service name (config only; runtime lives in ``cello.grpc``).

        Args:
            name: Service name
            methods: Optional list of method names

        Example:
            app.add_grpc_service("UserService", ["GetUser", "ListUsers"])
        """
        self._app.add_grpc_service(name, methods)

    def enable_messaging(self, config: "KafkaConfig" = None):
        """
        Record Kafka configuration (config only).

        NOTE: producing/consuming is provided by the ``cello.messaging`` module;
        this call records config and does not start a broker client.

        Args:
            config: KafkaConfig instance

        Example:
            from cello import App, KafkaConfig

            app = App()
            app.enable_messaging(KafkaConfig(
                brokers=["localhost:9092"],
                group_id="my-group"
            ))
        """
        if config is None:
            config = KafkaConfig()
        self._app.enable_messaging(config)

    def enable_rabbitmq(self, config: "RabbitMQConfig" = None):
        """
        Record RabbitMQ configuration (config only; use ``cello.messaging`` for the runtime).

        Args:
            config: RabbitMQConfig instance

        Example:
            from cello import App, RabbitMQConfig

            app = App()
            app.enable_rabbitmq(RabbitMQConfig(
                url="amqp://localhost",
                prefetch_count=20
            ))
        """
        if config is None:
            config = RabbitMQConfig()
        self._app.enable_rabbitmq(config)

    def enable_sqs(self, config: "SqsConfig" = None):
        """
        Record SQS configuration (config only; use ``cello.messaging`` for the runtime).

        Args:
            config: SqsConfig instance

        Example:
            from cello import App, SqsConfig

            app = App()
            app.enable_sqs(SqsConfig(
                region="us-west-2",
                queue_url="https://sqs.us-west-2.amazonaws.com/123/queue"
            ))
        """
        if config is None:
            config = SqsConfig()
        self._app.enable_sqs(config)

    # ========================================================================
    # End API Protocol Features
    # ========================================================================

    # ========================================================================
    # Advanced Pattern Features (v0.10.0+)
    # ========================================================================

    def enable_event_sourcing(self, config=None):
        """
        Record event-sourcing configuration (config only).

        NOTE: event stores, aggregates, and snapshots are provided by the
        ``cello.eventsourcing`` module; this call records config.

        Args:
            config: EventSourcingConfig instance or None for defaults.

        Returns:
            The App instance for method chaining.

        Example:
            from cello import App, EventSourcingConfig

            app = App()
            app.enable_event_sourcing(EventSourcingConfig(
                store_type="postgresql",
                snapshot_interval=100,
                enable_snapshots=True,
            ))
        """
        if config is None:
            config = EventSourcingConfig()
        self._app.enable_event_sourcing(config)
        return self

    def enable_cqrs(self, config=None):
        """
        Record CQRS configuration (config only).

        NOTE: the runtime is provided by the ``cello.cqrs`` module; this call records config.

        Configures the CQRS subsystem with event synchronization,
        command/query timeouts, and retry settings.

        Args:
            config: CqrsConfig instance or None for defaults.

        Returns:
            The App instance for method chaining.

        Example:
            from cello import App, CqrsConfig

            app = App()
            app.enable_cqrs(CqrsConfig(
                enable_event_sync=True,
                command_timeout_ms=10000,
            ))
        """
        if config is None:
            config = CqrsConfig()
        self._app.enable_cqrs(config)
        return self

    def enable_saga(self, config=None):
        """
        Record SAGA configuration (config only).

        NOTE: the runtime is provided by the ``cello.saga`` module; this call records config.

        Configures the saga orchestration subsystem with retry behaviour,
        timeouts, and logging settings.

        Args:
            config: SagaConfig instance or None for defaults.

        Returns:
            The App instance for method chaining.

        Example:
            from cello import App, SagaConfig

            app = App()
            app.enable_saga(SagaConfig(
                max_retries=5,
                timeout_ms=60000,
            ))
        """
        if config is None:
            config = SagaConfig()
        self._app.enable_saga(config)
        return self

    # ========================================================================
    # End Advanced Pattern Features
    # ========================================================================

    def enable_telemetry(self, config: "OpenTelemetryConfig" = None):
        """
        Enable OpenTelemetry distributed tracing and metrics.

        Args:
            config: OpenTelemetryConfig instance

        Example:
            from cello import App, OpenTelemetryConfig

            app = App()
            app.enable_telemetry(OpenTelemetryConfig(
                service_name="my-service",
                otlp_endpoint="http://collector:4317",
                sampling_rate=0.1
            ))
        """
        if config is None:
            config = OpenTelemetryConfig("cello-service")
        self._app.enable_telemetry(config)

    def enable_health_checks(self, config: "HealthCheckConfig" = None):
        """
        Enable Kubernetes-compatible health check endpoints.

        Adds the following endpoints:
        - GET /health/live - Liveness probe
        - GET /health/ready - Readiness probe
        - GET /health/startup - Startup probe
        - GET /health - Full health report

        Args:
            config: HealthCheckConfig instance

        Example:
            from cello import App, HealthCheckConfig

            app = App()
            app.enable_health_checks(HealthCheckConfig(
                base_path="/health",
                include_system_info=True
            ))
        """
        self._app.enable_health_checks(config)

    def enable_graphql(self, config: "GraphQLConfig" = None, schema=None):
        """
        Enable a GraphQL endpoint.

        Queries are parsed with a real GraphQL parser; ``max_depth``,
        ``max_complexity`` and ``introspection`` from the config are enforced
        before any resolver runs, and the operation type comes from the
        document (a named or commented mutation is a mutation). Playground and
        introspection are off unless the config enables them. The endpoint
        runs behind the normal middleware chain, so JWT/Basic/API key auth and
        rate limiting apply to it. Only ``POST`` executes operations.

        Args:
            config: GraphQLConfig instance
            schema: The resolvers to serve: a :class:`cello.graphql.Schema` or
                :class:`cello.graphql.GraphQL`. The config's limits apply.
                Without it the endpoint resolves only resolvers registered from
                Rust, which a Python app cannot add.

        Example:
            from cello import App, GraphQLConfig
            from cello.graphql import Schema, Query

            @Query
            def users(info) -> list:
                return [{"id": 1, "name": "Ada"}]

            app = App()
            app.enable_graphql(GraphQLConfig(path="/graphql"), schema=Schema().query(users))
        """
        if config is None:
            config = GraphQLConfig()
        if schema is None:
            self._app.enable_graphql(config)
            return

        from cello.graphql import GraphQL as _GraphQL, Schema as _Schema

        limits = {
            "max_depth": config.max_depth,
            "max_complexity": config.max_complexity,
            "introspection": config.introspection,
        }
        if isinstance(schema, _Schema):
            engine = schema.build(**limits)
        elif isinstance(schema, _GraphQL):
            engine = schema
            for name, value in limits.items():
                setattr(engine, name, value)
        else:
            raise TypeError("schema must be a cello.graphql.Schema or cello.graphql.GraphQL")

        async def graphql_endpoint(request):
            try:
                body = request.json()
            except Exception:
                body = None
            if not isinstance(body, dict) or not isinstance(body.get("query"), str):
                return Response.json(
                    {"errors": [{"message": 'Expected a JSON body with a "query" string'}]},
                    status=400,
                )
            variables = body.get("variables")
            if variables is not None and not isinstance(variables, dict):
                return Response.json(
                    {"errors": [{"message": '"variables" must be an object'}]}, status=400
                )
            result = await engine.execute(
                body["query"], variables, body.get("operationName"), context=request
            )
            # The document was rejected before execution.
            return Response.json(result, status=400 if result.get("data") is None else 200)

        self.post(config.path)(graphql_endpoint)

        if config.playground:
            page = _GRAPHQL_PLAYGROUND.replace("__ENDPOINT__", json.dumps(config.path))

            def graphql_playground(request):
                return Response.html(page)

            self.get(config.path)(graphql_playground)

    # ========================================================================
    # End Enterprise Features
    # ========================================================================

    def add_guard(self, guard):
        """
        Add a security guard to the application.

        Args:
            guard: A guard object or function.
        """
        self._app.add_guard(guard)

    def register_singleton(self, name: str, value):
        """
        Register a singleton dependency.

        Args:
            name: Dependency name
            value: The singleton value
        """
        self._app.register_singleton(name, value)

    def run(self, host: str = "127.0.0.1", port: int = 8000,
            debug: bool = None, env: str = None,
            workers: int = None, reload: bool = False,
            logs: bool = None, allow_per_worker_state: bool = False):
        """
        Start the HTTP server.

        Args:
            host: Host address to bind to (default: "127.0.0.1")
            port: Port to bind to (default: 8000)
            debug: Enable debug mode (default: True in dev, False in prod). With debug
                off, unhandled handler errors return a generic 500 and the details
                are written to the server log instead of the response.
            env: Environment "development" or "production" (default: "development")
            workers: Number of worker threads (default: CPU count)
            reload: Enable hot reload (default: False)
            logs: Enable logging (default: True in dev)
            allow_per_worker_state: Run several workers even though sessions or
                rate limiting keep their state in process memory (each worker
                then has its own sessions and its own counters). Without it,
                an explicit ``workers > 1`` with either enabled raises
                ``RuntimeError``, and a defaulted worker count falls back to 1.
                Also settable with ``CELLO_ALLOW_PER_WORKER_STATE=1``.

        Example:
            # Simple development server
            app.run()

            # Production configuration
            app.run(
                host="0.0.0.0",
                port=8080,
                env="production",
                workers=4,
            )
        """
        import sys
        import os
        import argparse
        import subprocess
        import time

        # Windows multi-process workers re-execute the user's script with
        # CELLO_WORKER=1. The parent resolved host/port/debug/logs (CLI included)
        # and passes them down, so a worker never falls back to defaults such as
        # exposing error details.
        if os.environ.get("CELLO_WORKER") == "1":
            os.environ.pop("CELLO_WORKER", None)  # Prevent grandchild workers
            host = os.environ.get("CELLO_HOST", host)
            port = int(os.environ.get("CELLO_PORT", port))
            self._app.set_expose_errors(os.environ.get("CELLO_EXPOSE_ERRORS") == "1")
            if os.environ.get("CELLO_LOGS") == "1":
                self.enable_logging()
            try:
                self._app.run(host, port, None)
            except (KeyboardInterrupt, SystemExit):
                pass
            return

        env_explicit = env is not None
        debug_flag = False

        # Parse CLI arguments (only if running as main script)
        if "unittest" not in sys.modules:
            parser = argparse.ArgumentParser(description="Cello Web Server", add_help=False)
            parser.add_argument("--host", default=host)
            parser.add_argument("--port", type=int, default=port)
            parser.add_argument("--env", default=None)
            parser.add_argument("--debug", action="store_true")
            parser.add_argument("--reload", action="store_true")
            parser.add_argument("--workers", type=int, default=workers,
                                help="Number of worker processes (default: CPU count)")
            parser.add_argument("--no-logs", action="store_true")

            # Use parse_known_args to avoid conflicts
            args, _ = parser.parse_known_args()

            # Update configuration from CLI
            host = args.host
            port = args.port
            if env is None and args.env is not None:
                env = args.env
                env_explicit = True
            if workers is None: workers = args.workers
            if reload is False and args.reload: reload = True
            debug_flag = args.debug

            # Logs logic: CLI --no-logs disables it
            if logs is None and args.no_logs:
                logs = False

        if env is None and os.environ.get("CELLO_ENV"):
            env = os.environ["CELLO_ENV"]
            env_explicit = True
        if env is None:
            env = "development"

        # Debug (error details in 500 responses) is on for an explicitly chosen
        # development environment, --debug, or an interactive terminal. A process
        # without a TTY (container, service manager, CI) that never said
        # "development" gets production-safe errors.
        if debug is None:
            try:
                interactive = sys.stdout.isatty()
            except (AttributeError, ValueError):
                interactive = False
            debug = debug_flag or (env == "development" and (env_explicit or interactive))
        if logs is None:
            logs = env == "development"

        # Reloading Logic (Development only)
        if reload and os.environ.get("CELLO_RUN_MAIN") != "true":
            print(f"🔄 Hot reload enabled ({env})")
            print(f"   Watching {os.getcwd()}")

            # Simple polling reloader
            while True:
                p = subprocess.Popen(
                    [sys.executable] + sys.argv,
                    env={**os.environ, "CELLO_RUN_MAIN": "true"}
                )
                try:
                    # Wait for process or file change
                    self._watch_files(p)
                except KeyboardInterrupt:
                    p.terminate()
                    sys.exit(0)

                print("🔄 Reloading...")
                p.terminate()
                p.wait()
                time.sleep(0.5)

        # Unhandled handler errors are described to the client only in debug mode;
        # in production they can carry database messages and row values, so the
        # client gets a generic 500 and the details go to the server log.
        self._app.set_expose_errors(bool(debug))

        # Configure App
        if logs:
            self.enable_logging()

        # Determine worker count (default: all CPU cores)
        workers_explicit = workers is not None
        if workers is None:
            # Single worker in test/debug mode, multi-worker in production
            if ("unittest" in sys.modules or "pytest" in sys.modules or debug
                    or env == "development"):
                workers = 1
            else:
                workers = os.cpu_count() or 1

        if workers > 1:
            workers = self._check_per_worker_state(
                workers, workers_explicit,
                allow_per_worker_state
                or os.environ.get("CELLO_ALLOW_PER_WORKER_STATE") == "1",
            )

        # Print startup banner (skip in test environments)
        if "unittest" not in sys.modules and "pytest" not in sys.modules:
            self._print_banner(host, port, workers, env)

        # Run Server
        if workers > 1:
            self._run_multiprocess(host, port, workers, env, bool(debug), bool(logs))
        else:
            try:
                self._app.run(host, port, None)
            except KeyboardInterrupt:
                pass  # Handled by Rust ctrl_c

    def _check_per_worker_state(self, workers: int, explicit: bool, allowed: bool) -> int:
        """Workers are separate processes: in-memory sessions and rate limits
        are per worker, so a login made on one worker is unknown to the others
        and a limit of N becomes about N x workers. Refuse that unless the app
        opted in; a defaulted worker count falls back to one worker."""
        import warnings

        state = set(self._app.per_process_state())
        if "cache" in state:
            warnings.warn(
                f"response cache is per worker ({workers} workers): each worker "
                "caches separately and invalidation reaches only one of them",
                RuntimeWarning, stacklevel=3,
            )
        blocking = sorted(state & {"session", "rate_limit"})
        if not blocking or allowed:
            return workers
        what = " and ".join("sessions" if b == "session" else "rate limiting" for b in blocking)
        message = (
            f"{what} keep state in process memory, but workers={workers} runs "
            f"{workers} separate processes: sessions created on one worker are "
            "missing on the others and rate limits apply per worker. Run one "
            "worker, or pass allow_per_worker_state=True (or set "
            "CELLO_ALLOW_PER_WORKER_STATE=1) to accept per-worker state."
        )
        if explicit:
            raise RuntimeError(message)
        warnings.warn(message + " Falling back to 1 worker.", RuntimeWarning, stacklevel=3)
        return 1

    @staticmethod
    def _print_banner(host: str, port: int, workers: int, env: str):
        """Print the Cello startup banner with ASCII art logo."""
        v = __version__
        url = f"http://{host}:{port}"
        banner = f"""
\033[38;5;208m     ██████╗███████╗██╗     ██╗      ██████╗\033[0m
\033[38;5;208m    ██╔════╝██╔════╝██║     ██║     ██╔═══██╗\033[0m
\033[38;5;214m    ██║     █████╗  ██║     ██║     ██║   ██║\033[0m
\033[38;5;214m    ██║     ██╔══╝  ██║     ██║     ██║   ██║\033[0m
\033[38;5;220m    ╚██████╗███████╗███████╗███████╗╚██████╔╝\033[0m
\033[38;5;220m     ╚═════╝╚══════╝╚══════╝╚══════╝ ╚═════╝\033[0m

    \033[1mv{v}\033[0m  \033[2m|\033[0m  Rust-powered Python Web Framework

    \033[32m➜\033[0m  \033[1mServer:\033[0m    {url}
    \033[32m➜\033[0m  \033[1mWorkers:\033[0m   {workers}
    \033[32m➜\033[0m  \033[1mEnvironment:\033[0m {env}

    \033[2mPress CTRL+C to stop\033[0m
"""
        print(banner)

    def _run_multiprocess(self, host: str, port: int, workers: int, env: str,
                          debug: bool = False, logs: bool = False):
        """Run server with multiple worker processes for maximum throughput.

        Cross-platform multi-process spawning:
          - Unix/macOS: ``os.fork()``; SO_REUSEPORT lets the kernel distribute
            connections. Each child resets the database pool / Redis connection
            it inherited, so no TCP session is shared with the parent.
          - Windows: subprocess re-execution of the script (``CELLO_WORKER=1``).

        Architecture:
            Parent process: supervisor only (does not serve)
            N child processes: each runs as an independent worker
            Total: exactly N serving processes
        """
        import sys

        if sys.platform == "win32":
            self._run_multiprocess_spawn(host, port, workers, debug, logs)
        else:
            self._run_multiprocess_fork(host, port, workers)

    def _run_multiprocess_fork(self, host: str, port: int, workers: int):
        """Unix/macOS: fork-based multi-process (best performance)."""
        import os
        import signal
        import sys

        print(f"    \033[32m➜\033[0m  \033[1mMode:\033[0m      SO_REUSEPORT (kernel load balancing)")

        child_pids = []

        for i in range(workers):
            pid = os.fork()
            if pid == 0:
                # Child process: own pools, then serve until told to stop.
                code = 0
                try:
                    signal.signal(signal.SIGINT, signal.SIG_DFL)
                    signal.signal(signal.SIGTERM, signal.SIG_DFL)
                    self._app._reset_after_fork()
                    self._app.run(host, port, None)
                except (KeyboardInterrupt, SystemExit):
                    pass
                except Exception as exc:
                    print(f"[cello] worker {os.getpid()} failed: {exc}", file=sys.stderr)
                    code = 1
                finally:
                    os._exit(code)
            else:
                child_pids.append(pid)

        stopping = False

        def _stop(signum=None, frame=None):
            nonlocal stopping
            stopping = True
            for pid in child_pids:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass

        signal.signal(signal.SIGINT, _stop)
        signal.signal(signal.SIGTERM, _stop)

        # Supervise: wait for every worker to exit.
        remaining = set(child_pids)
        while remaining:
            try:
                pid, _status = os.wait()
            except ChildProcessError:
                break
            except InterruptedError:
                continue
            remaining.discard(pid)
            if not stopping and remaining:
                print(f"[cello] worker {pid} exited", file=sys.stderr)

    def _run_multiprocess_spawn(self, host: str, port: int, workers: int,
                                debug: bool = False, logs: bool = False):
        """Windows/cross-platform: subprocess-based multi-process.

        On Windows, multiprocessing uses 'spawn' (not fork), which requires all
        arguments to be picklable. PyO3 Rust objects (like Cello) cannot be pickled.

        Instead, we spawn subprocesses that re-execute the user's script with
        CELLO_WORKER=1 env var set. This forces each subprocess into single-worker
        mode, ensuring all routes are properly registered in each worker process.
        The resolved host, port, debug and logging settings are passed down.
        """
        import subprocess
        import signal
        import os
        import sys

        print(f"    \033[32m➜\033[0m  \033[1mMode:\033[0m      Multi-process (subprocess re-execution)")

        children: list[subprocess.Popen] = []

        worker_env = {
            **os.environ,
            "CELLO_WORKER": "1",
            "CELLO_HOST": str(host),
            "CELLO_PORT": str(port),
            "CELLO_EXPOSE_ERRORS": "1" if debug else "0",
            "CELLO_LOGS": "1" if logs else "0",
        }

        for i in range(workers):
            p = subprocess.Popen(
                [sys.executable] + sys.argv,
                env=worker_env,
            )
            children.append(p)

        def _cleanup(signum=None, frame=None):
            for p in children:
                try:
                    p.terminate()
                except OSError:
                    pass

        # SIGINT (Ctrl+C) works on all platforms
        signal.signal(signal.SIGINT, lambda s, f: (_cleanup(), sys.exit(0)))

        # SIGTERM is available on Unix; on Windows it may not be delivered
        try:
            signal.signal(signal.SIGTERM, lambda s, f: (_cleanup(), sys.exit(0)))
        except (OSError, ValueError):
            pass  # SIGTERM not supported on this platform

        # Supervise only: the parent does not serve.
        try:
            for p in children:
                while True:
                    try:
                        p.wait(timeout=1)
                        break
                    except subprocess.TimeoutExpired:
                        continue
        except KeyboardInterrupt:
            pass
        finally:
            _cleanup()
            for p in children:
                try:
                    p.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    p.kill()

    def _watch_files(self, process):
        import os
        import time

        mtimes = {}

        def get_mtimes():
            changes = False
            for root, dirs, files in os.walk(os.getcwd()):
                # Prune in place: hidden dirs (.git, .venv, .tox, …), caches and
                # build/dependency trees can hold tens of thousands of files.
                dirs[:] = [
                    d for d in dirs
                    if not d.startswith(".") and d not in _RELOAD_SKIP_DIRS
                ]

                for file in files:
                    if file.endswith(".py"):
                        path = os.path.join(root, file)
                        try:
                            mtime = os.stat(path).st_mtime
                            if path not in mtimes:
                                mtimes[path] = mtime
                            elif mtimes[path] != mtime:
                                mtimes[path] = mtime
                                return True
                        except OSError:
                            pass
            return False

        # Initial scan
        get_mtimes()

        while process.poll() is None:
            if get_mtimes():
                return
            time.sleep(1)


# Directories the hot-reload watcher never descends into.
_RELOAD_SKIP_DIRS = frozenset({
    "__pycache__", "target", "node_modules", "venv", "env", "build", "dist",
    "site-packages", "htmlcov",
})


class Depends:
    """
    Dependency injection marker for handler arguments.

    Example:
        @app.get("/users")
        def get_users(db=Depends("database")):
            return db.query("SELECT * FROM users")
    """

    def __init__(self, dependency: str):
        self.dependency = dependency


def cache(ttl: int = None, tags: list = None):
    """
    Decorator to cache response (Smart Caching).

    Supports both sync and async handlers.

    Args:
        ttl: Time to live in seconds (overrides default).
        tags: List of tags for invalidation.
    """
    import inspect
    from functools import wraps
    from cello._cello import Response

    def _set_cache_headers(response):
        """Apply cache headers to a Response object."""
        if not isinstance(response, Response):
            if isinstance(response, dict):
                response = Response.json(response)
            elif isinstance(response, str):
                response = Response.text(response)
            elif isinstance(response, bytes):
                response = Response.binary(response)

        if isinstance(response, Response):
            if ttl is not None:
                response.set_header("X-Cache-TTL", str(ttl))
            if tags:
                if isinstance(tags, list):
                    response.set_header("X-Cache-Tags", ",".join(tags))
                elif isinstance(tags, str):
                    response.set_header("X-Cache-Tags", tags)

        return response

    def decorator(func):
        if inspect.iscoroutinefunction(func):
            @wraps(func)
            async def async_wrapper(*args, **kwargs):
                response = await func(*args, **kwargs)
                return _set_cache_headers(response)
            return async_wrapper
        else:
            @wraps(func)
            def wrapper(*args, **kwargs):
                response = func(*args, **kwargs)
                return _set_cache_headers(response)
            return wrapper
    return decorator


_GRAPHQL_PLAYGROUND = """<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>GraphQL Playground</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/graphql-playground-react@1.7.28/build/static/css/index.css">
<script src="https://cdn.jsdelivr.net/npm/graphql-playground-react@1.7.28/build/static/js/middleware.js"></script>
</head>
<body>
<div id="root"></div>
<script>
window.addEventListener("load", function () {
  GraphQLPlayground.init(document.getElementById("root"), {
    endpoint: __ENDPOINT__,
    settings: {"request.credentials": "same-origin"}
  });
});
</script>
</body>
</html>
"""
