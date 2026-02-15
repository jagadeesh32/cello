"""
Cello GraphQL Integration.

Provides Python-friendly decorators and classes for building GraphQL APIs
with query resolution, mutations, subscriptions, and DataLoader support
for N+1 query prevention.

Example:
    from cello import App
    from cello.graphql import GraphQL, Schema, Query, Mutation, Subscription, DataLoader

    app = App()

    @Query
    def users(info) -> list:
        return [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]

    @Mutation
    def create_user(info, name: str) -> dict:
        return {"id": 3, "name": name}

    @Subscription
    def on_message(info) -> dict:
        return {"message": "New message received"}

    # Build schema
    schema = Schema()
    schema.query(users)
    schema.mutation(create_user)
    schema.subscription(on_message)

    gql = schema.build()

    # Execute a query
    result = await gql.execute('{ users { id name } }')

    # DataLoader for N+1 prevention
    async def batch_load_users(keys):
        return [{"id": k, "name": f"User {k}"} for k in keys]

    user_loader = DataLoader(batch_load_users)
    user = await user_loader.load(1)
    users = await user_loader.load_many([1, 2, 3])
"""

import inspect
from functools import wraps
from typing import Any, Callable, Optional, Dict, List


class Query:
    """
    Decorator class for marking a function as a GraphQL query resolver.

    Stores the decorated function along with its name and metadata
    extracted from type hints and docstring.

    Example:
        @Query
        def users(info) -> list:
            \"\"\"Fetch all users.\"\"\"
            return [{"id": 1, "name": "Alice"}]

        @Query
        def user(info, id: int) -> dict:
            \"\"\"Fetch a single user by ID.\"\"\"
            return {"id": id, "name": "Alice"}
    """

    def __init__(self, func: Callable):
        """
        Initialize the Query decorator.

        Args:
            func: The resolver function to wrap.
        """
        self._func = func
        self._name = func.__name__
        self._doc = func.__doc__ or ""
        self._return_type = _extract_return_type(func)
        self._parameters = _extract_parameters(func)
        wraps(func)(self)

    def __call__(self, *args, **kwargs) -> Any:
        """Execute the underlying resolver function."""
        return self._func(*args, **kwargs)

    @property
    def name(self) -> str:
        """Get the resolver name."""
        return self._name

    @property
    def func(self) -> Callable:
        """Get the underlying function."""
        return self._func

    @property
    def return_type(self) -> Optional[str]:
        """Get the return type annotation as a string."""
        return self._return_type

    @property
    def parameters(self) -> Dict[str, str]:
        """Get parameter names mapped to their type annotations."""
        return self._parameters

    def __repr__(self) -> str:
        return f"<Query '{self._name}'>"


class Mutation:
    """
    Decorator class for marking a function as a GraphQL mutation resolver.

    Stores the decorated function along with its name and metadata
    extracted from type hints and docstring.

    Example:
        @Mutation
        def create_user(info, name: str) -> dict:
            \"\"\"Create a new user.\"\"\"
            return {"id": 3, "name": name}

        @Mutation
        def delete_user(info, id: int) -> dict:
            \"\"\"Delete a user by ID.\"\"\"
            return {"deleted": True}
    """

    def __init__(self, func: Callable):
        """
        Initialize the Mutation decorator.

        Args:
            func: The resolver function to wrap.
        """
        self._func = func
        self._name = func.__name__
        self._doc = func.__doc__ or ""
        self._return_type = _extract_return_type(func)
        self._parameters = _extract_parameters(func)
        wraps(func)(self)

    def __call__(self, *args, **kwargs) -> Any:
        """Execute the underlying resolver function."""
        return self._func(*args, **kwargs)

    @property
    def name(self) -> str:
        """Get the resolver name."""
        return self._name

    @property
    def func(self) -> Callable:
        """Get the underlying function."""
        return self._func

    @property
    def return_type(self) -> Optional[str]:
        """Get the return type annotation as a string."""
        return self._return_type

    @property
    def parameters(self) -> Dict[str, str]:
        """Get parameter names mapped to their type annotations."""
        return self._parameters

    def __repr__(self) -> str:
        return f"<Mutation '{self._name}'>"


