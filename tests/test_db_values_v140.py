"""
v1.4.0 native Postgres value layer (``src/db/value.rs``, ``src/db/error.rs``).

Covers the deliberate breaks listed in docs/releases/v1.4.0.md
("Deliberate breaks — value layer"):

1. NUMERIC decodes to its exact decimal string; Decimal / int / float / str bind.
2. NULL of any type decodes to None.
3. Arrays decode to lists and lists bind to array parameters.
4. "char", interval, money, inet, cidr, macaddr, timetz decode to text.
5. Integer parameters are range-checked.
6. bool binds to text as "true" / "false"; mismatches raise ParameterTypeError.
7. temporal / uuid parameters accept ISO strings and Python objects.
8. SQLSTATE-mapped exception hierarchy.
9. fetchrow / fetchval return the first of several rows.

Where possible the server is the oracle: a decoded value is compared with
the server's own ``::text`` rendering of the same value.
"""

import datetime
import decimal
import random
import uuid

import pytest

import cello
from cello import App, DatabaseConfig

from orm_compat_support import DSN, call, drop_tables, make_db, pg, run

pytestmark = pg

T = "v140_vals"


@pytest.fixture(scope="module")
def db():
    database = make_db()
    yield database
    run(drop_tables(database, T, T + "_child", T + "_parent"))
    call(database.close)


def val(db, sql, *params):
    return call(db.fetchval, sql, *params)


def server_version(db) -> int:
    return int(val(db, "SELECT current_setting('server_version_num')::int"))


# ── 1. NUMERIC ───────────────────────────────────────────────────────────────

NUMERIC_LITERALS = [
    "0", "0.0", "0.000", "-0", "1", "-1", "12.50", "12.3400", "-0.5", "0.0001",
    "0.00001", "9999", "10000", "10001", "99999999", "100000000", "-100000000.00000001",
    "123456789012345678901234567890.123456789012345678901234567890",
    "1e100", "1e-100", "-1.5e-30", "3.14159265358979323846264338327950288",
    "0.1", "0.01", "0.001", "1000.0001", "12345.6789", "-98765.4321000",
    "1e1000", "1e-1000", "99999.99999", "0.99999999999999999999",
]


def test_numeric_decodes_like_server_text(db):
    rows = call(
        db.fetch,
        "SELECT x::numeric AS n, x::numeric::text AS t FROM unnest($1::text[]) AS x",
        NUMERIC_LITERALS,
    )
    assert len(rows) == len(NUMERIC_LITERALS)
    for r in rows:
        assert type(r["n"]) is str
        assert r["n"] == r["t"]


def test_numeric_random_values_decode_like_server_text(db):
    rng = random.Random(1234)
    lits = []
    for _ in range(400):
        ip = str(rng.randrange(0, 10 ** rng.randrange(0, 40)))
        scale = rng.randrange(0, 30)
        fp = "".join(rng.choice("0123456789") for _ in range(scale))
        sign = rng.choice(["", "-"])
        lits.append(f"{sign}{ip}.{fp}" if scale else f"{sign}{ip}")
    rows = call(
        db.fetch,
        "SELECT x::numeric AS n, x::numeric::text AS t FROM unnest($1::text[]) AS x",
        lits,
    )
    assert [r["n"] for r in rows] == [r["t"] for r in rows]


def test_numeric_special_values(db):
    assert val(db, "SELECT 'NaN'::numeric") == "NaN"
    if server_version(db) >= 140000:
        assert val(db, "SELECT 'Infinity'::numeric") == "Infinity"
        assert val(db, "SELECT '-Infinity'::numeric") == "-Infinity"
    assert val(db, "SELECT NULL::numeric") is None


def test_numeric_in_table_and_typmod(db):
    async def main():
        await drop_tables(db, T)
        await db.execute(f"CREATE TABLE {T} (id int, a numeric(10,2), b numeric)")
        await db.execute(f"INSERT INTO {T} VALUES (1, 3.14159, 2.50), (2, NULL, NULL)")
        return await db.fetch(f"SELECT a, b FROM {T} ORDER BY id")

    assert run(main()) == [{"a": "3.14", "b": "2.50"}, {"a": None, "b": None}]


