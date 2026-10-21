---
title: Migrations Walkthrough
description: Generate, review and apply SQL migrations from your Cello ORM models with the experimental cello migrate command.
---

# :material-database-sync: Migrations Walkthrough

!!! warning "Experimental in 1.4.1"

    `cello migrate` generates DDL from your models and runs it against your database. Read every generated file before you apply it. See the [migrations guide](../../features/advanced/migrations.md).

`cello migrate` compares your `cello.orm` models with the live Postgres schema and writes the difference as plain SQL files. This example drives the whole workflow (`sql`, `make`, `apply`, `status`) from a script, in a throw-away schema and a temporary directory, and shows an incremental second migration and the handling of destructive changes.

## Features Demonstrated

- `python -m cello migrate --experimental sql` previews the next migration without writing it
- `make` writes `NNNN_<name>.sql`; `apply` runs pending files in order, one transaction per file; `status` lists history and whether the models are in sync
- An incremental migration (new columns and a new table) generated from the difference between the models and the live schema
- Destructive changes (`DROP COLUMN`, `DROP TABLE`) written commented out unless `--allow-destructive` is given
- Targeting a schema through `options=-c search_path=...` in the database URL

## Complete Source Code

**`examples/migrations_demo_models.py`**

```python
"""
Models for examples/migrations_demo.py.

Set CELLO_DEMO_STAGE=2 to get the second version of the models (a new field, a
new model and a changed constraint), so the demo can show an incremental
migration.
"""

import os

from cello.orm import (
    AutoField, BooleanField, CharField, ForeignKey, IntegerField, Model, TextField,
)

STAGE = int(os.environ.get("CELLO_DEMO_STAGE", "1"))


class Author(Model):
    id = AutoField()
    name = CharField(max_length=100)

    class Meta:
        table_name = "ex_authors"


class Book(Model):
    id = AutoField()
    title = CharField(max_length=200, unique=True)
    author = ForeignKey(Author, on_delete="CASCADE")
    if STAGE >= 2:
        pages = IntegerField(default=0)          # new column
        blurb = TextField(null=True)             # new nullable column

    class Meta:
        table_name = "ex_books"


if STAGE >= 2:
    class Review(Model):                          # new table
        id = AutoField()
        book = ForeignKey(Book, on_delete="CASCADE")
        stars = IntegerField()
        public = BooleanField(default=True)

        class Meta:
            table_name = "ex_reviews"
```

**`examples/migrations_demo.py`**

