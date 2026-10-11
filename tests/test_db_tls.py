"""Postgres TLS (libpq ``sslmode``) for the native pool and the migration session.

Starts throwaway Postgres clusters (``initdb`` into a temp dir, non-default
ports) with a freshly generated CA + server/client certificates, and removes
everything afterwards. Skipped with a reason when ``initdb``/``pg_ctl`` or the
``openssl`` CLI are unavailable.

Servers:
  * ``tls``   — ``ssl=on``; pg_hba is ``hostssl``-only for normal users
    (scram-sha-256), ``cert`` auth for ``certuser``.
  * ``plain`` — ``ssl=off`` (the server answers ``N`` to SSLRequest), trust.
"""

from __future__ import annotations

import asyncio
import glob
import os
import shutil
import socket
import subprocess
import tempfile
from pathlib import Path

import pytest

from cello import App, DatabaseConfig

try:
    from cello._cello import MigrationConnection
except ImportError:  # pragma: no cover
    MigrationConnection = None

PASSWORD = "s3cret-pw"


# ---------------------------------------------------------------------------
# Environment discovery
# ---------------------------------------------------------------------------


def _pg_bindir() -> str | None:
    found = shutil.which("initdb")
    if found and shutil.which("pg_ctl"):
        return str(Path(found).parent)
    cands = sorted(
        glob.glob("/usr/lib/postgresql/*/bin"),
        key=lambda p: int(p.split("/")[-2]) if p.split("/")[-2].isdigit() else 0,
    )
    for d in reversed(cands):
        if os.path.exists(f"{d}/initdb") and os.path.exists(f"{d}/pg_ctl"):
            return d
    return None


BINDIR = _pg_bindir()
OPENSSL = shutil.which("openssl")

pytestmark = [
    pytest.mark.skipif(BINDIR is None, reason="initdb/pg_ctl not found (install postgresql server binaries)"),
    pytest.mark.skipif(OPENSSL is None, reason="openssl CLI not found (needed to generate test certificates)"),
]

IS_ROOT = hasattr(os, "geteuid") and os.geteuid() == 0


def _as_pg_user(cmd: list[str]) -> list[str]:
    """Postgres refuses to run as root: drop to the `postgres` account."""
    if IS_ROOT:
        return ["runuser", "-u", "postgres", "--", *cmd]
    return cmd


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _run(cmd, **kw):
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw)


def _make_base_dir() -> Path:
    base = Path(tempfile.mkdtemp(prefix="cello-pgtls-"))
    if IS_ROOT:
        # The server user must be able to traverse to its data dir; a private
        # TMPDIR (0700 parents) is not reachable, so fall back to /tmp.
        ok = subprocess.run(["runuser", "-u", "postgres", "--", "test", "-x", str(base)]).returncode == 0
        if not ok:
            shutil.rmtree(base, ignore_errors=True)
            base = Path(tempfile.mkdtemp(prefix="cello-pgtls-", dir="/tmp"))
        os.chmod(base, 0o755)
    return base


# ---------------------------------------------------------------------------
# Certificates
# ---------------------------------------------------------------------------


def _ca(d: Path, name: str, cn: str):
    _run([
        OPENSSL, "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
        "-nodes", "-keyout", str(d / f"{name}.key"), "-out", str(d / f"{name}.crt"),
        "-days", "2", "-subj", f"/CN={cn}",
        "-addext", "basicConstraints=critical,CA:TRUE",
        "-addext", "keyUsage=critical,keyCertSign,cRLSign",
    ])


def _leaf(d: Path, name: str, cn: str, ext: str):
    _run([
        OPENSSL, "req", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes",
        "-keyout", str(d / f"{name}.key"), "-out", str(d / f"{name}.csr"), "-subj", f"/CN={cn}",
    ])
    (d / f"{name}.ext").write_text(ext)
    _run([
        OPENSSL, "x509", "-req", "-in", str(d / f"{name}.csr"), "-CA", str(d / "ca.crt"),
        "-CAkey", str(d / "ca.key"), "-CAcreateserial", "-out", str(d / f"{name}.crt"),
        "-days", "2", "-extfile", str(d / f"{name}.ext"),
    ])


def _make_certs(d: Path):
    d.mkdir()
    _ca(d, "ca", "Cello Test CA")
    _ca(d, "other_ca", "Unrelated CA")
    _leaf(d, "server", "localhost",
          "subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\n"
          "keyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n")
    _leaf(d, "client", "certuser",
          "basicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature\n"
          "extendedKeyUsage=clientAuth\n")
    for f in d.iterdir():
        os.chmod(f, 0o644)


