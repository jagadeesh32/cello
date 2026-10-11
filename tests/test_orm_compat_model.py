"""
ORM compatibility suite — model semantics, defaults, save/delete, foreign keys
and DDL (``create_table`` / ``drop_table``).

See ``tests/orm_compat_support.py`` for conventions (``# PINNED-QUIRK:``).
"""

import pytest

from cello import orm
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

from orm_compat_support import (
    bind,
    call,
    columns,
    column_order,
    constraints,
    drop_tables,
    fk_rules,
    make_db,
    pg,
    run,
    table_exists,
)

pytestmark = pg


# ── Models ────────────────────────────────────────────────────────────────────

class Author(Model):
    id = AutoField()
    name = CharField(max_length=100)
    email = CharField(max_length=255, null=True, unique=True)
    active = BooleanField(default=True)
    rating = FloatField(default=1.5)
    nick = CharField(max_length=20, default="it's")
    bio = TextField(null=True, db_column="biography")

    class Meta:
        table_name = "oc_mdl_author"

    def greet(self):
        return f"hi {self.name}"

    @property
    def display(self):
        return f"{self.name} <{self.email}>"

    def __str__(self):
        return f"Author({self.name})"


class Book(Model):
    id = AutoField()
    title = CharField(max_length=200)
    author = ForeignKey(Author)
    editor = ForeignKey(Author, null=True, on_delete="SET NULL", db_column="editor_ref")
    pages = IntegerField(null=True)

    class Meta:
        table_name = "oc_mdl_book"


class OcCamelThing(Model):  # no Meta -> table name is the lower-cased class name
    id = AutoField()
    label = CharField(max_length=10)


class OcBase(Model):
    id = AutoField()
    name = CharField(max_length=50)

    class Meta:
        table_name = "oc_mdl_base"

    def shout(self):
        return self.name.upper()


class OcChild(OcBase):  # Meta is not inherited -> table "occhild"
    extra = IntegerField(null=True)


class Greeter:
    def hello(self):
        return f"hello {self.label}"


class OcMixed(Greeter, Model):
    id = AutoField()
    label = CharField(max_length=10)

    class Meta:
        table_name = "oc_mdl_mixed"


class OcNoPk(Model):
    label = CharField(max_length=10)

    class Meta:
        table_name = "oc_mdl_nopk"


class OcCode(Model):
    code = CharField(max_length=10, primary_key=True)
    label = CharField(max_length=10, null=True)

    class Meta:
        table_name = "oc_mdl_code"


class OcCodeRef(Model):
    id = AutoField()
    code = ForeignKey(OcCode)

    class Meta:
        table_name = "oc_mdl_coderef"


class OcDefaults(Model):
    id = AutoField()
    created = DateTimeField(auto_now_add=True)
    stamp = DateTimeField(auto_now_add=True, null=True)
    lazy = IntegerField(null=True, default=lambda: 7)
    maybe = IntegerField(null=True, default=5)
    big = BigIntegerField(default=10)
    payload = JSONField(default={})
    items = JSONField(default=[])
    flag = BooleanField(default=False)

    class Meta:
        table_name = "oc_mdl_defaults"


class OcBadJsonDefault(Model):
    id = AutoField()
    payload = JSONField(default={"a": 1})

    class Meta:
        table_name = "oc_mdl_badjson"


ALL_TABLES = [
    "oc_mdl_book", "oc_mdl_author", "occamelthing", "oc_mdl_base", "occhild",
    "oc_mdl_mixed", "oc_mdl_nopk", "oc_mdl_coderef", "oc_mdl_code", "oc_mdl_defaults",
    "oc_mdl_badjson",
]


@pytest.fixture(scope="module")
def db():
    database = make_db()
    yield database
    run(drop_tables(database, *ALL_TABLES))
    call(database.close)


@pytest.fixture
def tables(db):
    bind(db)
    run(drop_tables(db, *ALL_TABLES))

    async def create():
        for m in (Author, Book, OcCamelThing, OcBase, OcChild, OcMixed, OcNoPk, OcCode, OcDefaults):
            await m.create_table()

    run(create())
    yield db
    run(drop_tables(db, *ALL_TABLES))