```python
#!/usr/bin/env python3
"""
Migrations walkthrough for Cello v1.4.1 (experimental).

Runs `python -m cello migrate --experimental {sql,make,apply,status}` end to
end against a throw-away Postgres schema, using the models in
``migrations_demo_models.py``:

  1. preview the first migration       (sql)
  2. write 0001_initial.sql            (make)
  3. apply it                          (apply)
  4. show the history                  (status)
  5. change the models (STAGE=2), generate and apply an incremental migration
  6. show what a destructive change looks like (a field removed again)

Everything happens in a temporary schema and a temporary migrations directory;
both are removed when the demo ends.

Run:
    export DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/cello_test
    python examples/migrations_demo.py --port 8000     # --port is accepted but
                                                       # unused: no server runs

The same commands by hand (from the examples/ directory):
    export CELLO_DATABASE_URL=postgresql://user:pass@localhost/shop
    python -m cello migrate --experimental sql    --models migrations_demo_models
    python -m cello migrate --experimental make   --models migrations_demo_models
    python -m cello migrate --experimental apply  --models migrations_demo_models
    python -m cello migrate --experimental status --models migrations_demo_models
"""

import argparse
import asyncio
import os
import shutil
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path
from urllib.parse import quote

from cello import App, DatabaseConfig

HERE = Path(__file__).resolve().parent
BASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://postgres:postgres@127.0.0.1:5432/cello_test"
)


def sh(args, env_extra=None, check=True):
    """Run a command, echo it and its output, and return the completed process."""
    env = dict(os.environ, PYTHONPATH=str(HERE), **(env_extra or {}))
    shown = " ".join(["python -m cello migrate --experimental"] + args)
    print(f"\n$ {shown}" + (f"   [{' '.join(f'{k}={v}' for k, v in (env_extra or {}).items())}]" if env_extra else ""))
    proc = subprocess.run(
        [sys.executable, "-m", "cello", "migrate", "--experimental"] + args,
        env=env, cwd=HERE, capture_output=True, text=True,
    )
    out = (proc.stdout + proc.stderr).rstrip()
    if out:
        print(out)
    if check and proc.returncode != 0:
        raise SystemExit(f"command failed with exit code {proc.returncode}")
    return proc


def main():
    parser = argparse.ArgumentParser(description="Cello migrations walkthrough")
    parser.add_argument("--port", type=int, default=8000, help="unused (no server runs)")
    parser.add_argument("--keep", action="store_true", help="keep the schema and files")
    args = parser.parse_args()

    # Migrations act on the first schema of the connection's search_path, so a
    # unique schema keeps the demo away from your real tables.
    schema = "ex_mig_" + uuid.uuid4().hex[:8]
    sep = "&" if "?" in BASE_URL else "?"
    dsn = f"{BASE_URL}{sep}options=-c%20search_path%3D{quote(schema)}"
    mig_dir = tempfile.mkdtemp(prefix="cello_migrations_")

    # A small pool creates and (at the end) drops the schema.
    admin_app = App()
    admin_app.enable_database(DatabaseConfig(url=BASE_URL, pool_size=1))

    async def admin_sql(sql):
        return await admin_app.database.execute(sql)

    asyncio.run(admin_sql(f'CREATE SCHEMA "{schema}"'))
    print(f"schema: {schema}\nmigrations dir: {mig_dir}")

    common = ["--models", "migrations_demo_models", "--dsn", dsn, "--dir", mig_dir]
    try:
        print("\n=== 1. Preview the first migration (nothing is written) ===")
        sh(["sql"] + common)

        print("\n=== 2. Write it ===")
        sh(["make", "--name", "initial"] + common)
        print("files:", sorted(p.name for p in Path(mig_dir).iterdir()))

        print("\n=== 3. Apply it ===")
        sh(["apply"] + common)

        print("\n=== 4. Status ===")
        sh(["status"] + common)

        print("\n=== 5. Change the models (STAGE=2): new columns + a new table ===")
        stage2 = {"CELLO_DEMO_STAGE": "2"}
        sh(["make", "--name", "reviews"] + common, stage2)
        print("\n--- generated file ---")
        print((Path(mig_dir) / sorted(os.listdir(mig_dir))[-1]).read_text().rstrip())
        sh(["apply"] + common, stage2)
        sh(["status"] + common, stage2)

        print("\n=== 6. Go back to STAGE=1: the removals are destructive ===")
        # Without --allow-destructive the DROP statements are only written
        # as comments, and `make` writes no file when nothing else changed.
        sh(["sql"] + common)
        sh(["make", "--name", "undo"] + common)
        print("files:", sorted(p.name for p in Path(mig_dir).iterdir()))
    finally:
        if args.keep:
            print(f"\nkept schema {schema} and {mig_dir}")
        else:
            asyncio.run(admin_sql(f'DROP SCHEMA "{schema}" CASCADE'))
            shutil.rmtree(mig_dir, ignore_errors=True)
            print(f"\ncleaned up schema {schema} and {mig_dir}")


if __name__ == "__main__":
    main()
```

## Running This Example

```bash
export DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/cello_test
python examples/migrations_demo.py
```

The script needs no server (`--port` is accepted for consistency but unused). It creates a schema named `ex_mig_<random>`, runs the CLI through `subprocess`, and drops the schema and the temporary directory at the end (`--keep` leaves them in place).

The same commands by hand, from the `examples/` directory:

```bash
export CELLO_DATABASE_URL=postgresql://user:pass@localhost/shop
python -m cello migrate --experimental sql    --models migrations_demo_models
python -m cello migrate --experimental make   --models migrations_demo_models
python -m cello migrate --experimental apply  --models migrations_demo_models
python -m cello migrate --experimental status --models migrations_demo_models
```

### Captured output

A real run (the random schema name, directory and timestamps are shortened):

