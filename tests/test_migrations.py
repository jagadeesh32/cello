"""
Live-Postgres tests for the experimental ``cello migrate`` (v1.4.0 Phase 05).

Every test runs in its own Postgres schema (``search_path`` set through the
DSN), so the bookkeeping table, advisory lock users and model tables of one
test never see another's. The schema is dropped afterwards.

Configure with ``CELLO_TEST_DATABASE_URL`` (see ``tests/orm_compat_support.py``).
"""

import os
import subprocess
import sys
import textwrap
import threading
import uuid

import pytest

from cello._cello import MigrationConnection
from cello.migrate import LOCK_KEY, MigrationError, Migrator, load_files, main, parse_body
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

from orm_compat_support import DSN, PG_AVAILABLE

pytestmark = pytest.mark.skipif(not PG_AVAILABLE, reason="PostgreSQL not reachable")


def _with_search_path(dsn: str, schema: str) -> str:
    sep = "&" if "?" in dsn else "?"
    return f"{dsn}{sep}options=-c%20search_path%3D{schema}"


@pytest.fixture
def env(tmp_path):
    schema = "cmig_" + uuid.uuid4().hex[:10]
    admin = MigrationConnection(DSN)
    admin.execute(f'CREATE SCHEMA "{schema}"')
    dsn = _with_search_path(DSN, schema)
    conn = MigrationConnection(dsn)
    try:
        yield dsn, tmp_path / "migrations", conn
    finally:
        conn.close()
        admin.execute(f'DROP SCHEMA "{schema}" CASCADE')
        admin.close()


def model(_cls_name, _table, **fields):
    """Define a model class (a fresh class each call, same table)."""
    ns = dict(fields)
    ns["Meta"] = type("Meta", (), {"table_name": _table})
    ns["__module__"] = __name__
    return type(_cls_name, (Model,), ns)


def cols(conn, table):
    t = conn.introspect([table]).get(table)
    return {c["name"]: c for c in t["columns"]} if t else None


def cons(conn, table):
    return conn.introspect([table])[table]["constraints"]


def q(conn, sql, *params):
    return conn.query(sql, [None if p is None else str(p) for p in params])


def make_apply(mig, **kw):
    path = mig.make(**kw)
    applied = mig.apply(allow_destructive=kw.get("allow_destructive", False))
    return path, applied


# ── Create / add / alter ─────────────────────────────────────────────────────


def test_empty_to_create_orders_foreign_keys(env):
    dsn, d, conn = env
    Author = model("Author", "author", id=AutoField(), name=CharField(max_length=100))
    Book = model(
        "Book",
        "book",
        id=AutoField(),
        title=CharField(max_length=200),
        author=ForeignKey(Author, on_delete="CASCADE"),
    )
    mig = Migrator(dsn, [Book, Author], d)  # order given does not matter
    path, applied = make_apply(mig)
    assert path.name == "0001_initial.sql"
    assert applied == ["0001_initial"]
    text = path.read_text()
    assert text.index('CREATE TABLE "author"') < text.index('CREATE TABLE "book"')
    assert Book._meta.schema.create_table_sql(False) + ";" in text
    assert "BEGIN;" in text and "COMMIT;" in text
    assert cols(conn, "book")["author_id"]["type"] == "integer"
    fk = [c for c in cons(conn, "book") if c["kind"] == "foreign"]
    assert fk[0]["ref_table"] == "author" and fk[0]["on_delete"] == "CASCADE"
    rows = q(conn, 'SELECT name, checksum FROM "cello_migrations"')
    assert rows == [["0001_initial", load_files(d)[0].checksum]]


def test_add_nullable_column(env):
    dsn, d, conn = env
    V1 = model("Item", "item", id=AutoField(), name=TextField())
    make_apply(Migrator(dsn, [V1], d))
    conn.execute("INSERT INTO item (name) VALUES ('a')")
    V2 = model("Item", "item", id=AutoField(), name=TextField(), note=TextField(null=True))
    path, _ = make_apply(Migrator(dsn, [V2], d))
    assert 'ALTER TABLE "item" ADD COLUMN "note" TEXT;' in path.read_text()
    c = cols(conn, "item")["note"]
    assert c["type"] == "text" and c["not_null"] is False
    assert q(conn, "SELECT note FROM item") == [[None]]


