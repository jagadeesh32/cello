---
title: Database & Redis Integration
description: Configure connection-pooled PostgreSQL and Redis clients in Cello, and use the @transactional decorator for atomic operations.
---

# :material-database-cog: Database & Redis Integration

Cello's data-layer helpers give you production-ready database and cache connectivity through simple configuration objects and a single `enable_*` call. This example wires up a PostgreSQL connection pool, an async Redis client, and demonstrates how the `@transactional` decorator wraps an async handler in a database transaction — so errors automatically trigger a rollback without any try/except boilerplate in your business logic.

## Features Demonstrated

- `DatabaseConfig` — typed configuration for PostgreSQL with pool size and connection lifetime settings
- `RedisConfig` — typed configuration for Redis with pool size control
- `app.enable_database(config)` — initialises the connection pool and makes it available to handlers
- `app.enable_redis(config)` — initialises the async Redis client
- `@transactional` decorator from `cello.database` — wraps an async handler in an atomic DB transaction
- CRUD endpoints for a users resource with proper HTTP status codes (`201`, `404`)
- Key/value cache endpoints backed by an in-memory mock (drop-in for real Redis calls)
- Fund-transfer endpoint showing transactional integrity with balance validation

## Complete Source Code

```python
#!/usr/bin/env python3
"""
Database & Redis Integration Demo for Cello v1.0.1.
Run with: python examples/database_demo.py
Test:
    curl http://127.0.0.1:8000/users
    curl -X POST http://127.0.0.1:8000/users -d '{"name": "Alice", "email": "alice@example.com"}'
    curl http://127.0.0.1:8000/cache/test
    curl -X POST http://127.0.0.1:8000/transfer -d '{"from": 1, "to": 2, "amount": 100}'
"""

from cello import App, Response, DatabaseConfig, RedisConfig
from cello.database import transactional

app = App()

db_config = DatabaseConfig(url="postgresql://user:password@localhost:5432/mydb", pool_size=20, max_lifetime_secs=1800)
redis_config = RedisConfig(url="redis://localhost:6379", pool_size=10)
app.enable_database(db_config)
app.enable_redis(redis_config)
app.enable_cors()
app.enable_logging()

mock_users = [
    {"id": 1, "name": "Alice", "email": "alice@example.com", "balance": 1000},
    {"id": 2, "name": "Bob",   "email": "bob@example.com",   "balance": 500},
    {"id": 3, "name": "Charlie","email":"charlie@example.com","balance": 750},
]
mock_cache = {}
next_id = 4

@app.get("/")
def home(request):
    return {"message": "Cello v1.0.1 - Data Layer Demo",
            "features": {"database": "Connection pooling", "redis": "Async Redis client", "transactions": "@transactional decorator"}}

@app.get("/users")
def list_users(request):
    return {"users": mock_users, "count": len(mock_users)}

@app.get("/users/{id}")
def get_user(request):
    user_id = int(request.params.get("id", 0))
    user = next((u for u in mock_users if u["id"] == user_id), None)
    if user is None:
        return Response.json({"error": "User not found"}, status=404)
    return {"user": user}

@app.post("/users")
def create_user(request):
    global next_id
    data = request.json()
    user = {"id": next_id, "name": data.get("name","Anonymous"), "email": data.get("email",""), "balance": 0}
    mock_users.append(user)
    next_id += 1
    return Response.json({"user": user, "created": True}, status=201)

@app.get("/cache/{key}")
def get_cached(request):
    key = request.params.get("key", "")
    value = mock_cache.get(key)
    if value is None:
        return Response.json({"key": key, "hit": False}, status=404)
    return {"key": key, "value": value, "hit": True}

@app.post("/cache")
def set_cached(request):
    data = request.json()
    key, value = data.get("key",""), data.get("value","")
    if not key:
        return Response.json({"error": "Key required"}, status=400)
    mock_cache[key] = value
    return {"key": key, "stored": True}

@app.post("/transfer")
@transactional
async def transfer_funds(request):
    data = request.json()
    from_id, to_id, amount = int(data["from"]), int(data["to"]), float(data["amount"])
    if amount <= 0:
        return Response.json({"error": "Amount must be positive"}, status=400)
    from_user = next((u for u in mock_users if u["id"] == from_id), None)
    to_user   = next((u for u in mock_users if u["id"] == to_id), None)
    if not from_user or not to_user:
        return Response.json({"error": "User not found"}, status=404)
    if from_user["balance"] < amount:
        return Response.json({"error": "Insufficient funds"}, status=400)
    from_user["balance"] -= amount
    to_user["balance"]   += amount
    return {"success": True, "transfer": {"from": {"id": from_id, "new_balance": from_user["balance"]},
                                           "to": {"id": to_id, "new_balance": to_user["balance"]}, "amount": amount}}

@app.get("/db/status")
def database_status(request):
    return {"database": {"status": "connected", "pool_size": db_config.pool_size}}

@app.get("/redis/status")
def redis_status(request):
    return {"redis": {"status": "connected", "cached_keys": len(mock_cache)}}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

## Running This Example

```bash
python examples/database_demo.py
# Test:
curl http://127.0.0.1:8000/
```

## Key Concepts

- **`DatabaseConfig` / `RedisConfig`** — using typed configuration objects (rather than raw strings) lets Cello validate settings at startup and surface misconfiguration errors before the first request arrives.
- **`pool_size` and `max_lifetime_secs`** — connection pools prevent the overhead of opening a new TCP connection on every request; `max_lifetime_secs` evicts stale connections so the pool stays healthy across long-running processes.
- **`@transactional` on an async handler** — the decorator opens a transaction before the handler body runs and commits on success; any unhandled exception triggers an automatic rollback, keeping your data consistent without explicit `BEGIN` / `COMMIT` / `ROLLBACK` calls.
- **Returning `Response.json(..., status=404)`** — returning a `Response` object directly from a handler bypasses Cello's automatic JSON serialisation, giving you full control over status codes and headers for error cases.
- **`/db/status` and `/redis/status` endpoints** — exposing connection pool metadata as dedicated health endpoints is a common pattern for readiness probes in container orchestration environments.
