"""
v1.4.0 Phase 02 — the row pipeline.

* Row → JSON fast path: ``QuerySet.json()`` / returning a queryset from a
  handler writes rows straight from the Postgres wire format. The output must be
  **byte-identical** to serializing ``[u.to_dict() for u in rows]`` — checked
  here for every value type the value layer decodes, NULLs included, against a
  reference that rebuilds instances and dicts with the 1.4.0-pre Python code.
* Native ``to_dict()`` and native hydration (``await qs``) must equal the
  previous Python implementations exactly (values, key order, types).
* The serializer accepts model instances, ``RawJSON`` and querysets (holes
  filled after the handler returns), checked over live HTTP.
"""

import json
import math
import threading
import time

import pytest

import cello
from cello import App, RawJSON
from cello import orm
from cello.orm import (
    AutoField,
    BigIntegerField,
    BooleanField,
    CharField,
    DateTimeField,
    Field,
    FloatField,
    ForeignKey,
    IntegerField,
    JSONField,
    Model,
    QuerySet,
    TextField,
)

from orm_compat_support import call, drop_tables, make_db, pg, run

pytestmark = pg


# ── Reference: the 1.4.0-pre Python implementations ──────────────────────────

def ref_from_row(cls, row):
    obj = cls.__new__(cls)
    d = obj.__dict__
    for attr, col in cls._meta.hydrate:
        d[attr] = row.get(col)
    return obj


def ref_to_dict(obj):
    out = {}
    for name, field in obj._meta.fields.items():
        if getattr(field, "write_only", False):
            continue
        if isinstance(field, ForeignKey):
            val = getattr(obj, field.column, None)
            if val is None:
                related = getattr(obj, name, None)
                if isinstance(related, Model):
                    pk = related._meta.pk_name
                    val = None if pk is None else getattr(related, pk, None)
            out[field.column] = val
        else:
            out[name] = getattr(obj, name, None)
    return out


def encode(obj) -> bytes:
    """The response serializer's bytes for ``obj`` (what a handler returning
    it sends)."""
    return bytes(RawJSON.encode(obj))


# ── Raw: every decoded type, wire → JSON vs wire → Python → JSON ─────────────

EXPRS = [
    # ints / oid / bool
    "(-32768)::int2", "2147483647::int4", "(-9223372036854775808)::int8",
    "9223372036854775807::int8", "4294967295::oid", "true", "false",
    # floats (float4 widening, shortest repr, no exponent, non-finite → null)
    "0.1::float4", "1.5::float4", "0.1::float8", "1e20::float8", "1e-7::float8",
    "1e308::float8", "(-0.0)::float8", "123456789.123::float8",
    "'NaN'::float8", "'Infinity'::float8", "'-Infinity'::float8", "'NaN'::float4",
    # numeric
    "12.50::numeric", "12.50::numeric(10,2)", "(-0.000001)::numeric", "0::numeric",
    "123456789012345678901234567890.5::numeric", "'NaN'::numeric",
    "'Infinity'::numeric", "'-Infinity'::numeric",
    # text-like, escapes, unicode
    "''::text", "'x'::varchar(5)", "'x'::char(3)", "'x'::name", "'abc'",
    "E'q \" b \\\\ n \\n t \\t r \\r bell \\x07 \\x01 \\x1f del \\x7f'::text",
    "'héllo ✓ 日本 🎻 😀'::text", "'</script>'::text", "U&'\\2028\\2029'::text",
    "'x'::\"char\"", "''::\"char\"",
    # uuid / temporal
    "'A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11'::uuid",
    "TIMESTAMPTZ '2024-01-02 03:04:05.123456+00'", "TIMESTAMPTZ '2024-01-02 03:04:05+05:30'",
    "TIMESTAMPTZ '2024-01-02 03:04:05.1+00'", "'infinity'::timestamptz",
    "'-infinity'::timestamptz", "TIMESTAMP '1999-12-31 23:59:59.999999'",
    "'infinity'::timestamp", "DATE '2024-02-29'", "DATE '0099-01-01'", "'-infinity'::date",
    "TIME '13:14:15.25'", "TIMETZ '13:14:15.25-05:30'",
    "interval '1 year 2 mons 3 days 04:05:06.5'", "interval '-1 days +02:00:00'",
    # json / jsonb (sorted keys, int/u64/float numbers, unicode, nesting)
    "'{\"b\": 1, \"a\": [1.5, \"x\", null, true, {}]}'::json",
    "'{\"big\": 18446744073709551615, \"neg\": -9223372036854775808, \"f\": 1.0, "
    "\"e\": 1e300, \"tiny\": 1e-300, \"s\": \"é\\n\\\"\", \"n\": {\"z\": [{}, []]}}'::jsonb",
    "'\"str\"'::jsonb", "'[]'::jsonb", "'{}'::jsonb", "'null'::jsonb", "'1.0'::jsonb",
    "'0.1'::jsonb", "'-5'::jsonb", "'{\"k\": \"日本\"}'::jsonb",
    # arrays (NULL elements, multi-dim, empty, lower bounds, element types)
    "ARRAY[1, NULL, 3]::int4[]", "ARRAY['a', NULL, '\"q\"\\']::text[]",
    "ARRAY[[1, 2], [3, 4]]", "'{}'::int4[]", "'[0:1]={7,8}'::int[]",
    "ARRAY[1.50, NULL]::numeric[]", "ARRAY['{\"a\": 1}'::jsonb, NULL]",
    "ARRAY['a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::uuid]",
    "ARRAY[1.5::float8, 'NaN', 'Infinity']", "ARRAY[DATE '2024-01-01']",
    "ARRAY[true, false, NULL]", "ARRAY[TIMESTAMPTZ '2024-01-02 03:04:05+00']",
    "ARRAY[interval '1 day']",
    # network / money
    "'192.168.0.1'::inet", "'10.1.0.0/16'::cidr", "'::1'::inet",
    "'08:00:2b:01:02:03'::macaddr", "12.5::money", "(-0.05)::money",
    # unsupported → placeholder (and NULL of it → null)
    "point '(1,2)'", "ARRAY[point '(1,2)']", "NULL::point", "'<a/>'::xml",
    # NULLs of supported types
    "NULL", "NULL::int4", "NULL::text", "NULL::jsonb", "NULL::numeric",
    "NULL::uuid", "NULL::int4[]", "NULL::bytea",
]


