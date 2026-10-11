"""
ORM semantics fixes in 1.4.0 (the "Deliberate breaks — ORM" section of
``docs/releases/v1.4.0.md``).

SQL-shape checks use a recording stand-in pool and run everywhere; behaviour
checks run against live Postgres and are skipped when it is unreachable.
"""

import asyncio
import json

import pytest

from cello import TransactionAborted, transactional
from cello.database import current_transaction
from cello.orm import (
    AutoField,
    BigIntegerField,
    BooleanField,
    CharField,
    DateTimeField,
    FieldError,
    ForeignKey,
    IntegerField,
    JSONField,
    Model,
    TextField,
)

from orm_compat_support import bind, call, columns, drop_tables, make_db, pg, run


# ── Models ────────────────────────────────────────────────────────────────────

class FxAuthor(Model):
    id = AutoField()
    name = CharField(max_length=50)
    active = BooleanField(default=True)
    score = IntegerField(null=True, default=5)

    class Meta:
        table_name = "fx_author"


class FxBook(Model):
    id = AutoField()
    title = CharField(max_length=50)
    author = ForeignKey(FxAuthor, null=True)
    editor = ForeignKey(FxAuthor, null=True, on_delete="SET NULL", db_column="editor_ref")
    code = CharField(max_length=10, null=True, db_column="book_code")
    small = IntegerField(null=True)
    big = BigIntegerField(null=True)

    class Meta:
        table_name = "fx_book"


_counter = {"n": 0}


def _next():
    _counter["n"] += 1
    return _counter["n"]


class FxDefaults(Model):
    id = AutoField()
    seq = IntegerField(default=_next)
    tags = JSONField(default=[])
    meta = JSONField(default={"k": [1, 2]})
    created = DateTimeField(auto_now_add=True)
    touched = DateTimeField(auto_now=True)
    note = TextField(null=True, default="n/a")

    class Meta:
        table_name = "fx_defaults"


class _BigSerial(IntegerField):
    sql_type = "BIGSERIAL"


class _Uuid(TextField):
    sql_type = "UUID"


class FxBigPk(Model):
    id = _BigSerial(primary_key=True)
    label = TextField(null=True)

    class Meta:
        table_name = "fx_bigpk"


class FxUuidPk(Model):
    id = _Uuid(primary_key=True)

    class Meta:
        table_name = "fx_uuidpk"


class FxRefs(Model):
    id = AutoField()
    big = ForeignKey(FxBigPk, null=True)
    uid = ForeignKey(FxUuidPk, null=True)

    class Meta:
        table_name = "fx_refs"


class FxStamp(Model):
    id = AutoField()
    label = TextField(null=True)
    touched = DateTimeField(auto_now=True)

    class Meta:
        table_name = "fx_stamp"


class FxNoPk(Model):
    label = TextField()

    class Meta:
        table_name = "fx_nopk"


class RecordingDB:
    def __init__(self, rows=None):
        self.calls = []
        self.rows = rows or []

    async def fetch(self, sql, *params):
        self.calls.append((sql, params))
        return self.rows

    async def fetchrow(self, sql, *params):
        self.calls.append((sql, params))
        return {"n": 0}

    async def execute(self, sql, *params):
        self.calls.append((sql, params))
        return 0


def _sql(qs):
    return qs._select_sql()


# ── 1. Undeclared identifiers are FieldError ─────────────────────────────────

@pytest.mark.parametrize(
    "build",
    [
        lambda: FxBook.objects.filter(nickname=1),
        lambda: FxBook.objects.exclude(nickname__gt=1),
        lambda: FxBook.objects.order_by("-nickname"),
    ],
    ids=["filter", "exclude", "order_by"],
)
def test_undeclared_identifier_is_field_error(build):
    with pytest.raises(FieldError) as ei:
        build()
    msg = str(ei.value)
    assert "'nickname'" in msg and "Declare the field on FxBook" in msg
    assert "await db.fetch(" in msg


def test_undeclared_identifier_in_update_and_values_sends_nothing():
    db = RecordingDB()
    with pytest.raises(FieldError, match="'nickname'"):
        run(FxBook.objects.using(db).update(nickname="x"))
    with pytest.raises(FieldError, match="'nickname'"):
        run(FxBook.objects.using(db).values("title", "nickname"))
    assert db.calls == []


