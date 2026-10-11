"""
ORM Phase 04 (1.4.0): Q objects, aggregates, relations (select_related /
prefetch_related / reverse accessors), bulk operations and the public
``db.fetch_json`` raw fast path.

SQL-shape checks use a recording stand-in pool and run everywhere; behaviour
checks run against live Postgres (tables prefixed ``adv_``) and are skipped
when it is unreachable.
"""

import json

import pytest

from cello import RawJSON
from cello import orm
from cello.orm import (
    AutoField,
    Avg,
    BigIntegerField,
    BooleanField,
    CharField,
    Count,
    DateTimeField,
    FieldError,
    FloatField,
    ForeignKey,
    IntegerField,
    JSONField,
    Max,
    Min,
    Model,
    Q,
    Sum,
    TextField,
)

from orm_compat_support import bind, drop_tables, make_db, pg, run


# ── Models ────────────────────────────────────────────────────────────────────

class AdvPublisher(Model):
    id = AutoField()
    name = CharField(max_length=40)

    class Meta:
        table_name = "adv_publisher"


class AdvAuthor(Model):
    id = AutoField()
    name = CharField(max_length=40)
    publisher = ForeignKey(AdvPublisher, null=True, related_name="authors")

    class Meta:
        table_name = "adv_author"


class AdvBook(Model):
    id = AutoField()
    title = CharField(max_length=10)
    pages = IntegerField(null=True)
    big = BigIntegerField(null=True)
    price = FloatField(null=True)
    category = CharField(max_length=20, null=True, db_column="cat")
    active = BooleanField(default=True)
    meta = JSONField(null=True)
    author = ForeignKey(AdvAuthor, null=True)
    editor = ForeignKey(AdvAuthor, null=True, on_delete="SET NULL", related_name="edited")
    created = DateTimeField(auto_now_add=True)
    touched = DateTimeField(auto_now=True)

    class Meta:
        table_name = "adv_book"


class AdvTag(Model):
    code = CharField(max_length=10, primary_key=True)
    label = TextField(null=True)
    weight = IntegerField(default=1)

    class Meta:
        table_name = "adv_tag"


class AdvFromRow(Model):
    """Overrides _from_row: relations take the Python (prefetch-style) path."""
    id = AutoField()
    author = ForeignKey(AdvAuthor, null=True, related_name="from_rows")

    class Meta:
        table_name = "adv_fromrow"

    @classmethod
    def _from_row(cls, row):
        obj = super()._from_row(row)
        obj.loaded_by = "python"
        return obj


TABLES = ("adv_fromrow", "adv_book", "adv_author", "adv_publisher", "adv_tag")


class RecordingDB:
    """Records SQL; returns empty results."""

    def __init__(self):
        self.calls = []

    async def fetch(self, sql, *params):
        self.calls.append((sql, params))
        return []

    async def fetchrow(self, sql, *params):
        self.calls.append((sql, params))
        return None

    async def execute(self, sql, *params):
        self.calls.append((sql, params))
        return 0


class Counting:
    """Forwards to a real database and counts statements by method."""

    def __init__(self, db):
        self._db = db
        self.calls = []

    def transaction(self):
        outer = self
        manager = self._db.transaction()

        class _Tx:
            async def __aenter__(self):
                tx = await manager.__aenter__()
                inner = Counting(tx)
                inner.calls = outer.calls
                return inner

            async def __aexit__(self, *exc):
                return await manager.__aexit__(*exc)

        return _Tx()

    def __getattr__(self, name):
        target = getattr(self._db, name)
        if name in ("execute", "fetch", "fetchrow", "fetchval", "_fetch_json",
                    "_fetch_models", "_fetch_related", "fetch_json"):
            def call(*args):
                self.calls.append((name, args))
                return target(*args)
            return call
        return target


class PlainDB:
    """A database exposing only the public query methods (no native hooks)."""

    def __init__(self, db):
        self._db = db
        self.calls = []

    async def fetch(self, sql, *params):
        self.calls.append(sql)
        return await self._db.fetch(sql, *params)

    async def fetchrow(self, sql, *params):
        self.calls.append(sql)
        return await self._db.fetchrow(sql, *params)

    async def execute(self, sql, *params):
        self.calls.append(sql)
        return await self._db.execute(sql, *params)


