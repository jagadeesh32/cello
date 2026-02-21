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

- Simple JSON: ~12,000-15,000 RPS
- JSON with data: ~10,000-12,000 RPS
- Path parameters: ~12,000-15,000 RPS

### Multi-Worker Mode (recommended for benchmarks)

With `--workers N` (where N = core count), expect near-linear scaling:

| Cores | Expected RPS (JSON) |
|-------|-------------------|
| 2     | ~25,000-30,000    |
| 4     | ~50,000-60,000    |
| 8     | ~100,000-120,000  |

**Reference benchmark**: 150,000+ req/s on a 4-core Linux server (native, not WSL2) with `wrk -t12 -c400 -d10s`.

### Platform Notes

- **Native Linux (x86_64)**: Best performance. Intel Xeon / AMD EPYC recommended for production benchmarks.
- **WSL2**: Expect ~40-60% of native performance due to virtual network adapter overhead.
- **macOS**: Good performance on Apple Silicon. Use `wrk` for accurate results.
- **Best practice**: Run wrk on a separate machine to avoid client/server CPU contention.

## Comparison with Other Frameworks

### Expected Results (4 workers, 4 cores, wrk 12t/400c)

| Framework | Server | Requests/sec |
|-----------|--------|-------------|
| **Cello** | Built-in (Rust) | **150,000+** |
| Robyn | Built-in (Rust) | ~40,000 |
| FastAPI | Granian (Rust) | ~25,000 |
| Flask | Granian (Rust) | ~18,000 |
| FastAPI | Uvicorn | ~12,000 |
| Flask | Gunicorn | ~3,000 |

### How to Reproduce

For fair comparison, use the same machine, worker count, and wrk settings:

```bash
# Cello (4 workers)
python benchmarks/quick_bench.py --server --workers 4
wrk -t12 -c400 -d10s http://127.0.0.1:8080/

# Robyn (4 workers)
python app.py --workers 4
wrk -t12 -c400 -d10s http://127.0.0.1:8080/

# FastAPI + Granian (4 workers)
granian --interface asgi --workers 4 --host 127.0.0.1 --port 8000 app:app
wrk -t12 -c400 -d10s http://127.0.0.1:8000/

# Flask + Granian (4 workers)
granian --interface wsgi --workers 4 --host 127.0.0.1 --port 8000 app:app
wrk -t12 -c400 -d10s http://127.0.0.1:8000/

# FastAPI + Uvicorn (4 workers)
uvicorn app:app --workers 4 --host 127.0.0.1 --port 8000
wrk -t12 -c400 -d10s http://127.0.0.1:8000/

# Flask + Gunicorn (4 workers)
gunicorn -w 4 -b 127.0.0.1:5000 app:app
wrk -t12 -c400 -d10s http://127.0.0.1:5000/
```
