"""
Rust schema registry (v1.4.0 Phase 01, ``src/schema``).

Every ORM model is compiled once, at class definition, into a Rust
``ModelSpec``. These tests pin what the registry holds, how it resolves query
identifiers, and that the DDL it renders is byte-for-byte what the 1.3.x Python
implementation rendered (the reference implementation is copied below). They
need no database.
"""

import gc
import json
import re

import pytest

from cello import _cello
from cello import orm
from cello.orm import (
    AutoField,
    BigIntegerField,
    BooleanField,
    CharField,
    DateTimeField,
    FieldError,
    FloatField,
    ForeignKey,
    IntegerField,
    JSONField,
    Model,
    TextField,
)


# ── Reference: the 1.3.x / Phase 00 Python DDL renderer ──────────────────────

def _ref_quote_ident(name):
    return '"' + str(name).replace('"', '""') + '"'


def _ref_sql_literal(value):
    if value is orm._NOW:
        return "now()"
    if isinstance(value, bool):
        return "TRUE" if value else "FALSE"
    if isinstance(value, (int, float)):
        return str(value)
    escaped = str(value).replace("'", "''")
    return f"'{escaped}'"


def _ref_column_ddl(f):
    parts = [_ref_quote_ident(f.column), f.ddl_type()]
    if f.primary_key:
        parts.append("PRIMARY KEY")
    if f.unique and not f.primary_key:
        parts.append("UNIQUE")
    if not f.null and not f.primary_key:
        parts.append("NOT NULL")
    if f.default is not None and not callable(f.default):
        default = f.default
        # 1.4.0 (deliberate break): a JSONField dict/list default renders as
        # JSON text, not Python repr.
        if isinstance(f, JSONField) and isinstance(default, (dict, list)):
            try:
                default = json.dumps(default)
            except ValueError:
                pass
        parts.append(f"DEFAULT {_ref_sql_literal(default)}")
    base = " ".join(parts)
    if isinstance(f, ForeignKey):
        target = f.to._meta.table_name
        target_pk = f.to._meta.pk_column
        base = (
            f"{base} REFERENCES {_ref_quote_ident(target)}({_ref_quote_ident(target_pk)}) "
            f"ON DELETE {f.on_delete}"
        )
    return base


def _ref_create_table(cls, if_not_exists=True):
    cols = [_ref_column_ddl(f) for f in cls._meta.fields.values()]
    exists = "IF NOT EXISTS " if if_not_exists else ""
    return f'CREATE TABLE {exists}{_ref_quote_ident(cls._meta.table_name)} ({", ".join(cols)})'


def _ref_drop_table(cls, if_exists=True):
    exists = "IF EXISTS " if if_exists else ""
    return f"DROP TABLE {exists}{_ref_quote_ident(cls._meta.table_name)}"


# ── Models ────────────────────────────────────────────────────────────────────

class SrAuthor(Model):
    id = AutoField()
    name = CharField(max_length=100)
    email = CharField(max_length=255, null=True, unique=True)
    active = BooleanField(default=True)
    rating = FloatField(default=1.5)
    nick = CharField(max_length=20, default="it's")
    bio = TextField(null=True, db_column="biography")

    class Meta:
        table_name = "sr_author"


class SrBook(Model):
    id = AutoField()
    title = CharField(max_length=200)
    author = ForeignKey(SrAuthor)
    editor = ForeignKey(SrAuthor, null=True, on_delete="set   null", db_column="editor_ref")
    pages = IntegerField(null=True)

    class Meta:
        table_name = "sr_book"


class SrDefaults(Model):
    id = AutoField()
    created = DateTimeField(auto_now_add=True)
    stamp = DateTimeField(auto_now_add=True, null=True)
    lazy = IntegerField(null=True, default=lambda: 7)
    maybe = IntegerField(null=True, default=5)
    big = BigIntegerField(default=10 ** 20)
    neg = FloatField(default=-2.5e-10)
    payload = JSONField(default={})
    items = JSONField(default=[])
    bad = JSONField(default={"a": 1})     # 1.3.x rendered Python repr; 1.4.0 renders JSON
    flag = BooleanField(default=False)

    class Meta:
        table_name = "sr_defaults"