def test_fk_columns_and_db_column_remain_valid():
    sql, params = _sql(
        FxBook.objects.filter(author_id=1, editor_ref=2, book_code="c", code="d")
        .order_by("editor", "-book_code")
    )
    assert sql == (
        'SELECT * FROM "fx_book" WHERE "author_id" = $1 AND "editor_ref" = $2 '
        'AND "book_code" = $3 AND "book_code" = $4 ORDER BY "editor_ref" ASC, "book_code" DESC'
    )
    assert params == [1, 2, "c", "d"]
    # A FK with db_column is not reachable as "<name>_id".
    with pytest.raises(FieldError):
        FxBook.objects.filter(editor_id=1)


# ── 2. Defaults in __init__ ──────────────────────────────────────────────────

def test_defaults_applied_on_construction():
    a = FxAuthor(name="x")
    assert (a.active, a.score, a.id) == (True, 5, None)
    # Explicit None is kept (not replaced by the default).
    b = FxAuthor(name="x", score=None, active=False)
    assert (b.active, b.score) == (False, None)


def test_callable_default_called_per_instance_and_containers_copied():
    d1, d2 = FxDefaults(), FxDefaults()
    assert d2.seq == d1.seq + 1
    assert d1.tags == [] and d1.tags is not d2.tags
    assert d1.meta == {"k": [1, 2]} and d1.meta is not d2.meta
    d1.meta["k"].append(3)
    assert d2.meta == {"k": [1, 2]} and FxDefaults().meta == {"k": [1, 2]}
    # auto_now / auto_now_add are filled by the database, not __init__.
    assert d1.created is None and d1.touched is None
    # A callable default is not called when a value is passed.
    before = _counter["n"]
    assert FxDefaults(seq=0).seq == 0 and _counter["n"] == before


def test_insert_uses_now_for_auto_timestamps():
    db = RecordingDB()
    run(FxDefaults.objects.create(using=db, seq=1))
    sql, params = db.calls[0]
    assert sql == (
        'INSERT INTO "fx_defaults" ("seq", "tags", "meta", "created", "touched", "note") '
        "VALUES ($1, $2, $3, now(), now(), $4) RETURNING *"
    )
    assert params == (1, [], {"k": [1, 2]}, "n/a")
    # An explicit auto_now_add value is sent; auto_now is always now().
    db = RecordingDB()
    run(FxDefaults.objects.create(using=db, seq=1, created="x", touched="y"))
    sql, params = db.calls[0]
    assert "VALUES ($1, $2, $3, $4, now(), $5)" in sql and params[3] == "x"


# ── 3. create() / save() ─────────────────────────────────────────────────────

def test_create_with_explicit_pk_is_insert():
    db = RecordingDB()
    run(FxAuthor.objects.create(using=db, id=7, name="a"))
    assert db.calls[0][0].startswith('INSERT INTO "fx_author" ("id", "name", "active", "score")')


def test_save_with_pk_updates_then_inserts_when_missing():
    db = RecordingDB(rows=[])  # UPDATE ... RETURNING matches nothing
    run(FxAuthor(id=3, name="a").save(using=db))
    (upd, upd_params), (ins, _) = db.calls
    assert upd == (
        'UPDATE "fx_author" SET "name" = $1, "active" = $2, "score" = $3 '
        'WHERE "id" = $4 RETURNING "id"'
    )
    assert upd_params == ("a", True, 5, 3)
    assert ins.startswith('INSERT INTO "fx_author" ("id", "name"')

    db = RecordingDB(rows=[{"id": 3}])  # row exists -> no INSERT
    run(FxAuthor(id=3, name="a").save(using=db))
    assert len(db.calls) == 1


def test_save_without_pk_inserts_without_autofield():
    db = RecordingDB()
    run(FxAuthor(name="a").save(using=db))
    assert db.calls[0][0] == (
        'INSERT INTO "fx_author" ("name", "active", "score") VALUES ($1, $2, $3) RETURNING *'
    )


def test_auto_now_set_on_update():
    db = RecordingDB(rows=[{"id": 1, "touched": "T"}])
    obj = FxDefaults(id=1)
    run(obj.save(using=db))
    sql, _ = db.calls[0]
    assert '"touched" = now()' in sql and sql.endswith("RETURNING *")
    assert obj.touched == "T"


# ── 4. delete() ──────────────────────────────────────────────────────────────

def test_delete_on_model_without_pk_is_value_error():
    with pytest.raises(ValueError, match="FxNoPk has no primary key"):
        run(FxNoPk(label="x").delete(using=RecordingDB()))