@pytest.mark.parametrize(
    "value",
    [
        decimal.Decimal("12.3400"), decimal.Decimal("-0.5"), decimal.Decimal("0"),
        decimal.Decimal("-0"), decimal.Decimal("1E+3"), decimal.Decimal("1.23E-7"),
        decimal.Decimal("123456789012345678901234567890.000000000000000000001"),
        decimal.Decimal("0.00"), decimal.Decimal("10000"), decimal.Decimal("9999.9999"),
        "12.50", "-1e5", "  7.25  ", "+3", ".5", "5.",
    ],
    ids=str,
)
def test_numeric_bind_matches_server_parse(db, value):
    # Binary encoding must produce exactly what the server's own text input
    # produces (value AND display scale).
    got = val(db, "SELECT $1::numeric", value)
    expected = val(db, "SELECT $1::text::numeric", str(value))
    assert got == expected


def test_numeric_bind_random_decimals_match_server_parse(db):
    rng = random.Random(99)
    vals = []
    for _ in range(300):
        digits = "".join(rng.choice("0123456789") for _ in range(rng.randrange(1, 35)))
        exp = rng.randrange(-40, 40)
        vals.append(decimal.Decimal(f"{rng.choice(['', '-'])}{digits}E{exp}"))

    async def main():
        out = []
        for v in vals:
            out.append(
                (
                    await db.fetchval("SELECT $1::numeric", v),
                    await db.fetchval("SELECT $1::text::numeric", str(v)),
                )
            )
        return out

    for got, expected in run(main()):
        assert got == expected


def test_numeric_bind_int_float_special(db):
    assert val(db, "SELECT $1::numeric", 2**70) == str(2**70)
    assert val(db, "SELECT $1::numeric", -(2**100)) == str(-(2**100))
    assert val(db, "SELECT $1::numeric", 5) == "5"
    assert val(db, "SELECT $1::numeric", 0.1) == "0.1"
    assert val(db, "SELECT $1::numeric", 1.5e-7) == "0.00000015"
    assert val(db, "SELECT $1::numeric", float("nan")) == "NaN"
    assert val(db, "SELECT $1::numeric", decimal.Decimal("NaN")) == "NaN"
    if server_version(db) >= 140000:
        assert val(db, "SELECT $1::numeric", decimal.Decimal("Infinity")) == "Infinity"
        assert val(db, "SELECT $1::numeric", float("-inf")) == "-Infinity"


def test_numeric_bind_invalid_string(db):
    with pytest.raises(cello.ParameterValueError) as ei:
        val(db, "SELECT $1::numeric", "12abc")
    assert isinstance(ei.value, ValueError) and isinstance(ei.value, RuntimeError)
    assert "numeric" in str(ei.value) and "$1" in str(ei.value)


def test_decimal_into_float_still_binds(db):
    assert val(db, "SELECT $1::float8", decimal.Decimal("1.5")) == 1.5


# ── 2. NULL of any type ──────────────────────────────────────────────────────

@pytest.mark.parametrize(
    "typ",
    ["numeric", "inet", "money", "interval", "timetz", "point", "int4[]", "text[]",
     "macaddr", "tsvector", "\"char\"", "box", "xml"],
)
def test_null_of_any_type_is_none(db, typ):
    assert val(db, f"SELECT NULL::{typ}") is None


# ── 3. Arrays ────────────────────────────────────────────────────────────────

ARRAYS = [
    ("ARRAY[1,NULL,3]::int2[]", [1, None, 3]),
    ("ARRAY[1,NULL,3]::int4[]", [1, None, 3]),
    ("ARRAY[9223372036854775807,NULL]::int8[]", [9223372036854775807, None]),
    ("ARRAY[0.5,NULL]::float4[]", [0.5, None]),
    ("ARRAY[0.1,NULL]::float8[]", [0.1, None]),
    ("ARRAY[true,false,NULL]", [True, False, None]),
    ("ARRAY['a',NULL,'日本']::text[]", ["a", None, "日本"]),
    ("ARRAY['a','b']::varchar[]", ["a", "b"]),
    ("ARRAY['a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::uuid, NULL]",
     ["a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11", None]),
    ("ARRAY[1.50, NULL, -2]::numeric[]", ["1.50", None, "-2"]),
    ("ARRAY['2024-01-02 03:04:05+00'::timestamptz, NULL]", ["2024-01-02T03:04:05+00:00", None]),
    ("ARRAY['2024-02-29'::date, NULL]", ["2024-02-29", None]),
    ("ARRAY['{\"a\":[1,null]}'::jsonb, NULL]", [{"a": [1, None]}, None]),
    ("'{}'::text[]", []),
    ("ARRAY[[1,2],[3,4]]", [[1, 2], [3, 4]]),
    ("ARRAY[[[1],[2]],[[3],[NULL::int]]]", [[[1], [2]], [[3], [None]]]),
    ("'[0:1]={7,8}'::int4[]", [7, 8]),  # lower bound ignored
    ("NULL::int4[]", None),
]


