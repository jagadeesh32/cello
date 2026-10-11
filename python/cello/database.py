"""
Cello Database & Redis helpers.

Since v1.3.0 the database and Redis integrations are **native and real**: the
connection pools live in Rust (``deadpool-postgres`` and the ``redis`` crate) and
are reached through ``app.database`` / ``request.database`` and ``app.redis`` /
``request.redis``. The ``Database``, ``Transaction`` and ``Redis`` types are the
native classes exported from the compiled extension — import them from ``cello``
if you need them for type hints::

    from cello import App, DatabaseConfig, RedisConfig
    from cello.database import transactional

    app = App()
    app.enable_database(DatabaseConfig(url="postgresql://user:pass@localhost/db"))
    app.enable_redis(RedisConfig(url="redis://localhost:6379"))

    @app.on_event("startup")
    async def startup():
        await app.database.execute("CREATE TABLE IF NOT EXISTS users(id serial primary key, name text)")

    @app.get("/users")
    async def users(request):
        return {"users": await request.database.fetch("SELECT * FROM users")}

This module provides the :func:`transactional` decorator, the ambient
transaction used by the ORM (:func:`current_transaction`) and
:class:`TransactionAborted`; the query and Redis methods are documented on the
native ``Database`` / ``Redis`` classes.

Ambient transaction (1.4.0)
---------------------------
Inside ``async with db.transaction() as tx`` (and inside a ``@transactional``
handler) the open transaction is recorded in a :mod:`contextvars` variable. ORM
calls that do not pass ``using=`` run on it, so they commit or roll back with
the block. Tasks created inside the block inherit it; concurrent requests each
see only their own. An explicit ``using(db)`` always wins.

If a statement inside the block fails and the error is caught, Postgres has
already aborted the transaction and the ``COMMIT`` at the end of the block
would silently roll back. Leaving such a block normally now rolls back and
raises :class:`TransactionAborted` instead.
"""

import contextvars
from functools import wraps
from typing import Callable, Optional

from cello._cello import Transaction as _NativeTransaction

__all__ = ["transactional", "current_transaction", "TransactionAborted"]


class TransactionAborted(RuntimeError):
    """Raised when a transaction block exits normally after a statement in it
    failed (and the error was caught): Postgres aborted the transaction, so it
    was rolled back instead of committed."""


_current_tx: contextvars.ContextVar = contextvars.ContextVar(
    "cello_current_transaction", default=None
)


def current_transaction() -> Optional[_NativeTransaction]:
    """The transaction opened by the innermost enclosing ``async with
    db.transaction()`` / ``@transactional`` in this task, or ``None``."""
    return _current_tx.get()


class _TxState:
    __slots__ = ("token", "failed", "error", "objs")

    def __init__(self, token, objs):
        self.token = token
        self.failed = False
        self.error = None
        self.objs = objs  # keeps both ids below alive while registered


# id(manager) and id(the object __aenter__ returned) -> _TxState, while open.
# The native __aenter__ returns a second handle on the same connection, and
# statements may be sent through either one.
_TX_STATE: dict = {}


def _install_transaction_hooks() -> None:
    """Wrap the native ``Transaction`` so it (a) publishes itself as the
    ambient transaction and (b) notices a failed statement.

    Only Python-level wrapping: the native class keeps doing BEGIN / COMMIT /
    ROLLBACK and every query. Idempotent.
    """
    cls = _NativeTransaction
    if getattr(cls, "_cello_ambient_hooks", False):
        return
    orig_aenter = cls.__aenter__
    orig_aexit = cls.__aexit__
    orig_execute = cls.execute

    async def __aenter__(self):
        tx = await orig_aenter(self)
        state = _TxState(_current_tx.set(tx), (self, tx))
        _TX_STATE[id(self)] = state
        _TX_STATE[id(tx)] = state
        return tx

    async def __aexit__(self, exc_type, exc, tb):
        state = _TX_STATE.pop(id(self), None)
        if state is None:
            return await orig_aexit(self, exc_type, exc, tb)
        _TX_STATE.pop(id(state.objs[1]), None)
        try:
            _current_tx.reset(state.token)
        except ValueError:  # exited from another context; nothing to restore here
            pass
        if exc_type is None and state.failed:
            # A statement failed and the error was swallowed. If Postgres
            # aborted the transaction, COMMIT would silently roll back.
            try:
                await orig_execute(self, "SELECT 1")
            except Exception:
                await orig_aexit(self, TransactionAborted, None, None)  # ROLLBACK
                raise TransactionAborted(
                    "the transaction was rolled back: a statement inside "
                    "`async with db.transaction()` failed and the error was "
                    "caught, so Postgres aborted the transaction and nothing "
                    "in the block was committed. Let the error propagate, or "
                    "use a SAVEPOINT around statements that may fail."
                ) from state.error
        return await orig_aexit(self, exc_type, exc, tb)

    def tracking(orig):
        async def method(self, *args):
            try:
                return await orig(self, *args)
            except Exception as exc:
                state = _TX_STATE.get(id(self))
                if state is not None and not state.failed:
                    state.failed = True
                    state.error = exc
                raise

        method.__name__ = orig.__name__
        method.__qualname__ = f"Transaction.{orig.__name__}"
        method.__doc__ = orig.__doc__
        return method

    cls.__aenter__ = __aenter__
    cls.__aexit__ = __aexit__
    for name in (
        "execute", "fetch", "fetchrow", "fetchval", "executemany",
        "_fetch_json", "_fetch_models", "_fetch_related", "fetch_json",
    ):
        if hasattr(cls, name):
            setattr(cls, name, tracking(getattr(cls, name)))
    cls._cello_ambient_hooks = True


_install_transaction_hooks()


def transactional(func: Callable) -> Callable:
    """
    Run an async handler inside a database transaction.

    The handler must be ``async`` and is called with an extra ``tx`` keyword
    argument — a native transaction bound to a single pooled connection. The
    transaction commits when the handler returns normally and rolls back if it
    raises. ORM calls in the handler that do not pass ``using=`` run on it too.

    Example::

        @app.post("/transfer")
        @transactional
        async def transfer(request, tx):
            data = request.json()
            await tx.execute(
                "UPDATE accounts SET balance = balance - $1 WHERE id = $2",
                data["amount"], data["from"],
            )
            await tx.execute(
                "UPDATE accounts SET balance = balance + $1 WHERE id = $2",
                data["amount"], data["to"],
            )
            return {"ok": True}

    Requires ``app.enable_database(...)``; ``request.database`` must be available
    (it is injected automatically for every route once the database is enabled).
    """
    import inspect

    if not inspect.iscoroutinefunction(func):
        raise TypeError(
            "@transactional requires an 'async def' handler; native transactions "
            "are asynchronous."
        )

    @wraps(func)
    async def wrapper(request, *args, **kwargs):
        try:
            database = request.database
        except AttributeError as exc:
            raise RuntimeError(
                "@transactional needs a database. Call app.enable_database(...) first."
            ) from exc
        async with database.transaction() as tx:
            return await func(request, *args, tx=tx, **kwargs)

    return wrapper
