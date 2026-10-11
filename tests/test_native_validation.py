"""v1.4.0 Phase 03 — native ``body=Model`` validation, visibility, OpenAPI.

``@app.post(..., body=SomeOrmModel)`` validates the raw request body in Rust
against the model's compiled schema. These tests pin the contract over live
HTTP (like ``tests/test_upgrades.py``):

* 400 ``{"detail": [...]}`` with ``loc``/``msg``/``type`` items, every error
  collected, input values never echoed;
* lax coercion consistent with the Pydantic ``body=`` path;
* unknown keys and read-only fields (``AutoField``, ``read_only=True``) ignored;
* ``write_only=True`` fields accepted on input, never serialized;
* OpenAPI ``requestBody`` schemas generated from the same compiled spec;
* no 500 / no panic on malformed input (fuzzed).

Pydantic / dataclass / plain-class ``body=`` and 422 type-hint validation are
covered unchanged by ``tests/test_upgrades.py`` and ``tests/test_cello.py``; a
few cross-checks live here too.
"""

import dataclasses
import json
import os
import random
import threading
import time

import pytest

requests = pytest.importorskip("requests")

from cello import App, LimitsConfig, Response
from cello.validation import _native_validator
from cello.orm import (
    AutoField,
    BigIntegerField,
    BooleanField,
    CharField,
    DateTimeField,
    FloatField,
    ForeignKey,
    IntegerField,
    JSONField,
    Model,
    TextField,
)

_PORT = [22300]


def _serve(app):
    _PORT[0] += 1
    p = _PORT[0]
    threading.Thread(target=lambda: app.run(host="127.0.0.1", port=p), daemon=True).start()
    base = f"http://127.0.0.1:{p}"
    deadline = time.time() + 10
    while time.time() < deadline:
        try:
            requests.get(base + "/__ping__", timeout=0.5)
            return base
        except Exception:
            time.sleep(0.1)
    raise RuntimeError(f"server on :{p} never came up")


def _app():
    app = App()

    @app.get("/__ping__")
    def ping(request):
        return {"ok": True}

    return app


# ── Models ────────────────────────────────────────────────────────────────────


class NvAuthor(Model):
    id = AutoField()
    name = CharField(max_length=50)


class NvAccount(Model):
    id = AutoField()
    email = CharField(max_length=20)
    password = CharField(max_length=128, write_only=True)
    active = BooleanField(default=True)
    age = IntegerField()
    balance = FloatField(null=True)
    visits = BigIntegerField(null=True)
    joined = DateTimeField(null=True)
    prefs = JSONField(null=True)
    bio = TextField(null=True)
    author = ForeignKey(NvAuthor, null=True)
    slug = CharField(max_length=40, null=True, read_only=True)


GOOD = {"email": "ada@example.com", "password": "s3cret!", "age": 36}

ERROR_KEYS = {"loc", "msg", "type"}


def _validator(model=NvAccount):
    return _native_validator(model)


def _check(body, model=NvAccount):
    """Validate raw bytes directly (no HTTP). Returns instance or (status, json)."""
    out = _validator(model).validate_bytes(body if isinstance(body, bytes) else json.dumps(body).encode())
    if isinstance(out, Response):
        return out.status, json.loads(bytes(out.body()))
    return out


@pytest.fixture(scope="module")
def url():
    app = _app()

    @app.post("/accounts", body=NvAccount)
    async def create(request, account: NvAccount):
        return {
            "cls": type(account).__name__,
            "dict": account.to_dict(),
            "password": account.password,
            "author_attr": account.author,
        }

    @app.post("/accounts/sync", body=NvAccount)
    def create_sync(request, account):
        return {"cls": type(account).__name__, "dict": account.to_dict()}

    @app.post("/accounts/echo", body=NvAccount)
    def echo(request, account):
        return account  # serialized from the registry: no password

    @app.post("/accounts/list", body=NvAccount)
    def echo_list(request, account):
        return {"items": [account, account]}

    @app.post("/accounts/response", body=NvAccount)
    def echo_response(request, account):
        return Response.json(account, status=201)

    return _serve(app)


# ── Happy path ────────────────────────────────────────────────────────────────