def test_add_not_null_column_with_default_fills_rows(env):
    dsn, d, conn = env
    V1 = model("Item", "item", id=AutoField(), name=TextField())
    make_apply(Migrator(dsn, [V1], d))
    conn.execute("INSERT INTO item (name) VALUES ('a'), ('b')")
    V2 = model(
        "Item",
        "item",
        id=AutoField(),
        name=TextField(),
        qty=IntegerField(default=7),
        active=BooleanField(default=True),
    )
    make_apply(Migrator(dsn, [V2], d))
    assert q(conn, "SELECT qty::text, active::text FROM item ORDER BY id") == [
        ["7", "true"],
        ["7", "true"],
    ]
    assert cols(conn, "item")["qty"]["not_null"] is True


def test_add_not_null_without_default_fails_and_rolls_back(env):
    dsn, d, conn = env
    V1 = model("Item", "item", id=AutoField(), name=TextField())
    make_apply(Migrator(dsn, [V1], d))
    conn.execute("INSERT INTO item (name) VALUES ('a')")
    V2 = model("Item", "item", id=AutoField(), name=TextField(), extra=TextField(null=True), req=TextField())
    mig = Migrator(dsn, [V2], d)
    path = mig.make()
    assert "fails if the table already has rows" in path.read_text()
    with pytest.raises(MigrationError, match="rolled back"):
        mig.apply()
    # Nothing from the failed file stuck, and it is still pending.
    assert set(cols(conn, "item")) == {"id", "name"}
    assert mig.status()["pending"] == [path.stem]


def test_type_changes(env):
    dsn, d, conn = env
    V1 = model("T", "t", id=AutoField(), code=CharField(max_length=10), n=IntegerField(), s=TextField(), v=CharField(max_length=20, default="x"))
    make_apply(Migrator(dsn, [V1], d))
    conn.execute("INSERT INTO t (code, n, s) VALUES ('abc', 5, '12')")
    V2 = model("T", "t", id=AutoField(), code=CharField(max_length=50), n=BigIntegerField(), s=TextField(), v=TextField(default="y"))
    path, _ = make_apply(Migrator(dsn, [V2], d))
    text = path.read_text()
    assert 'ALTER COLUMN "code" TYPE character varying(50);' in text
    assert 'ALTER COLUMN "n" TYPE bigint;' in text
    # The default is dropped around the type change and set to the declared one.
    assert text.index('"v" DROP DEFAULT') < text.index('"v" TYPE text') < text.index('"v" SET DEFAULT \'y\'')
    c = cols(conn, "t")
    assert c["code"]["type"] == "character varying(50)"
    assert c["n"]["type"] == "bigint"
    assert c["v"]["default"] == "'y'::text"
    assert q(conn, "SELECT code, n::text FROM t") == [["abc", "5"]]
    assert Migrator(dsn, [V2], d).make() is None

    # text -> integer has no lossless cast: flagged, written disabled.
    V3 = model("T", "t", id=AutoField(), code=CharField(max_length=50), n=BigIntegerField(), s=IntegerField(), v=TextField(default="y"))
    mig = Migrator(dsn, [V3], d)
    assert mig.make() is None  # the only change is destructive
    path = mig.make(allow_destructive=True)
    body, destructive = parse_body(load_files(d)[-1])
    assert destructive == ['ALTER TABLE "t" ALTER COLUMN "s" TYPE integer USING "s"::integer;']
    with pytest.raises(MigrationError, match="destructive"):
        mig.apply()
    mig.apply(allow_destructive=True)
    assert cols(conn, "t")["s"]["type"] == "integer"
    assert q(conn, "SELECT s::text FROM t") == [["12"]]