# ---------------------------------------------------------------------------
# Clusters
# ---------------------------------------------------------------------------


class Cluster:
    def __init__(self, base: Path, name: str, port: int):
        self.data = base / name
        self.port = port
        self.log = base / f"{name}.log"

    def init(self, pwfile: Path | None):
        self.data.mkdir(mode=0o700)
        if IS_ROOT:
            shutil.chown(self.data, "postgres", "postgres")
        cmd = [f"{BINDIR}/initdb", "-D", str(self.data), "-U", "postgres", "-N", "-E", "UTF8",
               "--locale=C"]
        if pwfile:
            cmd += ["--auth=scram-sha-256", f"--pwfile={pwfile}"]
        else:
            cmd += ["--auth=trust"]
        _run(_as_pg_user(cmd))

    def write(self, name: str, text: str, mode: int = 0o600):
        p = self.data / name
        p.write_text(text)
        os.chmod(p, mode)
        if IS_ROOT:
            shutil.chown(p, "postgres", "postgres")

    def start(self):
        if IS_ROOT:
            self.log.touch()
            shutil.chown(self.log, "postgres", "postgres")
        _run(_as_pg_user([f"{BINDIR}/pg_ctl", "-D", str(self.data), "-l", str(self.log),
                          "-w", "-t", "60", "start"]))

    def stop(self):
        subprocess.run(_as_pg_user([f"{BINDIR}/pg_ctl", "-D", str(self.data), "-m", "immediate",
                                    "-w", "stop"]), capture_output=True)


@pytest.fixture(scope="module")
def env():
    base = _make_base_dir()
    clusters: list[Cluster] = []
    try:
        certs = base / "certs"
        _make_certs(certs)

        pw = base / "pw"
        pw.write_text(PASSWORD + "\n")
        os.chmod(pw, 0o644)

        tls = Cluster(base, "tls", _free_port())
        tls.init(pw)
        for f in ("server.crt", "server.key", "ca.crt"):
            shutil.copy(certs / f, tls.data / f)
            os.chmod(tls.data / f, 0o600)
            if IS_ROOT:
                shutil.chown(tls.data / f, "postgres", "postgres")
        with open(tls.data / "postgresql.conf", "a") as fh:
            fh.write(
                f"\nport = {tls.port}\nlisten_addresses = '127.0.0.1'\n"
                "unix_socket_directories = ''\nssl = on\n"
                "ssl_cert_file = 'server.crt'\nssl_key_file = 'server.key'\n"
                "ssl_ca_file = 'ca.crt'\n"
            )
        tls.write("pg_hba.conf",
                  "hostssl all certuser 127.0.0.1/32 cert\n"
                  "hostssl all all 127.0.0.1/32 scram-sha-256\n")
        tls.start()
        clusters.append(tls)

        plain = Cluster(base, "plain", _free_port())
        plain.init(None)
        with open(plain.data / "postgresql.conf", "a") as fh:
            fh.write(
                f"\nport = {plain.port}\nlisten_addresses = '127.0.0.1'\n"
                "unix_socket_directories = ''\nssl = off\n"
            )
        plain.write("pg_hba.conf", "host all all 127.0.0.1/32 trust\n")
        plain.start()
        clusters.append(plain)

        # Roles for client-certificate auth.
        mc = MigrationConnection(
            f"postgresql://postgres:{PASSWORD}@127.0.0.1:{tls.port}/postgres?sslmode=require"
        )
        mc.execute("CREATE ROLE certuser LOGIN")
        mc.close()

        yield {"certs": certs, "tls": tls.port, "plain": plain.port}
    except subprocess.CalledProcessError as e:  # pragma: no cover - env-specific
        pytest.skip(f"could not start a throwaway Postgres: {e.cmd}: {e.stderr.strip()}")
    finally:
        for c in clusters:
            c.stop()
        shutil.rmtree(base, ignore_errors=True)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _url(port: int, host: str = "localhost", user: str = "postgres", query: str = "") -> str:
    auth = f"{user}:{PASSWORD}" if user == "postgres" else user
    url = f"postgresql://{auth}@{host}:{port}/postgres"
    return f"{url}?{query}" if query else url


def _db(url: str, **kw):
    app = App()
    app.enable_database(DatabaseConfig(url=url, pool_size=2, **kw))
    return app.database