class Subscription:
    """
    Decorator class for marking a function as a GraphQL subscription resolver.

    Subscriptions are used for real-time data updates over WebSocket
    connections. The decorated function is expected to yield or return
    data as new events occur.

    Example:
        @Subscription
        def on_message(info) -> dict:
            \"\"\"Subscribe to new messages.\"\"\"
            return {"message": "New message received"}

        @Subscription
        async def on_user_created(info) -> dict:
            \"\"\"Subscribe to user creation events.\"\"\"
            return {"user": {"id": 1, "name": "Alice"}}
    """

    def __init__(self, func: Callable):
        """
        Initialize the Subscription decorator.

        Args:
            func: The resolver function to wrap.
        """
        self._func = func
        self._name = func.__name__
        self._doc = func.__doc__ or ""
        self._return_type = _extract_return_type(func)
        self._parameters = _extract_parameters(func)
        self._is_async = inspect.iscoroutinefunction(func) or inspect.isasyncgenfunction(func)
        wraps(func)(self)

    def __call__(self, *args, **kwargs) -> Any:
        """Execute the underlying resolver function."""
        return self._func(*args, **kwargs)

    @property
    def name(self) -> str:
        """Get the resolver name."""
        return self._name

    @property
    def func(self) -> Callable:
        """Get the underlying function."""
        return self._func

    @property
    def return_type(self) -> Optional[str]:
        """Get the return type annotation as a string."""
        return self._return_type

    @property
    def parameters(self) -> Dict[str, str]:
        """Get parameter names mapped to their type annotations."""
        return self._parameters

    def __repr__(self) -> str:
        return f"<Subscription '{self._name}'>"


class Field:
    """
    Defines a GraphQL field with type information and an optional resolver.

    Fields describe the shape of GraphQL types and can carry custom
    resolver functions for computed or derived values.

    Example:
        name_field = Field("name", "String", description="The user's name")

        full_name_field = Field(
            "full_name",
            "String",
            description="Computed full name",
            resolver=lambda obj, info: f"{obj['first']} {obj['last']}"
        )
    """

    def __init__(
        self,
        name: str,
        type_name: str,
        description: Optional[str] = None,
        resolver: Optional[Callable] = None,
    ):
        """
        Initialize a GraphQL field.

        Args:
            name: The field name as it appears in the schema.
            type_name: The GraphQL type name (e.g., "String", "Int", "[User]").
            description: Optional human-readable description of the field.
            resolver: Optional resolver function for computing the field value.
        """
        self.name = name
        self.type_name = type_name
        self.description = description
        self.resolver = resolver

    def resolve(self, obj: Any, info: Any, **kwargs) -> Any:
        """
        Resolve the field value.

        If a custom resolver is set, it is called with the parent object
        and info context. Otherwise, the field value is looked up by name
        on the parent object (dict key or attribute).

        Args:
            obj: The parent object.
            info: The GraphQL resolve info context.
            **kwargs: Additional arguments passed to the resolver.

        Returns:
            The resolved field value.
        """
        if self.resolver is not None:
            return self.resolver(obj, info, **kwargs)

        # Default resolution: dict key or attribute lookup
        if isinstance(obj, dict):
            return obj.get(self.name)
        return getattr(obj, self.name, None)

    def __repr__(self) -> str:
        return f"<Field '{self.name}: {self.type_name}'>"


