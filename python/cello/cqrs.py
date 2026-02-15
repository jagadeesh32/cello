"""
Cello CQRS (Command Query Responsibility Segregation) Module.

Provides Python-friendly wrappers for the CQRS pattern including
commands, queries, result types, handler decorators, and bus
dispatchers. Designed for use with the Cello framework's Rust-powered
runtime.

Example:
    from cello import App
    from cello.cqrs import (
        Command, Query, CommandResult, QueryResult,
        CommandBus, QueryBus, command_handler, query_handler,
        CqrsConfig,
    )

    # Define commands and queries
    class CreateOrder(Command):
        def validate(self):
            if not self.items:
                raise ValueError("Order must have at least one item")

    class GetOrder(Query):
        pass

    # Create buses
    command_bus = CommandBus()
    query_bus = QueryBus()

    # Register handlers
    @command_handler(CreateOrder)
    async def handle_create_order(command):
        order_id = str(uuid.uuid4())
        # ... create order logic ...
        return CommandResult.ok({"order_id": order_id})

    @query_handler(GetOrder)
    async def handle_get_order(query):
        order_id = query.order_id
        # ... lookup logic ...
        if order_id:
            return QueryResult.ok({"order_id": order_id, "status": "active"})
        return QueryResult.not_found()

    command_bus.register(CreateOrder, handle_create_order)
    query_bus.register(GetOrder, handle_get_order)

    app = App()

    @app.post("/orders")
    async def create_order(request):
        data = request.json()
        cmd = CreateOrder(items=data["items"], total=data["total"])
        result = await command_bus.dispatch(cmd)
        if result.success:
            return {"created": True, **result.data}
        return {"error": result.error}

    @app.get("/orders/{id}")
    async def get_order(request):
        order_id = request.params["id"]
        result = await query_bus.execute(GetOrder(order_id=order_id))
        if result.found:
            return result.data
        return {"error": "Order not found"}
"""

import inspect
import time
import uuid
from typing import Any, Callable, Dict, Optional, Type


class Command:
    """
    Base class for CQRS commands.

    Commands represent intentions to change state. Subclass this to
    define specific commands. All keyword arguments passed to __init__
    are stored as instance attributes.

    Attributes:
        id: Unique command identifier (auto-generated UUID).
        timestamp: Unix timestamp when the command was created.
        command_type: String name of the command class.

    Example:
        class CreateUser(Command):
            def validate(self):
                if not self.name:
                    raise ValueError("Name is required")

        cmd = CreateUser(name="Alice", email="alice@example.com")
        print(cmd.command_type)  # "CreateUser"
        print(cmd.name)          # "Alice"
        cmd.validate()
    """

    def __init__(self, **kwargs):
        """
        Initialize a new Command.

        All keyword arguments are stored as instance attributes,
        making them accessible as properties on the command object.

        Args:
            **kwargs: Arbitrary keyword arguments stored as attributes.
        """
        self.id: str = str(uuid.uuid4())
        self.timestamp: float = time.time()
        self.__dict__.update(kwargs)

    @property
    def command_type(self) -> str:
        """
        Get the command type name.

        Returns:
            The class name of this command.
        """
        return self.__class__.__name__

    def validate(self) -> None:
        """
        Validate the command.

        Override this method in subclasses to add validation logic.
        Raise ValueError or other exceptions for invalid commands.

        Example:
            class TransferMoney(Command):
                def validate(self):
                    if self.amount <= 0:
                        raise ValueError("Amount must be positive")
                    if self.from_account == self.to_account:
                        raise ValueError("Cannot transfer to same account")
        """
        pass

    def to_dict(self) -> Dict[str, Any]:
        """
        Serialize the command to a dictionary.

        Returns:
            Dictionary representation of the command, excluding
            private attributes.

        Example:
            cmd = CreateUser(name="Alice", role="admin")
            data = cmd.to_dict()
            # {"id": "...", "command_type": "CreateUser", "timestamp": ...,
            #  "name": "Alice", "role": "admin"}
        """
        result = {
            "id": self.id,
            "command_type": self.command_type,
            "timestamp": self.timestamp,
        }
        for key, value in self.__dict__.items():
            if not key.startswith("_") and key not in ("id", "timestamp"):
                result[key] = value
        return result

    def __repr__(self) -> str:
        attrs = {
            k: v for k, v in self.__dict__.items()
            if not k.startswith("_") and k not in ("id", "timestamp")
        }
        return f"{self.command_type}(id={self.id!r}, {attrs})"


