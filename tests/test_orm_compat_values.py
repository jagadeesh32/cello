"""
ORM compatibility suite — frozen value shapes.

Pins exactly what ``pg_value_to_py`` (``src/db/value.rs``) returns today for
each Postgres type, through every raw entry point (``fetch`` / ``fetchrow`` /
``fetchval`` / transaction ``fetch``), through real table columns, and through
ORM model instances. Also pins how Python parameters are bound
(``py_to_sqlparam`` / ``SqlParam::to_sql``), since the ORM relies on it for
every write and filter.

See ``tests/orm_compat_support.py`` for conventions (``# PINNED-QUIRK:``).
"""

import datetime
import decimal
import math

import pytest

import cello
from cello.orm import (
    AutoField,
    BigIntegerField,
    BooleanField,
    CharField,
    DateTimeField,
    FloatField,
    IntegerField,
    JSONField,
    Model,
    TextField,
)

from orm_compat_support import bind, call, drop_tables, make_db, pg, run

pytestmark = pg

T_TYPES = "oc_val_types"
T_MODEL = "oc_val_model"


# ── Literal -> Python shape table ─────────────────────────────────────────────
# (sql expression, exact expected python value, exact expected type)

SHAPES = [
    # TIMESTAMPTZ: RFC 3339 string, always normalised to UTC ("+00:00"),
    # fractional seconds printed with chrono's automatic precision
    # (none / millis / micros) — not a datetime.
    ("TIMESTAMPTZ '2024-01-02 03:04:05.123456+00'", "2024-01-02T03:04:05.123456+00:00", str),
    ("TIMESTAMPTZ '2024-01-02 03:04:05+00'", "2024-01-02T03:04:05+00:00", str),
    ("TIMESTAMPTZ '2024-01-02 03:04:05.1+00'", "2024-01-02T03:04:05.100+00:00", str),
    ("TIMESTAMPTZ '2024-01-02 03:04:05.123+00'", "2024-01-02T03:04:05.123+00:00", str),
    ("TIMESTAMPTZ '2024-01-02 03:04:05.1234+00'", "2024-01-02T03:04:05.123400+00:00", str),
    ("TIMESTAMPTZ '2024-01-02 03:04:05+05:30'", "2024-01-01T21:34:05+00:00", str),
    # TIMESTAMP: exact %Y-%m-%dT%H:%M:%S%.6f (always 6 fractional digits).
    ("TIMESTAMP '2024-01-02 03:04:05'", "2024-01-02T03:04:05.000000", str),
    ("TIMESTAMP '2024-01-02 03:04:05.5'", "2024-01-02T03:04:05.500000", str),
    ("TIMESTAMP '1999-12-31 23:59:59.999999'", "1999-12-31T23:59:59.999999", str),
    # DATE / TIME: strings.
    ("DATE '2024-02-29'", "2024-02-29", str),
    ("DATE '0099-01-01'", "0099-01-01", str),
    ("TIME '13:14:15'", "13:14:15", str),
    ("TIME '13:14:15.25'", "13:14:15.250", str),
    ("TIME '00:00:00.000001'", "00:00:00.000001", str),
    # UUID: lower-case hyphenated string.
    ("'A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11'::uuid", "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11", str),
    # bool / ints / oid.
    ("true", True, bool),
    ("false", False, bool),
    ("(-32768)::int2", -32768, int),
    ("2147483647::int4", 2147483647, int),
    ("(-9223372036854775808)::int8", -9223372036854775808, int),
    ("9223372036854775807::int8", 9223372036854775807, int),
    ("4294967295::oid", 4294967295, int),
    ("1", 1, int),
    # floats: float4 is widened to a Python float with float4 precision.
    ("0.1::float4", 0.10000000149011612, float),
    ("0.1::float8", 0.1, float),
    ("1e308::float8", 1e308, float),
    ("'Infinity'::float8", math.inf, float),
    ("'-Infinity'::float8", -math.inf, float),
    # text-like.
    ("'x'::text", "x", str),
    ("'x'::varchar(5)", "x", str),
    ("'x'::char(3)", "x  ", str),  # bpchar keeps its blank padding
    ("'x'::name", "x", str),
    ("'abc'", "abc", str),  # untyped literal resolves to text
    ("''::text", "", str),
    ("'héllo ✓ 日本 🎻'::text", "héllo ✓ 日本 🎻", str),
    # bytea -> bytes.
    ("'\\xdeadbeef'::bytea", b"\xde\xad\xbe\xef", bytes),
    ("''::bytea", b"", bytes),
    # NULL of a supported type -> None.
    ("NULL", None, type(None)),
    ("NULL::int4", None, type(None)),
    ("NULL::text", None, type(None)),
    ("NULL::timestamptz", None, type(None)),
    ("NULL::jsonb", None, type(None)),
    ("NULL::uuid", None, type(None)),
    ("NULL::bytea", None, type(None)),
]

