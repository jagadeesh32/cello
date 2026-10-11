"""
``cello migrate`` — **experimental** schema migrations (v1.4.0 Phase 05).

Diffs the ORM's compiled schema registry against the live Postgres catalog and
writes reviewable SQL files (``migrations/NNNN_<slug>.sql``). ``apply`` runs the
pending files in order, one transaction each, under a Postgres advisory lock,
and records them in ``cello_migrations(id, name, checksum, applied_at)``.

It ships behind an explicit ``--experimental`` flag: a migration tool that is
wrong once is worse than none, so the rules are conservative:

* destructive statements (``DROP TABLE``, ``DROP COLUMN``, type changes with no
  lossless cast) are written commented-out unless ``--allow-destructive`` is
  given, and ``apply`` refuses files with live destructive statements without it;
* renames are never guessed (a drop + add pair is flagged as a possible rename);
* already-applied files are checksummed — an edited applied file stops ``apply``;
* ``make`` refuses to run while files are pending, so it never diffs against a
  database that is behind the migration directory.

Usage::

    python -m cello migrate --experimental make   --app main:app
    python -m cello migrate --experimental sql    --models myapp.models
    python -m cello migrate --experimental apply  --app main:app [--allow-destructive]
    python -m cello migrate --experimental status --app main:app

Programmatic use (tests, scripts)::

    from cello.migrate import Migrator
    m = Migrator(dsn, [Author, Book], directory="migrations")
    m.make()        # -> Path of the new file, or None when in sync
    m.apply()       # -> names applied
"""

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import importlib
import json
import os
import re
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Optional, Sequence

from cello._cello import MigrationConnection

__all__ = [
    "Migrator",
    "MigrationError",
    "MigrationFile",
    "collect_models",
    "main",
    "MIGRATIONS_TABLE",
]

#: Bookkeeping table, created in ``current_schema()``.
MIGRATIONS_TABLE = "cello_migrations"

#: Session-level advisory lock key (``b"cellomig"`` as a big-endian int64).
LOCK_KEY = int.from_bytes(b"cellomig", "big")

_FILE_RE = re.compile(r"^(\d{4})_([a-z0-9_]+)\.sql$")
_TABLES_PREFIX = "-- cello:tables "
_DESTRUCTIVE = "-- cello:destructive"
_TX_CONTROL = re.compile(
    r"^\s*(BEGIN|COMMIT|ROLLBACK|END|ABORT|START\s+TRANSACTION|SAVEPOINT|RELEASE)\b",
    re.IGNORECASE,
)

_EXPERIMENTAL_MSG = (
    "cello migrate is EXPERIMENTAL in 1.4.0: it generates and runs DDL against your "
    "database and can lose data if a migration is applied without review.\n"
    "Re-run with --experimental to acknowledge, e.g.\n"
    "    python -m cello migrate --experimental status --app main:app"
)


class MigrationError(RuntimeError):
    """A migration could not be generated or applied; nothing unsafe was run."""


@dataclass(frozen=True)
class MigrationFile:
    number: int
    name: str  # file stem, e.g. "0001_initial"
    path: Path
    text: str

    @property
    def checksum(self) -> str:
        return checksum(self.text)


def checksum(text: str) -> str:
    """SHA-256 of the file text with line endings normalized to ``\\n``."""
    norm = text.replace("\r\n", "\n").replace("\r", "\n")
    return hashlib.sha256(norm.encode("utf-8")).hexdigest()


def _comment(text: str) -> str:
    """Make arbitrary text safe inside a single ``--`` comment line."""
    return text.replace("\r", "\\r").replace("\n", "\\n")


def _slug(text: str) -> str:
    s = re.sub(r"[^a-z0-9]+", "_", text.lower()).strip("_")
    return (s[:48].rstrip("_")) or "auto"


# ── Migration files ──────────────────────────────────────────────────────────