class Query:
    """
    Base class for CQRS queries.

    Queries represent requests for data that do not modify state.
    Subclass this to define specific queries. All keyword arguments
    passed to __init__ are stored as instance attributes.

    Attributes:
        id: Unique query identifier (auto-generated UUID).
        timestamp: Unix timestamp when the query was created.
        query_type: String name of the query class.

    Example:
        class GetUserById(Query):
            pass

        query = GetUserById(user_id="user-123")
        print(query.query_type)  # "GetUserById"
        print(query.user_id)     # "user-123"
    """

    def __init__(self, **kwargs):
        """
        Initialize a new Query.

        All keyword arguments are stored as instance attributes,
        making them accessible as properties on the query object.

        Args:
            **kwargs: Arbitrary keyword arguments stored as attributes.
        """
        self.id: str = str(uuid.uuid4())
        self.timestamp: float = time.time()
        self.__dict__.update(kwargs)

    @property
    def query_type(self) -> str:
        """
        Get the query type name.

        Returns:
            The class name of this query.
        """
        return self.__class__.__name__

    def to_dict(self) -> Dict[str, Any]:
        """
        Serialize the query to a dictionary.

        Returns:
            Dictionary representation of the query, excluding
            private attributes.

        Example:
            query = GetUserById(user_id="user-123")
            data = query.to_dict()
            # {"id": "...", "query_type": "GetUserById", "timestamp": ...,
            #  "user_id": "user-123"}
        """
        result = {
            "id": self.id,
            "query_type": self.query_type,
            "timestamp": self.timestamp,
        }
        for key, value in self.__dict__.items():
            if not key.startswith("_") and key not in ("id", "timestamp"):
                result[key] = value
        return result

    def __repr__(self) -> str:
        attrs = {
            k: v for k, v in self.__dict__.items()
            if not k.startswith("_") and k not in ("id", "timestamp")
        }
        return f"{self.query_type}(id={self.id!r}, {attrs})"


class CommandResult:
    """
    Result of executing a command.

    Wraps the outcome of command dispatch, indicating success or
    failure along with associated data or error information.

    Attributes:
        success: Whether the command executed successfully.
        data: Optional result data on success.
        error: Optional error message on failure.

    Example:
        result = CommandResult.ok({"order_id": "order-123"})
        print(result.success)  # True
        print(result.data)     # {"order_id": "order-123"}

        result = CommandResult.fail("Insufficient funds")
        print(result.success)  # False
        print(result.error)    # "Insufficient funds"
    """

    def __init__(
        self,
        success: bool,
        data: Optional[Any] = None,
        error: Optional[str] = None,
    ):
        """
        Initialize a CommandResult.

        Args:
            success: Whether the command succeeded.
            data: Optional result data.
            error: Optional error message.
        """
        self.success: bool = success
        self.data: Optional[Any] = data
        self.error: Optional[str] = error

    @classmethod
    def ok(cls, data: Any = None) -> "CommandResult":
        """
        Create a successful CommandResult.

        Args:
            data: Optional result data.

        Returns:
            CommandResult with success=True.

        Example:
            result = CommandResult.ok({"id": "123"})
        """
        return cls(success=True, data=data)

    @classmethod
    def fail(cls, error: str) -> "CommandResult":
        """
        Create a failed CommandResult.

        Args:
            error: Error message describing the failure.

        Returns:
            CommandResult with success=False.

        Example:
            result = CommandResult.fail("Validation failed: name is required")
        """
        return cls(success=False, error=error)

    @classmethod
    def rejected(cls, reason: str) -> "CommandResult":
        """
        Create a rejected CommandResult.

        Used when a command is rejected before execution (e.g. due
        to validation or authorization failures).

        Args:
            reason: Reason the command was rejected.

        Returns:
            CommandResult with success=False and the rejection reason.

        Example:
            result = CommandResult.rejected("Unauthorized: admin role required")
        """
        return cls(success=False, error=f"Rejected: {reason}")

    def __repr__(self) -> str:
        if self.success:
            return f"CommandResult(success=True, data={self.data!r})"
        return f"CommandResult(success=False, error={self.error!r})"


