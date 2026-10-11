---
title: Database API
description: Reference for Cello's native Postgres layer - DatabaseConfig, Database, Transaction, @transactional, SQL parameter binding and the DatabaseError hierarchy
---

# Database API

Cello's Postgres layer is a native connection pool (`deadpool-postgres` over
`tokio-postgres`) exposed to Python as awaitables. The GIL is released for the
whole round trip. This page is the reference; for a guided introduction see
[Database & Redis](../../data-layer.md), and for models see the [ORM API](orm.md).

!!! info "Verified"
    Every signature, value and error on this page was observed by running
    Cello 1.4.0 against a live Postgres server.

---

## Enabling the database

```python
from cello import App, DatabaseConfig

app = App()
app.enable_database(DatabaseConfig(
    url="postgresql://user:pass@localhost:5432/mydb",
    pool_size=10,
    sslmode="verify-full",
    sslrootcert="/etc/cello/db-ca.pem",
))
```

`enable_database(config)` requires a `DatabaseConfig` (calling it with no
argument raises `ValueError`). The pool is created lazily: the first query opens
the first connection. Afterwards:

| Where | Handle |
|-------|--------|
| Startup / shutdown hooks | `app.database` |
| Route handlers | `request.database` (alias `request.db`) |

### `DatabaseConfig`

```python
DatabaseConfig(url, pool_size=10, min_idle=1, max_lifetime_secs=1800,
               connection_timeout_secs=5, idle_timeout_secs=300,
               application_name=None, sslmode=None, sslrootcert=None,
               sslcert=None, sslkey=None)
```