@pytest.fixture(scope="module")
def db():
    database = make_db(pool_size=3)

    async def setup():
        await database.execute("DROP TYPE IF EXISTS fp_mood CASCADE")
        await database.execute("CREATE TYPE fp_mood AS ENUM ('ok', 'sad')")
        await database.execute("DROP DOMAIN IF EXISTS fp_posint CASCADE")
        await database.execute("CREATE DOMAIN fp_posint AS int4 CHECK (VALUE > 0)")

    run(setup())
    yield database


def raw_pair(db, sql, *params):
    native = call(db._fetch_json, None, sql, *params)
    rows = call(db.fetch, sql, *params)
    return bytes(native), encode(rows)


@pytest.mark.parametrize("expr", EXPRS)
def test_raw_json_matches_python_path(db, expr):
    sql = f"SELECT {expr} AS v, 1 AS k, {expr} AS w FROM generate_series(1, 3)"
    native, ref = raw_pair(db, sql)
    assert native == ref
    assert isinstance(json.loads(native), list)  # always valid JSON


@pytest.mark.parametrize(
    "expr", ["'ok'::fp_mood", "ARRAY['ok', 'sad']::fp_mood[]", "5::fp_posint"]
)
def test_raw_json_enum_and_domain(db, expr):
    native, ref = raw_pair(db, f"SELECT {expr} AS v")
    assert native == ref


def test_raw_json_duplicate_and_odd_column_names(db):
    native, ref = raw_pair(db, 'SELECT 1 AS a, 2 AS b, 3 AS a, 4 AS "we""ird\nname"')
    assert native == ref == b'[{"a":3,"b":2,"we\\"ird\\nname":4}]'


def test_raw_json_empty_result(db):
    native, ref = raw_pair(db, "SELECT 1 AS a WHERE false")
    assert native == ref == b"[]"


def test_raw_json_params(db):
    native, ref = raw_pair(db, "SELECT $1::int4 + 1 AS a, $2::text AS b", 41, "x")
    assert native == ref == b'[{"a":42,"b":"x"}]'


def test_raw_json_bytea_is_not_serializable(db):
    with pytest.raises(TypeError, match="bytea"):
        call(db._fetch_json, None, "SELECT '\\xdead'::bytea AS b")
    with pytest.raises(TypeError):
        encode(call(db.fetch, "SELECT '\\xdead'::bytea AS b"))