class QueryResult:
    """
    Result of executing a query.

    Wraps the outcome of query execution, including the returned data
    or error information.

    Attributes:
        data: The query result data, or None.
        error: Optional error message.
        found: Whether data was found (True if data is not None and
               no error occurred).

    Example:
        result = QueryResult.ok({"name": "Alice", "email": "alice@example.com"})
        print(result.found)  # True
        print(result.data)   # {"name": "Alice", ...}

        result = QueryResult.not_found()
        print(result.found)  # False
    """

    def __init__(
        self,
        data: Optional[Any] = None,
        error: Optional[str] = None,
    ):
        """
        Initialize a QueryResult.

        Args:
            data: Optional query result data.
            error: Optional error message.
        """
        self.data: Optional[Any] = data
        self.error: Optional[str] = error

    @property
    def found(self) -> bool:
        """
        Whether the query found data.

        Returns:
            True if data is not None and no error occurred.
        """
        return self.data is not None and self.error is None

    @classmethod
    def ok(cls, data: Any) -> "QueryResult":
        """
        Create a successful QueryResult with data.

        Args:
            data: The query result data.

        Returns:
            QueryResult with data populated.

        Example:
            result = QueryResult.ok({"users": [{"id": 1, "name": "Alice"}]})
        """
        return cls(data=data)

    @classmethod
    def not_found(cls) -> "QueryResult":
        """
        Create a QueryResult indicating no data was found.

        Returns:
            QueryResult with data=None and no error.

        Example:
            result = QueryResult.not_found()
            print(result.found)  # False
        """
        return cls(data=None, error=None)

    @classmethod
    def fail(cls, error: str) -> "QueryResult":
        """
        Create a failed QueryResult.

        Args:
            error: Error message describing the failure.

        Returns:
            QueryResult with error populated.

        Example:
            result = QueryResult.fail("Database connection timeout")
        """
        return cls(data=None, error=error)

    def __repr__(self) -> str:
        if self.error:
            return f"QueryResult(error={self.error!r})"
        if self.found:
            return f"QueryResult(data={self.data!r})"
        return "QueryResult(not_found)"


def command_handler(command_class: Type[Command]) -> Callable:
    """
    Decorator to mark an async function as a handler for a command type.

    The decorated function is called when the matching command type is
    dispatched through the CommandBus. It should accept a single command
    argument and return a CommandResult.

    Args:
        command_class: The Command subclass this handler processes.

    Returns:
        Decorator function for the command handler.

    Example:
        @command_handler(CreateOrder)
        async def handle_create_order(command):
            order_id = str(uuid.uuid4())
            return CommandResult.ok({"order_id": order_id})

        command_bus.register(CreateOrder, handle_create_order)
    """
    def decorator(func: Callable) -> Callable:
        func._cello_command_handler = True
        func._cello_command_class = command_class
        func._cello_command_type = command_class.__name__
        return func
    return decorator


def query_handler(query_class: Type[Query]) -> Callable:
    """
    Decorator to mark an async function as a handler for a query type.

    The decorated function is called when the matching query type is
    executed through the QueryBus. It should accept a single query
    argument and return a QueryResult.

    Args:
        query_class: The Query subclass this handler processes.

    Returns:
        Decorator function for the query handler.

    Example:
        @query_handler(GetOrder)
        async def handle_get_order(query):
            order = await db.find_order(query.order_id)
            if order:
                return QueryResult.ok(order)
            return QueryResult.not_found()

        query_bus.register(GetOrder, handle_get_order)
    """
    def decorator(func: Callable) -> Callable:
        func._cello_query_handler = True
        func._cello_query_class = query_class
        func._cello_query_type = query_class.__name__
        return func
    return decorator


class CommandBus:
    """
    Dispatches commands to their registered handlers.

    The CommandBus maintains a registry of command types to handler
    functions. When a command is dispatched, the bus looks up the
    appropriate handler, validates the command, and executes the handler.

    Example:
        bus = CommandBus()

        @command_handler(CreateUser)
        async def handle_create_user(command):
            return CommandResult.ok({"user_id": "123"})

        bus.register(CreateUser, handle_create_user)

        result = await bus.dispatch(CreateUser(name="Alice"))
        print(result.success)  # True
    """

    def __init__(self):
        """Initialize the CommandBus with an empty handler registry."""
        self._handlers: Dict[str, Callable] = {}

    def register(self, command_type: Type[Command], handler: Callable) -> None:
        """
        Register a handler for a command type.

        Args:
            command_type: The Command subclass to handle.
            handler: Async callable that processes the command.

        Example:
            bus.register(CreateOrder, handle_create_order)
        """
        self._handlers[command_type.__name__] = handler

    async def dispatch(self, command: Command) -> CommandResult:
        """
        Dispatch a command to its registered handler.

        Validates the command before dispatching. If no handler is
        registered for the command type, returns a failed result.

        Args:
            command: The command instance to dispatch.

        Returns:
            CommandResult from the handler, or a failure result
            if no handler is found or validation fails.

        Example:
            cmd = CreateOrder(items=["Widget"], total=42.00)
            result = await bus.dispatch(cmd)
            if result.success:
                print("Order created:", result.data)
        """
        # Validate the command
        try:
            command.validate()
        except (ValueError, TypeError) as e:
            return CommandResult.rejected(str(e))

        handler = self._handlers.get(command.command_type)
        if handler is None:
            return CommandResult.fail(
                f"No handler registered for command: {command.command_type}"
            )

        try:
            if inspect.iscoroutinefunction(handler):
                result = await handler(command)
            else:
                result = handler(command)

            if isinstance(result, CommandResult):
                return result
            return CommandResult.ok(result)
        except Exception as e:
            return CommandResult.fail(str(e))

    def __repr__(self) -> str:
        handler_types = list(self._handlers.keys())
        return f"CommandBus(handlers={handler_types})"