class SrBase(Model):
    id = AutoField()
    name = CharField(max_length=50)

    class Meta:
        table_name = "sr_base"


class SrChild(SrBase):
    extra = IntegerField(null=True)


class SrGreeter:
    def hello(self):
        return "hi"


class SrMixed(SrGreeter, Model):
    id = AutoField()
    label = CharField(max_length=10)


class SrNoPk(Model):
    label = CharField(max_length=10)


class SrCode(Model):
    code = CharField(max_length=10, primary_key=True)


class SrWeird(Model):
    id = AutoField()
    spaced = TextField(null=True, db_column="my col")
    quoted = TextField(null=True, db_column='we"ird')
    raw = orm.Field(null=True)

    class Meta:
        table_name = 'sr "weird" table'


class _Numeric(orm.Field):
    sql_type = "NUMERIC(10,2)"


class SrCustom(Model):
    id = AutoField()
    amount = _Numeric(default=0)
    title = CharField()          # default max_length 255


ALL = [SrAuthor, SrBook, SrDefaults, SrBase, SrChild, SrMixed, SrNoPk, SrCode,
       SrWeird, SrCustom]


class RecordingDB:
    def __init__(self):
        self.calls = []

    async def fetch(self, sql, *params):
        self.calls.append(sql)
        return []

    async def fetchrow(self, sql, *params):
        self.calls.append(sql)
        return {"n": 0}

    async def execute(self, sql, *params):
        self.calls.append(sql)
        return 0


def _run(coro):
    import asyncio

    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


# ── Registry contents ─────────────────────────────────────────────────────────

def test_every_model_has_a_registered_spec():
    ids = set()
    for cls in ALL:
        spec = cls._meta.schema
        assert isinstance(spec, _cello.ModelSpec)
        assert spec.owner
        assert spec.name == cls.__name__
        assert spec.table == cls._meta.table_name
        assert len(spec) == len(cls._meta.fields)
        got = _cello.schema_get(spec.id)
        assert got is not None and not got.owner and got.table == spec.table
        ids.add(spec.id)
    assert len(ids) == len(ALL)


def test_field_order_matches_meta_fields_including_inheritance():
    for cls in ALL:
        assert [f.name for f in cls._meta.schema.fields] == list(cls._meta.fields)
    # Subclass: own fields first, then inherited (pinned 1.3.x order).
    assert [f.name for f in SrChild._meta.schema.fields] == ["extra", "id", "name"]
    assert SrChild._meta.schema.table == "srchild"
    assert SrChild._meta.schema.pk_name == "id"
    assert [f.name for f in SrMixed._meta.schema.fields] == ["id", "label"]
    assert SrMixed._meta.schema.table == "srmixed"


def test_author_field_specs():
    s = SrAuthor._meta.schema
    assert s.pk_name == "id" and s.pk_column == "id" and s.quoted_pk == '"id"'
    assert s.quoted_table == '"sr_author"'
    f = {x.name: x for x in s.fields}
    assert (f["id"].pg_type, f["id"].kind, f["id"].auto, f["id"].primary_key) == (
        "SERIAL", "serial", True, True)
    assert (f["name"].pg_type, f["name"].max_length, f["name"].null) == ("VARCHAR(100)", 100, False)
    assert (f["email"].null, f["email"].unique) == (True, True)
    assert (f["active"].default_kind, f["active"].default_sql) == ("bool", "TRUE")
    assert (f["rating"].default_kind, f["rating"].default_sql) == ("float", "1.5")
    assert (f["nick"].default_kind, f["nick"].default_sql) == ("text", "'it''s'")
    assert (f["bio"].column, f["bio"].quoted_column) == ("biography", '"biography"')
    for x in s.fields:
        assert x.readable and x.writable and not x.write_only
        assert x.references is None
    assert s.field("nope") is None and s.field("bio").column == "biography"