def load_files(directory: Path) -> list[MigrationFile]:
    """Every migration file in ``directory``, in order. Unrecognised ``.sql``
    names and duplicate numbers are errors, never silently skipped."""
    directory = Path(directory)
    if not directory.exists():
        return []
    files: list[MigrationFile] = []
    seen: dict[int, str] = {}
    for p in sorted(directory.iterdir()):
        if p.suffix != ".sql" or not p.is_file():
            continue
        m = _FILE_RE.match(p.name)
        if not m:
            raise MigrationError(
                f"{p}: migration files must be named NNNN_<slug>.sql "
                "(lowercase letters, digits and underscores)"
            )
        n = int(m.group(1))
        if n in seen:
            raise MigrationError(f"two migrations are numbered {n:04d}: {seen[n]} and {p.name}")
        seen[n] = p.name
        files.append(MigrationFile(n, p.stem, p, p.read_text(encoding="utf-8")))
    files.sort(key=lambda f: f.number)
    return files


def managed_tables(files: Sequence[MigrationFile]) -> list[str]:
    """The tables the latest migration file declares as managed."""
    for f in reversed(files):
        for line in f.text.splitlines():
            if line.startswith(_TABLES_PREFIX):
                try:
                    tables = json.loads(line[len(_TABLES_PREFIX):])
                except ValueError as e:
                    raise MigrationError(f"{f.path}: malformed cello:tables header: {e}") from None
                if not isinstance(tables, list) or not all(isinstance(t, str) for t in tables):
                    raise MigrationError(f"{f.path}: cello:tables must be a JSON list of names")
                return tables
        return []
    return []


def parse_body(f: MigrationFile) -> tuple[str, list[str]]:
    """The SQL between the file's ``BEGIN;`` and ``COMMIT;`` lines, and the
    live (uncommented) destructive statements in it.

    The runner owns the transaction, so the body must not contain its own
    transaction control.
    """
    lines = f.text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    begins = [i for i, l in enumerate(lines) if l.strip().upper() == "BEGIN;"]
    commits = [i for i, l in enumerate(lines) if l.strip().upper() == "COMMIT;"]
    if len(begins) != 1 or len(commits) != 1 or begins[0] > commits[0]:
        raise MigrationError(
            f"{f.path}: a migration must contain exactly one 'BEGIN;' line followed by "
            "exactly one 'COMMIT;' line"
        )
    body = lines[begins[0] + 1 : commits[0]]
    for l in body:
        if _TX_CONTROL.match(l):
            raise MigrationError(
                f"{f.path}: transaction control inside the body ({l.strip()!r}); "
                "apply runs each file in its own transaction"
            )
    destructive = []
    for i, l in enumerate(body):
        if l.startswith(_DESTRUCTIVE):
            nxt = next((x for x in body[i + 1 :] if x.strip()), "")
            if not nxt.lstrip().startswith("--"):
                destructive.append(nxt.strip())
    return "\n".join(body), destructive


def render(
    name: str,
    ops: Sequence[dict],
    warnings: Sequence[str],
    tables: Iterable[str],
    allow_destructive: bool,
) -> str:
    """The text of a migration file."""
    out = [
        f"-- cello migration {name}",
        "-- EXPERIMENTAL: generated by `python -m cello migrate --experimental make`.",
        "-- Review every statement before applying. Renames are not detected.",
        f"-- Generated: {_dt.datetime.now(_dt.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}",
        _TABLES_PREFIX + json.dumps(sorted(set(tables))),
    ]
    for w in warnings:
        out.append(f"-- WARNING: {_comment(w)}")
    out += ["", "BEGIN;", ""]
    for op in ops:
        sql = op["sql"]
        if "\n" in sql or "\r" in sql:
            raise MigrationError(
                f"{op['kind']} on {op['table']!r}: the statement contains a line break "
                "(from a default value or identifier), which experimental migrations do "
                "not support; write this migration by hand"
            )
        target = op["table"] + (f".{op['column']}" if op.get("column") else "")
        note = f": {op['note']}" if op.get("note") else ""
        if op["destructive"]:
            state = "" if allow_destructive else " [disabled: needs --allow-destructive; review, then uncomment]"
            out.append(_comment(f"{_DESTRUCTIVE}{state} {op['kind']} {target}{note}"))
            out.append(("" if allow_destructive else "-- ") + sql + ";")
        else:
            out.append(_comment(f"-- {op['kind']} {target}{note}"))
            out.append(sql + ";")
        out.append("")
    out += ["COMMIT;", ""]
    return "\n".join(out)


