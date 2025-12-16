#!/usr/bin/env python3
"""
Cello Framework Benchmark Suite
================================

Comprehensive benchmark to measure:
- Requests per second (RPS)
- Latency (avg, min, max, p50, p95, p99)
- Throughput
- Concurrent connection handling

Requirements:
    pip install aiohttp

Usage:
    # First, start the benchmark server:
    python benchmarks/benchmark.py --server

    # Then in another terminal, run the benchmarks:
    python benchmarks/benchmark.py --client

    # Or use external tools like wrk:
    wrk -t4 -c100 -d30s http://127.0.0.1:8080/

Author: Jagadeesh Katla
"""

import argparse
import asyncio
import json
import statistics
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import List

# Check for aiohttp
try:
    import aiohttp
except ImportError:
    print("Please install aiohttp: pip install aiohttp")
    sys.exit(1)


# =============================================================================
# Benchmark Configuration
# =============================================================================

@dataclass
class BenchmarkConfig:
    """Benchmark configuration."""
    host: str = "127.0.0.1"
    port: int = 8080
    duration: int = 10  # seconds
    concurrency: int = 100
    warmup: int = 2  # seconds


@dataclass
class BenchmarkResult:
    """Benchmark result."""
    name: str
    requests: int
    duration: float
    rps: float
    latency_avg: float
    latency_min: float
    latency_max: float
    latency_p50: float
    latency_p95: float
    latency_p99: float
    errors: int
    throughput_mb: float


# =============================================================================
# Benchmark Server (Cello)
# =============================================================================

def run_benchmark_server(host: str = "127.0.0.1", port: int = 8080):
    """Run the Cello benchmark server."""
    try:
        from cello import App, Response
    except ImportError:
        print("Error: cello not installed. Run: maturin develop")
        sys.exit(1)

    app = App()
    
    # Simple JSON endpoint
    @app.get("/")
    def home(request):
        return {"message": "Hello, Cello!"}
    
    # JSON response with data
    @app.get("/json")
    def json_endpoint(request):
        return {
            "success": True,
            "data": {
                "id": 1,
                "name": "Benchmark",
                "items": [1, 2, 3, 4, 5],
                "nested": {"key": "value"}
            }
        }
    
    # Text response
    @app.get("/text")
    def text_endpoint(request):
        return Response.text("Hello, World! This is a plain text response.")
    
    # Path parameter
    @app.get("/users/{id}")
    def user_endpoint(request):
        return {"id": request.params["id"], "name": "User"}
    
    # Query parameters
    @app.get("/search")
    def search_endpoint(request):
        query = request.query.get("q", "")
        return {"query": query, "results": []}
    
    # POST JSON
    @app.post("/echo")
    def echo_endpoint(request):
        return request.json()
    
    # Large JSON response
    @app.get("/large")
    def large_endpoint(request):
        return {
            "items": [{"id": i, "name": f"Item {i}", "data": "x" * 100} for i in range(100)]
        }

    print(f"\n{'='*60}")
    print("  CELLO BENCHMARK SERVER")
    print(f"{'='*60}")
    print(f"\n  Running at: http://{host}:{port}")
    print("\n  Endpoints:")
    print("    GET  /        - Simple JSON")
    print("    GET  /json    - JSON with data")
    print("    GET  /text    - Plain text")
    print("    GET  /users/1 - Path parameter")
    print("    GET  /search  - Query parameters")
    print("    POST /echo    - Echo JSON")
    print("    GET  /large   - Large JSON response")
    print(f"\n{'='*60}")
    print("  Press Ctrl+C to stop")
    print(f"{'='*60}\n")
    
    app.run(host=host, port=port)


# =============================================================================
# Benchmark Client
# =============================================================================

