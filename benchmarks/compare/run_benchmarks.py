#!/usr/bin/env python3
"""
Cello Framework Comparison Benchmarks
======================================

Automated benchmark runner that compares Cello against other Python web frameworks.
Each framework serves the same JSON endpoint: GET / -> {"message": "Hello, World!"}

Requirements:
    - wrk (sudo apt install wrk)
    - All frameworks installed (pip install -r requirements.txt)
    - Cello built (maturin develop --release)

Usage:
    python run_benchmarks.py                        # Run all benchmarks
    python run_benchmarks.py --workers 4            # Use 4 workers per framework
    python run_benchmarks.py --only cello robyn     # Test specific frameworks
    python run_benchmarks.py --duration 15          # 15 second wrk runs
    python run_benchmarks.py --save results.json    # Save results to JSON

Author: Jagadeesh Katla
"""

import argparse
import json
import os
import re
import signal
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

APPS_DIR = Path(__file__).parent / "apps"
DEFAULT_PORT = 8080
WRK_THREADS = 12
WRK_CONNECTIONS = 400
WRK_DURATION = 10
STARTUP_WAIT = 3
SHUTDOWN_WAIT = 2


@dataclass
class BenchmarkResult:
    framework: str
    server: str
    requests_per_sec: float = 0.0
    avg_latency_ms: float = 0.0
    p50_latency_ms: float = 0.0
    p99_latency_ms: float = 0.0
    transfer_per_sec: str = ""
    total_requests: int = 0
    errors: int = 0
    error_msg: str = ""


@dataclass
class FrameworkConfig:
    name: str
    key: str
    server: str
    start_cmd: list[str]
    port: int = DEFAULT_PORT
    env: dict = field(default_factory=dict)
    spawn_count: int = 1  # Start N separate processes (for frameworks that need external multi-process)


def get_cpu_count():
    """Get the number of CPU cores."""
    try:
        return os.cpu_count() or 4
    except Exception:
        return 4


def check_wrk():
    """Check if wrk is installed."""
    try:
        subprocess.run(["wrk", "--version"], capture_output=True, text=True)
        return True
    except FileNotFoundError:
        return False


def get_framework_configs(workers: int, port: int) -> list[FrameworkConfig]:
    """Build the list of framework configurations."""
    apps = str(APPS_DIR)
    configs = []

    # 1. Cello (built-in Rust server with internal multi-worker via os.fork + SO_REUSEPORT)
    configs.append(FrameworkConfig(
        name="Cello",
        key="cello",
        server="Built-in (Rust/Tokio)",
        port=port,
        start_cmd=[
            sys.executable, f"{apps}/cello_app.py",
        ],
        env={"BENCH_WORKERS": str(workers)},
    ))

    # 2. Robyn (built-in Rust server)
    configs.append(FrameworkConfig(
        name="Robyn",
        key="robyn",
        server="Built-in (Rust)",
        port=port,
        start_cmd=[
            sys.executable, f"{apps}/robyn_app.py",
            "--processes", str(workers),
            "--log-level", "WARN",
        ],
    ))

    # 3. BlackSheep + Granian
    # PYTHONPATH must include the compare dir so Granian workers can import apps.*
    granian_env = {"PYTHONPATH": str(APPS_DIR.parent)}
    configs.append(FrameworkConfig(
        name="BlackSheep + Granian",
        key="blacksheep-granian",
        server="Granian (Rust)",
        port=port,
        start_cmd=[
            sys.executable, "-m", "granian",
            "--interface", "asgi",
            "--workers", str(workers),
            "--host", "127.0.0.1",
            "--port", str(port),
            "--no-ws",
            "apps.blacksheep_app:app",
        ],
        env=granian_env,
    ))

    # 4. FastAPI + Granian
    configs.append(FrameworkConfig(
        name="FastAPI + Granian",
        key="fastapi-granian",
        server="Granian (Rust)",
        port=port,
        start_cmd=[
            sys.executable, "-m", "granian",
            "--interface", "asgi",
            "--workers", str(workers),
            "--host", "127.0.0.1",
            "--port", str(port),
            "--no-ws",
            "apps.fastapi_app:app",
        ],
        env=granian_env,
    ))

    return configs


