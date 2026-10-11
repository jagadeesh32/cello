#!/usr/bin/env python3
"""
Create and seed the benchmark tables (run once before benchmarking; paired.py
runs it for you). Uses Cello's own native pool, so it needs a Cello install.

    python seed.py [--dsn DSN] [--drop]

bench_users    200 rows x 8 columns (int, text, bool, float, timestamptz, uuid,
               jsonb, numeric). Read-only during benchmarks.
bench_users_w  same shape, empty; the insert endpoint writes here and the
               harness truncates it before every timed run.
               created_at / ext_id / balance are nullable here because the ORM
               binds Python str as Postgres text and cannot bind timestamptz,
               uuid or numeric parameters without an explicit SQL cast (a known
               v1.3.0 limitation), so the insert endpoint leaves them NULL.

--drop removes both tables (cleanup).
"""

import argparse
import asyncio
import os
import sys

from cello import App, DatabaseConfig

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from app import DEFAULT_DSN, READ_TABLE, WRITE_TABLE, LIST_ROWS  # noqa: E402

DDL_READ = f"""
CREATE TABLE "{READ_TABLE}" (
    id          SERIAL PRIMARY KEY,
    name        TEXT NOT NULL,
    active      BOOLEAN NOT NULL,
    score       DOUBLE PRECISION NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL,
    ext_id      UUID NOT NULL,
    profile     JSONB NOT NULL,
    balance     NUMERIC(12,2) NOT NULL
)"""

DDL_WRITE = f"""
CREATE TABLE "{WRITE_TABLE}" (
    id          SERIAL PRIMARY KEY,
    name        TEXT NOT NULL,
    active      BOOLEAN NOT NULL,
    score       DOUBLE PRECISION NOT NULL,
    created_at  TIMESTAMPTZ,
    ext_id      UUID,
    profile     JSONB NOT NULL,
    balance     NUMERIC(12,2)
)"""

# Deterministic seed rows generated server-side (no client-side type binding).
SEED = f"""
INSERT INTO "{READ_TABLE}" (name, active, score, created_at, ext_id, profile, balance)
SELECT
    'user_' || g,
    (g % 3 <> 0),
    g * 1.25,
    timestamptz '2026-01-01 00:00:00+00' + (g || ' minutes')::interval,
    md5(g::text)::uuid,
    jsonb_build_object('plan', CASE WHEN g % 2 = 0 THEN 'pro' ELSE 'free' END,
                       'tags', jsonb_build_array('t' || (g % 5), 't' || (g % 7)),
                       'logins', g * 3),
    (g * 10.5)::numeric(12,2)
FROM generate_series(1, {LIST_ROWS}) AS g
"""


async def main(dsn: str, drop: bool):
    app = App()
    app.enable_database(DatabaseConfig(url=dsn, pool_size=1))
    db = app.database
    await db.execute(f'DROP TABLE IF EXISTS "{READ_TABLE}"')
    await db.execute(f'DROP TABLE IF EXISTS "{WRITE_TABLE}"')
    if drop:
        print(f"dropped {READ_TABLE}, {WRITE_TABLE}")
        return
    await db.execute(DDL_READ)
    await db.execute(DDL_WRITE)
    await db.execute(SEED)
    n = await db.fetchval(f'SELECT count(*) FROM "{READ_TABLE}"')
    print(f"seeded {READ_TABLE}: {n} rows; {WRITE_TABLE}: empty")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--dsn", default=os.environ.get("BENCH_DSN", DEFAULT_DSN))
    p.add_argument("--drop", action="store_true")
    a = p.parse_args()
    asyncio.new_event_loop().run_until_complete(main(a.dsn, a.drop))
