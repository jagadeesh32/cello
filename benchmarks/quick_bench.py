#!/usr/bin/env python3
"""
Quick Benchmark for Cello
=========================

A simple, standalone benchmark that can be run without external dependencies.

Usage:
    Terminal 1: python benchmarks/quick_bench.py --server
    Terminal 2: python benchmarks/quick_bench.py --bench

Author: Jagadeesh Katla
"""

import argparse
import http.client
import json
import statistics
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed


def run_server():
    """Start the Cello benchmark server."""
    try:
        from cello import App, Response
    except ImportError:
        print("Error: Cello not installed. Run: maturin develop")
        sys.exit(1)
    
    app = App()
    
    @app.get("/")
    def home(request):
        return {"message": "Hello, World!"}
    
    @app.get("/json")
    def json_data(request):
        return {
            "id": 1,
            "name": "Cello Framework",
            "version": "0.4.0",
            "features": ["fast", "async", "rust-powered"]
        }
    
    @app.post("/echo")
    def echo(request):
        return request.json()
    
    print("\n" + "=" * 50)
    print("  CELLO BENCHMARK SERVER")
    print("=" * 50)
    print(f"\n  http://127.0.0.1:8080/")
    print("  Press Ctrl+C to stop\n")
    
    app.run(host="127.0.0.1", port=8080)


def make_request(host: str, port: int, path: str) -> float:
    """Make a single request and return latency in ms."""
    conn = http.client.HTTPConnection(host, port, timeout=5)
    start = time.perf_counter()
    try:
        conn.request("GET", path)
        response = conn.getresponse()
        response.read()
        return (time.perf_counter() - start) * 1000
    except Exception as e:
        return -1
    finally:
        conn.close()


def benchmark(host: str, port: int, path: str, requests: int, threads: int) -> dict:
    """Run benchmark with multiple threads."""
    latencies = []
    errors = 0
    
    print(f"\n  Benchmarking {path} ({requests} requests, {threads} threads)...")
    
    start_time = time.perf_counter()
    
    with ThreadPoolExecutor(max_workers=threads) as executor:
        futures = [executor.submit(make_request, host, port, path) for _ in range(requests)]
        
        for future in as_completed(futures):
            latency = future.result()
            if latency >= 0:
                latencies.append(latency)
            else:
                errors += 1
    
    duration = time.perf_counter() - start_time
    
    if not latencies:
        return {"error": "All requests failed"}
    
    sorted_lat = sorted(latencies)
    
    return {
        "path": path,
        "requests": len(latencies),
        "errors": errors,
        "duration": duration,
        "rps": len(latencies) / duration,
        "latency": {
            "avg": statistics.mean(latencies),
            "min": min(latencies),
            "max": max(latencies),
            "p50": sorted_lat[int(len(sorted_lat) * 0.50)],
            "p95": sorted_lat[int(len(sorted_lat) * 0.95)],
            "p99": sorted_lat[int(len(sorted_lat) * 0.99)],
        }
    }


def run_benchmark(host: str = "127.0.0.1", port: int = 8080):
    """Run the benchmark suite."""
    print("\n" + "=" * 60)
    print("  CELLO PERFORMANCE BENCHMARK")
    print("=" * 60)
    
    # Test connectivity
    try:
        conn = http.client.HTTPConnection(host, port, timeout=2)
        conn.request("GET", "/")
        conn.getresponse()
        conn.close()
    except Exception:
        print(f"\n  Error: Cannot connect to http://{host}:{port}")
        print("  Make sure the server is running: python quick_bench.py --server\n")
        sys.exit(1)
    
    # Warmup
    print("\n  Warming up...")
    for _ in range(100):
        make_request(host, port, "/")
    
    results = []
    
    # Test 1: Simple endpoint
    result = benchmark(host, port, "/", requests=10000, threads=50)
    results.append(result)
    
    # Test 2: JSON endpoint
    result = benchmark(host, port, "/json", requests=10000, threads=50)
    results.append(result)
    
    # Print results
    print("\n" + "=" * 60)
    print("  RESULTS")
    print("=" * 60)
    
    for r in results:
        if "error" in r:
            print(f"\n  {r['path']}: ERROR - {r['error']}")
            continue
        
        print(f"\n  Endpoint: {r['path']}")
        print(f"  {'-' * 40}")
        print(f"  Requests:     {r['requests']:,}")
        print(f"  Errors:       {r['errors']}")
        print(f"  Duration:     {r['duration']:.2f}s")
        print(f"  RPS:          {r['rps']:,.0f} req/s")
        print(f"  Latency (ms):")
        print(f"    Average:    {r['latency']['avg']:.2f}")
        print(f"    Min:        {r['latency']['min']:.2f}")
        print(f"    Max:        {r['latency']['max']:.2f}")
        print(f"    p50:        {r['latency']['p50']:.2f}")
        print(f"    p95:        {r['latency']['p95']:.2f}")
        print(f"    p99:        {r['latency']['p99']:.2f}")
    
    # Summary
    valid_results = [r for r in results if "error" not in r]
    if valid_results:
        avg_rps = statistics.mean([r["rps"] for r in valid_results])
        avg_latency = statistics.mean([r["latency"]["avg"] for r in valid_results])
        
        print("\n" + "=" * 60)
        print("  SUMMARY")
        print("=" * 60)
        print(f"\n  Average RPS:     {avg_rps:,.0f} requests/second")
        print(f"  Average Latency: {avg_latency:.2f}ms")
        print()
        
        # Performance rating
        if avg_rps > 50000:
            print("  🚀 EXCELLENT - Production ready for high-traffic apps!")
        elif avg_rps > 20000:
            print("  ✅ GREAT - Handles heavy loads efficiently!")
        elif avg_rps > 10000:
            print("  👍 GOOD - Solid performance for most use cases.")
        else:
            print("  ⚠️  MODERATE - Consider optimizing for production.")
    
    print("\n" + "=" * 60 + "\n")


def main():
    parser = argparse.ArgumentParser(description="Cello Quick Benchmark")
    parser.add_argument("--server", action="store_true", help="Run the server")
    parser.add_argument("--bench", action="store_true", help="Run the benchmark")
    parser.add_argument("--host", default="127.0.0.1", help="Server host")
    parser.add_argument("--port", type=int, default=8080, help="Server port")
    
    args = parser.parse_args()
    
    if args.server:
        run_server()
    elif args.bench:
        run_benchmark(args.host, args.port)
    else:
        print("\nCello Quick Benchmark")
        print("=" * 40)
        print("\nUsage:")
        print("  Terminal 1: python quick_bench.py --server")
        print("  Terminal 2: python quick_bench.py --bench")
        print()


if __name__ == "__main__":
    main()
