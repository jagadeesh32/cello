#!/usr/bin/env python3
"""
Postgres TLS demo for Cello v1.4.0.

Connects to the database once per libpq `sslmode` and asks the server how the
connection really looks (`pg_stat_ssl`), so you can see what each mode does:

    disable      never encrypt
    prefer       (default) encrypt if the server offers TLS, no verification
    require      encrypt, no verification
    verify-ca    encrypt, certificate chain must lead to `sslrootcert`
    verify-full  verify-ca + the certificate must match the host name

Against a local Debian/Ubuntu cluster (self-signed "snakeoil" certificate)
disable / prefer / require succeed, and the two verifying modes are rejected
because that certificate is not issued by a CA you trust. That is the point:
`prefer` and `require` encrypt but do not authenticate the server.

To see a verifying mode succeed, give the CA that signed your server's
certificate:

    CELLO_PG_ROOT_CERT=/etc/cello/db-ca.pem \\
    DATABASE_URL=postgresql://app:secret@db.internal:5432/app \\
        python examples/postgres_tls_demo.py

The demo also serves the result over HTTP (the same `request.database` handle
your handlers use). Pass --no-serve to just print the table and exit.

Run:
    export DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/cello_test
    python examples/postgres_tls_demo.py --port 8000
    curl localhost:8000/tls
"""

import argparse
import asyncio
import os

from cello import App, DatabaseConfig

DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://postgres:postgres@127.0.0.1:5432/cello_test"
)
ROOT_CERT = os.environ.get("CELLO_PG_ROOT_CERT")  # PEM file with your CA

MODES = ["disable", "prefer", "require", "verify-ca", "verify-full"]

TLS_QUERY = (
    "SELECT ssl, version, cipher FROM pg_stat_ssl WHERE pid = pg_backend_pid()"
)


def make_app(sslmode):
    """An app whose pool uses `sslmode` (the option overrides ?sslmode= in the URL)."""
    app = App()
    kwargs = {"url": DATABASE_URL, "pool_size": 2, "sslmode": sslmode}
    if ROOT_CERT:
        # With sslrootcert, ONLY these CAs are trusted. Without it, the bundled
        # public root store is used (right for managed databases).
        kwargs["sslrootcert"] = ROOT_CERT
    app.enable_database(DatabaseConfig(**kwargs))
    return app


async def probe(sslmode):
    """Open a connection with `sslmode` and report what the server sees."""
    try:
        app = make_app(sslmode)
    except ValueError as exc:  # bad sslmode / unreadable certificate file
        return {"sslmode": sslmode, "ok": False, "error": f"config: {exc}"}
    try:
        row = await app.database.fetchrow(TLS_QUERY)
    except Exception as exc:  # handshake failures are OperationalError
        return {"sslmode": sslmode, "ok": False, "error": f"{type(exc).__name__}: {exc}"}
    finally:
        try:
            await app.database.close()
        except Exception:
            pass
    return {
        "sslmode": app.database.sslmode,
        "ok": True,
        "encrypted": row["ssl"],
        "protocol": row["version"],
        "cipher": row["cipher"],
    }


async def probe_all():
    return [await probe(m) for m in MODES]


def print_table(results):
    print(f"server: {DATABASE_URL.split('@')[-1]}   sslrootcert: {ROOT_CERT or '(none)'}\n")
    print(f"{'sslmode':<12} {'result':<9} details")
    for r in results:
        if r["ok"]:
            detail = f"encrypted={r['encrypted']}"
            if r["encrypted"]:
                detail += f" {r['protocol']} {r['cipher']}"
            print(f"{r['sslmode']:<12} {'connected':<9} {detail}")
        else:
            print(f"{r['sslmode']:<12} {'REJECTED':<9} {r['error']}")


# The app below serves the default (`prefer`) connection, like a real service.
app = App()
app.enable_database(DatabaseConfig(url=DATABASE_URL, pool_size=2))


@app.get("/tls")
async def tls(request):
    row = await request.database.fetchrow(TLS_QUERY)
    return {
        "configured_sslmode": request.database.sslmode,
        "encrypted": row["ssl"],
        "protocol": row["version"],
        "cipher": row["cipher"],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Postgres TLS demo")
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--no-serve", action="store_true", help="print the table and exit")
    args = parser.parse_args()

    print_table(asyncio.run(probe_all()))
    if not args.no_serve:
        print()
        app.run(host=args.host, port=args.port)
