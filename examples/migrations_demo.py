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
