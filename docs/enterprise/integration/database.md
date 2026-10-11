---
title: Database Integration
description: Native PostgreSQL pool, Redis client and transactions in Cello Framework
---

# Database Integration

Cello ships a native data layer: a PostgreSQL connection pool and an async Redis
client, both implemented in Rust and exposed to Python as awaitables, with
transactions and a built-in ORM on top. This page is an overview; the references
are linked at the end.

## PostgreSQL

### Configuration

```python
from cello import App, DatabaseConfig

app = App()

app.enable_database(DatabaseConfig(
    url="postgresql://user:pass@localhost:5432/mydb",
    pool_size=20,
    sslmode="verify-full",
    sslrootcert="/etc/cello/db-ca.pem",
))
```

| Option | Default | Description |
|--------|---------|-------------|
| `url` | required | Postgres URL or libpq DSN |
| `pool_size` | `10` | Maximum pooled connections |
| `sslmode` | `prefer` | `disable`, `prefer`, `require`, `verify-ca`, `verify-full` |
| `sslrootcert` / `sslcert` / `sslkey` | `None` | PEM files for server verification and client certificates |

Only PostgreSQL is supported. See [`DatabaseConfig`](../../reference/api/database.md#databaseconfig)
for every option.

### Queries

After `enable_database()`, the pool is `app.database` in hooks and
`request.database` in handlers.

```python
@app.get("/users")
async def users(request):
    rows = await request.database.fetch("SELECT * FROM users WHERE active = $1", True)
    return {"users": rows}

@app.get("/users/{id}")
async def one(request):
    return {"user": await request.database.fetchrow(
        "SELECT * FROM users WHERE id = $1", int(request.params["id"])
    )}
```

`fetch`, `fetchrow`, `fetchval`, `execute` and `fetch_json` take positional
`$1, $2, ...` parameters. Database errors are `cello.DatabaseError` subclasses
such as `UniqueViolation`.

### Transactions

```python
from cello.database import transactional

@app.post("/transfer")
@transactional
async def transfer(request, tx):
    await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", 100, 1)
    await tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", 100, 2)
    return {"success": True}
```

The transaction commits when the handler returns and rolls back when it raises.
`async with request.database.transaction() as tx:` does the same for a block.

## Redis

```python
from cello import App, RedisConfig

app = App()

app.enable_redis(RedisConfig(
    url="redis://localhost:6379",
    pool_size=10,
    key_prefix="myapp:",
))

# Local development
app.enable_redis(RedisConfig.local())  # redis://localhost:6379

# Cluster mode
app.enable_redis(RedisConfig.cluster("redis://redis1:6379", pool_size=20))
```

`rediss://` URLs use TLS. In handlers use `request.redis`:

```python
@app.get("/counter")
async def counter(request):
    r = request.redis
    await r.set("key", "value", ex=300)
    n = await r.incr("counter")
    await r.hset("user:1", "name", "Alice")
    await r.lpush("queue", "item1")
    return {"value": await r.get("key"), "count": n}
```

Values are stored as bytes; pass strings, not Python objects.

## API Reference

| Name | Description |
|------|-------------|
| `DatabaseConfig` | Postgres pool configuration (Rust-backed) |
| `RedisConfig` | Redis configuration (Rust-backed) |
| `Database` | The native pool behind `app.database` / `request.database` |
| `Transaction` | The object yielded by `database.transaction()` |
| `transactional` | Decorator for automatic transaction management |
| `DatabaseError` and subclasses | SQLSTATE-mapped database exceptions |

## See Also

- [Database API](../../reference/api/database.md)
- [ORM API](../../reference/api/orm.md)
- [Database & Redis guide](../../data-layer.md)
