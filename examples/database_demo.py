#!/usr/bin/env python3
"""
Database & Redis Integration Demo for Cello v1.0.0.

This example demonstrates the Data Layer features introduced in v0.8.0:
  - Database connection pooling with health monitoring
  - Redis integration with async client
  - Automatic transaction management with @transactional

Run with:
    python examples/database_demo.py

Then test with:
    curl http://127.0.0.1:8000/
    curl http://127.0.0.1:8000/users
    curl -X POST http://127.0.0.1:8000/users -d '{"name": "Alice", "email": "alice@example.com"}'
    curl http://127.0.0.1:8000/cache/test
    curl -X POST http://127.0.0.1:8000/transfer -d '{"from": 1, "to": 2, "amount": 100}'

Author: Jagadeesh Katla
"""

from cello import App, Response, DatabaseConfig, RedisConfig
from cello.database import transactional

app = App()

# =============================================================================
# Database Configuration
# =============================================================================

# Configure database connection pool
db_config = DatabaseConfig(
    url="postgresql://user:password@localhost:5432/mydb",
    pool_size=20,
    max_lifetime_secs=1800,
)

# Configure Redis connection
redis_config = RedisConfig(
    url="redis://localhost:6379",
    pool_size=10,
)

# Enable data layer features
# Note: These require actual database/Redis servers to connect.
# For demo purposes, the framework will print configuration info.
app.enable_database(db_config)
app.enable_redis(redis_config)

# Enable middleware
app.enable_cors()
app.enable_logging()


# =============================================================================
# In-memory mock data (simulates database)
# =============================================================================

mock_users = [
    {"id": 1, "name": "Alice", "email": "alice@example.com", "balance": 1000},
    {"id": 2, "name": "Bob", "email": "bob@example.com", "balance": 500},
    {"id": 3, "name": "Charlie", "email": "charlie@example.com", "balance": 750},
]

mock_cache = {}

next_id = 4


# =============================================================================
# Routes
# =============================================================================


@app.get("/")
def home(request):
    """Root endpoint with feature overview."""
    return {
        "message": "Cello v1.0.0 - Data Layer Demo",
        "features": {
            "database": "Connection pooling with health monitoring & reconnection",
            "redis": "Async Redis client with pool, Pub/Sub, cluster mode",
            "transactions": "Automatic transaction management with @transactional",
        },
        "endpoints": [
            "GET  /              - This overview",
            "GET  /users         - List all users",
            "GET  /users/{id}    - Get user by ID",
            "POST /users         - Create a new user",
            "GET  /cache/{key}   - Get cached value (Redis demo)",
            "POST /cache         - Set cached value (Redis demo)",
            "POST /transfer      - Transfer funds (transaction demo)",
            "GET  /db/status     - Database pool status",
            "GET  /redis/status  - Redis connection status",
        ],
    }


@app.get("/users")
def list_users(request):
    """List all users (simulates SELECT * FROM users)."""
    return {
        "users": mock_users,
        "count": len(mock_users),
        "note": "In production, this queries the database via connection pool",
    }


@app.get("/users/{id}")
def get_user(request):
    """Get a user by ID (simulates SELECT * FROM users WHERE id = $1)."""
    user_id = int(request.params.get("id", 0))
    user = next((u for u in mock_users if u["id"] == user_id), None)

    if user is None:
        return Response.json({"error": "User not found"}, status=404)

    return {"user": user}


@app.post("/users")
def create_user(request):
    """Create a new user (simulates INSERT INTO users)."""
    global next_id
    try:
        data = request.json()
        user = {
            "id": next_id,
            "name": data.get("name", "Anonymous"),
            "email": data.get("email", ""),
            "balance": 0,
        }
        mock_users.append(user)
        next_id += 1
        return Response.json({"user": user, "created": True}, status=201)
    except Exception as e:
        return Response.json({"error": str(e)}, status=400)


# =============================================================================
# Redis Cache Demo
# =============================================================================


@app.get("/cache/{key}")
def get_cached(request):
    """Get a cached value (simulates Redis GET)."""
    key = request.params.get("key", "")
    value = mock_cache.get(key)

    if value is None:
        return Response.json({"key": key, "value": None, "hit": False}, status=404)

    return {"key": key, "value": value, "hit": True}


