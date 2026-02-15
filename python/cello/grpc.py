"""
Cello gRPC Integration.

Provides Python-friendly wrappers for gRPC service definition, request/response
handling, server management, and client channels with connection pooling.

Example:
    from cello import App
    from cello.grpc import GrpcService, grpc_method, GrpcServer, GrpcChannel

    class UserService(GrpcService):
        @grpc_method
        def get_user(self, request):
            return {"id": request.data["id"], "name": "Alice"}

        @grpc_method(stream=True)
        def list_users(self, request):
            yield {"id": 1, "name": "Alice"}
            yield {"id": 2, "name": "Bob"}

    server = GrpcServer()
    server.register_service(UserService())

    @app.on_event("startup")
    async def setup():
        await server.start("[::]:50051")

    @app.on_event("shutdown")
    async def teardown():
        await server.stop()

    # Client usage
    async def call_user_service():
        channel = await GrpcChannel.connect("localhost:50051")
        result = await channel.call("UserService", "get_user", {"id": 1})
        await channel.close()
"""

from functools import wraps
from typing import Any, Callable, Optional


class GrpcError(Exception):
    """
    Exception for gRPC errors with status codes.

    Provides standard gRPC status codes as class attributes for convenience.

    Example:
        raise GrpcError(
            code=GrpcError.NOT_FOUND,
            message="User not found",
            details="No user with id=42"
        )
    """

    # Standard gRPC status codes
    OK: int = 0
    CANCELLED: int = 1
    UNKNOWN: int = 2
    INVALID_ARGUMENT: int = 3
    DEADLINE_EXCEEDED: int = 4
    NOT_FOUND: int = 5
    ALREADY_EXISTS: int = 6
    PERMISSION_DENIED: int = 7
    RESOURCE_EXHAUSTED: int = 8
    FAILED_PRECONDITION: int = 9
    ABORTED: int = 10
    OUT_OF_RANGE: int = 11
    UNIMPLEMENTED: int = 12
    INTERNAL: int = 13
    UNAVAILABLE: int = 14
    DATA_LOSS: int = 15
    UNAUTHENTICATED: int = 16

    def __init__(self, code: int, message: str, details: str = None):
        """
        Initialize a gRPC error.

        Args:
            code: gRPC status code (use class attributes like GrpcError.NOT_FOUND).
            message: Human-readable error message.
            details: Optional additional error details.
        """
        self.code = code
        self.message = message
        self.details = details
        super().__init__(f"GrpcError(code={code}, message={message})")

    def __repr__(self) -> str:
        return f"GrpcError(code={self.code}, message={self.message!r}, details={self.details!r})"


def grpc_method(func: Callable = None, *, stream: bool = False) -> Callable:
    """
    Decorator to mark a method as a gRPC endpoint.

    Stores metadata on the function including the method name and whether
    it uses streaming. Can be used with or without arguments.

    Args:
        func: The method to decorate (when used without parentheses).
        stream: Whether this method uses streaming responses (default: False).

    Returns:
        Decorated function with gRPC metadata attached.

    Example:
        class MyService(GrpcService):
            @grpc_method
            def unary_call(self, request):
                return {"result": "ok"}

            @grpc_method(stream=True)
            def streaming_call(self, request):
                yield {"chunk": 1}
                yield {"chunk": 2}
    """
    def decorator(fn: Callable) -> Callable:
        @wraps(fn)
        def wrapper(*args, **kwargs):
            return fn(*args, **kwargs)

        # Attach gRPC metadata to the function
        wrapper._grpc_method = True
        wrapper._grpc_method_name = fn.__name__
        wrapper._grpc_stream = stream
        return wrapper

    if func is not None:
        # Called without parentheses: @grpc_method
        return decorator(func)
    # Called with parentheses: @grpc_method(stream=True)
    return decorator


