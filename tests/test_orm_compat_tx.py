"""
ORM compatibility suite — transactions (``async with db.transaction()`` and
``@transactional``) interacting with the ORM.

See ``tests/orm_compat_support.py`` for conventions (``# PINNED-QUIRK:``).
"""

import pytest

from cello import transactional
from cello.database import TransactionAborted
from cello.orm import AutoField, CharField, IntegerField, Model

from orm_compat_support import bind, call, drop_tables, make_db, pg, run

pytestmark = pg

TABLE = "oc_tx_acct"


class Acct(Model):
    id = AutoField()
    owner = CharField(max_length=20, unique=True)
    bal = IntegerField(default=0)

    class Meta:
        table_name = TABLE


class FakeRequest:
    def __init__(self, database):
        self.database = database


@pytest.fixture(scope="module")
def db():
    database = make_db(pool_size=4)
    yield database
    run(drop_tables(database, TABLE))
    call(database.close)


@pytest.fixture
def acct(db):
    bind(db)

    async def setup():
        await drop_tables(db, TABLE)
        await Acct.create_table()
        await Acct.objects.create(owner="a", bal=100)
        await Acct.objects.create(owner="b", bal=100)

    run(setup())
    yield db


async def _balances():
    return {a.owner: a.bal for a in await Acct.objects.order_by("id")}


# ── async with db.transaction() ──────────────────────────────────────────────

def test_orm_inside_transaction_commits(acct):
    db = acct

    async def main():
        async with db.transaction() as tx:
            created = await Acct.objects.create(owner="c", bal=5, using=tx)
            await Acct(owner="d", bal=6).save(using=tx)
            n = await Acct.objects.using(tx).filter(owner="a").update(bal=70)
            m = await Acct.objects.using(tx).filter(owner="b").delete()
            inside_count = await Acct.objects.using(tx).count()
            inside_get = await Acct.objects.using(tx).get(owner="c")
            inside_first = await Acct.objects.using(tx).order_by("-id").first()
            inside_exists = await Acct.objects.using(tx).filter(owner="d").exists()
            inside_values = await Acct.objects.using(tx).order_by("id").values("owner")
            # Not yet visible on other pooled connections. (1.4.0: without
            # using() the ORM would run inside the transaction, so the pool is
            # named explicitly.)
            outside_count = await Acct.objects.using(db).count()
            outside_bal = (await Acct.objects.using(db).get(owner="a")).bal
        return (created.id, n, m, inside_count, inside_get.bal, inside_first.owner,
                inside_exists, inside_values, outside_count, outside_bal)

    (cid, n, m, inside_count, get_bal, first_owner, exists, values,
     outside_count, outside_bal) = run(main())
    assert cid == 3 and n == 1 and m == 1
    assert inside_count == 3 and get_bal == 5 and first_owner == "d" and exists is True
    assert values == [{"owner": "a"}, {"owner": "c"}, {"owner": "d"}]
    assert outside_count == 2 and outside_bal == 100
    assert run(_balances()) == {"a": 70, "c": 5, "d": 6}


def test_orm_inside_transaction_rolls_back_on_exception(acct):
    db = acct

    async def main():
        with pytest.raises(ValueError, match="boom"):
            async with db.transaction() as tx:
                await Acct.objects.create(owner="c", bal=5, using=tx)
                await Acct.objects.using(tx).filter(owner="a").update(bal=0)
                raise ValueError("boom")
        return await _balances()

    assert run(main()) == {"a": 100, "b": 100}


def test_orm_without_using_is_outside_the_transaction(acct):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "ORM calls without using(tx)
    # run outside the transaction"): inside the block they run in it and roll
    # back with it; an explicit using(db) still targets the pool.
    db = acct

    async def main():
        with pytest.raises(ValueError):
            async with db.transaction() as tx:
                await Acct.objects.create(owner="tx", using=tx)
                await Acct.objects.create(owner="inside")
                assert await Acct.objects.count() == 4
                assert await Acct.objects.using(db).count() == 2
                raise ValueError
        return sorted(a.owner for a in await Acct.objects.all())

    assert run(main()) == ["a", "b"]


