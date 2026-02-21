# Cello Framework Comparison Benchmarks

Reproducible benchmarks comparing Cello against other Python web frameworks.

## Quick Start

```bash
# Install all frameworks
pip install -r requirements.txt

# Run the full comparison
python run_benchmarks.py

# Or test specific frameworks
python run_benchmarks.py --only cello robyn blacksheep-granian fastapi-granian

# Custom worker count
python run_benchmarks.py --workers 4 --duration 10
```

## Requirements

- Python 3.12+
- `wrk` benchmarking tool (`sudo apt install wrk` on Ubuntu/Debian)
- All frameworks installed (see requirements.txt)

## What's Tested

Each framework serves the same JSON endpoint: `GET /` returning `{"message": "Hello, World!"}`.
All frameworks run with the **same number of workers** and **same number of processes** on the **same machine**.

With `--workers N`, every framework creates **N+1 processes** (N workers + 1 supervisor/parent),
ensuring a completely fair comparison.

| Framework | Server | Protocol |
|-----------|--------|----------|
| Cello | Built-in (Rust/Tokio) | HTTP/1.1 |
| Robyn | Built-in (Rust) | HTTP/1.1 |
| BlackSheep + Granian | Granian (Rust) | HTTP/1.1 |
| FastAPI + Granian | Granian (Rust) | HTTP/1.1 |

## Latest Results (4 workers, 5 processes each)

| Framework | Req/sec | Avg Latency | p99 Latency | Relative |
|-----------|---------|-------------|-------------|----------|
| **Cello** | **170,000+** | **2.8ms** | **15ms** | **1.0x (fastest)** |
| BlackSheep + Granian | ~92,000 | 4.3ms | 13ms | 1.9x slower |
| FastAPI + Granian | ~55,000 | 7.1ms | 17ms | 3.1x slower |
| Robyn | ~29,000 | 14.2ms | 38ms | 5.9x slower |

## Benchmark Settings

- **Tool**: wrk
- **Threads**: 12
- **Connections**: 400
- **Duration**: 10 seconds
- **Workers**: Auto-detected (CPU count) or configurable via `--workers`
- **Processes**: N+1 per framework (fair comparison)