class GrpcRequest:
    """
    Represents an incoming gRPC request.

    Encapsulates the target service, method, payload data, and any
    additional metadata (headers) sent by the client.

    Example:
        request = GrpcRequest(
            service="UserService",
            method="get_user",
            data={"id": 42},
            metadata={"authorization": "Bearer token123"}
        )
    """

    def __init__(
        self,
        service: str,
        method: str,
        data: dict = None,
        metadata: dict = None,
    ):
        """
        Initialize a gRPC request.

        Args:
            service: Target service name.
            method: Target method name.
            data: Request payload as a dictionary.
            metadata: Optional request metadata (headers).
        """
        self._service = service
        self._method = method
        self._data = data or {}
        self._metadata = metadata or {}

    @property
    def service(self) -> str:
        """Get the target service name."""
        return self._service

    @property
    def method(self) -> str:
        """Get the target method name."""
        return self._method

    @property
    def data(self) -> dict:
        """Get the request payload data."""
        return self._data

    @property
    def metadata(self) -> dict:
        """Get the request metadata."""
        return self._metadata

    def __repr__(self) -> str:
        return f"GrpcRequest(service={self._service!r}, method={self._method!r})"


class GrpcResponse:
    """
    Represents a gRPC response.

    Contains the response payload, a status code, a human-readable
    message, and optional response metadata.

    Example:
        response = GrpcResponse.ok({"id": 1, "name": "Alice"})
        error_response = GrpcResponse.error(5, "User not found")
    """

    def __init__(
        self,
        data: dict = None,
        status_code: int = 0,
        message: str = "OK",
    ):
        """
        Initialize a gRPC response.

        Args:
            data: Response payload as a dictionary.
            status_code: gRPC status code (0 = OK).
            message: Human-readable status message.
        """
        self._data = data or {}
        self._status_code = status_code
        self._message = message
        self._metadata: dict = {}

    @property
    def data(self) -> dict:
        """Get the response payload data."""
        return self._data

    @property
    def status_code(self) -> int:
        """Get the gRPC status code."""
        return self._status_code

    @property
    def message(self) -> str:
        """Get the status message."""
        return self._message

    @property
    def metadata(self) -> dict:
        """Get the response metadata."""
        return self._metadata

    @classmethod
    def ok(cls, data: dict) -> "GrpcResponse":
        """
        Create a successful gRPC response.

        Args:
            data: Response payload.

        Returns:
            GrpcResponse with status OK (0).
        """
        return cls(data=data, status_code=0, message="OK")

    @classmethod
    def error(cls, code: int, message: str) -> "GrpcResponse":
        """
        Create an error gRPC response.

        Args:
            code: gRPC status code.
            message: Error message.

        Returns:
            GrpcResponse with the specified error code.
        """
        return cls(data=None, status_code=code, message=message)

    def __repr__(self) -> str:
        return (
            f"GrpcResponse(status_code={self._status_code}, "
            f"message={self._message!r})"
        )


class GrpcService:
    """
    Base class for defining gRPC services.

    Subclass this and decorate methods with @grpc_method to register
    them as gRPC endpoints. The service name is auto-extracted from the
    class name if not provided explicitly.

    Example:
        class UserService(GrpcService):
            @grpc_method
            def get_user(self, request):
                user_id = request.data["id"]
                return {"id": user_id, "name": "Alice"}

            @grpc_method(stream=True)
            def list_users(self, request):
                yield {"id": 1, "name": "Alice"}
                yield {"id": 2, "name": "Bob"}

        service = UserService()
        print(service.get_name())       # "UserService"
        print(service.get_methods())    # [{"name": "get_user", ...}, ...]
    """

    def __init__(self, name: str = None):
        """
        Initialize the gRPC service.

        Args:
            name: Service name. If None, auto-extracted from the class name.
        """
        self._name = name or self.__class__.__name__
        self._methods: dict = {}
        self._discover_methods()

    def _discover_methods(self) -> None:
        """Scan the class for methods decorated with @grpc_method."""
        for attr_name in dir(self):
            if attr_name.startswith("_"):
                continue
            attr = getattr(self, attr_name, None)
            if callable(attr) and getattr(attr, "_grpc_method", False):
                self._methods[attr._grpc_method_name] = {
                    "name": attr._grpc_method_name,
                    "handler": attr,
                    "stream": attr._grpc_stream,
                }

    def get_methods(self) -> list[dict]:
        """
        Return metadata for all registered gRPC methods.

        Returns:
            List of dicts with keys: name, stream.
        """
        return [
            {"name": info["name"], "stream": info["stream"]}
            for info in self._methods.values()
        ]

    def get_name(self) -> str:
        """
        Get the service name.

        Returns:
            The service name string.
        """
        return self._name

    def __repr__(self) -> str:
        method_count = len(self._methods)
        return f"GrpcService(name={self._name!r}, methods={method_count})"