class QueryBus:
    """
    Executes queries through their registered handlers.

    The QueryBus maintains a registry of query types to handler
    functions. When a query is executed, the bus looks up the
    appropriate handler and returns its result.

    Example:
        bus = QueryBus()

        @query_handler(GetUser)
        async def handle_get_user(query):
            user = await db.find(query.user_id)
            if user:
                return QueryResult.ok(user)
            return QueryResult.not_found()

        bus.register(GetUser, handle_get_user)

        result = await bus.execute(GetUser(user_id="123"))
        if result.found:
            print("User:", result.data)
    """

    def __init__(self):
        """Initialize the QueryBus with an empty handler registry."""
        self._handlers: Dict[str, Callable] = {}

    def register(self, query_type: Type[Query], handler: Callable) -> None:
        """
        Register a handler for a query type.

        Args:
            query_type: The Query subclass to handle.
            handler: Async callable that processes the query.

        Example:
            bus.register(GetOrder, handle_get_order)
        """
        self._handlers[query_type.__name__] = handler

    async def execute(self, query: Query) -> QueryResult:
        """
        Execute a query through its registered handler.

        If no handler is registered for the query type, returns a
        failed result.

        Args:
            query: The query instance to execute.

        Returns:
            QueryResult from the handler, or a failure result
            if no handler is found.

        Example:
            result = await bus.execute(GetOrder(order_id="order-123"))
            if result.found:
                print("Order:", result.data)
        """
        handler = self._handlers.get(query.query_type)
        if handler is None:
            return QueryResult.fail(
                f"No handler registered for query: {query.query_type}"
            )

        try:
            if inspect.iscoroutinefunction(handler):
                result = await handler(query)
            else:
                result = handler(query)

            if isinstance(result, QueryResult):
                return result
            return QueryResult.ok(result)
        except Exception as e:
            return QueryResult.fail(str(e))

    def __repr__(self) -> str:
        handler_types = list(self._handlers.keys())
        return f"QueryBus(handlers={handler_types})"


class CqrsConfig:
    """
    Configuration for the CQRS subsystem.

    Controls event synchronization, timeouts, and retry behaviour
    for command and query processing.

    Attributes:
        enable_event_sync: Whether to synchronize events between
            command and query sides (default: True).
        command_timeout_ms: Timeout for command execution in
            milliseconds (default: 5000).
        query_timeout_ms: Timeout for query execution in
            milliseconds (default: 3000).
        max_retries: Maximum retry attempts for failed operations
            (default: 3).

    Example:
        config = CqrsConfig(
            enable_event_sync=True,
            command_timeout_ms=10000,
            query_timeout_ms=5000,
            max_retries=5,
        )
    """

    def __init__(
        self,
        enable_event_sync: bool = True,
        command_timeout_ms: int = 5000,
        query_timeout_ms: int = 3000,
        max_retries: int = 3,
    ):
        """
        Initialize CqrsConfig.

        Args:
            enable_event_sync: Synchronize events between sides (default: True).
            command_timeout_ms: Command timeout in ms (default: 5000).
            query_timeout_ms: Query timeout in ms (default: 3000).
            max_retries: Maximum retry attempts (default: 3).
        """
        self.enable_event_sync: bool = enable_event_sync
        self.command_timeout_ms: int = command_timeout_ms
        self.query_timeout_ms: int = query_timeout_ms
        self.max_retries: int = max_retries

    def __repr__(self) -> str:
        return (
            f"CqrsConfig(enable_event_sync={self.enable_event_sync}, "
            f"command_timeout_ms={self.command_timeout_ms}, "
            f"query_timeout_ms={self.query_timeout_ms}, "
            f"max_retries={self.max_retries})"
        )
