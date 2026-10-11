"""``python -m cello`` / the ``cello`` console command.

Commands:

* ``migrate`` — EXPERIMENTAL schema migrations (see :mod:`cello.migrate`).
"""

import sys

_USAGE = """usage: python -m cello <command> [options]

commands:
  migrate    EXPERIMENTAL schema migrations for cello.orm models
             (python -m cello migrate --experimental --help)
"""


def main(argv=None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv or argv[0] in ("-h", "--help", "help"):
        print(_USAGE)
        return 0
    if argv[0] == "migrate":
        from cello.migrate import main as migrate_main

        return migrate_main(argv[1:])
    print(f"unknown command {argv[0]!r}\n\n{_USAGE}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