def test_foreign_key_specs():
    s = SrBook._meta.schema
    author, editor = s.field("author"), s.field("editor")
    assert (author.column, author.pg_type) == ("author_id", "INTEGER")
    assert author.references == ("sr_author", "id", "CASCADE", SrAuthor._meta.schema.id)
    assert (editor.column, editor.null) == ("editor_ref", True)
    assert editor.references == ("sr_author", "id", "SET NULL", SrAuthor._meta.schema.id)


def test_default_kinds():
    f = {x.name: x for x in SrDefaults._meta.schema.fields}
    assert (f["created"].default_kind, f["created"].default_sql, f["created"].auto_now_add) == (
        "now", "now()", True)
    assert (f["stamp"].default_kind, f["stamp"].null) == ("now", True)
    assert (f["lazy"].default_kind, f["lazy"].default_sql) == ("callable", None)
    assert (f["maybe"].default_kind, f["maybe"].default_sql) == ("int", "5")
    assert f["big"].default_sql == str(10 ** 20)
    assert f["neg"].default_sql == str(-2.5e-10)
    assert (f["payload"].default_kind, f["payload"].default_sql) == ("other", "'{}'")
    assert f["items"].default_sql == "'[]'"
    assert f["bad"].default_sql == "'{\"a\": 1}'"
    assert f["bad"].default_kind == "other"
    assert f["flag"].default_sql == "FALSE"
    assert f["id"].default_kind == "none"


def test_no_pk_and_custom_type_specs():
    s = SrNoPk._meta.schema
    assert s.pk_name is None and s.pk_column is None and s.quoted_pk is None
    c = {x.name: x for x in SrCustom._meta.schema.fields}
    assert (c["amount"].pg_type, c["amount"].kind, c["amount"].default_sql) == (
        "NUMERIC(10,2)", "custom", "0")
    assert (c["title"].pg_type, c["title"].max_length) == ("VARCHAR(255)", 255)
    assert SrCode._meta.schema.pk_name == "code"
    assert SrCode._meta.schema.field("code").kind == "varchar"


def test_meta_caches_come_from_the_registry():
    m = SrBook._meta
    assert m.quoted_table == '"sr_book"'
    assert m.quoted_pk == '"id"'
    assert m.quoted_columns == {
        "id": '"id"', "title": '"title"', "author": '"author_id"',
        "editor": '"editor_ref"', "pages": '"pages"',
    }
    assert list(m.quoted_columns) == list(m.fields)
    assert m.lookup == {
        "id": '"id"', "title": '"title"', "author": '"author_id"', "author_id": '"author_id"',
        "editor": '"editor_ref"', "editor_ref": '"editor_ref"', "pages": '"pages"',
    }


# ── Identifier resolution ─────────────────────────────────────────────────────

BAD = ['x" = 1 OR "1', 'name"', "name; DROP TABLE t", "1name", "", "na me", "name\n",
       "näme", "\ud800", "my col", 'we"ird']


@pytest.mark.parametrize("bad", BAD)
def test_resolve_rejects_non_identifiers(bad):
    assert SrWeird._meta.schema.resolve(bad) is None
    assert bad not in SrWeird._meta.lookup
    with pytest.raises(FieldError) as exc:
        SrWeird.objects.filter(**{bad: 1})
    assert repr(bad) in str(exc.value)


def test_resolve_non_strings():
    s = SrAuthor._meta.schema
    for bad in (1, None, b"name", ["name"]):
        assert s.resolve(bad) is None
    with pytest.raises(FieldError):
        SrAuthor.objects.all()._resolve_column(["name"])
    with pytest.raises(FieldError):
        SrAuthor.objects.all()._resolve_column(b"name")