# JSON / JSONB -> nested Python objects (never a raw string).
JSON_SHAPES = [
    ("'{\"a\":{\"b\":[1,2.5,null,true,\"x\"]}}'::jsonb", {"a": {"b": [1, 2.5, None, True, "x"]}}),
    ("'{\"a\":{\"b\":[1,2.5,null,true,\"x\"]}}'::json", {"a": {"b": [1, 2.5, None, True, "x"]}}),
    ("'[1,2]'::jsonb", [1, 2]),
    ("'[]'::jsonb", []),
    ("'{}'::jsonb", {}),
    ("'\"s\"'::jsonb", "s"),
    ("'null'::jsonb", None),
    ("'true'::jsonb", True),
    ("'1'::jsonb", 1),
    ("'1.0'::jsonb", 1.0),
    ("'-5'::jsonb", -5),
    ("'18446744073709551615'::jsonb", 18446744073709551615),
    ("'{\"k\":\"日本\"}'::jsonb", {"k": "日本"}),
]

# Types that decoded to the "<unsupported column type: ...>" placeholder in
# 1.3.0 and are decoded in 1.4.0 — a deliberate break, see
# docs/releases/v1.4.0.md "Deliberate breaks — value layer".
DECODED_SINCE_140 = [
    # NUMERIC -> exact decimal string (scale kept), NULL -> None.
    ("12.50::numeric", "12.50"),
    ("12.50::numeric(10,2)", "12.50"),
    ("NULL::numeric", None),
    # NULL of any type -> None.
    ("NULL::inet", None),
    # inet / money / interval / timetz -> Postgres text form (money without
    # the locale's currency symbol).
    ("'192.168.0.1'::inet", "192.168.0.1"),
    ("12.5::money", "12.50"),
    ("interval '1 day'", "1 day"),
    ("TIMETZ '13:14:15+02'", "13:14:15+02"),
    # arrays -> lists.
    ("ARRAY[1,2,3]", [1, 2, 3]),
    ("ARRAY['a','b']", ["a", "b"]),
    ("'{}'::int4[]", []),
]

# Types still not supported: a non-NULL value decodes to the diagnostic
# placeholder string.
PLACEHOLDERS = [
    ("point(1,2)", "<unsupported column type: point>"),
    ("ARRAY[point(1,2)]", "<unsupported column type: _point>"),
]


def _close(db):
    call(db.close)


@pytest.fixture(scope="module")
def db():
    database = make_db()
    yield database
    run(drop_tables(database, T_TYPES, T_MODEL))
    _close(database)


def _same(actual, expected, typ):
    assert type(actual) is typ, (actual, typ)
    if isinstance(expected, float) and math.isnan(expected):
        assert math.isnan(actual)
    else:
        assert actual == expected


# ── Raw: literals through each entry point ────────────────────────────────────

@pytest.mark.parametrize("expr,expected,typ", SHAPES, ids=[s[0] for s in SHAPES])
def test_raw_scalar_shape_fetchval(db, expr, expected, typ):
    _same(call(db.fetchval, f"SELECT {expr} AS v"), expected, typ)


@pytest.mark.parametrize("expr,expected,typ", SHAPES, ids=[s[0] for s in SHAPES])
def test_raw_scalar_shape_fetchrow_and_fetch(db, expr, expected, typ):
    row = call(db.fetchrow, f"SELECT {expr} AS v")
    assert type(row) is dict and list(row) == ["v"]
    _same(row["v"], expected, typ)
    rows = call(db.fetch, f"SELECT {expr} AS v")
    assert type(rows) is list and len(rows) == 1 and type(rows[0]) is dict
    _same(rows[0]["v"], expected, typ)