@pytest.fixture(scope="module")
def db():
    database = make_db(pool_size=4)

    async def setup():
        await drop_tables(database, *TABLES)
        for m in (AdvPublisher, AdvAuthor, AdvBook, AdvTag, AdvFromRow):
            await m.create_table(using=database)

    async def teardown():
        await drop_tables(database, *TABLES)

    run(setup())
    yield database
    run(teardown())


@pytest.fixture
def data(db):
    """Fresh rows: 2 publishers, 3 authors, 10 books."""
    bind(db)

    async def go():
        await db.execute("TRUNCATE adv_fromrow, adv_book, adv_author, adv_publisher, "
                         "adv_tag RESTART IDENTITY CASCADE")
        p1 = await AdvPublisher.objects.create(name="P1")
        p2 = await AdvPublisher.objects.create(name="P2")
        a1 = await AdvAuthor.objects.create(name="Ann", publisher=p1)
        a2 = await AdvAuthor.objects.create(name="Bob", publisher=p2)
        a3 = await AdvAuthor.objects.create(name="Cid")
        books = []
        for i in range(10):
            books.append(await AdvBook.objects.create(
                title=f"b{i}",
                pages=None if i % 4 == 0 else i * 10,
                big=i,
                price=i * 1.5,
                category="sci" if i < 6 else "art",
                author=[a1, a2, None][i % 3],
                editor=a3 if i == 2 else None,
                meta={"i": i},
            ))
        return {"p": [p1, p2], "a": [a1, a2, a3], "b": books}

    return run(go())


def ids(objs):
    return [o.id for o in objs]


# ── Q objects: SQL shape ──────────────────────────────────────────────────────

def _where(qs):
    return qs._select_sql()


def test_q_negation_is_exactly_exclude():
    rec = RecordingDB()
    base = AdvBook.objects.using(rec)
    assert _where(base.filter(~Q(pages=5))) == _where(base.exclude(pages=5))
    assert _where(base.filter(~Q(pages=5, title="x"))) == _where(base.exclude(pages=5, title="x"))
    assert _where(base.exclude(Q(pages=5))) == _where(base.exclude(pages=5))
    # ~~q behaves as q in a WHERE clause.
    assert _where(base.filter(~~Q(pages=5))) == _where(base.filter(pages=5))


def test_q_or_and_nesting_sql():
    rec = RecordingDB()
    qs = AdvBook.objects.using(rec).filter(
        (Q(pages__gt=10) | Q(title__startswith="a_")) & ~Q(author=None), active=True
    )
    sql, params = qs._select_sql()
    assert sql == (
        'SELECT * FROM "adv_book" WHERE (("pages" > $1 OR "title" LIKE $2 ESCAPE \'\\\') '
        'AND ("author_id" IS NULL) IS NOT TRUE) AND "active" = $3'
    )
    assert params == [10, "a\\_%", True]


def test_q_empty_and_errors():
    rec = RecordingDB()
    base = AdvBook.objects.using(rec)
    assert _where(base.filter(Q())) == _where(base)
    assert _where(base.filter(Q() | Q(pages=1))) == _where(base.filter(pages=1))
    assert _where(base.exclude(Q())) == _where(base)
    assert not Q() and Q(a=1)
    with pytest.raises(TypeError):
        base.filter({"pages": 1})
    with pytest.raises(TypeError):
        Q("pages")
    with pytest.raises(TypeError):
        Q(pages=1) | {"a": 1}
    # Identifiers inside Q go through the registry.
    with pytest.raises(FieldError, match="no declared field or column 'nickname'"):
        base.filter(Q(pages=1) | Q(nickname="x"))
    with pytest.raises(FieldError, match="Invalid field name"):
        base.filter(Q(**{'x" OR 1=1 --': 1}))
    with pytest.raises(FieldError):
        base.exclude(~Q(title__regex="x"))
    with pytest.raises(ValueError, match="out of range"):
        base.filter(Q(pages=2**40))
    assert rec.calls == []


# ── Q objects: live NULL semantics ───────────────────────────────────────────