def test_raw_json_decode_error_is_a_database_error(db):
    with pytest.raises(cello.DatabaseError):
        call(db._fetch_json, None, "SELECT '1e400'::jsonb AS j")


# ── Models: every field type, NULLs, FKs, db_column ──────────────────────────

class NumericF(Field):
    sql_type = "NUMERIC(12,2)"


class UUIDF(Field):
    sql_type = "UUID"


class DateF(Field):
    sql_type = "DATE"


class TimestampF(Field):
    sql_type = "TIMESTAMP"


class IntervalF(Field):
    sql_type = "INTERVAL"


class IntArrayF(Field):
    sql_type = "INT4[]"


class TextArrayF(Field):
    sql_type = "TEXT[]"


class InetF(Field):
    sql_type = "INET"


class FpOwner(Model):
    id = AutoField()
    name = CharField(max_length=50)

    class Meta:
        table_name = "fp_owner"


class FpAll(Model):
    id = AutoField()
    i = IntegerField(null=True)
    big = BigIntegerField(null=True)
    f = FloatField(null=True)
    b = BooleanField(null=True)
    c = CharField(max_length=40, null=True)
    t = TextField(null=True)
    j = JSONField(null=True)
    at = DateTimeField(null=True)
    n = NumericF(null=True)
    u = UUIDF(null=True)
    d = DateF(null=True)
    ts = TimestampF(null=True)
    iv = IntervalF(null=True)
    ia = IntArrayF(null=True)
    ta = TextArrayF(null=True)
    ip = InetF(null=True)
    renamed = TextField(null=True, db_column="col_x")
    owner = ForeignKey(FpOwner, null=True)
    other = ForeignKey(FpOwner, null=True, db_column="other_ref")

    class Meta:
        table_name = "fp_all"


ROWS = [
    dict(i=-5, big=2**62, f=0.1, b=True, c='quo"te\\', t="日本 🎻\n\t\x01",
         j={"b": [1, 2.5, None], "a": {"x": "é"}, "u": 18446744073709551615},
         at="2024-01-02T03:04:05.5Z", n="12.50", u="a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
         d="2024-02-29", ts="1999-12-31T23:59:59.999999", ia=[1, None, 3],
         ta=["a", None, ""], renamed="r"),
    dict(i=0, big=-(2**63), f=float("nan"), b=False, c="", t="", j=[], n="-0.01",
         at="2024-01-02T03:04:05+05:30", ia=[], ta=[]),
    dict(f=float("inf"), j=[1, "s", {"k": None}], n="NaN"),
    dict(),  # all NULL
]


@pytest.fixture(scope="module")
def mtables(db):
    orm.setup(db)

    async def setup():
        await drop_tables(db, "fp_all", "fp_owner")
        await FpOwner.create_table(using=db)
        await FpAll.create_table(using=db)
        o1 = await FpOwner.objects.create(using=db, name="o1")
        o2 = await FpOwner.objects.create(using=db, name="o2")
        for k, row in enumerate(ROWS):
            obj = await FpAll.objects.create(
                using=db,
                **row, owner=o1 if k % 2 == 0 else None, other=o2 if k == 0 else None
            )
            if k == 0:
                await db.execute(
                    "UPDATE fp_all SET iv = interval '1 day 02:00:00', ip = '10.0.0.1' "
                    "WHERE id = $1", obj.id,
                )

    run(setup())
    yield db
    run(drop_tables(db, "fp_all", "fp_owner"))


def model_reference(db, qs) -> bytes:
    sql, params = qs._select_sql()
    rows = call(db.fetch, sql, *params)
    return encode([ref_to_dict(ref_from_row(qs.model, r)) for r in rows])


def model_paths(db, qs):
    native = bytes(call(qs.json))
    taught = encode([u.to_dict() for u in run(qs.all())])
    instances = encode(run(qs.all()))
    return native, taught, instances


@pytest.mark.parametrize(
    "build",
    [
        lambda: FpAll.objects.order_by("id"),
        lambda: FpAll.objects.filter(b=True),
        lambda: FpAll.objects.filter(i__isnull=True).order_by("-id"),
        lambda: FpAll.objects.exclude(owner=None).order_by("id").limit(1),
        lambda: FpAll.objects.filter(id=-1),
        lambda: FpOwner.objects.order_by("-id"),
    ],
)
def test_model_json_is_byte_identical(mtables, build):
    qs = build().using(mtables)
    ref = model_reference(mtables, qs)
    native, taught, instances = model_paths(mtables, qs)
    assert native == ref
    assert taught == ref
    assert instances == ref


