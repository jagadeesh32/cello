"""
Shared plumbing for the ORM compatibility suite (``tests/test_orm_compat_*.py``).

The compatibility suite pins the *observable* behaviour of the v1.3.0 ORM
(``python/cello/orm.py``) and of the raw ``db.fetch*`` value decoding
(``src/db/value.rs``) so the v1.4.0 Rust schema registry / query compiler can be
checked against it. See ``docs/releases/v1.4.0-plan.md`` -> "Compatibility
contract" and "How this gets verified": these tests must pass **unchanged**
against the new engine. A test that needs editing is a break that has to be
justified in the plan first.

Conventions used across the suite:

* Tests are plain ``def`` functions that drive coroutines through ``run()`` on a
  single persistent event loop, so they behave identically with or without
  ``--asyncio-mode=auto``.
* ``# PINNED-QUIRK:`` marks a test (or assertion) that pins behaviour which looks
  like a bug. It is pinned so the rewrite cannot change it *silently*; whether
  the rewrite keeps it is a team decision recorded in the plan.
* Tests only use declared fields and the foreign-key ``<name>_id`` column as
  identifiers. Undeclared-identifier and injection behaviour is covered
  elsewhere (it is the one deliberate break in 1.4.0).
* Exceptions raised by the database itself are pinned as ``RuntimeError`` (the
  current native layer maps every driver / server error to ``RuntimeError``).

Configure via env (same defaults as ``tests/test_native_db.py``)::

    CELLO_TEST_DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/cello_test
"""

import asyncio
import os

import pytest

from cello import App, DatabaseConfig
from cello import orm

DSN = os.environ.get(
    "CELLO_TEST_DATABASE_URL",
    "postgresql://postgres:postgres@127.0.0.1:5432/cello_test",
)

_LOOP = asyncio.new_event_loop()


def run(coro):
    """Run a coroutine to completion on the suite's persistent loop."""
    return _LOOP.run_until_complete(coro)


def make_db(pool_size: int = 3):
    app = App()
    app.enable_database(DatabaseConfig(url=DSN, pool_size=pool_size))
    return app.database


def _pg_available() -> bool:
    async def check():
        db = make_db(pool_size=1)
        await db.ping()
        await db.close()

    try:
        run(check())
        return True
    except Exception:
        return False


PG_AVAILABLE = _pg_available()

pg = pytest.mark.skipif(not PG_AVAILABLE, reason="PostgreSQL not reachable")


def bind(db) -> None:
    """Bind the ORM default database (other test modules rebind it too)."""
    orm.setup(db)


async def drop_tables(db, *names: str) -> None:
    for name in names:
        await db.execute(f'DROP TABLE IF EXISTS "{name}" CASCADE')


async def columns(db, table: str) -> dict:
    """information_schema view of a table: name -> column description."""
    rows = await db.fetch(
        "SELECT column_name::text AS column_name, data_type::text AS data_type, "
        "character_maximum_length::int AS character_maximum_length, "
        "is_nullable::text AS is_nullable, column_default::text AS column_default, "
        "ordinal_position::int AS ordinal_position "
        "FROM information_schema.columns WHERE table_name::text = $1 "
        "ORDER BY ordinal_position",
        table,
    )
    return {r["column_name"]: r for r in rows}


async def column_order(db, table: str) -> list:
    return list((await columns(db, table)).keys())


async def constraints(db, table: str) -> list:
    """Sorted list of (constraint_type, column_name) for a table."""
    rows = await db.fetch(
        "SELECT tc.constraint_type::text AS constraint_type, "
        "kcu.column_name::text AS column_name "
        "FROM information_schema.table_constraints tc "
        "JOIN information_schema.key_column_usage kcu "
        "  ON tc.constraint_name = kcu.constraint_name "
        " AND tc.table_name = kcu.table_name "
        "WHERE tc.table_name::text = $1",
        table,
    )
    return sorted((r["constraint_type"], r["column_name"]) for r in rows)


async def fk_rules(db, table: str) -> list:
    """Sorted list of (column, referenced_table, referenced_column, delete_rule)."""
    rows = await db.fetch(
        "SELECT kcu.column_name::text AS column_name, ccu.table_name::text AS ref_table, "
        "       ccu.column_name::text AS ref_column, rc.delete_rule::text AS delete_rule "
        "FROM information_schema.referential_constraints rc "
        "JOIN information_schema.key_column_usage kcu "
        "  ON rc.constraint_name = kcu.constraint_name "
        "JOIN information_schema.constraint_column_usage ccu "
        "  ON rc.unique_constraint_name = ccu.constraint_name "
        "WHERE kcu.table_name::text = $1",
        table,
    )
    return sorted(
        (r["column_name"], r["ref_table"], r["ref_column"], r["delete_rule"]) for r in rows
    )


async def table_exists(db, table: str) -> bool:
    return bool(
        await db.fetchval(
            "SELECT count(*) FROM information_schema.tables WHERE table_name::text = $1",
            table,
        )
    )


def call(fn, *args):
    """Call a native awaitable-returning method (``db.fetch`` ...) inside the loop.

    The native methods create their future eagerly and need a running loop, so
    they must be invoked from inside a coroutine rather than passed to ``run``.
    """
    async def _inner():
        return await fn(*args)

    return run(_inner())