@pg
def test_q_live_semantics(data):
    async def go():
        B = AdvBook.objects
        out = {}
        out["or"] = ids(await B.filter(Q(pages__gt=70) | Q(pages__isnull=True)).order_by("id"))
        out["not"] = ids(await B.filter(~Q(pages__gte=30)).order_by("id"))
        out["exclude"] = ids(await B.exclude(pages__gte=30).order_by("id"))
        out["and_kw"] = ids(await B.filter(Q(category="sci") | Q(pages=90), author=data["a"][0]).order_by("id"))
        out["nested"] = ids(await B.filter(~(Q(category="sci") & ~Q(pages=None))).order_by("id"))
        out["get"] = (await B.get(Q(title="b3") | Q(title="zz"))).title
        out["exclude_q_kw"] = ids(await B.exclude(Q(category="art"), pages__gt=70).order_by("id"))
        return out

    out = run(go())
    # pages: b0 NULL, b1 10, b2 20, b3 30, b4 NULL, b5 50, b6 60, b7 70, b8 NULL, b9 90
    assert out["or"] == [1, 5, 9, 10]
    # NOT(pages >= 30) keeps NULL rows, exactly like exclude().
    assert out["not"] == out["exclude"] == [1, 2, 3, 5, 9]
    # author a1 = books 0, 3, 6, 9
    assert out["and_kw"] == [1, 4, 10]
    # NOT(sci AND pages IS NOT NULL): art rows + sci rows with NULL pages
    assert out["nested"] == [1, 5, 7, 8, 9, 10]
    assert out["get"] == "b3"
    # NOT(art AND pages > 70): keeps everything but b9
    assert out["exclude_q_kw"] == list(range(1, 10))


# ── Aggregates ────────────────────────────────────────────────────────────────

def test_aggregate_alias_and_field_validation():
    rec = RecordingDB()
    qs = AdvBook.objects.using(rec)
    for bad in ["a b", "1x", 'n" FROM pg_user --', "", "näme"]:
        with pytest.raises(FieldError, match="Invalid alias"):
            run(qs.aggregate(**{bad: Count("id")}))
        with pytest.raises(FieldError, match="Invalid alias"):
            qs.values("category").annotate(**{bad: Count("id")})
    with pytest.raises(FieldError, match="no declared field or column 'nope'"):
        run(qs.aggregate(Sum("nope")))
    with pytest.raises(FieldError, match="Invalid field name"):
        run(qs.aggregate(Sum('price") FROM x --')))
    with pytest.raises(FieldError):
        Sum("*")
    with pytest.raises(FieldError):
        Count("*", distinct=True)
    with pytest.raises(FieldError):
        Sum(3)
    with pytest.raises(TypeError):
        run(qs.aggregate(total="price"))
    with pytest.raises(TypeError, match="needs the fields to group by"):
        qs.values().annotate(n=Count("id"))
    with pytest.raises(ValueError, match="already a key"):
        qs.values("category").annotate(category=Count("id"))
    with pytest.raises(FieldError):
        qs.values("category").annotate(n=Count("nope"))
    assert rec.calls == []
    assert run(qs.aggregate()) == {}


def test_aggregate_sql_shape():
    rec = RecordingDB()

    async def go():
        try:
            await AdvBook.objects.using(rec).filter(active=True).aggregate(
                Sum("price"), n=Count("author", distinct=True), c=Count()
            )
        except AttributeError:  # recording fetchrow returns None
            pass
        await AdvBook.objects.using(rec).values("category").annotate(n=Count("id")).order_by("-n", "category")

    run(go())
    assert rec.calls[0] == (
        'SELECT sum("price") AS "price__sum", count(DISTINCT "author_id") AS "n", '
        'count(*) AS "c" FROM "adv_book" WHERE "active" = $1',
        (True,),
    )
    assert rec.calls[1][0] == (
        'SELECT "cat" AS "category", count("id") AS "n" FROM "adv_book" '
        'GROUP BY "cat" ORDER BY "n" DESC, "cat" ASC'
    )


@pg
def test_aggregate_values_and_shapes(data):
    async def go():
        B = AdvBook.objects
        full = await B.aggregate(
            Count(), n=Count("pages"), d=Count("author", distinct=True),
            s_int=Sum("pages"), s_big=Sum("big"), s_float=Sum("price"),
            avg_int=Avg("pages"), avg_float=Avg("price"),
            mn=Min("title"), mx=Max("pages"),
        )
        empty = await B.filter(id=-1).aggregate(c=Count("id"), s=Sum("pages"), a=Avg("price"), m=Max("title"))
        sliced = await B.order_by("-id").limit(3).aggregate(s=Sum("big"))
        filtered = await B.filter(Q(category="art")).aggregate(c=Count())
        return full, empty, sliced, filtered

    full, empty, sliced, filtered = run(go())
    assert full == {
        "count": 10, "n": 7, "d": 2,
        "s_int": 330,                 # sum(integer) -> bigint -> int
        "s_big": "45",                # sum(bigint) -> numeric -> decimal string
        "s_float": 67.5,              # sum(double) -> float
        "avg_int": full["avg_int"],
        "avg_float": 6.75,
        "mn": "b0", "mx": 90,
    }
    assert isinstance(full["avg_int"], str) and float(full["avg_int"]) == pytest.approx(330 / 7)
    assert empty == {"c": 0, "s": None, "a": None, "m": None}
    assert sliced == {"s": "24"}      # 9 + 8 + 7
    assert filtered == {"c": 4}


