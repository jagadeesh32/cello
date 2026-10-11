#!/usr/bin/env python3
"""
Cello ORM / data-layer benchmark app (v1.4.0 Phase 02 baseline harness).

One endpoint per row of the "Where the time goes" table in
docs/releases/v1.4.0-plan.md, plus a hot-path control:

    GET  /control              {"ok": true}                       (e) hot-path control
    GET  /orm/list             200 rows x 8 cols via ORM,          (a) headline case
                               {"users": [u.to_dict() for u in users]}
    GET  /orm/list_qs          same rows, {"users": qs}            (a') Phase 02 fast path
    GET  /raw/list             same 200 rows via db.fetch          (b)
    GET  /orm/get/{id}         User.objects.get(id=...)            (c)
    POST /orm/insert           User.objects.create(...)            (d) writes go to bench_users_w
    POST /validate             20-field body= DTO                  (f)
    POST /_bench/reset         TRUNCATE bench_users_w (harness-only, untimed)

The read table (bench_users) is seeded once by seed.py and never written while
benchmarking, so reads see a constant 200-row result. Inserts go to a separate
table (bench_users_w) that the harness truncates before every timed run, so the
write table starts empty for each A and each B run and never grows unboundedly.

Configuration (env or CLI):
    --port / BENCH_PORT              default 8801
    --dsn  / BENCH_DSN               default postgresql://postgres:postgres@127.0.0.1:5432/cello_test
    --pool / BENCH_POOL              default 16

The server runs single-process (workers=1, production env, logging off) so the
number measures one Cello process, not the process count.
"""

import argparse
import dataclasses
import os
import sys

from cello import App, DatabaseConfig, Response
from cello.orm import (
    Model,
    Field,
    AutoField,
    IntegerField,
    TextField,
    BooleanField,
    FloatField,
    JSONField,
    DateTimeField,
    setup,
)

DEFAULT_DSN = "postgresql://postgres:postgres@127.0.0.1:5432/cello_test"

READ_TABLE = "bench_users"
WRITE_TABLE = "bench_users_w"
LIST_ROWS = 200


# ── Fields the ORM does not ship (uuid, numeric) ─────────────────────────────

class UUIDField(Field):
    sql_type = "UUID"


class NumericField(Field):
    sql_type = "NUMERIC(12,2)"


# ── Models: 8 mixed-type columns ─────────────────────────────────────────────
# int (id), text, bool, float, timestamptz, uuid, jsonb, numeric

class User(Model):
    id = AutoField()
    name = TextField()
    active = BooleanField()
    score = FloatField()
    created_at = DateTimeField()
    ext_id = UUIDField()
    profile = JSONField()
    balance = NumericField()

    class Meta:
        table_name = READ_TABLE


class UserW(Model):
    id = AutoField()
    name = TextField()
    active = BooleanField()
    score = FloatField()
    created_at = DateTimeField()
    ext_id = UUIDField()
    profile = JSONField()
    balance = NumericField()

    class Meta:
        table_name = WRITE_TABLE


# ── 20-field request DTO ─────────────────────────────────────────────────────
# Plain dataclass with explicit coercion in __post_init__: this is the
# "dict + model + coercions" path the plan describes, with no third-party
# validation dependency (the body= wrapper also accepts Pydantic models).

@dataclasses.dataclass
class SignupDTO:
    f01_name: str
    f02_email: str
    f03_age: int
    f04_height: float
    f05_active: bool
    f06_city: str
    f07_country: str
    f08_zip: str
    f09_phone: str
    f10_score: float
    f11_level: int
    f12_newsletter: bool
    f13_lang: str
    f14_tz: str
    f15_referrer: str
    f16_visits: int
    f17_ratio: float
    f18_admin: bool
    f19_notes: str
    f20_plan: str

    def __post_init__(self):
        for f in dataclasses.fields(self):
            v = getattr(self, f.name)
            t = f.type
            if t is bool:
                if not isinstance(v, bool):
                    raise ValueError(f"{f.name}: expected bool")
            elif t is int:
                if isinstance(v, bool) or not isinstance(v, int):
                    v = int(v)
            elif t is float:
                if isinstance(v, bool):
                    raise ValueError(f"{f.name}: expected float")
                v = float(v)
            elif t is str:
                if not isinstance(v, str):
                    raise ValueError(f"{f.name}: expected str")
            setattr(self, f.name, v)


INSERT_SQL_DEFAULTS = dict(
    name="bench-insert",
    active=True,
    score=1.5,
    profile={"plan": "pro", "tags": ["a", "b"]},
)