def test_valid_body_builds_unsaved_instance(url):
    r = requests.post(f"{url}/accounts", json=GOOD)
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["cls"] == "NvAccount"
    assert body["password"] == "s3cret!"  # write-only: accepted on input
    d = body["dict"]
    assert "password" not in d  # ... never serialized
    assert d["id"] is None  # unsaved
    assert d["email"] == "ada@example.com" and d["age"] == 36
    # Absent fields are filled exactly as Model(**data) fills them (defaults).
    assert d["active"] is True and d["balance"] is None and d["author_id"] is None
    assert d == NvAccount(**GOOD).to_dict()


def test_sync_handler_and_param_injection(url):
    r = requests.post(f"{url}/accounts/sync", json=GOOD)
    assert r.status_code == 200 and r.json()["cls"] == "NvAccount"


def test_lax_coercion_matches_pydantic_rules(url):
    body = dict(
        GOOD,
        age="42",
        active="yes",
        balance="1.5",
        visits=9007199254740993,
        joined="2024-01-02 08:34:05+05:30",
        prefs={"theme": "dark", "n": [1, 2, {"deep": None}]},
        author_id=7,
    )
    d = requests.post(f"{url}/accounts", json=body).json()["dict"]
    assert d["age"] == 42
    assert d["active"] is True
    assert d["balance"] == 1.5
    assert d["visits"] == 9007199254740993  # exact, no float round-trip
    # Normalized to the RFC 3339 text a TIMESTAMPTZ row decodes to.
    assert d["joined"] == "2024-01-02T03:04:05+00:00"
    assert d["prefs"] == {"theme": "dark", "n": [1, 2, {"deep": None}]}
    assert d["author_id"] == 7


@pytest.mark.parametrize(
    "field,value,expected",
    [
        ("age", 42.0, 42),
        ("age", True, 1),
        ("age", " 7 ", 7),
        ("age", "1_000", 1000),
        ("age", "4.0", 4),
        ("active", 0, False),
        ("active", "OFF", False),
        ("active", "t", True),
        ("balance", 3, 3.0),
        ("balance", "inf", float("inf")),
        ("joined", "2024-01-02", "2024-01-02T00:00:00+00:00"),
        ("joined", "2024-01-02T03:04:05Z", "2024-01-02T03:04:05+00:00"),
        ("joined", "2024-01-02T03:04:05.250", "2024-01-02T03:04:05.250+00:00"),
        ("joined", 1704164645, "2024-01-02T03:04:05+00:00"),
        ("joined", 1704164645000, "2024-01-02T03:04:05+00:00"),
        ("balance", None, None),
        ("bio", "😀" * 3, "😀" * 3),
    ],
)
def test_coercion_table(field, value, expected):
    inst = _check(dict(GOOD, **{field: value}))
    assert isinstance(inst, NvAccount), inst
    assert getattr(inst, field) == expected


def test_foreign_key_by_name_or_column():
    a = _check(dict(GOOD, author=3))
    b = _check(dict(GOOD, author_id="3"))
    for inst in (a, b):
        assert inst.author == 3 and inst.author_id == 3
        assert inst.to_dict()["author_id"] == 3


def test_unknown_keys_and_read_only_fields_are_ignored():
    inst = _check(dict(GOOD, id=999, slug="hijack", is_admin=True, extra={"a": [1, 2]}))
    assert isinstance(inst, NvAccount)
    assert inst.id is None  # mass assignment of the primary key is impossible
    assert inst.slug is None
    assert not hasattr(inst, "is_admin")


def test_duplicate_keys_last_wins():
    inst = _check(b'{"email":"a@b.c","password":"p","age":1,"age":2}')
    assert inst.age == 2


def test_large_body_releases_gil_path(url):
    # > 32 KiB: parsed with the GIL released; same result.
    body = dict(GOOD, bio="x" * 100_000, prefs={"k": ["v"] * 5000})
    d = requests.post(f"{url}/accounts", json=body).json()["dict"]
    assert len(d["bio"]) == 100_000 and len(d["prefs"]["k"]) == 5000


# ── Errors ────────────────────────────────────────────────────────────────────