@pytest.mark.parametrize("expr,expected", ARRAYS, ids=[a[0] for a in ARRAYS])
def test_array_decode(db, expr, expected):
    got = val(db, f"SELECT {expr}")
    assert got == expected
    if expected:
        assert type(got) is list


def test_array_float4_decodes_with_float4_precision(db):
    assert val(db, "SELECT ARRAY[0.1]::float4[]") == [0.10000000149011612]


BIND_ARRAYS = [
    ("int2[]", [1, None, -3]),
    ("int4[]", [1, None, 2**31 - 1]),
    ("int8[]", [2**63 - 1, None]),
    ("float8[]", [0.25, None]),
    ("bool[]", [True, None, False]),
    ("text[]", ["a", None, "日本"]),
    ("varchar[]", ["x"]),
    ("uuid[]", [uuid.UUID(int=1), "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11"]),
    ("numeric[]", [decimal.Decimal("1.50"), 2, None]),
    ("date[]", [datetime.date(2024, 2, 29), "2024-03-01"]),
    ("jsonb[]", [{"a": 1}, [1, 2], None]),
    ("int4[]", []),
    ("int4[]", [[1, 2], [3, 4]]),
]

BIND_EXPECTED = {
    "uuid[]": ["00000000-0000-0000-0000-000000000001", "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11"],
    "numeric[]": ["1.50", "2", None],
    "date[]": ["2024-02-29", "2024-03-01"],
}


@pytest.mark.parametrize("typ,value", BIND_ARRAYS, ids=[f"{t}-{i}" for i, (t, _) in enumerate(BIND_ARRAYS)])
def test_array_bind_round_trip(db, typ, value):
    got = val(db, f"SELECT $1::{typ}", value)
    assert got == BIND_EXPECTED.get(typ, value)


def test_array_bind_tuple_and_any(db):
    assert val(db, "SELECT $1::int4[]", (1, 2)) == [1, 2]
    assert val(db, "SELECT 2 = ANY($1::int4[])", [1, 2, 3]) is True
    assert call(db.fetch, "SELECT x FROM unnest($1::text[]) AS x", ["p", "q"]) == [
        {"x": "p"}, {"x": "q"},
    ]


def test_array_bind_errors(db):
    with pytest.raises(cello.ParameterValueError):
        val(db, "SELECT $1::int4[]", [[1, 2], [3]])  # ragged
    with pytest.raises(cello.ParameterValueError):
        val(db, "SELECT $1::int4[]", [[1], 2])  # mixed depth
    with pytest.raises(cello.ParameterRangeError):
        val(db, "SELECT $1::int2[]", [1, 2**20])
    with pytest.raises(cello.ParameterTypeError):
        val(db, "SELECT $1::int4[]", ["x"])
    with pytest.raises(cello.ParameterTypeError):
        val(db, "SELECT $1::int4", [1])


def test_list_still_binds_as_json(db):
    # Unchanged from 1.3.0: lists into json / jsonb / text bind as JSON.
    assert val(db, "SELECT $1::jsonb", [1, "x", None]) == [1, "x", None]
    assert val(db, "SELECT $1::text", [1, 2]) == "[1,2]"
    assert val(db, "SELECT $1::json", [{"b": 1, "a": 2}]) == [{"a": 2, "b": 1}]


def test_array_column_round_trip(db):
    async def main():
        await drop_tables(db, T)
        await db.execute(f"CREATE TABLE {T} (id int, tags text[], nums int4[])")
        await db.execute(f"INSERT INTO {T} VALUES ($1, $2, $3)", 1, ["a", "b"], [1, None])
        return await db.fetchrow(f"SELECT tags, nums FROM {T}")

    assert run(main()) == {"tags": ["a", "b"], "nums": [1, None]}


