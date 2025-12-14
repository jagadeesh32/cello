"""
Cello - Ultra-fast Rust-powered Python web framework.

A high-performance async web framework with Rust core and Python developer experience.
All I/O, routing, and JSON serialization happen in Rust for maximum performance.

Features:
- SIMD-accelerated JSON parsing
- Middleware system with CORS, logging, compression
- Blueprint-based routing with inheritance
- WebSocket and SSE support
- File uploads and multipart form handling

Example:
    from cello import App, Blueprint

    app = App()

    # Enable built-in middleware
    app.enable_cors()
    app.enable_logging()

    @app.get("/")
    def home(request):
        return {"message": "Hello, Cello!"}

    # Blueprint for route grouping
    api = Blueprint("/api")

    @api.get("/users/{id}")
    def get_user(request):
        return {"id": request.params["id"]}

    app.register_blueprint(api)

    if __name__ == "__main__":
        app.run()
"""

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

__all__ = [
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
]
__version__ = "0.2.0"


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
    """

    def __init__(self):
        """Create a new Cello application."""
        self._app = Cello()

    def get(self, path: str):
        """
        Register a GET route.

        Args:
            path: URL path pattern (e.g., "/users/{id}")

        Returns:
            Decorator function for the route handler.

        Example:
            @app.get("/hello/{name}")
            def hello(request):
                return {"message": f"Hello, {request.params['name']}!"}
        """
        def decorator(func):
            self._app.get(path, func)
            return func
        return decorator

    def post(self, path: str):
        """Register a POST route."""
        def decorator(func):
            self._app.post(path, func)
            return func
        return decorator

    def put(self, path: str):
        """Register a PUT route."""
        def decorator(func):
            self._app.put(path, func)
            return func
        return decorator

    def delete(self, path: str):
        """Register a DELETE route."""
        def decorator(func):
            self._app.delete(path, func)
            return func
        return decorator

    def patch(self, path: str):
        """Register a PATCH route."""
        def decorator(func):
            self._app.patch(path, func)
            return func
        return decorator

    def options(self, path: str):
        """Register an OPTIONS route."""
        def decorator(func):
            self._app.options(path, func)
            return func
        return decorator

    def head(self, path: str):
        """Register a HEAD route."""
        def decorator(func):
            self._app.head(path, func)
            return func
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
            for method in methods:
                method_upper = method.upper()
                if method_upper == "GET":
                    self._app.get(path, func)
                elif method_upper == "POST":
                    self._app.post(path, func)
                elif method_upper == "PUT":
                    self._app.put(path, func)
                elif method_upper == "DELETE":
                    self._app.delete(path, func)
                elif method_upper == "PATCH":
                    self._app.patch(path, func)
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

    def run(self, host: str = "127.0.0.1", port: int = 8000):
        """
        Start the HTTP server.

        Args:
            host: Host address to bind to (default: "127.0.0.1")
            port: Port to bind to (default: 8000)
        """
        self._app.run(host, port)