def test_narrowing_varchar_fails_instead_of_truncating(env):
    dsn, d, conn = env
    V1 = model("T", "t", id=AutoField(), code=CharField(max_length=20))
    make_apply(Migrator(dsn, [V1], d))
    conn.execute("INSERT INTO t (code) VALUES ('abcdefghij')")
    V2 = model("T", "t", id=AutoField(), code=CharField(max_length=5))
    mig = Migrator(dsn, [V2], d)
    text = mig.make().read_text()
    assert "USING" not in text  # an explicit ::varchar(5) would silently truncate
    with pytest.raises(MigrationError, match="rolled back"):
        mig.apply()
    assert q(conn, "SELECT code FROM t") == [["abcdefghij"]]


def test_nullability_and_defaults(env):
    dsn, d, conn = env
    V1 = model("T", "t", id=AutoField(), a=TextField(null=True), b=TextField(), c=IntegerField(default=1))
    make_apply(Migrator(dsn, [V1], d))
    V2 = model("T", "t", id=AutoField(), a=TextField(), b=TextField(null=True), c=IntegerField())
    path, _ = make_apply(Migrator(dsn, [V2], d))
    text = path.read_text()
    assert '"a" SET NOT NULL' in text and '"b" DROP NOT NULL' in text and '"c" DROP DEFAULT' in text
    c = cols(conn, "t")
    assert c["a"]["not_null"] and not c["b"]["not_null"] and c["c"]["default"] is None
    assert Migrator(dsn, [V2], d).make() is None


def test_foreign_key_add_and_change(env):
    dsn, d, conn = env
    A = model("A", "a", id=AutoField())
    B1 = model("B", "b", id=AutoField(), name=TextField())
    make_apply(Migrator(dsn, [A, B1], d))
    B2 = model("B", "b", id=AutoField(), name=TextField(), a=ForeignKey(A, null=True))
    make_apply(Migrator(dsn, [A, B2], d))
    fks = [c for c in cons(conn, "b") if c["kind"] == "foreign"]
    assert [(f["columns"], f["ref_table"], f["on_delete"]) for f in fks] == [
        (["a_id"], "a", "CASCADE")
    ]
    B3 = model("B", "b", id=AutoField(), name=TextField(), a=ForeignKey(A, null=True, on_delete="SET NULL"))
    path, _ = make_apply(Migrator(dsn, [A, B3], d))
    text = path.read_text()
    assert text.index("DROP CONSTRAINT") < text.index("ADD FOREIGN KEY")
    fks = [c for c in cons(conn, "b") if c["kind"] == "foreign"]
    assert [f["on_delete"] for f in fks] == ["SET NULL"]
    # Removing the relation (keeping a plain integer column) drops the FK only.
    B4 = model("B", "b", id=AutoField(), name=TextField(), a_id=IntegerField(null=True))
    make_apply(Migrator(dsn, [A, B4], d))
    assert [c for c in cons(conn, "b") if c["kind"] == "foreign"] == []
    assert "a_id" in cols(conn, "b")


def test_unique_add_and_drop(env):
    dsn, d, conn = env
    V1 = model("T", "t", id=AutoField(), email=TextField())
    make_apply(Migrator(dsn, [V1], d))
    V2 = model("T", "t", id=AutoField(), email=TextField(unique=True))
    path, _ = make_apply(Migrator(dsn, [V2], d))
    assert 'ALTER TABLE "t" ADD UNIQUE ("email");' in path.read_text()
    assert [c["columns"] for c in cons(conn, "t") if c["kind"] == "unique"] == [["email"]]
    V3 = model("T", "t", id=AutoField(), email=TextField())
    path, _ = make_apply(Migrator(dsn, [V3], d))
    assert "DROP CONSTRAINT" in path.read_text()
    assert [c for c in cons(conn, "t") if c["kind"] == "unique"] == []


def test_unique_add_with_duplicates_rolls_back(env):
    dsn, d, conn = env
    V1 = model("T", "t", id=AutoField(), email=TextField())
    make_apply(Migrator(dsn, [V1], d))
    conn.execute("INSERT INTO t (email) VALUES ('x'), ('x')")
    mig = Migrator(dsn, [model("T", "t", id=AutoField(), email=TextField(unique=True))], d)
    mig.make()
    with pytest.raises(MigrationError, match="rolled back"):
        mig.apply()


