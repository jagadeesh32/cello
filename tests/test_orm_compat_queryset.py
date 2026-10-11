"""
ORM compatibility suite — QuerySet / Manager behaviour.

Every method and every lookup in ``_OPERATORS`` is exercised against a fixed
dataset, asserting *results* (not SQL text, which the 1.4.0 query compiler will
legitimately change). See ``tests/orm_compat_support.py`` for conventions.
"""

import asyncio

import pytest

from cello.orm import (
    AutoField,
    BooleanField,
    CharField,
    FloatField,
    ForeignKey,
    FieldError,
    IntegerField,
    Model,
    QuerySet,
)

from orm_compat_support import bind, call, drop_tables, make_db, pg, run

pytestmark = pg


class Owner(Model):
    id = AutoField()
    name = CharField(max_length=20)

    class Meta:
        table_name = "oc_qs_owner"


class Item(Model):
    id = AutoField()
    name = CharField(max_length=50)
    category = CharField(max_length=20, null=True)
    qty = IntegerField(null=True)
    price = FloatField(null=True)
    active = BooleanField(null=True)
    owner = ForeignKey(Owner, null=True)
    code = CharField(max_length=10, null=True, db_column="item_code")

    class Meta:
        table_name = "oc_qs_item"


# id: (name, category, qty, price, active, owner_id, code)
DATA = {
    1: ("apple", "fruit", 10, 1.5, True, 1, "A1"),
    2: ("Banana", "fruit", 5, 0.25, False, 1, "B2"),
    3: ("cherry", "fruit", None, 3.0, True, 2, None),
    4: ("carrot", "veg", 7, None, None, None, "C4"),
    5: ("Apricot", "fruit", 0, 2.0, True, 2, None),
    6: ("", None, -3, 0.0, False, None, ""),
    7: ("日本茶", "drink", 2**31 - 1, 1e10, True, 1, None),
    8: ("50%_off", "promo", 1, 0.1, None, None, "P8"),
}
ALL = sorted(DATA)


@pytest.fixture(scope="module")
def db():
    database = make_db()
    yield database
    run(drop_tables(database, "oc_qs_item", "oc_qs_owner"))
    call(database.close)


@pytest.fixture
def data(db):
    bind(db)

    async def setup():
        await drop_tables(db, "oc_qs_item", "oc_qs_owner")
        await Owner.create_table()
        await Item.create_table()
        await Owner.objects.create(name="o1")
        await Owner.objects.create(name="o2")
        for i in ALL:
            name, cat, qty, price, active, owner_id, code = DATA[i]
            await Item.objects.create(
                name=name, category=cat, qty=qty, price=price, active=active,
                owner_id=owner_id, code=code,
            )

    run(setup())
    yield db


def ids(qs):
    """Sorted ids of an (unordered) queryset."""
    return sorted(i.id for i in run(_await(qs)))


def ordered(qs):
    return [i.id for i in run(_await(qs))]


async def _await(qs):
    return await qs


def test_dataset_sanity(data):
    rows = run(_await(Item.objects.order_by("id")))
    assert [(r.id, r.name, r.category, r.qty, r.price, r.active, r.owner_id, r.code)
            for r in rows] == [(i, *DATA[i]) for i in ALL]


# ── exact / multiple conditions / chaining ───────────────────────────────────

def test_filter_exact(data):
    assert ids(Item.objects.filter(name="apple")) == [1]
    assert ids(Item.objects.filter(name__exact="apple")) == [1]
    assert ids(Item.objects.filter(name="Apple")) == []  # case-sensitive
    assert ids(Item.objects.filter(category="fruit")) == [1, 2, 3, 5]
    assert ids(Item.objects.filter(qty=0)) == [5]
    assert ids(Item.objects.filter(price=0.1)) == [8]
    assert ids(Item.objects.filter(name="日本茶")) == [7]
    assert ids(Item.objects.filter(name="")) == [6]


def test_filter_multiple_kwargs_and_chaining_are_and(data):
    assert ids(Item.objects.filter(category="fruit", active=True)) == [1, 3, 5]
    assert ids(Item.objects.filter(category="fruit").filter(active=True)) == [1, 3, 5]
    assert ids(Item.objects.filter(category="fruit").filter(qty__gt=4).filter(active=False)) == [2]
    assert ids(Item.objects.filter()) == ALL
    assert ids(Item.objects.exclude()) == ALL


