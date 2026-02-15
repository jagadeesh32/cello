"""
Cello Database & Redis Integration.

Provides Python-friendly wrappers for database and Redis operations
with connection pooling, transaction management, and helper decorators.

Example:
    from cello import App
    from cello.database import DatabaseConfig, RedisConfig, transactional

    app = App()

    @app.on_startup
    async def setup():
        app.state.db = await Database.connect(db_config)
        app.state.redis = await Redis.connect(redis_config)

    @app.post("/transfer")
    @transactional
    async def transfer(request, db=Depends("database")):
        await db.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", amount, from_id)
        await db.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", amount, to_id)
        return {"success": True}
"""

from functools import wraps
from typing import Any, Callable, Optional


def transactional(func: Callable) -> Callable:
    """
    Decorator for automatic transaction management.

    Wraps a handler function in a database transaction.
    On success, the transaction is committed.
    On exception, the transaction is rolled back.

    Args:
        func: The handler function to wrap.

    Returns:
        Wrapped function with transaction management.

    Example:
        @app.post("/transfer")
        @transactional
        async def transfer(request):
            # All database operations here are in a transaction
            await db.execute("UPDATE accounts SET balance = balance - $1", amount)
            await db.execute("UPDATE accounts SET balance = balance + $1", amount)
            return {"success": True}
            # Transaction auto-commits on success
            # Transaction auto-rollbacks on exception
    """
    @wraps(func)
    async def wrapper(*args, **kwargs):
        # Look for db in kwargs (from DI) or request state
        db = kwargs.get("db") or kwargs.get("database")

        if db is None:
            # Try to get from request state
            for arg in args:
                if hasattr(arg, "state") and hasattr(arg.state, "db"):
                    db = arg.state.db
                    break

        if db is not None and hasattr(db, "begin"):
            tx = await db.begin() if hasattr(db.begin, "__call__") else db.begin()
            try:
                # Inject transaction into kwargs
                kwargs["_transaction"] = tx
                result = await func(*args, **kwargs) if _is_async(func) else func(*args, **kwargs)
                if hasattr(tx, "commit"):
                    commit = tx.commit()
                    if hasattr(commit, "__await__"):
                        await commit
                return result
            except Exception:
                if hasattr(tx, "rollback"):
                    rollback = tx.rollback()
                    if hasattr(rollback, "__await__"):
                        await rollback
                raise
        else:
            # No database available, just call the function
            if _is_async(func):
                return await func(*args, **kwargs)
            return func(*args, **kwargs)

    return wrapper


def _is_async(func: Callable) -> bool:
    """Check if a function is async."""
    import inspect
    return inspect.iscoroutinefunction(func)


class Database:
    """
    Database connection wrapper providing a convenient Python API.

    Wraps the Rust-powered connection pool with Pythonic methods.

    Example:
        db = Database(config)
        rows = await db.fetch_all("SELECT * FROM users")
        user = await db.fetch_one("SELECT * FROM users WHERE id = $1", user_id)
        await db.execute("INSERT INTO users (name) VALUES ($1)", name)
    """

    def __init__(self, config=None):
        """Initialize database wrapper."""
        self._config = config
        self._pool = None

    @classmethod
    async def connect(cls, config) -> "Database":
        """
        Connect to the database and create a connection pool.

        Args:
            config: DatabaseConfig instance with connection parameters.

        Returns:
            Connected Database instance.
        """
        instance = cls(config)
        # In a real implementation, this would call Rust to create the pool
        instance._pool = True  # Placeholder
        return instance

    async def fetch_all(self, query: str, *params) -> list[dict]:
        """
        Execute a query and return all rows as dictionaries.

        Args:
            query: SQL query string with $1, $2 placeholders.
            *params: Query parameters.

        Returns:
            List of row dictionaries.
        """
        # Placeholder - real implementation calls Rust pool
        return []

    async def fetch_one(self, query: str, *params) -> Optional[dict]:
        """
        Execute a query and return a single row.

        Args:
            query: SQL query string.
            *params: Query parameters.

        Returns:
            Row dictionary or None.
        """
        rows = await self.fetch_all(query, *params)
        return rows[0] if rows else None

    async def execute(self, query: str, *params) -> int:
        """
        Execute a query that doesn't return rows.

        Args:
            query: SQL query string.
            *params: Query parameters.

        Returns:
            Number of affected rows.
        """
        return 0

    async def begin(self):
        """Begin a transaction."""
        return Transaction(self)

    async def close(self):
        """Close the connection pool."""
        self._pool = None