# ── Destructive changes ──────────────────────────────────────────────────────


def test_drop_column_is_guarded(env):
    dsn, d, conn = env
    V1 = model("T", "t", id=AutoField(), keep=TextField(), old=TextField(null=True))
    make_apply(Migrator(dsn, [V1], d))
    conn.execute("INSERT INTO t (keep, old) VALUES ('k', 'precious')")
    V2 = model("T", "t", id=AutoField(), keep=TextField())
    mig = Migrator(dsn, [V2], d)
    # Only a destructive change: nothing is written without the flag.
    assert mig.make() is None
    assert load_files(d)[-1].name == "0001_initial"
    path = mig.make(allow_destructive=True)
    assert '\nALTER TABLE "t" DROP COLUMN "old";' in path.read_text()
    with pytest.raises(MigrationError, match="--allow-destructive"):
        mig.apply()
    assert "old" in cols(conn, "t")
    mig.apply(allow_destructive=True)
    assert "old" not in cols(conn, "t")


def test_destructive_commented_out_beside_safe_changes(env):
    dsn, d, conn = env
    V1 = model("T", "t", id=AutoField(), old=TextField(null=True))
    make_apply(Migrator(dsn, [V1], d))
    V2 = model("T", "t", id=AutoField(), new=TextField(null=True))
    mig = Migrator(dsn, [V2], d)
    warnings = []
    path = mig.make(echo=warnings.append)
    text = path.read_text()
    assert '\n-- ALTER TABLE "t" DROP COLUMN "old";' in text
    assert "[disabled" in text
    assert any("Renames are NOT" in w for w in warnings)
    assert "WARNING:" in text and "RENAME COLUMN" in text
    mig.apply()  # the commented drop is not live, so no flag needed
    assert set(cols(conn, "t")) == {"id", "old", "new"}


def test_drop_table_for_removed_model(env):
    dsn, d, conn = env
    A = model("A", "a", id=AutoField())
    B = model("B", "b", id=AutoField(), a=ForeignKey(A))
    make_apply(Migrator(dsn, [A, B], d))
    mig = Migrator(dsn, [A], d)
    assert mig.make() is None
    assert cols(conn, "b") is not None
    # Still tracked; with the flag the drop is live.
    path = mig.make(allow_destructive=True)
    assert 'DROP TABLE "b";' in path.read_text()
    mig.apply(allow_destructive=True)
    assert cols(conn, "b") is None and cols(conn, "a") is not None
    assert Migrator(dsn, [A], d).make(allow_destructive=True) is None


def test_gated_table_drop_stays_tracked(env):
    dsn, d, conn = env
    A = model("A", "a", id=AutoField())
    B = model("B", "b", id=AutoField())
    make_apply(Migrator(dsn, [A, B], d))
    A2 = model("A", "a", id=AutoField(), x=TextField(null=True))
    path, _ = make_apply(Migrator(dsn, [A2], d))  # safe add + gated drop
    assert '-- DROP TABLE "b";' in path.read_text()
    path = Migrator(dsn, [A2], d).make(allow_destructive=True)
    assert path is not None and 'DROP TABLE "b";' in path.read_text()


# ── History, idempotence, locking ────────────────────────────────────────────


def test_idempotent_after_apply(env):
    dsn, d, conn = env
    A = model(
        "A",
        "a",
        id=AutoField(),
        name=CharField(max_length=40, unique=True),
        score=FloatField(default=1.5),
        flag=BooleanField(default=False),
        data=JSONField(default={"k": [1, 2]}),
        created=DateTimeField(auto_now_add=True),
        big=BigIntegerField(null=True, default=-3),
        label=TextField(default="it's"),
    )
    B = model("B", "b", id=AutoField(), a=ForeignKey(A, on_delete="RESTRICT", null=True), me=TextField(null=True))
    mig = Migrator(dsn, [A, B], d)
    make_apply(mig)
    assert mig.make() is None
    assert mig.sql() == ""
    st = mig.status()
    assert st["pending"] == [] and st["drift"] == [] and st["problems"] == []


