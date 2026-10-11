"""
Cello ORM — a small async ORM over the native Postgres pool.

This is a *real, working* ORM built on ``app.database`` (deadpool-postgres):
typed models, a chainable async QuerySet with ``Q`` objects, aggregates and
``values(...).annotate(...)`` grouping, ``ForeignKey`` with reverse accessors,
``select_related`` (one JOIN, hydrated natively) and ``prefetch_related``, and
``bulk_create`` / ``bulk_update``. It is intentionally lightweight: no
many-to-many fields, no lookups across relations, no signals/admin.
What is here is tested end-to-end against real Postgres; where the line falls
is documented in the class docstrings and docs/data-layer.md.

Quick start::

    from cello import App, DatabaseConfig
    from cello.orm import Model, AutoField, CharField, BooleanField, ForeignKey, setup

    class User(Model):
        id = AutoField()
        name = CharField(max_length=100)
        email = CharField(max_length=255, null=True)
        active = BooleanField(default=True)

    app = App()
    app.enable_database(DatabaseConfig(url="postgresql://postgres:postgres@localhost/db"))

    @app.on_event("startup")
    async def startup():
        setup(app.database)          # bind the ORM to the pool
        await User.create_table()

    @app.get("/users")
    async def users(request):
        rows = await User.objects.filter(active=True).order_by("-id").limit(20)
        return {"users": [u.to_dict() for u in rows]}

    @app.post("/users")
    async def create(request):
        data = request.json()
        u = await User.objects.create(name=data["name"], email=data.get("email"))
        return u.to_dict()
"""

import asyncio
import copy
import json
import re
from typing import Any, Optional

from cello._cello import RawJSON as _RawJSON
from cello._cello import orm_bind as _orm_bind
from cello._cello import schema_compile_field as _schema_compile_field
from cello._cello import schema_ident as _schema_ident
from cello._cello import schema_register as _schema_register
from cello.database import TransactionAborted, _current_tx, current_transaction

__all__ = [
    "setup",
    "Model",
    "Field",
    "AutoField",
    "IntegerField",
    "BigIntegerField",
    "FloatField",
    "BooleanField",
    "CharField",
    "TextField",
    "JSONField",
    "DateTimeField",
    "ForeignKey",
    "Q",
    "Aggregate",
    "Count",
    "Sum",
    "Avg",
    "Min",
    "Max",
    "DoesNotExist",
    "MultipleObjectsReturned",
    "FieldError",
    "TransactionAborted",
    "current_transaction",
]


# ── Module-level default database binding ─────────────────────────────────────

_DEFAULT_DB = None


def setup(database) -> None:
    """Bind the ORM to a native ``Database`` pool (usually ``app.database``).

    Call once in an ``on_event("startup")`` hook. After this, model queries that
    do not pass ``.using(db)`` run against this pool — or, inside
    ``async with db.transaction()`` / ``@transactional``, against that
    transaction (see :func:`cello.database.current_transaction`).
    """
    global _DEFAULT_DB
    _DEFAULT_DB = database


def _require_db(db):
    """The database for a query: explicit ``using`` first, then the ambient
    transaction of the current task, then the ``setup()`` default."""
    if db:
        return db
    db = _current_tx.get()
    if db is not None:
        return db
    db = _DEFAULT_DB
    if db is None:
        raise RuntimeError(
            "ORM is not bound to a database. Call cello.orm.setup(app.database) "
            "in a startup hook, or pass .using(request.database)."
        )
    return db


class DoesNotExist(Exception):
    """Raised by ``get()`` when no row matches."""


class MultipleObjectsReturned(Exception):
    """Raised by ``get()`` when more than one row matches."""


class FieldError(ValueError):
    """Raised when a query names a field or lookup that cannot be used safely.

    Subclasses ``ValueError`` so existing ``except ValueError`` handlers catch it.
    """


# ── Identifier safety ─────────────────────────────────────────────────────────
#
# Every identifier that reaches generated SQL is validated and quoted by the
# Rust schema registry (``src/schema``), which compiles each model once at class
# definition. Query identifiers (``filter``/``exclude``/``order_by``/``update``
# keys) must match ``^[A-Za-z_][A-Za-z0-9_]*$`` *and* name a declared field
# or column: values are always parameterized, but identifiers cannot be.


_ON_DELETE_ACTIONS = {"CASCADE", "RESTRICT", "SET NULL", "SET DEFAULT", "NO ACTION"}


# ── Fields ────────────────────────────────────────────────────────────────────

class Field:
    """Base class for model fields."""

    # Overridden by subclasses; may be a callable(field) -> str for parametrised types.
    sql_type: Any = "TEXT"

    def __init__(
        self,
        primary_key: bool = False,
        null: bool = False,
        default: Any = None,
        unique: bool = False,
        db_column: Optional[str] = None,
        write_only: bool = False,
        read_only: bool = False,
    ):
        # Visibility (v1.4.0): ``write_only`` fields are accepted by ``body=``
        # validation but never serialized; ``read_only`` fields are serialized
        # but ignored on ``body=`` input.
        if write_only and read_only:
            raise ValueError("a field cannot be both write_only and read_only")
        self.write_only = bool(write_only)
        self.read_only = bool(read_only)
        self.primary_key = primary_key
        self.null = null
        self.default = default
        self.unique = unique
        self.db_column = db_column
        self.name = None  # set by the metaclass

    @property
    def column(self) -> str:
        return self.db_column or self.name

    def ddl_type(self) -> str:
        return self.sql_type(self) if callable(self.sql_type) else self.sql_type

    def column_ddl(self) -> str:
        """The column definition used in ``CREATE TABLE`` (rendered in Rust)."""
        return _schema_compile_field(_field_descriptor(self)).column_ddl()


class AutoField(Field):
    """Auto-incrementing integer primary key (``SERIAL PRIMARY KEY``)."""

    sql_type = "SERIAL"

    def __init__(self, **kwargs):
        kwargs["primary_key"] = True
        super().__init__(**kwargs)


class IntegerField(Field):
    sql_type = "INTEGER"


class BigIntegerField(Field):
    sql_type = "BIGINT"


class FloatField(Field):
    sql_type = "DOUBLE PRECISION"


class BooleanField(Field):
    sql_type = "BOOLEAN"


class CharField(Field):
    def __init__(self, max_length: int = 255, **kwargs):
        self.max_length = max_length
        super().__init__(**kwargs)

    sql_type = staticmethod(lambda f: f"VARCHAR({f.max_length})")


class TextField(Field):
    sql_type = "TEXT"


class JSONField(Field):
    """A ``JSONB`` column. A ``dict`` / ``list`` default is copied per instance
    and rendered as JSON in the DDL ``DEFAULT``."""

    sql_type = "JSONB"


class DateTimeField(Field):
    """A ``TIMESTAMPTZ`` column.

    * ``auto_now_add=True`` — set to the database's ``now()`` on INSERT when the
      value is ``None``.
    * ``auto_now=True`` — set to ``now()`` on every INSERT and every
      ``save()`` UPDATE (an explicit value is overwritten).

    Both also give the column ``DEFAULT now()``. The value is filled in from the
    row the database returns, so it is a timestamp string like every other
    ``TIMESTAMPTZ`` read.
    """

    sql_type = "TIMESTAMPTZ"

    def __init__(self, auto_now_add: bool = False, auto_now: bool = False, **kwargs):
        self.auto_now_add = auto_now_add
        self.auto_now = auto_now
        if (auto_now_add or auto_now) and "default" not in kwargs:
            kwargs["default"] = _NOW
        super().__init__(**kwargs)


# Serial pseudo-types are not real column types: a column referencing one has
# the underlying integer type.
_SERIAL_BASE = {
    "SERIAL": "INTEGER", "SERIAL4": "INTEGER",
    "BIGSERIAL": "BIGINT", "SERIAL8": "BIGINT",
    "SMALLSERIAL": "SMALLINT", "SERIAL2": "SMALLINT",
}


class ForeignKey(Field):
    """A foreign-key column. Stored as ``<name>_id`` referencing the target's PK.

    ``to`` must be a Model class with a primary key. The column type follows the
    target's primary key (``SERIAL`` → ``INTEGER``, ``BIGSERIAL`` → ``BIGINT``,
    a ``VARCHAR`` / ``UUID`` / ... key → the same type).

    On an instance, ``book.author_id`` always holds the id. ``book.author``
    holds what was assigned: a model instance is kept (and its pk copied to
    ``author_id``), an id is stored as-is. Rows loaded from the database have
    the id in both. There is no lazy loading: load related rows with
    ``select_related("author")`` (one JOIN) or ``prefetch_related(...)``
    (one extra query), or with the target's queryset.

    The target model gets a reverse accessor, ``author.book_set`` (or
    ``related_name=``), that returns a QuerySet of the rows pointing at it.
    """

    def __init__(self, to, on_delete: str = "CASCADE", related_name: Optional[str] = None,
                 **kwargs):
        action = " ".join(str(on_delete).upper().split())
        if action not in _ON_DELETE_ACTIONS:
            raise ValueError(
                f"ForeignKey on_delete={on_delete!r} is not valid; use one of "
                f"{sorted(_ON_DELETE_ACTIONS)}"
            )
        target = getattr(to, "_meta", None)
        if target is not None and target.pk_name is None:
            raise ValueError(
                f"ForeignKey to {to.__name__}: {to.__name__} declares no primary "
                "key, so nothing can reference it. Add a primary key "
                "(e.g. `id = AutoField()`) to the target model."
            )
        if related_name is not None and (
            not isinstance(related_name, str) or not related_name.isidentifier()
            or related_name.startswith("_")
        ):
            raise ValueError(
                f"ForeignKey related_name={related_name!r} must be a Python "
                "identifier that does not start with an underscore."
            )
        self.to = to
        self.on_delete = action
        self.related_name = related_name
        super().__init__(**kwargs)

    @property
    def column(self) -> str:
        return self.db_column or f"{self.name}_id"

    def ddl_type(self) -> str:
        target = getattr(self.to, "_meta", None)
        if target is None:
            # Not a model: create_table raises the original AttributeError.
            return "INTEGER"
        pk_type = target.fields[target.pk_name].ddl_type()
        return _SERIAL_BASE.get(pk_type.strip().upper(), pk_type)