@pg
def test_annotate_group_by(data):
    async def go():
        B = AdvBook.objects
        rows = await B.values("category").annotate(n=Count("id"), pages=Sum("pages")).order_by("-n", "category")
        by_author = await B.filter(author__isnull=False).values("author").annotate(
            n=Count("id"), top=Max("pages")).order_by("author")
        limited = await B.values("category").annotate(n=Count()).order_by("n").limit(1)
        groups = await B.values("category").annotate(n=Count()).count()
        first = await B.values("category").annotate(n=Count()).order_by("-n").first()
        qs = B.values("category").annotate(n=Count("id")).order_by("category")
        native = await qs.json()
        encoded = RawJSON.encode(await qs)
        return rows, by_author, limited, groups, first, native, encoded

    rows, by_author, limited, groups, first, native, encoded = run(go())
    assert rows == [{"category": "sci", "n": 6, "pages": 110}, {"category": "art", "n": 4, "pages": 220}]
    assert list(rows[0]) == ["category", "n", "pages"]
    assert by_author == [{"author": 1, "n": 4, "top": 90}, {"author": 2, "n": 3, "top": 70}]
    assert limited == [{"category": "art", "n": 4}]
    assert groups == 2
    assert first == {"category": "sci", "n": 6}
    assert bytes(native) == bytes(encoded) == b'[{"category":"art","n":4},{"category":"sci","n":6}]'


@pg
def test_values_queryset_compat_and_json_parity(data):
    async def go():
        B = AdvBook.objects
        out = []
        for qs in (
            B.order_by("id").values("title", "author", "author_id", "category"),
            B.filter(id__in=[1, 2]).order_by("id").values(),
            B.filter(id=-1).values("title"),
            B.order_by("-id").limit(2).values("meta", "price", "created"),
        ):
            rows = await qs
            out.append((rows, bytes(await qs.json()), bytes(RawJSON.encode(rows)),
                        bytes((await orm._run_querysets([qs]))[0])))
        return out

    for rows, native, encoded, returned in run(go()):
        assert native == encoded == returned
    assert run(go())[0][0][0] == {"title": "b0", "author": 1, "author_id": 1, "category": "sci"}


# ── Relations ─────────────────────────────────────────────────────────────────

def test_relation_validation():
    rec = RecordingDB()
    B = AdvBook.objects.using(rec)
    with pytest.raises(FieldError, match="has no ForeignKey 'title'"):
        B.select_related("title")
    with pytest.raises(FieldError, match="reverse accessor; use prefetch_related"):
        AdvAuthor.objects.select_related("advbook_set")
    with pytest.raises(FieldError, match="has no ForeignKey 'nope'"):
        B.select_related("author__nope")
    with pytest.raises(FieldError, match="no ForeignKey or reverse accessor"):
        AdvAuthor.objects.prefetch_related("books")
    with pytest.raises(TypeError):
        B.select_related()
    with pytest.raises(FieldError):
        B.select_related(1)
    assert B.select_related("author").select_related(None)._related == ()
    assert B.select_related("author", "author").select_related("author__publisher")._related == (
        "author", "author__publisher")

    class AdvSameCol(Model):
        id = AutoField()
        author = ForeignKey(AdvAuthor, db_column="author", related_name="same_cols")

        class Meta:
            table_name = "adv_samecol"

    with pytest.raises(FieldError, match="stores its id under its own name"):
        AdvSameCol.objects.select_related("author")
    with pytest.raises(FieldError, match="stores its id under its own name"):
        AdvSameCol.objects.prefetch_related("author")
    assert rec.calls == []