def test_model_json_keys_and_values(mtables):
    out = json.loads(bytes(call(FpAll.objects.using(mtables).order_by("id").json)))
    assert list(out[0]) == [
        "id", "i", "big", "f", "b", "c", "t", "j", "at", "n", "u", "d", "ts", "iv",
        "ia", "ta", "ip", "renamed", "owner_id", "other_ref",
    ]
    assert out[0]["n"] == "12.50" and out[0]["iv"] == "1 day 02:00:00"
    assert out[0]["j"] == {"a": {"x": "é"}, "b": [1, 2.5, None], "u": 18446744073709551615}
    assert out[1]["f"] is None  # NaN → null, as the serializer writes it
    assert all(v is None for k, v in out[3].items() if k not in ("id", "owner_id"))


def test_model_json_inside_transaction_sees_uncommitted_rows(mtables):
    async def go():
        async with mtables.transaction():
            await FpOwner.objects.create(name="tx-only")
            inside = bytes(await FpOwner.objects.filter(name="tx-only").json())
            assert json.loads(inside)[0]["name"] == "tx-only"
            raise RuntimeError("rollback")

    with pytest.raises(RuntimeError):
        run(go())
    assert run(FpOwner.objects.using(mtables).filter(name="tx-only").count()) == 0


def test_model_json_in_transaction_value(mtables):
    async def go():
        async with mtables.transaction():
            await FpOwner.objects.create(name="tx2")
            out = bytes(await FpOwner.objects.filter(name="tx2").json())
            await FpOwner.objects.filter(name="tx2").delete()
            return out

    assert json.loads(run(go()))[0]["name"] == "tx2"


# ── Table/model mismatch and identifier edge cases ────────────────────────────

class FpPartial(Model):
    id = AutoField()
    present = TextField(null=True)
    missing = TextField(null=True)  # not in the table: to_dict() gives None

    class Meta:
        table_name = "fp_partial"


class FpClash(Model):
    """A plain field named like the ForeignKey's column: one to_dict key,
    first position, value from the later field."""

    id = AutoField()
    owner = ForeignKey(FpOwner, null=True)
    owner_id = IntegerField(null=True, db_column="raw_owner")

    class Meta:
        table_name = "fp_clash"


def test_missing_and_extra_columns(mtables):
    run(drop_tables(mtables, "fp_partial"))
    call(mtables.execute,
         "CREATE TABLE fp_partial (id serial primary key, extra int, present text)")
    call(mtables.execute, "INSERT INTO fp_partial (extra, present) VALUES (7, 'p')")
    qs = FpPartial.objects.using(mtables).order_by("id")
    native, taught, instances = model_paths(mtables, qs)
    assert native == taught == instances == model_reference(mtables, qs)
    assert native == b'[{"id":1,"present":"p","missing":null}]'
    run(drop_tables(mtables, "fp_partial"))


def test_duplicate_to_dict_key(mtables):
    run(drop_tables(mtables, "fp_clash"))
    call(mtables.execute,
         "CREATE TABLE fp_clash (id serial primary key, owner_id int, raw_owner int)")
    call(mtables.execute, "INSERT INTO fp_clash (owner_id, raw_owner) VALUES (1, 99)")
    qs = FpClash.objects.using(mtables).order_by("id")
    native, taught, instances = model_paths(mtables, qs)
    assert native == taught == instances == model_reference(mtables, qs)
    assert native == b'[{"id":1,"owner_id":99}]'
    run(drop_tables(mtables, "fp_clash"))


# ── Visibility: write_only fields never serialize ─────────────────────────────

def _secret_field():
    f = TextField(null=True)
    f.write_only = True  # Phase 03 adds the option; the registry already honours it
    return f


class FpSecret(Model):
    id = AutoField()
    login = TextField()
    password = _secret_field()

    class Meta:
        table_name = "fp_secret"


def test_write_only_is_never_serialized(mtables):
    async def go():
        await drop_tables(mtables, "fp_secret")
        await FpSecret.create_table(using=mtables)
        await FpSecret.objects.create(using=mtables, login="a", password="hunter2")
        qs = FpSecret.objects.using(mtables)
        return bytes(await qs.json()), await qs.all()

    native, objs = run(go())
    assert FpSecret._meta.schema.field("password").write_only
    assert native == b'[{"id":1,"login":"a"}]'
    assert objs[0].password == "hunter2"  # still loaded on the instance
    assert objs[0].to_dict() == {"id": 1, "login": "a"}
    assert encode(objs) == native
    run(drop_tables(mtables, "fp_secret"))