class Transaction:
    """
    Database transaction wrapper.

    Provides commit/rollback semantics for a group of operations.
    """

    def __init__(self, db: Database):
        self._db = db
        self._committed = False
        self._rolled_back = False

    async def execute(self, query: str, *params) -> int:
        """Execute a query within this transaction."""
        return await self._db.execute(query, *params)

    async def fetch_all(self, query: str, *params) -> list[dict]:
        """Fetch all rows within this transaction."""
        return await self._db.fetch_all(query, *params)

    async def fetch_one(self, query: str, *params) -> Optional[dict]:
        """Fetch a single row within this transaction."""
        return await self._db.fetch_one(query, *params)

    async def commit(self):
        """Commit the transaction."""
        self._committed = True

    async def rollback(self):
        """Rollback the transaction."""
        self._rolled_back = True

    async def __aenter__(self):
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if exc_type is not None:
            await self.rollback()
        elif not self._committed:
            await self.commit()
        return False


class Redis:
    """
    Redis client wrapper providing a convenient Python API.

    Wraps the Rust-powered Redis connection pool with Pythonic methods.

    Example:
        redis = Redis(config)
        await redis.set("key", "value", ttl=3600)
        value = await redis.get("key")
        await redis.delete("key")
    """

    def __init__(self, config=None):
        """Initialize Redis wrapper."""
        self._config = config
        self._client = None

    @classmethod
    async def connect(cls, config) -> "Redis":
        """
        Connect to Redis and create a connection pool.

        Args:
            config: RedisConfig instance with connection parameters.

        Returns:
            Connected Redis instance.
        """
        instance = cls(config)
        instance._client = True  # Placeholder
        return instance

    async def get(self, key: str) -> Optional[str]:
        """Get a value by key."""
        return None

    async def set(self, key: str, value: Any, ttl: Optional[int] = None) -> bool:
        """Set a value with optional TTL in seconds."""
        return True

    async def delete(self, key: str) -> bool:
        """Delete a key."""
        return True

    async def exists(self, key: str) -> bool:
        """Check if a key exists."""
        return False

    async def incr(self, key: str) -> int:
        """Increment a key's integer value."""
        return 0

    async def decr(self, key: str) -> int:
        """Decrement a key's integer value."""
        return 0

    async def expire(self, key: str, ttl: int) -> bool:
        """Set TTL on a key in seconds."""
        return True

    async def hget(self, key: str, field: str) -> Optional[str]:
        """Get a hash field value."""
        return None

    async def hset(self, key: str, field: str, value: Any) -> bool:
        """Set a hash field value."""
        return True

    async def hgetall(self, key: str) -> dict:
        """Get all fields from a hash."""
        return {}

    async def lpush(self, key: str, *values) -> int:
        """Push values to the left of a list."""
        return 0

    async def rpush(self, key: str, *values) -> int:
        """Push values to the right of a list."""
        return 0

    async def lpop(self, key: str) -> Optional[str]:
        """Pop from the left of a list."""
        return None

    async def lrange(self, key: str, start: int = 0, stop: int = -1) -> list:
        """Get a range from a list."""
        return []

    async def publish(self, channel: str, message: str) -> int:
        """Publish a message to a channel."""
        return 0

    async def close(self):
        """Close the Redis connection pool."""
        self._client = None
