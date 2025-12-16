# Cello Benchmarks

Benchmark suite for measuring Cello framework performance.

## Quick Start

### Option 1: Quick Benchmark (No Dependencies)

```bash
# Terminal 1 - Start the server
python benchmarks/quick_bench.py --server

# Terminal 2 - Run benchmark
python benchmarks/quick_bench.py --bench
```

### Option 2: Full Benchmark Suite

Requires: `pip install aiohttp`

```bash
# Terminal 1 - Start the server
python benchmarks/benchmark.py --server

# Terminal 2 - Run benchmark
python benchmarks/benchmark.py --client --concurrency 100 --duration 10
```

### Option 3: Using wrk (Recommended for accurate results)

Install wrk: `brew install wrk` (macOS) or build from source.

```bash
# Terminal 1 - Start the server
python benchmarks/quick_bench.py --server

# Terminal 2 - Run wrk
wrk -t4 -c100 -d30s http://127.0.0.1:8080/
wrk -t4 -c100 -d30s http://127.0.0.1:8080/json
```

## Benchmark Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | Simple JSON response |
| `GET /json` | JSON with nested data |
| `GET /text` | Plain text response |
| `GET /users/{id}` | Path parameter handling |
| `GET /large` | Large JSON response |
| `POST /echo` | Echo POST body |

## Metrics Measured

- **RPS** - Requests per second
- **Latency** - Average, min, max, p50, p95, p99
- **Throughput** - MB/s transferred
- **Errors** - Failed requests

## Expected Results

On a modern machine (M1 Mac, 8-core), expect:

- Simple JSON: 50,000+ RPS
- JSON with data: 30,000+ RPS
- Path parameters: 40,000+ RPS

## Comparison with Other Frameworks

For fair comparison, use the same machine and settings:

```bash
# Cello
wrk -t4 -c100 -d30s http://127.0.0.1:8080/

# Flask
wrk -t4 -c100 -d30s http://127.0.0.1:5000/

# FastAPI
wrk -t4 -c100 -d30s http://127.0.0.1:8000/
```