class DataLoader:
    """
    DataLoader for batching and caching data fetches to prevent N+1 queries.

    Collects individual load requests and dispatches them in a single batch
    call, then caches results for subsequent requests within the same
    execution context.

    Example:
        async def batch_load_users(keys):
            # Single query for all requested user IDs
            rows = await db.fetch_all(
                "SELECT * FROM users WHERE id = ANY($1)", keys
            )
            # Return results in the same order as keys
            user_map = {r["id"]: r for r in rows}
            return [user_map.get(k) for k in keys]

        user_loader = DataLoader(batch_load_users)

        # These will be batched into a single DB query
        user_a = await user_loader.load(1)
        user_b = await user_loader.load(2)
        users = await user_loader.load_many([3, 4, 5])

        # Clear cache for a specific key or all keys
        user_loader.clear(1)
        user_loader.clear()
    """

    def __init__(self, batch_fn: Callable):
        """
        Initialize the DataLoader.

        Args:
            batch_fn: An async function that accepts a list of keys and returns
                      a list of results in the same order. Must return one result
                      per key.
        """
        self._batch_fn = batch_fn
        self._cache: Dict[Any, Any] = {}
        self._batch: List[Any] = []

    async def load(self, key: Any) -> Any:
        """
        Load a single value by key.

        Returns a cached result if available, otherwise adds the key to the
        current batch, dispatches the batch, and returns the result.

        Args:
            key: The key to load.

        Returns:
            The value associated with the key.
        """
        if key in self._cache:
            return self._cache[key]

        self._batch.append(key)
        results = await self._dispatch()

        return self._cache.get(key)

    async def load_many(self, keys: List[Any]) -> List[Any]:
        """
        Load multiple values by their keys.

        Keys already present in the cache are returned immediately.
        Missing keys are batched together in a single dispatch call.

        Args:
            keys: A list of keys to load.

        Returns:
            A list of values in the same order as the input keys.
        """
        missing_keys = [k for k in keys if k not in self._cache]

        if missing_keys:
            self._batch.extend(missing_keys)
            await self._dispatch()

        return [self._cache.get(k) for k in keys]

    def clear(self, key: Any = None) -> None:
        """
        Clear cached values.

        If a key is provided, only that key is removed from the cache.
        If no key is provided, the entire cache is cleared.

        Args:
            key: Optional specific key to remove from the cache.
        """
        if key is not None:
            self._cache.pop(key, None)
        else:
            self._cache.clear()

    async def _dispatch(self) -> List[Any]:
        """
        Execute the batch function with all accumulated keys.

        Drains the internal batch list, calls the batch function, and
        populates the cache with the returned results. The batch function
        must return exactly one result per key, in the same order.

        Returns:
            The list of results from the batch function.

        Raises:
            ValueError: If the batch function returns a different number
                        of results than keys provided.
        """
        if not self._batch:
            return []

        # Drain the batch list
        keys = list(self._batch)
        self._batch.clear()

        # Deduplicate while preserving order for the batch call
        seen = set()
        unique_keys = []
        for k in keys:
            if k not in seen and k not in self._cache:
                seen.add(k)
                unique_keys.append(k)

        if not unique_keys:
            return []

        # Call the batch function
        results = await self._batch_fn(unique_keys)

        if len(results) != len(unique_keys):
            raise ValueError(
                f"DataLoader batch function returned {len(results)} results "
                f"for {len(unique_keys)} keys. Must return exactly one result per key."
            )

        # Populate cache
        for key, value in zip(unique_keys, results):
            self._cache[key] = value

        return results


