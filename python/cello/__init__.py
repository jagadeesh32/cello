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
__version__ = "0.3.0"


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

    def run(self, host: str = "127.0.0.1", port: int = 8000, 
            debug: bool = None, env: str = None, 
            workers: int = None, reload: bool = False,
            loogs: bool = None):
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
            parser.add_argument("--workers", type=int, default=workers)
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
            if loogs is None:
                loogs = not args.no_logs and debug

        # Set defaults if still None
        if env is None: env = "development"
        if debug is None: debug = (env == "development")
        if loogs is None: loogs = debug

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
        if loogs:
            self.enable_logging()

        # Run Server
        try:
             self._app.run(host, port, workers)
        except KeyboardInterrupt:
            pass # Handled by Rust ctrl_c

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