def test_raw_float_nan(db):
    v = call(db.fetchval, "SELECT 'NaN'::float8")
    assert type(v) is float and math.isnan(v)


@pytest.mark.parametrize("expr,expected", JSON_SHAPES, ids=[s[0] for s in JSON_SHAPES])
def test_raw_json_shape(db, expr, expected):
    v = call(db.fetchval, f"SELECT {expr} AS v")
    assert v == expected
    assert type(v) is type(expected)


def test_raw_json_nested_types_exact(db):
    v = call(db.fetchval, "SELECT '{\"i\":1,\"f\":2.5,\"b\":false,\"n\":null,\"l\":[{}]}'::jsonb")
    assert type(v["i"]) is int
    assert type(v["f"]) is float
    assert v["b"] is False
    assert v["n"] is None
    assert v["l"] == [{}] and type(v["l"]) is list and type(v["l"][0]) is dict


def test_raw_json_object_keys_are_sorted(db):
    # PINNED-QUIRK: object keys come back in sorted order, even for ``json``
    # (which preserves input order server-side) — serde_json's default map.
    for cast in ("json", "jsonb"):
        v = call(db.fetchval, f"SELECT '{{\"b\":1,\"a\":2,\"c\":3}}'::{cast}")
        assert list(v) == ["a", "b", "c"]


def test_raw_json_out_of_range_number_raises(db):
    # PINNED-QUIRK: a JSON number outside f64 range fails the whole query.
    with pytest.raises(RuntimeError):
        call(db.fetchval, "SELECT '1e400'::jsonb")


@pytest.mark.parametrize("expr,expected", PLACEHOLDERS, ids=[s[0] for s in PLACEHOLDERS])
def test_raw_unknown_type_placeholder(db, expr, expected):
    v = call(db.fetchval, f"SELECT {expr} AS v")
    assert v == expected and type(v) is str


@pytest.mark.parametrize(
    "expr,expected", DECODED_SINCE_140, ids=[s[0] for s in DECODED_SINCE_140]
)
def test_raw_formerly_unsupported_types_decode(db, expr, expected):
    # v1.4.0 deliberate break (was the placeholder string in 1.3.0).
    v = call(db.fetchval, f"SELECT {expr} AS v")
    assert v == expected and type(v) is type(expected)


def test_raw_unsupported_type_null_is_none(db):
    # v1.4.0 deliberate break: NULL of an unsupported type was the placeholder.
    assert call(db.fetchval, "SELECT NULL::point") is None


def test_raw_single_char_type_decodes(db):
    # v1.4.0 deliberate break: the one-byte "char" type raised in 1.3.0; it
    # now decodes to a one-character str.
    assert call(db.fetchval, "SELECT 'x'::\"char\"") == "x"


# ── Raw: real table columns ───────────────────────────────────────────────────