# ── Models the native encoder must not take: same bytes via instances ────────

class FpProp(FpOwner):
    @property
    def name(self):  # shadows the inherited field
        return "computed"

    class Meta:
        table_name = "fp_owner"


class FpCustom(FpOwner):
    def to_dict(self):
        return {"custom": self.name}

    class Meta:
        table_name = "fp_owner"


def test_property_shadowing_falls_back(mtables):
    assert not orm._native_json(FpProp)
    qs = FpProp.objects.using(mtables).order_by("id")
    native = bytes(call(qs.json))
    assert native == encode([u.to_dict() for u in run(qs.all())])
    assert json.loads(native)[0]["name"] == "computed"


def test_custom_to_dict_is_used(mtables):
    assert not orm._native_json(FpCustom)
    qs = FpCustom.objects.using(mtables).order_by("id").limit(1)
    assert bytes(call(qs.json)) == b'[{"custom":"o1"}]'
    assert encode(run(qs.all())) == b'[{"custom":"o1"}]'


def test_native_json_eligibility(mtables):
    assert orm._native_json(FpAll) and orm._native_json(FpOwner)


class StandInDB:
    """Not a native pool: the fallback paths must give the same bytes."""

    def __init__(self, real):
        self.real = real

    async def fetch(self, sql, *params):
        return await self.real.fetch(sql, *params)


def test_non_native_database_falls_back(mtables):
    qs = FpAll.objects.order_by("id")
    stand = qs.using(StandInDB(mtables))
    assert bytes(call(stand.json)) == bytes(call(qs.using(mtables).json))
    native = run(qs.using(mtables).all())
    fallback = run(stand.all())
    assert [list(vars(a)) for a in native] == [list(vars(b)) for b in fallback]
    assert [repr(vars(a)) for a in native] == [repr(vars(b)) for b in fallback]


# ── Native hydration and to_dict() vs the Python reference ───────────────────

def test_hydration_matches_from_row(mtables):
    qs = FpAll.objects.using(mtables).order_by("id")
    sql, params = qs._select_sql()
    ref = [ref_from_row(FpAll, r) for r in call(mtables.fetch, sql, *params)]
    got = run(qs.all())
    assert [type(o) for o in got] == [FpAll] * len(ref)
    for a, b in zip(got, ref):
        assert list(vars(a)) == list(vars(b))
        for k in vars(b):
            va, vb = vars(a)[k], vars(b)[k]
            assert type(va) is type(vb)
            assert va == vb or (isinstance(va, float) and math.isnan(va) and math.isnan(vb))
    one = run(FpAll.objects.using(mtables).get(id=got[0].id))
    assert list(vars(one)) == list(vars(got[0]))


class FpNew(FpOwner):
    made = 0

    def __new__(cls, *args, **kwargs):
        FpNew.made += 1
        return super().__new__(cls)

    class Meta:
        table_name = "fp_owner"


def test_hydration_calls_user_new(mtables):
    FpNew.made = 0
    rows = run(FpNew.objects.using(mtables).all())
    assert FpNew.made == len(rows) > 0


class FpFromRow(FpOwner):
    @classmethod
    def _from_row(cls, row):
        obj = super()._from_row(row)
        obj.tagged = True
        return obj

    class Meta:
        table_name = "fp_owner"


def test_overridden_from_row_is_used(mtables):
    rows = run(FpFromRow.objects.using(mtables).all())
    assert rows and all(r.tagged for r in rows)
    assert not orm._native_json(FpFromRow)


def test_to_dict_matches_reference_in_every_state():
    owner = FpOwner(id=7, name="o")
    unsaved_owner = FpOwner(name="u")
    cases = [
        FpAll(),
        FpAll(i=1, j={"a": [1]}, owner=owner),
        FpAll(owner=unsaved_owner),
        FpAll(owner=5, other=owner),
        FpAll(owner_id=3),
    ]
    deleted = FpAll(i=1)
    del deleted.__dict__["i"]
    cases.append(deleted)
    extra = FpAll(i=2)
    extra.not_a_field = 1
    cases.append(extra)
    for obj in cases:
        got, ref = obj.to_dict(), ref_to_dict(obj)
        assert type(got) is dict
        assert list(got) == list(ref) and got == ref
        assert encode(obj) == encode(ref)