def test_all_errors_collected_with_frozen_shape(url):
    bad = {
        "email": "x" * 21,
        "password": 12345,
        "age": 1.5,
        "active": "maybe",
        "balance": "lots",
        "visits": "9" * 30,
        "joined": "yesterday",
        "bio": ["not", "a", "string"],
    }
    r = requests.post(f"{url}/accounts", json=bad)
    assert r.status_code == 400
    detail = r.json()["detail"]
    assert all(set(item) == ERROR_KEYS for item in detail)
    got = [(item["loc"], item["type"]) for item in detail]
    assert got == [
        (["email"], "string_too_long"),
        (["password"], "string_type"),
        (["active"], "bool_parsing"),
        (["age"], "int_from_float"),
        (["balance"], "float_parsing"),
        (["visits"], "less_than_equal"),
        (["joined"], "datetime_parsing"),
        (["bio"], "string_type"),
    ]
    by_loc = {item["loc"][0]: item["msg"] for item in detail}
    assert by_loc["email"] == "String should have at most 20 characters"
    assert by_loc["visits"] == "Input should be less than or equal to 9223372036854775807"
    # Input values are never echoed back.
    assert "12345" not in r.text and "maybe" not in r.text and "x" * 21 not in r.text


def test_missing_required_fields(url):
    r = requests.post(f"{url}/accounts", json={})
    assert r.status_code == 400
    assert r.json() == {
        "detail": [
            {"loc": ["email"], "msg": "Field required", "type": "missing"},
            {"loc": ["password"], "msg": "Field required", "type": "missing"},
            {"loc": ["age"], "msg": "Field required", "type": "missing"},
        ]
    }


@pytest.mark.parametrize(
    "field,value,etype",
    [
        ("email", None, "string_type"),
        ("email", 5, "string_type"),
        ("age", None, "int_type"),
        ("age", [], "int_type"),
        ("age", {"a": 1}, "int_type"),
        ("age", "1e3", "int_parsing"),
        ("age", 2**31, "less_than_equal"),
        ("age", -(2**31) - 1, "greater_than_equal"),
        ("active", None, "bool_type"),
        ("active", 2, "bool_parsing"),
        ("active", " true", "bool_parsing"),
        ("balance", [1], "float_type"),
        ("joined", True, "datetime_type"),
        ("joined", "2024-13-45", "datetime_parsing"),
        ("author_id", "x", "int_parsing"),
    ],
)
def test_error_table(field, value, etype):
    status, body = _check(dict(GOOD, **{field: value}))
    assert status == 400
    assert body["detail"] == [
        {"loc": [field], "msg": body["detail"][0]["msg"], "type": etype}
    ]


def test_absent_fields_get_init_defaults():
    calls = []

    def make_tags():
        calls.append(1)
        return ["new"]

    class NvDefaults(Model):
        id = AutoField()
        name = CharField(max_length=10)
        flag = BooleanField(default=True)
        tags = JSONField(default=make_tags)
        meta = JSONField(default={"a": [1]})

    a = _check({"name": "x"}, model=NvDefaults)
    b = _check({"name": "y"}, model=NvDefaults)
    assert (a.flag, a.tags, a.meta) == (True, ["new"], {"a": [1]})
    assert a.tags is not b.tags and a.meta is not b.meta  # fresh per instance
    assert len(calls) == 2
    assert a.to_dict() == NvDefaults(name="x").to_dict()


def test_null_bytes_are_rejected(url):
    """Postgres refuses U+0000 in text and jsonb: a 400, not a later DB error."""
    r = requests.post(
        f"{url}/accounts",
        json=dict(GOOD, email="a\x00b", bio="ok", prefs={"k": ["x\x00"]}),
    )
    assert r.status_code == 400
    item = {"msg": "String should not contain null characters (U+0000)", "type": "string_null_byte"}
    assert r.json()["detail"] == [dict(loc=["email"], **item), dict(loc=["prefs"], **item)]
    status, body = _check(dict(GOOD, prefs={"a\x00": 1}))
    assert status == 400 and body["detail"][0]["type"] == "string_null_byte"


