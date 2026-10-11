#!/usr/bin/env python3
"""
Paired A/B benchmark driver for Cello builds (wrk-based).

Starts build A and build B *simultaneously* (each from its own venv python, on
its own port), warms both up, then for every endpoint alternates wrk runs
between them and reports mean rps, delta, spread and latency.

    python3 paired.py --a /path/venvA/bin/python --b /path/venvB/bin/python

This script itself only needs the standard library; it can run under any
python3. The servers run under the A/B interpreters. The harness never imports
cello itself.

Why paired: on a shared-CPU box, sequential runs drift by +-20% over minutes.
Both servers are up the whole time and runs are interleaved within each round,
so slow drift (thermal, neighbours, page cache) hits A and B alike and cancels
in the paired ratio. See README.md.

Ordering: by default each round alternates which build goes first
(round 1: A,B; round 2: B,A; ...) - an ABBA design, which cancels both linear
drift and any "second runner" bias. --order ab gives strict A,B,A,B.
"""

import argparse
import json
import os
import re
import resource
import signal
import socket
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.request
from datetime import datetime

HERE = os.path.dirname(os.path.abspath(__file__))
WRK = os.environ.get("WRK", "/usr/bin/wrk")

VALIDATE_BODY = {
    "f01_name": "Ada", "f02_email": "ada@example.com", "f03_age": 36,
    "f04_height": 1.68, "f05_active": True, "f06_city": "London",
    "f07_country": "UK", "f08_zip": "N1 9GU", "f09_phone": "+44 20 7946 0000",
    "f10_score": 98.5, "f11_level": 7, "f12_newsletter": False,
    "f13_lang": "en", "f14_tz": "Europe/London", "f15_referrer": "search",
    "f16_visits": 1234, "f17_ratio": 0.42, "f18_admin": False,
    "f19_notes": "benchmark payload", "f20_plan": "pro",
}

# name -> (method, path, body or None, reset-before-each-run, plan-table label)
ENDPOINTS = {
    "control":  ("GET",  "/control",     None,          False, "(e) {\"ok\": true} hot-path control"),
    "orm_list": ("GET",  "/orm/list",    None,          False, "(a) ORM list 200x8, to_dict()"),
    "orm_list_qs": ("GET", "/orm/list_qs", None,         False, "(a') ORM list 200x8, return the queryset (Phase 02 fast path; falls back to to_dict() on builds without it)"),
    "raw_list": ("GET",  "/raw/list",    None,          False, "(b) raw db.fetch 200x8"),
    "orm_get":  ("GET",  "/orm/get/42",  None,          False, "(c) .get(id=...) one row"),
    "insert":   ("POST", "/orm/insert",  {},            True,  "(d) ORM insert one row"),
    "validate": ("POST", "/validate",    VALIDATE_BODY, False, "(f) body= 20-field DTO"),
}
DEFAULT_SET = list(ENDPOINTS)

# Profiling decomposition of orm_list (opt-in via --endpoints; not in the default set).
ENDPOINTS.update({
    "prof_fetch_only":       ("GET", "/prof/fetch_only",       None, False, "orm_list minus models/to_dict/serialization"),
    "prof_orm_no_serialize": ("GET", "/prof/orm_no_serialize", None, False, "orm_list minus serialization"),
    "prof_pg_json":          ("GET", "/prof/pg_json",          None, False, "Postgres builds the JSON (rough ceiling)"),
    "prof_async_noop":       ("GET", "/prof/async_noop",       None, False, "async handler, no I/O (async-path floor)"),
    "prof_select1":          ("GET", "/prof/select1",          None, False, "async handler + fetchval('SELECT 1') (DB-call floor)"),
})


# ── process management ───────────────────────────────────────────────────────

_children = []