def test_delete_unsaved_instance_sends_nothing():
    db = RecordingDB()
    assert run(FxAuthor(name="x").delete(using=db)) == 0
    assert db.calls == []


# ── 5. ForeignKey ────────────────────────────────────────────────────────────

def test_fk_assignment_keeps_id_in_sync():
    a1, a2 = FxAuthor(id=1, name="a"), FxAuthor(id=2, name="b")
    b = FxBook(title="t", author=a1)
    assert b.author is a1 and b.author_id == 1
    b.author = a2
    assert b.author is a2 and b.author_id == 2
    b.author = 9
    assert b.author == 9 and b.author_id == 9
    b.author = a1
    b.author_id = 2               # a different id drops the cached instance
    assert b.author == 2 and b.author_id == 2
    b.author = a1
    b.author_id = 1               # the same id keeps it
    assert b.author is a1
    b.editor = a2                 # db_column FK
    assert b.editor_ref == 2 and b.to_dict()["editor_ref"] == 2
    b.author = None
    assert b.author is None and b.author_id is None


def test_fk_instance_saved_later_uses_its_pk():
    a = FxAuthor(name="a")
    b = FxBook(title="t", author=a)
    assert b.author_id is None
    a.id = 4  # e.g. after `await a.save()`
    assert b.to_dict()["author_id"] == 4
    db = RecordingDB()
    run(b.save(using=db))
    assert db.calls[0][1][1] == 4


def test_fk_filter_update_by_instance_use_pk():
    a = FxAuthor(id=3, name="a")
    sql, params = _sql(FxBook.objects.filter(author=a, editor__in=[a, 4]))
    assert params == [3, 3, 4]
    db = RecordingDB()
    run(FxBook.objects.using(db).filter(id=1).update(author=a, editor=None))
    assert db.calls[0] == (
        'UPDATE "fx_book" SET "author_id" = $1, "editor_ref" = $2 WHERE "id" = $3', (3, None, 1)
    )


def test_fk_ddl_type_follows_target_pk():
    ddl = FxRefs._meta.schema.create_table_sql()
    assert '"big_id" BIGINT REFERENCES "fx_bigpk"("id")' in ddl
    assert '"uid_id" UUID REFERENCES "fx_uuidpk"("id")' in ddl
    assert '"author_id" INTEGER' in FxBook._meta.schema.create_table_sql()
    assert FxBigPk._meta.schema.field("id").pg_type == "BIGSERIAL"


def test_fk_to_model_without_pk_fails_at_definition():
    with pytest.raises(ValueError, match="FxNoPk declares no primary key"):
        class FxBadRef(Model):
            id = AutoField()
            target = ForeignKey(FxNoPk)


def test_fk_int_range_follows_target_pk():
    FxRefs.objects.filter(big=2**40)  # BIGINT: fine
    with pytest.raises(ValueError, match="out of range"):
        FxBook.objects.filter(author=2**40)


# ── 6. JSONField DDL default ─────────────────────────────────────────────────

def test_json_default_renders_as_json():
    ddl = FxDefaults._meta.schema.create_table_sql()
    assert """"meta" JSONB NOT NULL DEFAULT '{"k": [1, 2]}'""" in ddl
    assert """"tags" JSONB NOT NULL DEFAULT '[]'""" in ddl
    assert JSONField(default={"q": "it's"}, db_column="j").column_ddl() == (
        '"j" JSONB NOT NULL DEFAULT ' + "'" + json.dumps({"q": "it's"}).replace("'", "''") + "'"
    )


# ── 7. Equality and hashing ──────────────────────────────────────────────────

def test_eq_and_hash():
    a, b = FxAuthor(id=1, name="a"), FxAuthor(id=1, name="other")
    assert a == b and hash(a) == hash(b) and len({a, b}) == 1
    assert a != FxAuthor(id=2, name="a")
    assert a != FxBook(id=1, title="t")         # different class, same pk
    assert hash(a) == hash((FxAuthor, 1))
    assert a != 1 and a != None  # noqa: E711
    u1, u2 = FxAuthor(name="x"), FxAuthor(name="x")
    assert u1 != u2 and u1 == u1 and hash(u1) == object.__hash__(u1)
    assert len({u1, u2}) == 2
    n1, n2 = FxNoPk(label="x"), FxNoPk(label="x")
    assert n1 != n2 and n1 == n1


# ── 8. QuerySet SQL ──────────────────────────────────────────────────────────