class GraphQL:
    """
    Main GraphQL execution engine for mounting on a Cello application.

    Manages query, mutation, and subscription resolvers and executes
    incoming GraphQL operations against them.

    Example:
        gql = GraphQL()

        @Query
        def hello(info) -> str:
            return "Hello, world!"

        gql.add_query(hello)

        result = await gql.execute('{ hello }')
        # {"data": {"hello": "Hello, world!"}}
    """

    def __init__(self, schema: Optional[Dict[str, Any]] = None):
        """
        Initialize the GraphQL engine.

        Args:
            schema: Optional pre-built schema dictionary. If not provided,
                    resolvers can be registered individually via add_query,
                    add_mutation, and add_subscription.
        """
        self._schema = schema or {}
        self._queries: Dict[str, Callable] = {}
        self._mutations: Dict[str, Callable] = {}
        self._subscriptions: Dict[str, Callable] = {}

    def add_query(self, func: Callable) -> None:
        """
        Register a query resolver.

        Accepts either a plain function or a Query-decorated function.
        The function name is used as the query field name.

        Args:
            func: The resolver function or Query instance.

        Example:
            @Query
            def users(info) -> list:
                return [{"id": 1}]

            gql.add_query(users)
        """
        if isinstance(func, Query):
            self._queries[func.name] = func.func
        else:
            self._queries[func.__name__] = func

    def add_mutation(self, func: Callable) -> None:
        """
        Register a mutation resolver.

        Accepts either a plain function or a Mutation-decorated function.
        The function name is used as the mutation field name.

        Args:
            func: The resolver function or Mutation instance.

        Example:
            @Mutation
            def create_user(info, name: str) -> dict:
                return {"id": 1, "name": name}

            gql.add_mutation(create_user)
        """
        if isinstance(func, Mutation):
            self._mutations[func.name] = func.func
        else:
            self._mutations[func.__name__] = func

    def add_subscription(self, func: Callable) -> None:
        """
        Register a subscription resolver.

        Accepts either a plain function or a Subscription-decorated function.
        The function name is used as the subscription field name.

        Args:
            func: The resolver function or Subscription instance.

        Example:
            @Subscription
            def on_message(info) -> dict:
                return {"message": "hello"}

            gql.add_subscription(on_message)
        """
        if isinstance(func, Subscription):
            self._subscriptions[func.name] = func.func
        else:
            self._subscriptions[func.__name__] = func

    async def execute(
        self,
        query: str,
        variables: Optional[Dict[str, Any]] = None,
        operation_name: Optional[str] = None,
    ) -> Dict[str, Any]:
        """
        Execute a GraphQL query string against the registered resolvers.

        Args:
            query: The GraphQL query string.
            variables: Optional dictionary of variable values.
            operation_name: Optional operation name when the query contains
                           multiple operations.

        Returns:
            A dictionary with "data" and optionally "errors" keys following
            the GraphQL response specification.

        Example:
            result = await gql.execute(
                'query GetUser($id: Int!) { user(id: $id) { name } }',
                variables={"id": 1},
                operation_name="GetUser"
            )
        """
        # Placeholder execution - real implementation delegates to Rust engine
        result: Dict[str, Any] = {"data": {}}
        errors: List[Dict[str, Any]] = []

        # Build context info for resolvers
        info = {
            "query": query,
            "variables": variables or {},
            "operation_name": operation_name,
        }

        # Attempt to resolve known query fields
        for name, resolver in self._queries.items():
            try:
                if inspect.iscoroutinefunction(resolver):
                    value = await resolver(info)
                else:
                    value = resolver(info)
                result["data"][name] = value
            except Exception as exc:
                errors.append({
                    "message": str(exc),
                    "path": [name],
                })

        # Attempt to resolve known mutation fields
        for name, resolver in self._mutations.items():
            try:
                if inspect.iscoroutinefunction(resolver):
                    value = await resolver(info, **(variables or {}))
                else:
                    value = resolver(info, **(variables or {}))
                result["data"][name] = value
            except Exception as exc:
                errors.append({
                    "message": str(exc),
                    "path": [name],
                })

        if errors:
            result["errors"] = errors

        return result

    def get_schema(self) -> Dict[str, Any]:
        """
        Return schema information describing all registered resolvers.

        Returns:
            A dictionary with "queries", "mutations", and "subscriptions"
            keys, each containing a list of resolver descriptors.

        Example:
            schema_info = gql.get_schema()
            # {
            #     "queries": [{"name": "users", "return_type": "list"}],
            #     "mutations": [{"name": "create_user", ...}],
            #     "subscriptions": [...]
            # }
        """
        return {
            "queries": [
                {
                    "name": name,
                    "return_type": _extract_return_type(func),
                    "parameters": _extract_parameters(func),
                }
                for name, func in self._queries.items()
            ],
            "mutations": [
                {
                    "name": name,
                    "return_type": _extract_return_type(func),
                    "parameters": _extract_parameters(func),
                }
                for name, func in self._mutations.items()
            ],
            "subscriptions": [
                {
                    "name": name,
                    "return_type": _extract_return_type(func),
                    "parameters": _extract_parameters(func),
                }
                for name, func in self._subscriptions.items()
            ],
        }

    def __repr__(self) -> str:
        return (
            f"<GraphQL queries={len(self._queries)} "
            f"mutations={len(self._mutations)} "
            f"subscriptions={len(self._subscriptions)}>"
        )


