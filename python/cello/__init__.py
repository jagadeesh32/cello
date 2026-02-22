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

from .validation import wrap_handler_with_validation
from .database import transactional, Database, Redis, Transaction
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
    # v0.7.0 - Enterprise features
    "OpenTelemetryConfig",
    "HealthCheckConfig",
    "DatabaseConfig",
    "GraphQLConfig",
    # v0.8.0 - Data Layer features
    "RedisConfig",
    "transactional",
    # v0.9.0 - API Protocol features
    "GrpcConfig",
    "KafkaConfig",
    "RabbitMQConfig",
    "SqsConfig",
    # v0.10.0 - Advanced Pattern features
    "EventSourcingConfig",
    "CqrsConfig",
    "SagaConfig",
    # Config validators
    "validate_jwt_config",
    "validate_session_config",
    "validate_rate_limit_config",
    "validate_tls_config",
]
__version__ = "1.0.0"


class Blueprint:
    """
    Blueprint for grouping routes with a common prefix.

    Provides Flask-like decorator syntax for route registration.
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

    def get(self, path: str):
        """Register a GET route."""
        def decorator(func):
            self._bp.get(path, func)
            return func
        return decorator

    def post(self, path: str):
        """Register a POST route."""
        def decorator(func):
            self._bp.post(path, func)
            return func
        return decorator

    def put(self, path: str):
        """Register a PUT route."""
        def decorator(func):
            self._bp.put(path, func)
            return func
        return decorator

    def delete(self, path: str):
        """Register a DELETE route."""
        def decorator(func):
            self._bp.delete(path, func)
            return func
        return decorator

    def patch(self, path: str):
        """Register a PATCH route."""
        def decorator(func):
            self._bp.patch(path, func)
            return func
        return decorator

    def register(self, blueprint: "Blueprint"):
        """Register a nested blueprint."""
        self._bp.register(blueprint._bp)

    def get_all_routes(self):
        """Get all routes including from nested blueprints."""
        return self._bp.get_all_routes()


class App:
    """
    The main Cello application class.

    Provides a Flask-like API for defining routes and running the server.
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

    def _register_route(self, method: str, path: str, func, tags: list = None, summary: str = None, description: str = None):
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
            "tags": tags or []
        })

    def get(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None):
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
            wrapped = wrap_handler_with_validation(func)
            
            if guards:
                from .guards import verify_guards
                original_handler = wrapped
                
                # We need to wrap again to check guards
                # Note: Rust calls the handler with (request, ...) so signature is preserved?
                # wrap_handler_with_validation preserves signature mostly but handles args.
                # Here we just need to intercept.
                
                def guard_wrapper(request, *args, **kwargs):
                    verify_guards(guards, request)
                    return original_handler(request, *args, **kwargs)
                
                # Copy metadata
                import functools
                functools.update_wrapper(guard_wrapper, original_handler)
                wrapped = guard_wrapper

            self._app.get(path, wrapped)
            self._register_route("GET", path, func, tags, summary, description)
            return wrapped
        return decorator

    def post(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None):
        """Register a POST route."""
        def decorator(func):
            wrapped = wrap_handler_with_validation(func)
            if guards:
                from .guards import verify_guards
                original_handler = wrapped
                def guard_wrapper(request, *args, **kwargs):
                    verify_guards(guards, request)
                    return original_handler(request, *args, **kwargs)
                import functools
                functools.update_wrapper(guard_wrapper, original_handler)
                wrapped = guard_wrapper

            self._app.post(path, wrapped)
            self._register_route("POST", path, func, tags, summary, description)
            return wrapped
        return decorator

    def put(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None):
        """Register a PUT route."""
        def decorator(func):
            wrapped = wrap_handler_with_validation(func)
            if guards:
                from .guards import verify_guards
                original_handler = wrapped
                def guard_wrapper(request, *args, **kwargs):
                    verify_guards(guards, request)
                    return original_handler(request, *args, **kwargs)
                import functools
                functools.update_wrapper(guard_wrapper, original_handler)
                wrapped = guard_wrapper

            self._app.put(path, wrapped)
            self._register_route("PUT", path, func, tags, summary, description)
            return wrapped
        return decorator

    def delete(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None):
        """Register a DELETE route."""
        def decorator(func):
            wrapped = wrap_handler_with_validation(func)
            if guards:
                from .guards import verify_guards
                original_handler = wrapped
                def guard_wrapper(request, *args, **kwargs):
                    verify_guards(guards, request)
                    return original_handler(request, *args, **kwargs)
                import functools
                functools.update_wrapper(guard_wrapper, original_handler)
                wrapped = guard_wrapper

            self._app.delete(path, wrapped)
            self._register_route("DELETE", path, func, tags, summary, description)
            return wrapped
        return decorator

    def patch(self, path: str, tags: list = None, summary: str = None, description: str = None, guards: list = None):
        """Register a PATCH route."""
        def decorator(func):
            wrapped = wrap_handler_with_validation(func)
            if guards:
                from .guards import verify_guards
                original_handler = wrapped
                def guard_wrapper(request, *args, **kwargs):
                    verify_guards(guards, request)
                    return original_handler(request, *args, **kwargs)
                import functools
                functools.update_wrapper(guard_wrapper, original_handler)
                wrapped = guard_wrapper

            self._app.patch(path, wrapped)
            self._register_route("PATCH", path, func, tags, summary, description)
            return wrapped
        return decorator

    def options(self, path: str, guards: list = None):
        """Register an OPTIONS route."""
        def decorator(func):
            wrapped = func
            if guards:
                 from .guards import verify_guards
                 original_handler = wrapped
                 def guard_wrapper(request, *args, **kwargs):
                     verify_guards(guards, request)
                     return original_handler(request, *args, **kwargs)
                 import functools
                 functools.update_wrapper(guard_wrapper, original_handler)
                 wrapped = guard_wrapper
                 
            self._app.options(path, wrapped)
            return wrapped
        return decorator

    def head(self, path: str, guards: list = None):
        """Register a HEAD route."""
        def decorator(func):
            wrapped = func
            if guards:
                 from .guards import verify_guards
                 original_handler = wrapped
                 def guard_wrapper(request, *args, **kwargs):
                     verify_guards(guards, request)
                     return original_handler(request, *args, **kwargs)
                 import functools
                 functools.update_wrapper(guard_wrapper, original_handler)
                 wrapped = guard_wrapper
                 
            self._app.head(path, wrapped)
            return wrapped
        return decorator

    def websocket(self, path: str):
        """
        Register a WebSocket route.

        Args:
            path: URL path for WebSocket endpoint

        Example:
            @app.websocket("/ws")
            def websocket_handler(ws):
                while True:
                    msg = ws.recv()
                    if msg is None:
                        break
                    ws.send_text(f"Echo: {msg.text}")
        """
        def decorator(func):
            self._app.websocket(path, func)
            return func
        return decorator

    def route(self, path: str, methods: list = None):
        """
        Register a route that handles multiple HTTP methods.

        Args:
            path: URL path pattern
            methods: List of HTTP methods (e.g., ["GET", "POST"])
        """
        if methods is None:
            methods = ["GET"]

        def decorator(func):
            wrapped = wrap_handler_with_validation(func)
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
                    self._app.options(path, func)
                elif method_upper == "HEAD":
                    self._app.head(path, func)
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

    def enable_caching(self, ttl: int = 300, methods: list = None, exclude_paths: list = None):
        """
        Enable smart caching middleware.

        Args:
            ttl: Default TTL in seconds (default: 300)
            methods: List of HTTP methods to cache (default: ["GET", "HEAD"])
            exclude_paths: List of paths to exclude from cache
        """
        self._app.enable_caching(ttl, methods, exclude_paths)

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

    def enable_openapi(self, title: str = "Cello API", version: str = "1.0.0"):
        """
        Enable OpenAPI documentation endpoints.

        This adds:
        - GET /docs - Swagger UI
        - GET /redoc - ReDoc documentation
        - GET /openapi.json - OpenAPI JSON schema

        Args:
            title: API title (default: "Cello API")
            version: API version (default: "1.0.0")
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
                
                # Add request body for POST/PUT/PATCH
                if method in ["post", "put", "patch"]:
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
                "paths": paths
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

        Configures an async connection pool for PostgreSQL, MySQL, or SQLite.
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
        """
        if config is None:
            config = DatabaseConfig("sqlite://cello.db")
        self._app.enable_database(config)

    def enable_redis(self, config: "RedisConfig" = None):
        """
        Enable Redis connection pooling.

        Configures an async Redis client with connection pooling,
        supporting standard and cluster modes.

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

    # ========================================================================
    # End Data Layer Features
    # ========================================================================

    # ========================================================================
    # API Protocol Features (v0.9.0+)
    # ========================================================================

    def enable_grpc(self, config: "GrpcConfig" = None):
        """
        Enable gRPC service support.

        Configures a gRPC server with service registration, reflection,
        and optional gRPC-Web support.

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
        Register a gRPC service with the application.

        Args:
            name: Service name
            methods: Optional list of method names

        Example:
            app.add_grpc_service("UserService", ["GetUser", "ListUsers"])
        """
        self._app.add_grpc_service(name, methods)

    def enable_messaging(self, config: "KafkaConfig" = None):
        """
        Enable Kafka message queue integration.

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
        Enable RabbitMQ message queue integration.

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
        Enable AWS SQS message queue integration.

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
        Enable event sourcing. Config: EventSourcingConfig or None for defaults.

        Configures the event sourcing subsystem with storage backend,
        snapshot support, and event retention settings.

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
        Enable CQRS pattern. Config: CqrsConfig or None for defaults.

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
        Enable saga orchestration. Config: SagaConfig or None for defaults.

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

    def enable_graphql(self, config: "GraphQLConfig" = None):
        """
        Enable GraphQL endpoint with optional Playground.

        Args:
            config: GraphQLConfig instance

        Example:
            from cello import App, GraphQLConfig

            app = App()
            app.enable_graphql(GraphQLConfig(
                path="/graphql",
                playground=True,
                introspection=True
            ))
        """
        if config is None:
            config = GraphQLConfig()
        self._app.enable_graphql(config)

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
            logs: bool = None):
        """
        Start the HTTP server.

        Args:
            host: Host address to bind to (default: "127.0.0.1")
            port: Port to bind to (default: 8000)
            debug: Enable debug mode (default: True in dev, False in prod)
            env: Environment "development" or "production" (default: "development")
            workers: Number of worker threads (default: CPU count)
            reload: Enable hot reload (default: False)
            logs: Enable logging (default: True in dev)

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

        # Parse CLI arguments (only if running as main script)
        if "unittest" not in sys.modules:
            parser = argparse.ArgumentParser(description="Cello Web Server", add_help=False)
            parser.add_argument("--host", default=host)
            parser.add_argument("--port", type=int, default=port)
            parser.add_argument("--env", default=env or "development")
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
            if env is None: env = args.env
            if workers is None: workers = args.workers
            if reload is False and args.reload: reload = True

            # Debug logic: CLI flag enables it, or defaults to dev env
            if debug is None:
                debug = args.debug or (env == "development")

            # Logs logic: CLI --no-logs disables it
            if logs is None:
                logs = not args.no_logs and debug

        # Set defaults if still None
        if env is None: env = "development"
        if debug is None: debug = (env == "development")
        if logs is None: logs = debug

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

        # Configure App
        if logs:
            self.enable_logging()

        # Determine worker count (default: all CPU cores)
        if workers is None:
            # Single worker in test/debug mode, multi-worker in production
            if "unittest" in sys.modules or "pytest" in sys.modules or debug:
                workers = 1
            else:
                workers = os.cpu_count() or 1

        # Print startup banner (skip in test environments)
        if "unittest" not in sys.modules and "pytest" not in sys.modules:
            self._print_banner(host, port, workers, env)

        # Run Server
        if workers > 1:
            if sys.platform == "win32":
                # Windows lacks os.fork() and SO_REUSEPORT.
                # Fall back to single worker with a clear message.
                print(f"\n    \033[33m⚠\033[0m  \033[1mWindows detected:\033[0m multi-worker requires Linux/macOS (os.fork + SO_REUSEPORT)")
                print(f"    \033[33m⚠\033[0m  Running with \033[1m1 worker\033[0m. For multi-worker, deploy on Linux or use WSL.\n")
                try:
                    self._app.run(host, port, None)
                except KeyboardInterrupt:
                    pass
            else:
                # Unix/Linux/macOS: fork + SO_REUSEPORT for best performance.
                self._run_multiprocess(host, port, workers, env)
        else:
            try:
                self._app.run(host, port, None)
            except KeyboardInterrupt:
                pass  # Handled by Rust ctrl_c

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

    def _run_multiprocess(self, host: str, port: int, workers: int, env: str):
        """Run server with multiple worker processes for maximum throughput.

        Uses os.fork() directly for reliable multi-process spawning.
        Each child process gets its own Python GIL and Tokio runtime.
        SO_REUSEPORT allows the kernel to distribute connections across workers.

        Architecture (matches Granian/Robyn behavior):
            Parent process: runs as worker + supervises children
            N child processes: each runs as an independent worker
            Total: N+1 processes on the port (same as Granian --workers N)
        """
        import os
        import signal

        print(f"    \033[32m➜\033[0m  \033[1mMode:\033[0m      SO_REUSEPORT (kernel load balancing)")

        child_pids = []

        # Fork N child workers. Parent also runs as a worker (N+1 total).
        # This matches Granian's behavior where --workers N creates N+1 processes.
        for i in range(workers):
            pid = os.fork()
            if pid == 0:
                # Child process: run server and exit
                try:
                    self._app.run(host, port, None)
                except (KeyboardInterrupt, SystemExit):
                    pass
                except Exception:
                    pass
                finally:
                    os._exit(0)
            else:
                child_pids.append(pid)

        # Parent: set up signal forwarding, then run as last worker
        def _cleanup(signum=None, frame=None):
            for pid in child_pids:
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass

        signal.signal(signal.SIGINT, lambda s, f: (_cleanup(), os._exit(0)))
        signal.signal(signal.SIGTERM, lambda s, f: (_cleanup(), os._exit(0)))

        try:
            # Parent also runs a server (another worker)
            self._app.run(host, port, None)
        except KeyboardInterrupt:
            pass
        finally:
            _cleanup()
            # Wait for children
            for pid in child_pids:
                try:
                    os.waitpid(pid, os.WNOHANG)
                except ChildProcessError:
                    pass

    def _watch_files(self, process):
        import os
        import time

        mtimes = {}

        def get_mtimes():
            changes = False
            for root, dirs, files in os.walk(os.getcwd()):
                if "__pycache__" in dirs:
                    dirs.remove("__pycache__")
                if ".git" in dirs:
                    dirs.remove(".git")
                if "target" in dirs:
                    dirs.remove("target")
                if ".venv" in dirs:
                    dirs.remove(".venv")

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
    
    Args:
        ttl: Time to live in seconds (overrides default).
        tags: List of tags for invalidation.
    """
    from functools import wraps
    from cello._cello import Response
    
    def decorator(func):
        @wraps(func)
        def wrapper(*args, **kwargs):
            response = func(*args, **kwargs)
            
            # Ensure we have a Response object to set headers
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
                     # Check if tags is list
                     if isinstance(tags, list):
                         response.set_header("X-Cache-Tags", ",".join(tags))
                     elif isinstance(tags, str):
                         response.set_header("X-Cache-Tags", tags)
            
            return response
        return wrapper
    return decorator