def test_make_refuses_with_pending_files(env):
    dsn, d, conn = env
    mig = Migrator(dsn, [model("A", "a", id=AutoField())], d)
    mig.make()
    mig2 = Migrator(dsn, [model("A", "a", id=AutoField(), x=TextField(null=True))], d)
    with pytest.raises(MigrationError, match="not applied yet"):
        mig2.make()


def test_checksum_tamper_refused(env):
    dsn, d, conn = env
    mig = Migrator(dsn, [model("A", "a", id=AutoField())], d)
    path, _ = make_apply(mig)
    path.write_text(path.read_text().replace("COMMIT;", "-- sneaky\nCOMMIT;"))
    with pytest.raises(MigrationError, match="edited after it was applied"):
        mig.apply()
    with pytest.raises(MigrationError, match="edited after it was applied"):
        mig.make()
    assert mig.status()["problems"]
    # Line-ending changes alone are not tampering.
    original = path.read_text().replace("-- sneaky\n", "")
    path.write_bytes(original.replace("\n", "\r\n").encode())
    assert mig.apply() == []


def test_missing_applied_file_refused(env):
    dsn, d, conn = env
    mig = Migrator(dsn, [model("A", "a", id=AutoField())], d)
    path, _ = make_apply(mig)
    path.unlink()
    with pytest.raises(MigrationError, match="file is missing"):
        mig.apply()


def test_transaction_control_in_body_refused(env):
    dsn, d, conn = env
    mig = Migrator(dsn, [model("A", "a", id=AutoField())], d)
    path = mig.make()
    path.write_text(path.read_text().replace("BEGIN;\n", "BEGIN;\nEND;\n", 1))
    with pytest.raises(MigrationError, match="transaction control"):
        mig.apply()
    assert cols(conn, "a") is None


def test_lock_blocks_concurrent_apply(env):
    dsn, d, conn = env
    mig = Migrator(dsn, [model("A", "a", id=AutoField())], d)
    mig.make()
    holder = MigrationConnection(dsn)
    try:
        holder.query("SELECT pg_advisory_lock($1::text::bigint)::text", [str(LOCK_KEY)])
        with pytest.raises(MigrationError, match="holds the migration lock"):
            mig.apply(lock_timeout=0.3)
        assert cols(conn, "a") is None
    finally:
        holder.close()  # closing the session releases the lock
    assert mig.apply(lock_timeout=5) == ["0001_initial"]