def test_joined_sql_shape():
    qs = AdvBook.objects.using(RecordingDB()).filter(active=True).select_related(
        "author__publisher", "editor").order_by("-id").limit(5)
    sql, params = qs._joined_sql(qs._join_plan())
    assert sql == (
        'SELECT "_c0".*, NULL AS "__cello_join__", "_c1".*, NULL AS "__cello_join__", "_c2".*, '
        'NULL AS "__cello_join__", "_c3".* FROM (SELECT * FROM "adv_book" WHERE "active" = $1 '
        'ORDER BY "id" DESC LIMIT 5) AS "_c0" '
        'LEFT JOIN "adv_author" AS "_c1" ON "_c1"."id" = "_c0"."author_id" '
        'LEFT JOIN "adv_publisher" AS "_c2" ON "_c2"."id" = "_c1"."publisher_id" '
        'LEFT JOIN "adv_author" AS "_c3" ON "_c3"."id" = "_c0"."editor_id" '
        'ORDER BY "_c0"."id" DESC'
    )
    assert params == [True]


@pg
def test_select_related_native(data, db):
    counting = Counting(db)

    async def go():
        return await AdvBook.objects.using(counting).select_related(
            "author__publisher", "editor").order_by("id")

    books = run(go())
    assert [c[0] for c in counting.calls] == ["_fetch_related"]
    assert len(books) == 10
    b0, b1, b2 = books[0], books[1], books[2]
    assert isinstance(b0.author, AdvAuthor) and b0.author.name == "Ann"
    assert b0.author_id == b0.author.id == 1
    assert isinstance(b0.author.publisher, AdvPublisher) and b0.author.publisher.name == "P1"
    assert b0.author.publisher_id == 1
    assert b1.author.name == "Bob" and b1.author.publisher.name == "P2"
    assert b2.author is None and b2.author_id is None        # NULL FK
    assert b2.editor.name == "Cid" and b2.editor.publisher is None  # NULL nested FK
    assert b0.editor is None
    # Columns of equal names in different tables never mix.
    assert b0.title == "b0" and b0.author.name == "Ann" and b0.id == 1
    # Instances behave like loaded ones.
    assert b0.to_dict() == run(AdvBook.objects.get(id=1)).to_dict()


@pg
def test_select_related_matches_fallbacks(data, db):
    def shape(books):
        return [(b.to_dict(), b.author and b.author.to_dict(),
                 b.author and b.author.publisher and b.author.publisher.to_dict())
                for b in books]

    plain = PlainDB(db)

    async def go():
        native = await AdvBook.objects.select_related("author__publisher").filter(
            Q(pages__gt=10) | Q(pages=None)).order_by("-id").limit(6)
        fallback = await AdvBook.objects.using(plain).select_related("author__publisher").filter(
            Q(pages__gt=10) | Q(pages=None)).order_by("-id").limit(6)
        return native, fallback

    native, fallback = run(go())
    assert shape(native) == shape(fallback)
    assert len(plain.calls) == 3  # base + one per relation

    async def from_row():
        await AdvFromRow.objects.create(author=data["a"][0])
        await AdvFromRow.objects.create(author=None)
        return await AdvFromRow.objects.select_related("author__publisher").order_by("id")

    rows = run(from_row())
    assert [r.loaded_by for r in rows] == ["python", "python"]
    assert rows[0].author.name == "Ann" and rows[0].author.publisher.name == "P1"
    assert rows[1].author is None


@pg
def test_select_related_json_is_byte_identical(data, db):
    counting = Counting(db)

    async def go():
        qs = AdvBook.objects.using(counting).select_related("author__publisher").order_by("id")
        native = bytes(await qs.json())
        returned = bytes((await orm._run_querysets([qs]))[0])
        python = bytes(RawJSON.encode([b.to_dict() for b in await qs]))
        plain = bytes(await AdvBook.objects.order_by("id").json())
        return native, returned, python, plain

    native, returned, python, plain = run(go())
    assert native == returned == python == plain
    # The JSON paths skip the join: it does not change to_dict().
    assert [c[0] for c in counting.calls] == ["_fetch_json", "_fetch_json", "_fetch_related"]


@pg
def test_select_related_in_transaction_and_get(data, db):
    async def go():
        async with db.transaction():
            await AdvAuthor.objects.filter(id=1).update(name="Ann2")
            inside = await AdvBook.objects.select_related("author").get(id=1)
            first = await AdvBook.objects.select_related("author").order_by("id").first()
        return inside, first

    inside, first = run(go())
    assert inside.author.name == "Ann2" and first.author.name == "Ann2"