def test_raw_table_columns_round_trip(db):
    async def main():
        await drop_tables(db, T_TYPES)
        await db.execute(
            f"CREATE TABLE {T_TYPES} ("
            " id SERIAL PRIMARY KEY, ts TIMESTAMPTZ, tsn TIMESTAMP, d DATE, t TIME,"
            " u UUID, n NUMERIC(10,2), j JSONB, js JSON, b BOOLEAN, i2 SMALLINT,"
            " i4 INTEGER, i8 BIGINT, f4 REAL, f8 DOUBLE PRECISION, tx TEXT,"
            " vc VARCHAR(20), bc CHAR(4), by BYTEA, ip INET)"
        )
        await db.execute(
            f"INSERT INTO {T_TYPES} (ts, tsn, d, t, u, n, j, js, b, i2, i4, i8, f4, f8,"
            " tx, vc, bc, by, ip) VALUES ("
            " '2024-05-06 07:08:09.010203+00', '2024-05-06 07:08:09', '2024-05-06',"
            " '07:08:09', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 3.14,"
            " '{\"k\":[1,{\"z\":null}]}', '[true]', true, 7, 8, 9, 1.5, 2.25,"
            " 'text', 'vc', 'ab', '\\x00ff', '10.0.0.1')"
        )
        await db.execute(f"INSERT INTO {T_TYPES} DEFAULT VALUES")
        return await db.fetch(f"SELECT * FROM {T_TYPES} ORDER BY id")

    full, empty = run(main())
    assert list(full) == [
        "id", "ts", "tsn", "d", "t", "u", "n", "j", "js", "b", "i2", "i4", "i8",
        "f4", "f8", "tx", "vc", "bc", "by", "ip",
    ]
    assert full == {
        "id": 1,
        "ts": "2024-05-06T07:08:09.010203+00:00",
        "tsn": "2024-05-06T07:08:09.000000",
        "d": "2024-05-06",
        "t": "07:08:09",
        "u": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
        "n": "3.14",  # v1.4.0: exact decimal string (was the placeholder)
        "j": {"k": [1, {"z": None}]},
        "js": [True],
        "b": True,
        "i2": 7,
        "i4": 8,
        "i8": 9,
        "f4": 1.5,
        "f8": 2.25,
        "tx": "text",
        "vc": "vc",
        "bc": "ab  ",
        "by": b"\x00\xff",
        "ip": "10.0.0.1",  # v1.4.0: text form (was the placeholder)
    }
    # All-NULL row: every column decodes to None (v1.4.0; numeric and inet
    # were the placeholder in 1.3.0).
    expected_empty = {k: None for k in full}
    expected_empty["id"] = 2
    assert empty == expected_empty


def test_raw_transaction_fetch_same_shapes(db):
    async def main():
        async with db.transaction() as tx:
            row = await tx.fetchrow(
                "SELECT TIMESTAMPTZ '2024-01-02 03:04:05+00' AS ts, '{\"a\":1}'::jsonb AS j,"
                " 1.5::numeric AS n, 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::uuid AS u"
            )
            rows = await tx.fetch("SELECT DATE '2024-02-29' AS d")
            none = await tx.fetchrow("SELECT 1 WHERE false")
        return row, rows, none

    row, rows, none = run(main())
    assert row == {
        "ts": "2024-01-02T03:04:05+00:00",
        "j": {"a": 1},
        "n": "1.5",  # v1.4.0: was the placeholder
        "u": "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
    }
    assert rows == [{"d": "2024-02-29"}]
    assert none is None


# ── Raw: fetch / fetchrow / fetchval result-shape rules ───────────────────────

def test_raw_empty_results(db):
    assert call(db.fetch, "SELECT 1 AS v WHERE false") == []
    assert call(db.fetchrow, "SELECT 1 AS v WHERE false") is None
    assert call(db.fetchval, "SELECT 1 AS v WHERE false") is None
    assert call(db.fetchval, "SELECT NULL") is None


def test_raw_column_order_and_names(db):
    row = call(db.fetchrow, "SELECT 3 AS c, 1 AS a, 2 AS b, 4 AS \"MixedCase\"")
    assert list(row.items()) == [("c", 3), ("a", 1), ("b", 2), ("MixedCase", 4)]
    # Unaliased expressions take Postgres' generated names.
    assert call(db.fetchrow, "SELECT 1, 'x'") == {"?column?": "x"}


def test_raw_duplicate_column_names_collapse(db):
    # PINNED-QUIRK: rows are dicts, so duplicate names collapse to the LAST
    # value — and fetchval (documented as "first column") returns it too.
    assert call(db.fetchrow, "SELECT 1 AS a, 2 AS a") == {"a": 2}
    assert call(db.fetchval, "SELECT 1 AS a, 2 AS a") == 2


def test_raw_fetchrow_and_fetchval_return_first_of_multiple_rows(db):
    # v1.4.0 deliberate break: 1.3.0 raised "unexpected number of rows" when
    # the query yielded more than one row; now the first row is returned.
    assert call(db.fetchrow, "SELECT * FROM generate_series(1, 3) AS g") == {"g": 1}
    assert call(db.fetchval, "SELECT * FROM generate_series(1, 3)") == 1
    assert call(db.fetch, "SELECT * FROM generate_series(1, 3) AS g") == [
        {"g": 1}, {"g": 2}, {"g": 3},
    ]


def test_raw_execute_returns_rowcount(db):
    assert call(db.execute, "SELECT * FROM generate_series(1, 4)") == 4
    assert type(call(db.execute, "SELECT 1")) is int