class GrpcServer:
    """
    gRPC server that hosts registered services.

    Wraps the Rust-powered gRPC server with a Pythonic API for
    service registration, startup, and shutdown.

    Example:
        server = GrpcServer()
        server.register_service(UserService())
        server.register_service(OrderService())

        # In an async context
        await server.start("[::]:50051")

        # Graceful shutdown
        await server.stop()
    """

    def __init__(self, config: Any = None):
        """
        Initialize the gRPC server.

        Args:
            config: Optional server configuration dict or object.
        """
        self._config = config
        self._services: dict[str, GrpcService] = {}
        self._running = False

    def register_service(self, service: GrpcService) -> None:
        """
        Register a gRPC service with the server.

        Args:
            service: A GrpcService subclass instance.

        Raises:
            TypeError: If the provided object is not a GrpcService instance.
            ValueError: If a service with the same name is already registered.
        """
        if not isinstance(service, GrpcService):
            raise TypeError(
                f"Expected GrpcService instance, got {type(service).__name__}"
            )
        name = service.get_name()
        if name in self._services:
            raise ValueError(f"Service '{name}' is already registered")
        self._services[name] = service

    def get_services(self) -> list[str]:
        """
        Get the names of all registered services.

        Returns:
            List of registered service name strings.
        """
        return list(self._services.keys())

    async def start(self, address: str = "[::]:50051") -> None:
        """
        Start the gRPC server on the given address.

        Args:
            address: Bind address in host:port format (default: "[::]:50051").

        Raises:
            RuntimeError: If the server is already running.
        """
        if self._running:
            raise RuntimeError("gRPC server is already running")
        # In a real implementation, this would call the Rust gRPC engine
        self._running = True
        self._address = address

    async def stop(self) -> None:
        """
        Gracefully stop the gRPC server.

        Waits for in-flight requests to complete before shutting down.
        """
        if self._running:
            # In a real implementation, this would signal the Rust server to stop
            self._running = False

    def __repr__(self) -> str:
        service_count = len(self._services)
        return f"GrpcServer(services={service_count}, running={self._running})"


class GrpcChannel:
    """
    gRPC client channel for making remote procedure calls.

    Wraps the Rust-powered gRPC client with a Pythonic async API
    for connecting to remote services and making calls.

    Example:
        channel = await GrpcChannel.connect("localhost:50051")

        result = await channel.call(
            "UserService",
            "get_user",
            {"id": 42}
        )
        print(result)  # {"id": 42, "name": "Alice"}

        await channel.close()
    """

    def __init__(self, target: str):
        """
        Initialize a gRPC channel.

        Use the async classmethod `connect()` instead of constructing directly.

        Args:
            target: Target address in host:port format.
        """
        self._target = target
        self._connected = False

    @classmethod
    async def connect(cls, target: str) -> "GrpcChannel":
        """
        Create and connect a gRPC channel to the target address.

        Args:
            target: Target address in host:port format (e.g., "localhost:50051").

        Returns:
            A connected GrpcChannel instance.
        """
        instance = cls(target)
        # In a real implementation, this would establish a Rust-powered connection
        instance._connected = True
        return instance

    async def call(self, service: str, method: str, request: dict) -> dict:
        """
        Make a unary gRPC call to a remote service method.

        Args:
            service: Target service name.
            method: Target method name.
            request: Request payload as a dictionary.

        Returns:
            Response payload as a dictionary.

        Raises:
            GrpcError: If the call fails or the channel is not connected.
        """
        if not self._connected:
            raise GrpcError(
                code=GrpcError.UNAVAILABLE,
                message="Channel is not connected",
                details=f"Target: {self._target}",
            )
        # Placeholder - real implementation calls Rust gRPC client
        return {}

    async def close(self) -> None:
        """
        Close the gRPC channel and release resources.
        """
        self._connected = False

    def __repr__(self) -> str:
        return (
            f"GrpcChannel(target={self._target!r}, "
            f"connected={self._connected})"
        )