def build_app(dsn: str, pool: int) -> App:
    app = App()
    app.enable_database(DatabaseConfig(url=dsn, pool_size=pool))

    @app.on_event("startup")
    async def startup():
        setup(app.database)

    # (e) hot-path control
    @app.get("/control")
    def control(request):
        return {"ok": True}

    # (a) ORM list, the currently-taught shape
    @app.get("/orm/list")
    async def orm_list(request):
        users = await User.objects.order_by("id").limit(LIST_ROWS)
        return {"users": [u.to_dict() for u in users]}

    # (a') ORM list, the 1.4.0 Phase 02 shape: return the queryset itself and
    # its rows are written to the response natively. A build without the fast
    # path (no QuerySet.json) gets the taught to_dict() shape instead, so a
    # paired A/B run compares "the best each build offers".
    from cello.orm import QuerySet as _QS
    _has_fast_path = hasattr(_QS, "json")

    @app.get("/orm/list_qs")
    async def orm_list_qs(request):
        qs = User.objects.order_by("id").limit(LIST_ROWS)
        if _has_fast_path:
            return {"users": qs}
        return {"users": [u.to_dict() for u in await qs]}

    # (b) raw list, same rows, no ORM
    @app.get("/raw/list")
    async def raw_list(request):
        rows = await request.database.fetch(
            f'SELECT * FROM "{READ_TABLE}" ORDER BY "id" ASC LIMIT {LIST_ROWS}'
        )
        return {"users": rows}

    # (c) single row
    @app.get("/orm/get/{id}")
    async def orm_get(request):
        u = await User.objects.get(id=int(request.params["id"]))
        return u.to_dict()

    # (d) insert one row (numeric/timestamptz/uuid filled by DB defaults)
    @app.post("/orm/insert")
    async def orm_insert(request):
        u = await UserW.objects.create(**INSERT_SQL_DEFAULTS)
        return {"id": u.id}

    # (f) 20-field body validation
    @app.post("/validate", body=SignupDTO)
    def validate(request, dto: SignupDTO):
        return {"ok": True, "plan": dto.f20_plan, "age": dto.f03_age}

    # ── profiling decomposition endpoints (not part of the paired default set) ──
    # Each removes one stage of /orm/list so the stages can be costed by rps
    # difference. See profile.md / RESULTS.md.

    # DB round trip + row_to_pydict only (no model objects, ~no serialization)
    @app.get("/prof/fetch_only")
    async def prof_fetch_only(request):
        rows = await request.database.fetch(
            f'SELECT * FROM "{READ_TABLE}" ORDER BY "id" ASC LIMIT {LIST_ROWS}'
        )
        return {"n": len(rows)}

    # ORM objects + to_dict(), but not serialized
    @app.get("/prof/orm_no_serialize")
    async def prof_orm_no_serialize(request):
        users = await User.objects.order_by("id").limit(LIST_ROWS)
        out = [u.to_dict() for u in users]
        return {"n": len(out)}

    # Rough ceiling: Postgres builds the JSON, Python only passes one string through.
    @app.get("/prof/pg_json")
    async def prof_pg_json(request):
        s = await request.database.fetchval(
            f'SELECT json_build_object(\'users\', json_agg(t ORDER BY t.id))::text '
            f'FROM (SELECT * FROM "{READ_TABLE}" ORDER BY "id" LIMIT {LIST_ROWS}) t'
        )
        r = Response.text(s)
        r.set_header("Content-Type", "application/json")
        return r

    # Fixed per-request floors: an async handler with no I/O, and one trivial query.
    @app.get("/prof/async_noop")
    async def prof_async_noop(request):
        return {"ok": True}

    @app.get("/prof/select1")
    async def prof_select1(request):
        return {"v": await request.database.fetchval("SELECT 1")}

    # harness-only: keep the write table bounded (called between timed runs)
    @app.post("/_bench/reset")
    async def reset(request):
        await request.database.execute(f'TRUNCATE "{WRITE_TABLE}" RESTART IDENTITY')
        return {"reset": True}

    return app


def main():
    p = argparse.ArgumentParser(description="Cello ORM benchmark app")
    p.add_argument("--port", type=int, default=int(os.environ.get("BENCH_PORT", "8801")))
    p.add_argument("--dsn", default=os.environ.get("BENCH_DSN", DEFAULT_DSN))
    p.add_argument("--pool", type=int, default=int(os.environ.get("BENCH_POOL", "16")))
    args, _ = p.parse_known_args()

    app = build_app(args.dsn, args.pool)
    # App.run() re-parses sys.argv with its own parser; hand it a clean argv.
    sys.argv = sys.argv[:1]
    app.run(host="127.0.0.1", port=args.port, env="production",
            debug=False, workers=1, logs=False)


if __name__ == "__main__":
    main()