_NOW = object()  # sentinel for DEFAULT now()
_DEFAULT = object()  # sentinel for the DEFAULT keyword in bulk_create


class _JsonDefault:
    """A JSONField default, rendered as JSON text in DDL. Not a ``str``, so the
    registry still classifies the default as a structured ("other") value."""

    __slots__ = ("text",)

    def __init__(self, text: str):
        self.text = text

    def __str__(self) -> str:
        return self.text


def _ddl_default(field: Field):
    default = field.default
    if isinstance(field, JSONField) and isinstance(default, (dict, list)):
        try:
            return _JsonDefault(json.dumps(default))
        except (TypeError, ValueError):
            # Not JSON-serialisable (or circular): keep the 1.3 rendering;
            # Postgres rejects it at create_table.
            return default
    return default


def _field_descriptor(field: Field) -> dict:
    """Describe a field for the Rust schema registry (cold path: class definition).

    Only plain values cross into Rust. Declaration errors that 1.3.x raised
    lazily at ``create_table`` (a ``sql_type`` callable that fails, a ForeignKey
    whose target is not a model) are not raised here; ``create_table`` replays
    them so they surface exactly as before.
    """
    try:
        sql_type = field.ddl_type()
    except Exception:
        sql_type = None
    fk = None
    if isinstance(field, ForeignKey):
        fk = {"on_delete": field.on_delete}
        target = getattr(field.to, "_meta", None)
        if target is not None:
            fk["table"] = str(target.table_name)
            fk["pk_column"] = None if target.pk_column is None else str(target.pk_column)
            schema = getattr(target, "schema", None)
            fk["model_id"] = schema.id if schema is not None else None
    return {
        "name": str(field.name),
        "column": str(field.column),
        "sql_type": None if sql_type is None else str(sql_type),
        "primary_key": bool(field.primary_key),
        "null": bool(field.null),
        "unique": bool(field.unique),
        "max_length": getattr(field, "max_length", None),
        "default": _ddl_default(field),
        "default_now": field.default is _NOW,
        "auto": isinstance(field, AutoField),
        "auto_now_add": bool(getattr(field, "auto_now_add", False)),
        "write_only": bool(getattr(field, "write_only", False)),
        "read_only": bool(getattr(field, "read_only", False)),
        "fk": fk,
    }


def _replay_declaration_errors(cls) -> None:
    """Re-raise a declaration error deferred at class definition (see above)."""
    for field in cls._meta.fields.values():
        field.ddl_type()
        if isinstance(field, ForeignKey):
            field.to._meta


# ── Model metaclass ───────────────────────────────────────────────────────────

_INT_RANGES = {
    "SMALLINT": (-(2**15), 2**15 - 1), "INT2": (-(2**15), 2**15 - 1),
    "INTEGER": (-(2**31), 2**31 - 1), "INT": (-(2**31), 2**31 - 1),
    "INT4": (-(2**31), 2**31 - 1),
    "BIGINT": (-(2**63), 2**63 - 1), "INT8": (-(2**63), 2**63 - 1),
}

# How a field takes part in INSERT / UPDATE (see Model._insert / _update).
_W_PLAIN, _W_SERIAL, _W_NOW_ADD, _W_NOW = 0, 1, 2, 3

# How __init__ fills a field that was not passed.
_D_NONE, _D_VALUE, _D_CALL, _D_COPY = 0, 1, 2, 3


_NO_RANGE = (float("-inf"), float("inf"))


def _int_range(field):
    try:
        t = field.ddl_type().strip().upper()
    except Exception:
        return None
    return _INT_RANGES.get(_SERIAL_BASE.get(t, t))


class _Meta:
    def __init__(self, table_name, fields, pk_name, pk_column, schema):
        self.table_name = table_name
        self.fields = fields              # dict name -> Field
        self.pk_name = pk_name
        self.pk_column = pk_column
        # Compiled Rust spec (src/schema). Owns this model's registry entry.
        self.schema = schema
        # Hot-path caches handed back by the registry once, at class creation:
        # valid query identifier -> quoted column, and field name -> quoted column.
        self.lookup = schema.lookup()
        self.quoted_columns = schema.quoted_columns()
        self.quoted_table = schema.quoted_table
        self.quoted_pk = schema.quoted_pk

        # Everything below is derived once here so the per-row / per-query paths
        # are plain loops over precomputed tuples.
        fks = {n: f for n, f in fields.items() if isinstance(f, ForeignKey)}
        self.fks = fks
        # Identifier -> Field, with the registry's precedence (a field name wins
        # over another field's column). Only identifiers in ``lookup``.
        by_ident = {}
        for n, f in fields.items():
            by_ident.setdefault(f.column, f)
        for n, f in fields.items():
            by_ident[n] = f
        by_ident = {k: f for k, f in by_ident.items() if k in self.lookup}
        self.column_of = {k: f.column for k, f in by_ident.items()}
        self.field_of = by_ident
        # Reverse accessors installed on this model by ForeignKeys pointing at
        # it: name -> (source model, ForeignKey field name), or None when two
        # ForeignKeys claim the same default name (see _install_reverse).
        self.reverse = {}
        self.int_ranges = {}
        for k, f in by_ident.items():
            r = _int_range(f)
            if r is not None:
                self.int_ranges[k] = r
        # Row hydration: (attribute, column); a ForeignKey also sets <column>.
        hydrate = []
        for n, f in fields.items():
            hydrate.append((n, f.column))
            if n in fks and f.column != n:
                hydrate.append((f.column, f.column))
        self.hydrate = tuple(hydrate)
        # Every attribute _from_row sets (see _native_json).
        self.attr_names = frozenset(a for a, _ in hydrate)
        # __init__ plan: (name, column-or-None for FKs, default mode, default).
        init = []
        for n, f in fields.items():
            d = f.default
            if d is None or d is _NOW:
                mode = _D_NONE
            elif callable(d):
                mode = _D_CALL
            elif isinstance(d, (dict, list, set)):
                mode = _D_COPY
            else:
                mode = _D_VALUE
            init.append((n, fks[n].column if n in fks else None, mode, d))
        self.init_plan = tuple(init)
        # INSERT / UPDATE plan: (name, field, quoted column, write mode).
        write = []
        for n, f in fields.items():
            if getattr(f, "auto_now", False):
                mode = _W_NOW
            elif getattr(f, "auto_now_add", False):
                mode = _W_NOW_ADD
            elif isinstance(f, AutoField) or _serial(f):
                mode = _W_SERIAL
            else:
                mode = _W_PLAIN
            write.append((n, f, self.quoted_columns[n], mode))
        self.write_plan = tuple(write)
        self.auto_now = tuple(n for n, f, q, m in write if m == _W_NOW)
        # Attribute names that _fk_setattr keeps in sync: name -> (name, column).
        self.fk_attrs = {}
        for n, f in fks.items():
            self.fk_attrs[n] = (n, f.column)
        for n, f in fks.items():
            if f.column not in fields:
                self.fk_attrs.setdefault(f.column, (n, f.column))


def _serial(field) -> bool:
    try:
        return field.ddl_type().strip().upper() in _SERIAL_BASE
    except Exception:
        return False


def _pk_of(obj):
    pk_name = obj._meta.pk_name
    return None if pk_name is None else getattr(obj, pk_name, None)


def _fk_setattr(self, name, value):
    """Keep a ForeignKey's ``<name>`` and ``<name>_id`` attributes in sync.

    Installed only on models that declare a ForeignKey.
    """
    entry = self._meta.fk_attrs.get(name)
    if entry is None:
        object.__setattr__(self, name, value)
        return
    fname, col = entry
    d = self.__dict__
    if name == fname:
        if isinstance(value, Model):
            if col == fname:
                d[col] = _pk_of(value)
            else:
                d[fname] = value
                d[col] = _pk_of(value)
        else:
            d[fname] = value
            d[col] = value
    else:  # assigning the id column directly
        d[col] = value
        cached = d.get(fname)
        if isinstance(cached, Model) and _pk_of(cached) != value:
            d[fname] = value


class ModelMeta(type):
    def __new__(mcls, name, bases, namespace):
        if name == "Model":
            return super().__new__(mcls, name, bases, namespace)

        fields = {}
        for key, value in list(namespace.items()):
            if isinstance(value, Field):
                value.name = key
                fields[key] = value
                del namespace[key]
        own = tuple(fields)

        # Inherit fields from base models.
        for base in bases:
            if hasattr(base, "_meta"):
                for k, v in base._meta.fields.items():
                    fields.setdefault(k, v)

        pk_name = next((n for n, f in fields.items() if f.primary_key), None)
        meta_opts = namespace.get("Meta")
        table_name = getattr(meta_opts, "table_name", None) or name.lower()
        pk_column = fields[pk_name].column if pk_name else None

        cls = super().__new__(mcls, name, bases, namespace)
        # FK attribute sync, unless the class (or a mixin) has its own __setattr__.
        if cls.__setattr__ is object.__setattr__ and any(
            isinstance(f, ForeignKey) for f in fields.values()
        ):
            cls.__setattr__ = _fk_setattr
        schema = _schema_register(
            name, str(table_name), [_field_descriptor(f) for f in fields.values()]
        )
        cls._meta = _Meta(table_name, fields, pk_name, pk_column, schema)
        cls.objects = Manager(cls)
        for fname in own:
            if isinstance(fields[fname], ForeignKey):
                _install_reverse(cls, fname, fields[fname])
        cls.DoesNotExist = type("DoesNotExist", (DoesNotExist,), {})
        cls.MultipleObjectsReturned = type(
            "MultipleObjectsReturned", (MultipleObjectsReturned,), {}
        )
        return cls