async def benchmark_endpoint(
    session: aiohttp.ClientSession,
    url: str,
    duration: int,
    name: str,
    method: str = "GET",
    json_data: dict = None
) -> BenchmarkResult:
    """Benchmark a single endpoint."""
    latencies: List[float] = []
    errors = 0
    total_bytes = 0
    
    start_time = time.perf_counter()
    end_time = start_time + duration
    
    while time.perf_counter() < end_time:
        req_start = time.perf_counter()
        try:
            if method == "GET":
                async with session.get(url) as response:
                    data = await response.read()
                    total_bytes += len(data)
            elif method == "POST":
                async with session.post(url, json=json_data) as response:
                    data = await response.read()
                    total_bytes += len(data)
            
            latencies.append((time.perf_counter() - req_start) * 1000)  # ms
        except Exception:
            errors += 1
    
    actual_duration = time.perf_counter() - start_time
    
    if not latencies:
        return BenchmarkResult(
            name=name,
            requests=0,
            duration=actual_duration,
            rps=0,
            latency_avg=0,
            latency_min=0,
            latency_max=0,
            latency_p50=0,
            latency_p95=0,
            latency_p99=0,
            errors=errors,
            throughput_mb=0
        )
    
    sorted_latencies = sorted(latencies)
    
    return BenchmarkResult(
        name=name,
        requests=len(latencies),
        duration=actual_duration,
        rps=len(latencies) / actual_duration,
        latency_avg=statistics.mean(latencies),
        latency_min=min(latencies),
        latency_max=max(latencies),
        latency_p50=sorted_latencies[int(len(sorted_latencies) * 0.50)],
        latency_p95=sorted_latencies[int(len(sorted_latencies) * 0.95)],
        latency_p99=sorted_latencies[int(len(sorted_latencies) * 0.99)],
        errors=errors,
        throughput_mb=total_bytes / (1024 * 1024) / actual_duration
    )


async def run_concurrent_benchmark(
    url: str,
    concurrency: int,
    duration: int,
    name: str,
    method: str = "GET",
    json_data: dict = None
) -> BenchmarkResult:
    """Run benchmark with multiple concurrent connections."""
    connector = aiohttp.TCPConnector(limit=concurrency)
    timeout = aiohttp.ClientTimeout(total=30)
    
    async with aiohttp.ClientSession(connector=connector, timeout=timeout) as session:
        # Create concurrent tasks
        tasks = [
            benchmark_endpoint(session, url, duration, name, method, json_data)
            for _ in range(concurrency)
        ]
        
        results = await asyncio.gather(*tasks)
    
    # Aggregate results
    total_requests = sum(r.requests for r in results)
    total_duration = max(r.duration for r in results)
    total_errors = sum(r.errors for r in results)
    total_throughput = sum(r.throughput_mb for r in results)
    
    all_latencies = []
    for r in results:
        if r.latency_avg > 0:
            all_latencies.append(r.latency_avg)
    
    if not all_latencies:
        return BenchmarkResult(
            name=name,
            requests=0,
            duration=total_duration,
            rps=0,
            latency_avg=0,
            latency_min=0,
            latency_max=0,
            latency_p50=0,
            latency_p95=0,
            latency_p99=0,
            errors=total_errors,
            throughput_mb=0
        )
    
    return BenchmarkResult(
        name=name,
        requests=total_requests,
        duration=total_duration,
        rps=total_requests / total_duration,
        latency_avg=statistics.mean([r.latency_avg for r in results if r.latency_avg > 0]),
        latency_min=min(r.latency_min for r in results if r.latency_min > 0),
        latency_max=max(r.latency_max for r in results),
        latency_p50=statistics.mean([r.latency_p50 for r in results if r.latency_p50 > 0]),
        latency_p95=statistics.mean([r.latency_p95 for r in results if r.latency_p95 > 0]),
        latency_p99=statistics.mean([r.latency_p99 for r in results if r.latency_p99 > 0]),
        errors=total_errors,
        throughput_mb=total_throughput
    )


def print_result(result: BenchmarkResult):
    """Print benchmark result."""
    print(f"\n  {result.name}")
    print(f"  {'-' * 50}")
    print(f"  Requests:      {result.requests:,}")
    print(f"  Duration:      {result.duration:.2f}s")
    print(f"  RPS:           {result.rps:,.0f} req/s")
    print(f"  Throughput:    {result.throughput_mb:.2f} MB/s")
    print(f"  Latency:")
    print(f"    Average:     {result.latency_avg:.2f}ms")
    print(f"    Min:         {result.latency_min:.2f}ms")
    print(f"    Max:         {result.latency_max:.2f}ms")
    print(f"    p50:         {result.latency_p50:.2f}ms")
    print(f"    p95:         {result.latency_p95:.2f}ms")
    print(f"    p99:         {result.latency_p99:.2f}ms")
    print(f"  Errors:        {result.errors}")