def test_none_is_null():
    assert _sql(FxBook.objects.filter(small=None)) == (
        'SELECT * FROM "fx_book" WHERE "small" IS NULL', [])
    assert _sql(FxBook.objects.filter(small__ne=None)) == (
        'SELECT * FROM "fx_book" WHERE "small" IS NOT NULL', [])
    assert _sql(FxBook.objects.exclude(small=None)) == (
        'SELECT * FROM "fx_book" WHERE ("small" IS NULL) IS NOT TRUE', [])


def test_in_rejects_strings_and_materializes_iterables():
    for v in ("ab", b"ab", bytearray(b"ab")):
        with pytest.raises(TypeError):
            FxBook.objects.filter(title__in=v)
    sql, params = _sql(FxBook.objects.filter(small__in=(x for x in (1, 2))))
    assert sql == 'SELECT * FROM "fx_book" WHERE "small" IN ($1, $2)' and params == [1, 2]
    assert _sql(FxBook.objects.filter(small__in={3}))[1] == [3]
    with pytest.raises(TypeError):
        FxBook.objects.filter(small__in=5)


def test_like_escaping():
    sql, params = _sql(FxBook.objects.filter(title__contains="50%_a\\b"))
    assert sql == """SELECT * FROM "fx_book" WHERE "title" LIKE $1 ESCAPE '\\'"""
    assert params == ["%50\\%\\_a\\\\b%"]
    assert _sql(FxBook.objects.filter(title__istartswith="_"))[1] == ["\\_%"]
    assert _sql(FxBook.objects.filter(title__iendswith="%"))[0].endswith("ILIKE $1 ESCAPE '\\'")
    assert _sql(FxBook.objects.filter(title__iendswith="%"))[1] == ["%\\%"]


def test_exclude_is_not_of_conjunction_and_null_safe():
    sql, params = _sql(FxBook.objects.filter(title="t").exclude(small=1, big__gt=2))
    assert sql == (
        'SELECT * FROM "fx_book" WHERE "title" = $1 AND ("small" = $2 AND "big" > $3) IS NOT TRUE'
    )
    assert params == ["t", 1, 2]
    assert _sql(FxBook.objects.exclude(small__in=[]))[0].endswith("WHERE (FALSE) IS NOT TRUE")


def test_order_by_replaces():
    qs = FxBook.objects.order_by("title").order_by("-id")
    assert _sql(qs)[0] == 'SELECT * FROM "fx_book" ORDER BY "id" DESC'
    assert _sql(qs.order_by())[0] == 'SELECT * FROM "fx_book"'


def test_count_and_exists_respect_slicing():
    db = RecordingDB()
    qs = FxBook.objects.using(db).filter(small=1)
    run(qs.count())
    run(qs.limit(5).offset(2).count())
    run(qs.offset(3).exists())
    run(qs.exists())
    assert [c[0] for c in db.calls] == [
        'SELECT count(*) AS n FROM "fx_book" WHERE "small" = $1',
        'SELECT count(*) AS n FROM (SELECT 1 FROM "fx_book" WHERE "small" = $1 LIMIT 5 OFFSET 2) AS _cello_sub',
        'SELECT EXISTS (SELECT 1 FROM (SELECT 1 FROM "fx_book" WHERE "small" = $1 OFFSET 3) AS _cello_sub) AS n',
        'SELECT EXISTS (SELECT 1 FROM "fx_book" WHERE "small" = $1) AS n',
    ]


def test_sliced_update_delete_raise_and_empty_update_is_noop():
    db = RecordingDB()
    with pytest.raises(TypeError, match="cannot update a sliced queryset"):
        run(FxBook.objects.using(db).limit(1).update(title="x"))
    with pytest.raises(TypeError, match="cannot delete a sliced queryset"):
        run(FxBook.objects.using(db).offset(1).delete())
    assert run(FxBook.objects.using(db).filter(id=1).update()) == 0
    assert db.calls == []


def test_int_range_checked_per_column_type():
    with pytest.raises(ValueError, match=r"out of range for FxBook\.small"):
        FxBook.objects.filter(small=2**31)
    with pytest.raises(ValueError):
        FxBook.objects.exclude(small__in=[1, -(2**31) - 1])
    with pytest.raises(ValueError):
        run(FxBook.objects.using(RecordingDB()).update(small=2**31))
    FxBook.objects.filter(small=2**31 - 1, big=2**63 - 1)
    with pytest.raises(ValueError):
        FxBook.objects.filter(big=2**63)