def test_invalid_json_item_is_the_existing_one(url):
    expected = {"detail": [{"loc": ["body"], "msg": "Invalid JSON body", "type": "value_error.json"}]}
    for raw in ["{bad json", "", "{\"email\": \"a\"} trailing", "\xff"]:
        r = requests.post(
            f"{url}/accounts", data=raw.encode("latin-1"), headers={"Content-Type": "application/json"}
        )
        assert r.status_code == 400 and r.json() == expected, raw


def test_non_object_body(url):
    for raw in ["[1, 2]", "42", "\"s\"", "null", "true"]:
        r = requests.post(f"{url}/accounts", data=raw, headers={"Content-Type": "application/json"})
        assert r.status_code == 400
        assert r.json() == {
            "detail": [
                {
                    "loc": ["body"],
                    "msg": "Input should be a valid dictionary or instance of NvAccount",
                    "type": "model_type",
                }
            ]
        }


def test_excessive_nesting_is_rejected_not_crashed(url):
    deep = '{"email":"a","password":"p","age":1,"prefs":' + "[" * 100_000 + "]" * 100_000 + "}"
    r = requests.post(f"{url}/accounts", data=deep, headers={"Content-Type": "application/json"})
    assert r.status_code == 400
    assert r.json()["detail"][0]["type"] == "value_error.json"
    # Nesting inside an *ignored* key is bounded the same way.
    deep2 = '{"zzz":' + "[" * 100_000 + "]" * 100_000 + "}"
    r = requests.post(f"{url}/accounts", data=deep2, headers={"Content-Type": "application/json"})
    assert r.status_code == 400


def test_max_body_size_is_enforced_before_validation():
    app = _app()
    app.set_limits(LimitsConfig(max_body_size=1024))

    @app.post("/a", body=NvAccount)
    def create(request, account):
        return {"ok": True}

    base = _serve(app)
    big = dict(GOOD, bio="x" * 4096)
    assert requests.post(f"{base}/a", json=big).status_code == 413
    assert requests.post(f"{base}/a", json=GOOD).status_code == 200


def _malformed_corpus(n=400, seed=1234):
    rnd = random.Random(seed)
    good = json.dumps(dict(GOOD, prefs={"a": [1, 2]}, joined="2024-01-02")).encode()
    fixed = [
        b"", b"{", b"}", b"[", b"{{}}", b'{"a"}', b'{"a":}', b'{"age":01}', b'{"age":-}',
        b'{"age":1e999}', b'{"age":NaN}', b'{"age":Infinity}', b'{"email":"\\ud800"}',
        b'{"email":"\\u0000"}', b'{"email":"\xff\xfe"}', b"\x00\x00\x00", b'{"age":' + b"9" * 5000 + b"}",
        b'{"joined":"' + b"9" * 5000 + b'"}', b'{"joined":1e308}', b'{"joined":-1e308}',
        b'{"' + b"k" * 100_000 + b'":1}', b"\xef\xbb\xbf{}", b'{"email":"a",}', b"[]" * 1000,
    ]
    out = list(fixed)
    while len(out) < n:
        b = bytearray(good)
        for _ in range(rnd.randint(1, 6)):
            op = rnd.random()
            i = rnd.randrange(len(b) + 1)
            if op < 0.4 and b:
                del b[min(i, len(b) - 1)]
            elif op < 0.8:
                b.insert(i, rnd.choice(b'{}[]",:\\0123456789eE.-+ntfaulrsx\x00\xff'))
            else:
                b = b[:i]
        out.append(bytes(b))
    return out


def test_fuzz_malformed_bodies_never_crash():
    v = _validator()
    for raw in _malformed_corpus():
        out = v.validate_bytes(raw)
        if isinstance(out, Response):
            assert out.status == 400
            detail = json.loads(bytes(out.body()))["detail"]
            assert detail and all(set(item) == ERROR_KEYS for item in detail)
        else:
            assert isinstance(out, NvAccount)


def test_fuzz_over_http_never_500(url):
    for raw in _malformed_corpus(n=80, seed=99):
        r = requests.post(f"{url}/accounts", data=raw, headers={"Content-Type": "application/json"})
        assert r.status_code in (200, 400), (raw[:80], r.status_code, r.text[:200])


# ── Visibility: write_only / read_only ────────────────────────────────────────