def test_concurrent_applies_run_once(env):
    dsn, d, conn = env
    A = model("A", "a", id=AutoField())
    B = model("B", "b", id=AutoField(), a=ForeignKey(A))
    Migrator(dsn, [A, B], d).make()
    results, errors = [], []

    def run():
        try:
            results.append(Migrator(dsn, [A, B], d).apply(lock_timeout=30))
        except Exception as e:  # pragma: no cover - reported below
            errors.append(e)

    threads = [threading.Thread(target=run) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert errors == []
    assert sorted(results) == [[], [], [], ["0001_initial"]]
    assert q(conn, 'SELECT count(*)::text FROM "cello_migrations"') == [["1"]]


# ── Round trip ───────────────────────────────────────────────────────────────

_CANONICAL = {
    "SERIAL": "integer",
    "INTEGER": "integer",
    "BIGINT": "bigint",
    "DOUBLE PRECISION": "double precision",
    "BOOLEAN": "boolean",
    "TEXT": "text",
    "JSONB": "jsonb",
    "TIMESTAMPTZ": "timestamp with time zone",
}


def _canonical(ddl):
    if ddl.startswith("VARCHAR("):
        return "character varying" + ddl[len("VARCHAR"):]
    return _CANONICAL[ddl]


def test_round_trip_introspection_matches_registry(env):
    dsn, d, conn = env
    Author = model(
        "Author",
        "author",
        id=AutoField(),
        name=CharField(max_length=80, unique=True),
        bio=TextField(null=True),
        rating=FloatField(default=0.0),
        active=BooleanField(default=True),
        meta=JSONField(null=True),
        joined=DateTimeField(auto_now_add=True),
        views=BigIntegerField(default=0),
    )
    Book = model(
        "Book",
        "book",
        id=AutoField(),
        title=CharField(max_length=200),
        author=ForeignKey(Author, on_delete="SET NULL", null=True),
        pages=IntegerField(null=True),
    )
    make_apply(Migrator(dsn, [Author, Book], d))
    live = conn.introspect(["author", "book"])
    for m in (Author, Book):
        spec = m._meta.schema
        t = live[spec.table]
        by_col = {c["name"]: c for c in t["columns"]}
        assert list(by_col) == [f.column for f in spec.fields]
        for f in spec.fields:
            c = by_col[f.column]
            assert c["type"] == _canonical(f.pg_type), f.column
            assert c["not_null"] == (f.primary_key or not f.null), f.column
            assert (c["default"] is not None) == (f.default_sql is not None or f.auto), f.column
        uniques = sorted(c["columns"][0] for c in t["constraints"] if c["kind"] == "unique")
        assert uniques == sorted(f.column for f in spec.fields if f.unique and not f.primary_key)
        pk = [c["columns"] for c in t["constraints"] if c["kind"] == "primary"]
        assert pk == [[spec.pk_column]]
        fks = sorted(
            (c["columns"][0], c["ref_table"], c["ref_columns"][0], c["on_delete"])
            for c in t["constraints"]
            if c["kind"] == "foreign"
        )
        want = sorted(
            (f.column, f.references[0], f.references[1], f.references[2])
            for f in spec.fields
            if f.references
        )
        assert fks == want


def test_quoting_of_odd_identifiers(env):
    dsn, d, conn = env
    A = model("Odd", 'odd"table', id=AutoField(), x=TextField(db_column='we"ird col', null=True))
    mig = Migrator(dsn, [A], d)
    make_apply(mig)
    assert set(cols(conn, 'odd"table')) == {"id", 'we"ird col'}
    assert mig.make() is None


def test_reserved_table_name(env):
    dsn, d, conn = env
    with pytest.raises(MigrationError, match="reserved"):
        Migrator(dsn, [model("M", "cello_migrations", id=AutoField())], d)


# ── CLI ──────────────────────────────────────────────────────────────────────


def test_cli_requires_experimental(capsys):
    assert main(["status"]) == 2
    assert "EXPERIMENTAL" in capsys.readouterr().err


def test_cli_end_to_end(env, tmp_path):
    dsn, d, conn = env
    pkg = tmp_path / "proj"
    pkg.mkdir()
    (pkg / "shopmodels.py").write_text(
        textwrap.dedent(
            """
            from cello.orm import Model, AutoField, CharField

            class Product(Model):
                id = AutoField()
                name = CharField(max_length=60)
            """
        )
    )

    def cli(*args):
        return subprocess.run(
            [sys.executable, "-m", "cello", "migrate", *args],
            cwd=pkg,
            capture_output=True,
            text=True,
            env={**os.environ, "CELLO_DATABASE_URL": dsn},
            timeout=60,
        )

    r = cli("make", "--models", "shopmodels")
    assert r.returncode == 2 and "--experimental" in r.stderr
    r = cli("--experimental", "sql", "--models", "shopmodels")
    assert r.returncode == 0, r.stderr
    assert 'CREATE TABLE "product"' in r.stdout
    r = cli("--experimental", "make", "--models", "shopmodels", "--name", "Create Products!")
    assert r.returncode == 0, r.stderr
    assert (pkg / "migrations" / "0001_create_products.sql").exists()
    r = cli("--experimental", "status", "--models", "shopmodels")
    assert "pending  0001_create_products" in r.stdout
    r = cli("--experimental", "apply", "--models", "shopmodels")
    assert r.returncode == 0, r.stderr
    assert "Applied 0001_create_products" in r.stdout
    r = cli("--experimental", "status", "--models", "shopmodels")
    assert "models:  in sync" in r.stdout
    assert cols(conn, "product") is not None
    r = cli("--experimental", "status")
    assert r.returncode == 1 and "--app" in r.stderr