# ── Model discovery ──────────────────────────────────────────────────────────


def _all_models() -> list[type]:
    from cello.orm import Model

    seen, stack, out = set(), list(Model.__subclasses__()), []
    while stack:
        cls = stack.pop()
        if cls in seen:
            continue
        seen.add(cls)
        out.append(cls)
        stack.extend(cls.__subclasses__())
    return out


def _resolve_app(spec: str):
    module, _, attr = spec.partition(":")
    if not module:
        raise MigrationError(f"--app {spec!r}: expected module:attribute")
    obj = importlib.import_module(module)
    for part in (attr or "app").split("."):
        obj = getattr(obj, part)
    return obj


def collect_models(app: Optional[str] = None, models: Sequence[str] = ()) -> list[type]:
    """Import ``--app`` / ``--models`` and return the ORM models to migrate.

    With ``--models``, only models defined in those modules (or their
    submodules) are used. With only ``--app``, every model imported by the app
    outside the ``cello`` package is used.
    """
    if os.getcwd() not in sys.path:
        sys.path.insert(0, os.getcwd())
    if app:
        _resolve_app(app)
    prefixes = []
    for name in models:
        for part in name.split(","):
            part = part.strip()
            if part:
                importlib.import_module(part)
                prefixes.append(part)
    if not app and not prefixes:
        raise MigrationError("pass --app module:attr or --models module to find your models")

    def wanted(cls) -> bool:
        mod = cls.__module__ or ""
        if prefixes:
            return any(mod == p or mod.startswith(p + ".") for p in prefixes)
        return not (mod == "cello" or mod.startswith("cello."))

    found = [c for c in _all_models() if wanted(c)]
    return sorted(found, key=lambda c: (c.__module__, c.__qualname__))


# ── Migrator ─────────────────────────────────────────────────────────────────