def test_returning_a_model_serializes_without_write_only(url):
    r = requests.post(f"{url}/accounts/echo", json=dict(GOOD, author_id=2))
    assert r.status_code == 200
    body = r.json()
    assert "password" not in body and "s3cret" not in r.text
    assert body["email"] == GOOD["email"] and body["author_id"] == 2
    # Same keys as to_dict(), in declaration order.
    assert list(body) == list(NvAccount(**GOOD).to_dict())

    r = requests.post(f"{url}/accounts/list", json=GOOD)
    assert [("password" in item) for item in r.json()["items"]] == [False, False]

    r = requests.post(f"{url}/accounts/response", json=GOOD)
    assert r.status_code == 201 and "password" not in r.json()


def test_to_dict_and_values_shape_omit_write_only():
    acc = NvAccount(email="e", password="pw", age=1)
    assert "password" not in acc.to_dict()
    assert acc.password == "pw"  # still a normal attribute


def test_spec_visibility_flags():
    schema = NvAccount._meta.schema
    pw, slug, ident = schema.field("password"), schema.field("slug"), schema.field("id")
    assert pw.write_only and not pw.readable and pw.writable
    assert not slug.writable and slug.readable and not slug.write_only
    assert ident.auto and ident.writable  # auto fields are read-only by `auto`


def test_write_only_and_read_only_together_is_an_error():
    with pytest.raises(ValueError):
        CharField(write_only=True, read_only=True)


def test_custom_init_still_runs():
    calls = []

    class NvCustom(Model):
        id = AutoField()
        name = CharField(max_length=10)

        def __init__(self, **kw):
            calls.append(kw)
            super().__init__(**kw)
            self.name = self.name.upper()

    inst = _check({"name": "ada", "id": 5}, model=NvCustom)
    assert inst.name == "ADA" and inst.id is None
    assert calls == [{"name": "ada"}]


# ── Other body= kinds are unchanged ───────────────────────────────────────────


def test_pydantic_and_dataclass_body_paths_unchanged():
    pydantic = pytest.importorskip("pydantic")

    class PUser(pydantic.BaseModel):
        name: str
        age: int

    @dataclasses.dataclass
    class DPoint:
        x: int
        y: int

    app = _app()

    @app.post("/p", body=PUser)
    def p(request, user):
        return {"age": user.age}

    @app.post("/d", body=DPoint)
    def d(request, pt):
        return {"x": pt.x}

    @app.post("/hint")
    def hint(request, user: PUser):
        return {"age": user.age}

    base = _serve(app)
    assert requests.post(f"{base}/p", json={"name": "a", "age": "3"}).json() == {"age": 3}
    bad = requests.post(f"{base}/p", json={"name": "a", "age": "x"})
    # Pydantic's own items (with `input`), as in 1.3.
    assert bad.status_code == 400 and bad.json()["detail"][0]["input"] == "x"
    assert requests.post(f"{base}/d", json={"x": 1, "y": 2}).json() == {"x": 1}
    assert requests.post(f"{base}/d", json={"x": 1}).status_code == 400
    # Type-hint validation keeps its 422.
    assert requests.post(f"{base}/hint", json={"name": "a", "age": "x"}).status_code == 422


# ── OpenAPI ───────────────────────────────────────────────────────────────────


