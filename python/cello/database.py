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

        SECURITY: Always use parameterized queries with $1, $2, ... placeholders.
        Never use string formatting (f-strings, %, .format()) to build queries,
        as this creates SQL injection vulnerabilities.

        Good:  await db.fetch_all("SELECT * FROM users WHERE id = $1", user_id)
        Bad:   await db.fetch_all(f"SELECT * FROM users WHERE id = {user_id}")

        Args:
            query: SQL query string with $1, $2 placeholders for parameters.
                   Never interpolate user input directly into this string.
            *params: Query parameters (positional, matching $1, $2, ... placeholders).

        Returns:
            List of row dictionaries.
        """
        # Placeholder - real implementation calls Rust pool
        return []

    async def fetch_one(self, query: str, *params) -> Optional[dict]:
        """
        Execute a query and return a single row.

        SECURITY: Always use parameterized queries with $1, $2, ... placeholders.
        Never use string formatting to build queries.

        Args:
            query: SQL query string with $1, $2 placeholders for parameters.
                   Never interpolate user input directly into this string.
            *params: Query parameters (positional, matching $1, $2, ... placeholders).

        Returns:
            Row dictionary or None.
        """
        rows = await self.fetch_all(query, *params)
        return rows[0] if rows else None

    async def execute(self, query: str, *params) -> int:
        """
        Execute a query that doesn't return rows.

        SECURITY: Always use parameterized queries with $1, $2, ... placeholders.
        Never use string formatting to build queries.

        Args:
            query: SQL query string with $1, $2 placeholders for parameters.
                   Never interpolate user input directly into this string.
            *params: Query parameters (positional, matching $1, $2, ... placeholders).

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
        """Execute a query within this transaction.

        SECURITY: Always use parameterized queries ($1, $2, ...).
        Never interpolate user input directly into the query string.
        """
        return await self._db.execute(query, *params)

    async def fetch_all(self, query: str, *params) -> list[dict]:
        """Fetch all rows within this transaction.

        SECURITY: Always use parameterized queries ($1, $2, ...).
        Never interpolate user input directly into the query string.
        """
        return await self._db.fetch_all(query, *params)

    async def fetch_one(self, query: str, *params) -> Optional[dict]:
        """Fetch a single row within this transaction.

        SECURITY: Always use parameterized queries ($1, $2, ...).
        Never interpolate user input directly into the query string.
        """
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

    async def sismember(self, key: str, member: str) -> bool:
        """Check if member exists in a set."""
        return False

    async def publish(self, channel: str, message: str) -> int:
        """Publish a message to a channel."""
        return 0

    async def eval(self, script: str, numkeys: int, *keys_and_args) -> Any:
        """
        Execute a Lua script atomically on the server.

        Args:
            script:        Lua script source.
            numkeys:       Number of key arguments that follow.
            *keys_and_args: KEYS (first ``numkeys`` items) then ARGV.

        Returns:
            Script return value (int, str, list, or None).

        Example::

            result = await r.eval(
                "return redis.call('SET', KEYS[1], ARGV[1])",
                1, "mykey", "myvalue"
            )
        """
        return None

    async def evalsha(self, sha: str, numkeys: int, *keys_and_args) -> Any:
        """
        Execute a previously loaded Lua script by its SHA1 digest.

        Use ``script_load`` to upload the script once and reuse the hash
        to save bandwidth on hot paths.

        Args:
            sha:           SHA1 hex string returned by ``script_load``.
            numkeys:       Number of key arguments that follow.
            *keys_and_args: KEYS then ARGV.

        Returns:
            Script return value.

        Example::

            sha = await r.script_load("return 1")
            result = await r.evalsha(sha, 0)
        """
        return None

    async def script_load(self, script: str) -> str:
        """
        Upload a Lua script to the server and return its SHA1 digest.

        The script is cached server-side; call ``evalsha`` with the
        returned hash to execute it without re-sending the source.

        Args:
            script: Lua script source.

        Returns:
            SHA1 hex string identifying the cached script.

        Example::

            sha = await r.script_load(\"\"\"
                local m = redis.call('SISMEMBER', KEYS[1], ARGV[1])
                if m == 0 then return 0 end
                redis.call('LPUSH', KEYS[2], ARGV[2])
                return 1
            \"\"\")
            result = await r.evalsha(sha, 2, "tokens", "queue", token, data)
        """
        import hashlib
        return hashlib.sha1(script.encode()).hexdigest()

    async def close(self):
        """Close the Redis connection pool."""
        self._client = None