def test_filter_db_column_field(data):
    assert ids(Item.objects.filter(code="A1")) == [1]
    assert ids(Item.objects.filter(code__isnull=True)) == [3, 5, 7]
    assert ids(Item.objects.filter(code="")) == [6]
    got = run(Item.objects.get(code="P8"))
    assert got.code == "P8" and "code" in got.to_dict()


def test_filter_exact_none_matches_nothing(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "x = NULL matches nothing"):
    # filter(x=None) is IS NULL, exclude(x=None) is IS NOT NULL.
    assert ids(Item.objects.filter(qty=None)) == [3]
    assert ids(Item.objects.exclude(qty=None)) == [1, 2, 4, 5, 6, 7, 8]
    assert ids(Item.objects.filter(qty__exact=None)) == [3]
    assert ids(Item.objects.filter(qty__ne=None)) == [1, 2, 4, 5, 6, 7, 8]


# ── comparison lookups ───────────────────────────────────────────────────────

def test_ne(data):
    # SQL three-valued logic: NULL rows never match "!=".
    assert ids(Item.objects.filter(qty__ne=5)) == [1, 4, 5, 6, 7, 8]
    # 1.4.0 (deliberate break): exclude() keeps rows where the condition is
    # NULL (unknown), so the NULL-qty row 3 is included.
    assert ids(Item.objects.exclude(qty__ne=5)) == [2, 3]


def test_gt_gte_lt_lte(data):
    assert ids(Item.objects.filter(qty__gt=5)) == [1, 4, 7]
    assert ids(Item.objects.filter(qty__gte=5)) == [1, 2, 4, 7]
    assert ids(Item.objects.filter(qty__lt=1)) == [5, 6]
    assert ids(Item.objects.filter(qty__lte=1)) == [5, 6, 8]
    assert ids(Item.objects.filter(price__lt=0.3)) == [2, 6, 8]
    assert ids(Item.objects.filter(price__gte=2.0)) == [3, 5, 7]
    assert ids(Item.objects.filter(qty__gte=0, qty__lte=5)) == [2, 5, 8]
    assert ids(Item.objects.filter(qty__gt=-100).filter(qty__lt=100)) == [1, 2, 4, 5, 6, 8]


def test_negated_comparisons(data):
    # 1.4.0 (deliberate break): NULL rows are no longer dropped by exclude()
    # (qty is NULL for row 3, price for row 4).
    assert ids(Item.objects.exclude(qty__gt=5)) == [2, 3, 5, 6, 8]
    assert ids(Item.objects.exclude(qty__gte=5)) == [3, 5, 6, 8]
    assert ids(Item.objects.exclude(qty__lt=1)) == [1, 2, 3, 4, 7, 8]
    assert ids(Item.objects.exclude(qty__lte=1)) == [1, 2, 3, 4, 7]
    assert ids(Item.objects.exclude(price__lt=1)) == [1, 3, 4, 5, 7]


def test_large_int_filter_values(data):
    assert ids(Item.objects.filter(qty=2**31 - 1)) == [7]
    assert ids(Item.objects.filter(qty=-(2**31))) == []
    # 1.4.0 (deliberate break; was PINNED-QUIRK "out-of-range value wraps"):
    # a value outside the column's integer range raises instead of wrapping.
    with pytest.raises(Exception):
        ids(Item.objects.filter(qty__gt=2**31))
    with pytest.raises(Exception):
        ids(Item.objects.filter(qty__in=[1, -(2**31) - 1]))


# ── pattern lookups ──────────────────────────────────────────────────────────

def test_contains_and_icontains(data):
    assert ids(Item.objects.filter(name__contains="an")) == [2]
    assert ids(Item.objects.filter(name__contains="ap")) == [1]
    assert ids(Item.objects.filter(name__icontains="AP")) == [1, 5]
    assert ids(Item.objects.filter(name__icontains="banana")) == [2]
    assert ids(Item.objects.filter(name__contains="本")) == [7]
    assert ids(Item.objects.exclude(name__icontains="a")) == [3, 6, 7, 8]
    assert ids(Item.objects.exclude(name__contains="a")) == [3, 5, 6, 7, 8]


