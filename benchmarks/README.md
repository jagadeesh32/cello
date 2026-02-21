# Cello Benchmarks

Benchmark suite for measuring Cello framework performance.

## Quick Start

### Option 1: Quick Benchmark (No Dependencies)

```bash
# Terminal 1 - Start the server (single worker)
python benchmarks/quick_bench.py --server

# Terminal 2 - Run benchmark
python benchmarks/quick_bench.py --bench
```

### Option 2: Multi-Worker Benchmark (Recommended)

For maximum throughput, use multiple worker processes:

```bash
# Terminal 1 - Start with N workers (e.g., 4 for 4-core machine)
python benchmarks/quick_bench.py --server --workers 4

# Terminal 2 - Run wrk benchmark
wrk -t12 -c400 -d10s http://127.0.0.1:8080/
wrk -t12 -c400 -d10s http://127.0.0.1:8080/json
```

### Option 3: Full Benchmark Suite

Requires: `pip install aiohttp`

```bash
# Terminal 1 - Start the server
python benchmarks/benchmark.py --server

# Terminal 2 - Run benchmark
python benchmarks/benchmark.py --client --concurrency 100 --duration 10
```

### Option 4: Using wrk (Recommended for accurate results)

Install wrk: `sudo apt install wrk` (Linux) or `brew install wrk` (macOS).

```bash
# Terminal 1 - Start the server with workers matching core count
python benchmarks/quick_bench.py --server --workers $(nproc)

# Terminal 2 - Run wrk
wrk -t12 -c400 -d10s http://127.0.0.1:8080/
wrk -t12 -c400 -d10s http://127.0.0.1:8080/json
```

## Benchmark Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | Simple JSON response |
| `GET /json` | JSON with nested data |
| `POST /echo` | Echo POST body |

## Metrics Measured

- **RPS** - Requests per second
- **Latency** - Average, min, max, p50, p95, p99
- **Throughput** - MB/s transferred
- **Errors** - Failed requests

## Expected Results

### Single Worker (1 process)

On a modern machine, expect per-worker:

- Simple JSON: ~25,000-35,000 RPS
- JSON with data: ~20,000-25,000 RPS
- Path parameters: ~25,000-35,000 RPS

### Multi-Worker Mode (recommended for benchmarks)

With `--workers N`, Cello forks N+1 processes (N children + parent), all serving via SO_REUSEPORT.
Each worker runs a single-threaded Tokio event loop for zero GIL contention.
Expect near-linear scaling:

| Workers | Processes | Expected RPS (JSON) |
|---------|-----------|-------------------|
| 2       | 3         | ~70,000-90,000    |
| 4       | 5         | ~160,000-175,000  |
| 8       | 9         | ~200,000+         |

**Reference benchmark**: 170,000+ req/s with 4 workers (5 processes) using `wrk -t12 -c400 -d10s`.

### Platform Notes

- **Native Linux (x86_64)**: Best performance. Intel Xeon / AMD EPYC recommended for production benchmarks.
- **WSL2**: Expect ~40-60% of native performance due to virtual network adapter overhead.
- **macOS**: Good performance on Apple Silicon. Use `wrk` for accurate results.
- **Best practice**: Run wrk on a separate machine to avoid client/server CPU contention.

## Comparison with Other Frameworks

### Benchmark Results (4 workers, 5 processes each, wrk 12t/400c/10s)

| Framework | Server | Requests/sec | Avg Latency | Relative |
|-----------|--------|-------------|-------------|----------|
| **Cello** | Built-in (Rust/Tokio) | **170,000+** | **2.8ms** | **1.0x (fastest)** |
| BlackSheep | Granian (Rust) | ~92,000 | 4.3ms | 1.9x slower |
| FastAPI | Granian (Rust) | ~55,000 | 7.1ms | 3.1x slower |
| Robyn | Built-in (Rust) | ~29,000 | 14.2ms | 5.9x slower |

### How to Reproduce

For fair comparison, use the same machine, worker count, and wrk settings:

```bash
# Cello (4 workers)
python benchmarks/quick_bench.py --server --workers 4
wrk -t12 -c400 -d10s http://127.0.0.1:8080/

# Robyn (4 workers)
python app.py --workers 4
wrk -t12 -c400 -d10s http://127.0.0.1:8080/

# BlackSheep + Granian (4 workers)
granian --interface asgi --workers 4 --host 127.0.0.1 --port 8000 app:app
wrk -t12 -c400 -d10s http://127.0.0.1:8000/

# FastAPI + Granian (4 workers)
granian --interface asgi --workers 4 --host 127.0.0.1 --port 8000 app:app
wrk -t12 -c400 -d10s http://127.0.0.1:8000/
```

### Automated Comparison

Use the automated benchmark runner for reproducible results:

```bash
cd benchmarks/compare
pip install -r requirements.txt
python run_benchmarks.py --workers 4
```