class Model(metaclass=ModelMeta):
    """Base class for ORM models. Define fields as class attributes.

    * Construction applies field defaults (a callable default is called, a
      ``dict`` / ``list`` default is copied). An explicit ``None`` is kept.
    * ``save()`` inserts when the primary key is ``None``; otherwise it updates
      that row and inserts if no row was updated.
    * Two instances are equal when they are the same class and have the same
      non-``None`` primary key; ``hash()`` follows the same rule (unsaved
      instances hash by identity).
    """

    def __init__(self, **kwargs):
        d = self.__dict__
        for name, fk_col, mode, default in self._meta.init_plan:
            if name in kwargs:
                value = kwargs[name]
            elif mode == _D_NONE:
                value = None
            elif mode == _D_VALUE:
                value = default
            elif mode == _D_CALL:
                value = default()
            else:
                value = copy.deepcopy(default)
            if fk_col is None:
                d[name] = value
                continue
            # ForeignKey: <name> keeps what was passed, <column> holds the id.
            ident = _pk_of(value) if isinstance(value, Model) else value
            if fk_col == name:
                d[name] = ident
            else:
                d[name] = value
                d[fk_col] = ident
                if fk_col in kwargs:
                    d[fk_col] = kwargs[fk_col]
                    if isinstance(value, Model) and ident != d[fk_col]:
                        d[name] = d[fk_col]

    def __eq__(self, other):
        if self is other:
            return True
        if type(self) is not type(other):
            return NotImplemented
        pk = _pk_of(self)
        return pk is not None and pk == _pk_of(other)

    def __hash__(self):
        pk = _pk_of(self)
        if pk is None:
            return object.__hash__(self)
        return hash((type(self), pk))

    def to_dict(self) -> dict:
        """The row as a dict: one key per field (``<name>_id`` for a
        ForeignKey), in declaration order; ``write_only`` fields are left out.

        Built natively from the compiled schema (``src/schema``): the same
        ``getattr`` reads and dict as the 1.3 Python loop, without the
        interpreter overhead.
        """
        return self._meta.schema.to_dict(self)

    @classmethod
    def _from_row(cls, row: dict) -> "Model":
        obj = cls.__new__(cls)
        d = obj.__dict__
        for attr, col in cls._meta.hydrate:
            d[attr] = row.get(col)
        return obj

    def _load_row(self, row: dict) -> None:
        """Refresh every field from a RETURNING row, keeping an assigned
        related instance when it is still the referenced row."""
        meta = self._meta
        d = self.__dict__
        keep = None
        if meta.fks:
            keep = [(n, f.column, d.get(n)) for n, f in meta.fks.items()
                    if isinstance(d.get(n), Model)]
        for attr, col in meta.hydrate:
            d[attr] = row.get(col)
        if keep:
            for n, col, obj in keep:
                if col != n and _pk_of(obj) == d.get(col):
                    d[n] = obj

    async def save(self, using=None) -> "Model":
        """Insert or update this row.

        Inserts when the primary key is ``None`` (or the model has none).
        Otherwise updates every column of that row; if no row has that primary
        key, inserts it. Values generated by the database (``AutoField``,
        ``auto_now`` / ``auto_now_add``) are read back with ``RETURNING``.
        """
        db = _require_db(using)
        meta = self._meta
        if meta.pk_name is not None and getattr(self, meta.pk_name, None) is not None:
            if await self._update(db):
                return self
        return await self._insert(db)

    async def _insert(self, db) -> "Model":
        meta = self._meta
        cols, marks, params = [], [], []
        for name, field, quoted, mode in meta.write_plan:
            value = _field_value(self, name, field)
            if mode == _W_PLAIN:
                pass
            elif mode == _W_SERIAL:
                if value is None:
                    continue
            elif mode == _W_NOW or value is None:  # auto_now, or auto_now_add unset
                cols.append(quoted)
                marks.append("now()")
                continue
            params.append(value)
            cols.append(quoted)
            marks.append(f"${len(params)}")
        if cols:
            sql = (
                f'INSERT INTO {meta.quoted_table} ({", ".join(cols)}) '
                f'VALUES ({", ".join(marks)}) RETURNING *'
            )
        else:
            sql = f"INSERT INTO {meta.quoted_table} DEFAULT VALUES RETURNING *"
        row = await db.fetchrow(sql, *params)
        self._load_row(row)
        return self

    async def _update(self, db) -> bool:
        """UPDATE every non-pk column; ``False`` when no row has this pk."""
        meta = self._meta
        sets, params = [], []
        for name, field, quoted, mode in meta.write_plan:
            if name == meta.pk_name:
                continue
            if mode == _W_NOW:
                sets.append(f"{quoted} = now()")
                continue
            params.append(_field_value(self, name, field))
            sets.append(f"{quoted} = ${len(params)}")
        if not sets:
            sets.append(f"{meta.quoted_pk} = {meta.quoted_pk}")
        params.append(getattr(self, meta.pk_name))
        returning = "*" if meta.auto_now else meta.quoted_pk
        sql = (
            f'UPDATE {meta.quoted_table} SET {", ".join(sets)} '
            f"WHERE {meta.quoted_pk} = ${len(params)} RETURNING {returning}"
        )
        rows = await db.fetch(sql, *params)
        if not rows:
            return False
        if meta.auto_now:
            row = rows[0]
            d = self.__dict__
            for name in meta.auto_now:
                d[name] = row.get(meta.fields[name].column)
        return True

    async def delete(self, using=None) -> int:
        """Delete this row by primary key; returns the number of rows deleted
        (``0`` for an instance whose primary key is ``None``)."""
        db = _require_db(using)
        meta = self._meta
        if meta.pk_name is None:
            raise ValueError(
                f"{type(self).__name__} has no primary key, so an instance cannot "
                f"be deleted on its own. Use {type(self).__name__}.objects"
                ".filter(...).delete()."
            )
        pk_val = getattr(self, meta.pk_name, None)
        if pk_val is None:
            return 0
        return await db.execute(
            f"DELETE FROM {meta.quoted_table} WHERE {meta.quoted_pk} = $1",
            pk_val,
        )

    @classmethod
    async def create_table(cls, using=None, if_not_exists: bool = True) -> None:
        db = _require_db(using)
        schema = cls._meta.schema
        if not schema.complete:
            _replay_declaration_errors(cls)
        await db.execute(schema.create_table_sql(bool(if_not_exists)))

    @classmethod
    async def drop_table(cls, using=None, if_exists: bool = True) -> None:
        db = _require_db(using)
        await db.execute(cls._meta.schema.drop_table_sql(bool(if_exists)))

    def __repr__(self):
        pk = getattr(self, self._meta.pk_name, None) if self._meta.pk_name else None
        return f"<{self.__class__.__name__} pk={pk}>"


def _fk_value(obj, name, field):
    """The id a ForeignKey stores: ``<column>``, else an assigned instance's pk."""
    val = getattr(obj, field.column, None)
    if val is None:
        related = getattr(obj, name, None)
        if isinstance(related, Model):
            val = _pk_of(related)
    return val


def _field_value(obj, name, field):
    if isinstance(field, ForeignKey):
        return _fk_value(obj, name, field)
    return getattr(obj, name, None)


# ── Reverse accessors (1.4.0 Phase 04) ───────────────────────────────────────


class _ReverseAccessor:
    """``author.book_set``: the rows of the source model whose ForeignKey
    points at this instance, as a QuerySet (``Book.objects.filter(author=a)``).

    A non-data descriptor: ``prefetch_related("book_set")`` stores a QuerySet
    holding the prefetched rows in the instance ``__dict__``, which then wins.
    """

    __slots__ = ("source", "field_name", "name", "explicit", "ambiguous")

    def __init__(self, source, field_name, name, explicit):
        self.source = source
        self.field_name = field_name
        self.name = name
        self.explicit = explicit
        self.ambiguous = None

    def __get__(self, obj, owner=None):
        if obj is None:
            return self
        if self.ambiguous is not None:
            raise AttributeError(self.ambiguous)
        return self.source.objects.filter(**{self.field_name: obj})

    def __repr__(self):
        return f"<reverse accessor {self.name} -> {self.source.__name__}.{self.field_name}>"


def _install_reverse(source, fname, field) -> None:
    """Install the reverse accessor for ``source.<fname>`` (a ForeignKey) on
    its target model. Called by the metaclass for ForeignKeys declared on the
    class itself (inherited ones get no accessor).

    The name is ``related_name``, else ``<source name lowercased>_set``. An
    explicit name that collides with a field or attribute of the target, or
    with another model's accessor, raises ``ValueError``. A default name that
    collides is not installed (a field / attribute) or becomes ambiguous
    (another ForeignKey's default accessor; accessing it raises
    ``AttributeError`` asking for ``related_name``). Redefining a model (same
    module and qualified name) replaces its accessors.
    """
    target = field.to
    tmeta = getattr(target, "_meta", None)
    if not isinstance(tmeta, _Meta):
        return
    explicit = field.related_name is not None
    name = field.related_name if explicit else f"{source.__name__.lower()}_set"

    def clash(what):
        if explicit:
            raise ValueError(
                f"ForeignKey {source.__name__}.{fname}: related_name {name!r} "
                f"clashes with {what} on {target.__name__}; choose another name."
            )

    if name in tmeta.fields or name in tmeta.attr_names:
        return clash("a field")
    found = None
    for klass in target.__mro__:
        if name in klass.__dict__:
            found = (klass, klass.__dict__[name])
            break
    if found is not None:
        klass, existing = found
        if not isinstance(existing, _ReverseAccessor):
            return clash("an attribute")
        if klass is target:
            redefined = (
                existing.source is not source
                and existing.source.__module__ == source.__module__
                and existing.source.__qualname__ == source.__qualname__
            )
            if not redefined:
                if explicit or existing.explicit:
                    raise ValueError(
                        f"ForeignKey {source.__name__}.{fname}: reverse accessor "
                        f"{name!r} on {target.__name__} is already used by "
                        f"{existing.source.__name__}.{existing.field_name}; set a "
                        "different related_name."
                    )
                existing.ambiguous = (
                    f"{target.__name__}.{name} is ambiguous: both "
                    f"{existing.source.__name__}.{existing.field_name} and "
                    f"{source.__name__}.{fname} point at {target.__name__}. "
                    "Give the ForeignKeys a related_name."
                )
                tmeta.reverse[name] = None
                return
    setattr(target, name, _ReverseAccessor(source, fname, name, explicit))
    tmeta.reverse[name] = (source, fname)