class Migrator:
    """Plan, write, inspect and apply migrations for ``models`` against ``dsn``."""

    def __init__(self, dsn: str, models: Sequence[type], directory="migrations"):
        self.dsn = dsn
        self.models = list(models)
        self.directory = Path(directory)
        for m in self.models:
            if m._meta.table_name == MIGRATIONS_TABLE:
                raise MigrationError(f"{m.__name__}: table name {MIGRATIONS_TABLE!r} is reserved")

    # -- helpers --------------------------------------------------------------

    def connect(self) -> MigrationConnection:
        return MigrationConnection(self.dsn)

    def _specs(self):
        from cello.orm import _replay_declaration_errors

        specs = []
        for m in self.models:
            schema = m._meta.schema
            if not schema.complete:
                _replay_declaration_errors(m)
            specs.append(schema)
        return specs

    @staticmethod
    def _has_table(conn: MigrationConnection) -> bool:
        rows = conn.query(
            "SELECT count(*)::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace "
            "WHERE n.nspname = current_schema() AND c.relname::text = $1 AND c.relkind = 'r'",
            [MIGRATIONS_TABLE],
        )
        return rows[0][0] != "0"

    def _applied(self, conn: MigrationConnection) -> list[tuple[str, str, str]]:
        if not self._has_table(conn):
            return []
        return [
            (r[0], r[1], r[2])
            for r in conn.query(
                f'SELECT name, checksum, applied_at::text FROM "{MIGRATIONS_TABLE}" ORDER BY id'
            )
        ]

    @staticmethod
    def _check_history(files: Sequence[MigrationFile], applied) -> list[MigrationFile]:
        """Verify applied files are untouched and in order; return the pending ones."""
        by_name = {f.name: f for f in files}
        problems = []
        last = 0
        for name, digest, _ in applied:
            f = by_name.get(name)
            if f is None:
                problems.append(f"{name} is recorded as applied but its file is missing")
                continue
            if f.checksum != digest:
                problems.append(
                    f"{f.path} was edited after it was applied (checksum mismatch); "
                    "restore it, and put further changes in a new migration"
                )
            last = max(last, f.number)
        if problems:
            raise MigrationError("refusing to continue:\n  " + "\n  ".join(problems))
        done = {a[0] for a in applied}
        pending = [f for f in files if f.name not in done]
        early = [f.name for f in pending if f.number < last]
        if early:
            raise MigrationError(
                f"pending migrations {early} are numbered before an applied one; "
                "renumber them after the latest applied migration"
            )
        return pending

    # -- commands -------------------------------------------------------------

    def plan(self, conn: Optional[MigrationConnection] = None) -> tuple[list[dict], list[str]]:
        """Diff the models against the live schema: ``(ops, warnings)``."""
        files = load_files(self.directory)
        own = conn is None
        conn = conn or self.connect()
        try:
            return conn.plan(self._specs(), managed_tables(files))
        finally:
            if own:
                conn.close()

    def _render_next(self, name, allow_destructive, conn):
        files = load_files(self.directory)
        pending = self._check_history(files, self._applied(conn))
        if pending:
            raise MigrationError(
                f"{len(pending)} migration(s) not applied yet ({', '.join(f.name for f in pending)}); "
                "apply them before generating a new one"
            )
        ops, warnings = conn.plan(self._specs(), managed_tables(files))
        number = (files[-1].number + 1) if files else 1
        if not ops:
            return None, ops, warnings, number
        if not name:
            name = "initial" if not files else f"{ops[0]['kind']}_{ops[0]['table']}"
            if files and len(ops) > 1:
                name += "_and_more"
        stem = f"{number:04d}_{_slug(name)}"
        tables = {str(m._meta.table_name) for m in self.models}
        if not allow_destructive:
            # A table leaves the managed set only once its DROP TABLE is live;
            # a commented-out drop keeps it tracked for the next make.
            tables.update(managed_tables(files))
        text = render(stem, ops, warnings, tables, allow_destructive)
        return stem, ops, warnings, text

    def sql(self, allow_destructive: bool = False) -> str:
        """The migration ``make`` would write, without writing it (``""`` when in sync)."""
        conn = self.connect()
        try:
            stem, _, _, text = self._render_next(None, allow_destructive, conn)
            return text if stem else ""
        finally:
            conn.close()

    def make(
        self, name: Optional[str] = None, allow_destructive: bool = False, echo=None
    ) -> Optional[Path]:
        """Write the next migration file; ``None`` when the schema is in sync or
        when the only changes are destructive and not allowed."""
        echo = echo or (lambda *_: None)
        conn = self.connect()
        try:
            stem, ops, warnings, text = self._render_next(name, allow_destructive, conn)
        finally:
            conn.close()
        for w in warnings:
            echo(f"warning: {w}")
        if stem is None:
            echo("No changes: the database matches the models.")
            return None
        gated = [o for o in ops if o["destructive"] and not allow_destructive]
        if gated:
            echo(
                f"warning: {len(gated)} destructive statement(s) written commented-out; "
                "review them, then re-run make with --allow-destructive or uncomment by hand"
            )
            if len(gated) == len(ops):
                echo("No file written: every change is destructive and --allow-destructive was not given.")
                for o in gated:
                    echo(f"  {o['sql']};")
                return None
        self.directory.mkdir(parents=True, exist_ok=True)
        path = self.directory / f"{stem}.sql"
        with open(path, "x", encoding="utf-8", newline="\n") as fh:
            fh.write(text)
        echo(f"Wrote {path} ({len(ops)} statement(s)). Review it, then run apply.")
        return path

    def status(self) -> dict:
        """Applied / pending files, history problems and model drift."""
        files = load_files(self.directory)
        conn = self.connect()
        try:
            applied = self._applied(conn)
            result = {
                "applied": [(a[0], a[2]) for a in applied],
                "pending": [],
                "problems": [],
                "drift": None,
                "warnings": [],
            }
            try:
                pending = self._check_history(files, applied)
            except MigrationError as e:
                result["problems"].append(str(e))
                return result
            result["pending"] = [f.name for f in pending]
            if not pending:
                ops, warnings = conn.plan(self._specs(), managed_tables(files))
                result["drift"] = ops
                result["warnings"] = warnings
            return result
        finally:
            conn.close()

    def _lock(self, conn: MigrationConnection, timeout: float) -> None:
        deadline = time.monotonic() + max(0.0, timeout)
        while True:
            got = conn.query("SELECT pg_try_advisory_lock($1::text::bigint)::text", [str(LOCK_KEY)])
            if got[0][0] == "true":
                return
            if time.monotonic() >= deadline:
                raise MigrationError(
                    "another `cello migrate apply` holds the migration lock on this database "
                    f"(waited {timeout:g}s); try again when it has finished"
                )
            time.sleep(0.1)

    def apply(
        self, allow_destructive: bool = False, lock_timeout: float = 60.0, echo=None
    ) -> list[str]:
        """Apply pending migrations in order, each in its own transaction, under
        an advisory lock. Returns the names applied."""
        echo = echo or (lambda *_: None)
        conn = self.connect()
        try:
            self._lock(conn, lock_timeout)
            try:
                conn.execute(
                    f'CREATE TABLE IF NOT EXISTS "{MIGRATIONS_TABLE}" ('
                    "id SERIAL PRIMARY KEY, name TEXT NOT NULL UNIQUE, "
                    "checksum TEXT NOT NULL, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())"
                )
                # Read files only once the lock is held: a concurrent run that
                # finished first is then visible as applied.
                files = load_files(self.directory)
                pending = self._check_history(files, self._applied(conn))
                bodies = []
                for f in pending:
                    body, destructive = parse_body(f)
                    if destructive and not allow_destructive:
                        raise MigrationError(
                            f"{f.path} contains destructive statements; re-run apply with "
                            "--allow-destructive after reviewing them:\n  " + "\n  ".join(destructive)
                        )
                    bodies.append((f, body))
                done = []
                for f, body in bodies:
                    conn.execute("BEGIN")
                    try:
                        conn.execute(body)
                        conn.query(
                            f'INSERT INTO "{MIGRATIONS_TABLE}" (name, checksum) '
                            "VALUES ($1, $2) RETURNING id::text",
                            [f.name, f.checksum],
                        )
                        conn.execute("COMMIT")
                    except Exception as e:
                        try:
                            conn.execute("ROLLBACK")
                        except Exception:
                            pass
                        raise MigrationError(
                            f"{f.name} failed and was rolled back: {e}"
                            + (f" (applied before it: {', '.join(done)})" if done else "")
                        ) from e
                    done.append(f.name)
                    echo(f"Applied {f.name}")
                if not done:
                    echo("Nothing to apply.")
                return done
            finally:
                if not conn.closed:
                    try:
                        conn.query("SELECT pg_advisory_unlock($1::text::bigint)::text", [str(LOCK_KEY)])
                    except Exception:
                        pass  # closing the session releases it too
        finally:
            conn.close()


