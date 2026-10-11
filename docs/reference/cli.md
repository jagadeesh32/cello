---
title: CLI Reference
description: Command-line options and environment variables for running Cello applications
---

# CLI Reference

Cello applications are run directly with `python app.py`. Command-line arguments configure the server at startup and override any values passed to `app.run()` in code.

---

## Usage

```bash
python app.py [OPTIONS]
```

---

## Options

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--host HOST` | `str` | `127.0.0.1` | IP address to bind to. Use `0.0.0.0` to accept connections on all interfaces. |
| `--port PORT` | `int` | `8000` | TCP port to listen on. |
| `--env ENV` | `str` | `development` | Environment name. Use `production` to disable debug mode and verbose logging. |
| `--workers N` | `int` | CPU count | Number of Tokio worker threads. |
| `--debug` | flag | Off in prod | Enable debug mode with verbose error pages. |
| `--reload` | flag | Off | Enable hot reload. Watches `.py` files and restarts on changes. |
| `--no-logs` | flag | Off | Disable request logging output. |

---

## Examples

### Development Server

```bash
python app.py
```

Starts on `127.0.0.1:8000` with debug mode and logging enabled.

### Production Server

```bash
python app.py --host 0.0.0.0 --port 8000 --env production --workers 8
```

### Hot Reload (Development)

```bash
python app.py --reload
```

Watches all `.py` files in the current directory. When a change is detected, the server restarts automatically.

### Custom Port

```bash
python app.py --port 3000
```

### Disable Logging

```bash
python app.py --no-logs
```

---

## Environment Variables

You can use environment variables instead of (or in addition to) command-line flags. Read them in your application code:

```python
import os

app.run(
    host=os.environ.get("HOST", "0.0.0.0"),
    port=int(os.environ.get("PORT", "8000")),
    env=os.environ.get("CELLO_ENV", "development"),
    workers=int(os.environ.get("WORKERS", "4")),
)
```

### Common Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `CELLO_ENV` | Environment name | `production` |
| `HOST` | Bind address | `0.0.0.0` |
| `PORT` | Bind port | `8000` |
| `WORKERS` | Worker thread count | `8` |
| `JWT_SECRET` | JWT signing secret | Random string |
| `DATABASE_URL` | Database connection string. Read by `cello migrate` when `--dsn` and `CELLO_DATABASE_URL` are not set | `postgresql://...` |
| `CELLO_DATABASE_URL` | Database URL for `cello migrate` (takes priority over `DATABASE_URL`) | `postgresql://...` |
| `CELLO_RUN_MAIN` | Internal: set by the reloader | `true` |

---

## Precedence

When the same setting is specified in multiple places, the following order applies (highest priority first):

1. Command-line arguments (`--port 3000`)
2. Values passed to `app.run()` (`port=3000`)
3. Defaults

---

## Programmatic Equivalent

Every CLI flag corresponds to a parameter on `app.run()`:

```python
app.run(
    host="0.0.0.0",       # --host
    port=8000,             # --port
    env="production",      # --env
    workers=8,             # --workers
    debug=False,           # --debug
    reload=False,          # --reload
    logs=False,            # --no-logs
)
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Clean shutdown (Ctrl+C) |
| `1` | Startup error (port in use, invalid config, etc.) |

---

## `cello migrate`

!!! warning "Experimental in 1.4.0"
    `migrate` generates DDL from your `cello.orm` models and runs it against
    your database. Every command exits with code `2` unless `--experimental` is
    given. Review each generated file before applying it. See
    [Migrations](../features/advanced/migrations.md) for the full guide.

The package registers a `cello` console script (`cello = "cello.__main__:main"`
in `pyproject.toml`), so `cello migrate ...` and `python -m cello migrate ...`
are the same command. This is separate from `python app.py`, which runs your
server.

```bash
python -m cello migrate --experimental {make,apply,status,sql} [options]
```

```text
$ python -m cello --help
usage: python -m cello <command> [options]

commands:
  migrate    EXPERIMENTAL schema migrations for cello.orm models
             (python -m cello migrate --experimental --help)