def _kill_all():
    for p in _children:
        if p.poll() is None:
            try:
                os.killpg(p.pid, signal.SIGINT)
            except ProcessLookupError:
                pass
    deadline = time.time() + 3
    for p in _children:
        try:
            p.wait(timeout=max(0.1, deadline - time.time()))
        except subprocess.TimeoutExpired:
            try:
                os.killpg(p.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass


def _on_signal(signum, _frame):
    print(f"\n[paired] signal {signum}: stopping servers", file=sys.stderr)
    _kill_all()
    sys.exit(130)


def port_free(port):
    with socket.socket() as s:
        return s.connect_ex(("127.0.0.1", port)) != 0


def wait_port(port, proc, timeout=30):
    end = time.time() + timeout
    while time.time() < end:
        if proc.poll() is not None:
            raise RuntimeError(f"server on :{port} exited early (code {proc.returncode})")
        with socket.socket() as s:
            if s.connect_ex(("127.0.0.1", port)) == 0:
                return
        time.sleep(0.2)
    raise RuntimeError(f"server on :{port} did not start in {timeout}s")


def http(method, port, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data, method=method,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=10) as r:
        return r.status, r.read()


def start_server(py, port, dsn, pool, logdir, tag):
    log = open(os.path.join(logdir, f"server_{tag}.log"), "w")
    env = dict(os.environ, BENCH_PORT=str(port), BENCH_DSN=dsn, BENCH_POOL=str(pool),
               PYTHONDONTWRITEBYTECODE="1")
    p = subprocess.Popen([py, os.path.join(HERE, "app.py")], cwd=logdir, env=env,
                         stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    _children.append(p)
    _server_pids.append(p.pid)
    return p


def build_info(py, logdir):
    out = subprocess.run(
        [py, "-c", "import cello, sys; print(cello.__version__, cello.__file__, sys.version.split()[0])"],
        cwd=logdir, capture_output=True, text=True, check=True)
    return out.stdout.strip()


# ── wrk ──────────────────────────────────────────────────────────────────────

_UNITS = {"us": 1e-3, "ms": 1.0, "s": 1e3, "m": 60e3}


def _ms(tok):
    m = re.match(r"([\d.]+)(us|ms|s|m)$", tok)
    return float(m.group(1)) * _UNITS[m.group(2)]


def lua_for(method, body, logdir, name):
    if method == "GET":
        return None
    path = os.path.join(logdir, f"{name}.lua")
    with open(path, "w") as f:
        f.write(f'wrk.method = "{method}"\n')
        f.write(f"wrk.body = {json.dumps(json.dumps(body))}\n")
        f.write('wrk.headers["Content-Type"] = "application/json"\n')
    return path


# ── co-tenant CPU accounting ─────────────────────────────────────────────────
# This box is shared: other builds/tests can eat cores mid-run. For each run we
# measure busy CPU on the whole machine and subtract what the benchmark itself
# used (both servers, wrk, and every postgres process), leaving "external"
# cores. Rounds where either side saw more than --max-external cores are re-run.

_TICK = os.sysconf("SC_CLK_TCK")


def _busy_jiffies():
    with open("/proc/stat") as f:
        v = [int(x) for x in f.readline().split()[1:]]
    # user nice system idle iowait irq softirq steal ...
    return v[0] + v[1] + v[2] + v[5] + v[6] + (v[7] if len(v) > 7 else 0)


def _pid_jiffies(pid):
    try:
        with open(f"/proc/{pid}/stat") as f:
            parts = f.read().rsplit(")", 1)[1].split()
        return int(parts[11]) + int(parts[12])  # utime + stime (all threads)
    except (FileNotFoundError, ProcessLookupError, IndexError):
        return 0


def _postgres_jiffies():
    total = 0
    for d in os.listdir("/proc"):
        if d.isdigit():
            try:
                with open(f"/proc/{d}/comm") as f:
                    if f.read().strip().startswith("postgres"):
                        total += _pid_jiffies(d)
            except OSError:
                pass
    return total


_server_pids = []


def _ours_jiffies():
    return sum(_pid_jiffies(p) for p in _server_pids) + _postgres_jiffies()


def run_wrk(port, path, duration, threads, conns, script):
    cmd = [WRK, f"-t{threads}", f"-c{conns}", f"-d{duration}s", "--latency"]
    if script:
        cmd += ["-s", script]
    cmd.append(f"http://127.0.0.1:{port}{path}")
    t0, busy0, ours0 = time.time(), _busy_jiffies(), _ours_jiffies()
    ru0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    out = subprocess.run(cmd, capture_output=True, text=True).stdout
    ru1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    wall = time.time() - t0
    wrk_cpu = (ru1.ru_utime + ru1.ru_stime) - (ru0.ru_utime + ru0.ru_stime)
    ours = (_ours_jiffies() - ours0) / _TICK + wrk_cpu
    external = max(0.0, ((_busy_jiffies() - busy0) / _TICK - ours) / wall)
    rps = float(re.search(r"Requests/sec:\s+([\d.]+)", out).group(1))
    p50 = _ms(re.search(r"^\s+50%\s+(\S+)", out, re.M).group(1))
    p99 = _ms(re.search(r"^\s+99%\s+(\S+)", out, re.M).group(1))
    non2xx = re.search(r"Non-2xx or 3xx responses:\s+(\d+)", out)
    errs = re.search(r"Socket errors:.*", out)
    return {"rps": rps, "p50": p50, "p99": p99,
            "non2xx": int(non2xx.group(1)) if non2xx else 0,
            "socket_errors": errs.group(0) if errs else "",
            "external_cores": external, "raw": out}


# ── reporting ────────────────────────────────────────────────────────────────

def summarize(name, runs_a, runs_b):
    ra = [r["rps"] for r in runs_a]
    rb = [r["rps"] for r in runs_b]
    ma, mb = statistics.mean(ra), statistics.mean(rb)
    ratios = [b / a for a, b in zip(ra, rb)]  # per-round paired ratios
    return {
        "endpoint": name,
        "mean_a": ma, "mean_b": mb,
        "delta_pct": (mb / ma - 1) * 100,
        "min_a": min(ra), "max_a": max(ra), "min_b": min(rb), "max_b": max(rb),
        "cv_a": statistics.pstdev(ra) / ma * 100, "cv_b": statistics.pstdev(rb) / mb * 100,
        "pair_min_pct": (min(ratios) - 1) * 100, "pair_max_pct": (max(ratios) - 1) * 100,
        "pair_median_pct": (statistics.median(ratios) - 1) * 100,
        "p50_a": statistics.median(r["p50"] for r in runs_a),
        "p50_b": statistics.median(r["p50"] for r in runs_b),
        "p99_a": statistics.median(r["p99"] for r in runs_a),
        "p99_b": statistics.median(r["p99"] for r in runs_b),
        "non2xx": sum(r["non2xx"] for r in runs_a + runs_b),
        "ext_max": max(r["external_cores"] for r in runs_a + runs_b),
        "rps_a": ra, "rps_b": rb,
    }


def table(rows):
    hdr = ("| endpoint | mean rps A | mean rps B | delta B vs A (means) | median paired delta "
           "| A min–max | B min–max | paired-round delta range | p50 A / B (ms) | p99 A / B (ms) "
           "| max co-tenant cores | rounds retried / flagged |")
    sep = "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    lines = [hdr, sep]
    for s in rows:
        lines.append(
            f"| {s['endpoint']} | {s['mean_a']:,.0f} | {s['mean_b']:,.0f} | {s['delta_pct']:+.1f}% "
            f"| {s['pair_median_pct']:+.1f}% "
            f"| {s['min_a']:,.0f}–{s['max_a']:,.0f} | {s['min_b']:,.0f}–{s['max_b']:,.0f} "
            f"| {s['pair_min_pct']:+.1f}% … {s['pair_max_pct']:+.1f}% "
            f"| {s['p50_a']:.2f} / {s['p50_b']:.2f} | {s['p99_a']:.2f} / {s['p99_b']:.2f} "
            f"| {s['ext_max']:.1f} | {s.get('rounds_retried', 0)} / {s.get('rounds_flagged_noisy', 0)} |")
    return "\n".join(lines)


# ── main ─────────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--a", required=True, help="python interpreter of venv A (baseline)")
    ap.add_argument("--b", required=True, help="python interpreter of venv B (candidate)")
    ap.add_argument("--port-a", type=int, default=8811)
    ap.add_argument("--port-b", type=int, default=8812)
    ap.add_argument("--dsn", default=os.environ.get(
        "BENCH_DSN", "postgresql://postgres:postgres@127.0.0.1:5432/cello_test"))
    ap.add_argument("--pool", type=int, default=16, help="Postgres pool size per server")
    ap.add_argument("--endpoints", default=",".join(DEFAULT_SET),
                    help=f"comma list from: {','.join(ENDPOINTS)}")
    ap.add_argument("--rounds", type=int, default=10)
    ap.add_argument("--duration", type=int, default=5, help="seconds per wrk run")
    ap.add_argument("--warmup", type=int, default=5, help="seconds of untimed wrk per server per endpoint")
    ap.add_argument("--threads", type=int, default=2)
    ap.add_argument("--conns", type=int, default=32)
    ap.add_argument("--order", choices=["abba", "ab"], default="abba")
    ap.add_argument("--max-external", type=float, default=1.5,
                    help="re-run a round if either run saw more than this many co-tenant cores busy")
    ap.add_argument("--dip", type=float, default=0.8,
                    help="after all rounds, re-run any round where a run fell below this fraction of its "
                         "own build's median rps (host stalls; 0 disables)")
    ap.add_argument("--max-retries", type=int, default=3, help="re-runs per noisy round before keeping it (flagged)")
    ap.add_argument("--retry-wait", type=float, default=3.0, help="seconds to wait before re-running a noisy round")
    ap.add_argument("--no-seed", action="store_true", help="skip (re)seeding the tables")
    ap.add_argument("--json", help="write full results (incl. per-run numbers) to this file")
    ap.add_argument("--logdir", default=None, help="server logs + lua scripts (default: a temp dir)")
    args = ap.parse_args()

    names = [n.strip() for n in args.endpoints.split(",") if n.strip()]
    for n in names:
        if n not in ENDPOINTS:
            ap.error(f"unknown endpoint {n!r}")
    if not os.access(WRK, os.X_OK):
        ap.error(f"wrk not found at {WRK} (set WRK=...)")
    for port in (args.port_a, args.port_b):
        if not port_free(port):
            ap.error(f"port {port} already in use")

    logdir = args.logdir or tempfile.mkdtemp(prefix="cello-paired-")
    os.makedirs(logdir, exist_ok=True)
    signal.signal(signal.SIGINT, _on_signal)
    signal.signal(signal.SIGTERM, _on_signal)

    info_a, info_b = build_info(args.a, logdir), build_info(args.b, logdir)
    print(f"[paired] A: {args.a}\n          cello {info_a}")
    print(f"[paired] B: {args.b}\n          cello {info_b}")
    print(f"[paired] wrk -t{args.threads} -c{args.conns} -d{args.duration}s, "
          f"{args.rounds} rounds, order={args.order}, warmup={args.warmup}s; logs in {logdir}")

    if not args.no_seed:
        subprocess.run([args.a, os.path.join(HERE, "seed.py"), "--dsn", args.dsn],
                       cwd=logdir, check=True)

    results = []
    try:
        pa = start_server(args.a, args.port_a, args.dsn, args.pool, logdir, "A")
        pb = start_server(args.b, args.port_b, args.dsn, args.pool, logdir, "B")
        wait_port(args.port_a, pa)
        wait_port(args.port_b, pb)

        # Sanity: every endpoint answers 2xx on both, and both builds agree on GET bodies.
        for n in names:
            method, path, body, reset, _ = ENDPOINTS[n]
            sa, ba = http(method, args.port_a, path, body)
            sb, bb = http(method, args.port_b, path, body)
            if not (200 <= sa < 300 and 200 <= sb < 300):
                raise RuntimeError(f"{n}: status A={sa} B={sb}")
            if method == "GET" and json.loads(ba) != json.loads(bb):
                print(f"[paired] WARNING: {n} response bodies differ between A and B", file=sys.stderr)

        for n in names:
            method, path, body, reset, label = ENDPOINTS[n]
            script = lua_for(method, body, logdir, n)
            servers = {"A": args.port_a, "B": args.port_b}

            def one(tag, dur):
                if reset:
                    http("POST", servers[tag], "/_bench/reset", {})
                return run_wrk(servers[tag], path, dur, args.threads, args.conns, script)

            if args.warmup > 0:
                one("A", args.warmup)
                one("B", args.warmup)

            runs = {"A": [], "B": []}
            retried = flagged = 0
            for r in range(args.rounds):
                order = ("A", "B") if (args.order == "ab" or r % 2 == 0) else ("B", "A")
                for attempt in range(args.max_retries + 1):
                    got = {}
                    for tag in order:
                        res = one(tag, args.duration)
                        got[tag] = res
                        print(f"  {n:9s} round {r + 1}/{args.rounds} {tag}: {res['rps']:>10,.0f} rps"
                              f"  p50 {res['p50']:.2f}ms  p99 {res['p99']:.2f}ms"
                              f"  ext {res['external_cores']:.1f} cores"
                              + (f"  NON-2xx {res['non2xx']}" if res["non2xx"] else ""), flush=True)
                    worst = max(g["external_cores"] for g in got.values())
                    if worst <= args.max_external:
                        break
                    if attempt < args.max_retries:
                        retried += 1
                        print(f"  {n:9s} round {r + 1}: co-tenant load {worst:.1f} cores "
                              f"> {args.max_external} — discarding round and re-running", flush=True)
                        time.sleep(args.retry_wait)
                    else:
                        flagged += 1
                        print(f"  {n:9s} round {r + 1}: still noisy after {args.max_retries} retries — kept, flagged",
                              flush=True)
                for tag in order:
                    runs[tag].append(got[tag])
            # Dip pass. On this WSL2 box a run sometimes loses 35–60% of its
            # throughput (seen roughly every 30 s on 2026-10-10, hitting A and B
            # alike in A/A runs) with no co-tenant CPU visible in /proc/stat. A
            # CPU-spin probe once showed it as a machine-wide slowdown, but the
            # cause was not pinned down. Each build is compared only with its own
            # median, so a uniform A/B difference never triggers a re-run, however
            # large. A build that dips *intermittently* would be masked: check
            # "rounds retried", or run with --dip 0.
            dips = 0
            if args.dip > 0:
                for attempt in range(args.max_retries):
                    med = {t: statistics.median(x["rps"] for x in runs[t]) for t in runs}
                    bad = [i for i in range(len(runs["A"]))
                           if any(runs[t][i]["rps"] < args.dip * med[t] for t in runs)]
                    if not bad:
                        break
                    for i in bad:
                        dips += 1
                        print(f"  {n:9s} round {i + 1}: dip below {args.dip:.0%} of own median — re-running",
                              flush=True)
                        order = ("A", "B") if (args.order == "ab" or i % 2 == 0) else ("B", "A")
                        for tag in order:
                            res = one(tag, args.duration)
                            runs[tag][i] = res
                            print(f"  {n:9s} round {i + 1}/{args.rounds} {tag}: {res['rps']:>10,.0f} rps (re-run)",
                                  flush=True)
            if reset:
                http("POST", args.port_a, "/_bench/reset", {})
            s = summarize(n, runs["A"], runs["B"])
            s["label"] = label
            s["rounds_retried"] = retried + dips
            s["rounds_flagged_noisy"] = flagged
            results.append(s)
    finally:
        _kill_all()

    print()
    print(f"Paired benchmark — {datetime.now():%Y-%m-%d %H:%M}, wrk -t{args.threads} -c{args.conns} "
          f"-d{args.duration}s × {args.rounds} rounds ({args.order})")
    print(f"A = {info_a}\nB = {info_b}\n")
    print(table(results))
    bad = [s["endpoint"] for s in results if s["non2xx"]]
    if bad:
        print(f"\nWARNING: non-2xx responses on {bad} — those numbers are not valid.")

    if args.json:
        with open(args.json, "w") as f:
            json.dump({"args": vars(args), "a": info_a, "b": info_b, "results": results}, f, indent=2)
        print(f"\n[paired] full results: {args.json}")


if __name__ == "__main__":
    main()
