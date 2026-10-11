#!/usr/bin/env python3
"""
Stage-by-stage cost of the /orm/list path (200 rows x 8 columns), measured
in-process, without HTTP, on one asyncio loop with one pooled connection.

    venv/bin/python profile_stages.py [--dsn DSN] [--iters 300]

Stages (each timed separately, median of --iters, after warmup):

  db_execute      db.execute(SELECT ...): Postgres + wire + bridge. The rows
                  come back over the wire but are discarded in Rust (no decode,
                  no Python objects).
  db_fetch        db.fetch(SELECT ...): the same, plus pg_value_to_py +
                  row_to_pydict (dict per row, under the GIL).
  -> row_to_pydict ~= db_fetch - db_execute
  from_row        [User._from_row(r) for r in rows]          (setattr x 8 per row)
  to_dict         [u.to_dict() for u in users]               (getattr x 8 per row)
  serialize       Response.json({"users": dicts}). This is a PROXY: it is the
                  python_to_json + serde_json::to_vec path. The handler-return path
                  uses python_to_json_bytes_direct, which is at most as expensive,
                  so treat this as an upper bound.
  queryset_build  the ORM's SQL assembly (order_by/limit/_select_sql) on its own

Reference points: db.execute("SELECT 1") (fixed bridge + round-trip floor) and
stdlib json.dumps of the same payload (a yardstick for the serializer).

Run seed.py first (paired.py does it for you).
"""

import argparse
import asyncio
import json
import os
import statistics
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from cello import App, DatabaseConfig, Response  # noqa: E402
from cello.orm import setup  # noqa: E402
from app import DEFAULT_DSN, READ_TABLE, LIST_ROWS, User  # noqa: E402

SQL = f'SELECT * FROM "{READ_TABLE}" ORDER BY "id" ASC LIMIT {LIST_ROWS}'


async def main(dsn, iters):
    app = App()
    app.enable_database(DatabaseConfig(url=dsn, pool_size=1))
    db = app.database
    setup(db)
    ns = time.perf_counter_ns

    samples = {k: [] for k in
               ("db_execute", "db_fetch", "from_row", "to_dict", "serialize", "queryset_build",
                "bridge_floor", "stdlib_json")}

    for i in range(iters + 50):
        t0 = ns(); await db.execute(SQL); t1 = ns()
        rows = await db.fetch(SQL); t2 = ns()
        users = [User._from_row(r) for r in rows]; t3 = ns()
        dicts = [u.to_dict() for u in users]; t4 = ns()
        Response.json({"users": dicts}); t5 = ns()
        User.objects.order_by("id").limit(LIST_ROWS)._select_sql(); t6 = ns()
        await db.execute("SELECT 1"); t7 = ns()
        json.dumps({"users": dicts}); t8 = ns()
        if i >= 50:
            for k, v in zip(samples, (t1 - t0, t2 - t1, t3 - t2, t4 - t3, t5 - t4, t6 - t5,
                                      t7 - t6, t8 - t7)):
                samples[k].append(v / 1e3)

    med = {k: statistics.median(v) for k, v in samples.items()}
    med["row_to_pydict (fetch - execute)"] = med["db_fetch"] - med["db_execute"]
    total = med["db_fetch"] + med["from_row"] + med["to_dict"] + med["serialize"] + med["queryset_build"]

    print(f"{LIST_ROWS} rows x 8 cols, {iters} iterations, median microseconds per request\n")
    print("| stage | µs | % of in-process total |")
    print("|---|---:|---:|")
    order = [
        ("db_execute (Postgres + wire + bridge, no decode)", med["db_execute"]),
        ("row_to_pydict + pg_value_to_py (fetch − execute)", med["row_to_pydict (fetch - execute)"]),
        ("_from_row × 200", med["from_row"]),
        ("to_dict() × 200", med["to_dict"]),
        ("serialize (Response.json proxy)", med["serialize"]),
        ("QuerySet SQL assembly", med["queryset_build"]),
    ]
    for name, v in order:
        print(f"| {name} | {v:,.0f} | {v / total * 100:.1f}% |")
    print(f"| **total** | {total:,.0f} | 100% |")
    print()
    print("Reference points (not part of the total):")
    print(f"- db.execute('SELECT 1') round trip (bridge + Postgres floor): {med['bridge_floor']:,.0f} µs")
    print(f"- stdlib json.dumps of the same payload: {med['stdlib_json']:,.0f} µs")
    await db.close()


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--dsn", default=os.environ.get("BENCH_DSN", DEFAULT_DSN))
    p.add_argument("--iters", type=int, default=300)
    a = p.parse_args()
    asyncio.new_event_loop().run_until_complete(main(a.dsn, a.iters))