```

### Subcommands

| Command | What it does |
|---------|--------------|
| `make` | Diffs the models against the live database and writes the next `migrations/NNNN_<slug>.sql`. Writes nothing when the database already matches. Refuses to run while files are pending. |
| `sql` | Prints what `make` would write, without writing it. Prints `-- No changes: the database matches the models.` when in sync. |
| `apply` | Applies pending files in order, each in its own transaction, under a Postgres advisory lock. |
| `status` | Lists applied and pending files, history problems and warnings, and whether the models are in sync. |

### Flags

The common flags are accepted both before and after the subcommand name.

| Flag | Applies to | Description |
|------|-----------|-------------|
| `--experimental` | all | Required. Acknowledges that migrations are experimental. |
| `--app APP` | all | The app, as `module:attribute`. Imports it; when `--models` is not given, every model imported outside the `cello` package is migrated. |
| `--models MODELS` | all | Module(s) defining the models (repeatable, or comma separated). Only models defined in these modules (or submodules) are migrated. |
| `--dsn DSN` | all | Postgres URL. Default: `CELLO_DATABASE_URL`, then `DATABASE_URL`, then the `--app` app's `enable_database(...)`. |
| `--dir DIR` | all | Migrations directory. Default: `./migrations`. |
| `--allow-destructive` | all | Emit / apply `DROP TABLE`, `DROP COLUMN` and lossy type changes. Without it they are written commented out, and `apply` refuses live ones. |
| `--name NAME` | `make` | Slug for the file name (`0001_<name>.sql`). |
| `--lock-timeout SECONDS` | `apply` | Seconds to wait for a concurrent `apply`. Default `60`. |

At least one of `--app` or `--models` is required; otherwise the command prints
`error: pass --app module:attr or --models module to find your models` and exits
`1`. `--help` works without `--experimental`.

`make --help`, as printed by 1.4.0:

```text
usage: cello migrate make [-h] [--experimental] [--app APP] [--models MODELS]
                          [--dsn DSN] [--dir DIR] [--allow-destructive]
                          [--name NAME]

options:
  -h, --help           show this help message and exit
  --experimental       acknowledge that migrations are experimental (required)
  --app APP            the app, as module:attribute
  --models MODELS      module(s) defining the models (repeatable, or comma
                       separated)
  --dsn DSN            Postgres URL (default: CELLO_DATABASE_URL /
                       DATABASE_URL)
  --dir DIR            migrations directory (default: ./migrations)
  --allow-destructive  emit / apply DROP TABLE, DROP COLUMN and lossy type
                       changes
  --name NAME          slug for the file name
```

`apply --help` adds `--lock-timeout LOCK_TIMEOUT` (`seconds to wait for a
concurrent apply (default 60)`) in place of `--name`. `status --help` and
`sql --help` show only the common flags.

### Environment variables

| Variable | Meaning |
|----------|---------|
| `CELLO_DATABASE_URL` | Postgres URL. Checked first (after `--dsn`). |
| `DATABASE_URL` | Postgres URL. Checked second. |

TLS follows the URL: add `?sslmode=verify-full&sslrootcert=/path/ca.pem`. The
migration session honours the same `sslmode` options as the app pool
(default `prefer`).

### Migrate exit codes

| Code | Meaning |
|------|---------|
| `0` | Success. For `status`, no history problems. |
| `1` | A migration error (missing models, pending files blocking `make`, checksum mismatch, refused destructive statement, ...), a database or import error, or `status` reported history problems. The message is printed to stderr as `error: ...`. |
| `2` | `--experimental` was not given, no subcommand was given, or the arguments could not be parsed. |

### Example session

Run against a live database with one model, `RefAuthor` (table `ref_mig_author`):

```text
$ export DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/cello_test
$ python -m cello migrate --experimental sql --models refmodels
-- cello migration 0001_initial
-- EXPERIMENTAL: generated by `python -m cello migrate --experimental make`.
-- Review every statement before applying. Renames are not detected.
-- Generated: 2026-10-11T06:57:48Z
-- cello:tables ["ref_mig_author"]

BEGIN;

-- create_table ref_mig_author
CREATE TABLE "ref_mig_author" ("id" SERIAL PRIMARY KEY, "name" VARCHAR(50) NOT NULL);

COMMIT;

$ python -m cello migrate --experimental make --models refmodels --name initial
Wrote migrations/0001_initial.sql (1 statement(s)). Review it, then run apply.
$ python -m cello migrate --experimental status --models refmodels
pending  0001_initial
$ python -m cello migrate --experimental apply --models refmodels
Applied 0001_initial
$ python -m cello migrate --experimental status --models refmodels
applied  0001_initial  (2026-10-11 06:57:48.821346+00)
models:  in sync
$ python -m cello migrate --experimental apply --models nonexist; echo $?
error: ModuleNotFoundError: No module named 'nonexist'
1
$ python -m cello migrate status; echo $?
cello migrate is EXPERIMENTAL in 1.4.0: it generates and runs DDL against your database and can lose data if a migration is applied without review.
Re-run with --experimental to acknowledge, e.g.
    python -m cello migrate --experimental status --app main:app
2
```

Applied files are recorded in the `cello_migrations(id, name, checksum,
applied_at)` table.