# ── Raw: parameter binding ────────────────────────────────────────────────────

def test_param_binding_round_trips(db):
    async def main():
        return [
            await db.fetchval("SELECT $1::text", "s"),
            await db.fetchval("SELECT $1::text", "日本 🎻"),
            await db.fetchval("SELECT $1::int2", 7),
            await db.fetchval("SELECT $1::int4", -7),
            await db.fetchval("SELECT $1::int8", 2**63 - 1),
            await db.fetchval("SELECT $1::float8", 1.25),
            await db.fetchval("SELECT $1::float8", 3),  # int binds into float8
            await db.fetchval("SELECT $1::float4", 0.1),
            await db.fetchval("SELECT $1::bool", True),
            await db.fetchval("SELECT $1::bytea", b"\x00\x01"),
            await db.fetchval("SELECT $1::jsonb", {"a": [1, None]}),
            await db.fetchval("SELECT $1::jsonb", [1, "x"]),
            await db.fetchval("SELECT $1::int4", None),
            await db.fetchval("SELECT $1::text IS NULL", None),
        ]

    out = run(main())
    assert out == [
        "s", "日本 🎻", 7, -7, 2**63 - 1, 1.25, 3.0, 0.10000000149011612, True,
        b"\x00\x01", {"a": [1, None]}, [1, "x"], None, True,
    ]
    assert type(out[6]) is float


def test_param_dict_and_list_bind_as_json_text(db):
    # dict / list parameters bind as JSON; into a text slot they arrive as the
    # compact, key-sorted JSON text.
    assert call(db.fetchval, "SELECT $1::text", {"b": 1, "a": 2}) == '{"a":2,"b":1}'
    assert call(db.fetchval, "SELECT $1::text", [1, 2]) == "[1,2]"


def test_param_bool_into_text_binds_true_false(db):
    # v1.4.0 deliberate break: 1.3.0 sent the binary byte ("\x01", and False
    # failed as a NUL byte).
    assert call(db.fetchval, "SELECT $1::text", True) == "true"
    assert call(db.fetchval, "SELECT $1::text", False) == "false"


def test_param_int_overflow_raises(db):
    # v1.4.0 deliberate break: 1.3.0 wrapped (two's complement) silently.
    for sql, value in [
        ("SELECT $1::int4", 2**31),
        ("SELECT $1::int4", 2**32 + 5),
        ("SELECT $1::int2", 2**15),
    ]:
        with pytest.raises(cello.ParameterRangeError) as ei:
            call(db.fetchval, sql, value)
        assert isinstance(ei.value, OverflowError)
        assert isinstance(ei.value, RuntimeError)
        assert "$1" in str(ei.value)


def test_param_int_beyond_i64_raises(db):
    # v1.4.0 deliberate break: 1.3.0 bound the float's IEEE-754 bits as int8.
    with pytest.raises(cello.ParameterRangeError):
        call(db.fetchval, "SELECT $1::int8", 2**63)


@pytest.mark.parametrize(
    "sql,param",
    [
        ("SELECT $1::text", 5),        # int into text
        ("SELECT $1::text", 1.5),      # float into text
        ("SELECT $1::int4", 1.5),      # float into int4
        ("SELECT $1::bool", 1),        # int into bool
        ("SELECT $1::jsonb", "x"),     # str into jsonb
        ("SELECT $1::jsonb", 5),       # int into jsonb
        ("SELECT $1", 5),              # untyped parameter
    ],
)
def test_param_type_mismatches_raise_runtime_error(db, sql, param):
    # Mismatched parameters still raise RuntimeError. v1.4.0: the error is a
    # cello.ParameterTypeError (DataError -> DatabaseError -> RuntimeError, and
    # TypeError) raised before the query runs, naming the parameter and its
    # Postgres type. str into numeric / timestamptz was removed from this
    # list: those strings are parsed since v1.4.0 (deliberate break).
    with pytest.raises(RuntimeError) as ei:
        call(db.fetchval, sql, param)
    assert isinstance(ei.value, cello.ParameterTypeError)
    assert isinstance(ei.value, TypeError)
    assert "parameter $1" in str(ei.value)


