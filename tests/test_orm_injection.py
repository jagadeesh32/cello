"""
ORM identifier-injection guard (v1.3.1 / v1.4.0 Phase 00).

Values passed to ``filter``/``exclude``/``update`` are always parameterized, but
the *keys* (and ``order_by`` arguments) become SQL identifiers, which cannot be
parameterized. These tests pin the guard:

* identifiers outside ``^[A-Za-z_][A-Za-z0-9_]*$`` raise ``FieldError`` (a
  ``ValueError``) and never reach the database;
* declared fields and ForeignKey ``<name>_id`` columns work with no warning;
* an undeclared-but-valid identifier raises ``FieldError`` too (1.4.0; it was a
  ``DeprecationWarning`` in 1.3.1).

The "never reaches the database" checks use a recording stand-in for the pool,
so they run everywhere. The end-to-end checks use the live Postgres server and
are skipped when it is unreachable (same logic as ``tests/test_native_db.py``).
"""

import asyncio
import os
import warnings

import pytest

from cello import App, DatabaseConfig
from cello.orm import (
    AutoField,
    CharField,
    FieldError,
    ForeignKey,
    IntegerField,
    Model,
)

DSN = os.environ.get(
    "CELLO_TEST_DATABASE_URL",
    "postgresql://postgres:postgres@127.0.0.1:5432/cello_test",
)

PAYLOAD = 'x" = 1 OR "1'
BAD_IDENTIFIERS = [
    PAYLOAD,
    'name"',
    "name; DROP TABLE inj_author",
    "name--",
    "1name",
    "",
    "na me",
    "name\n",          # `$` would match before a trailing newline; fullmatch must not
    "näme",            # non-ASCII is rejected (ASCII-only identifier rule)
]


def _run(coro):
    loop = asyncio.new_event_loop()
    try:
        return loop.run_until_complete(coro)
    finally:
        loop.close()


def _pg_available():
    async def check():
        app = App()
        app.enable_database(DatabaseConfig(url=DSN, pool_size=1))
        await app.database.ping()
        await app.database.close()
    try:
        _run(check())
        return True
    except Exception:
        return False


pg = pytest.mark.skipif(not _pg_available(), reason="PostgreSQL not reachable")


class InjAuthor(Model):
    id = AutoField()
    name = CharField(max_length=100)
    age = IntegerField(null=True)

    class Meta:
        table_name = "inj_author"


class InjBook(Model):
    id = AutoField()
    title = CharField(max_length=100)
    author = ForeignKey(InjAuthor)
    code = CharField(max_length=20, null=True, db_column="book_code")

    class Meta:
        table_name = "inj_book"


class RecordingDB:
    """Stands in for the pool and records every SQL statement it is sent."""

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


# ── Rejection: bad identifiers never reach the database ──────────────────────

@pytest.mark.parametrize("bad", BAD_IDENTIFIERS)
def test_filter_rejects_bad_identifier(bad):
    db = RecordingDB()
    with pytest.raises(FieldError) as exc:
        InjAuthor.objects.using(db).filter(**{bad: 1})
    assert repr(bad) in str(exc.value)
    assert db.calls == []


@pytest.mark.parametrize("bad", BAD_IDENTIFIERS)
def test_exclude_rejects_bad_identifier(bad):
    db = RecordingDB()
    with pytest.raises(FieldError):
        InjAuthor.objects.using(db).exclude(**{bad: 1})
    assert db.calls == []


@pytest.mark.parametrize("bad", BAD_IDENTIFIERS)
def test_order_by_rejects_bad_identifier(bad):
    db = RecordingDB()
    with pytest.raises(FieldError):
        InjAuthor.objects.using(db).order_by(bad)
    with pytest.raises(FieldError):
        InjAuthor.objects.using(db).order_by("-" + bad)
    assert db.calls == []


@pytest.mark.parametrize("bad", BAD_IDENTIFIERS)
def test_update_rejects_bad_identifier(bad):
    db = RecordingDB()
    with pytest.raises(FieldError):
        _run(InjAuthor.objects.using(db).filter(id=1).update(**{bad: 1}))
    assert db.calls == []


@pytest.mark.parametrize(
    "key",
    [
        PAYLOAD + "__gt",             # payload before a valid lookup
        PAYLOAD + "__in",
        'name__" OR 1=1 --',          # payload as the "lookup" part
        "name__gt__" + PAYLOAD,
        "__gt",                       # empty field name before a lookup
    ],
)
def test_lookup_splitting_cannot_smuggle_identifiers(key):
    db = RecordingDB()
    with pytest.raises(FieldError):
        InjAuthor.objects.using(db).filter(**{key: 1})
    assert db.calls == []


def test_order_by_rejects_double_dash_and_non_strings():
    db = RecordingDB()
    with pytest.raises(FieldError):
        InjAuthor.objects.using(db).order_by("--name")
    with pytest.raises(FieldError):
        InjAuthor.objects.using(db).order_by(1)
    assert db.calls == []


def test_unsupported_lookup_on_declared_field_is_a_clear_error():
    with pytest.raises(FieldError, match="Unsupported lookup 'iexact' on field 'name'"):
        InjAuthor.objects.filter(name__iexact="a")


def test_field_error_is_a_value_error():
    assert issubclass(FieldError, ValueError)


def test_invalid_on_delete_is_rejected():
    with pytest.raises(ValueError):
        ForeignKey(InjAuthor, on_delete="CASCADE; DROP TABLE inj_author")
    assert ForeignKey(InjAuthor, on_delete="set null").on_delete == "SET NULL"


# ── Declared identifiers: no warnings, correct SQL ────────────────────────────

def _no_warnings(fn):
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        result = fn()
    assert [w for w in caught if issubclass(w.category, DeprecationWarning)] == []
    return result