def parse_wrk_output(output: str) -> dict:
    """Parse wrk output and extract metrics."""
    result = {
        "requests_per_sec": 0.0,
        "avg_latency_ms": 0.0,
        "p50_latency_ms": 0.0,
        "p99_latency_ms": 0.0,
        "transfer_per_sec": "",
        "total_requests": 0,
        "errors": 0,
    }

    # Requests/sec
    match = re.search(r"Requests/sec:\s+([\d.]+)", output)
    if match:
        result["requests_per_sec"] = float(match.group(1))

    # Average latency
    match = re.search(r"Latency\s+([\d.]+)(us|ms|s)", output)
    if match:
        val = float(match.group(1))
        unit = match.group(2)
        if unit == "us":
            val /= 1000
        elif unit == "s":
            val *= 1000
        result["avg_latency_ms"] = val

    # Transfer/sec
    match = re.search(r"Transfer/sec:\s+(.+)", output)
    if match:
        result["transfer_per_sec"] = match.group(1).strip()

    # Total requests
    match = re.search(r"(\d+)\s+requests in", output)
    if match:
        result["total_requests"] = int(match.group(1))

    # Socket errors
    match = re.search(
        r"Socket errors:.*?connect\s+(\d+).*?read\s+(\d+).*?write\s+(\d+).*?timeout\s+(\d+)",
        output,
    )
    if match:
        result["errors"] = sum(int(x) for x in match.groups())

    # Non-2xx responses
    match = re.search(r"Non-2xx or 3xx responses:\s+(\d+)", output)
    if match:
        result["errors"] += int(match.group(1))

    return result


def parse_wrk_latency_distribution(output: str) -> dict:
    """Parse wrk latency distribution."""
    result = {"p50_latency_ms": 0.0, "p99_latency_ms": 0.0}

    def to_ms(val_str, unit_str):
        val = float(val_str)
        if unit_str == "us":
            val /= 1000
        elif unit_str == "s":
            val *= 1000
        return val

    match = re.search(r"50%\s+([\d.]+)(us|ms|s)", output)
    if match:
        result["p50_latency_ms"] = to_ms(match.group(1), match.group(2))

    match = re.search(r"99%\s+([\d.]+)(us|ms|s)", output)
    if match:
        result["p99_latency_ms"] = to_ms(match.group(1), match.group(2))

    return result


def wait_for_server(port: int, timeout: int = 10) -> bool:
    """Wait for a server to become available on the given port."""
    import socket
    start = time.time()
    while time.time() - start < timeout:
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(1)
            sock.connect(("127.0.0.1", port))
            sock.close()
            return True
        except (ConnectionRefusedError, OSError, socket.timeout):
            time.sleep(0.3)
    return False


def kill_port(port: int):
    """Kill any process listening on the given port."""
    try:
        result = subprocess.run(
            ["lsof", "-ti", f":{port}"],
            capture_output=True, text=True,
        )
        pids = result.stdout.strip().split()
        for pid in pids:
            if pid:
                try:
                    os.kill(int(pid), signal.SIGKILL)
                except (ProcessLookupError, ValueError):
                    pass
        if pids:
            time.sleep(0.5)
    except FileNotFoundError:
        try:
            subprocess.run(["fuser", "-k", f"{port}/tcp"], capture_output=True)
            time.sleep(0.5)
        except FileNotFoundError:
            pass


def count_port_processes(port: int) -> int:
    """Count how many processes are listening on a port."""
    try:
        result = subprocess.run(
            ["lsof", "-ti", f":{port}"],
            capture_output=True, text=True,
        )
        pids = [p for p in result.stdout.strip().split() if p]
        return len(pids)
    except FileNotFoundError:
        return -1


def run_wrk(port: int, duration: int, threads: int, connections: int) -> str:
    """Run wrk and return its output."""
    cmd = [
        "wrk",
        f"-t{threads}",
        f"-c{connections}",
        f"-d{duration}s",
        "--latency",
        f"http://127.0.0.1:{port}/",
    ]
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=duration + 30)
        return result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return ""