def _ssl_row(db):
    async def go():
        return await db.fetchrow(
            "SELECT ssl, version, cipher FROM pg_stat_ssl WHERE pid = pg_backend_pid()"
        )

    return asyncio.run(go())


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


def test_verify_full_with_right_ca(env):
    db = _db(_url(env["tls"]), sslmode="verify-full", sslrootcert=str(env["certs"] / "ca.crt"))
    assert db.sslmode == "verify-full"
    row = _ssl_row(db)
    assert row["ssl"] is True
    assert row["version"].startswith("TLS")


def test_verify_full_from_dsn_query_string(env):
    ca = env["certs"] / "ca.crt"
    db = _db(_url(env["tls"], query=f"sslmode=verify-full&sslrootcert={ca}&application_name=tls-test"))
    assert db.sslmode == "verify-full"

    async def go():
        return await db.fetchrow(
            "SELECT s.ssl, a.application_name FROM pg_stat_ssl s "
            "JOIN pg_stat_activity a USING (pid) WHERE s.pid = pg_backend_pid()"
        )

    row = asyncio.run(go())
    assert row == {"ssl": True, "application_name": "tls-test"}


def test_verify_full_fails_on_wrong_hostname(env):
    # Certificate names only DNS:localhost; connecting by IP must fail.
    db = _db(_url(env["tls"], host="127.0.0.1"), sslmode="verify-full",
             sslrootcert=str(env["certs"] / "ca.crt"))
    with pytest.raises(Exception, match="NotValidForName"):
        _ssl_row(db)


def test_verify_ca_ignores_hostname(env):
    db = _db(_url(env["tls"], host="127.0.0.1"), sslmode="verify-ca",
             sslrootcert=str(env["certs"] / "ca.crt"))
    assert _ssl_row(db)["ssl"] is True


def test_verify_full_fails_with_wrong_ca(env):
    db = _db(_url(env["tls"]), sslmode="verify-full",
             sslrootcert=str(env["certs"] / "other_ca.crt"))
    with pytest.raises(Exception, match="UnknownIssuer"):
        _ssl_row(db)


def test_verify_full_without_rootcert_uses_public_roots(env):
    # The private test CA is not a public root, so verification must fail.
    db = _db(_url(env["tls"]), sslmode="verify-full")
    with pytest.raises(Exception, match="UnknownIssuer"):
        _ssl_row(db)


def test_require_works_without_ca(env):
    db = _db(_url(env["tls"], host="127.0.0.1"), sslmode="require")
    assert db.sslmode == "require"
    assert _ssl_row(db)["ssl"] is True


def test_require_with_rootcert_verifies_chain(env):
    # libpq semantics: require + sslrootcert behaves like verify-ca.
    db = _db(_url(env["tls"]), sslmode="require", sslrootcert=str(env["certs"] / "other_ca.crt"))
    assert db.sslmode == "verify-ca"
    with pytest.raises(Exception, match="UnknownIssuer"):
        _ssl_row(db)


def test_default_is_prefer_and_uses_tls(env):
    db = _db(_url(env["tls"]))
    assert db.sslmode == "prefer"
    assert _ssl_row(db)["ssl"] is True


def test_disable_against_ssl_only_server_fails(env):
    db = _db(_url(env["tls"], host="127.0.0.1"), sslmode="disable")
    with pytest.raises(Exception, match="no encryption|pg_hba"):
        _ssl_row(db)


def test_prefer_falls_back_on_non_tls_server(env):
    db = _db(_url(env["plain"], host="127.0.0.1"), sslmode="prefer")
    row = _ssl_row(db)
    assert row["ssl"] is False


def test_require_fails_on_non_tls_server(env):
    db = _db(_url(env["plain"], host="127.0.0.1"), sslmode="require")
    with pytest.raises(Exception, match="does not support TLS"):
        _ssl_row(db)


def test_config_option_overrides_dsn(env):
    db = _db(_url(env["plain"], host="127.0.0.1", query="sslmode=require"), sslmode="disable")
    assert db.sslmode == "disable"
    assert _ssl_row(db)["ssl"] is False


def test_scram_channel_binding_over_tls(env):
    # channel_binding=require makes the client insist on SCRAM-SHA-256-PLUS,
    # which only works if tls-server-end-point is computed correctly.
    db = _db(_url(env["tls"], query="channel_binding=require"), sslmode="verify-full",
             sslrootcert=str(env["certs"] / "ca.crt"))
    assert _ssl_row(db)["ssl"] is True