# ── CLI ──────────────────────────────────────────────────────────────────────


def _resolve_dsn(args) -> str:
    dsn = getattr(args, "dsn", None) or os.environ.get("CELLO_DATABASE_URL") or os.environ.get(
        "DATABASE_URL"
    )
    if not dsn and getattr(args, "app", None):
        try:
            dsn = _resolve_app(args.app).database.dsn
        except Exception:
            dsn = None
    if not dsn:
        raise MigrationError(
            "no database: pass --dsn, set CELLO_DATABASE_URL / DATABASE_URL, or call "
            "app.enable_database(...) in the --app module"
        )
    return dsn


def _parser() -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    S = argparse.SUPPRESS
    common.add_argument("--experimental", action="store_true", default=S,
                        help="acknowledge that migrations are experimental (required)")
    common.add_argument("--app", default=S, help="the app, as module:attribute")
    common.add_argument("--models", action="append", default=S,
                        help="module(s) defining the models (repeatable, or comma separated)")
    common.add_argument("--dsn", default=S, help="Postgres URL (default: CELLO_DATABASE_URL / DATABASE_URL)")
    common.add_argument("--dir", default=S, help="migrations directory (default: ./migrations)")
    common.add_argument("--allow-destructive", action="store_true", default=S,
                        help="emit / apply DROP TABLE, DROP COLUMN and lossy type changes")

    p = argparse.ArgumentParser(
        prog="cello migrate",
        parents=[common],
        description="EXPERIMENTAL schema migrations for cello.orm models (Postgres).",
    )
    sub = p.add_subparsers(dest="command")
    mk = sub.add_parser("make", parents=[common], help="write the next migration file")
    mk.add_argument("--name", default=None, help="slug for the file name")
    ap = sub.add_parser("apply", parents=[common], help="apply pending migrations")
    ap.add_argument("--lock-timeout", type=float, default=60.0,
                    help="seconds to wait for a concurrent apply (default 60)")
    sub.add_parser("status", parents=[common], help="show applied / pending migrations and drift")
    sub.add_parser("sql", parents=[common], help="print the next migration without writing it")
    return p