# ── 4. "char", interval, money, inet, cidr, macaddr, timetz ──────────────────

def test_single_char(db):
    assert val(db, "SELECT 'x'::\"char\"") == "x"
    assert val(db, "SELECT ''::\"char\"") == ""
    assert val(db, "SELECT relkind FROM pg_class WHERE relname = 'pg_class'") == "r"


TEXT_FORM = [
    # interval (IntervalStyle = postgres)
    "interval '1 day'", "interval '-1 day'", "interval '0'", "interval '1 year 2 months 3 days 04:05:06.5'",
    "interval '-1 day +2 hours'", "interval '-02:00:00'", "interval '1 day -02:00:01.25'",
    "interval '27 hours'", "interval '1 mon'", "interval '-13 months'", "interval '0.000001 seconds'",
    "interval '1 year -1 mon'", "interval '-1 year -2 days -00:00:03'", "interval '100000 hours'",
    # timetz
    "TIMETZ '13:14:15+02'", "TIMETZ '13:14:15.25-05:30'", "TIMETZ '00:00:00+00'",
    "TIMETZ '23:59:59.999999-12'", "TIMETZ '12:00+05:45'",
    # inet / cidr
    "'192.168.0.1'::inet", "'10.1.0.0/16'::inet", "'10.0.0.0/8'::cidr", "'::1'::inet",
    "'2001:db8::/32'::cidr", "'2001:db8::1/64'::inet", "'0.0.0.0/0'::cidr", "'fe80::1'::inet",
    # macaddr
    "'08:00:2b:01:02:03'::macaddr", "'08:00:2b:01:02:03:04:05'::macaddr8",
]


def test_text_form_types_match_server_text(db):
    if val(db, "SELECT current_setting('IntervalStyle')") != "postgres":
        pytest.skip("IntervalStyle is not 'postgres'")
    for expr in TEXT_FORM:
        # format('%s') renders through the type's output function (a ::text
        # cast of inet would add the /32 netmask).
        row = call(db.fetchrow, f"SELECT {expr} AS v, format('%s', {expr}) AS t")
        assert row["v"] == row["t"], expr
        assert type(row["v"]) is str


def test_money_is_plain_decimal(db):
    assert val(db, "SELECT 12.5::money") == "12.50"
    assert val(db, "SELECT (-0.05)::money") == "-0.05"
    assert val(db, "SELECT 1234567.89::money") == "1234567.89"


def test_infinite_timestamps_and_dates_decode_to_text(db):
    assert val(db, "SELECT 'infinity'::timestamptz") == "infinity"
    assert val(db, "SELECT '-infinity'::timestamp") == "-infinity"
    assert val(db, "SELECT 'infinity'::date") == "infinity"


def test_still_unsupported_type_is_placeholder(db):
    assert val(db, "SELECT point(1,2)") == "<unsupported column type: point>"


def test_enum_decodes_to_text(db):
    async def main():
        await db.execute("DROP TYPE IF EXISTS v140_mood")
        await db.execute("CREATE TYPE v140_mood AS ENUM ('ok', 'sad')")
        try:
            return (
                await db.fetchval("SELECT 'sad'::v140_mood"),
                await db.fetchval("SELECT ARRAY['ok','sad']::v140_mood[]"),
                await db.fetchval("SELECT $1::v140_mood::text", "ok"),
            )
        finally:
            await db.execute("DROP TYPE v140_mood")

    assert run(main()) == ("sad", ["ok", "sad"], "ok")


# ── 5. integer range checks ──────────────────────────────────────────────────

@pytest.mark.parametrize(
    "typ,lo,hi",
    [("int2", -(2**15), 2**15 - 1), ("int4", -(2**31), 2**31 - 1), ("int8", -(2**63), 2**63 - 1)],
)
def test_int_bounds(db, typ, lo, hi):
    assert val(db, f"SELECT $1::{typ}", lo) == lo
    assert val(db, f"SELECT $1::{typ}", hi) == hi
    for bad in (lo - 1, hi + 1):
        with pytest.raises(cello.ParameterRangeError) as ei:
            val(db, f"SELECT $1::{typ}", bad)
        assert isinstance(ei.value, OverflowError)
        assert typ in str(ei.value) and str(bad) in str(ei.value)