def test_startswith_istartswith_endswith(data):
    assert ids(Item.objects.filter(name__startswith="ca")) == [4]
    assert ids(Item.objects.filter(name__startswith="a")) == [1]
    assert ids(Item.objects.filter(name__istartswith="a")) == [1, 5]
    assert ids(Item.objects.filter(name__istartswith="B")) == [2]
    assert ids(Item.objects.filter(name__endswith="y")) == [3]
    assert ids(Item.objects.filter(name__endswith="T")) == []  # endswith is case-sensitive
    assert ids(Item.objects.exclude(name__startswith="c")) == [1, 2, 5, 6, 7, 8]
    assert ids(Item.objects.exclude(name__endswith="t")) == [1, 2, 3, 6, 7, 8]


def test_pattern_wildcards_are_not_escaped(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "'%' and '_' are wildcards"):
    # '%', '_' and '\\' in the value match literally.
    assert ids(Item.objects.filter(name__contains="%")) == [8]
    assert ids(Item.objects.filter(name__contains="_")) == [8]
    assert ids(Item.objects.filter(name__startswith="_")) == []
    assert ids(Item.objects.filter(name__endswith="_off")) == [8]
    assert ids(Item.objects.filter(name__contains="a_r")) == []
    assert ids(Item.objects.filter(name__icontains="C_R")) == []
    assert ids(Item.objects.filter(name__contains="%_")) == [8]
    assert ids(Item.objects.filter(name__contains="\\")) == []


def test_pattern_with_empty_string(data):
    assert ids(Item.objects.filter(name__contains="")) == ALL
    assert ids(Item.objects.filter(category__contains="")) == [1, 2, 3, 4, 5, 7, 8]


# ── in ───────────────────────────────────────────────────────────────────────

def test_in(data):
    assert ids(Item.objects.filter(id__in=[1, 3, 99])) == [1, 3]
    assert ids(Item.objects.filter(id__in=(2,))) == [2]
    assert ids(Item.objects.filter(id__in={4, 5})) == [4, 5]
    assert ids(Item.objects.filter(name__in=["apple", "日本茶", "nope"])) == [1, 7]
    assert ids(Item.objects.filter(id__in=list(range(1, 500)))) == ALL
    # NULL inside the list never matches.
    assert ids(Item.objects.filter(qty__in=[None, 5])) == [2]


def test_in_empty(data):
    assert ids(Item.objects.filter(id__in=[])) == []
    assert ids(Item.objects.filter(id__in=())) == []
    assert ids(Item.objects.exclude(id__in=[])) == ALL
    assert ids(Item.objects.filter(category="fruit").exclude(id__in=[])) == [1, 2, 3, 5]


def test_not_in(data):
    assert ids(Item.objects.exclude(id__in=[1, 2, 3])) == [4, 5, 6, 7, 8]
    # 1.4.0 (deliberate break; was "NOT IN drops NULL rows"): row 6 (NULL
    # category) is kept.
    assert ids(Item.objects.exclude(category__in=["fruit"])) == [4, 6, 7, 8]