def test_param_int_into_numeric_is_accepted(db):
    # An int parameter into a numeric slot is accepted (v1.4.0: the result is
    # the exact decimal string instead of the placeholder).
    assert call(db.fetchval, "SELECT $1::numeric", 5) == "5"


@pytest.mark.parametrize(
    "param",
    [datetime.datetime(2024, 1, 1), datetime.date(2024, 1, 1), object()],
    ids=["datetime", "date", "object"],
)
def test_param_unserialisable_objects_raise_type_error(db, param):
    # Non-primitive objects fall back to json.dumps, whose TypeError propagates.
    with pytest.raises(TypeError):
        call(db.fetchval, "SELECT $1::text", param)


def test_param_decimal_binds_as_float(db):
    # Anything float()-convertible (e.g. Decimal) binds as a float parameter.
    assert call(db.fetchval, "SELECT $1::float8", decimal.Decimal("1.5")) == 1.5
    with pytest.raises(RuntimeError):
        call(db.fetchval, "SELECT $1::text", decimal.Decimal("1.5"))


# ── ORM: value shapes on model instances ──────────────────────────────────────

class ValModel(Model):
    id = AutoField()
    label = CharField(max_length=50)
    note = TextField(null=True)
    n = IntegerField(null=True)
    big = BigIntegerField(null=True)
    score = FloatField(null=True)
    flag = BooleanField(null=True)
    doc = JSONField(null=True)
    at = DateTimeField(null=True)

    class Meta:
        table_name = T_MODEL


@pytest.fixture
def vtable(db):
    bind(db)
    run(ValModel.drop_table())
    run(ValModel.create_table())
    yield db
    run(ValModel.drop_table())


def test_orm_value_shapes_from_create_and_fetch(vtable):
    async def main():
        created = await ValModel.objects.create(
            label="héllo 🎻", note="", n=-5, big=2**62, score=0.1, flag=False,
            doc={"z": 1, "a": [True, None, 1.5]},
        )
        # DateTimeField can only be written through raw SQL (see below).
        await vtable.execute(
            f"UPDATE {T_MODEL} SET at = '2024-03-04 05:06:07.5+02' WHERE id = $1", created.id
        )
        fetched = await ValModel.objects.get(id=created.id)
        return created, fetched

    created, fetched = run(main())
    # Instance attributes after create() come from the RETURNING row.
    assert created.to_dict() == {
        "id": 1, "label": "héllo 🎻", "note": "", "n": -5, "big": 2**62,
        "score": 0.1, "flag": False, "doc": {"a": [True, None, 1.5], "z": 1}, "at": None,
    }
    assert fetched.to_dict() == {
        "id": 1, "label": "héllo 🎻", "note": "", "n": -5, "big": 2**62,
        "score": 0.1, "flag": False, "doc": {"a": [True, None, 1.5], "z": 1},
        "at": "2024-03-04T03:06:07.500+00:00",
    }
    assert type(fetched.at) is str
    assert type(fetched.doc) is dict and list(fetched.doc) == ["a", "z"]
    assert type(fetched.score) is float and type(fetched.big) is int
    assert fetched.flag is False and fetched.note == ""


def test_orm_all_null_values(vtable):
    obj = run(ValModel.objects.create(label="x"))
    got = run(ValModel.objects.get(id=obj.id))
    for name in ("note", "n", "big", "score", "flag", "doc", "at"):
        assert getattr(got, name) is None
        assert getattr(obj, name) is None


def test_orm_int_written_to_float_field_reads_back_float(vtable):
    obj = run(ValModel.objects.create(label="x", score=3))
    assert obj.score == 3.0 and type(obj.score) is float


def test_orm_float_precision(vtable):
    vals = [0.1, 1 / 3, 1e-300, 1.7976931348623157e308, -0.0, 123456789.123456789]
    async def main():
        out = []
        for v in vals:
            o = await ValModel.objects.create(label="f", score=v)
            out.append((await ValModel.objects.get(id=o.id)).score)
        return out
    assert run(main()) == vals


def test_orm_large_ints(vtable):
    async def main():
        a = await ValModel.objects.create(label="a", big=2**63 - 1, n=2**31 - 1)
        b = await ValModel.objects.create(label="b", big=-(2**63), n=-(2**31))
        return a, b
    a, b = run(main())
    assert (a.big, a.n, b.big, b.n) == (2**63 - 1, 2**31 - 1, -(2**63), -(2**31))