@app.post("/cache")
def set_cached(request):
    """Set a cached value (simulates Redis SET with optional TTL)."""
    try:
        data = request.json()
        key = data.get("key", "")
        value = data.get("value", "")
        ttl = data.get("ttl")

        if not key:
            return Response.json({"error": "Key is required"}, status=400)

        mock_cache[key] = value
        result = {"key": key, "value": value, "stored": True}
        if ttl:
            result["ttl"] = ttl
            result["note"] = f"In production, key expires in {ttl} seconds"
        return result
    except Exception as e:
        return Response.json({"error": str(e)}, status=400)


# =============================================================================
# Transaction Demo
# =============================================================================


@app.post("/transfer")
@transactional
async def transfer_funds(request):
    """
    Transfer funds between users.

    The @transactional decorator wraps this in a database transaction.
    On success: transaction auto-commits.
    On failure: transaction auto-rollbacks.
    """
    try:
        data = request.json()
        from_id = int(data.get("from", 0))
        to_id = int(data.get("to", 0))
        amount = float(data.get("amount", 0))

        if amount <= 0:
            return Response.json({"error": "Amount must be positive"}, status=400)

        from_user = next((u for u in mock_users if u["id"] == from_id), None)
        to_user = next((u for u in mock_users if u["id"] == to_id), None)

        if from_user is None or to_user is None:
            return Response.json({"error": "User not found"}, status=404)

        if from_user["balance"] < amount:
            return Response.json({"error": "Insufficient funds"}, status=400)

        # These would be database operations in production
        from_user["balance"] -= amount
        to_user["balance"] += amount

        return {
            "success": True,
            "transfer": {
                "from": {"id": from_id, "new_balance": from_user["balance"]},
                "to": {"id": to_id, "new_balance": to_user["balance"]},
                "amount": amount,
            },
            "note": "In production, this runs inside a database transaction",
        }
    except (ValueError, TypeError) as e:
        return Response.json({"error": f"Invalid input: {e}"}, status=400)


# =============================================================================
# Status Endpoints
# =============================================================================


@app.get("/db/status")
def database_status(request):
    """Database connection pool status."""
    return {
        "database": {
            "status": "connected",
            "pool": {
                "url": "postgresql://***@localhost:5432/mydb",
                "pool_size": db_config.pool_size,
                "max_lifetime_secs": db_config.max_lifetime_secs,
            },
            "note": "Pool metrics available via Prometheus at /metrics",
        }
    }


@app.get("/redis/status")
def redis_status(request):
    """Redis connection status."""
    return {
        "redis": {
            "status": "connected",
            "pool": {
                "url": "redis://***@localhost:6379",
                "pool_size": redis_config.pool_size,
            },
            "cached_keys": len(mock_cache),
            "note": "Redis stats available via Prometheus at /metrics",
        }
    }


# =============================================================================
# Configuration Reference
# =============================================================================


@app.get("/config")
def show_config(request):
    """Show available configuration options for Data Layer features."""
    return {
        "DatabaseConfig": {
            "url": "Connection string (postgresql://user:pass@host/db)",
            "pool_size": "Maximum connections in pool (default: 10)",
            "min_idle": "Minimum idle connections to maintain",
            "max_lifetime_secs": "Max lifetime of a connection in seconds",
            "idle_timeout_secs": "Timeout for idle connections",
            "connect_timeout_secs": "Connection attempt timeout",
        },
        "RedisConfig": {
            "url": "Redis connection URL (redis://host:port)",
            "pool_size": "Connection pool size (default: 5)",
            "cluster_mode": "Enable Redis Cluster support",
            "default_ttl": "Default TTL for keys in seconds",
            "tls": "Enable TLS for Redis connection",
            "key_prefix": "Prefix for all keys",
        },
        "transactional": {
            "usage": "@transactional decorator on handler functions",
            "behavior": "Auto-commit on success, auto-rollback on exception",
        },
    }


if __name__ == "__main__":
    print("Cello v1.0.0 - Data Layer Demo")
    print()
    print("  Available endpoints:")
    print("  - GET  /              - Feature overview")
    print("  - GET  /users         - List all users")
    print("  - GET  /users/{id}    - Get user by ID")
    print("  - POST /users         - Create a new user")
    print("  - GET  /cache/{key}   - Get cached value")
    print("  - POST /cache         - Set cached value")
    print("  - POST /transfer      - Transfer funds (transaction demo)")
    print("  - GET  /db/status     - Database pool status")
    print("  - GET  /redis/status  - Redis connection status")
    print("  - GET  /config        - Configuration reference")
    print()
    app.run(host="127.0.0.1", port=8000)
