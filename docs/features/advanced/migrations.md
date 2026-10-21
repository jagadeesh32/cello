# Migrations (experimental)

!!! warning "Experimental in 1.4.1"
    `cello migrate` generates DDL from your models and runs it against your
    database. Every command refuses to run without `--experimental`. Read each
    generated file before you apply it, and take a backup before you apply in
    production. If a migration is wrong, it can lose data.

`cello migrate` compares your `cello.orm` models with the live Postgres schema
and writes the difference as plain SQL files that you can review:

```
migrations/
  0001_initial.sql
  0002_add_column_book.sql
```

`apply` runs the pending files in order. Each file runs in its own
transaction, and the run holds a Postgres advisory lock. Each applied file is
recorded with its checksum in `cello_migrations(id, name, checksum, applied_at)`.

## Quick start

```python
# shop/models.py
from cello.orm import Model, AutoField, CharField, ForeignKey

class Author(Model):
    id = AutoField()
    name = CharField(max_length=100)

class Book(Model):
    id = AutoField()
    title = CharField(max_length=200, unique=True)
    author = ForeignKey(Author, on_delete="CASCADE")
```

```bash
export CELLO_DATABASE_URL=postgresql://user:pass@localhost/shop

python -m cello migrate --experimental sql    --models shop.models  # preview
python -m cello migrate --experimental make   --models shop.models  # write 0001_initial.sql
python -m cello migrate --experimental apply  --models shop.models
python -m cello migrate --experimental status --models shop.models
```

The `cello` console command is the same thing as `python -m cello`. For
example, `cello migrate --experimental status ...` works too.

## Commands

| Command | What it does |
|---|---|
| `make` | Diffs the models against the database and writes the next `NNNN_<slug>.sql` file. Use `--name` to choose the slug. When the database already matches the models, it writes nothing. |
| `sql` | Prints the file `make` would write, without writing it. |
| `apply` | Applies pending files in order, each in its own transaction, under the advisory lock. |
| `status` | Lists applied and pending files and any history problems. When nothing is pending, it also reports whether the models are in sync. |

## Options

| Option | Meaning |
|---|---|
| `--experimental` | Required. Without it, every command exits with code 2. |
| `--models MODULE` | The modules that define your models. You can repeat it or pass a comma-separated list. Only models defined in these modules (or their submodules) are migrated. |
| `--app MODULE:ATTR` | Imports your app. When `--models` is not given, every model imported outside the `cello` package is migrated. |
| `--dsn URL` | The database URL. Without it, the URL comes from `CELLO_DATABASE_URL`, then `DATABASE_URL`, then the `--app` app's `enable_database(...)`. |
| `--dir PATH` | The migrations directory. The default is `./migrations`. |
| `--allow-destructive` | Writes and applies destructive statements (see below). |
| `--lock-timeout SECONDS` | (`apply`) How long to wait for a concurrent `apply`. The default is 60. |

Migrations act on `current_schema()`, which is the first schema on the
connection's `search_path`. To target another schema, add
`?options=-c%20search_path%3Dmyschema` to the URL.

## What the diff covers