# ── Instances are real model instances ───────────────────────────────────────

def test_instances_are_real_cls_instances(tables):
    async def main():
        a = await Author.objects.create(name="Ann", email="a@x")
        return a, await Author.objects.get(id=a.id), await Author.objects.all(), \
            await Author.objects.filter(name="Ann").first()

    created, got, listed, first = run(main())
    for obj in (created, got, listed[0], first):
        assert type(obj) is Author
        assert isinstance(obj, Author) and isinstance(obj, Model)
        assert obj.greet() == "hi Ann"
        assert obj.display == "Ann <a@x>"
        assert str(obj) == "Author(Ann)"
        assert repr(obj) == "<Author pk=1>"


def test_repr_and_default_str(tables):
    assert repr(Author(name="x")) == "<Author pk=None>"
    t = run(OcCamelThing.objects.create(label="x"))
    assert repr(t) == "<OcCamelThing pk=1>"
    assert str(t) == "<OcCamelThing pk=1>"  # no __str__ -> falls back to __repr__
    assert repr(OcNoPk(label="x")) == "<OcNoPk pk=None>"


def test_instances_have_identity_equality(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "no __eq__"): instances of the
    # same class with the same non-None pk are equal and hash alike.
    a = run(Author.objects.create(name="Ann"))
    b = run(Author.objects.get(id=a.id))
    c = run(Author.objects.get(id=a.id))
    assert b == c and not (b != c) and a == b
    assert b == b
    assert len({b, c}) == 1 and hash(b) == hash(c)
    # Unsaved instances compare (and hash) by identity only.
    u1, u2 = Author(name="x"), Author(name="x")
    assert u1 != u2 and u1 == u1 and len({u1, u2}) == 2


def test_instances_accept_extra_attributes(tables):
    a = run(Author.objects.create(name="Ann"))
    a.scratch = 123
    assert a.scratch == 123
    assert "scratch" not in a.to_dict()


def test_field_attributes_are_removed_from_class(tables):
    # PINNED-QUIRK: the metaclass deletes Field class attributes, so model
    # classes expose no per-field attribute (fields live only in _meta).
    assert not hasattr(Author, "name")
    assert not hasattr(Book, "author")
    assert "name" not in vars(Author)
    assert list(Author._meta.fields) == ["id", "name", "email", "active", "rating", "nick", "bio"]


def test_init_does_not_apply_defaults_and_ignores_unknown_kwargs():
    a = Author(name="x", bogus=1)
    # 1.4.0 (deliberate break; was PINNED-QUIRK "defaults NOT applied on
    # construction"): defaults are applied in __init__.
    assert a.active is True and a.rating == 1.5 and a.nick == "it's"
    assert a.id is None and a.email is None and a.bio is None
    assert not hasattr(a, "bogus")  # unknown kwargs silently ignored
    assert a.to_dict() == {
        "id": None, "name": "x", "email": None, "active": True, "rating": 1.5,
        "nick": "it's", "bio": None,
    }


def test_init_with_db_column_uses_field_name():
    a = Author(name="x", bio="b", biography="ignored")
    assert a.bio == "b"
    assert not hasattr(a, "biography")


def test_fk_init_attributes():
    author = Author(id=5, name="x")
    b1 = Book(title="t", author=author)
    assert b1.author is author
    # 1.4.0 (deliberate break; was PINNED-QUIRK "``<name>_id`` only exists if
    # passed in"): passing an instance also sets the id column.
    assert b1.author_id == 5
    assert b1.to_dict() == {
        "id": None, "title": "t", "author_id": 5, "editor_ref": None, "pages": None,
    }
    b2 = Book(title="t", author_id=5)
    assert b2.author is None and b2.author_id == 5
    assert b2.to_dict()["author_id"] == 5
    # FK with db_column: the "<name>_id" kwarg is the db_column itself.
    b3 = Book(title="t", author_id=1, editor_ref=9)
    assert b3.editor_ref == 9 and b3.editor is None
    assert b3.to_dict()["editor_ref"] == 9
    b4 = Book(title="t", author_id=1, editor_id=9)
    assert not hasattr(b4, "editor_id") and b4.editor is None