def _forward_fk(model, name):
    """The ForeignKey ``model.<name>`` that select_related / prefetch_related
    can attach an instance to, or ``None``."""
    field = model._meta.fks.get(name) if isinstance(name, str) else None
    if field is None:
        return None
    if field.column == name:
        raise FieldError(
            f"{model.__name__}.{name} stores its id under its own name "
            f"(db_column={name!r}), so a related instance cannot be attached "
            "to it. Load the related rows with the target's queryset."
        )
    if not isinstance(getattr(field.to, "_meta", None), _Meta):
        raise FieldError(f"{model.__name__}.{name} does not reference a model.")
    return field


def _reverse_of(model, name):
    """``(source model, ForeignKey name)`` for reverse accessor ``name``."""
    try:
        entry = model._meta.reverse[name]
    except (KeyError, TypeError):
        return None
    if entry is None:
        raise FieldError(
            f"{model.__name__}.{name} is ambiguous: several ForeignKeys point at "
            f"{model.__name__} with the same default accessor name. Give them a "
            "related_name."
        )
    return entry


def _split_path(path, what):
    if not isinstance(path, str) or not path:
        raise FieldError(f"{what}() expects relation names as strings, got {path!r}.")
    return path.split("__")


# ── Q objects (1.4.0 Phase 04) ───────────────────────────────────────────────


class Q:
    """A composable filter condition.

    ``Q(**lookups)`` holds the same lookups as ``filter(**lookups)`` (ANDed).
    Combine with ``|`` (OR), ``&`` (AND) and ``~`` (NOT), nest freely, and pass
    to ``filter()`` / ``exclude()`` / ``get()``, alongside keyword lookups::

        Book.objects.filter(Q(title__startswith="A") | Q(pages__gt=300), active=True)
        Book.objects.filter(~Q(author=None))

    NULL semantics: a lookup is SQL, so a comparison with a NULL column is
    *unknown*, and a row is returned only when the whole condition is *true*.
    ``~q`` means "``q`` is not true" (SQL ``(q) IS NOT TRUE``): it keeps rows
    where ``q`` is false *or unknown*, exactly like ``exclude()``.
    ``filter(~Q(x=5))`` is the same query as ``exclude(x=5)`` and keeps the
    rows where ``x`` is NULL. An empty ``Q()`` adds no condition; inside ``|`` /
    ``&`` it is dropped.
    """

    __slots__ = ("children", "connector", "negated")

    AND = "AND"
    OR = "OR"

    def __init__(self, *args, **kwargs):
        for a in args:
            if not isinstance(a, Q):
                raise TypeError(f"Q() positional arguments must be Q objects, got {a!r}.")
        self.children = list(args) + list(kwargs.items())
        self.connector = Q.AND
        self.negated = False

    @classmethod
    def _make(cls, children, connector, negated):
        q = cls.__new__(cls)
        q.children = children
        q.connector = connector
        q.negated = negated
        return q

    def _combine(self, other, connector):
        if not isinstance(other, Q):
            return NotImplemented
        return Q._make([self, other], connector, False)

    def __or__(self, other):
        return self._combine(other, Q.OR)

    def __and__(self, other):
        return self._combine(other, Q.AND)

    def __invert__(self):
        # In a WHERE clause only "is it true" matters, so ~~q behaves as q.
        return Q._make(list(self.children), self.connector, not self.negated)

    def __bool__(self):
        return bool(self.children)

    def __repr__(self):
        inner = f" {self.connector} ".join(
            repr(c) if isinstance(c, Q) else f"{c[0]}={c[1]!r}" for c in self.children
        )
        return f"<Q: {'NOT ' if self.negated else ''}({inner})>"


# ── Aggregates (1.4.0 Phase 04) ──────────────────────────────────────────────


class Aggregate:
    """An SQL aggregate over one field, for ``aggregate()`` and
    ``values(...).annotate()``. Subclasses: ``Count``, ``Sum``, ``Avg``,
    ``Min``, ``Max``.

    The value comes back in the Postgres result type, decoded like any other
    column (see docs/data-layer.md, "Aggregates"): ``count`` is an ``int``;
    ``sum`` of an ``INTEGER`` is an ``int`` but of a ``BIGINT`` is ``NUMERIC``
    (a decimal string); ``avg`` of an integer column is ``NUMERIC`` (a decimal
    string) and of a ``DOUBLE PRECISION`` column a ``float``; ``min`` / ``max``
    keep the column type. Over no rows every aggregate but ``Count`` is
    ``None``.
    """

    function = None

    def __init__(self, field, distinct: bool = False):
        if not isinstance(field, str):
            raise FieldError(
                f"{type(self).__name__}() expects a field name as a string, got {field!r}."
            )
        if not isinstance(distinct, bool):
            raise TypeError("distinct must be True or False.")
        if field == "*" and (self.function != "count" or distinct):
            raise FieldError(f"{type(self).__name__}('*') is not valid; name a field.")
        self.field = field
        self.distinct = distinct

    @property
    def default_alias(self) -> str:
        if self.field == "*":
            return self.function
        return f"{self.field}__{self.function}"

    def _sql(self, qs) -> str:
        if self.field == "*":
            return "count(*)"
        column = qs._resolve_column(self.field)
        return f'{self.function}({"DISTINCT " if self.distinct else ""}{column})'

    def __repr__(self):
        d = ", distinct=True" if self.distinct else ""
        return f"{type(self).__name__}({self.field!r}{d})"


class Count(Aggregate):
    """Number of non-NULL values of a field; ``Count("*")`` counts rows."""

    function = "count"

    def __init__(self, field="*", distinct: bool = False):
        super().__init__(field, distinct)


class Sum(Aggregate):
    function = "sum"


class Avg(Aggregate):
    function = "avg"


class Min(Aggregate):
    function = "min"


class Max(Aggregate):
    function = "max"


def _alias(name) -> str:
    """Validate an aggregate / annotation alias and quote it (registry rule:
    ``^[A-Za-z_][A-Za-z0-9_]*$``)."""
    quoted = _schema_ident(name)
    if quoted is None:
        raise FieldError(
            f"Invalid alias {name!r}: aliases must match ^[A-Za-z_][A-Za-z0-9_]*$."
        )
    return quoted


def _aggregates(args, kwargs) -> dict:
    """``{alias: Aggregate}`` from ``aggregate()`` / ``annotate()`` arguments."""
    out = {}
    for a in args:
        if not isinstance(a, Aggregate):
            raise TypeError(f"expected an aggregate such as Sum('price'), got {a!r}.")
        if a.default_alias in out:
            raise ValueError(f"duplicate aggregate alias {a.default_alias!r}.")
        out[a.default_alias] = a
    for alias, a in kwargs.items():
        if not isinstance(a, Aggregate):
            raise TypeError(f"{alias}= expects an aggregate such as Sum('price'), got {a!r}.")
        if alias in out:
            raise ValueError(f"duplicate aggregate alias {alias!r}.")
        out[alias] = a
    return out


# Bulk operations: Postgres accepts at most 65535 bind parameters per statement.
_MAX_PARAMS = 65535

_TYPE_MODIFIER = re.compile(r"\([^)]*\)")


def _cast_type(model, name) -> str:
    """The SQL type a ``bulk_update`` parameter for ``model.<name>`` is cast to:
    the registry's column type, with serial pseudo-types mapped to their
    integer type and length / precision modifiers dropped (the assignment
    then applies the column's own limits, as ``save()`` does, instead of an
    explicit cast silently truncating a too-long string)."""
    meta = model._meta
    cache = meta.__dict__.setdefault("_casts", {})
    t = cache.get(name)
    if t is None:
        spec = meta.schema.field(name)
        ddl = None if spec is None else spec.pg_type
        if ddl is None:
            _replay_declaration_errors(model)
            raise FieldError(f"{model.__name__}.{name} has no column type.")
        base = ddl.strip()
        base = _SERIAL_BASE.get(base.upper(), base)
        t = cache[name] = _TYPE_MODIFIER.sub("", base)
    return t


# ── QuerySet / Manager ────────────────────────────────────────────────────────

_OPERATORS = {
    "exact": "=",
    "ne": "!=",
    "gt": ">",
    "gte": ">=",
    "lt": "<",
    "lte": "<=",
    "contains": "LIKE",
    "icontains": "ILIKE",
    "startswith": "LIKE",
    "istartswith": "ILIKE",
    "endswith": "LIKE",
    "iendswith": "ILIKE",
    "in": "IN",
    "isnull": "IS",
}

# LIKE pattern builders; the value is escaped first (see _like_escape).
_LIKE = {
    "contains": "%{}%",
    "icontains": "%{}%",
    "startswith": "{}%",
    "istartswith": "{}%",
    "endswith": "%{}",
    "iendswith": "%{}",
}


def _like_escape(value) -> str:
    return str(value).replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")


def _resolve_slow(model, field_name) -> str:
    """Report an identifier that missed ``_meta.lookup``: it is either not a
    valid identifier or not declared on the model. Always raises."""
    if model._meta.schema.resolve(field_name) is None:
        raise FieldError(
            f"Invalid field name {field_name!r} for {model.__name__}: "
            "field names must match ^[A-Za-z_][A-Za-z0-9_]*$."
        )
    raise FieldError(
        f"{model.__name__} has no declared field or column {field_name!r}. "
        f"Declare the field on {model.__name__}, or query the column with "
        "`await db.fetch(sql, *params)` and parameters."
    )