| Change in the models | Generated SQL |
|---|---|
| New model | `CREATE TABLE`, exactly as `Model.create_table()` would write it. ForeignKey targets are created first. If two new tables reference each other, the foreign keys are added afterwards. |
| New field | `ALTER TABLE ... ADD COLUMN` with its type, `UNIQUE`, `NOT NULL`, `DEFAULT` and `REFERENCES` |
| Type changed | `ALTER COLUMN ... TYPE` (see [Type changes](#type-changes)) |
| `null=` changed | `SET NOT NULL` / `DROP NOT NULL` |
| `default=` changed | `SET DEFAULT` / `DROP DEFAULT` |
| `unique=` changed | `ADD UNIQUE (col)` / `DROP CONSTRAINT <live name>` |
| ForeignKey added, removed or retargeted, or `on_delete` changed | `ADD FOREIGN KEY` / `DROP CONSTRAINT <live name>` |
| Field removed | `DROP COLUMN` (**destructive**) |
| Model removed | `DROP TABLE` (**destructive**). This applies only to tables that an earlier migration file managed. |

Statements run in this order: foreign keys and unique constraints are dropped
first, then tables are created, columns are altered and added, unique
constraints and foreign keys are added, and finally columns and tables are
dropped.

### How types and defaults are compared

The diff does not keep its own table of type aliases. Before it compares,
`make` creates each model as a temporary table with only its declared types and
defaults, inside a transaction that is always rolled back. It then reads back
what Postgres stored (`format_type`, `pg_get_expr`). Both sides of the
comparison are therefore rendered by Postgres. `VARCHAR(100)` matches
`character varying(100)`, and `default="x"` matches `'x'::character varying`.
Custom `sql_type` strings are handled the same way. An invalid declared type or
default stops `make` with an error and nothing is written.

### Type changes

| Kind | Examples | Emitted as |
|---|---|---|
| Lossless | `integer` → `bigint`, `varchar(10)` → `varchar(50)`, `varchar` → `text`, `integer` → `double precision` | Plain `ALTER COLUMN ... TYPE` |
| Checked | `bigint` → `integer`, `varchar(50)` → `varchar(10)`, `text` → `varchar(n)`, anything → `text` | Plain `ALTER COLUMN ... TYPE`. Postgres uses its assignment cast, which **raises** on a value that does not fit. It never truncates the value, and the failure rolls back the whole file. |
| Flagged | `text` → `integer`, `text` → `jsonb`, `double precision` → `integer`, `bigint` → `double precision` | `ALTER COLUMN ... TYPE t USING "col"::t`, marked **destructive** |

An explicit `USING col::varchar(n)` cast would silently cut long values, so
`USING` is only written for flagged changes, which have no assignment cast.
When the type changes and the column has a default, the default is dropped
before the type change and the declared default is set again afterwards.

## Destructive statements

`DROP TABLE`, `DROP COLUMN` and flagged type changes are **destructive**:

- Without `--allow-destructive`, `make` writes them **commented out**, each
  under a `-- cello:destructive [disabled ...]` marker. It also prints a
  warning. If every change is destructive, it writes no file.
- With `--allow-destructive`, they are written live, still under the marker.
- `apply` refuses a file that contains a live destructive statement unless it
  is also given `--allow-destructive`. This also covers a statement you
  uncommented by hand.

```sql
-- cello:destructive [disabled: needs --allow-destructive; review, then uncomment] drop_column book.subtitle: drops the column and its data
-- ALTER TABLE "book" DROP COLUMN "subtitle";
```

### Renames are not detected

Renaming a field or a model looks exactly like dropping one and adding
another. `make` never guesses which you meant. When a table loses columns and
gains columns in the same migration, the file carries a warning. The drop
stays destructive, so it is disabled unless you allow it. To keep the data,
edit the file before you apply it. Replace the `ADD COLUMN` and the
`DROP COLUMN` with:

```sql
ALTER TABLE "book" RENAME COLUMN "subtitle" TO "tagline";
```

## History and safety checks

- **Checksums.** Each applied file's SHA-256 is recorded (line endings are
  normalized, so a checkout that changes LF to CRLF still matches). If you edit
  an applied file, `apply` and `make` refuse to run, and `status` reports the
  problem. Put further changes in a new migration.
- **Missing or out-of-order files.** If a file recorded as applied is missing,
  or a pending file is numbered before an applied one, `apply` stops.
- **Pending files block `make`.** `make` diffs against the live database, so
  it refuses to run while files are unapplied. Otherwise it would emit the same
  changes twice.
- **One transaction per file.** The runner wraps the statements between the
  file's `BEGIN;` and `COMMIT;` lines in its own transaction, together with the
  insert into `cello_migrations`. If a statement fails (`SET NOT NULL` on rows
  that hold NULL, `ADD UNIQUE` over duplicate values, a narrowing type change),
  the whole file rolls back and stays pending. The body must not contain
  transaction control, and `apply` rejects a file that does.
- **Advisory lock.** `apply` takes a session-level `pg_advisory_lock` on a
  dedicated connection. A second deploy that runs `apply` at the same time
  waits for up to `--lock-timeout` seconds. It then sees the migrations as
  applied and does nothing.
- **Quoting.** Every table, column and constraint name is quoted through the
  schema registry's quoting. Identifiers from the live catalog are quoted the
  same way. No identifier is built from command-line input.

## Programmatic use

```python
from cello.migrate import Migrator

m = Migrator("postgresql://...", [Author, Book], directory="migrations")
m.sql()                       # the next migration's text, "" when in sync
path = m.make()               # Path, or None when in sync
m.apply(lock_timeout=30)      # names applied
m.status()                    # {"applied", "pending", "problems", "drift", "warnings"}
```

`Migrator` is synchronous and uses its own connection. Run it as a separate
command or script, not from inside a request handler.

## Not covered (yet)

The diff leaves these alone. It writes no SQL for them, and in several cases
it prints a warning instead:

- primary-key changes, and converting a column to or from `SERIAL` or
  identity
- multi-column `UNIQUE` / `FOREIGN KEY` constraints, `CHECK` constraints,
  indexes, unique indexes, and views
- generated columns
- `ON UPDATE` actions other than `NO ACTION` (a ForeignKey with another action
  is recreated)
- renames (see above), and data migrations: write those by hand in a
  migration file
- statements that cannot run inside a transaction (`CREATE INDEX
  CONCURRENTLY`)
- defaults or identifiers that contain a line break: `make` refuses, so write
  that migration by hand
- schemas other than `current_schema()`, and databases other than Postgres