def test_resolve_declared_and_undeclared():
    s = SrBook._meta.schema
    assert s.resolve("author") == ('"author_id"', True)
    assert s.resolve("author_id") == ('"author_id"', True)
    assert s.resolve("editor_ref") == ('"editor_ref"', True)
    assert s.resolve("nickname") == ('"nickname"', False)
    # Declared columns that are not identifiers cannot be queried (Phase 00 rule).
    assert SrWeird._meta.lookup == {"id": '"id"', "spaced": '"my col"',
                                    "quoted": '"we""ird"', "raw": '"raw"'}


def test_field_name_wins_over_another_fields_column():
    class SrClash(Model):
        id = AutoField()
        author = ForeignKey(SrAuthor)          # column author_id
        author_id = IntegerField(null=True, db_column="other")

    assert SrClash._meta.lookup["author_id"] == '"other"'
    assert SrClash._meta.schema.resolve("author_id") == ('"other"', True)
    assert SrClash.objects.all()._resolve_column("author") == '"author_id"'


def test_resolve_column_uses_registry_quoting():
    qs = SrWeird.objects.all()
    assert qs._resolve_column("spaced") == '"my col"'
    assert qs._resolve_column("quoted") == '"we""ird"'
    db = RecordingDB()
    _run(SrWeird.objects.using(db).filter(quoted="v").order_by("-spaced").all())
    assert db.calls == [
        'SELECT * FROM "sr ""weird"" table" WHERE "we""ird" = $1 ORDER BY "my col" DESC'
    ]


# ── DDL parity ────────────────────────────────────────────────────────────────

@pytest.mark.parametrize("cls", ALL, ids=lambda c: c.__name__)
def test_create_table_sql_matches_reference(cls):
    spec = cls._meta.schema
    assert spec.create_table_sql() == _ref_create_table(cls)
    assert spec.create_table_sql(False) == _ref_create_table(cls, False)
    assert spec.drop_table_sql() == _ref_drop_table(cls)
    assert spec.drop_table_sql(False) == _ref_drop_table(cls, False)
    for name, field in cls._meta.fields.items():
        assert field.column_ddl() == _ref_column_ddl(field)
        assert spec.field(name).column_ddl() == _ref_column_ddl(field)


def test_create_table_sql_exact_strings():
    assert SrBook._meta.schema.create_table_sql() == (
        'CREATE TABLE IF NOT EXISTS "sr_book" ('
        '"id" SERIAL PRIMARY KEY, '
        '"title" VARCHAR(200) NOT NULL, '
        '"author_id" INTEGER NOT NULL REFERENCES "sr_author"("id") ON DELETE CASCADE, '
        '"editor_ref" INTEGER REFERENCES "sr_author"("id") ON DELETE SET NULL, '
        '"pages" INTEGER)'
    )
    assert SrWeird._meta.schema.drop_table_sql(False) == 'DROP TABLE "sr ""weird"" table"'


def test_fk_to_model_without_pk_is_rejected_at_definition():
    # 1.4.0 (deliberate break): 1.3.x rendered REFERENCES "srnopk"("None") and
    # failed at create_table; now the ForeignKey itself is rejected.
    with pytest.raises(ValueError, match="declares no primary key"):
        class SrRefNoPk(Model):
            id = AutoField()
            target = ForeignKey(SrNoPk)


def test_create_and_drop_table_send_registry_sql():
    db = RecordingDB()
    _run(SrDefaults.create_table(using=db))
    _run(SrDefaults.create_table(using=db, if_not_exists=False))
    _run(SrDefaults.drop_table(using=db))
    _run(SrDefaults.drop_table(using=db, if_exists=0))
    assert db.calls == [
        _ref_create_table(SrDefaults), _ref_create_table(SrDefaults, False),
        _ref_drop_table(SrDefaults), _ref_drop_table(SrDefaults, False),
    ]