def test_openapi_request_body_from_registry():
    pydantic = pytest.importorskip("pydantic")

    class PItem(pydantic.BaseModel):
        title: str

    class Plain:
        def __init__(self, a):
            self.a = a

    app = _app()
    app.enable_openapi()

    @app.post("/accounts", body=NvAccount)
    def a1(request, acc):
        return {}

    @app.put("/accounts/{id}", body=NvAccount)
    def a2(request, acc):
        return {}

    @app.post("/items", body=PItem)
    def i1(request, item):
        return {}

    @app.post("/plain", body=Plain)
    def p1(request, x):
        return {}

    @app.post("/nobody")
    def n1(request):
        return {}

    base = _serve(app)
    spec = requests.get(f"{base}/openapi.json").json()
    schemas = spec["components"]["schemas"]

    acc = schemas["NvAccount"]
    assert acc["required"] == ["email", "password", "age"]
    assert list(acc["properties"]) == [
        "id", "email", "password", "active", "age", "balance", "visits",
        "joined", "prefs", "bio", "author_id", "slug",
    ]
    p = acc["properties"]
    assert p["id"] == {"type": "integer", "format": "int32", "readOnly": True}
    assert p["email"] == {"type": "string", "maxLength": 20}
    assert p["password"]["writeOnly"] is True
    assert p["active"] == {"type": "boolean", "default": True}
    assert p["visits"] == {"type": "integer", "format": "int64", "nullable": True}
    assert p["joined"] == {"type": "string", "format": "date-time", "nullable": True}
    assert p["slug"]["readOnly"] is True
    assert p["balance"]["format"] == "double"

    ref = {"$ref": "#/components/schemas/NvAccount"}
    post = spec["paths"]["/accounts"]["post"]
    put = spec["paths"]["/accounts/{id}"]["put"]
    for op in (post, put):  # one component shared by both routes
        assert op["requestBody"]["content"]["application/json"]["schema"] == ref
        assert op["requestBody"]["required"] is True
        assert "400" in op["responses"]

    items = spec["paths"]["/items"]["post"]["requestBody"]["content"]["application/json"]["schema"]
    assert items == {"$ref": "#/components/schemas/PItem"}
    assert schemas["PItem"]["required"] == ["title"]

    # Classes that cannot describe themselves keep the 1.3 generic schema.
    for path in ("/plain", "/nobody"):
        rb = spec["paths"][path]["post"]["requestBody"]
        assert rb["content"]["application/json"]["schema"] == {"type": "object"}


def test_openapi_same_class_name_different_models():
    def make():
        class Thing(Model):
            id = AutoField()
            label = CharField(max_length=5)

        return Thing

    A, B = make(), make()

    app = _app()
    app.enable_openapi()

    @app.post("/a", body=A)
    def a(request, x):
        return {}

    @app.post("/b", body=B)
    def b(request, x):
        return {}

    base = _serve(app)
    spec = requests.get(f"{base}/openapi.json").json()
    refs = {
        spec["paths"][p]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"]
        for p in ("/a", "/b")
    }
    assert refs == {"#/components/schemas/Thing", "#/components/schemas/Thing2"}


# ── Live Postgres: a validated instance saves ────────────────────────────────

DSN = os.environ.get(
    "CELLO_TEST_DATABASE_URL", "postgresql://postgres:postgres@127.0.0.1:5432/cello_test"
)


def _pg_ok():
    import asyncio

    from cello import DatabaseConfig

    async def check():
        a = App()
        a.enable_database(DatabaseConfig(url=DSN, pool_size=1))
        await a.database.ping()
        await a.database.close()

    try:
        asyncio.new_event_loop().run_until_complete(check())
        return True
    except Exception:
        return False


@pytest.mark.skipif(not _pg_ok(), reason="PostgreSQL not reachable")
def test_validated_instance_saves_and_round_trips():
    from cello import DatabaseConfig

    table = f"nv_member_{os.getpid()}"

    class NvMember(Model):
        id = AutoField()
        email = CharField(max_length=40)
        password = CharField(max_length=128, write_only=True)
        age = IntegerField()
        active = BooleanField(default=True)
        prefs = JSONField(null=True)

        class Meta:
            table_name = table

    app = _app()
    app.enable_database(DatabaseConfig(url=DSN, pool_size=2))

    @app.on_event("startup")
    async def startup():
        await NvMember.drop_table(using=app.database)
        await NvMember.create_table(using=app.database)

    @app.post("/members", body=NvMember)
    async def create(request, member):
        await member.save(using=request.database)
        return member

    @app.post("/cleanup")
    async def cleanup(request):
        await NvMember.drop_table(using=request.database)
        return {"ok": True}

    base = _serve(app)
    r = requests.post(
        f"{base}/members",
        json={"email": "m@x.io", "password": "pw", "age": "30", "id": 777, "prefs": {"a": [1]}},
    )
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["id"] != 777 and isinstance(body["id"], int)  # DB-assigned
    assert body["age"] == 30 and body["active"] is True and body["prefs"] == {"a": [1]}
    assert "password" not in body
    assert requests.post(f"{base}/cleanup").status_code == 200