# ── Serializer: querysets / instances / RawJSON in handler results (HTTP) ────

requests = pytest.importorskip("requests")

_PORT = [17680]


@pytest.fixture(scope="module")
def server(mtables):
    from orm_compat_support import DSN
    from cello import DatabaseConfig

    app = App()
    app.enable_database(DatabaseConfig(url=DSN, pool_size=4))

    def qs():
        return FpAll.objects.using(app.database).order_by("id")

    @app.get("/ping")
    def ping(request):
        return {"ok": True}

    @app.get("/ref")
    async def ref(request):
        return {"rows": [u.to_dict() for u in await qs()], "n": 1}

    @app.get("/nested")
    async def nested(request):
        return {"rows": qs(), "n": 1}

    @app.get("/top")
    async def top(request):
        return qs()

    @app.get("/sync")
    def sync(request):
        return {"rows": qs(), "n": 1}

    @app.get("/sync_blocking", blocking=True)
    def sync_blocking(request):
        return {"rows": qs(), "n": 1}

    @app.get("/multi")
    async def multi(request):
        return [qs().limit(1), {"x": qs().filter(id=-1)}, (qs(),), "tail"]

    @app.get("/multi_ref")
    async def multi_ref(request):
        return [
            [u.to_dict() for u in await qs().limit(1)],
            {"x": []},
            ([u.to_dict() for u in await qs()],),
            "tail",
        ]

    @app.get("/instances")
    async def instances(request):
        return {"rows": await qs(), "n": 1}

    @app.get("/raw")
    async def raw(request):
        return {"rows": await qs().json(), "n": 1}

    @app.get("/one")
    async def one(request):
        return (await qs())[0]

    @app.get("/one_ref")
    async def one_ref(request):
        return (await qs())[0].to_dict()

    @app.get("/bad")
    async def bad(request):
        return {"rows": FpAll.objects.using(app.database).filter(i="not an int")}

    @app.get("/response_json")
    def response_json(request):
        return cello.Response.json({"rows": qs()})

    _PORT[0] += 1
    port = _PORT[0]
    # debug=True: the error-text assertions below need details in the 500 body
    # (without a TTY the default is now production-safe generic errors).
    threading.Thread(
        target=lambda: app.run(host="127.0.0.1", port=port, debug=True), daemon=True
    ).start()
    base = f"http://127.0.0.1:{port}"
    deadline = time.time() + 10
    while time.time() < deadline:
        try:
            requests.get(base + "/ping", timeout=0.5)
            return base
        except Exception:
            time.sleep(0.1)
    raise RuntimeError("server did not start")


def _get(server, path):
    r = requests.get(server + path, timeout=10)
    return r.status_code, r.content


@pytest.mark.parametrize("path", ["/nested", "/sync", "/sync_blocking", "/instances", "/raw"])
def test_http_returned_queryset_matches_taught_shape(server, path):
    status, body = _get(server, path)
    assert (status, body) == _get(server, "/ref")
    assert status == 200


def test_http_top_level_and_multiple_holes(server):
    status, body = _get(server, "/top")
    assert status == 200 and json.loads(body) == json.loads(_get(server, "/ref")[1])["rows"]
    assert _get(server, "/multi") == _get(server, "/multi_ref")


def test_http_model_instance(server):
    assert _get(server, "/one") == _get(server, "/one_ref")


def test_http_deferred_query_error_is_500(server):
    status, body = _get(server, "/bad")
    assert status == 500 and b"ParameterTypeError" in body


def test_queryset_outside_a_handler_result_is_a_clear_error(server):
    status, body = _get(server, "/response_json")
    assert status == 500 and b"QuerySet" in body
    with pytest.raises(TypeError, match="QuerySet"):
        RawJSON.encode({"x": FpAll.objects.all()})


def test_rawjson_api():
    r = RawJSON.encode({"a": [1, "x"]})
    assert bytes(r) == b'{"a":[1,"x"]}' and str(r) == '{"a":[1,"x"]}' and len(r) == 13
    assert r == RawJSON.encode({"a": [1, "x"]}) and "RawJSON" in repr(r)
    assert encode({"nested": r}) == b'{"nested":{"a":[1,"x"]}}'
    with pytest.raises(TypeError):
        RawJSON("[]")
    assert isinstance(FpAll.objects.all(), QuerySet)