def _coerce(model, ident, value):
    """Prepare a query value: a model instance becomes its primary key, and an
    int is range-checked against an integer column."""
    t = type(value)
    if t is int:
        r = model._meta.int_ranges.get(ident)
        if r is not None and not (r[0] <= value <= r[1]):
            raise ValueError(
                f"{value} is out of range for {model.__name__}.{ident} "
                f"(an integer column holds {r[0]}..{r[1]})."
            )
    elif t is not str and isinstance(value, Model):
        pk = _pk_of(value)
        if pk is None:
            raise ValueError(
                f"{type(value).__name__} instance used in a {model.__name__} query "
                "has no primary key (save it first)."
            )
        return pk
    return value


def _is_data_descriptor(value) -> bool:
    t = type(value)
    return hasattr(t, "__set__") or hasattr(t, "__delete__")


def _native_json(model) -> bool:
    """Whether ``QuerySet.json()`` may encode ``model`` rows straight from the
    wire.

    The native encoder writes what ``to_dict()`` returns for an instance freshly
    loaded from the row. That holds unless the class overrides ``to_dict``,
    ``_from_row`` or ``__getattribute__``, or a data descriptor (such as a
    ``@property``) on the class shadows a field attribute. Those models are
    encoded from real instances instead: same bytes, Python speed. Checked per
    call (about a microsecond), so later changes to the class are honoured.
    """
    if (
        model.to_dict is not _BASE_TO_DICT
        or model._from_row.__func__ is not _BASE_FROM_ROW
        or model.__getattribute__ is not object.__getattribute__
    ):
        return False
    names = model._meta.attr_names
    for klass in model.__mro__[:-1]:
        attrs = klass.__dict__
        for name in names.intersection(attrs):
            if _is_data_descriptor(attrs[name]):
                return False
    return True


async def _run_querysets(querysets):
    """Run the querysets a handler returned; one ``RawJSON`` each, in order.

    Called by the response serializer (``src/handler.rs``) after the handler
    has returned, on the persistent event loop.
    """
    if len(querysets) == 1:
        return [await querysets[0].json()]
    return await asyncio.gather(*[qs.json() for qs in querysets])