@pg
def test_reverse_accessors(data):
    a1, a2, a3 = data["a"]

    async def go():
        return (
            ids(await a1.advbook_set.order_by("id")),
            await a2.advbook_set.count(),
            ids(await a3.edited),
            ids(await data["p"][0].authors),
            ids(await a1.advbook_set.filter(pages__gt=30).order_by("id")),
        )

    assert run(go()) == ([1, 4, 7, 10], 3, [3], [1], [7, 10])
    assert isinstance(AdvAuthor.advbook_set, orm._ReverseAccessor)
    with pytest.raises(ValueError, match="no primary key"):
        AdvAuthor(name="x").advbook_set


def test_reverse_accessor_collisions():
    class AdvTarget(Model):
        id = AutoField()
        label = CharField()

        def helper(self):
            return 1

        class Meta:
            table_name = "adv_target"

    class AdvTwo(Model):
        id = AutoField()
        a = ForeignKey(AdvTarget)
        b = ForeignKey(AdvTarget)

        class Meta:
            table_name = "adv_two"

    with pytest.raises(AttributeError, match="ambiguous"):
        AdvTarget(id=1).advtwo_set
    with pytest.raises(FieldError, match="ambiguous"):
        AdvTarget.objects.prefetch_related("advtwo_set")
    for bad in ("label", "helper"):
        with pytest.raises(ValueError, match="clashes with"):
            class AdvBad(Model):
                id = AutoField()
                t = ForeignKey(AdvTarget, related_name=bad)

                class Meta:
                    table_name = "adv_bad"

    class AdvOne(Model):
        id = AutoField()
        t = ForeignKey(AdvTarget, related_name="ones")

        class Meta:
            table_name = "adv_one"

    with pytest.raises(ValueError, match="already used by AdvOne"):
        class AdvOther(Model):
            id = AutoField()
            t = ForeignKey(AdvTarget, related_name="ones")

            class Meta:
                table_name = "adv_other"

    for bad in ("1x", "_private", "a-b", 3):
        with pytest.raises(ValueError, match="related_name"):
            ForeignKey(AdvTarget, related_name=bad)
    # A default name clashing with an attribute is simply not installed.

    class AdvTarget2(Model):
        id = AutoField()
        advthing_set = CharField()

        class Meta:
            table_name = "adv_target2"

    class AdvThing(Model):
        id = AutoField()
        t = ForeignKey(AdvTarget2)

        class Meta:
            table_name = "adv_thing"

    assert "advthing_set" not in AdvTarget2._meta.reverse


def _redefine():
    class AdvRedef(Model):
        id = AutoField()
        author = ForeignKey(AdvAuthor, related_name="redefs")

        class Meta:
            table_name = "adv_redef"

    return AdvRedef


def test_redefined_model_replaces_its_accessor():
    first = _redefine()
    second = _redefine()
    assert AdvAuthor.redefs.source is second and first is not second


@pg
def test_prefetch_related(data, db):
    counting = Counting(db)

    async def go():
        authors = await AdvAuthor.objects.using(counting).prefetch_related(
            "advbook_set", "publisher", "edited").order_by("id")
        n_queries = len(counting.calls)
        books = {a.name: [b.id for b in await a.advbook_set] for a in authors}
        n_after = len(counting.calls)
        filtered = ids(await authors[0].advbook_set.filter(pages__gt=30))
        json_cached = bytes(await authors[0].advbook_set.json())
        return authors, n_queries, books, n_after, filtered, json_cached

    authors, n_queries, books, n_after, filtered, json_cached = run(go())
    assert n_queries == 4          # authors + one per relation
    assert n_after == n_queries    # cached: no query
    assert books == {"Ann": [1, 4, 7, 10], "Bob": [2, 5, 8], "Cid": []}
    assert authors[0].publisher.name == "P1" and authors[2].publisher is None
    assert [b.id for b in run(authors[2].edited.all())] == [3]
    # Children point back at the prefetched parent.
    assert run(authors[0].advbook_set.all())[0].author is authors[0]
    assert sorted(filtered) == [7, 10]
    expected = json.dumps([b.to_dict() for b in run(authors[0].advbook_set.all())],
                          separators=(",", ":"))
    assert json.loads(json_cached) == json.loads(expected)