def run_single_benchmark(
    config: FrameworkConfig,
    duration: int,
    threads: int,
    connections: int,
    warmup: int,
) -> BenchmarkResult:
    """Run a benchmark for a single framework."""
    result = BenchmarkResult(
        framework=config.name,
        server=config.server,
    )

    # Ensure port is free
    kill_port(config.port)
    time.sleep(0.5)

    # Build environment
    env = os.environ.copy()
    env.update(config.env)

    # Start the server process(es)
    print(f"    Starting {config.name}...", end=" ", flush=True)
    procs = []
    try:
        for _ in range(config.spawn_count):
            proc = subprocess.Popen(
                config.start_cmd,
                env=env,
                cwd=str(APPS_DIR.parent),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
            )
            procs.append(proc)
            # Small stagger for SO_REUSEPORT processes
            if config.spawn_count > 1:
                time.sleep(0.1)
    except Exception as e:
        result.error_msg = f"Failed to start: {e}"
        print(f"FAILED ({e})")
        for p in procs:
            p.kill()
        return result

    # Wait for server to be ready
    if not wait_for_server(config.port, timeout=20):
        stderr_output = ""
        try:
            stderr_output = procs[0].stderr.read().decode(errors="replace")[:500]
        except Exception:
            pass
        error_detail = "Server did not start in time"
        if stderr_output:
            error_detail += f"\n      stderr: {stderr_output.strip()}"
        result.error_msg = error_detail
        print("TIMEOUT")
        if stderr_output:
            print(f"      stderr: {stderr_output.strip()[:200]}")
        for p in procs:
            p.terminate()
            try:
                p.wait(timeout=5)
            except subprocess.TimeoutExpired:
                p.kill()
        kill_port(config.port)
        return result

    # Allow all processes to finish binding
    if config.spawn_count > 1:
        time.sleep(1)

    # Check how many processes are on the port
    num_procs = count_port_processes(config.port)
    if num_procs > 0:
        print(f"OK ({num_procs} processes)")
    else:
        print("OK")

    # Warmup
    if warmup > 0:
        print(f"    Warming up ({warmup}s)...", end=" ", flush=True)
        run_wrk(config.port, warmup, threads, connections)
        print("done")

    # Run the actual benchmark
    print(f"    Benchmarking ({duration}s, {threads}t, {connections}c)...", end=" ", flush=True)
    wrk_output = run_wrk(config.port, duration, threads, connections)
    print("done")

    # Parse results
    if wrk_output:
        metrics = parse_wrk_output(wrk_output)
        latency = parse_wrk_latency_distribution(wrk_output)
        result.requests_per_sec = metrics["requests_per_sec"]
        result.avg_latency_ms = metrics["avg_latency_ms"]
        result.p50_latency_ms = latency["p50_latency_ms"]
        result.p99_latency_ms = latency["p99_latency_ms"]
        result.transfer_per_sec = metrics["transfer_per_sec"]
        result.total_requests = metrics["total_requests"]
        result.errors = metrics["errors"]
    else:
        result.error_msg = "wrk produced no output"

    # Stop all server processes
    print(f"    Stopping {config.name}...", end=" ", flush=True)
    for p in procs:
        p.terminate()
    for p in procs:
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
    # Also kill any remaining child processes on the port
    kill_port(config.port)
    time.sleep(SHUTDOWN_WAIT)
    print("done")

    return result


def print_results(results: list[BenchmarkResult], workers: int, duration: int):
    """Print a formatted comparison table."""
    print()
    print("=" * 90)
    print(f"  BENCHMARK RESULTS  |  Workers: {workers}  |  wrk: {WRK_THREADS}t / {WRK_CONNECTIONS}c / {duration}s")
    print("=" * 90)
    print()
    print(f"  {'Framework':<25} {'Server':<22} {'Req/sec':>10} {'Avg(ms)':>9} {'p50(ms)':>9} {'p99(ms)':>9}")
    print(f"  {'-' * 25} {'-' * 22} {'-' * 10} {'-' * 9} {'-' * 9} {'-' * 9}")

    sorted_results = sorted(results, key=lambda r: r.requests_per_sec, reverse=True)

    for r in sorted_results:
        if r.error_msg:
            short_err = r.error_msg.split("\n")[0]
            print(f"  {r.framework:<25} {r.server:<22} {'ERROR':>10}   {short_err}")
        else:
            rps = f"{r.requests_per_sec:,.0f}"
            avg = f"{r.avg_latency_ms:.2f}" if r.avg_latency_ms else "N/A"
            p50 = f"{r.p50_latency_ms:.2f}" if r.p50_latency_ms else "N/A"
            p99 = f"{r.p99_latency_ms:.2f}" if r.p99_latency_ms else "N/A"
            print(f"  {r.framework:<25} {r.server:<22} {rps:>10} {avg:>9} {p50:>9} {p99:>9}")

    print()
    print("=" * 90)

    # Show relative performance bar chart
    valid = [r for r in sorted_results if not r.error_msg and r.requests_per_sec > 0]
    if valid:
        best = valid[0]
        print()
        print("  Relative Performance (normalized to fastest):")
        print()
        for r in valid:
            ratio = r.requests_per_sec / best.requests_per_sec
            bar_len = int(ratio * 40)
            bar = "#" * bar_len
            if r == best:
                label = "fastest"
            else:
                label = f"{best.requests_per_sec / r.requests_per_sec:.1f}x slower"
            print(f"  {r.framework:<25} {bar:<40} {ratio:>5.0%}  ({label})")

    print()
    print("=" * 90)
    print()


