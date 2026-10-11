# Database & Redis (native data layer)

Cello ships a **real** async data layer: a PostgreSQL connection pool
(`deadpool-postgres` + `tokio-postgres`) and a Redis client (the `redis` crate's
connection manager), both implemented in Rust and exposed to Python as
awaitables. The GIL is released for the whole database/Redis round-trip.

There are three ways to use it, smallest to largest:

1. **Raw queries** — `request.database.fetch(...)`.
2. **Transactions** — `async with request.database.transaction()` or `@transactional`.
3. **The ORM** — `Model.objects.filter(...)` (see [ORM](#orm)).

> Requires Python 3.12+, a reachable PostgreSQL, and (optionally) Redis.

References: [ORM API](reference/api/orm.md) and [Database API](reference/api/database.md).

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

The pools are created lazily — the first query opens the first connection — and
live on Cello's persistent event loop, so they survive across requests.

## TLS (encrypted connections)

Postgres connections — the request pool and the `cello migrate` session — speak
TLS through rustls, the same TLS stack the HTTP server and `rediss://` Redis
use. TLS is controlled by libpq's `sslmode`, set in the URL or on
`DatabaseConfig` (the `DatabaseConfig` option wins when both are given):

```python
# Recommended for production: encrypted, certificate chain and hostname verified.
app.enable_database(DatabaseConfig(
    url="postgresql://app:secret@db.internal:5432/app",
    sslmode="verify-full",
    sslrootcert="/etc/cello/db-ca.pem",   # your server's CA
))

# The same thing in the URL (also works for `cello migrate --dsn ...`):
"postgresql://app:secret@db.internal/app?sslmode=verify-full&sslrootcert=/etc/cello/db-ca.pem"
```

| `sslmode` | Encrypted | Certificate chain checked | Hostname checked | Protects against |
|-----------|-----------|---------------------------|------------------|------------------|
| `disable` | no | – | – | nothing |
| `prefer` *(default)* | if the server supports TLS, else plaintext | no | no | passive eavesdropping, only when the server offers TLS |
| `require` | yes (fails if the server has no TLS) | no | no | passive eavesdropping |
| `verify-ca` | yes | yes | no | eavesdropping and impostors without a cert from your CA |
| `verify-full` | yes | yes | yes | eavesdropping and impersonation (**use this in production**) |

- **The default is `prefer`**, as in libpq. Code that worked before keeps
  working, and a server with TLS enabled now gets an encrypted connection. A
  server without TLS still gets a plaintext one. Because `prefer` does not
  verify anything, an active attacker can downgrade it or impersonate the
  server.
- **`require` encrypts but does not authenticate the server.** Anyone who can
  intercept the TCP connection can present their own certificate. (When the
  server uses `scram-sha-256` password auth, SCRAM channel binding still makes
  that interception fail. With `md5`/`password` auth it does not.) Use
  `require` only where you trust the network path, or as a stepping stone to
  `verify-full`.
- As in libpq, `require` together with an explicit `sslrootcert` behaves like
  `verify-ca`.
- `allow` is accepted and treated as `prefer`.

Other options (URL query keys or `DatabaseConfig` arguments):

| Option | Meaning |
|--------|---------|
| `sslrootcert` | PEM file with the CA certificate(s) to trust. When given, **only** these CAs are trusted (libpq behaviour). When omitted, or set to `system`, the bundled Mozilla root store (`webpki-roots`) is used. That suits managed databases whose certificates chain to a public CA. |
| `sslcert` / `sslkey` | PEM client certificate (chain) and private key (PKCS#8, PKCS#1 or SEC1) for `cert` authentication or `clientcert=verify-full` in `pg_hba.conf`. Both must be given. |
| `channel_binding` | `disable` / `prefer` (default) / `require`. Passed to the driver. `require` refuses to authenticate unless SCRAM-SHA-256-PLUS (channel binding to the TLS certificate) is used. |

Certificate files are read when `enable_database()` runs, so a missing file,
non-PEM content or an invalid `sslmode` raises `ValueError` at startup rather
than on the first request. Handshake failures are raised as
`cello.OperationalError` from the first query, with the reason, for example:

```
error performing TLS handshake: invalid peer certificate: NotValidForName
error performing TLS handshake: invalid peer certificate: UnknownIssuer
error performing TLS handshake: server does not support TLS
```

`app.database.sslmode` reports the effective mode.

**Unix sockets** (`host=/var/run/postgresql`) never use TLS: the server does not
offer it there, and `sslmode` is ignored, as in libpq. Hosts given as IP
addresses are checked against the certificate's IP SANs under `verify-full`.

**Differences from libpq:** there is no implicit `~/.postgresql/root.crt`,
`postgresql.crt` or `postgresql.key` lookup (pass the paths explicitly). There
are no CRLs or `sslpassword`, and private keys must be unencrypted. With
`prefer`, if the server accepts TLS but the handshake itself fails, the
connection is not retried in plaintext.

**Cost.** The handshake runs once per pooled connection, on the cold path.
Measured on localhost (aarch64), it added about 7 ms to opening a connection
(11.1 → 18.7 ms p50, SCRAM auth included). Once the connection is open, the
encryption adds about 0–6 % to the p50 of `SELECT 1` (≈0.4–0.6 ms bridge round
trip) and about 7–8 % to the p50 of a 1,000-row (~100 KB) fetch.

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
| `fetch_json(sql, *params)` | `cello.RawJSON` — every row, pre-serialized (see below) |
| `transaction()` | an async-context-manager transaction |
| `ping()` / `close()` | connectivity check / close the pool |

### Raw-SQL fast path: `fetch_json`

When the rows go straight into the response, `fetch_json` skips Python
objects entirely:

```python
@app.get("/report")
async def report(request):
    rows = await request.database.fetch_json(
        "SELECT category, count(*) AS n FROM books WHERE active = $1 GROUP BY category",
        True,
    )
    return {"report": rows}
```

The rows are encoded to JSON on the I/O thread, without the GIL, straight from
the Postgres wire format. The result is a `cello.RawJSON` that the response
writes verbatim, wherever it sits in the returned dict or list. The bytes are
exactly those of returning `await db.fetch(...)`: each row is an object keyed
by column name (a repeated column name keeps its first position and its last
value), with the same value shapes. `fetch_json` is also on transactions
(`await tx.fetch_json(...)`) and runs inside them. Use `fetch` when you need
the rows as Python objects.

### Type mapping

`bool/int/float/str/bytes` map to their SQL equivalents; `dict`/`list` map to
`json`/`jsonb` (a `list`/`tuple` bound to an array parameter such as
`= ANY($1)` becomes a Postgres array). Parameters are bound by the type Postgres
reports for each `$n`: `numeric` takes `Decimal`, `int`, `float` or `str`;
`timestamptz`, `timestamp`, `date`, `time` and `uuid` take Python objects or ISO
strings; an out-of-range integer raises `ParameterRangeError` and a value that
cannot bind raises `ParameterTypeError` before the query is sent. On the way
back, `numeric` is an exact decimal string, `jsonb` is a nested Python object,
arrays are lists, and timestamps/uuid are strings. The full tables are in the
[Database API](reference/api/database.md#sql-parameters) and the
[ORM value shapes](reference/api/orm.md#value-shapes).

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

**ORM calls inside a transaction use it.** Inside `async with db.transaction()`
or a `@transactional` handler, ORM calls that do not pass `using=` run on the
open transaction, so they commit or roll back with the block. Tasks started
inside the block inherit it; other requests never see it. An explicit
`.using(request.database)` still goes to the pool.

```python
async with request.database.transaction():
    order = await Order.objects.create(user=user, total=total)   # in the transaction
    await Stock.objects.filter(sku=sku).update(reserved=True)     # same transaction
```

**A caught error still aborts the transaction.** Postgres aborts a transaction
when any statement in it fails. If you catch the error and leave the block
normally, nothing is committed, and Cello raises `cello.TransactionAborted`
instead of letting the rollback go unnoticed. Let the error propagate, or wrap
the statement that may fail in a `SAVEPOINT` / `ROLLBACK TO SAVEPOINT`.

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
    # Return the queryset: Cello runs it and writes the rows straight into the
    # response (see "Returning rows" below).
    return {"users": User.objects.filter(active=True).order_by("-age").limit(20)}
```

### Returning rows: the fast shape

Return the queryset itself — on its own, or anywhere inside the dict / list
you return:

```python
@app.get("/users")
async def list_users(request):
    return {"users": User.objects.filter(active=True).order_by("-age")}
```

Cello runs the query after your handler returns and encodes the rows to JSON
directly from the Postgres wire format, off the GIL. No Python object is
created for any row or value. The response bytes are exactly those of
`{"users": [u.to_dict() for u in await qs]}` (same keys, same value shapes,
`write_only` fields left out). Several querysets in one response run
concurrently.

Because the query runs *after* the handler returns:

- it runs on the pool (or the queryset's `using()` database), outside any
  transaction the handler had open;
- a database error becomes a `500`, and your handler cannot catch it.

When either matters, await the rows inside the handler with `json()`. It
takes the same native path and returns a `cello.RawJSON` (pre-serialized JSON
that the response writes verbatim):

```python
@app.get("/users")
@transactional
async def list_users(request, tx):
    rows = await User.objects.filter(active=True).json()   # inside the transaction
    return {"users": rows}
```

When you need the rows as objects, `await` the queryset as before. Instances
are built natively from the wire rows and `to_dict()` is native, so
`[u.to_dict() for u in users]` is faster than in 1.3 too, but every value still
becomes a Python object. A model instance (or a list of them) can also be
returned directly; it is written as its `to_dict()`.

A model that overrides `to_dict()` or `_from_row()`, or shadows a field with a
`@property`, is encoded from real instances instead: same output, without the
speedup.

**Fields:** `AutoField, IntegerField, BigIntegerField, FloatField, BooleanField,
CharField(max_length=…), TextField, JSONField, DateTimeField(auto_now_add=…, auto_now=…),
ForeignKey(Model, on_delete="CASCADE")`. Every field takes `null`, `default`,
`unique`, `primary_key` and `db_column`.

**Defaults** are applied when the instance is built: `User(name="x").active` is
already `True`. A callable default is called for each instance, and a
`dict` / `list` default is copied. Passing `None` explicitly stores `NULL`
(leave the argument out to get the default). `auto_now_add=True` sets the
column to the database's `now()` on insert; `auto_now=True` sets it on every
insert and `save()`.

**Foreign keys:** the column is `<name>_id` (or `db_column`), typed like the
target's primary key. `book.author_id` is always the id; `book.author` is what
you assigned (an instance or an id), or the id on loaded rows. Assigning
`book.author = other` updates `author_id`, and `save()` persists it.
`filter(author=obj)`, `update(author=obj)` and `create(author=obj)` use the
instance's primary key. The target model must have a primary key.

**QuerySet** (chainable, then `await`): `filter(*q, **lookups)`,
`exclude(*q, **lookups)`, `order_by("-field")`, `limit(n)`, `offset(n)`,
`select_related(...)`, `prefetch_related(...)`, `values(*fields)` (then
`annotate(...)`). Terminal ops: `await qs` / `all()`, `json()`,
`get(*q, **kw)`, `first()`, `count()`, `exists()`, `aggregate(...)`,
`update(**kw)`, `delete()`. Lookups: `exact` (default), `ne`,
`gt/gte/lt/lte`, `in`, `contains/icontains`, `startswith/istartswith`,
`endswith/iendswith`, `isnull`.

- Field names must be declared on the model (a ForeignKey's `<name>_id` and any
  `db_column` count). Anything else raises `cello.orm.FieldError`; query
  undeclared columns with `await db.fetch(sql, *params)`.
- `filter(x=None)` is `x IS NULL`; `exclude(x=None)` is `x IS NOT NULL`.
- `x__in` takes any iterable except a `str` / `bytes` (a `TypeError`).
- Pattern lookups match `%`, `_` and `\` literally.
- `exclude(a=1, b=2)` excludes rows where *both* hold, and keeps rows where the
  condition is unknown because a column is NULL.
- A second `order_by()` replaces the first.
- `count()` / `exists()` respect `limit()` / `offset()`; `update()` / `delete()`
  on a sliced queryset raise `TypeError`; `update()` with no fields returns `0`.
- An int outside an integer column's range raises `ValueError` instead of
  wrapping.

**Manager** (`Model.objects`): `all/filter/exclude/order_by/get/create/count`,
`select_related/prefetch_related/values/aggregate`, `bulk_create/bulk_update`, plus
`using(db)` to target a specific pool (e.g. `User.objects.using(request.database)`).

**Instances:** `await user.save()`, `await user.delete()`, `user.to_dict()`.
`save()` inserts when the primary key is `None`; otherwise it updates the row
and inserts it if no row has that key. `Model.objects.create(...)` always
inserts, including with an explicit primary key. Database-generated values
(`AutoField`, `auto_now*`) are read back after the write. Instances are equal
when they are the same model with the same non-`None` primary key.
`delete()` on a model without a primary key raises `ValueError`.
Class: `await Model.create_table()`, `await Model.drop_table()`.

### Q objects: OR and NOT

`Q(**lookups)` holds the same lookups as `filter()`. Combine with `|` (OR),
`&` (AND) and `~` (NOT), nest freely, and pass to `filter()`, `exclude()` and
`get()` alongside keyword lookups (everything is ANDed):

```python
from cello.orm import Q

Book.objects.filter(Q(title__istartswith="the") | Q(pages__gt=500), active=True)
Book.objects.filter(~Q(author=None) & (Q(category="sci") | Q(category="art")))
await Book.objects.get(Q(isbn=code) | Q(slug=code))
```

**NULL semantics.** Lookups are SQL comparisons, so a comparison with a NULL
column is *unknown*, and a row is returned only when the whole condition is
*true*. `~q` means "`q` is not true" (`(q) IS NOT TRUE`): it keeps rows where
`q` is false **or unknown**. That is exactly `exclude()`:
`filter(~Q(pages=5))` sends the same SQL as `exclude(pages=5)` and keeps rows
where `pages` is NULL. Add `Q(pages__isnull=False)` to drop them. `exclude(q)`
is `filter(~q)`. An empty `Q()` adds no condition, and inside `|` / `&` it is
dropped. Field names in a `Q` go through the same registry check as keyword
lookups (`FieldError` for anything undeclared).

### Aggregates and GROUP BY

```python
from cello.orm import Count, Sum, Avg, Min, Max

await Book.objects.filter(active=True).aggregate(n=Count("id"), total=Sum("pages"))
# {"n": 42, "total": 12950}
await Book.objects.aggregate(Avg("price"))            # {"price__avg": 12.5}
await Book.objects.aggregate(authors=Count("author", distinct=True))

await (Book.objects.values("category")
       .annotate(n=Count("id"), longest=Max("pages"))
       .order_by("-n"))
# [{"category": "sci", "n": 12, "longest": 900}, ...]
```

- `aggregate()` returns one dict. A positional aggregate is named
  `<field>__<function>`; `Count()` (or `Count("*")`) counts rows and is named
  `count`. `distinct=True` works on every aggregate. `filter`/`exclude` select
  the rows; `order_by` + `limit`/`offset` aggregate just that slice.
- `values(...).annotate(...)` groups by the `values()` fields (`GROUP BY`) and
  adds one column per alias. `order_by()` takes the aliases as well as fields.
  `count()` counts groups. The rows are written natively when returned from a
  handler, like a queryset (same bytes as the awaited list).
- `values()` now returns a queryset of dicts. `await qs.values(...)` works as
  before, and it can also be chained or returned from a handler.
- Aliases must match `^[A-Za-z_][A-Za-z0-9_]*$` and are quoted. Field names go
  through the registry. A bad alias or field raises `FieldError` before any SQL.

**Value shapes** are the Postgres result types, decoded like any column:

| Aggregate | Column type | Python value |
|---|---|---|
| `Count` | any | `int` (`0` over no rows) |
| `Sum` | `INTEGER` / `SMALLINT` | `int` (Postgres returns `bigint`) |
| `Sum` | `BIGINT` | `str`: Postgres returns `numeric`, which the value layer gives as a decimal string (`"45"`) |
| `Avg` | any integer | `str`: `numeric` (`"4.5000000000000000"`) |
| `Sum` / `Avg` | `DOUBLE PRECISION` | `float` |
| `Min` / `Max` | any | the column's own type |

Every aggregate except `Count` is `None` over no rows.

### Relations

Each ForeignKey gives its target model a **reverse accessor**:
`author.book_set` (the source model's name, lowercased, plus `_set`), or the
name you pass as `ForeignKey(Author, related_name="books")`. It returns a
queryset of the rows that point at that instance:

```python
books = await author.books.filter(active=True).order_by("title")
n = await author.books.count()
```

**`select_related`** loads ForeignKey targets in the same query, with one
`LEFT JOIN` per ForeignKey:

```python
for book in await Book.objects.select_related("author__publisher").filter(active=True):
    print(book.title, book.author.name, book.author.publisher.name)
```

`book.author` becomes an `Author` instance (`book.author_id` keeps the id).
`__` follows further ForeignKeys. A NULL ForeignKey leaves `None`. On a native
database, rows are hydrated natively from the joined result, one model segment
at a time, so equal column names in different tables never mix. Any other
database, or a model that overrides `_from_row`, gets the same instances with
one query per relation.

**`prefetch_related`** runs one extra query per relation
(`WHERE <key> = ANY($1)` with every key of the batch). Names can be reverse
accessors or ForeignKeys:

```python
authors = await Author.objects.prefetch_related("books", "publisher")
for a in authors:
    titles = [b.title for b in await a.books]   # no query: already loaded
```

After a prefetch, `a.books` holds the loaded rows. Awaiting it runs no query,
and each book's `author` is that `a`. Chaining (`a.books.filter(...)`) runs a
new query. Nested paths work too: `prefetch_related("books__publisher")`.

Rules:

- `to_dict()` does not change: it writes `author_id`, never a nested object.
  So a returned queryset with `select_related` / `prefetch_related` takes the
  native JSON path on the base query alone (same bytes, no JOIN). If you need
  nested output, build the dicts yourself from the loaded instances.
- Reverse accessors are installed for ForeignKeys declared on the model
  itself. Inherited ForeignKeys get none.
- An explicit `related_name` that clashes with a field, an attribute or
  another accessor raises `ValueError` when the class is defined. When two
  ForeignKeys produce the same *default* name (two ForeignKeys from one model
  to the same target), that accessor raises `AttributeError` asking for a
  `related_name`. A default name that clashes with a field or attribute is not
  installed.
- A ForeignKey whose `db_column` equals its own name stores the id under the
  field name, so `select_related` / `prefetch_related` reject it with a
  `FieldError`.

### Bulk operations

```python
books = await Book.objects.bulk_create([Book(title=t) for t in titles])
for b in books:
    b.pages += 1
n = await Book.objects.bulk_update(books, ["pages"])
```

- `bulk_create(objs, batch_size=None, using=None)` sends one multi-row
  `INSERT ... VALUES (...), (...) RETURNING *` per batch and fills each
  instance from its row: primary keys, `auto_now` / `auto_now_add`,
  database defaults. Values are written the same way as `create()`. Constructor
  defaults are already applied, an `AutoField` left as `None` gets the
  database's value, and an explicit primary key is inserted as given. Rows come
  back in input order (Postgres returns a multi-row `INSERT ... RETURNING` in
  `VALUES` order).
- `bulk_update(objs, fields, batch_size=None, using=None)` sends one
  `UPDATE ... SET ... FROM (VALUES ...)` per batch and returns the number of
  rows updated. Only `fields` are written, plus `auto_now` fields, which are
  set to `now()` and read back, as `save()` does. Every instance needs a
  primary key, and each may appear once. Parameters are cast to the column
  types from the registry, without length modifiers, so a too-long string
  raises instead of being truncated.
- Batches are cut so no statement exceeds Postgres's 65,535 bind parameters.
  `batch_size` can lower that further. When there are several batches on a
  pool, they run in one transaction, so the call is all-or-nothing. Inside a
  transaction they use it.
- Neither calls `save()` or any per-instance hook.

Measured on one box (local Postgres, alternated rounds, medians): 1,000 rows
with `bulk_create` took **44–65 ms**. A loop of `create()` took **498 ms**
inside one transaction and **2,082 ms** in autocommit. Loading 200 books with
their authors took **1.9 ms** with `select_related` and **2.2 ms** with
`prefetch_related`, against **116 ms** for the N+1 loop.

### What the ORM is *not*

This is intentionally a small ORM. It does **not** provide many-to-many fields,
lookups across relations (`filter(author__name=...)`), `HAVING` filters on
annotations, per-object `annotate()` without `values()`, signals or an admin.
Migrations are experimental (`python -m cello migrate --experimental`). For anything
beyond the QuerySet, drop to raw `request.database` queries, with
`fetch_json` when the rows go straight to the response.

## Runnable example

See [`examples/database_orm_demo.py`](https://github.com/jagadeesh32/cello/blob/main/examples/database_orm_demo.py)
for a complete app using raw queries, transactions, the ORM, and Redis caching
against real servers.