def test_values_selects_only_requested_columns():
    db = RecordingDB(rows=[{"author_id": 1, "book_code": "c"}])
    out = run(FxBook.objects.using(db).values("author", "code", "author_id"))
    assert db.calls[0][0] == 'SELECT "author_id", "book_code", "author_id" FROM "fx_book"'
    assert out == [{"author": 1, "code": "c", "author_id": 1}]


# ── Live database ─────────────────────────────────────────────────────────────

LIVE_TABLES = ["fx_refs", "fx_bigpk", "fx_uuidpk", "fx_book", "fx_author", "fx_defaults",
               "fx_stamp"]


@pytest.fixture(scope="module")
def db():
    database = make_db(pool_size=4)
    yield database
    run(drop_tables(database, *LIVE_TABLES))
    call(database.close)


@pytest.fixture
def live(db):
    bind(db)

    async def setup():
        await drop_tables(db, *LIVE_TABLES)
        for m in (FxAuthor, FxBook, FxDefaults, FxBigPk, FxUuidPk, FxRefs, FxStamp):
            await m.create_table()

    run(setup())
    yield db


@pg
def test_live_defaults_and_auto_timestamps(live):
    async def main():
        d = await FxDefaults.objects.create()
        s = await FxStamp.objects.create(label="a")
        first_touch = s.touched
        await asyncio.sleep(0.01)
        s.label = "b"
        await s.save()
        again = await FxStamp.objects.get(id=s.id)
        return d, s, first_touch, again

    d, s, first_touch, again = run(main())
    assert d.created.endswith("+00:00") and d.touched.endswith("+00:00")
    assert d.meta == {"k": [1, 2]} and d.tags == [] and d.note == "n/a"
    assert first_touch.endswith("+00:00")
    assert again.touched > first_touch and again.label == "b"
    assert s.touched == again.touched


@pg
def test_live_explicit_none_stores_null(live):
    a = run(FxAuthor.objects.create(name="a", score=None))
    assert a.score is None
    assert run(FxAuthor.objects.get(id=a.id)).score is None
    b = run(FxAuthor.objects.create(name="b"))
    assert b.score == 5


@pg
def test_live_save_upsert_semantics(live):
    async def main():
        a = await FxAuthor(id=10, name="x").save()      # no row: INSERT
        a.name = "y"
        await a.save()                                   # row exists: UPDATE
        b = await FxAuthor.objects.create(name="z")      # serial 1
        return a, b, await FxAuthor.objects.order_by("id").values("id", "name")

    a, b, rows = run(main())
    assert rows == [{"id": 1, "name": "z"}, {"id": 10, "name": "y"}]


@pg
def test_live_fk_semantics(live):
    async def main():
        a1 = await FxAuthor.objects.create(name="a1")
        a2 = await FxAuthor.objects.create(name="a2")
        b1 = await FxBook.objects.create(title="b1", author=a1)
        b2 = await FxBook.objects.create(title="b2", author=a2.id)
        b1.author = a2
        await b1.save()
        loaded = await FxBook.objects.get(id=b1.id)
        by_obj = await FxBook.objects.filter(author=a2).order_by("id").values("title", "author")
        n = await FxBook.objects.filter(author=a2).update(editor=a1)
        excl = [b.title for b in await FxBook.objects.exclude(author=a1).order_by("id")]
        return a1, a2, b1, b2, loaded, by_obj, n, excl

    a1, a2, b1, b2, loaded, by_obj, n, excl = run(main())
    assert b1.author is a2 and b1.author_id == a2.id
    assert b2.author == a2.id and b2.author_id == a2.id
    assert loaded.author == loaded.author_id == a2.id
    assert by_obj == [{"title": "b1", "author": a2.id}, {"title": "b2", "author": a2.id}]
    assert n == 2
    assert excl == ["b1", "b2"]


@pg
def test_live_fk_to_bigserial_and_uuid(live):
    # (Binding a str into a UUID parameter is the value layer's business, so
    # only the bigserial reference is written through the ORM here.)
    async def main():
        big = await FxBigPk.objects.create(label="x")
        ref = await FxRefs.objects.create(big=big)
        cols = await columns(live, "fx_refs")
        return big, ref, cols

    big, ref, cols = run(main())
    assert cols["big_id"]["data_type"] == "bigint"
    assert cols["uid_id"]["data_type"] == "uuid"
    assert type(big.id) is int and ref.big_id == big.id and ref.big is big