@pg
def test_prefetch_nested_and_empty(data, db):
    counting = Counting(db)

    async def go():
        pubs = await AdvPublisher.objects.using(counting).prefetch_related(
            "authors__advbook_set").order_by("id")
        none = await AdvPublisher.objects.filter(id=-1).prefetch_related("authors__advbook_set")
        books = await AdvBook.objects.prefetch_related("author__publisher").order_by("id")
        return pubs, none, books, len(counting.calls)

    pubs, none, books, n = run(go())
    assert n == 3
    p1_authors = run(pubs[0].authors.all())
    assert [a.name for a in p1_authors] == ["Ann"]
    assert [b.id for b in run(p1_authors[0].advbook_set.all())] == [1, 4, 7, 10]
    assert none == []
    assert books[0].author.publisher.name == "P1" and books[2].author is None


# ── Bulk operations ──────────────────────────────────────────────────────────

@pg
def test_bulk_create(data, db):
    counting = Counting(db)

    async def go():
        objs = [AdvBook(title=f"n{i}", pages=i, author=data["a"][i % 2], meta={"k": [i]})
                for i in range(7)]
        out = await AdvBook.objects.bulk_create(objs, batch_size=3, using=counting)
        stored = await AdvBook.objects.filter(title__startswith="n").order_by("id")
        return objs, out, stored

    objs, out, stored = run(go())
    assert out is not objs and out == objs
    inserts = [c for c in counting.calls if c[0] == "fetch"]
    assert len(inserts) == 3       # 3 + 3 + 1, one statement per batch
    assert inserts[0][1][0].startswith('INSERT INTO "adv_book" ("title", "pages", "big", ')
    assert [o.id for o in objs] == list(range(11, 18))
    assert all(o.created and o.touched and o.active is True for o in objs)
    assert [o.to_dict() for o in objs] == [s.to_dict() for s in stored]
    assert objs[1].author_id == data["a"][1].id and objs[1].meta == {"k": [1]}


@pg
def test_bulk_create_mixed_pk_defaults_and_explicit_values(data):
    async def go():
        explicit = "2001-02-03T04:05:06+00:00"
        objs = [AdvBook(title="x1"), AdvBook(id=500, title="x2", created=explicit)]
        await AdvBook.objects.bulk_create(objs)
        tags = await AdvTag.objects.bulk_create([AdvTag(code="a"), AdvTag(code="b", weight=7)])
        return objs, tags, await AdvTag.objects.order_by("code").values("code", "weight")

    objs, tags, stored = run(go())
    assert objs[0].id == 11 and objs[1].id == 500
    assert objs[1].created.startswith("2001-02-03")
    assert stored == [{"code": "a", "weight": 1}, {"code": "b", "weight": 7}]
    assert run(AdvBook.objects.bulk_create([])) == []
    with pytest.raises(TypeError, match="AdvBook instances"):
        run(AdvBook.objects.bulk_create([AdvTag(code="z")]))
    with pytest.raises(ValueError, match="batch_size"):
        run(AdvBook.objects.bulk_create([AdvBook(title="q")], batch_size=0))