def test_client_certificate_auth(env):
    certs = env["certs"]
    db = _db(_url(env["tls"], user="certuser"), sslmode="verify-full",
             sslrootcert=str(certs / "ca.crt"), sslcert=str(certs / "client.crt"),
             sslkey=str(certs / "client.key"))

    async def go():
        return await db.fetchrow(
            "SELECT current_user AS u, s.ssl, s.client_dn FROM pg_stat_ssl s "
            "WHERE s.pid = pg_backend_pid()"
        )

    row = asyncio.run(go())
    assert row["u"] == "certuser"
    assert row["ssl"] is True
    assert "certuser" in row["client_dn"]


def test_data_round_trips_over_tls(env):
    db = _db(_url(env["tls"]), sslmode="verify-full", sslrootcert=str(env["certs"] / "ca.crt"))
    big = "x" * 200_000  # spans many TLS records

    async def go():
        await db.execute("DROP TABLE IF EXISTS tls_rt")
        await db.execute("CREATE TABLE tls_rt (id serial PRIMARY KEY, s text, j jsonb, b bytea)")
        await db.execute(
            "INSERT INTO tls_rt (s, j, b) VALUES ($1, $2, $3), ($4, $5, $6)",
            "héllo ✓", {"k": [1, 2, {"n": None}]}, b"\x00\x01\xff",
            big, {"big": True}, bytes(range(256)),
        )
        rows = await db.fetch("SELECT s, j, b FROM tls_rt ORDER BY id")
        async with db.transaction() as tx:
            await tx.execute("UPDATE tls_rt SET s = $1 WHERE id = 1", "updated")
        val = await db.fetchval("SELECT s FROM tls_rt WHERE id = 1")
        await db.execute("DROP TABLE tls_rt")
        return rows, val

    rows, val = asyncio.run(go())
    assert rows[0]["s"] == "héllo ✓"
    assert rows[0]["j"] == {"k": [1, 2, {"n": None}]}
    assert bytes(rows[0]["b"]) == b"\x00\x01\xff"
    assert rows[1]["s"] == big
    assert bytes(rows[1]["b"]) == bytes(range(256))
    assert val == "updated"


@pytest.mark.skipif(MigrationConnection is None, reason="MigrationConnection not built")
def test_migration_connection_uses_tls(env):
    ca = env["certs"] / "ca.crt"
    mc = MigrationConnection(_url(env["tls"], query=f"sslmode=verify-full&sslrootcert={ca}"))
    try:
        rows = mc.query("SELECT ssl::text FROM pg_stat_ssl WHERE pid = pg_backend_pid()")
        assert rows == [["true"]]
    finally:
        mc.close()


@pytest.mark.skipif(MigrationConnection is None, reason="MigrationConnection not built")
def test_migration_connection_verify_full_wrong_host_fails(env):
    ca = env["certs"] / "ca.crt"
    with pytest.raises(Exception, match="NotValidForName"):
        MigrationConnection(_url(env["tls"], host="127.0.0.1",
                                 query=f"sslmode=verify-full&sslrootcert={ca}"))


# -- configuration errors: raised by enable_database(), no server needed ------


def test_bad_rootcert_path_raises_at_enable_database(tmp_path):
    with pytest.raises(ValueError, match="sslrootcert"):
        App().enable_database(DatabaseConfig(
            url="postgresql://u@db.example.com/x", sslmode="verify-full",
            sslrootcert=str(tmp_path / "missing.pem")))


def test_non_pem_rootcert_raises(tmp_path):
    bad = tmp_path / "bad.pem"
    bad.write_text("not a certificate\n")
    with pytest.raises(ValueError, match="no PEM certificates"):
        App().enable_database(DatabaseConfig(
            url=f"postgresql://u@db.example.com/x?sslmode=verify-ca&sslrootcert={bad}"))


def test_invalid_sslmode_raises():
    with pytest.raises(ValueError, match="invalid sslmode"):
        App().enable_database(DatabaseConfig(url="postgresql://u@h/x", sslmode="sometimes"))


def test_sslcert_without_key_raises(tmp_path):
    with pytest.raises(ValueError, match="without sslkey"):
        App().enable_database(DatabaseConfig(
            url="postgresql://u@h/x", sslmode="require", sslcert=str(tmp_path / "c.pem")))


def test_unix_socket_host_never_uses_tls():
    app = App()
    app.enable_database(DatabaseConfig(url="host=/var/run/postgresql user=u dbname=x",
                                       sslmode="verify-full"))
    assert app.database.sslmode == "disable"