def test_in_with_string_iterates_characters(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "a str is iterated per
    # character"): str / bytes values for __in raise TypeError.
    for value in ("fruit", "", b"fruit"):
        with pytest.raises(TypeError, match="iterable of values"):
            Item.objects.filter(category__in=value)


def test_in_with_generator_raises(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "generators are rejected"):
    # any iterable is materialized once, when the query is built.
    qs = Item.objects.filter(id__in=(i for i in [1, 3]))
    assert ids(qs) == [1, 3]
    assert ids(qs) == [1, 3]  # re-running does not re-consume the generator
    assert ids(Item.objects.filter(id__in=range(2, 4))) == [2, 3]


# ── isnull ───────────────────────────────────────────────────────────────────

def test_isnull(data):
    assert ids(Item.objects.filter(qty__isnull=True)) == [3]
    assert ids(Item.objects.filter(qty__isnull=False)) == [1, 2, 4, 5, 6, 7, 8]
    assert ids(Item.objects.exclude(qty__isnull=True)) == [1, 2, 4, 5, 6, 7, 8]
    assert ids(Item.objects.exclude(qty__isnull=False)) == [3]
    assert ids(Item.objects.filter(active__isnull=True)) == [4, 8]
    assert ids(Item.objects.filter(name__isnull=True)) == []


def test_isnull_non_bool_values(data):
    # 1 / 0 compare equal to True / False.
    assert ids(Item.objects.filter(qty__isnull=1)) == [3]
    assert ids(Item.objects.filter(qty__isnull=0)) == [1, 2, 4, 5, 6, 7, 8]
    # Other values use truthiness...
    assert ids(Item.objects.filter(qty__isnull="yes")) == [3]
    assert ids(Item.objects.filter(qty__isnull="")) == [1, 2, 4, 5, 6, 7, 8]
    # PINNED-QUIRK: ...so isnull=None means IS NOT NULL.
    assert ids(Item.objects.filter(qty__isnull=None)) == [1, 2, 4, 5, 6, 7, 8]


# ── booleans, empty strings, NULL ─────────────────────────────────────────────

def test_boolean_filters(data):
    assert ids(Item.objects.filter(active=True)) == [1, 3, 5, 7]
    assert ids(Item.objects.filter(active=False)) == [2, 6]
    # 1.4.0 (deliberate break; was PINNED-QUIRK "exclude() drops NULL rows"):
    # NULL rows (4, 8) are kept by exclude().
    assert ids(Item.objects.exclude(active=True)) == [2, 4, 6, 8]
    assert ids(Item.objects.exclude(active=False)) == [1, 3, 4, 5, 7, 8]
    assert ids(Item.objects.filter(active__ne=True)) == [2, 6]
    assert ids(Item.objects.filter(active__in=[True, False])) == [1, 2, 3, 5, 6, 7]


def test_empty_string_vs_null(data):
    assert ids(Item.objects.filter(category__isnull=True)) == [6]
    assert ids(Item.objects.filter(code="")) == [6]
    assert ids(Item.objects.filter(code__isnull=True)) == [3, 5, 7]
    assert ids(Item.objects.exclude(name="")) == [1, 2, 3, 4, 5, 7, 8]
    got = run(Item.objects.get(id=6))
    assert got.name == "" and got.category is None and got.code == ""


def test_exclude_drops_null_rows(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "NULL rows are excluded too"):
    # row 6 (NULL category) is kept.
    assert ids(Item.objects.exclude(category="fruit")) == [4, 6, 7, 8]


def test_exclude_multiple_kwargs_is_and_of_negations(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "NOT(a) AND NOT(b)"):
    # exclude(a=1, b=2) means NOT (a=1 AND b=2); chained excludes still AND.
    assert ids(Item.objects.exclude(category="fruit", qty=10)) == [2, 3, 4, 5, 6, 7, 8]
    assert ids(Item.objects.exclude(category="fruit", qty=7)) == ALL
    assert ids(Item.objects.exclude(category="fruit").exclude(qty=7)) == [6, 7, 8]


def test_filter_and_exclude_combined(data):
    assert ids(Item.objects.filter(category="fruit").exclude(active=False)) == [1, 3, 5]
    assert ids(Item.objects.exclude(active=False).filter(category="fruit")) == [1, 3, 5]


@pytest.mark.parametrize(
    "kwargs",
    [{"qty": True}, {"name": 5}, {"active": 1}, {"qty": 1.5}],
    ids=["int-col-bool", "text-col-int", "bool-col-int", "int-col-float"],
)
def test_filter_type_mismatch_raises_runtime_error(data, kwargs):
    with pytest.raises(RuntimeError):
        ids(Item.objects.filter(**kwargs))


# ── ForeignKey filtering ─────────────────────────────────────────────────────

def test_fk_filter_by_id(data):
    o1 = run(Owner.objects.get(name="o1"))
    assert ids(Item.objects.filter(owner=o1.id)) == [1, 2, 7]
    assert ids(Item.objects.filter(owner_id=1)) == [1, 2, 7]
    assert ids(Item.objects.filter(owner__in=[1, 2])) == [1, 2, 3, 5, 7]
    assert ids(Item.objects.filter(owner_id__in=[2])) == [3, 5]
    assert ids(Item.objects.filter(owner__isnull=True)) == [4, 6, 8]
    assert ids(Item.objects.filter(owner_id__isnull=False)) == [1, 2, 3, 5, 7]
    # 1.4.0 (deliberate break): exclude() keeps the NULL-owner rows 4, 6, 8.
    assert ids(Item.objects.exclude(owner=1)) == [3, 4, 5, 6, 8]
    assert ids(Item.objects.filter(owner__gt=1)) == [3, 5]


def test_fk_filter_by_object_raises(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "filtering by an instance raises
    # TypeError"): a related instance is replaced by its primary key.
    o1 = run(Owner.objects.get(name="o1"))
    o2 = run(Owner.objects.get(name="o2"))
    assert ids(Item.objects.filter(owner=o1)) == [1, 2, 7]
    assert ids(Item.objects.filter(owner_id=o2)) == [3, 5]
    assert ids(Item.objects.filter(owner__in=[o1, o2])) == [1, 2, 3, 5, 7]
    assert ids(Item.objects.exclude(owner=o1)) == [3, 4, 5, 6, 8]
    with pytest.raises(ValueError, match="no primary key"):
        Item.objects.filter(owner=Owner(name="unsaved"))


# ── order_by ─────────────────────────────────────────────────────────────────

def test_order_by_asc_desc_nulls(data):
    # Postgres defaults: NULLs sort LAST ascending and FIRST descending.
    assert ordered(Item.objects.order_by("qty")) == [6, 5, 8, 2, 4, 1, 7, 3]
    assert ordered(Item.objects.order_by("-qty")) == [3, 7, 1, 4, 2, 8, 5, 6]
    assert ordered(Item.objects.order_by("price")) == [6, 8, 2, 1, 5, 3, 7, 4]
    assert ordered(Item.objects.order_by("-id")) == [8, 7, 6, 5, 4, 3, 2, 1]
    assert ordered(Item.objects.order_by("id")) == ALL


def test_order_by_multiple_and_fk_and_db_column(data):
    assert ordered(Item.objects.order_by("category", "-price")) == [7, 3, 5, 1, 2, 8, 4, 6]
    assert ordered(Item.objects.order_by("-owner", "id")) == [4, 6, 8, 3, 5, 1, 2, 7]
    assert ordered(Item.objects.order_by("owner_id", "-id")) == [7, 2, 1, 5, 3, 8, 6, 4]
    assert ordered(Item.objects.order_by("code", "id")) == [6, 1, 2, 4, 8, 3, 5, 7]
    assert ordered(Item.objects.order_by("active", "id")) == [2, 6, 1, 3, 5, 7, 4, 8]


def test_order_by_chained_appends(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "a second order_by() appends"):
    # a second order_by() replaces the ordering; order_by() clears it.
    qs = Item.objects.order_by("category").order_by("-id")
    assert ordered(qs) == [8, 7, 6, 5, 4, 3, 2, 1]
    assert ordered(Item.objects.order_by("-id").order_by("category", "id")) == [7, 1, 2, 3, 5, 8, 4, 6]
    assert Item.objects.order_by("id").order_by()._order == []


def test_order_by_with_filter(data):
    assert ordered(Item.objects.filter(category="fruit").order_by("-price")) == [3, 5, 1, 2]
    assert ordered(Item.objects.order_by("-price").filter(category="fruit")) == [3, 5, 1, 2]


# ── limit / offset ───────────────────────────────────────────────────────────

def test_limit_offset(data):
    base = Item.objects.order_by("id")
    assert ordered(base.limit(3)) == [1, 2, 3]
    assert ordered(base.offset(5)) == [6, 7, 8]
    assert ordered(base.offset(2).limit(2)) == [3, 4]
    assert ordered(base.limit(2).offset(2)) == [3, 4]
    assert ordered(base.limit(0)) == []
    assert ordered(base.offset(100)) == []
    assert ordered(base.offset(0)) == ALL
    assert ordered(base.limit(100)) == ALL
    assert ordered(base.limit(5).limit(2)) == [1, 2]  # last call wins
    assert ordered(base.offset(5).offset(1)) == [2, 3, 4, 5, 6, 7, 8]
    assert ordered(base.limit(2).limit(None)) == ALL  # None clears
    assert ordered(base.limit("2")) == [1, 2]  # coerced with int()
    assert ordered(base.limit(2.9)) == [1, 2]


def test_negative_limit_raises(data):
    with pytest.raises(RuntimeError):
        ordered(Item.objects.order_by("id").limit(-1))


# ── terminal operations ──────────────────────────────────────────────────────

def test_all_and_await(data):
    qs = Item.objects.filter(category="veg")
    assert isinstance(Item.objects.all(), QuerySet)
    assert isinstance(qs, QuerySet)
    a = run(qs.all())
    b = run(_await(qs))
    assert type(a) is list and type(b) is list
    assert [x.id for x in a] == [x.id for x in b] == [4]


def test_await_twice_reexecutes(data):
    qs = Item.objects.filter(category="veg")

    async def main():
        first = await qs
        await Item.objects.create(name="kale", category="veg")
        second = await qs
        return len(first), len(second)

    assert run(main()) == (1, 2)


def test_await_with_gather(data):
    async def main():
        return await asyncio.gather(
            Item.objects.filter(category="fruit"),
            Item.objects.filter(category="veg"),
            Item.objects.filter(category="fruit").count(),
        )

    fruit, veg, n = run(main())
    assert sorted(i.id for i in fruit) == [1, 2, 3, 5]
    assert [i.id for i in veg] == [4]
    assert n == 4


def test_count(data):
    assert run(Item.objects.count()) == 8
    assert run(Item.objects.all().count()) == 8
    assert run(Item.objects.filter(category="fruit").count()) == 4
    assert run(Item.objects.filter(id__in=[]).count()) == 0
    assert type(run(Item.objects.count())) is int


def test_count_exists_ignore_limit_offset(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "count()/exists() ignore
    # limit/offset"): both count the sliced result.
    assert run(Item.objects.order_by("id").limit(2).count()) == 2
    assert run(Item.objects.all().offset(100).count()) == 0
    assert run(Item.objects.all().offset(6).count()) == 2
    assert run(Item.objects.filter(category="fruit").limit(10).count()) == 4
    assert run(Item.objects.all().limit(0).exists()) is False
    assert run(Item.objects.all().offset(7).exists()) is True
    assert run(Item.objects.all().offset(8).exists()) is False


def test_exists(data):
    assert run(Item.objects.filter(name="apple").exists()) is True
    assert run(Item.objects.filter(name="nope").exists()) is False
    assert run(Item.objects.all().exists()) is True


def test_first(data):
    assert run(Item.objects.order_by("-qty").first()).id == 3
    assert run(Item.objects.order_by("id").offset(2).first()).id == 3
    assert run(Item.objects.order_by("id").limit(5).first()).id == 1
    assert run(Item.objects.filter(name="nope").first()) is None
    f = run(Item.objects.filter(name="apple").first())
    assert type(f) is Item and f.name == "apple"


def test_get(data):
    assert run(Item.objects.get(id=1)).name == "apple"
    assert run(Item.objects.filter(category="veg").get()).id == 4
    assert run(Item.objects.filter(category="fruit").get(qty=5)).id == 2
    assert run(Item.objects.all().get(name="日本茶")).id == 7
    with pytest.raises(Item.MultipleObjectsReturned):
        run(Item.objects.filter(category="fruit").get())
    with pytest.raises(Item.DoesNotExist):
        run(Item.objects.filter(category="fruit").get(qty=7))


def test_get_overrides_user_limit(data):
    # PINNED-QUIRK: get() applies its own LIMIT 2, replacing a user limit().
    with pytest.raises(Item.MultipleObjectsReturned):
        run(Item.objects.order_by("id").limit(1).get())
    # offset is respected.
    assert run(Item.objects.order_by("id").offset(7).get()).id == 8


def test_values(data):
    v = run(Item.objects.filter(id__in=[1, 6]).order_by("id").values())
    assert v == [
        {"id": 1, "name": "apple", "category": "fruit", "qty": 10, "price": 1.5,
         "active": True, "owner_id": 1, "code": "A1"},
        {"id": 6, "name": "", "category": None, "qty": -3, "price": 0.0,
         "active": False, "owner_id": None, "code": ""},
    ]
    assert all(type(d) is dict for d in v)
    sub = run(Item.objects.order_by("-id").limit(2).values("qty", "name"))
    assert sub == [{"qty": 1, "name": "50%_off"}, {"qty": 2**31 - 1, "name": "日本茶"}]
    assert [list(d) for d in sub] == [["qty", "name"], ["qty", "name"]]
    assert run(Item.objects.filter(id__in=[]).values()) == []


def test_values_fk_and_unknown_keys(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "FK name and unknown keys yield
    # None"): an FK can be named either way and gives the id; unknown names
    # raise FieldError.
    v = run(Item.objects.filter(id=1).values("owner", "owner_id", "code", "item_code"))
    assert v == [{"owner": 1, "owner_id": 1, "code": "A1", "item_code": "A1"}]
    with pytest.raises(FieldError, match="'nope'"):
        run(Item.objects.filter(id=1).values("owner", "nope"))


# ── update ───────────────────────────────────────────────────────────────────

def test_update(data):
    assert run(Item.objects.filter(category="fruit").update(qty=0)) == 4
    assert ids(Item.objects.filter(qty=0)) == [1, 2, 3, 5]
    n = run(Item.objects.filter(category="veg", qty=7).update(price=9.5, active=True, code="Z"))
    assert n == 1
    got = run(Item.objects.get(id=4))
    assert (got.price, got.active, got.code) == (9.5, True, "Z")
    assert run(Item.objects.filter(name="nope").update(qty=1)) == 0
    assert type(run(Item.objects.filter(id=1).update(qty=1))) is int


def test_update_all_rows_and_null(data):
    assert run(Item.objects.all().update(category=None)) == 8
    assert run(Item.objects.filter(category__isnull=True).count()) == 8


def test_update_fk(data):
    assert run(Item.objects.filter(id=4).update(owner=2)) == 1
    assert run(Item.objects.get(id=4)).owner_id == 2
    assert run(Item.objects.filter(id=4).update(owner_id=None)) == 1
    assert run(Item.objects.get(id=4)).owner_id is None
    # 1.4.0 (deliberate break; was TypeError): a related instance is stored as
    # its primary key.
    assert run(Item.objects.filter(id=4).update(owner=run(Owner.objects.get(id=1)))) == 1
    assert run(Item.objects.get(id=4)).owner_id == 1


def test_update_with_multiple_where_params(data):
    qs = Item.objects.filter(category="fruit", qty__gte=5).exclude(name__in=["apple", "x"])
    assert run(qs.update(name="upd", price=0.5)) == 1
    assert run(Item.objects.get(name="upd")).id == 2


def test_update_without_kwargs_raises(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "SQL error"): update() with
    # nothing to set is a no-op returning 0.
    assert run(Item.objects.filter(id=1).update()) == 0
    assert run(Item.objects.all().update()) == 0
    assert run(Item.objects.get(id=1)).qty == 10


def test_update_and_delete_ignore_limit(data):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "limit/offset ignored, every
    # matching row affected"): a sliced queryset cannot be updated or deleted.
    with pytest.raises(TypeError, match="cannot update a sliced queryset"):
        run(Item.objects.filter(category="fruit").order_by("id").limit(1).update(qty=42))
    with pytest.raises(TypeError, match="cannot delete a sliced queryset"):
        run(Item.objects.filter(category="fruit").limit(1).offset(1).delete())
    with pytest.raises(TypeError):
        run(Item.objects.all().offset(0).delete())
    assert run(Item.objects.count()) == 8
    # order_by alone is still allowed (and ignored).
    assert run(Item.objects.filter(category="fruit").order_by("id").update(qty=42)) == 4


# ── delete ───────────────────────────────────────────────────────────────────

def test_delete(data):
    assert run(Item.objects.filter(owner__isnull=True).delete()) == 3
    assert ids(Item.objects.all()) == [1, 2, 3, 5, 7]
    assert run(Item.objects.filter(name="nope").delete()) == 0
    assert run(Item.objects.all().delete()) == 5
    assert run(Item.objects.count()) == 0


# ── immutability / cloning ───────────────────────────────────────────────────

def test_chaining_does_not_mutate_original(data):
    base = Item.objects.filter(category="fruit")
    before = ids(base)
    derived = [
        base.filter(qty__gt=1),
        base.exclude(active=True),
        base.order_by("-id"),
        base.limit(1),
        base.offset(3),
        base.using(data),
    ]
    assert ids(derived[0]) == [1, 2]
    assert ids(derived[1]) == [2]
    assert ordered(derived[2]) == [5, 3, 2, 1]
    assert len(run(_await(derived[3]))) == 1
    assert len(run(_await(derived[4]))) == 1
    assert ids(derived[5]) == before
    assert ids(base) == before == [1, 2, 3, 5]
    assert run(base.count()) == 4
    # Terminal ops on a derived queryset don't leak into the base either.
    run(base.first())
    run(base.get(qty=10))
    assert ids(base) == [1, 2, 3, 5]
    ord_base = Item.objects.order_by("id")
    ord_base.order_by("-qty")
    assert ordered(ord_base) == ALL


def test_manager_returns_fresh_querysets(data):
    a = Item.objects.filter(category="fruit")
    b = Item.objects.all()
    assert a is not b
    assert ids(b) == ALL
    assert Item.objects.all() is not Item.objects.all()
    assert Item.objects.all().model is Item
    assert Item.objects.model is Item


def test_using_none_falls_back_to_default(data):
    assert ids(Item.objects.using(None).filter(category="veg")) == [4]
    assert ids(Item.objects.filter(category="veg").using(None)) == [4]