def save_results(results: list[BenchmarkResult], filepath: str, workers: int, duration: int):
    """Save results to a JSON file."""
    import platform
    data = {
        "benchmark_settings": {
            "wrk_threads": WRK_THREADS,
            "wrk_connections": WRK_CONNECTIONS,
            "duration_seconds": duration,
            "workers": workers,
            "cpu_count": os.cpu_count(),
            "platform": platform.platform(),
        },
        "results": [],
    }
    for r in sorted(results, key=lambda x: x.requests_per_sec, reverse=True):
        data["results"].append({
            "framework": r.framework,
            "server": r.server,
            "requests_per_sec": r.requests_per_sec,
            "avg_latency_ms": r.avg_latency_ms,
            "p50_latency_ms": r.p50_latency_ms,
            "p99_latency_ms": r.p99_latency_ms,
            "transfer_per_sec": r.transfer_per_sec,
            "total_requests": r.total_requests,
            "errors": r.errors,
            "error": r.error_msg or None,
        })

    with open(filepath, "w") as f:
        json.dump(data, f, indent=2)
    print(f"  Results saved to: {filepath}")


def main():
    parser = argparse.ArgumentParser(
        description="Cello Framework Comparison Benchmarks",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python run_benchmarks.py                              Run all benchmarks
  python run_benchmarks.py --workers 4                  Use 4 workers
  python run_benchmarks.py --only cello robyn           Test specific frameworks
  python run_benchmarks.py --duration 15 --save out.json

Available framework keys:
  cello, robyn, blacksheep-granian, fastapi-granian
        """,
    )
    parser.add_argument("--workers", type=int, default=0,
                        help="Workers per framework (default: CPU count)")
    parser.add_argument("--duration", type=int, default=WRK_DURATION,
                        help=f"wrk duration in seconds (default: {WRK_DURATION})")
    parser.add_argument("--threads", type=int, default=WRK_THREADS,
                        help=f"wrk threads (default: {WRK_THREADS})")
    parser.add_argument("--connections", type=int, default=WRK_CONNECTIONS,
                        help=f"wrk connections (default: {WRK_CONNECTIONS})")
    parser.add_argument("--warmup", type=int, default=3,
                        help="Warmup duration in seconds (default: 3)")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT,
                        help=f"Port to use (default: {DEFAULT_PORT})")
    parser.add_argument("--only", nargs="+", metavar="KEY",
                        help="Only benchmark specific frameworks (by key)")
    parser.add_argument("--save", metavar="FILE",
                        help="Save results to JSON file")

    args = parser.parse_args()

    workers = args.workers or get_cpu_count()

    # Check wrk
    if not check_wrk():
        print("\nError: 'wrk' is not installed.")
        print("  Ubuntu/Debian: sudo apt install wrk")
        print("  macOS: brew install wrk")
        sys.exit(1)

    # Get framework configs
    configs = get_framework_configs(workers, args.port)

    # Filter if --only specified
    if args.only:
        valid_keys = {c.key for c in configs}
        for key in args.only:
            if key not in valid_keys:
                print(f"\nError: Unknown framework key '{key}'")
                print(f"  Available: {', '.join(sorted(valid_keys))}")
                sys.exit(1)
        configs = [c for c in configs if c.key in args.only]

    # Print header
    print()
    print("=" * 70)
    print("  CELLO FRAMEWORK COMPARISON BENCHMARKS")
    print("=" * 70)
    print()
    print(f"  CPU cores:    {os.cpu_count()}")
    print(f"  Workers:      {workers}")
    print(f"  wrk:          {args.threads} threads, {args.connections} connections, {args.duration}s")
    print(f"  Warmup:       {args.warmup}s")
    print(f"  Frameworks:   {len(configs)}")
    print()

    # Run benchmarks
    results = []
    for i, config in enumerate(configs, 1):
        print(f"  [{i}/{len(configs)}] {config.name} ({config.server})")
        result = run_single_benchmark(
            config, args.duration, args.threads, args.connections, args.warmup,
        )
        results.append(result)
        print()

    # Print results
    print_results(results, workers, args.duration)

    # Save if requested
    if args.save:
        save_results(results, args.save, workers, args.duration)

    print("  Done! All benchmarks completed.")
    print()


if __name__ == "__main__":
    main()