def test_oid_range(db):
    assert val(db, "SELECT $1::oid", 4294967295) == 4294967295
    with pytest.raises(cello.ParameterRangeError):
        val(db, "SELECT $1::oid", -1)


def test_big_int_into_float(db):
    assert val(db, "SELECT $1::float8", 2**70) == float(2**70)


# ── 6. bool → text, mismatches ───────────────────────────────────────────────

def test_bool_into_text_types(db):
    assert val(db, "SELECT $1::text", True) == "true"
    assert val(db, "SELECT $1::varchar", False) == "false"
    assert val(db, "SELECT $1::bool", False) is False


def test_mismatch_names_parameter_and_type(db):
    with pytest.raises(cello.ParameterTypeError) as ei:
        val(db, "SELECT $1::int4, $2::int8", 1, "x")
    msg = str(ei.value)
    assert "parameter $2" in msg and "int8" in msg and "str" in msg
    assert isinstance(ei.value, TypeError)
    assert isinstance(ei.value, cello.DataError)
    assert ei.value.sqlstate is None  # client-side, never sent


def test_mismatch_does_not_poison_connection(db):
    for _ in range(5):
        with pytest.raises(cello.ParameterTypeError):
            val(db, "SELECT $1::int4", "x")
    assert val(db, "SELECT $1::int4", 3) == 3


# ── 7. temporal / uuid coercion ──────────────────────────────────────────────

UTC = datetime.timezone.utc
PLUS2 = datetime.timezone(datetime.timedelta(hours=2))

TEMPORAL = [
    ("timestamptz", "2024-01-02T03:04:05Z", "2024-01-02T03:04:05+00:00"),
    ("timestamptz", "2024-01-02 03:04:05.5+05:30", "2024-01-01T21:34:05.500+00:00"),
    ("timestamptz", "2024-01-02T03:04:05", "2024-01-02T03:04:05+00:00"),  # naive = UTC
    ("timestamptz", "2024-01-02", "2024-01-02T00:00:00+00:00"),
    ("timestamptz", "infinity", "infinity"),
    ("timestamptz", datetime.datetime(2024, 1, 2, 3, 4, 5, 6), "2024-01-02T03:04:05.000006+00:00"),
    ("timestamptz", datetime.datetime(2024, 1, 2, 3, tzinfo=PLUS2), "2024-01-02T01:00:00+00:00"),
    ("timestamptz", datetime.date(2024, 1, 2), "2024-01-02T00:00:00+00:00"),
    ("timestamp", "2024-01-02T03:04:05.123456", "2024-01-02T03:04:05.123456"),
    ("timestamp", "2024-01-02T03:04:05+05:00", "2024-01-02T03:04:05.000000"),  # offset ignored
    ("timestamp", datetime.datetime(2024, 1, 2, 3, tzinfo=PLUS2), "2024-01-02T03:00:00.000000"),
    ("timestamp", datetime.datetime(1, 1, 1), "0001-01-01T00:00:00.000000"),
    ("date", "2024-02-29", "2024-02-29"),
    ("date", datetime.date(2024, 2, 29), "2024-02-29"),
    ("date", datetime.datetime(2024, 2, 29, 23, 59), "2024-02-29"),  # time ignored
    ("date", "-infinity", "-infinity"),
    ("time", "13:14:15", "13:14:15"),
    ("time", "13:14", "13:14:00"),
    ("time", datetime.time(13, 14, 15, 250000), "13:14:15.250"),
    ("time", "13:14:15.0000005", "13:14:15.000001"),  # rounded to µs
]


@pytest.mark.parametrize("typ,value,expected", TEMPORAL, ids=[f"{t}-{v!r}" for t, v, _ in TEMPORAL])
def test_temporal_bind(db, typ, value, expected):
    assert val(db, f"SELECT $1::{typ}", value) == expected


def test_temporal_bind_agrees_with_server_parse(db):
    # Offsets and fractional seconds as the server parses them (session UTC).
    for s in ["2024-06-30T23:59:59.999999-07:00", "1999-12-31 00:00:00+14", "2000-01-01T00:00:00Z"]:
        got = val(db, "SELECT $1::timestamptz", s)
        expected = val(db, "SELECT $1::text::timestamptz", s)
        assert got == expected, s