def test_orm_integer_field_overflow_raises(vtable):
    # v1.4.0 deliberate break: IntegerField silently wrapped values outside
    # int4 range in 1.3.0.
    for v in (2**31, 2**32 + 5):
        with pytest.raises(cello.ParameterRangeError):
            run(ValModel.objects.create(label="x", n=v))
    assert run(ValModel.objects.count()) == 0


def test_orm_bigint_beyond_i64_raises(vtable):
    # v1.4.0 deliberate break: see test_param_int_beyond_i64_raises.
    with pytest.raises(cello.ParameterRangeError):
        run(ValModel.objects.create(label="x", big=2**63))
    assert run(ValModel.objects.count()) == 0


@pytest.mark.parametrize(
    "kwargs,exc",
    [
        ({"note": 5}, RuntimeError),    # int into TEXT
        ({"n": 1.5}, RuntimeError),     # float into INTEGER
        ({"flag": 1}, RuntimeError),    # int into BOOLEAN
        ({"doc": "hello"}, RuntimeError),  # str into JSONB
        ({"doc": 5}, RuntimeError),     # int into JSONB
        ({"doc": True}, RuntimeError),  # bool into JSONB
    ],
    ids=["text-int", "int-float", "bool-int", "json-str", "json-int", "json-bool"],
)
def test_orm_write_type_mismatches_raise(vtable, kwargs, exc):
    # PINNED-QUIRK: no field-level coercion: a JSONField only accepts
    # dict/list (scalar JSON values fail). (DateTimeField writes were removed
    # from this list: they work since v1.4.0, see below.)
    with pytest.raises(exc):
        run(ValModel.objects.create(label="x", **kwargs))
    assert run(ValModel.objects.count()) == 0


@pytest.mark.parametrize(
    "value,expected",
    [
        ("2024-01-01T00:00:00Z", "2024-01-01T00:00:00+00:00"),
        ("2024-01-01 05:30:00+05:30", "2024-01-01T00:00:00+00:00"),
        (datetime.datetime(2024, 1, 1), "2024-01-01T00:00:00+00:00"),  # naive = UTC
        (
            datetime.datetime(2024, 1, 1, 2, tzinfo=datetime.timezone(datetime.timedelta(hours=2))),
            "2024-01-01T00:00:00+00:00",
        ),
    ],
    ids=["iso-z", "iso-offset", "naive-datetime", "aware-datetime"],
)
def test_orm_datetime_field_writes(vtable, value, expected):
    # v1.4.0 deliberate break: 1.3.0 rejected both ISO strings (RuntimeError)
    # and datetime objects (TypeError) for a DateTimeField.
    obj = run(ValModel.objects.create(label="x", at=value))
    assert obj.at == expected
    assert run(ValModel.objects.get(id=obj.id)).at == expected


def test_orm_json_list_and_nested(vtable):
    obj = run(ValModel.objects.create(label="x", doc=[1, {"a": None}, [], "s"]))
    assert obj.doc == [1, {"a": None}, [], "s"]
    assert run(ValModel.objects.get(id=obj.id)).doc == [1, {"a": None}, [], "s"]


def test_orm_unicode_and_empty_strings(vtable):
    samples = ["", " ", "日本語", "🎻🎻", "é", "ß", "it's \"quoted\"", "back\\slash", "%_"]
    async def main():
        ids = [(await ValModel.objects.create(label=s, note=s)).id for s in samples]
        return [(await ValModel.objects.get(id=i)) for i in ids]
    got = run(main())
    assert [g.label for g in got] == samples
    assert [g.note for g in got] == samples


def test_orm_varchar_length_counts_characters(vtable):
    # 50 multi-byte characters fit a CharField(max_length=50); 51 do not.
    obj = run(ValModel.objects.create(label="🎻" * 50))
    assert obj.label == "🎻" * 50
    with pytest.raises(RuntimeError):
        run(ValModel.objects.create(label="🎻" * 51))


def test_orm_nul_character_rejected(vtable):
    with pytest.raises(RuntimeError):
        run(ValModel.objects.create(label="a\x00b"))