def test_unbound_field_column_ddl():
    assert IntegerField(null=True).column_ddl() == '"None" INTEGER'
    assert CharField(max_length=5, db_column="c").column_ddl() == '"c" VARCHAR(5) NOT NULL'


def test_numeric_subclass_with_custom_str_is_quoted():
    class Evil(int):
        def __str__(self):
            return "1; DROP TABLE x"

    assert IntegerField(default=Evil(1), db_column="n").column_ddl() == (
        "\"n\" INTEGER NOT NULL DEFAULT '1; DROP TABLE x'"
    )


def test_other_defaults_render_python_str():
    from decimal import Decimal

    loop = {}
    loop["self"] = loop      # recursive container: str() handles it, so must we
    f = JSONField(default=loop, db_column="j")
    assert f.column_ddl() == _ref_column_ddl(f) == "\"j\" JSONB NOT NULL DEFAULT '{''self'': {...}}'"
    d = orm.Field(default=Decimal("1.10"), db_column="d")
    assert d.column_ddl() == _ref_column_ddl(d) == "\"d\" TEXT NOT NULL DEFAULT '1.10'"


# ── Deferred declaration errors (same as 1.3.x: raised at create_table) ──────

def test_fk_to_non_model_defines_and_queries_but_ddl_raises_attribute_error():
    class SrLoose(Model):
        id = AutoField()
        owner = ForeignKey("SrAuthor")

    spec = SrLoose._meta.schema
    assert not spec.complete
    assert spec.field("owner").references == (None, None, "CASCADE", None)
    db = RecordingDB()
    _run(SrLoose.objects.using(db).filter(owner_id=1).all())
    assert db.calls == ['SELECT * FROM "srloose" WHERE "owner_id" = $1']
    with pytest.raises(AttributeError, match="_meta"):
        _run(SrLoose.create_table(using=db))
    with pytest.raises(AttributeError):
        spec.create_table_sql()
    assert len(db.calls) == 1


def test_failing_sql_type_is_deferred_to_create_table():
    class Broken(orm.Field):
        sql_type = staticmethod(lambda f: f.missing_attr)

    class SrBroken(Model):
        id = AutoField()
        thing = Broken(null=True)

    assert not SrBroken._meta.schema.complete
    assert SrBroken._meta.schema.field("thing").pg_type is None
    with pytest.raises(AttributeError, match="missing_attr"):
        _run(SrBroken.create_table(using=RecordingDB()))


def test_on_delete_is_rechecked_by_the_registry():
    fk = ForeignKey(SrAuthor)
    fk.on_delete = "CASCADE; DROP TABLE sr_author"
    with pytest.raises(ValueError, match="on_delete"):
        type("SrBadFk", (Model,), {"__module__": __name__, "author": fk})


# ── Lifetime ──────────────────────────────────────────────────────────────────

def test_registry_entry_is_released_with_the_class():
    gc.collect()
    before = _cello.schema_count()

    class SrTemp(Model):
        id = AutoField()

    spec_id = SrTemp._meta.schema.id
    assert _cello.schema_count() == before + 1
    assert _cello.schema_get(spec_id).table == "srtemp"
    del SrTemp
    gc.collect()
    assert _cello.schema_count() == before
    assert _cello.schema_get(spec_id) is None


def test_ids_are_not_reused():
    seen = set()
    for _ in range(5):
        cls = type("SrLoop", (Model,), {"__module__": __name__, "id": AutoField()})
        seen.add(cls._meta.schema.id)
        del cls
        gc.collect()
    assert len(seen) == 5


def test_repr():
    assert re.fullmatch(r'<ModelSpec SrAuthor table="sr_author" fields=7>',
                        repr(SrAuthor._meta.schema))
    assert repr(SrBook._meta.schema.field("author")) == (
        '<FieldSpec SrBook.author column="author_id" type=INTEGER>'
    )