# ── Per-model exceptions ─────────────────────────────────────────────────────

def test_per_model_exception_classes(tables):
    assert issubclass(Author.DoesNotExist, orm.DoesNotExist)
    assert issubclass(Author.MultipleObjectsReturned, orm.MultipleObjectsReturned)
    assert issubclass(orm.DoesNotExist, Exception)
    assert issubclass(orm.MultipleObjectsReturned, Exception)
    assert Author.DoesNotExist is not Book.DoesNotExist
    assert not issubclass(Author.DoesNotExist, Book.DoesNotExist)
    assert not issubclass(orm.DoesNotExist, orm.MultipleObjectsReturned)
    assert Author.DoesNotExist.__name__ == "DoesNotExist"
    assert Author.MultipleObjectsReturned.__name__ == "MultipleObjectsReturned"
    # Subclass models get their own, unrelated to the parent's.
    assert OcChild.DoesNotExist is not OcBase.DoesNotExist
    assert not issubclass(OcChild.DoesNotExist, OcBase.DoesNotExist)


def test_get_raises_per_model_exceptions_with_messages(tables):
    run(Author.objects.create(name="Ann"))
    run(Author.objects.create(name="Ann"))
    with pytest.raises(Author.DoesNotExist) as ei:
        run(Author.objects.get(name="Nobody"))
    assert str(ei.value) == "Author matching query does not exist."
    assert type(ei.value) is Author.DoesNotExist
    with pytest.raises(Author.MultipleObjectsReturned) as ei:
        run(Author.objects.get(name="Ann"))
    assert str(ei.value) == "get() returned more than one Author."
    with pytest.raises(orm.DoesNotExist):
        run(Author.objects.get(name="Nobody"))
    with pytest.raises(Book.DoesNotExist):
        run(Book.objects.get(id=1))


# ── to_dict ───────────────────────────────────────────────────────────────────

def test_to_dict_keys_order_and_fk_naming(tables):
    async def main():
        a = await Author.objects.create(name="Ann", email="e", bio="bb")
        e = await Author.objects.create(name="Ed")
        b = await Book.objects.create(title="T", author_id=a.id, editor_ref=e.id, pages=3)
        return a, await Author.objects.get(id=a.id), await Book.objects.get(id=b.id)

    created, a, b = run(main())
    expected = {
        "id": 1, "name": "Ann", "email": "e", "active": True, "rating": 1.5,
        "nick": "it's", "bio": "bb",
    }
    assert created.to_dict() == expected and list(created.to_dict()) == list(expected)
    assert a.to_dict() == expected and list(a.to_dict()) == list(expected)
    # FK -> "<name>_id"; FK with db_column -> the db_column. Normal fields with
    # db_column keep the field name ("bio", not "biography").
    assert list(b.to_dict()) == ["id", "title", "author_id", "editor_ref", "pages"]
    assert b.to_dict() == {"id": 1, "title": "T", "author_id": 1, "editor_ref": 2, "pages": 3}
    assert type(b.to_dict()) is dict


# ── Defaults ──────────────────────────────────────────────────────────────────

def test_defaults_applied_on_insert(tables):
    a = run(Author.objects.create(name="Ann"))
    assert (a.active, a.rating, a.nick) == (True, 1.5, "it's")
    b = run(Author.objects.create(name="Bob", active=False, rating=0.0, nick=""))
    assert (b.active, b.rating, b.nick) == (False, 0.0, "")