class QuerySet:
    """A lazy, chainable, awaitable query.

    Chain ``filter``/``exclude``/``order_by``/``limit``/``offset`` (each returns a
    new QuerySet), then await it (or call ``all()``) to run it. Terminal helpers:
    ``get``, ``first``, ``count``, ``exists``, ``values``, ``delete``, ``update``.

    Returning a QuerySet from a handler (``return {"users": qs}``) runs it after
    the handler returns and writes its rows into the response natively: the
    fast path for list endpoints (see ``json()``).
    """

    def __init__(self, model, using=None):
        self.model = model
        self._db = using
        self._where = []       # list of (sql_fragment, params_list)
        self._order = []
        self._limit = None
        self._offset = None
        self._related = ()     # select_related paths
        self._prefetch = ()    # prefetch_related paths
        self._cache = None     # rows already loaded (prefetched reverse accessor)

    def _clone(self):
        cls = self.__class__
        qs = _object_new(cls)
        if cls is QuerySet:  # the common case, attribute by attribute (faster)
            qs.model = self.model
            qs._db = self._db
            qs._where = self._where[:]
            qs._order = self._order[:]
            qs._limit = self._limit
            qs._offset = self._offset
            qs._related = self._related
            qs._prefetch = self._prefetch
            qs._cache = None
            return qs
        # Subclasses (ValuesQuerySet, user subclasses) keep all their state.
        d = qs.__dict__
        d.update(self.__dict__)
        d["_where"] = self._where[:]
        d["_order"] = self._order[:]
        d["_cache"] = None
        return qs

    def using(self, database) -> "QuerySet":
        qs = self._clone()
        qs._db = database
        return qs

    def _resolve_column(self, field_name: str) -> str:
        """Map a field name from a query argument to a quoted SQL column.

        * A declared field name resolves to its column (``author`` → ``author_id``
          for a ForeignKey).
        * A declared column name (a ForeignKey's ``<name>_id``, or a
          ``db_column``) resolves to itself.
        * Anything else raises ``FieldError`` — it never reaches SQL.

        Declared names hit ``_meta.lookup``, a dict the Rust registry built once
        at class definition; a miss goes to ``_resolve_slow``, which raises.
        """
        try:
            quoted = self.model._meta.lookup.get(field_name)
        except TypeError:  # unhashable: not a name at all
            quoted = None
        if quoted is not None:
            return quoted
        return _resolve_slow(self.model, field_name)

    def _split_lookup(self, key: str):
        """Split ``field__lookup`` into ``(field_name, lookup)``.

        A trailing ``__<name>`` is only treated as a lookup when ``<name>`` is a
        supported lookup. An unsupported lookup on a declared field raises
        ``FieldError`` instead of being mistaken for a column name.
        """
        if not isinstance(key, str):
            raise FieldError(f"Invalid field name {key!r} for {self.model.__name__}.")
        if "__" in key:
            field_name, lookup = key.rsplit("__", 1)
            if lookup in _OPERATORS:
                return field_name, lookup
            if field_name in self.model._meta.fields:
                raise FieldError(
                    f"Unsupported lookup {lookup!r} on field {field_name!r} of "
                    f"{self.model.__name__}; supported lookups: "
                    f"{', '.join(sorted(_OPERATORS))}."
                )
        return key, "exact"

    def _condition(self, key, value):
        """One lookup as ``(sql_fragment, params)``; ``?`` marks a parameter."""
        model = self.model
        field_name, lookup = self._split_lookup(key)
        column = self._resolve_column(field_name)

        if lookup == "exact" or lookup == "ne":
            if value is None:
                return f'{column} IS {"NULL" if lookup == "exact" else "NOT NULL"}', []
            return f"{column} {_OPERATORS[lookup]} ?", [_coerce(model, field_name, value)]
        if lookup == "isnull":
            return f'{column} IS {"NULL" if value else "NOT NULL"}', []
        if lookup == "in":
            if isinstance(value, (str, bytes, bytearray)):
                raise TypeError(
                    f"{key}= expects an iterable of values, not "
                    f"{type(value).__name__} (wrap a single value in a list)."
                )
            values = list(value)
            if not values:
                return "FALSE", []
            lo, hi = model._meta.int_ranges.get(field_name, _NO_RANGE)
            for i, v in enumerate(values):
                t = type(v)
                if t is int:
                    if not lo <= v <= hi:
                        _coerce(model, field_name, v)  # raises
                elif t is not str and v is not None:
                    values[i] = _coerce(model, field_name, v)
            return f'{column} IN ({", ".join(["?"] * len(values))})', values
        pattern = _LIKE.get(lookup)
        if pattern is not None:
            return (
                f"{column} {_OPERATORS[lookup]} ? ESCAPE '\\'",
                [pattern.format(_like_escape(value))],
            )
        return f"{column} {_OPERATORS[lookup]} ?", [_coerce(model, field_name, value)]

    def _compile_q(self, q):
        """A ``Q`` tree as ``(sql_fragment, params)`` with ``?`` placeholders,
        or ``None`` when it holds no condition. Each lookup is compiled by
        ``_condition``, exactly as a ``filter()`` keyword is; a negated node is
        ``(...) IS NOT TRUE``, exactly as ``exclude()`` builds it."""
        frags, params = [], []
        for child in q.children:
            if isinstance(child, Q):
                r = self._compile_q(child)
                if r is None:
                    continue
                frag, p = r
            else:
                frag, p = self._condition(*child)
            frags.append(frag)
            params.extend(p)
        if not frags:
            return None
        if len(frags) == 1:
            body, atomic = frags[0], True
        else:
            body, atomic = f" {q.connector} ".join(frags), False
        if q.negated:
            return f"({body}) IS NOT TRUE", params
        return (body if atomic else f"({body})"), params

    def filter(self, *args, **kwargs) -> "QuerySet":
        """Rows for which every condition is true: keyword lookups and ``Q``
        objects, ANDed together."""
        qs = self._clone()
        for q in args:
            if not isinstance(q, Q):
                raise TypeError(
                    f"filter() positional arguments must be Q objects, got {q!r}."
                )
            r = self._compile_q(q)
            if r is not None:
                qs._where.append(r)
        for key, value in kwargs.items():
            qs._where.append(self._condition(key, value))
        return qs

    def exclude(self, *args, **kwargs) -> "QuerySet":
        """Rows for which the conditions, ANDed together, are not true.

        ``exclude(a=1, b=2)`` is ``NOT (a = 1 AND b = 2)``. Rows where the
        condition is unknown because a column is NULL are kept (the condition
        is not *true* for them). ``exclude(q)`` is ``filter(~q)``.
        """
        if args:
            for q in args:
                if not isinstance(q, Q):
                    raise TypeError(
                        f"exclude() positional arguments must be Q objects, got {q!r}."
                    )
            r = self._compile_q(~Q(*args, **kwargs))
            qs = self._clone()
            if r is not None:
                qs._where.append(r)
            return qs
        if not kwargs:
            return self._clone()
        frags, params = [], []
        for key, value in kwargs.items():
            frag, p = self._condition(key, value)
            frags.append(frag)
            params.extend(p)
        qs = self._clone()
        qs._where.append((f'({" AND ".join(frags)}) IS NOT TRUE', params))
        return qs

    def order_by(self, *fields) -> "QuerySet":
        """Set the ordering, replacing any previous ``order_by()``."""
        qs = self._clone()
        order = []
        for f in fields:
            if not isinstance(f, str):
                raise FieldError(
                    f"order_by() expects field names as strings, got {f!r}."
                )
            if f.startswith("-"):
                order.append(f"{self._resolve_column(f[1:])} DESC")
            else:
                order.append(f"{self._resolve_column(f)} ASC")
        qs._order = order
        return qs

    def select_related(self, *fields) -> "QuerySet":
        """Load the rows these ForeignKeys point at in the same query.

        ``Book.objects.select_related("author")`` runs one ``LEFT JOIN`` per
        ForeignKey and sets ``book.author`` to an ``Author`` instance
        (``book.author_id`` keeps the id). Follow further ForeignKeys with
        ``__``: ``select_related("author__publisher")``. A NULL ForeignKey, or
        one whose row does not exist, leaves the id in place. Accumulates over
        calls; ``select_related(None)`` clears it.

        ``to_dict()`` and the JSON response paths are unchanged (they write
        ``author_id``), so a returned queryset skips the JOIN.
        """
        qs = self._clone()
        if fields == (None,):
            qs._related = ()
            return qs
        if not fields:
            raise TypeError("select_related() needs ForeignKey names, e.g. select_related('author').")
        paths = list(self._related)
        for path in fields:
            model = self.model
            for part in _split_path(path, "select_related"):
                field = _forward_fk(model, part)
                if field is None:
                    hint = (
                        " It is a reverse accessor; use prefetch_related()."
                        if _reverse_of(model, part) is not None else ""
                    )
                    raise FieldError(
                        f"select_related(): {model.__name__} has no ForeignKey {part!r}.{hint}"
                    )
                model = field.to
            if path not in paths:
                paths.append(path)
        qs._related = tuple(paths)
        return qs

    def prefetch_related(self, *names) -> "QuerySet":
        """Load related rows with one extra query per relation.

        Each name is a ForeignKey (``"author"``: sets ``book.author`` to the
        instance) or a reverse accessor (``"book_set"`` / a ``related_name``:
        ``author.book_set`` then holds the loaded rows, so ``await
        author.book_set`` runs no query). The extra query is
        ``WHERE <key> = ANY($1)`` with all the keys of the batch. Chain with
        ``__`` (``"book_set__publisher"``). Accumulates over calls;
        ``prefetch_related(None)`` clears it.
        """
        qs = self._clone()
        if names == (None,):
            qs._prefetch = ()
            return qs
        paths = list(self._prefetch)
        for path in names:
            model = self.model
            for part in _split_path(path, "prefetch_related"):
                field = _forward_fk(model, part)
                if field is not None:
                    model = field.to
                    continue
                rev = _reverse_of(model, part)
                if rev is None:
                    raise FieldError(
                        f"prefetch_related(): {model.__name__} has no ForeignKey or "
                        f"reverse accessor {part!r}."
                    )
                model = rev[0]
            if path not in paths:
                paths.append(path)
        qs._prefetch = tuple(paths)
        return qs

    def limit(self, n: int) -> "QuerySet":
        qs = self._clone()
        qs._limit = n
        return qs

    def offset(self, n: int) -> "QuerySet":
        qs = self._clone()
        qs._offset = n
        return qs

    # -- SQL assembly (converts ``?`` placeholders to $1, $2, ...) --------------

    def _where_sql(self, start_index: int = 1):
        if not self._where:
            return "", [], start_index
        clauses, params = [], []
        idx = start_index
        for frag, frag_params in self._where:
            if frag_params:
                parts = frag.split("?")
                out = [parts[0]]
                for part in parts[1:]:
                    out.append(f"${idx}")
                    out.append(part)
                    idx += 1
                clauses.append("".join(out))
                params.extend(frag_params)
            else:
                clauses.append(frag)
        return " WHERE " + " AND ".join(clauses), params, idx

    def _slice_sql(self) -> str:
        sql = ""
        if self._limit is not None:
            sql += f" LIMIT {int(self._limit)}"
        if self._offset is not None:
            sql += f" OFFSET {int(self._offset)}"
        return sql

    def _select_sql(self, columns: str = "*"):
        where_sql, params, _ = self._where_sql()
        sql = f"SELECT {columns} FROM {self.model._meta.quoted_table}{where_sql}"
        if self._order:
            sql += " ORDER BY " + ", ".join(self._order)
        return sql + self._slice_sql(), params

    def _check_unsliced(self, op: str) -> None:
        if self._limit is not None or self._offset is not None:
            raise TypeError(
                f"cannot {op} a sliced queryset: limit()/offset() would be "
                f"ignored and every matching row affected. Filter on the primary "
                f"key instead, e.g. .filter(id__in=[...]).{op}(...)."
            )

    # -- Terminal operations (async) --------------------------------------------

    async def all(self) -> list:
        if self._related or self._prefetch or self._cache is not None:
            return await self._all_related()
        db = _require_db(self._db)
        return await self._rows(db)

    async def _rows(self, db) -> list:
        """The base query's instances (no relations)."""
        sql, params = self._select_sql()
        model = self.model
        fetch_models = getattr(db, "_fetch_models", None)
        if fetch_models is not None and model._from_row.__func__ is _BASE_FROM_ROW:
            # Native hydration: instances are built straight from the wire rows
            # (Model._from_row semantics, no intermediate row dicts).
            return await fetch_models(model, sql, *params)
        rows = await db.fetch(sql, *params)
        from_row = model._from_row
        return [from_row(r) for r in rows]

    async def _all_related(self) -> list:
        if self._cache is not None:
            return list(self._cache)
        db = _require_db(self._db)
        if self._related:
            objs = await self._rows_joined(db)
        else:
            objs = await self._rows(db)
        for path in self._prefetch:
            await _prefetch(db, self._db, self.model, objs, path.split("__"))
        return objs

    def _join_plan(self):
        """``select_related`` as join nodes ``(model, parent index, attribute,
        ForeignKey)``; node 0 is this queryset's model."""
        nodes = [(self.model, None, None, None)]
        index = {(): 0}
        for path in self._related:
            key = ()
            model = self.model
            for part in path.split("__"):
                parent = index[key]
                key += (part,)
                field = _forward_fk(model, part)
                if key not in index:
                    index[key] = len(nodes)
                    nodes.append((field.to, parent, part, field))
                model = field.to
        return nodes

    def _joined_sql(self, nodes):
        """The base query as a subquery, ``LEFT JOIN``ed to each related table.
        Each model's columns are a segment; segments are separated by a
        ``NULL AS "__cello_join__"`` column (see ``src/db/rows.rs``)."""
        base_sql, params = self._select_sql()
        cols = ['"_c0".*']
        joins = []
        for k, (model, parent, attr, field) in enumerate(nodes[1:], 1):
            meta = model._meta
            pmeta = nodes[parent][0]._meta
            cols.append(f'NULL AS "{_JOIN_SEPARATOR}"')
            cols.append(f'"_c{k}".*')
            joins.append(
                f' LEFT JOIN {meta.quoted_table} AS "_c{k}" ON '
                f'"_c{k}".{meta.quoted_pk} = "_c{parent}".{pmeta.quoted_columns[attr]}'
            )
        sql = f'SELECT {", ".join(cols)} FROM ({base_sql}) AS "_c0"{"".join(joins)}'
        if self._order:
            sql += " ORDER BY " + ", ".join('"_c0".' + o for o in self._order)
        return sql, params

    async def _rows_joined(self, db) -> list:
        nodes = self._join_plan()
        fetch_related = getattr(db, "_fetch_related", None)
        if fetch_related is not None and all(
            n[0]._from_row.__func__ is _BASE_FROM_ROW for n in nodes
        ):
            sql, params = self._joined_sql(nodes)
            plan = [(m, p, a) for m, p, a, _ in nodes]
            return await fetch_related(plan, sql, *params)
        # Not a native database, or a model overrides _from_row: same result
        # with one query per relation (prefetch-style), through _from_row.
        objs = await self._rows(db)
        level = {0: objs}
        for k, (model, parent, attr, field) in enumerate(nodes[1:], 1):
            level[k] = await _attach_forward(db, nodes[parent][0], level[parent], attr, field)
        return objs

    def json(self):
        """Run the query and return its rows as one pre-serialized JSON array.

        ``await qs.json()`` gives a :class:`cello.RawJSON`. Returned from a
        handler (alone or anywhere inside a dict / list), it is written into the
        response as-is. The bytes are exactly those of serializing
        ``[u.to_dict() for u in await qs]``, but on a native ``Database`` /
        ``Transaction`` the rows are encoded to JSON straight from the Postgres
        wire format, off the GIL, without a Python object per row or value.

        Returning the queryset itself (``return {"users": qs}``) does the same
        after the handler returns; use ``json()`` when the query must run inside
        the handler (an open transaction, or to catch its errors).
        """
        if self._cache is not None:
            return self._json_from_instances()
        db = _require_db(self._db)
        model = self.model
        fetch_json = getattr(db, "_fetch_json", None)
        # select_related / prefetch_related never change to_dict() (it writes
        # the ForeignKey id), so the native path runs the base query alone.
        if fetch_json is not None and _native_json(model):
            sql, params = self._select_sql()
            return fetch_json(model._meta.schema, sql, *params)
        return self._json_from_instances()

    async def _json_from_instances(self):
        return _RawJSON.encode([r.to_dict() for r in await self.all()])

    def __await__(self):
        return self.all().__await__()

    def values(self, *fields) -> "ValuesQuerySet":
        """Matching rows as plain dicts: ``await qs.values(...)``.

        Without arguments each dict is ``to_dict()``. With field names, only
        those columns are selected and each dict has exactly those keys; a
        ForeignKey can be named either way (``"author"`` or ``"author_id"``)
        and gives the id. The result is a queryset of dicts: chain
        ``annotate()`` for GROUP BY rows, ``order_by()``, ``limit()``, ...,
        or return it from a handler (written natively, like a queryset).
        """
        column_of = self.model._meta.column_of
        quoted, pairs = [], []
        for f in fields:
            quoted.append(self._resolve_column(f))
            pairs.append((f, column_of[f]))
        qs = object.__new__(ValuesQuerySet)
        qs.__dict__.update(self.__dict__)
        qs._where = list(self._where)
        qs._order = list(self._order)
        qs._cache = None
        qs._fields = tuple(fields) if fields else None
        qs._quoted = tuple(quoted)
        qs._pairs = tuple(pairs)
        qs._annotations = {}
        return qs

    async def aggregate(self, *args, **kwargs) -> dict:
        """Compute aggregates over the matching rows; returns ``{alias: value}``.

        ``await Book.objects.filter(active=True).aggregate(total=Sum("price"),
        n=Count("id"))``. A positional aggregate is named ``<field>__<function>``
        (``Sum("price")`` → ``"price__sum"``; ``Count()`` → ``"count"``).
        ``limit()`` / ``offset()`` / ``order_by()`` select the rows aggregated.
        """
        aggs = _aggregates(args, kwargs)
        if not aggs:
            return {}
        if getattr(self, "_annotations", None):
            raise TypeError("aggregate() over annotate() groups is not supported.")
        exprs = [f"{a._sql(self)} AS {_alias(alias)}" for alias, a in aggs.items()]
        db = _require_db(self._db)
        if self._limit is None and self._offset is None:
            where_sql, params, _ = self._where_sql()
            source = f"{self.model._meta.quoted_table}{where_sql}"
        else:
            sub, params = QuerySet._select_sql(self)
            source = f"({sub}) AS _cello_sub"
        row = await db.fetchrow(f'SELECT {", ".join(exprs)} FROM {source}', *params)
        return {alias: row.get(alias) for alias in aggs}

    async def first(self):
        rows = await self.limit(1).all()
        return rows[0] if rows else None

    async def get(self, *args, **kwargs):
        qs = self.filter(*args, **kwargs) if args or kwargs else self
        rows = await qs.limit(2).all()
        if not rows:
            raise self.model.DoesNotExist(
                f"{self.model.__name__} matching query does not exist."
            )
        if len(rows) > 1:
            raise self.model.MultipleObjectsReturned(
                f"get() returned more than one {self.model.__name__}."
            )
        return rows[0]

    def _bounded_sql(self):
        """``FROM ...`` for count/exists, honouring limit/offset via a subquery."""
        where_sql, params, _ = self._where_sql()
        table = self.model._meta.quoted_table
        if self._limit is None and self._offset is None:
            return f"{table}{where_sql}", params
        return f"(SELECT 1 FROM {table}{where_sql}{self._slice_sql()}) AS _cello_sub", params

    async def count(self) -> int:
        """Number of matching rows (after ``limit()`` / ``offset()``)."""
        db = _require_db(self._db)
        source, params = self._bounded_sql()
        row = await db.fetchrow(f"SELECT count(*) AS n FROM {source}", *params)
        return int(row["n"])

    async def exists(self) -> bool:
        """Whether any row matches (after ``limit()`` / ``offset()``)."""
        db = _require_db(self._db)
        source, params = self._bounded_sql()
        row = await db.fetchrow(f"SELECT EXISTS (SELECT 1 FROM {source}) AS n", *params)
        return bool(row["n"])

    async def delete(self) -> int:
        self._check_unsliced("delete")
        db = _require_db(self._db)
        where_sql, params, _ = self._where_sql()
        sql = f"DELETE FROM {self.model._meta.quoted_table}{where_sql}"
        return await db.execute(sql, *params)

    async def update(self, **kwargs) -> int:
        """Set columns on every matching row; returns the number of rows.

        ``update()`` with no fields returns ``0`` without a query.
        """
        self._check_unsliced("update")
        sets, params = [], []
        for name, value in kwargs.items():
            column = self._resolve_column(name)
            params.append(_coerce(self.model, name, value))
            sets.append(f"{column} = ${len(params)}")
        if not sets:
            return 0
        db = _require_db(self._db)
        where_sql, where_params, _ = self._where_sql(start_index=len(params) + 1)
        params.extend(where_params)
        sql = f'UPDATE {self.model._meta.quoted_table} SET {", ".join(sets)}{where_sql}'
        return await db.execute(sql, *params)