| Option | Type | Default | Meaning |
|--------|------|---------|---------|
| `url` | `str` | required | `postgresql://user:pass@host:port/db`, optionally with libpq query keys (`?sslmode=...`), or a libpq `key=value` DSN. |
| `pool_size` | `int` | `10` | Maximum pooled connections (minimum 1). |
| `sslmode` | `str \| None` | `None` (= `prefer`) | `disable`, `prefer`, `require`, `verify-ca`, `verify-full`. Overrides an `sslmode` in the URL. `allow` is treated as `prefer`. An unknown value raises `ValueError` from `enable_database()`. |
| `sslrootcert` | `str \| None` | `None` | PEM file of CA certificates to trust. When given, only these are trusted. When omitted, or `"system"`, the bundled Mozilla roots are used. |
| `sslcert` | `str \| None` | `None` | PEM client certificate (chain). Requires `sslkey`. |
| `sslkey` | `str \| None` | `None` | PEM private key (PKCS#8, PKCS#1 or SEC1), unencrypted. |
| `min_idle`, `max_lifetime_secs`, `connection_timeout_secs`, `idle_timeout_secs`, `application_name` | | see signature | Accepted and stored on the config, but **not applied by the native Postgres pool in 1.4.0** (observed: `show application_name` is empty with `application_name="zz"`). Only `url`, `pool_size` and the `ssl*` options take effect. |

There is also a convenience constructor,
`DatabaseConfig.postgres(host, port=5432, database="postgres", user="postgres", password=None, pool_size=10, sslmode=None, sslrootcert=None)`,
which builds the URL for you.

**TLS.** The default is `prefer`: encrypted when the server supports TLS,
plaintext otherwise, with no certificate checks. Use `sslmode="verify-full"` in
production. `require` encrypts but does not authenticate the server; `require`
with an explicit `sslrootcert` behaves like `verify-ca`. Unix-socket hosts never
use TLS. The `channel_binding` libpq key (`disable`/`prefer`/`require`) is
accepted in the URL. The full option table, trade-offs and handshake errors are in
[Database & Redis: TLS](../../data-layer.md#tls-encrypted-connections).

Certificate files are read when `enable_database()` runs, so a missing file or
`sslcert` without `sslkey` raises `ValueError` at startup. Observed:

```text
ValueError: invalid sslmode "bogus": expected one of disable, prefer, require, verify-ca, verify-full
ValueError: sslcert was given without sslkey
ValueError: cannot read sslrootcert "/nope.pem": No such file or directory (os error 2)
```

!!! note "Certificates are only read when TLS is required"
    With the default `prefer` mode, a bad `sslrootcert` path is **not** reported
    (it is only read for `require`, `verify-ca` and `verify-full`). A bad
    `sslcert` / `sslkey` path is reported in every mode.

`app.database.sslmode` reports the effective mode (after defaults, the
`require` + `sslrootcert` upgrade, and Unix sockets), and `enable_database()`
prints it in its startup banner:

```text
Database pool enabled (native deadpool-postgres):
   Pool size: 10
   TLS:       sslmode=prefer
```

---

## `Database`

The object behind `app.database` and `request.database`. Import it from `cello`
for type hints only; you do not construct it yourself.

| Member | Returns | Description |
|--------|---------|-------------|
| `await fetch(sql, *params)` | `list[dict]` | Every row. |
| `await fetchrow(sql, *params)` | `dict \| None` | The first row, or `None`. Further rows are discarded. |
| `await fetchval(sql, *params)` | any | The first column of the first row, or `None`. Further rows are discarded. |
| `await execute(sql, *params)` | `int` | Rows affected. |
| `await fetch_json(sql, *params)` | `cello.RawJSON` | Every row, pre-serialized. See below. |
| `transaction()` | async context manager | Starts a [transaction](#transactions). |
| `await ping()` | `True` | Acquires a connection and runs `SELECT 1`. Raises `OperationalError` when the server is unreachable. |
| `await close()` | | Closes the pool. |
| `sslmode` | `str` | The effective TLS mode. |
| `dsn` | `str` | The connection string, **including the password**. Do not log it. |

SQL uses positional `$1, $2, ...` parameters. Never interpolate user input into
the SQL string.

```python
@app.get("/users/{id}")
async def one(request):
    row = await request.database.fetchrow(
        "SELECT id, name FROM users WHERE id = $1", int(request.params["id"])
    )
    return {"user": row}
```

- `fetchrow` / `fetchval` return the first row of a multi-row result instead of
  raising. The remaining rows are still produced and sent by the server, so add
  `LIMIT 1` for large results. An error the server raises after the first row is
  not reported.
- A repeated column name collapses to one key (the last value).
- Each connection caches up to 512 prepared statements. After a schema change,
  the pool clears the caches and retries once (invisible to the caller). Inside a
  transaction the retry is not possible, so the error is raised with a message
  asking you to retry the transaction.
- A pooler in *transaction* pooling mode conflicts with prepared statements. Use
  session pooling.

### `fetch_json(sql, *params)`

Encodes the rows to JSON on the I/O thread without the GIL and returns a
`cello.RawJSON` that the response writes verbatim, anywhere in the returned dict
or list. The bytes equal returning `await db.fetch(...)`. Use `fetch` when you
need Python objects.

```python
@app.get("/report")
async def report(request):
    rows = await request.database.fetch_json(
        "SELECT active, count(*) AS n FROM users GROUP BY active"
    )
    return {"report": rows}
```

### Transaction members

`Transaction` (the object `transaction()` yields) has these members:

| Member | Returns |
|--------|---------|
| `await execute(sql, *params)` | `int` |
| `await fetch(sql, *params)` | `list[dict]` |
| `await fetchrow(sql, *params)` | `dict \| None` |
| `await fetch_json(sql, *params)` | `cello.RawJSON` |

!!! warning "No `fetchval` on `Transaction`"
    In 1.4.0 the native `Transaction` class has `execute`, `fetch`, `fetchrow`
    and `fetch_json` but **no `fetchval`** (`hasattr(Transaction, "fetchval")`
    is `False`). Use `(await tx.fetchrow(sql, ...))` and read the column, or
    run the scalar query on the pool. `Database` has `fetchval`.

---

## Transactions

```python
async with request.database.transaction() as tx:
    await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", 100, 1)
    await tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", 100, 2)
# COMMIT on a clean exit; any exception rolls the whole block back
```

A transaction holds one pooled connection for its lifetime (`BEGIN` /
`COMMIT` / `ROLLBACK` are explicit). Statements sent through it are tracked so
that a swallowed failure is detected.

### `@transactional`

```python
from cello.database import transactional   # also: from cello import transactional

@app.post("/transfer")
@transactional
async def transfer(request, tx):
    await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", 100, 1)
    return {"ok": True}
```

- The handler must be `async def`, otherwise `transactional(func)` raises
  `TypeError("@transactional requires an 'async def' handler; ...")`.
- A `tx` keyword argument is injected. The transaction commits when the handler
  returns and rolls back when it raises.
- It needs `app.enable_database(...)`; otherwise it raises `RuntimeError`.

### `current_transaction()`

`cello.database.current_transaction()` returns the transaction opened by the
innermost enclosing `async with db.transaction()` / `@transactional` in the
current task, or `None`. It is stored in a `contextvars` variable: tasks created
inside the block inherit it, and concurrent requests never see each other's.
ORM calls without `using=` run on it (see [ORM transactions](orm.md#transactions-and-the-ambient-transaction)).

### `TransactionAborted`

A `RuntimeError` subclass (`cello.TransactionAborted`). When a statement inside
the block fails and you **catch the error**, Postgres has already aborted the
transaction, so the `COMMIT` at the end would silently roll back. Cello detects
this: after a failed statement, a normal exit sends a probe (`SELECT 1`); if the
transaction is aborted it rolls back and raises `TransactionAborted`, chained to
the swallowed error (`__cause__`). Observed:

```python
try:
    async with db.transaction() as tx:
        await tx.execute("INSERT INTO t (email) VALUES ('new')")
        try:
            await tx.execute("INSERT INTO t (email) VALUES ('dup')")   # unique violation
        except UniqueViolation:
            pass
except TransactionAborted as e:
    e.__cause__      # the UniqueViolation
# 'new' was not committed
```

Either let the error propagate, or wrap the risky statement in a savepoint,
which commits normally:

```python
async with db.transaction() as tx:
    await tx.execute("SAVEPOINT s")
    try:
        await tx.execute("INSERT INTO t (email) VALUES ('dup')")
    except UniqueViolation:
        await tx.execute("ROLLBACK TO SAVEPOINT s")
    await tx.execute("INSERT INTO t (email) VALUES ('ok')")   # committed
```

A failure that did not abort the transaction (a parameter rejected in Python
before reaching the server, or a savepoint recovery) still commits.

---

## SQL parameters

Parameters are bound by the **target type Postgres reports** for each `$n`.

| Python value | Accepted for |
|--------------|--------------|
| `None` | any type (`NULL`) |
| `bool` | `bool`; `text`, `varchar`, `bpchar`, `name` (sent as `"true"` / `"false"`) |
| `int` | `int2`, `int4`, `int8` (range-checked), `oid` (`0..=4294967295`), `numeric` (exact, any size), `float`s |
| `float` | `float4`, `float8`, `numeric` (shortest round-trip digits, so `0.1` is `0.1`) |
| `decimal.Decimal` | `numeric` (scale kept: `Decimal("1.50")` stays `1.50`), `float4` / `float8` |
| `str` | text-like types, `json`, `xml`, `bytea`, `"char"`, enums, extension types such as `citext`; `numeric` (`"12.50"`, `"1e-5"`, `"NaN"`); temporal and `uuid` types as ISO / UUID text |
| `bytes` | `bytea` |
| `dict` / `list` | `json` / `jsonb` (a `list` bound to `text` also binds as JSON) |
| `list` / `tuple` | array parameters (`$1::int4[]`, `= ANY($1)`, an array column). Nested lists bind as multi-dimensional arrays and must be rectangular. |
| `datetime` / `date` / ISO string | `timestamptz`, `timestamp`, `date` (see below) |
| `datetime.time` / `"HH:MM[:SS[.ffffff]]"` | `time` |
| `uuid.UUID` / UUID string | `uuid` |

**Temporal and UUID coercions**

| Parameter | Accepts |
|-----------|---------|
| `timestamptz` | `datetime` (aware, or naive which is **treated as UTC**), `date` (midnight UTC), ISO string with offset / `Z` / space separator, no offset (= UTC), date-only, `"infinity"` / `"-infinity"` |
| `timestamp` | as above, but an offset is **ignored and the wall-clock time kept** (as Postgres parses text) |
| `date` | `date`, `datetime` (time part ignored), ISO date string, `"±infinity"` |
| `time` | `time` (any `tzinfo` ignored), `"HH:MM[:SS[.ffffff]]"` |
| `uuid` | `uuid.UUID`, or any string `uuid.UUID()` accepts (hyphenated, 32 hex digits, upper case) |

Fractional seconds beyond microseconds are rounded.

**Rejections happen before the query is sent**

| Situation | Error | Example message |
|-----------|-------|-----------------|
| Integer outside the parameter type's range | `ParameterRangeError` | `parameter $1: value 2147483648 is out of range for Postgres type int4` |
| A Python type that cannot bind to the parameter type | `ParameterTypeError` | `parameter $1: expected a value for Postgres type text, got Python int` |
| An unparsable value for a temporal / `uuid` type, or a ragged nested list | `ParameterValueError` | `parameter $1: invalid input for Postgres type date: "2024-02-30"` |
| Wrong number of parameters | `ProgrammingError` | `expected 1 parameters but got 0` |

When a mismatch is a text-to-type conversion, cast in SQL
(`$1::text::int8`). A `numeric` or `timestamptz` parameter no longer needs an
explicit cast: Decimal / int / float / str and datetime / ISO strings bind
directly.

Result values are documented in the [ORM value-shape table](orm.md#value-shapes).

---

## Exception hierarchy

All database errors are exported from `cello` and subclass `RuntimeError`, so
1.3.x code that catches `RuntimeError` keeps working.

```text
RuntimeError
└── DatabaseError                 .sqlstate .detail .constraint .table .column
    ├── IntegrityError            SQLSTATE class 23
    │   ├── UniqueViolation       23505
    │   ├── NotNullViolation      23502
    │   ├── ForeignKeyViolation   23503
    │   └── CheckViolation        23514
    ├── DataError                 class 22 (and parameters Cello cannot bind)
    │   ├── ParameterTypeError    also TypeError
    │   ├── ParameterRangeError   also OverflowError
    │   └── ParameterValueError   also ValueError
    ├── OperationalError          connection / pool failures; classes 08, 40, 53, 57, 58
    └── ProgrammingError          class 42; wrong number of parameters
```

| Class | Also a | SQLSTATE | Typical cause |
|-------|--------|----------|---------------|
| `DatabaseError` | `RuntimeError` | any | Base class; also any server error not mapped below. |
| `IntegrityError` | | class `23` | Any integrity violation not listed below. |
| `UniqueViolation` | | `23505` | Duplicate key. |
| `NotNullViolation` | | `23502` | `NULL` into a `NOT NULL` column. |
| `ForeignKeyViolation` | | `23503` | Missing or still-referenced row. |
| `CheckViolation` | | `23514` | `CHECK` constraint failed. |
| `DataError` | | class `22` | `1/0` (`22012`), `'abc'::int` (`22P02`), and parameters Cello cannot bind. |
| `ParameterTypeError` | `TypeError` | none | Python type does not fit the parameter type. |
| `ParameterRangeError` | `OverflowError` | none | Integer parameter out of range. |
| `ParameterValueError` | `ValueError` | none | Unparsable temporal / UUID value, ragged list. |
| `OperationalError` | | classes `08`, `40`, `53`, `57`, `58` | Connection refused, TLS handshake failure, authentication failure, pool checkout failure. |
| `ProgrammingError` | | class `42` | Syntax error (`42601`), missing relation (`42P01`), wrong parameter count. |

### Attributes

Every `DatabaseError` has five attributes: `.sqlstate`, `.detail`,
`.constraint`, `.table`, `.column`. They come from the server's error fields
when it sends them and are `None` otherwise. They are always `None` for
client-side parameter errors, which never reach the server.

Observed against Postgres:

| Operation | Class | `.sqlstate` | `.constraint` | `.table` | `.column` |
|-----------|-------|-------------|---------------|----------|-----------|
| duplicate `email` | `UniqueViolation` | `23505` | `ref_t_email_key` | `ref_t` | `None` |
| `NULL` into `email` | `NotNullViolation` | `23502` | `None` | `ref_t` | `email` |
| missing parent row | `ForeignKeyViolation` | `23503` | `ref_child_t_id_fkey` | `ref_child` | `None` |
| `n >= 0` check | `CheckViolation` | `23514` | `ref_t_n_check` | `ref_t` | `None` |
| `select 1/0` | `DataError` | `22012` | `None` | `None` | `None` |
| `select * from nope_tbl` | `ProgrammingError` | `42P01` | `None` | `None` | `None` |
| `selec 1` | `ProgrammingError` | `42601` | `None` | `None` | `None` |
| server refuses connection | `OperationalError` | `None` | `None` | `None` | `None` |
| `$1::int4` bound to `2**31` | `ParameterRangeError` | `None` | `None` | `None` | `None` |

A server error keeps the message form `db error: ERROR: ...` with the DETAIL and
HINT lines. Pool-checkout failures include their cause, for example
`Error occurred while creating a new object: error connecting to server:
Connection refused (os error 111)` or `... db error: FATAL: password
authentication failed for user "postgres"`. A failed authentication surfaces as
`OperationalError` with `.sqlstate` equal to `None`.

```python
from cello import UniqueViolation

try:
    await db.execute("INSERT INTO users (email) VALUES ($1)", email)
except UniqueViolation as e:
    e.sqlstate     # "23505"
    e.constraint   # "users_email_key"
    e.table        # "users"
    e.detail       # "Key (email)=(a@b.c) already exists."
```

See [Error Codes](../errors.md#database-errors) for what happens when a
`DatabaseError` escapes a handler.

---

## See Also

- [ORM API](orm.md) for models, querysets and value shapes
- [Database & Redis](../../data-layer.md) for TLS, Redis and the guided tour
- [Error Codes](../errors.md) for HTTP status handling
- [Migrations](../../features/advanced/migrations.md) and the [CLI reference](../cli.md#cello-migrate)