@pg
def test_bulk_create_respects_bind_parameter_limit(data, db):
    counting = Counting(db)
    n = 7000  # 10 parameters per row -> 70,000 > 65,535

    async def go():
        objs = [AdvBook(title=f"m{i}", pages=i) for i in range(n)]
        await AdvBook.objects.bulk_create(objs, using=counting)
        return objs, await AdvBook.objects.filter(title__startswith="m").count()

    objs, count = run(go())
    inserts = [c for c in counting.calls if c[0] == "fetch"]
    assert count == n and len(inserts) == 2
    assert all(len(c[1]) - 1 <= 65535 for c in inserts)
    assert len({o.id for o in objs}) == n
    assert orm._batches(list(range(10)), 65535 // 3 + 1, None) == [[i, i + 1] for i in range(0, 10, 2)]


@pg
def test_bulk_create_multi_batch_is_atomic(data):
    async def go():
        objs = [AdvTag(code=f"t{i}") for i in range(4)] + [AdvTag(code="t0")]
        with pytest.raises(Exception):
            await AdvTag.objects.bulk_create(objs, batch_size=2)
        return await AdvTag.objects.count()

    assert run(go()) == 0


@pg
def test_bulk_update(data, db):
    counting = Counting(db)
    books = data["b"]

    async def go():
        before = [b.touched for b in books]
        for i, b in enumerate(books):
            b.pages = i * 1000
            b.meta = {"new": i}
            b.author = data["a"][2] if i % 2 else None
            b.title = "SHOULD-NOT-WRITE"
        n = await AdvBook.objects.bulk_update(
            books, ["pages", "meta", "author_id"], batch_size=4, using=counting)
        stored = await AdvBook.objects.order_by("id")
        return before, n, stored

    before, n, stored = run(go())
    assert n == 10
    updates = [c for c in counting.calls if c[0] == "fetch"]
    assert len(updates) == 3
    assert '"pages" = "_v"."c1"' in updates[0][1][0] and "::INTEGER" in updates[0][1][0]
    assert [s.pages for s in stored] == [i * 1000 for i in range(10)]
    assert [s.meta for s in stored] == [{"new": i} for i in range(10)]
    assert [s.author_id for s in stored] == [None, 3] * 5
    assert all(s.title.startswith("b") for s in stored)
    # auto_now is written and read back, like save().
    assert all(b.touched != t for b, t in zip(data["b"], before))
    assert [b.touched for b in data["b"]] == [s.touched for s in stored]


@pg
def test_bulk_update_errors_and_limits(data):
    books = data["b"]
    with pytest.raises(FieldError, match="no declared field or column 'nope'"):
        run(AdvBook.objects.bulk_update(books, ["nope"]))
    with pytest.raises(FieldError, match="Invalid field name"):
        run(AdvBook.objects.bulk_update(books, ['pages" = 1 --']))
    with pytest.raises(FieldError, match="primary key"):
        run(AdvBook.objects.bulk_update(books, ["id"]))
    with pytest.raises(ValueError, match="at least one"):
        run(AdvBook.objects.bulk_update(books, []))
    with pytest.raises(TypeError):
        run(AdvBook.objects.bulk_update(books, "pages"))
    with pytest.raises(ValueError, match="more than once"):
        run(AdvBook.objects.bulk_update([books[0], books[0]], ["pages"]))
    with pytest.raises(ValueError, match="no primary key"):
        run(AdvBook.objects.bulk_update([AdvBook(title="x")], ["pages"]))
    assert run(AdvBook.objects.bulk_update([], ["pages"])) == 0
    # A too-long string is an error, not a silent truncation (VARCHAR(10)).
    books[0].title = "x" * 11
    with pytest.raises(Exception, match="too long"):
        run(AdvBook.objects.bulk_update([books[0]], ["title"]))
    # Non-integer primary key, field given by db_column name.

    async def tags():
        t = await AdvTag.objects.bulk_create([AdvTag(code="k1"), AdvTag(code="k2")])
        t[0].label, t[1].label = "one", None
        t[0].weight, t[1].weight = 5, 6
        n = await AdvTag.objects.bulk_update(t, ["label", "weight"])
        return n, await AdvTag.objects.order_by("code").values("label", "weight")

    assert run(tags()) == (2, [{"label": "one", "weight": 5}, {"label": None, "weight": 6}])
    books[1].category = "zzz"
    assert run(AdvBook.objects.bulk_update([books[1]], ["cat"])) == 1
    assert run(AdvBook.objects.get(id=2)).category == "zzz"


# ── Raw fast path: db.fetch_json ──────────────────────────────────────────────

@pg
def test_fetch_json_public(data, db):
    sql = "SELECT id, title, meta, price, NULL::text AS n, cat FROM adv_book WHERE id <= $1 ORDER BY id"

    async def go():
        raw = await db.fetch_json(sql, 3)
        rows = await db.fetch(sql, 3)
        empty = await db.fetch_json("SELECT 1 AS a WHERE false")
        async with db.transaction() as tx:
            await tx.execute("INSERT INTO adv_tag (code) VALUES ('inside')")
            inside = await tx.fetch_json("SELECT code FROM adv_tag WHERE code = $1", "inside")
            await tx.execute("ROLLBACK")  # end it early; aexit COMMIT is then a no-op
        return raw, rows, empty, inside

    raw, rows, empty, inside = run(go())
    assert isinstance(raw, RawJSON)
    assert bytes(raw) == bytes(RawJSON.encode(rows))
    assert bytes(empty) == b"[]"
    assert bytes(inside) == b'[{"code":"inside"}]'


@pg
def test_fetch_json_failure_marks_transaction(db):
    from cello import TransactionAborted

    async def go():
        async with db.transaction() as tx:
            try:
                await tx.fetch_json("SELECT * FROM adv_missing_table")
            except Exception:
                pass

    with pytest.raises(TransactionAborted):
        run(go())