async def run_benchmarks(config: BenchmarkConfig):
    """Run all benchmarks."""
    base_url = f"http://{config.host}:{config.port}"
    
    print(f"\n{'='*60}")
    print("  CELLO BENCHMARK SUITE")
    print(f"{'='*60}")
    print(f"\n  Target:       {base_url}")
    print(f"  Concurrency:  {config.concurrency}")
    print(f"  Duration:     {config.duration}s per test")
    print(f"  Warmup:       {config.warmup}s")
    
    # Warmup
    print(f"\n  Warming up for {config.warmup}s...")
    await run_concurrent_benchmark(
        f"{base_url}/",
        config.concurrency,
        config.warmup,
        "Warmup"
    )
    print("  Warmup complete!")
    
    results = []
    
    # Benchmark: Simple JSON
    print("\n  Running: Simple JSON endpoint...")
    result = await run_concurrent_benchmark(
        f"{base_url}/",
        config.concurrency,
        config.duration,
        "GET / (Simple JSON)"
    )
    results.append(result)
    print_result(result)
    
    # Benchmark: JSON with data
    print("\n  Running: JSON with data endpoint...")
    result = await run_concurrent_benchmark(
        f"{base_url}/json",
        config.concurrency,
        config.duration,
        "GET /json (JSON with data)"
    )
    results.append(result)
    print_result(result)
    
    # Benchmark: Text
    print("\n  Running: Text endpoint...")
    result = await run_concurrent_benchmark(
        f"{base_url}/text",
        config.concurrency,
        config.duration,
        "GET /text (Plain text)"
    )
    results.append(result)
    print_result(result)
    
    # Benchmark: Path parameter
    print("\n  Running: Path parameter endpoint...")
    result = await run_concurrent_benchmark(
        f"{base_url}/users/123",
        config.concurrency,
        config.duration,
        "GET /users/123 (Path param)"
    )
    results.append(result)
    print_result(result)
    
    # Benchmark: Large JSON
    print("\n  Running: Large JSON endpoint...")
    result = await run_concurrent_benchmark(
        f"{base_url}/large",
        config.concurrency,
        config.duration,
        "GET /large (Large JSON)"
    )
    results.append(result)
    print_result(result)
    
    # Summary
    print(f"\n{'='*60}")
    print("  SUMMARY")
    print(f"{'='*60}")
    print(f"\n  {'Endpoint':<35} {'RPS':>12} {'Avg Latency':>12}")
    print(f"  {'-'*35} {'-'*12} {'-'*12}")
    for r in results:
        print(f"  {r.name:<35} {r.rps:>10,.0f} {r.latency_avg:>10.2f}ms")
    
    avg_rps = statistics.mean([r.rps for r in results])
    avg_latency = statistics.mean([r.latency_avg for r in results])
    
    print(f"\n  Average RPS:     {avg_rps:,.0f} req/s")
    print(f"  Average Latency: {avg_latency:.2f}ms")
    print(f"\n{'='*60}\n")
    
    # Save results to JSON
    results_dict = {
        "config": {
            "host": config.host,
            "port": config.port,
            "concurrency": config.concurrency,
            "duration": config.duration,
        },
        "results": [
            {
                "name": r.name,
                "requests": r.requests,
                "rps": r.rps,
                "latency_avg_ms": r.latency_avg,
                "latency_p99_ms": r.latency_p99,
                "errors": r.errors,
            }
            for r in results
        ],
        "summary": {
            "avg_rps": avg_rps,
            "avg_latency_ms": avg_latency,
        }
    }
    
    with open("benchmark_results.json", "w") as f:
        json.dump(results_dict, f, indent=2)
    print("  Results saved to benchmark_results.json\n")


# =============================================================================
# Main
# =============================================================================

def main():
    parser = argparse.ArgumentParser(description="Cello Benchmark Suite")
    parser.add_argument("--server", action="store_true", help="Run benchmark server")
    parser.add_argument("--client", action="store_true", help="Run benchmark client")
    parser.add_argument("--host", default="127.0.0.1", help="Host (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=8080, help="Port (default: 8080)")
    parser.add_argument("--duration", type=int, default=10, help="Test duration in seconds (default: 10)")
    parser.add_argument("--concurrency", type=int, default=100, help="Concurrent connections (default: 100)")
    parser.add_argument("--warmup", type=int, default=2, help="Warmup duration in seconds (default: 2)")
    
    args = parser.parse_args()
    
    if args.server:
        run_benchmark_server(args.host, args.port)
    elif args.client:
        config = BenchmarkConfig(
            host=args.host,
            port=args.port,
            duration=args.duration,
            concurrency=args.concurrency,
            warmup=args.warmup,
        )
        asyncio.run(run_benchmarks(config))
    else:
        print("Usage:")
        print("  Start server:  python benchmark.py --server")
        print("  Run client:    python benchmark.py --client")
        print("\nOptions:")
        print("  --host HOST        Server host (default: 127.0.0.1)")
        print("  --port PORT        Server port (default: 8080)")
        print("  --duration SECS    Test duration (default: 10)")
        print("  --concurrency N    Concurrent connections (default: 100)")
        print("  --warmup SECS      Warmup duration (default: 2)")


if __name__ == "__main__":
    main()