class ValuesQuerySet(QuerySet):
    """``qs.values(...)``: a queryset whose rows are dicts.

    ``await`` it for a list of dicts. ``annotate(alias=Aggregate(...))`` groups
    by the named fields (``GROUP BY``) and adds one aggregate column per alias;
    ``order_by()`` then accepts the aliases too. Returned from a handler (or
    via ``json()``) the dicts are encoded natively, with the same bytes as
    serializing the awaited list.
    """

    def annotate(self, *args, **kwargs) -> "ValuesQuerySet":
        """Group by the ``values()`` fields and add aggregate columns::

            await Book.objects.values("category").annotate(n=Count("id")).order_by("-n")
            # [{"category": "sci-fi", "n": 12}, ...]

        Aliases follow the identifier rule and may not repeat a ``values()``
        key. Each row's keys are the ``values()`` fields, then the aliases.
        """
        if not self._fields:
            raise TypeError(
                "annotate() needs the fields to group by: "
                "qs.values('category').annotate(n=Count('id')). "
                "Use aggregate() for totals over the whole queryset."
            )
        aggs = _aggregates(args, kwargs)
        for alias, a in aggs.items():
            _alias(alias)
            if alias in self._fields or alias in self._annotations:
                raise ValueError(f"annotate(): alias {alias!r} is already a key of the rows.")
            a._sql(self)  # resolve the field now: errors before any SQL
        qs = self._clone()
        qs._annotations = {**self._annotations, **aggs}
        return qs

    def order_by(self, *fields) -> "ValuesQuerySet":
        """As ``QuerySet.order_by()``; an ``annotate()`` alias may be named."""
        ann = self._annotations
        if not ann:
            return QuerySet.order_by(self, *fields)
        plain, order = [], []
        for f in fields:
            name = f[1:] if isinstance(f, str) and f.startswith("-") else f
            if isinstance(name, str) and name in ann:
                order.append(f'{_alias(name)} {"DESC" if f.startswith("-") else "ASC"}')
            else:
                order.append(None)
                plain.append(f)
        resolved = iter(QuerySet.order_by(self, *plain)._order)
        qs = self._clone()
        qs._order = [o if o is not None else next(resolved) for o in order]
        return qs

    def _grouped_sql(self):
        keys = [f"{q} AS {_alias(k)}" for q, (k, _) in zip(self._quoted, self._pairs)]
        aggs = [f"{a._sql(self)} AS {_alias(alias)}" for alias, a in self._annotations.items()]
        where_sql, params, _ = self._where_sql()
        sql = (
            f'SELECT {", ".join(keys + aggs)} FROM {self.model._meta.quoted_table}'
            f'{where_sql} GROUP BY {", ".join(dict.fromkeys(self._quoted))}'
        )
        if self._order:
            sql += " ORDER BY " + ", ".join(self._order)
        return sql + self._slice_sql(), params

    async def all(self) -> list:
        if self._fields is None:
            return [r.to_dict() for r in await QuerySet.all(self)]
        db = _require_db(self._db)
        if self._annotations:
            sql, params = self._grouped_sql()
            return await db.fetch(sql, *params)
        sql, params = self._select_sql(", ".join(self._quoted))
        rows = await db.fetch(sql, *params)
        pairs = self._pairs
        return [{k: row.get(c) for k, c in pairs} for row in rows]

    def json(self):
        if self._fields is None:
            return QuerySet.json(self)
        db = _require_db(self._db)
        fetch_json = getattr(db, "_fetch_json", None)
        if fetch_json is None:
            return self._json_from_rows()
        if self._annotations:
            sql, params = self._grouped_sql()
        else:
            # Each key aliased to its name: the column-keyed native encoder
            # then writes exactly the dicts all() builds.
            sql, params = self._select_sql(", ".join(
                f"{q} AS {_alias(k)}" for q, (k, _) in zip(self._quoted, self._pairs)
            ))
        return fetch_json(None, sql, *params)

    async def _json_from_rows(self):
        return _RawJSON.encode(await self.all())

    _json_from_instances = _json_from_rows

    def _bounded_sql(self):
        if not self._annotations:
            return QuerySet._bounded_sql(self)
        sql, params = self._grouped_sql()
        return f"({sql}) AS _cello_sub", params


_JOIN_SEPARATOR = "__cello_join__"
_object_new = object.__new__


async def _attach_forward(db, model, objs, name, field) -> list:
    """Set ``obj.<name>`` to the related instance for each of ``objs`` (one
    ``WHERE pk = ANY($1)`` query). Objects whose id is NULL or matches no row
    keep the id. Returns the distinct related instances."""
    col = field.column
    ids = list(dict.fromkeys(
        v for v in (o.__dict__.get(col) for o in objs) if v is not None
    ))
    if not ids:
        return []
    target = field.to
    qs = QuerySet(target, db)
    qs._where = [(f"{target._meta.quoted_pk} = ANY(?)", [ids])]
    related = await qs._rows(db)
    pk_name = target._meta.pk_name
    by_pk = {r.__dict__.get(pk_name): r for r in related}
    for o in objs:
        r = by_pk.get(o.__dict__.get(col))
        if r is not None:
            o.__dict__[name] = r
    return list(by_pk.values())


async def _prefetch(db, using, model, objs, parts) -> None:
    """Load ``parts[0]`` for ``objs`` with one query, then the rest of the
    path for the loaded rows."""
    if not objs:
        return
    name, rest = parts[0], parts[1:]
    field = _forward_fk(model, name)
    if field is not None:
        children = await _attach_forward(db, model, objs, name, field)
        child_model = field.to
    else:
        source, fname = _reverse_of(model, name)
        sfield = source._meta.fields[fname]
        col = sfield.column
        pk_name = model._meta.pk_name
        keys = list(dict.fromkeys(
            v for v in (o.__dict__.get(pk_name) for o in objs) if v is not None
        ))
        children = []
        if keys:
            qs = QuerySet(source, db)
            qs._where = [(f"{source._meta.quoted_columns[fname]} = ANY(?)", [keys])]
            if source._meta.pk_name is not None:
                qs._order = [f"{source._meta.quoted_pk} ASC"]
            children = await qs._rows(db)
        groups = {}
        for c in children:
            groups.setdefault(c.__dict__.get(col), []).append(c)
        attach = col != fname
        for o in objs:
            pk = o.__dict__.get(pk_name)
            if pk is None:
                continue
            rows = groups.get(pk, [])
            if attach:
                for c in rows:
                    c.__dict__[fname] = o
            cached = source.objects.get_queryset(using).filter(**{fname: o})
            cached._cache = rows
            o.__dict__[name] = cached
        child_model = source
    if rest:
        await _prefetch(db, using, child_model, children, rest)