def test_declared_fields_and_fk_columns_do_not_warn():
    db = RecordingDB()

    def build():
        qs = InjBook.objects.using(db).filter(
            title="t", author=1, author_id=1, code="c", book_code="c",
            title__icontains="x", author_id__in=[1, 2],
        ).exclude(id__gt=5).order_by("-author_id", "title", "-code")
        _run(qs.all())
        _run(qs.update(author_id=2, book_code="z", title="u"))

    _no_warnings(build)
    select_sql, update_sql = db.calls
    assert '"author_id" = $' in select_sql
    assert '"book_code" = $' in select_sql
    assert 'ORDER BY "author_id" DESC, "title" ASC, "book_code" DESC' in select_sql
    assert 'SET "author_id" = $1, "book_code" = $2, "title" = $3' in update_sql


def test_undeclared_valid_identifier_warns_but_works():
    # 1.4.0 (deliberate break; was a DeprecationWarning in 1.3.1): an undeclared
    # identifier is a FieldError that names it and points at the alternatives.
    db = RecordingDB()
    with pytest.raises(FieldError, match="no declared field or column 'nickname'") as ei:
        InjAuthor.objects.using(db).filter(nickname="n")
    msg = str(ei.value)
    assert "Declare the field on InjAuthor" in msg and "db.fetch" in msg
    with pytest.raises(FieldError, match="'nickname'"):
        InjAuthor.objects.using(db).exclude(nickname__icontains="n")
    with pytest.raises(FieldError, match="'nickname'"):
        InjAuthor.objects.using(db).order_by("-nickname")
    with pytest.raises(FieldError, match="'nickname'"):
        _run(InjAuthor.objects.using(db).update(nickname="n"))
    with pytest.raises(FieldError, match="'nickname'"):
        _run(InjAuthor.objects.using(db).values("name", "nickname"))
    assert db.calls == []


# ── Live database ─────────────────────────────────────────────────────────────

async def _fresh_db():
    app = App()
    app.enable_database(DatabaseConfig(url=DSN, pool_size=2))
    db = app.database
    await InjBook.drop_table(using=db)
    await InjAuthor.drop_table(using=db)
    await InjAuthor.create_table(using=db)
    await InjBook.create_table(using=db)
    return db


@pg
def test_live_payload_rejected_and_table_untouched():
    async def main():
        db = await _fresh_db()
        a = await InjAuthor.objects.create(using=db, name="ann", age=30)
        await InjAuthor.objects.create(using=db, name="bob", age=40)

        qs = InjAuthor.objects.using(db)
        with pytest.raises(FieldError):
            await qs.filter(**{PAYLOAD: 1}).all()
        with pytest.raises(FieldError):
            await qs.exclude(**{PAYLOAD: 1}).all()
        with pytest.raises(FieldError):
            await qs.order_by("-" + PAYLOAD).all()
        with pytest.raises(FieldError):
            await qs.filter(id=a.id).update(**{PAYLOAD: 1})
        with pytest.raises(FieldError):
            await qs.filter(**{PAYLOAD + "__gt": 0}).delete()

        rows = await qs.order_by("id").all()
        assert [(r.name, r.age) for r in rows] == [("ann", 30), ("bob", 40)]
        await InjBook.drop_table(using=db)
        await InjAuthor.drop_table(using=db)
        await db.close()

    _run(main())


@pg
def test_live_declared_and_fk_queries_without_warnings():
    async def main():
        db = await _fresh_db()
        ann = await InjAuthor.objects.create(using=db, name="ann", age=30)
        bob = await InjAuthor.objects.create(using=db, name="bob", age=40)
        await InjBook.objects.create(using=db, title="a1", author=ann, code="A")
        await InjBook.objects.create(using=db, title="b1", author_id=bob.id, code="B")

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always")
            books = InjBook.objects.using(db)
            by_fk_name = await books.filter(author=ann.id).all()
            by_fk_col = await books.filter(author_id=bob.id).all()
            by_db_column = await books.filter(book_code="B").all()
            ordered = await books.order_by("-author_id").all()
            n = await books.filter(author_id=ann.id).update(book_code="Z")
            older = await InjAuthor.objects.using(db).exclude(age__lt=35).all()
        assert [w for w in caught if issubclass(w.category, DeprecationWarning)] == []

        assert [b.title for b in by_fk_name] == ["a1"]
        assert [b.title for b in by_fk_col] == ["b1"]
        assert [b.title for b in by_db_column] == ["b1"]
        assert [b.title for b in ordered] == ["b1", "a1"]
        assert n == 1
        assert (await books.get(title="a1")).code == "Z"
        assert [a.name for a in older] == ["bob"]

        await InjBook.drop_table(using=db)
        await InjAuthor.drop_table(using=db)
        await db.close()

    _run(main())


@pg
def test_live_undeclared_column_warns_and_still_works():
    # 1.4.0 (deliberate break): the column exists in the table but is not
    # declared, so the ORM refuses it; raw SQL is the escape hatch.
    async def main():
        db = await _fresh_db()
        await db.execute('ALTER TABLE "inj_author" ADD COLUMN nickname TEXT')
        await InjAuthor.objects.create(using=db, name="ann", age=30)
        await db.execute("UPDATE inj_author SET nickname = 'annie'")

        with pytest.raises(FieldError, match="nickname"):
            await InjAuthor.objects.using(db).filter(nickname="annie").all()
        rows = await db.fetch("SELECT name FROM inj_author WHERE nickname = $1", "annie")
        assert [r["name"] for r in rows] == ["ann"]

        await InjBook.drop_table(using=db)
        await InjAuthor.drop_table(using=db)
        await db.close()

    _run(main())