```text
schema: ex_mig_xxxxxxxx
migrations dir: /tmp/cello_migrations_xxxxxxxx

=== 1. Preview the first migration (nothing is written) ===

$ python -m cello migrate --experimental sql --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx
-- cello migration 0001_initial
-- EXPERIMENTAL: generated by `python -m cello migrate --experimental make`.
-- Review every statement before applying. Renames are not detected.
-- Generated: 2026-10-11T06:58:18
-- cello:tables ["ex_authors", "ex_books"]

BEGIN;

-- create_table ex_authors
CREATE TABLE "ex_authors" ("id" SERIAL PRIMARY KEY, "name" VARCHAR(100) NOT NULL);

-- create_table ex_books
CREATE TABLE "ex_books" ("id" SERIAL PRIMARY KEY, "title" VARCHAR(200) UNIQUE NOT NULL, "author_id" INTEGER NOT NULL REFERENCES "ex_authors"("id") ON DELETE CASCADE);

COMMIT;

=== 2. Write it ===

$ python -m cello migrate --experimental make --name initial --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx
Wrote /tmp/cello_migrations_xxxxxxxx/0001_initial.sql (2 statement(s)). Review it, then run apply.
files: ['0001_initial.sql']

=== 3. Apply it ===

$ python -m cello migrate --experimental apply --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx
Applied 0001_initial

=== 4. Status ===

$ python -m cello migrate --experimental status --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx
applied  0001_initial  (2026-10-11 06:58:18)
models:  in sync

=== 5. Change the models (STAGE=2): new columns + a new table ===

$ python -m cello migrate --experimental make --name reviews --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx   [CELLO_DEMO_STAGE=2]
Wrote /tmp/cello_migrations_xxxxxxxx/0002_reviews.sql (3 statement(s)). Review it, then run apply.

--- generated file ---
-- cello migration 0002_reviews
-- EXPERIMENTAL: generated by `python -m cello migrate --experimental make`.
-- Review every statement before applying. Renames are not detected.
-- Generated: 2026-10-11T06:58:19
-- cello:tables ["ex_authors", "ex_books", "ex_reviews"]

BEGIN;

-- create_table ex_reviews
CREATE TABLE "ex_reviews" ("id" SERIAL PRIMARY KEY, "book_id" INTEGER NOT NULL REFERENCES "ex_books"("id") ON DELETE CASCADE, "stars" INTEGER NOT NULL, "public" BOOLEAN NOT NULL DEFAULT TRUE);

-- add_column ex_books.pages
ALTER TABLE "ex_books" ADD COLUMN "pages" INTEGER NOT NULL DEFAULT 0;

-- add_column ex_books.blurb
ALTER TABLE "ex_books" ADD COLUMN "blurb" TEXT;

COMMIT;

$ python -m cello migrate --experimental apply --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx   [CELLO_DEMO_STAGE=2]
Applied 0002_reviews

$ python -m cello migrate --experimental status --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx   [CELLO_DEMO_STAGE=2]
applied  0001_initial  (2026-10-11 06:58:18)
applied  0002_reviews  (2026-10-11 06:58:19)
models:  in sync

=== 6. Go back to STAGE=1: the removals are destructive ===

$ python -m cello migrate --experimental sql --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx
-- cello migration 0003_drop_column_ex_books_and_more
-- EXPERIMENTAL: generated by `python -m cello migrate --experimental make`.
-- Review every statement before applying. Renames are not detected.
-- Generated: 2026-10-11T06:58:19
-- cello:tables ["ex_authors", "ex_books", "ex_reviews"]

BEGIN;

-- cello:destructive [disabled: needs --allow-destructive; review, then uncomment] drop_column ex_books.pages: drops the column and its data
-- ALTER TABLE "ex_books" DROP COLUMN "pages";

-- cello:destructive [disabled: needs --allow-destructive; review, then uncomment] drop_column ex_books.blurb: drops the column and its data
-- ALTER TABLE "ex_books" DROP COLUMN "blurb";

-- cello:destructive [disabled: needs --allow-destructive; review, then uncomment] drop_table ex_reviews: drops the table and all of its rows
-- DROP TABLE "ex_reviews";

COMMIT;

$ python -m cello migrate --experimental make --name undo --models migrations_demo_models --dsn postgresql://postgres:postgres@127.0.0.1:5432/cello_test?options=-c%20search_path%3Dex_mig_xxxxxxxx --dir /tmp/cello_migrations_xxxxxxxx
warning: 3 destructive statement(s) written commented-out; review them, then re-run make with --allow-destructive or uncomment by hand
No file written: every change is destructive and --allow-destructive was not given.
  ALTER TABLE "ex_books" DROP COLUMN "pages";
  ALTER TABLE "ex_books" DROP COLUMN "blurb";
  DROP TABLE "ex_reviews";
files: ['0001_initial.sql', '0002_reviews.sql']

cleaned up schema ex_mig_xxxxxxxx and /tmp/cello_migrations_xxxxxxxx
```

## Key Concepts

- **Plain SQL you can review** -- `make` writes ordinary `.sql` files into the migrations directory. Nothing runs until you call `apply`.
- **Diffed against the live database** -- `make` compares the models with what Postgres actually has, so it refuses to run while earlier files are still pending, and writes nothing when the schema already matches.
- **One transaction per file** -- each file runs in its own transaction under an advisory lock, and the applied file's checksum is recorded in `cello_migrations`. Editing an applied file makes `apply` refuse to run.
- **Destructive changes are opt-in** -- step 6 removes the added columns and the model again. The generated statements appear as comments under a `cello:destructive` marker, and no file is written. Pass `--allow-destructive` to `make` and `apply` to run them.
- **Renames are not detected** -- a rename looks like a drop plus an add. Edit the file to use `ALTER TABLE ... RENAME COLUMN` before applying.
- **Choosing the schema** -- migrations act on the first schema of the connection's `search_path`, which is how this demo isolates itself.
- **Programmatic use** -- `cello.migrate.Migrator` offers `sql()`, `make()`, `apply()` and `status()` for scripts.