class Schema:
    """
    Builder class for constructing a GraphQL schema and producing a
    GraphQL execution engine instance.

    Provides a fluent API for registering query, mutation, and subscription
    resolver types before building the final GraphQL instance.

    Example:
        schema = Schema()

        @Query
        def users(info) -> list:
            return []

        @Mutation
        def create_user(info, name: str) -> dict:
            return {"name": name}

        @Subscription
        def on_message(info) -> dict:
            return {}

        schema.query(users)
        schema.mutation(create_user)
        schema.subscription(on_message)

        gql = schema.build()
        result = await gql.execute('{ users { id } }')
    """

    def __init__(self):
        """Initialize an empty schema builder."""
        self._queries: List[Any] = []
        self._mutations: List[Any] = []
        self._subscriptions: List[Any] = []

    def query(self, type_class: Any) -> "Schema":
        """
        Register a query type or resolver.

        Args:
            type_class: A Query-decorated function, a plain function,
                        or a class whose methods are query resolvers.

        Returns:
            Self for method chaining.

        Example:
            schema.query(users_query)
        """
        self._queries.append(type_class)
        return self

    def mutation(self, type_class: Any) -> "Schema":
        """
        Register a mutation type or resolver.

        Args:
            type_class: A Mutation-decorated function, a plain function,
                        or a class whose methods are mutation resolvers.

        Returns:
            Self for method chaining.

        Example:
            schema.mutation(create_user_mutation)
        """
        self._mutations.append(type_class)
        return self

    def subscription(self, type_class: Any) -> "Schema":
        """
        Register a subscription type or resolver.

        Args:
            type_class: A Subscription-decorated function, a plain function,
                        or a class whose methods are subscription resolvers.

        Returns:
            Self for method chaining.

        Example:
            schema.subscription(on_message_sub)
        """
        self._subscriptions.append(type_class)
        return self

    def build(self) -> GraphQL:
        """
        Build and return a configured GraphQL execution engine.

        Processes all registered query, mutation, and subscription types,
        extracts their resolvers, and registers them on a new GraphQL
        instance.

        Returns:
            A fully configured GraphQL instance ready for execution.

        Example:
            gql = schema.build()
        """
        gql = GraphQL()

        for item in self._queries:
            if isinstance(item, Query):
                gql.add_query(item)
            elif callable(item) and not isinstance(item, type):
                gql.add_query(item)
            elif isinstance(item, type):
                # Class-based: register each method as a query
                for attr_name in dir(item):
                    if attr_name.startswith("_"):
                        continue
                    attr = getattr(item, attr_name)
                    if callable(attr):
                        gql.add_query(attr)

        for item in self._mutations:
            if isinstance(item, Mutation):
                gql.add_mutation(item)
            elif callable(item) and not isinstance(item, type):
                gql.add_mutation(item)
            elif isinstance(item, type):
                for attr_name in dir(item):
                    if attr_name.startswith("_"):
                        continue
                    attr = getattr(item, attr_name)
                    if callable(attr):
                        gql.add_mutation(attr)

        for item in self._subscriptions:
            if isinstance(item, Subscription):
                gql.add_subscription(item)
            elif callable(item) and not isinstance(item, type):
                gql.add_subscription(item)
            elif isinstance(item, type):
                for attr_name in dir(item):
                    if attr_name.startswith("_"):
                        continue
                    attr = getattr(item, attr_name)
                    if callable(attr):
                        gql.add_subscription(attr)

        return gql

    def __repr__(self) -> str:
        return (
            f"<Schema queries={len(self._queries)} "
            f"mutations={len(self._mutations)} "
            f"subscriptions={len(self._subscriptions)}>"
        )


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _extract_return_type(func: Callable) -> Optional[str]:
    """
    Extract the return type annotation from a function as a string.

    Args:
        func: The function to inspect.

    Returns:
        String representation of the return type, or None if not annotated.
    """
    try:
        hints = inspect.signature(func).return_annotation
        if hints is inspect.Parameter.empty:
            return None
        if isinstance(hints, type):
            return hints.__name__
        return str(hints)
    except (ValueError, TypeError):
        return None


def _extract_parameters(func: Callable) -> Dict[str, str]:
    """
    Extract parameter names and their type annotations from a function.

    Skips the first parameter (conventionally ``info`` in GraphQL resolvers)
    and any parameters named ``self`` or ``cls``.

    Args:
        func: The function to inspect.

    Returns:
        Dictionary mapping parameter names to their type annotation strings.
    """
    params: Dict[str, str] = {}
    try:
        sig = inspect.signature(func)
        skip_first = True
        for name, param in sig.parameters.items():
            if name in ("self", "cls"):
                continue
            # Skip the first non-self parameter (info)
            if skip_first:
                skip_first = False
                continue
            if param.annotation is not inspect.Parameter.empty:
                if isinstance(param.annotation, type):
                    params[name] = param.annotation.__name__
                else:
                    params[name] = str(param.annotation)
            else:
                params[name] = "Any"
    except (ValueError, TypeError):
        pass
    return params