@pg
def test_live_like_escape_and_null_safe_exclude(live):
    async def main():
        for t, s in (("50%_off", 1), ("5000 off", None), ("a\\b", 2)):
            await FxBook.objects.create(title=t, small=s)
        pct = [b.title for b in await FxBook.objects.filter(title__contains="%_")]
        bs = [b.title for b in await FxBook.objects.filter(title__contains="\\")]
        ex = sorted(b.title for b in await FxBook.objects.exclude(small=1))
        return pct, bs, ex

    pct, bs, ex = run(main())
    assert pct == ["50%_off"] and bs == ["a\\b"]
    assert ex == ["5000 off", "a\\b"]


@pg
def test_live_int_out_of_range_raises(live):
    with pytest.raises(Exception):
        run(FxBook.objects.filter(small=2**31).count())


# ── 9. Ambient transactions ──────────────────────────────────────────────────

@pg
def test_live_ambient_transaction_commit_and_rollback(live):
    db = live

    async def main():
        assert current_transaction() is None
        async with db.transaction() as tx:
            assert current_transaction() is tx
            await FxAuthor.objects.create(name="kept")
            assert await FxAuthor.objects.using(db).count() == 0  # not yet committed
        assert current_transaction() is None
        with pytest.raises(KeyError):
            async with db.transaction():
                await FxAuthor.objects.create(name="gone")
                await FxAuthor.objects.filter(name="kept").update(score=99)
                raise KeyError
        return await FxAuthor.objects.all().values("name", "score")

    assert run(main()) == [{"name": "kept", "score": 5}]


@pg
def test_live_ambient_transaction_inherited_by_tasks(live):
    db = live

    async def child(name):
        assert current_transaction() is not None
        await FxAuthor.objects.create(name=name)

    async def main():
        with pytest.raises(RuntimeError, match="boom"):
            async with db.transaction():
                await asyncio.gather(child("t1"), child("t2"))
                await asyncio.create_task(child("t3"))
                assert await FxAuthor.objects.count() == 3
                raise RuntimeError("boom")
        return await FxAuthor.objects.count()

    assert run(main()) == 0


@pg
def test_live_ambient_transaction_isolated_between_tasks(live):
    db = live
    inside = asyncio.Event()
    release = asyncio.Event()

    async def in_tx():
        with pytest.raises(ValueError):
            async with db.transaction():
                await FxAuthor.objects.create(name="tx-only")
                inside.set()
                await release.wait()
                raise ValueError

    async def outside():
        await inside.wait()
        assert current_transaction() is None
        n = await FxAuthor.objects.count()          # pool, not the other task's tx
        await FxAuthor.objects.create(name="pool")  # committed independently
        release.set()
        return n

    async def main():
        _, n = await asyncio.gather(in_tx(), outside())
        return n, [a.name for a in await FxAuthor.objects.all()]

    assert run(main()) == (0, ["pool"])


@pg
def test_live_transactional_decorator_is_ambient(live):
    class Req:
        database = live

    @transactional
    async def handler(request, tx):
        assert current_transaction() is tx
        await FxAuthor.objects.create(name="h")
        return await FxAuthor.objects.count()

    assert run(handler(Req())) == 1
    assert run(FxAuthor.objects.count()) == 1


@pg
def test_live_swallowed_error_raises_transaction_aborted(live):
    db = live

    async def main():
        with pytest.raises(TransactionAborted) as ei:
            async with db.transaction() as tx:
                await FxAuthor.objects.create(name="lost")
                try:
                    await tx.execute("SELECT 1/0")
                except RuntimeError:
                    pass
        assert isinstance(ei.value, RuntimeError)
        assert "division by zero" in str(ei.value.__cause__)
        return await FxAuthor.objects.count(), current_transaction()

    assert run(main()) == (0, None)


@pg
def test_live_recovered_savepoint_still_commits(live):
    db = live

    async def main():
        async with db.transaction() as tx:
            await FxAuthor.objects.create(name="ok")
            await tx.execute("SAVEPOINT sp")
            try:
                await tx.execute("SELECT 1/0")
            except RuntimeError:
                await tx.execute("ROLLBACK TO SAVEPOINT sp")
        return [a.name for a in await FxAuthor.objects.all()]

    assert run(main()) == ["ok"]


@pg
def test_live_python_side_error_does_not_abort(live):
    db = live

    async def main():
        async with db.transaction() as tx:
            await FxAuthor.objects.create(name="ok")
            try:
                await tx.execute("SELECT $1::text", object())  # rejected before the server
            except Exception:
                pass
        return await FxAuthor.objects.count()

    assert run(main()) == 1
