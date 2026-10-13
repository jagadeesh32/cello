# Database & Redis (native data layer)

Cello ships a **real** async data layer: a PostgreSQL connection pool
(`deadpool-postgres` + `tokio-postgres`) and a Redis client (the `redis` crate's
connection manager), both implemented in Rust and exposed to Python as
awaitables. The GIL is released for the whole database/Redis round-trip.

There are three ways to use it, smallest to largest:

1. **Raw queries** — `request.database.fetch(...)`, asyncpg-style.
2. **Transactions** — `async with request.database.transaction()` or `@transactional`.
3. **The ORM** — `Model.objects.filter(...)` (see [ORM](#orm)).

> Requires Python 3.12+, a reachable PostgreSQL, and (optionally) Redis.

## Setup

```python
from cello import App, DatabaseConfig, RedisConfig

app = App()
app.enable_database(DatabaseConfig(url="postgresql://user:pass@localhost:5432/mydb", pool_size=10))
app.enable_redis(RedisConfig(url="redis://localhost:6379"))
```

After `enable_database()` / `enable_redis()`:

| Where | Database | Redis |
|-------|----------|-------|
| In `on_event("startup"/"shutdown")` | `app.database` | `app.redis` |
| In a route handler | `request.database` (alias `request.db`) | `request.redis` |

> **Order matters.** Call `enable_database()` / `enable_redis()` **before**
> registering routes. `request.database` / `request.redis` are wired onto the
> request at route-registration time; a route registered *before* the client is
> enabled is left unwrapped (to keep the request path free of an extra Python
> frame) and will raise `AttributeError` when it touches `request.database` /
> `request.redis`. `app.database` / `app.redis` are unaffected and always work.

The pools are created lazily — the first query opens the first connection — and
live on Cello's persistent event loop, so they survive across requests.

## Lifecycle hooks

```python
@app.on_event("startup")
async def startup():
    await app.database.execute("""
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY, name TEXT NOT NULL, email TEXT
        )
    """)
    await app.redis.ping()

@app.on_event("shutdown")
async def shutdown():
    await app.database.close()
    await app.redis.close()
```

## Raw queries

All queries use positional `$1, $2, …` parameters. **Never** interpolate user
input into the SQL string.

```python
@app.get("/users")
async def users(request):
    rows = await request.database.fetch("SELECT * FROM users ORDER BY id")     # list[dict]
    return {"users": rows}

@app.get("/users/{id}")
async def one(request):
    row = await request.database.fetchrow("SELECT * FROM users WHERE id = $1", int(request.params["id"]))
    return {"user": row}                                                        # dict | None

@app.post("/users")
async def create(request):
    data = request.json()
    new_id = await request.database.fetchval(
        "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id",
        data["name"], data.get("email"),
    )
    return {"id": new_id}
```

| Method | Returns |
|--------|---------|
| `fetch(sql, *params)` | `list[dict]` — every row |
| `fetchrow(sql, *params)` | `dict \| None` — first row |
| `fetchval(sql, *params)` | first column of first row (or `None`) |
| `execute(sql, *params)` | `int` — rows affected |
| `transaction()` | an async-context-manager transaction |
| `ping()` / `close()` | connectivity check / close the pool |

### Prepared statements

Every query is prepared through the pooled connection's statement cache. The
first call with a given SQL string does the Parse/Describe round trip; every
later call reuses the server-side statement, so a repeated query costs one round
trip instead of two. Nothing to configure: use `$1` parameters as usual and the
same SQL text is what gets cached (`tokio-postgres` bounds the per-connection
cache and evicts on its own, so SQL built from interpolated values cannot grow
it without limit).

### Type mapping

`bool/int/float/str/bytes` map to their SQL equivalents; `dict`/`list` map to
`json`/`jsonb`. On the way back, `jsonb` becomes a nested Python object,
timestamps/uuid become ISO strings. For `numeric`/`timestamptz` **parameters**,
add an explicit cast — `$1::numeric` — so the value binds as text and Postgres
casts it.

## Transactions

```python
@app.post("/transfer")
async def transfer(request):
    data = request.json()
    async with request.database.transaction() as tx:
        await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", data["amount"], data["from"])
        await tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", data["amount"], data["to"])
    # commits on clean exit; any exception rolls the whole block back
    return {"ok": True}
```

Or the decorator form, which injects a `tx` argument:

```python
from cello.database import transactional

@app.post("/transfer")
@transactional
async def transfer(request, tx):
    await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", 100, 1)
    await tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", 100, 2)
    return {"ok": True}
```

## Redis

```python
@app.get("/cache-demo")
async def demo(request):
    r = request.redis
    await r.set("key", "value", ex=60)          # EX seconds, also px/nx/xx
    v = await r.get("key")
    await r.incr("counter")
    await r.hset("h", "field", "v")
    await r.rpush("q", "a", "b")
    items = await r.lrange("q", 0, -1)
    sha = await r.script_load("return 1")
    await r.evalsha(sha, 0)
    return {"value": v, "list": items}
```

Available: `get/set/setex/delete/exists/expire/ttl/incr/incrby/decr/decrby/mget/`
`keys/hget/hset/hgetall/hdel/lpush/rpush/lpop/rpop/lrange/llen/sadd/srem/smembers/`
`sismember/publish/eval/evalsha/script_load/ping/dbsize/flushdb/close`. Values are
stored as bytes — pass a string (e.g. `json.dumps(obj)`), not a raw Python object.

## ORM

A small built-in ORM built on the pool. Bind it once with `setup(app.database)`.

```python
from cello.orm import Model, AutoField, CharField, BooleanField, IntegerField, ForeignKey, setup

class User(Model):
    id = AutoField()
    name = CharField(max_length=100)
    email = CharField(max_length=255, null=True)
    active = BooleanField(default=True)
    age = IntegerField(null=True)

    class Meta:
        table_name = "users"      # defaults to the lowercased class name

@app.on_event("startup")
async def startup():
    setup(app.database)
    await User.create_table()

@app.get("/users")
async def list_users(request):
    users = await User.objects.filter(active=True).order_by("-age").limit(20)
    return {"users": [u.to_dict() for u in users]}
```

**Fields:** `AutoField, IntegerField, BigIntegerField, FloatField, BooleanField,
CharField(max_length=…), TextField, JSONField, DateTimeField(auto_now_add=…),
ForeignKey(Model, on_delete="CASCADE")`.

**QuerySet** (chainable, then `await`): `filter(**lookups)`, `exclude(**lookups)`,
`order_by("-field")`, `limit(n)`, `offset(n)`. Terminal ops:
`await qs` / `all()`, `get(**kw)`, `first()`, `count()`, `exists()`,
`values(*fields)`, `update(**kw)`, `delete()`. Lookups: `exact` (default), `ne`,
`gt/gte/lt/lte`, `in`, `contains/icontains`, `startswith/istartswith`, `endswith`,
`isnull`.

**Manager** (`Model.objects`): `all/filter/exclude/order_by/get/create/count`, plus
`using(db)` to target a specific pool (e.g. `User.objects.using(request.database)`).

**Instances:** `await user.save()` (insert or update), `await user.delete()`,
`user.to_dict()`. Class: `await Model.create_table()`, `await Model.drop_table()`.

### What the ORM is *not*

This is intentionally the common-80% ORM. It does **not** provide migration
autogeneration/diffing, lazy reverse relations, a `select_related` join planner,
or signals/admin. `ForeignKey` creates the column and constraint but does not add
magic relation attributes — load related rows explicitly (e.g.
`await User.objects.get(id=post.author_id)`). For anything beyond the QuerySet,
drop to raw `request.database` queries, which cover everything.

## Runnable example

See [`examples/database_orm_demo.py`](https://github.com/shuans/cello/blob/main/examples/database_orm_demo.py)
for a complete app using raw queries, transactions, the ORM, and Redis caching
against real servers.
