---
title: ORM API
description: Complete reference for Cello's built-in async ORM (v1.4.0) - models, fields, querysets, lookups, Q objects, aggregates, relations, bulk operations and value shapes
---

# ORM API

Cello's ORM is a small async ORM over the native Postgres pool. A model is a
Python class declaration that Cello compiles once into a Rust schema registry.
The same compiled description drives the SQL the ORM builds, the native
`body=Model` request validation, response serialization and the OpenAPI schema.

For a guided tour see [Database & Redis](../../data-layer.md). For the raw
query API, transactions and the exception hierarchy see
[Database API](database.md).

!!! info "Requirements"
    Python 3.12+ and PostgreSQL. Every example on this page was run against a
    live Postgres server on Cello 1.4.0.

---

## Importing

```python
from cello.orm import (
    setup, Model,
    Field, AutoField, IntegerField, BigIntegerField, FloatField, BooleanField,
    CharField, TextField, JSONField, DateTimeField, ForeignKey,
    Q, Aggregate, Count, Sum, Avg, Min, Max,
    DoesNotExist, MultipleObjectsReturned, FieldError,
    TransactionAborted, current_transaction,
)
```

`Manager` and `QuerySet` are importable from `cello.orm` as well, but they are
not in `__all__`; you normally reach them as `Model.objects` and by chaining.

---

## Binding the ORM to a database

### `setup(database)`

Binds the ORM to a native `Database` pool. Call it once in a startup hook.

```python
from cello import App, DatabaseConfig
from cello.orm import setup

app = App()
app.enable_database(DatabaseConfig(url="postgresql://user:pass@localhost/mydb"))

@app.on_event("startup")
async def startup():
    setup(app.database)
```

Which database a query runs on is decided in this order:

1. An explicit `using=` argument or `.using(db)`.
2. The ambient transaction of the current task, if you are inside
   `async with db.transaction()` or a `@transactional` handler
   (see [Transactions](#transactions-and-the-ambient-transaction)).
3. The database passed to `setup()`.

If none of these applies, the query raises `RuntimeError("ORM is not bound to a
database. ...")`.

---

## Declaring models

```python
from cello.orm import Model, AutoField, CharField, IntegerField, BooleanField

class User(Model):
    id = AutoField()
    name = CharField(max_length=100)
    email = CharField(max_length=255, unique=True, null=True)
    age = IntegerField(null=True)
    active = BooleanField(default=True)

    class Meta:
        table_name = "users"
```

| Rule | Behaviour |
|------|-----------|
| Table name | `Meta.table_name`, otherwise the **lowercased class name** (`class NoMeta(Model)` is table `nometa`). `table_name` is the only `Meta` option. |
| Field order | Declaration order. Inherited fields are added after the class's own fields. |
| Primary key | The first field with `primary_key=True` (`AutoField` always is). A model may have none, but then it cannot be the target of a `ForeignKey`, and an instance cannot be deleted on its own. |
| Class attributes | Field attributes are removed from the class; values live on instances. |
| `Model.objects` | The [`Manager`](#manager). |
| `Model.DoesNotExist` / `Model.MultipleObjectsReturned` | Per-model subclasses of the module-level exceptions. |
| `Model._meta` | Internal compiled metadata (`table_name`, `fields`, `pk_name`). Not a stable API. |

Defining a class registers it in the process-wide schema registry. When the
class is garbage collected its registry entry is removed.

---

## Fields

Every field class accepts the options of `Field`. `CharField`, `DateTimeField`
and `ForeignKey` add their own.

### `Field(primary_key=False, null=False, default=None, unique=False, db_column=None, write_only=False, read_only=False)`

| Option | Type | Default | Meaning |
|--------|------|---------|---------|
| `primary_key` | `bool` | `False` | Marks the primary-key column. |
| `null` | `bool` | `False` | `True` allows `NULL`. A column without `null=True` is `NOT NULL`. |
| `default` | any, or a callable | `None` | Applied when the **instance is built**. A callable is called once per instance (`default=uuid4`), and a `dict` / `list` default is deep-copied per instance. Also written into the DDL as `DEFAULT` for literal values (bool, number, text, and JSON for `JSONField`). A callable has no DDL default. |
| `unique` | `bool` | `False` | Adds `UNIQUE` to the column. |
| `db_column` | `str` | `None` | Column name when it differs from the attribute name. Queries may use either the field name or the column name. |
| `write_only` | `bool` | `False` | Accepted by `body=` validation, **never serialized**: left out of `to_dict()`, returned model instances, `Response.json(model)` and the queryset row-to-JSON path. |
| `read_only` | `bool` | `False` | Serialized normally, **ignored on `body=` input**. It does not change what `save()` writes. |

Setting both `write_only` and `read_only` raises `ValueError` at class
definition.

### Field classes

| Class | Postgres type | Extra options | Notes |
|-------|---------------|---------------|-------|
| `AutoField` | `SERIAL PRIMARY KEY` | none | `primary_key=True` is forced. Left as `None`, it is omitted from `INSERT` and read back through `RETURNING`. |
| `IntegerField` | `INTEGER` | none | Values outside the 32-bit range raise `ValueError` in queries and `ParameterRangeError` when bound. |
| `BigIntegerField` | `BIGINT` | none | |
| `FloatField` | `DOUBLE PRECISION` | none | |
| `BooleanField` | `BOOLEAN` | none | |
| `CharField` | `VARCHAR(n)` | `max_length: int = 255` | Native validation rejects longer strings with `string_too_long`. |
| `TextField` | `TEXT` | none | |
| `JSONField` | `JSONB` | none | Reads back as nested Python `dict` / `list` / scalar. A `dict` / `list` default is copied per instance and rendered in the DDL as JSON. |
| `DateTimeField` | `TIMESTAMPTZ` | `auto_now_add=False`, `auto_now=False` | Reads back as an RFC 3339 **string**. See below. |
| `ForeignKey` | the target primary key's type | `to`, `on_delete="CASCADE"`, `related_name=None` | See below. |

A custom field is a `Field` subclass whose `sql_type` class attribute is a type
string (or a callable `field -> str`).

### `DateTimeField(auto_now_add=False, auto_now=False, **options)`

| Option | Behaviour |
|--------|-----------|
| `auto_now_add=True` | When the value is `None` at `INSERT`, the column is set to the database's `now()`. An explicit value is written as given. The column gets `DEFAULT now()`. |
| `auto_now=True` | `now()` on **every** `INSERT` and every `save()` `UPDATE`, overwriting any value. The column gets `DEFAULT now()`. |

The instance receives the stored timestamp from `RETURNING`:

```python
class Book(Model):
    id = AutoField()
    title = CharField(max_length=100)
    created = DateTimeField(auto_now_add=True)
    updated = DateTimeField(auto_now=True)

b = Book(title="T")
await b.save()
b.created   # '2026-10-11T06:56:48.070801+00:00'  (a str)
```

### `ForeignKey(to, on_delete="CASCADE", related_name=None, **options)`

```python
class Book(Model):
    id = AutoField()
    author = ForeignKey(Author, on_delete="CASCADE", related_name="books", null=True)
```

| Option | Meaning |
|--------|---------|
| `to` | The target model class. It must declare a primary key, otherwise `ValueError` at class definition. |
| `on_delete` | One of `CASCADE`, `RESTRICT`, `SET NULL`, `SET DEFAULT`, `NO ACTION`, in any case (`"set null"` works). Anything else raises `ValueError`. |
| `related_name` | Name of the reverse accessor on the target. Must be a Python identifier that does not start with `_`. Default: `<source lowercased>_set`. |

- **Column.** `<name>_id` (or `db_column`). Its SQL type follows the target's
  primary key: `SERIAL` becomes `INTEGER`, `BIGSERIAL` becomes `BIGINT`, and
  other keys (`VARCHAR(5)`, `UUID`, ...) are used as declared.
- **Attributes.** `book.author_id` always holds the id. `book.author` holds
  what you assigned (an instance or an id). Assigning an instance copies its
  primary key into `author_id`; assigning `author_id` directly drops a cached
  instance whose key differs. Rows loaded from the database hold the id in both.
  There is no lazy loading: use `select_related` or `prefetch_related`.
- **`to_dict()`** writes `author_id`, never a nested object.
- **Queries.** `filter(author=obj)`, `exclude`, `update` and `create(author=obj)`
  accept a related instance and use its primary key (an unsaved instance raises
  `ValueError`). `values("author")` and `values("author_id")` both return the id.
- **Reverse accessor.** See [Relations](#relations).

---

## Model

### Construction and defaults

```python
u = User(name="x")
u.active    # True: the default is applied when the instance is built
```

- Defaults are applied at construction; an explicit `None` is kept and stored as
  `NULL` (leave the argument out to get the default).
- Unset fields without a default are `None`.
- A callable default is called once per instance.

### Instance methods

| Method | Description |
|--------|-------------|
| `await obj.save(using=None) -> Model` | Inserts when the primary key is `None` (or the model has none). Otherwise runs `UPDATE ... RETURNING` for every column, and inserts the row when no row has that key. Database-generated values (`AutoField`, `auto_now`, `auto_now_add`) are read back. Returns the instance. |
| `await obj.delete(using=None) -> int` | Deletes the row by primary key and returns the count. Returns `0` without sending SQL when the primary key is `None`. Raises `ValueError` on a model with no primary key (use `Model.objects.filter(...).delete()`). |
| `obj.to_dict() -> dict` | One key per field, in declaration order, `<name>_id` for a ForeignKey, `write_only` fields omitted. Built natively. |

### Class methods

| Method | Description |
|--------|-------------|
| `await Model.create_table(using=None, if_not_exists=True)` | Runs the `CREATE TABLE` rendered by the registry. |
| `await Model.drop_table(using=None, if_exists=True)` | Runs `DROP TABLE`. |

For schema changes on an existing database see
[Migrations](../../features/advanced/migrations.md) (experimental).

### Equality and hashing

Two instances are equal when they are the same concrete class and have the same
non-`None` primary key. `hash()` is `hash((class, pk))`. Instances without a
primary key value (unsaved, or a model with no key) compare and hash **by
identity**, so saving an unsaved instance changes its hash. Do not keep unsaved
instances in sets or as dict keys across a `save()`.

```python
await Book.objects.get(id=1) == await Book.objects.get(id=1)   # True
```

---

## Manager

`Model.objects` forwards to a fresh `QuerySet`. It has only these members:

| Method | Returns |
|--------|---------|
| `all()` | `QuerySet` |
| `filter(*q, **lookups)` / `exclude(*q, **lookups)` | `QuerySet` |
| `order_by(*fields)` | `QuerySet` |
| `select_related(*paths)` / `prefetch_related(*names)` | `QuerySet` |
| `values(*fields)` | `ValuesQuerySet` |
| `using(db)` / `get_queryset(using=None)` | `QuerySet` bound to `db` |
| `await get(*q, **lookups)` | one instance |
| `await create(using=None, **fields)` | the inserted instance |
| `await count()` | `int` |
| `await aggregate(*aggs, **named)` | `dict` |
| `await bulk_create(objs, batch_size=None, using=None)` | `list` |
| `await bulk_update(objs, fields, batch_size=None, using=None)` | `int` |

`limit`, `offset`, `first`, `exists`, `update`, `delete` and `json` are
`QuerySet` methods. Call `Model.objects.all()` (or `filter()`) first:
`await User.objects.all().limit(10)`.

### `create(using=None, **fields)`

Always an `INSERT`, including with an explicit primary key, so models with a
non-`AutoField` key can be created. Returns the instance populated from
`RETURNING *`.

```python
author = await Author.objects.create(name="Ann")
```

---

## QuerySet

A `QuerySet` is lazy, chainable and awaitable. Every chaining method returns a
new queryset; `await qs` (or `await qs.all()`) runs it and returns a list of
instances.

```python
users = await User.objects.filter(active=True).order_by("-age").limit(20)
```

Field names in queries are validated when the query is **built**, not when it is
awaited: `User.objects.filter(nope=1)` raises `FieldError` immediately.

### Chaining

| Method | Description |
|--------|-------------|
| `filter(*q, **lookups)` | Rows for which every condition is true. Positional arguments must be [`Q`](#q-objects) objects; all conditions are ANDed. |
| `exclude(*q, **lookups)` | Rows for which the conditions, ANDed, are **not true**. `exclude(a=1, b=2)` is `NOT (a = 1 AND b = 2)`. Rows where a column is `NULL` are kept. `exclude(q)` is `filter(~q)`. |
| `order_by(*fields)` | `"name"` ascending, `"-name"` descending. A second `order_by()` **replaces** the first; `order_by()` clears. Names go through the registry. |
| `limit(n)` / `offset(n)` | `LIMIT` / `OFFSET`. |
| `select_related(*paths)` | See [Relations](#relations). `select_related(None)` clears. |
| `prefetch_related(*names)` | See [Relations](#relations). `prefetch_related(None)` clears. |
| `using(db)` | Run on `db` (a `Database` or `Transaction`) instead of the ambient default. |
| `values(*fields)` | A `ValuesQuerySet` of dicts. See [Aggregates](#aggregates-and-group-by). |

### Terminal operations

| Method | Returns | Description |
|--------|---------|-------------|
| `await qs` / `await qs.all()` | `list[Model]` | Runs the query. Instances are built natively. |
| `await qs.get(*q, **lookups)` | `Model` | Exactly one row. Raises `Model.DoesNotExist` or `Model.MultipleObjectsReturned`. It always sends `LIMIT 2`, replacing a `limit()` you set. |
| `await qs.first()` | `Model \| None` | First row (`LIMIT 1`). |
| `await qs.count()` | `int` | Number of rows. After `limit()` / `offset()` it counts the slice (via a subquery). |
| `await qs.exists()` | `bool` | `EXISTS`. Respects `limit()` / `offset()`. |
| `await qs.aggregate(*aggs, **named)` | `dict` | See [Aggregates](#aggregates-and-group-by). |
| `await qs.update(**fields)` | `int` | `UPDATE` on every matching row; returns the row count. With no fields it returns `0` without sending a query. |
| `await qs.delete()` | `int` | `DELETE` on every matching row; returns the row count. |
| `qs.json()` | awaitable of `cello.RawJSON` | See [Returning rows](#returning-rows-from-a-handler). |

`update()` and `delete()` on a sliced queryset (after `limit()` / `offset()`)
raise `TypeError`, because the slice would otherwise be ignored and every match
affected. Filter on the primary key instead, for example
`.filter(id__in=[...]).delete()`.

```python
await User.objects.filter(age__lt=18).update(active=False)    # -> rows updated
await User.objects.filter(active=False).delete()               # -> rows deleted
await User.objects.filter(age__gt=65).exists()                 # -> bool
```

### Returning rows from a handler

```python
@app.get("/users")
async def users(request):
    return {"users": User.objects.filter(active=True).order_by("id")}
```

A `QuerySet` returned from a handler, on its own or anywhere inside the
returned dict / list / tuple, is run after the handler returns and its rows are
encoded to JSON on the I/O thread, straight from the Postgres wire format, with
no Python object per row. The bytes equal serializing
`[u.to_dict() for u in await qs]`.

Because the query runs after the handler returns:

- it runs on the pool (or the queryset's `using()` database), **outside** any
  transaction the handler had open;
- a database error becomes a `500` that the handler cannot catch.

When either matters, call `await qs.json()` inside the handler. It takes the
same native path and returns a `cello.RawJSON` that the response writes
verbatim:

```python
rows = await User.objects.filter(active=True).json()
return {"users": rows}
```

A model that overrides `to_dict()`, `_from_row()` or `__getattribute__`, or
shadows a field with a `@property`, is encoded from real instances instead
(same output, no speed-up). A `bytea` column raises `TypeError`.

---

## Lookups

A keyword has the form `field` or `field__lookup`. Only the **last** `__` segment
is treated as a lookup, and only when it is one of the names below. An
unsupported lookup on a declared field (for example `name__iexact`) raises
`FieldError` listing the supported ones.

| Lookup | SQL | Notes |
|--------|-----|-------|
| `exact` *(default)* | `col = $n` | `field=None` becomes `col IS NULL`. |
| `ne` | `col != $n` | `field__ne=None` becomes `col IS NOT NULL`. |
| `gt` / `gte` / `lt` / `lte` | `>` / `>=` / `<` / `<=` | |
| `in` | `col IN ($1, $2, ...)` | Any iterable (list, tuple, set, generator, range), materialized once when the query is built. A `str` / `bytes` raises `TypeError`. An empty iterable matches nothing (`FALSE`). |
| `isnull` | `col IS NULL` / `IS NOT NULL` | `True` / `False`. Note `isnull=None` means `IS NOT NULL`. |
| `contains` / `icontains` | `LIKE` / `ILIKE '%v%'` | See LIKE escaping below. |
| `startswith` / `istartswith` | `LIKE` / `ILIKE 'v%'` | |
| `endswith` / `iendswith` | `LIKE` / `ILIKE '%v'` | |

**LIKE escaping.** The value is escaped before the pattern is built, so `%`,
`_` and `\` in user input match literally (`ESCAPE '\'`):

```python
qs = P.objects.filter(s__contains="50%_")
qs._select_sql()    # internal helper; shows the generated SQL
# ('SELECT * FROM "ref_p" WHERE "s" LIKE $1 ESCAPE \'\\\'', ['%50\\%\\_%'])
```

**Identifier safety.** A key, `order_by` name, `update` field or `values` name
must be a declared field name, a ForeignKey's `<name>_id`, or a declared
`db_column`. Anything else raises `FieldError` and nothing is sent to the
database. Query undeclared columns with `await db.fetch(sql, *params)`. Lookups
across relations (`filter(author__name=...)`) are **not** supported.

**Integer range checks.** An `int` outside an integer column's range raises
`ValueError` naming the column and range, before any SQL, also inside `__in`
and `update()`:

```python
P.objects.filter(n=2**40)
# ValueError: 1099511627776 is out of range for P.n (an integer column holds -2147483648..2147483647).
```

---

## Q objects

`Q(**lookups)` holds the same lookups as `filter()`. Combine with `|` (OR),
`&` (AND) and `~` (NOT), nest freely, and pass to `filter()`, `exclude()` and
`get()` alongside keyword lookups (everything is ANDed).

```python
from cello.orm import Q

Book.objects.filter(Q(title__istartswith="the") | Q(pages__gt=500), active=True)
Book.objects.filter(~Q(author=None) & (Q(category="sci") | Q(category="art")))
await Book.objects.get(Q(isbn=code) | Q(slug=code))
```

| Fact | Detail |
|------|--------|
| Constructor | `Q(*q, **lookups)`; positional arguments must be `Q`. |
| NOT | `~q` compiles to `(q) IS NOT TRUE`, so it keeps rows where `q` is false **or unknown** (a NULL column). `filter(~Q(x=5))` is the same query as `exclude(x=5)`. Add `Q(x__isnull=False)` to drop NULL rows. |
| Empty | `Q()` adds no condition and is dropped from `\|` and `&`. `bool(Q())` is `False`. |
| Validation | Each lookup goes through the same field resolution, LIKE escaping, `None` handling and range checks as a keyword lookup. |

---

## Aggregates and GROUP BY

`Count`, `Sum`, `Avg`, `Min` and `Max` are subclasses of `Aggregate`.

| Class | Signature | Alias for a positional use |
|-------|-----------|----------------------------|
| `Count` | `Count(field="*", distinct=False)` | `count` (for `"*"`), else `<field>__count` |
| `Sum` | `Sum(field, distinct=False)` | `<field>__sum` |
| `Avg` | `Avg(field, distinct=False)` | `<field>__avg` |
| `Min` | `Min(field, distinct=False)` | `<field>__min` |
| `Max` | `Max(field, distinct=False)` | `<field>__max` |

`field` must be a string naming a declared field. `Sum("*")` and
`Count("*", distinct=True)` raise `FieldError`. `distinct` must be a `bool`.

### `aggregate(*aggs, **named)`

Returns one dict. `filter` / `exclude` select the rows; `order_by` with
`limit` / `offset` aggregate just that slice.

```python
await Book.objects.aggregate(n=Count(), s=Sum("pages"), a=Avg("pages"), mx=Max("price"))
# {'n': 2, 's': 3, 'a': '3.0000000000000000', 'mx': 1.5}

await Book.objects.aggregate(Sum("pages"))     # {'pages__sum': 3}
```

### `values(*fields).annotate(*aggs, **named)`

`values()` returns a `ValuesQuerySet` (a `QuerySet` subclass of dicts). It is
awaited as before, can be chained, and can be returned from a handler. With no
fields each dict is `to_dict()`. `annotate()` groups by the `values()` fields
(`GROUP BY`) and adds one column per alias; it needs `values(<fields>)` first.

```python
await Book.objects.values("active").annotate(n=Count("id"))
# [{'active': True, 'n': 2}]

await Book.objects.values("n").annotate(c=Count()).order_by("-c").limit(1)
```

- `order_by()` accepts annotation aliases as well as fields. `count()` counts
  groups.
- Aliases must match `^[A-Za-z_][A-Za-z0-9_]*$`, are quoted, and may not repeat
  a `values()` key. A bad alias or field raises `FieldError` before any SQL.
- Not available: `HAVING`, and `annotate()` without `values()`.

### Aggregate value shapes

Value shapes are the Postgres result types, decoded like any column.

| Aggregate | Column type | Python value |
|-----------|-------------|--------------|
| `Count` | any | `int` (`0` over no rows) |
| `Sum` | `INTEGER` | `int` (Postgres returns `bigint`) |
| `Sum` | `BIGINT` | `str` (`numeric`, e.g. `"45"`) |
| `Avg` | any integer | `str` (`numeric`, e.g. `"3.0000000000000000"`) |
| `Sum` / `Avg` | `DOUBLE PRECISION` | `float` |
| `Min` / `Max` | any | the column's own type |

Every aggregate except `Count` is `None` over no rows.

---

## Relations

### Reverse accessors

Each ForeignKey declared on a class installs an accessor on its target:
`author.book_set` by default, or the `related_name`. It returns
`Source.objects.filter(<fk>=instance)`.

```python
books = await author.books.filter(active=True).order_by("title")
n = await author.books.count()
```

- An explicit `related_name` that clashes with a field, an attribute or another
  accessor raises `ValueError` when the class is defined.
- Two default names from one model to the same target make the accessor raise
  `AttributeError` with a hint to add `related_name`.
- A default name that clashes with an attribute is not installed.
- Only ForeignKeys declared on the class itself install accessors, not
  inherited ones.

### `select_related(*paths)`

Loads ForeignKey targets in the same query, with one `LEFT JOIN` per
ForeignKey. `__` follows further ForeignKeys.

```python
rows = await Book.objects.select_related("author__publisher").order_by("id")
rows[0].author           # <Author pk=1>
rows[0].author_id        # 1
```

A NULL (or dangling) ForeignKey leaves the id in place. Naming a reverse accessor
raises `FieldError` pointing at `prefetch_related`.

### `prefetch_related(*names)`

One extra `WHERE key = ANY($1)` query per relation. A name is a ForeignKey or a
reverse accessor; chain with `__`.

```python
authors = await Author.objects.prefetch_related("books")
await authors[0].books      # no query: already loaded
```

After a prefetch, awaiting the accessor runs no query and each child's
ForeignKey attribute is set to the parent. Chaining on the accessor
(`a.books.filter(...)`) runs a new query.

A ForeignKey whose `db_column` equals its own name stores the id under the
field name and cannot hold an instance: `select_related` / `prefetch_related`
reject it with `FieldError`.

`to_dict()` never nests related objects, so a returned queryset or `json()` with
`select_related` / `prefetch_related` runs the base query alone.

---

## Bulk operations

### `bulk_create(objs, batch_size=None, using=None) -> list`

One multi-row `INSERT ... RETURNING *` per batch. Each instance is refreshed
from its row. Values are written as `create()` writes them: an `AutoField`
left as `None` takes the database's value, `auto_now` is `now()`,
`auto_now_add` is `now()` when unset, ForeignKey instances are written as their
primary key.

### `bulk_update(objs, fields, batch_size=None, using=None) -> int`

One `UPDATE ... FROM (VALUES ...)` per batch; returns the number of rows
updated. Only the named `fields` are written, plus `auto_now` fields. Every
instance needs a primary key and may appear once, otherwise `ValueError` is
raised before any SQL. Parameters are cast to the column types, without length
modifiers, so a too-long string raises instead of being truncated.

```python
books = await Book.objects.bulk_create([Book(title=f"x{i}") for i in range(3)])
for b in books:
    b.pages = (b.pages or 0) + 100
await Book.objects.bulk_update(books, ["pages"])        # -> 3
```

Batches are cut so that no statement exceeds Postgres' 65,535 bind parameters;
`batch_size` can lower that. Several batches on a pool run in one transaction
(all-or-nothing); on a transaction they use it. Neither method calls `save()`.

---

## Transactions and the ambient transaction

Inside `async with db.transaction()` and inside an `@transactional` handler, ORM
calls without `using=` run on that transaction and commit or roll back with it.

```python
try:
    async with request.database.transaction():
        await Book.objects.create(title="rolled back")   # in the transaction
        raise RuntimeError
except RuntimeError:
    pass
await Book.objects.filter(title="rolled back").count()   # 0
```

- The open transaction is held in a `contextvars` variable, available as
  `cello.orm.current_transaction()` (also `cello.database.current_transaction()`).
  Tasks created inside the block inherit it; other requests never see it.
- `.using(db)` always wins, so `Book.objects.using(request.database)` still
  reaches the pool from inside a block. Inside the block the pool does not see
  uncommitted rows:

```python
async with db.transaction():
    await P.objects.create(n=778)
    await P.objects.using(db).filter(n=778).count()   # 0 (pool connection)
    await P.objects.filter(n=778).count()             # 1 (the transaction)
```

- If a statement in the block fails and you catch the error, Postgres has
  aborted the transaction; leaving the block normally rolls back and raises
  `cello.TransactionAborted` (a `RuntimeError`), chained to the swallowed error.
  See [Database API](database.md#transactions).
- A returned queryset runs after the handler returns, outside the transaction;
  use `await qs.json()` inside it.

---

## Exceptions

| Exception | Base | Raised when |
|-----------|------|-------------|
| `DoesNotExist` | `Exception` | `get()` matches no row. Each model has its own subclass, `Model.DoesNotExist`, so `except Book.DoesNotExist` is specific. |
| `MultipleObjectsReturned` | `Exception` | `get()` matches more than one row. Also per model. |
| `FieldError` | `ValueError` | An undeclared or invalid field name, an unsupported lookup, a bad aggregate or alias, or a rejected relation path. |

Other errors: `ValueError` for out-of-range integers, an unsaved related
instance in a query, invalid `on_delete`, a ForeignKey to a model with no
primary key, and `delete()` on a model with no primary key; `TypeError` for
`__in` given a `str`, a non-`Q` positional argument, and `update()` /
`delete()` on a sliced queryset. Database failures are
[`DatabaseError` subclasses](database.md#exception-hierarchy), for example
`UniqueViolation` from `create()`.

```python
try:
    await Book.objects.get(title="nope")
except Book.DoesNotExist as e:
    print(e)    # Book matching query does not exist.
```

Unhandled, these exceptions become a `500` (see [Error Codes](../errors.md#database-errors)).

---

## Value shapes

Rows decode to a frozen set of Python shapes, the same for `fetch()`, ORM
instances, `to_dict()` and the native JSON path.

| Postgres type | Python value | Example |
|---------------|--------------|---------|
| `int2`, `int4`, `int8` | `int` | `5` |
| `float4`, `float8` | `float` | `1.5` |
| `bool` | `bool` | `True` |
| `text`, `varchar`, `bpchar` | `str` | `'x'` |
| `numeric` | `str`, the exact decimal text, scale kept | `'1.50'`, `'NaN'`; never a float |
| `timestamptz` | `str`, RFC 3339 | `'2024-01-02T03:04:05+00:00'` |
| `timestamp` | `str`, no offset | `'2024-01-02T03:04:05.000000'` |
| `date` | `str` | `'2024-01-02'` |
| `time` | `str` | `'06:56:48.092651'` |
| `uuid` | `str` | `'00000000-0000-0000-0000-000000000001'` |
| `json`, `jsonb` | nested `dict` / `list` / scalar (object keys come back sorted) | `{'a': [1, 2]}` |
| `bytea` | `bytes` | `b'x'` |
| 1-D array of a supported type | `list`, `None` for NULL elements | `[1, None, 3]` |
| multi-dimensional array | nested `list` (lower bounds dropped) | `[[1, 2], [3, 4]]` |
| `interval` | `str`, Postgres style | `'1 day'` |
| `timetz`, `inet`, `cidr`, `macaddr`, `"char"` | `str` | `'10.0.0.1'` |
| `money` | `str`, plain decimal with two fractional digits | `'12.50'` |
| enum | `str` label | |
| infinite `timestamp` / `timestamptz` / `date` | `'infinity'` / `'-infinity'` | |
| `NULL` (any type) | `None` | |
| other unsupported type | placeholder string `"<unsupported column type: ...>"` | |

Notes:

- `interval` is always rendered in the `postgres` style whatever the session's
  `IntervalStyle`. `money` loses its currency symbol because the binary value
  carries none.
- A `DateTimeField` therefore holds a **string**, not a `datetime`.
- Parameters accept more shapes than rows return; see
  [SQL parameters](database.md#sql-parameters).

---

## Breaking changes from 1.3.x

1.4.0 changed these behaviours on purpose. Each was a 1.3.x quirk or bug; the
full reasoning is in the [v1.4.0 release notes](../../releases/v1.4.0.md).

| Change | Migration |
|--------|-----------|
| Undeclared column names in `filter` / `exclude` / `order_by` / `update` / `values` raise `FieldError`. | Declare the column on the model, or move the query to `db.fetch(...)`. |
| Defaults are applied when the instance is built (`User(name="x").active` is `True`; callables are called per instance; `dict` / `list` defaults are copied). `create(x=None)` now writes `NULL`. | Leave an argument out to get its default instead of passing `None`. |
| `DateTimeField(auto_now_add=True)` now works on `create()`; `auto_now=True` is new. | None needed. |
| `create()` always inserts; `save()` inserts a missing row instead of silently doing nothing. | None needed. `create(id=99, ...)` now really creates row 99. |
| `delete()` on a model without a primary key raises `ValueError` (was `TypeError`). | Use `Model.objects.filter(...).delete()`. |
| ForeignKey assignment is synced (`book.author = obj` persists on `save()`); the column type follows the target's key; a target without a primary key raises at class definition. | None needed. |
| `JSONField(default={"a": 1})` renders valid JSON in the DDL. | None needed. |
| Equality and hashing follow the primary key. | Do not keep unsaved instances in sets across a `save()`. |
| `filter(x=None)` is `IS NULL`; `exclude(x=None)` is `IS NOT NULL`. | None needed (it matched nothing before). |
| `exclude()` is null-safe: rows with a NULL column are kept, and `exclude(a=1, b=2)` is `NOT (a AND b)`. | Add `.filter(col__isnull=False)` to drop NULL rows as before. |
| `x__in="abc"` raises `TypeError`; any other iterable is accepted. | Pass a list. |
| `contains` / `startswith` / `endswith` (and `i*`) match `%`, `_`, `\` literally; `iendswith` is new. | Do not rely on user-supplied wildcards. |
| A second `order_by()` replaces the first. | Pass all fields to one `order_by()`. |
| `count()` / `exists()` honour `limit()` / `offset()`; `update()` / `delete()` on a sliced queryset raise `TypeError`. | Filter on the primary key (`id__in=[...]`). |
| `update()` with no fields returns `0`. | None needed. |
| Integers outside a column's range raise `ValueError` (ORM) or `ParameterRangeError` (raw) instead of wrapping. | Validate or widen the column. |
| ORM calls inside a transaction use it. | Use `.using(db)` to reach the pool on purpose. |
| `values()` returns a queryset (still awaitable). | None needed. |
| `NUMERIC`, arrays, `interval`, `inet`, ... decode to real values instead of a placeholder string. | Update code that compared against the placeholder. |
| `fetchrow` / `fetchval` return the first row instead of raising on several rows. | Add `LIMIT 1` for large results. |
| Database errors are `DatabaseError` subclasses (still `RuntimeError`). | None needed; catch the specific class for finer handling. |
| The default Postgres `sslmode` is `prefer`. | Set `sslmode="disable"` to keep plaintext; use `"verify-full"` in production. |

---

## What is not covered

Lookups across relations (`filter(author__name=...)`), `HAVING`, per-object
`annotate()` without `values()`, `select_related` on reverse relations, and
many-to-many fields are not supported. Use raw queries with `request.database`
(`fetch_json` when the rows go straight into the response) for those.

---

## See Also

- [Database API](database.md) for `fetch`, transactions, parameter binding and errors
- [Database & Redis](../../data-layer.md) for a guided introduction
- [DTO Validation](../../features/advanced/dto-validation.md) for `body=Model`
- [Migrations](../../features/advanced/migrations.md) for `cello migrate`