def test_db_error_inside_transaction_aborts_it(acct):
    db = acct

    async def main():
        with pytest.raises(RuntimeError):
            async with db.transaction() as tx:
                await Acct.objects.create(owner="c", using=tx)
                await Acct.objects.create(owner="a", using=tx)  # unique violation
        return await _balances()

    assert run(main()) == {"a": 100, "b": 100}


def test_swallowed_db_error_then_normal_exit_silently_rolls_back(acct):
    # 1.4.0 (deliberate break; was PINNED-QUIRK "the normal-exit COMMIT silently
    # becomes a rollback"): exiting the block raises TransactionAborted.
    db = acct

    async def main():
        errors = 0
        with pytest.raises(TransactionAborted) as ei:
            async with db.transaction() as tx:
                await Acct.objects.create(owner="c", using=tx)
                try:
                    await Acct.objects.create(owner="a", using=tx)
                except RuntimeError:
                    errors += 1
                try:
                    await Acct.objects.using(tx).count()
                except RuntimeError:
                    errors += 1
        assert isinstance(ei.value, RuntimeError)
        assert "rolled back" in str(ei.value)
        assert isinstance(ei.value.__cause__, RuntimeError)  # the swallowed error
        return errors, await _balances()

    errors, balances = run(main())
    assert errors == 2
    assert balances == {"a": 100, "b": 100}


def test_transaction_not_active_outside_block(acct):
    db = acct

    async def main():
        t = db.transaction()
        with pytest.raises(RuntimeError, match="transaction is not active"):
            await Acct.objects.using(t).count()
        async with t as tx:
            await Acct.objects.using(tx).count()
        with pytest.raises(RuntimeError, match="transaction is not active"):
            await Acct.objects.create(owner="late", using=tx)
        return await Acct.objects.count()

    assert run(main()) == 2


def test_nested_transactions_are_independent(acct):
    # Each transaction() checks out its own connection; there are no savepoints,
    # so an inner block commits independently of the outer one.
    db = acct

    async def main():
        with pytest.raises(ValueError):
            async with db.transaction() as outer:
                await Acct.objects.using(outer).filter(owner="a").update(bal=1)
                async with db.transaction() as inner:
                    await Acct.objects.using(inner).filter(owner="b").update(bal=2)
                raise ValueError
        return await _balances()

    assert run(main()) == {"a": 100, "b": 2}


# ── @transactional ───────────────────────────────────────────────────────────

def test_transactional_commits_and_injects_tx(acct):
    seen = {}

    @transactional
    async def handler(request, amount, tx):
        seen["tx_type"] = type(tx).__name__
        await Acct.objects.using(tx).filter(owner="a").update(bal=100 - amount)
        await Acct.objects.using(tx).filter(owner="b").update(bal=100 + amount)
        return {"ok": True}

    assert handler.__name__ == "handler"
    assert run(handler(FakeRequest(acct), 30)) == {"ok": True}
    assert seen["tx_type"] == "Transaction"
    assert run(_balances()) == {"a": 70, "b": 130}


def test_transactional_rolls_back_orm_writes(acct):
    @transactional
    async def handler(request, tx):
        await Acct.objects.using(tx).filter(owner="a").update(bal=0)
        await Acct.objects.create(owner="c", using=tx)
        # 1.4.0 (deliberate break; was PINNED-QUIRK "not routed through tx ->
        # survives the rollback"): runs in the handler's transaction.
        await Acct.objects.create(owner="leak")
        raise KeyError("fail")

    with pytest.raises(KeyError):
        run(handler(FakeRequest(acct)))
    assert run(_balances()) == {"a": 100, "b": 100}


def test_transactional_requires_async_and_database(acct):
    with pytest.raises(TypeError):
        @transactional
        def sync_handler(request, tx):
            return None

    @transactional
    async def handler(request, tx):
        return 1

    with pytest.raises(RuntimeError, match="needs a database"):
        run(handler(object()))