def test_explicit_none_replaced_by_default(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "explicit None writes the
    # default"): an explicit None is written as NULL, so a NOT NULL column
    # with a default rejects it.
    with pytest.raises(RuntimeError):
        run(Author.objects.create(name="Ann", active=None, nick=None))
    assert run(Author.objects.count()) == 0


def test_defaults_model_quirks(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "auto_now_add without null=True
    # cannot be created"): auto_now_add is filled with now() on INSERT, and
    # every other default is applied.
    d = run(OcDefaults.objects.create())
    assert run(OcDefaults.objects.count()) == 1
    assert isinstance(d.created, str) and d.created.endswith("+00:00")
    assert isinstance(d.stamp, str) and d.stamp.endswith("+00:00")
    assert (d.lazy, d.maybe, d.big, d.payload, d.items, d.flag) == (7, 5, 10, {}, [], False)


def test_defaults_model_values_via_raw_insert(tables):
    db = tables

    async def main():
        await db.execute("INSERT INTO oc_mdl_defaults DEFAULT VALUES")
        return await OcDefaults.objects.get(id=1)

    d = run(main())
    assert isinstance(d.created, str) and d.created.endswith("+00:00")
    assert isinstance(d.stamp, str)
    assert d.lazy is None  # callable defaults have no DDL default
    assert (d.maybe, d.big, d.payload, d.items, d.flag) == (5, 10, {}, [], False)


def test_auto_now_add_nullable_inserts_null_and_callable_default_ignored(tables):
    class OcDefaults2(Model):
        id = AutoField()
        stamp = DateTimeField(auto_now_add=True, null=True)
        lazy = IntegerField(null=True, default=lambda: 7)
        maybe = IntegerField(null=True, default=5)

        class Meta:
            table_name = "oc_mdl_defaults2"

    async def main():
        await drop_tables(tables, "oc_mdl_defaults2")
        await OcDefaults2.create_table()
        try:
            d = await OcDefaults2.objects.create()
            e = await OcDefaults2.objects.create(maybe=None, lazy=3)
            return d, e
        finally:
            await OcDefaults2.drop_table()

    d, e = run(main())
    # 1.4.0 (deliberate breaks; were PINNED-QUIRKs):
    # auto_now_add with null=True stores now(), not NULL.
    assert isinstance(d.stamp, str) and isinstance(e.stamp, str)
    # Callable defaults are invoked.
    assert d.lazy == 7 and e.lazy == 3
    # Explicit None on a nullable defaulted field stores NULL.
    assert d.maybe == 5 and e.maybe is None


# ── AutoField / save / delete ─────────────────────────────────────────────────

def test_autofield_populated_and_sequential(tables):
    async def main():
        return [await Author.objects.create(name=n) for n in ("a", "b", "c")]

    objs = run(main())
    assert [o.id for o in objs] == [1, 2, 3]
    assert all(type(o.id) is int for o in objs)


def test_save_inserts_then_updates(tables):
    async def main():
        a = Author(name="Ann", email="x")
        ret = await a.save()
        assert ret is a
        assert a.id == 1 and a.active is True and a.rating == 1.5  # populated from RETURNING
        a.name = "Anne"
        a.email = None
        ret2 = await a.save()
        assert ret2 is a
        assert await Author.objects.count() == 1
        return await Author.objects.get(id=1)

    got = run(main())
    assert got.name == "Anne" and got.email is None


def test_save_update_writes_every_field(tables):
    async def main():
        await Author.objects.create(name="Ann", email="e", active=False, bio="b", rating=9.0)
        # A partially populated instance with a pk overwrites all other columns.
        await Author(id=1, name="New").save()
        return await Author.objects.get(id=1)

    got = run(main())
    # PINNED-QUIRK: None fields with a default are written as the default
    # (active -> True, rating -> 1.5) rather than left untouched or NULLed.
    assert got.to_dict() == {
        "id": 1, "name": "New", "email": None, "active": True, "rating": 1.5,
        "nick": "it's", "bio": None,
    }


def test_save_with_unknown_pk_is_silent_noop(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "UPDATE path, silent no-op"):
    # save() with a pk whose row does not exist inserts it; create() with an
    # explicit pk always inserts.
    a = run(Author(id=42, name="Ghost").save())
    assert a.id == 42
    assert run(Author.objects.count()) == 1
    b = run(Author.objects.create(id=99, name="Ghost"))
    assert b.id == 99 and b.active is True
    assert run(Author.objects.count()) == 2
    assert sorted(x.id for x in run(Author.objects.all().all())) == [42, 99]


def test_non_auto_primary_key_cannot_be_created(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "create(pk=...) takes the UPDATE
    # path"): create() with a non-AutoField pk inserts; without the pk it still
    # inserts NULL -> error.
    obj = run(OcCode.objects.create(code="A", label="x"))
    assert obj.code == "A"
    assert run(OcCode.objects.count()) == 1
    assert run(OcCode.objects.get(code="A")).label == "x"
    with pytest.raises(RuntimeError):
        run(OcCode.objects.create(label="x"))


def test_model_delete(tables):
    async def main():
        a = await Author.objects.create(name="Ann")
        await Author.objects.create(name="Bob")
        n1 = await a.delete()
        n2 = await a.delete()
        return a, n1, n2

    a, n1, n2 = run(main())
    assert (n1, n2) == (1, 0)
    assert a.id == 1  # pk is kept on the instance
    with pytest.raises(Author.DoesNotExist):
        run(Author.objects.get(id=1))
    assert run(Author.objects.count()) == 1
    assert run(Author(name="unsaved").delete()) == 0


def test_delete_without_pk_field_raises_type_error(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "TypeError from getattr(self,
    # None)"): a model with no primary key raises a clear ValueError.
    obj = run(OcNoPk.objects.create(label="x"))
    assert obj.label == "x" and repr(obj) == "<OcNoPk pk=None>"
    with pytest.raises(ValueError, match="OcNoPk has no primary key"):
        run(obj.delete())
    assert run(OcNoPk.objects.count()) == 1


# ── Foreign keys ─────────────────────────────────────────────────────────────

def test_fk_create_with_object(tables):
    async def main():
        a = await Author.objects.create(name="Ann")
        b = await Book.objects.create(title="T", author=a)
        return a, b, await Book.objects.get(id=b.id)

    a, b, got = run(main())
    # 1.4.0 (deliberate break; was PINNED-QUIRK "``author`` overwritten with the
    # raw id, ``author_id`` never set"): the assigned instance is kept and
    # ``author_id`` holds its id.
    assert b.author is a
    assert b.author_id == a.id and type(b.author_id) is int
    assert b.to_dict()["author_id"] == a.id
    # Loaded rows: both attributes hold the raw id; no lazy related object.
    assert got.author == a.id and got.author_id == a.id
    assert got.editor is None and got.editor_ref is None


def test_fk_create_with_id(tables):
    async def main():
        a = await Author.objects.create(name="Ann")
        return a, await Book.objects.create(title="T", author_id=a.id)

    a, b = run(main())
    assert b.author == a.id and b.author_id == a.id


def test_fk_unsaved_or_missing_target_raises(tables):
    with pytest.raises(RuntimeError):
        run(Book.objects.create(title="T", author=Author(name="unsaved")))
    with pytest.raises(RuntimeError):
        run(Book.objects.create(title="T", author_id=12345))
    assert run(Book.objects.count()) == 0


def test_fk_reassign_object_after_load_is_ignored(tables):
    async def main():
        a1 = await Author.objects.create(name="A1")
        a2 = await Author.objects.create(name="A2")
        await Book.objects.create(title="T", author_id=a1.id)
        b = await Book.objects.get(id=1)
        b.author = a2
        await b.save()
        after_obj = (await Book.objects.get(id=1)).author_id
        b.author_id = a2.id
        await b.save()
        after_id = (await Book.objects.get(id=1)).author_id
        return a1, a2, after_obj, after_id

    a1, a2, after_obj, after_id = run(main())
    # 1.4.0 (deliberate break; was PINNED-QUIRK "reassigning ``book.author =
    # other`` is silently lost"): assignment updates ``author_id`` and persists.
    assert after_obj == a2.id
    assert after_id == a2.id


def test_fk_on_delete_cascade_and_set_null(tables):
    async def main():
        a = await Author.objects.create(name="A")
        e = await Author.objects.create(name="E")
        other = await Author.objects.create(name="O")
        await Book.objects.create(title="1", author_id=a.id, editor_ref=e.id)
        await Book.objects.create(title="2", author_id=other.id, editor_ref=e.id)
        await e.delete()
        mid = [(b.title, b.editor_ref) for b in await Book.objects.order_by("title")]
        await a.delete()
        end = [b.title for b in await Book.objects.order_by("title")]
        return mid, end

    mid, end = run(main())
    assert mid == [("1", None), ("2", None)]
    assert end == ["2"]


# ── Inheritance / mixins / table names ───────────────────────────────────────

def test_subclass_model_fields_table_and_methods(tables):
    async def main():
        c = await OcChild.objects.create(name="kid", extra=3)
        return c, await OcChild.objects.get(id=c.id), await column_order(tables, "occhild")

    c, got, cols = run(main())
    assert OcChild._meta.table_name == "occhild"
    assert type(got) is OcChild and isinstance(got, OcBase)
    assert got.shout() == "KID"
    # PINNED-QUIRK: a subclass's own fields come BEFORE inherited ones, so the
    # inherited pk is not the first column / to_dict key.
    assert list(got.to_dict()) == ["extra", "id", "name"]
    assert cols == ["extra", "id", "name"]
    assert got.to_dict() == {"extra": 3, "id": 1, "name": "kid"}
    assert run(OcBase.objects.count()) == 0  # separate table, no multi-table inheritance


def test_plain_mixin_methods(tables):
    m = run(OcMixed.objects.create(label="m"))
    got = run(OcMixed.objects.get(id=m.id))
    assert got.hello() == "hello m" and isinstance(got, Greeter)


def test_default_table_name_is_lowercased_class_name(tables):
    assert OcCamelThing._meta.table_name == "occamelthing"
    assert run(table_exists(tables, "occamelthing"))


# ── DDL ──────────────────────────────────────────────────────────────────────

def _col(c):
    return (c["data_type"], c["character_maximum_length"], c["is_nullable"], c["column_default"])


def test_ddl_author_columns(tables):
    cols = run(columns(tables, "oc_mdl_author"))
    assert list(cols) == ["id", "name", "email", "active", "rating", "nick", "biography"]
    assert {k: _col(v) for k, v in cols.items()} == {
        "id": ("integer", None, "NO", "nextval('oc_mdl_author_id_seq'::regclass)"),
        "name": ("character varying", 100, "NO", None),
        "email": ("character varying", 255, "YES", None),
        "active": ("boolean", None, "NO", "true"),
        "rating": ("double precision", None, "NO", "1.5"),
        "nick": ("character varying", 20, "NO", "'it''s'::character varying"),
        "biography": ("text", None, "YES", None),
    }
    assert run(constraints(tables, "oc_mdl_author")) == [
        ("PRIMARY KEY", "id"), ("UNIQUE", "email"),
    ]


def test_ddl_book_foreign_keys(tables):
    cols = run(columns(tables, "oc_mdl_book"))
    assert {k: _col(v) for k, v in cols.items()} == {
        "id": ("integer", None, "NO", "nextval('oc_mdl_book_id_seq'::regclass)"),
        "title": ("character varying", 200, "NO", None),
        "author_id": ("integer", None, "NO", None),
        "editor_ref": ("integer", None, "YES", None),
        "pages": ("integer", None, "YES", None),
    }
    assert run(fk_rules(tables, "oc_mdl_book")) == [
        ("author_id", "oc_mdl_author", "id", "CASCADE"),
        ("editor_ref", "oc_mdl_author", "id", "SET NULL"),
    ]


def test_ddl_defaults_model(tables):
    cols = run(columns(tables, "oc_mdl_defaults"))
    assert {k: _col(v) for k, v in cols.items()} == {
        "id": ("integer", None, "NO", "nextval('oc_mdl_defaults_id_seq'::regclass)"),
        "created": ("timestamp with time zone", None, "NO", "now()"),
        "stamp": ("timestamp with time zone", None, "YES", "now()"),
        "lazy": ("integer", None, "YES", None),
        "maybe": ("integer", None, "YES", "5"),
        "big": ("bigint", None, "NO", "10"),
        "payload": ("jsonb", None, "NO", "'{}'::jsonb"),
        "items": ("jsonb", None, "NO", "'[]'::jsonb"),
        "flag": ("boolean", None, "NO", "false"),
    }


def test_ddl_non_integer_pk_target_breaks_fk(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "ForeignKey is always INTEGER"):
    # the FK column takes the target pk's type, so a VARCHAR pk can be referenced.
    run(OcCodeRef.create_table())
    assert run(table_exists(tables, "oc_mdl_coderef"))
    cols = run(columns(tables, "oc_mdl_coderef"))
    assert _col(cols["code_id"]) == ("character varying", 10, "NO", None)
    run(OcCode.objects.create(code="K", label="k"))
    ref = run(OcCodeRef.objects.create(code="K"))
    assert ref.code_id == "K"


def test_ddl_json_dict_default_is_invalid(tables):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "dict defaults rendered with
    # Python repr, create_table fails"): the default is rendered as JSON.
    run(OcBadJsonDefault.create_table())
    cols = run(columns(tables, "oc_mdl_badjson"))
    assert cols["payload"]["column_default"] == """'{"a": 1}'::jsonb"""
    call(tables.execute, "INSERT INTO oc_mdl_badjson DEFAULT VALUES")
    assert run(OcBadJsonDefault.objects.get(id=1)).payload == {"a": 1}


def test_create_and_drop_table_flags(tables):
    async def main():
        await Author.create_table()  # IF NOT EXISTS by default: no error
        with pytest.raises(RuntimeError):
            await Author.create_table(if_not_exists=False)
        await OcCamelThing.drop_table()
        assert not await table_exists(tables, "occamelthing")
        await OcCamelThing.drop_table()  # IF EXISTS by default: no error
        with pytest.raises(RuntimeError):
            await OcCamelThing.drop_table(if_exists=False)
        await OcCamelThing.create_table(if_not_exists=False)
        assert await table_exists(tables, "occamelthing")

    run(main())


def test_create_table_does_not_alter_existing(tables):
    async def main():
        await drop_tables(tables, "occamelthing")
        await tables.execute('CREATE TABLE "occamelthing" (id SERIAL PRIMARY KEY)')
        await OcCamelThing.create_table()
        return await column_order(tables, "occamelthing")

    assert run(main()) == ["id"]


# ── Database binding ─────────────────────────────────────────────────────────

def test_unbound_orm_raises_runtime_error(tables):
    orm.setup(None)
    try:
        for coro_fn in (
            lambda: Author.objects.all().all(),
            lambda: Author.objects.count(),
            lambda: Author.objects.create(name="x"),
            lambda: Author.create_table(),
        ):
            with pytest.raises(RuntimeError, match="ORM is not bound to a database"):
                run(coro_fn())
    finally:
        bind(tables)


def test_explicit_using(tables):
    orm.setup(None)
    try:
        db = tables

        async def main():
            a = await Author.objects.create(name="A", using=db)
            await Author(name="B").save(using=db)
            names = [x.name for x in await Author.objects.using(db).order_by("id")]
            n = await Author.objects.all().using(db).count()
            await a.delete(using=db)
            return names, n, await Author.objects.using(db).count()

        assert run(main()) == (["A", "B"], 2, 1)
    finally:
        bind(tables)


def test_base_field_and_default_charfield_ddl(tables):
    class OcPlain(Model):
        id = AutoField()
        raw = orm.Field(null=True)
        title = CharField()
        uniq = IntegerField(unique=True, null=True)

        class Meta:
            table_name = "oc_mdl_plain"

    async def main():
        await drop_tables(tables, "oc_mdl_plain")
        await OcPlain.create_table(using=tables)
        try:
            cols = await columns(tables, "oc_mdl_plain")
            cons = await constraints(tables, "oc_mdl_plain")
            obj = await OcPlain.objects.create(raw="r", title="t")
            return cols, cons, obj
        finally:
            await OcPlain.drop_table(using=tables)

    cols, cons, obj = run(main())
    assert {k: _col(v) for k, v in cols.items()} == {
        "id": ("integer", None, "NO", "nextval('oc_mdl_plain_id_seq'::regclass)"),
        "raw": ("text", None, "YES", None),       # bare Field -> TEXT
        "title": ("character varying", 255, "NO", None),  # CharField default 255
        "uniq": ("integer", None, "YES", None),
    }
    assert cons == [("PRIMARY KEY", "id"), ("UNIQUE", "uniq")]
    assert obj.to_dict() == {"id": 1, "raw": "r", "title": "t", "uniq": None}
    assert not run(table_exists(tables, "oc_mdl_plain"))