def _batches(items, per_row, batch_size):
    """Split ``items`` so no statement exceeds the bind-parameter limit."""
    limit = max(1, _MAX_PARAMS // max(1, per_row))
    if batch_size is not None:
        if isinstance(batch_size, bool) or not isinstance(batch_size, int) or batch_size < 1:
            raise ValueError("batch_size must be a positive int or None.")
        limit = min(limit, batch_size)
    return [items[i:i + limit] for i in range(0, len(items), limit)]


async def _run_batches(db, batches, run):
    """Run ``run(db, batch)`` per batch; several batches on a pool run in
    one transaction so the operation is all-or-nothing."""
    out = []
    start = getattr(db, "transaction", None)
    if len(batches) > 1 and start is not None:
        async with start() as tx:
            for b in batches:
                out.append(await run(tx, b))
    else:
        for b in batches:
            out.append(await run(db, b))
    return out


def _check_instances(model, objs, op):
    for o in objs:
        if not isinstance(o, model):
            raise TypeError(f"{op}() expects {model.__name__} instances, got {o!r}.")


class Manager:
    """Entry point exposed as ``Model.objects``."""

    def __init__(self, model):
        self.model = model

    def get_queryset(self, using=None) -> QuerySet:
        return QuerySet(self.model, using)

    def using(self, database) -> QuerySet:
        return self.get_queryset(database)

    def all(self) -> QuerySet:
        return self.get_queryset()

    def filter(self, *args, **kwargs) -> QuerySet:
        return self.get_queryset().filter(*args, **kwargs)

    def exclude(self, *args, **kwargs) -> QuerySet:
        return self.get_queryset().exclude(*args, **kwargs)

    def order_by(self, *fields) -> QuerySet:
        return self.get_queryset().order_by(*fields)

    def select_related(self, *fields) -> QuerySet:
        return self.get_queryset().select_related(*fields)

    def prefetch_related(self, *names) -> QuerySet:
        return self.get_queryset().prefetch_related(*names)

    def values(self, *fields) -> "ValuesQuerySet":
        return self.get_queryset().values(*fields)

    async def aggregate(self, *args, **kwargs) -> dict:
        return await self.get_queryset().aggregate(*args, **kwargs)

    async def get(self, *args, **kwargs):
        return await self.get_queryset().get(*args, **kwargs)

    async def create(self, using=None, **kwargs):
        """Insert a row and return the populated model instance.

        Always an INSERT, also when a primary key value is given.
        """
        obj = self.model(**kwargs)
        return await obj._insert(_require_db(using))

    async def count(self) -> int:
        return await self.get_queryset().count()

    async def bulk_create(self, objs, batch_size: Optional[int] = None, using=None) -> list:
        """Insert many instances with one multi-row ``INSERT ... RETURNING *``
        per batch; returns ``objs`` with primary keys and database-generated
        values filled in, like ``save()``.

        Each row is written as ``create()`` writes it: field values as built
        (defaults were applied by the constructor), an ``AutoField`` / serial
        key left as ``None`` gets the database's value (``DEFAULT``),
        ``auto_now`` is ``now()`` and ``auto_now_add`` is ``now()`` when unset.
        Batches are cut to stay under Postgres's 65,535 bind parameters
        (``batch_size`` lowers it further). Several batches on a pool run in
        one transaction. Rows come back in input order.
        """
        model = self.model
        objs = list(objs)
        _check_instances(model, objs, "bulk_create")
        if not objs:
            return objs
        db = _require_db(using)
        meta = model._meta
        plan = meta.write_plan
        cells = []
        for o in objs:
            row = []
            for name, field, quoted, mode in plan:
                if mode == _W_NOW:
                    row.append(_NOW)
                    continue
                value = _field_value(o, name, field)
                if mode == _W_NOW_ADD and value is None:
                    row.append(_NOW)
                elif mode == _W_SERIAL and value is None:
                    row.append(_DEFAULT)
                else:
                    row.append(value)
            cells.append(row)
        # A serial column nobody set is left out entirely.
        keep = [
            i for i, (name, field, quoted, mode) in enumerate(plan)
            if mode != _W_SERIAL or any(r[i] is not _DEFAULT for r in cells)
        ]
        if not keep:
            keep = [i for i, p in enumerate(plan) if p[3] == _W_SERIAL][:1]
        if not keep:  # a model without columns
            for o in objs:
                await o._insert(db)
            return objs
        cols = ", ".join(plan[i][2] for i in keep)
        head = f"INSERT INTO {meta.quoted_table} ({cols}) VALUES "
        pairs = list(zip(objs, cells))

        async def run(conn, batch):
            params, groups = [], []
            for _, row in batch:
                marks = []
                for i in keep:
                    v = row[i]
                    if v is _NOW:
                        marks.append("now()")
                    elif v is _DEFAULT:
                        marks.append("DEFAULT")
                    else:
                        params.append(v)
                        marks.append(f"${len(params)}")
                groups.append(f'({", ".join(marks)})')
            rows = await conn.fetch(head + ", ".join(groups) + " RETURNING *", *params)
            if len(rows) != len(batch):
                raise RuntimeError(
                    f"bulk_create: INSERT returned {len(rows)} rows for {len(batch)}."
                )
            for (o, _), r in zip(batch, rows):
                o._load_row(r)

        await _run_batches(db, _batches(pairs, len(keep), batch_size), run)
        return objs

    async def bulk_update(self, objs, fields, batch_size: Optional[int] = None,
                          using=None) -> int:
        """Write ``fields`` of many saved instances with one
        ``UPDATE ... FROM (VALUES ...)`` per batch; returns the number of rows
        updated.

        Only the named fields are written (plus ``auto_now`` fields, set to
        ``now()`` and read back, as ``save()`` does). Every instance needs a
        primary key, and each row may appear once. Parameters are cast to the
        column types from the registry. Batches stay under the 65,535
        bind-parameter limit; several batches on a pool run in one transaction.
        """
        model = self.model
        meta = model._meta
        if meta.pk_name is None:
            raise ValueError(f"{model.__name__} has no primary key; bulk_update() needs one.")
        if isinstance(fields, str):
            raise TypeError("bulk_update() fields must be a list of field names, not a str.")
        names = []
        for f in fields:
            try:
                field = meta.field_of.get(f)
            except TypeError:
                field = None
            if field is None:
                _resolve_slow(model, f)
            if field.name == meta.pk_name:
                raise FieldError("bulk_update() cannot update the primary key.")
            if field.name not in names:
                names.append(field.name)
        if not names:
            raise ValueError("bulk_update() needs at least one field name.")
        objs = list(objs)
        _check_instances(model, objs, "bulk_update")
        if not objs:
            return 0
        pk_name = meta.pk_name
        seen = set()
        for o in objs:
            pk = getattr(o, pk_name, None)
            if pk is None:
                raise ValueError(f"bulk_update(): a {model.__name__} instance has no primary key.")
            if pk in seen:
                raise ValueError(f"bulk_update(): primary key {pk!r} appears more than once.")
            seen.add(pk)
        db = _require_db(using)
        upd = [n for n in names if n not in meta.auto_now]
        casts = [_cast_type(model, pk_name)] + [_cast_type(model, n) for n in upd]
        cols = ", ".join(f'"c{i}"' for i in range(len(casts)))
        sets = [f'{meta.quoted_columns[n]} = "_v"."c{i}"' for i, n in enumerate(upd, 1)]
        sets += [f"{meta.quoted_columns[n]} = now()" for n in meta.auto_now]
        table, qpk = meta.quoted_table, meta.quoted_pk
        returning = ", ".join([f"{table}.{qpk}"] + [
            f"{table}.{meta.quoted_columns[n]}" for n in meta.auto_now
        ])
        fields_of = [(n, meta.fields[n]) for n in upd]

        async def run(conn, batch):
            params, groups = [], []
            for o in batch:
                marks = []
                values = [getattr(o, pk_name)] + [_field_value(o, n, f) for n, f in fields_of]
                for v, t in zip(values, casts):
                    params.append(v)
                    marks.append(f"${len(params)}::{t}")
                groups.append(f'({", ".join(marks)})')
            sql = (
                f'UPDATE {table} SET {", ".join(sets)} FROM (VALUES {", ".join(groups)}) '
                f'AS "_v" ({cols}) WHERE {table}.{qpk} = "_v"."c0"'
            )
            if not meta.auto_now:
                return await conn.execute(sql, *params)
            rows = await conn.fetch(sql + " RETURNING " + returning, *params)
            by_pk = {o.__dict__.get(pk_name): o for o in batch}
            for r in rows:
                o = by_pk.get(r.get(meta.pk_column))
                if o is not None:
                    for n in meta.auto_now:
                        o.__dict__[n] = r.get(meta.fields[n].column)
            return len(rows)

        counts = await _run_batches(db, _batches(objs, len(casts), batch_size), run)
        return sum(counts)


# ── Native serializer hooks (1.4.0 Phase 02) ─────────────────────────────────

_BASE_TO_DICT = Model.to_dict
_BASE_FROM_ROW = Model._from_row.__func__

# Lets the response serializer recognise model instances (written as their
# to_dict()) and querysets (run, then written as JSON rows) in handler results.
_orm_bind(Model, QuerySet, _BASE_TO_DICT, _run_querysets)