def main(argv: Optional[Sequence[str]] = None) -> int:
    """Entry point of ``python -m cello migrate``. Returns the exit code."""
    parser = _parser()
    args = parser.parse_args(list(sys.argv[1:] if argv is None else argv))
    if not getattr(args, "experimental", False):
        print(_EXPERIMENTAL_MSG, file=sys.stderr)
        return 2
    if not args.command:
        parser.print_help(sys.stderr)
        return 2
    allow = bool(getattr(args, "allow_destructive", False))
    try:
        models = collect_models(getattr(args, "app", None), getattr(args, "models", None) or ())
        mig = Migrator(_resolve_dsn(args), models, getattr(args, "dir", None) or "migrations")
        if args.command == "make":
            mig.make(args.name, allow_destructive=allow, echo=print)
        elif args.command == "sql":
            text = mig.sql(allow_destructive=allow)
            print(text if text else "-- No changes: the database matches the models.")
        elif args.command == "apply":
            mig.apply(allow_destructive=allow, lock_timeout=args.lock_timeout, echo=print)
        elif args.command == "status":
            st = mig.status()
            for name, at in st["applied"]:
                print(f"applied  {name}  ({at})")
            for name in st["pending"]:
                print(f"pending  {name}")
            for prob in st["problems"]:
                print(f"ERROR    {prob}")
            for w in st["warnings"]:
                print(f"warning: {w}")
            if st["drift"] is not None:
                if st["drift"]:
                    print(f"models:  {len(st['drift'])} change(s) not in a migration yet (run make)")
                else:
                    print("models:  in sync")
            return 1 if st["problems"] else 0
    except MigrationError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    except Exception as e:  # database / import errors: one line, no traceback
        print(f"error: {type(e).__name__}: {e}", file=sys.stderr)
        return 1
    return 0