@pytest.mark.parametrize(
    "typ,value,exc",
    [
        ("timestamptz", "not a date", cello.ParameterValueError),
        ("date", "2024-02-30", cello.ParameterValueError),
        ("time", "25:00", cello.ParameterValueError),
        ("time", "2024-01-01", cello.ParameterValueError),
        ("date", datetime.time(1, 2), cello.ParameterTypeError),
        ("time", datetime.date(2024, 1, 1), cello.ParameterTypeError),
        ("text", datetime.datetime(2024, 1, 1), cello.ParameterTypeError),
        ("uuid", "not-a-uuid", cello.ParameterValueError),
        ("uuid", 5, cello.ParameterTypeError),
    ],
)
def test_temporal_and_uuid_bind_errors(db, typ, value, exc):
    with pytest.raises(exc) as ei:
        val(db, f"SELECT $1::{typ}", value)
    assert isinstance(ei.value, RuntimeError)


def test_uuid_bind(db):
    u = uuid.uuid4()
    assert val(db, "SELECT $1::uuid", u) == str(u)
    assert val(db, "SELECT $1::uuid", str(u).upper()) == str(u)
    assert val(db, "SELECT $1::uuid", u.hex) == str(u)


def test_datetime_column_insert_and_filter(db):
    async def main():
        await drop_tables(db, T)
        await db.execute(f"CREATE TABLE {T} (id int, at timestamptz, d date)")
        await db.execute(
            f"INSERT INTO {T} VALUES (1, $1, $2), (2, $3, $4)",
            datetime.datetime(2024, 1, 1, tzinfo=UTC), datetime.date(2024, 1, 1),
            "2024-06-01T12:00:00Z", "2024-06-01",
        )
        return await db.fetch(
            f"SELECT id FROM {T} WHERE at > $1 AND d >= $2 ORDER BY id",
            datetime.datetime(2024, 3, 1), "2024-01-01",
        )

    assert run(main()) == [{"id": 2}]


# ── 8. exception hierarchy ───────────────────────────────────────────────────

def test_hierarchy_exported():
    assert issubclass(cello.DatabaseError, RuntimeError)
    for name in ["IntegrityError", "DataError", "OperationalError", "ProgrammingError"]:
        assert issubclass(getattr(cello, name), cello.DatabaseError)
    for name in ["UniqueViolation", "NotNullViolation", "ForeignKeyViolation", "CheckViolation"]:
        assert issubclass(getattr(cello, name), cello.IntegrityError)
    assert issubclass(cello.ParameterTypeError, (cello.DataError))
    assert issubclass(cello.ParameterTypeError, TypeError)
    assert issubclass(cello.ParameterRangeError, OverflowError)
    assert issubclass(cello.ParameterValueError, ValueError)
    for name in ["DatabaseError", "IntegrityError", "UniqueViolation", "ParameterTypeError"]:
        assert name in cello.__all__
    assert cello.DatabaseError.sqlstate is None


def test_integrity_errors_carry_details(db):
    async def main():
        await drop_tables(db, T + "_child", T + "_parent")
        await db.execute(f"CREATE TABLE {T}_parent (id int PRIMARY KEY)")
        await db.execute(
            f"CREATE TABLE {T}_child (id int PRIMARY KEY, name text NOT NULL,"
            f" qty int CONSTRAINT {T}_qty_pos CHECK (qty > 0),"
            f" parent_id int CONSTRAINT {T}_fk REFERENCES {T}_parent(id))"
        )
        await db.execute(f"INSERT INTO {T}_parent VALUES (1)")
        await db.execute(f"INSERT INTO {T}_child VALUES (1, 'a', 1, 1)")
        out = {}
        for key, sql in [
            ("unique", f"INSERT INTO {T}_child VALUES (1, 'b', 1, 1)"),
            ("notnull", f"INSERT INTO {T}_child VALUES (2, NULL, 1, 1)"),
            ("check", f"INSERT INTO {T}_child VALUES (3, 'c', 0, 1)"),
            ("fk", f"INSERT INTO {T}_child VALUES (4, 'd', 1, 99)"),
        ]:
            try:
                await db.execute(sql)
            except cello.DatabaseError as e:
                out[key] = e
        return out

    errs = run(main())
    u = errs["unique"]
    assert type(u) is cello.UniqueViolation and isinstance(u, RuntimeError)
    assert (u.sqlstate, u.table, u.constraint) == ("23505", T + "_child", T + "_child_pkey")
    assert "already exists" in u.detail
    n = errs["notnull"]
    assert type(n) is cello.NotNullViolation
    assert (n.sqlstate, n.table, n.column) == ("23502", T + "_child", "name")
    c = errs["check"]
    assert type(c) is cello.CheckViolation and c.constraint == f"{T}_qty_pos"
    f = errs["fk"]
    assert type(f) is cello.ForeignKeyViolation and f.constraint == f"{T}_fk"
    assert isinstance(f, cello.IntegrityError)


def test_data_and_programming_errors(db):
    with pytest.raises(cello.DataError) as ei:
        val(db, "SELECT 1/0")
    assert ei.value.sqlstate == "22012"
    with pytest.raises(cello.DataError) as ei:
        val(db, "SELECT 'x'::int")
    assert ei.value.sqlstate == "22P02"
    with pytest.raises(cello.ProgrammingError) as ei:
        val(db, "SELECT * FROM v140_no_such_table")
    assert ei.value.sqlstate == "42P01" and "v140_no_such_table" in str(ei.value)
    with pytest.raises(cello.ProgrammingError):
        val(db, "SELEC 1")
    with pytest.raises(cello.ProgrammingError):
        val(db, "SELECT $1::int, $2::int", 1)  # wrong parameter count


def test_operational_error_on_unreachable_server():
    app = App()
    app.enable_database(DatabaseConfig(url="postgresql://u:p@127.0.0.1:1/x", pool_size=1))
    with pytest.raises(cello.OperationalError) as ei:
        call(app.database.fetchval, "SELECT 1")
    assert isinstance(ei.value, RuntimeError)


def test_integrity_error_inside_transaction(db):
    async def main():
        await drop_tables(db, T)
        await db.execute(f"CREATE TABLE {T} (id int PRIMARY KEY)")
        try:
            async with db.transaction() as tx:
                await tx.execute(f"INSERT INTO {T} VALUES (1)")
                await tx.execute(f"INSERT INTO {T} VALUES (1)")
        except cello.UniqueViolation as e:
            caught = e.sqlstate
        return caught, await db.fetchval(f"SELECT count(*) FROM {T}")

    assert run(main()) == ("23505", 0)


# ── 9. fetchrow / fetchval with several rows ─────────────────────────────────

def test_first_row_of_many(db):
    assert call(db.fetchrow, "SELECT g, g * 2 AS d FROM generate_series(4, 9) g") == {"g": 4, "d": 8}
    assert val(db, "SELECT g FROM generate_series(4, 9) g") == 4
    assert call(db.fetchrow, "SELECT 1 WHERE false") is None


def test_discarded_rows_do_not_leak_into_next_query():
    # One connection: the rows dropped after the first must not be seen by
    # the next statement on it.
    app = App()
    app.enable_database(DatabaseConfig(url=DSN, pool_size=1))
    d = app.database

    async def main():
        out = []
        for i in range(20):
            out.append(await d.fetchval("SELECT g FROM generate_series($1::int, 100000) g", i))
            out.append(await d.fetchval("SELECT $1::text", f"next{i}"))
        await d.close()
        return out

    got = run(main())
    assert got == [x for i in range(20) for x in (i, f"next{i}")]


def test_transaction_fetchrow_first_row(db):
    async def main():
        async with db.transaction() as tx:
            a = await tx.fetchrow("SELECT g FROM generate_series(1, 5) g")
            b = await tx.fetchrow("SELECT 'after' AS s")
        return a, b

    assert run(main()) == ({"g": 1}, {"s": "after"})


def test_statement_cache_retry_still_works_with_first_row_path(db):
    async def main():
        await drop_tables(db, T)
        await db.execute(f"CREATE TABLE {T} (a int)")
        await db.execute(f"INSERT INTO {T} VALUES (1), (2)")
        first = await db.fetchrow(f"SELECT * FROM {T} ORDER BY a")
        await db.execute(f"ALTER TABLE {T} ADD COLUMN b numeric DEFAULT 1.50")
        second = await db.fetchrow(f"SELECT * FROM {T} ORDER BY a")
        return first, second

    assert run(main()) == ({"a": 1}, {"a": 1, "b": "1.50"})
