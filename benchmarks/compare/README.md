# Cello Framework Comparison Benchmarks

Reproducible benchmarks comparing Cello against other Python web frameworks.

## Quick Start

```bash
# Install all frameworks
pip install -r requirements.txt

# Run the full comparison
python run_benchmarks.py

# Or test specific frameworks
python run_benchmarks.py --only cello robyn fastapi-uvicorn
```

## Requirements

- Python 3.12+
- `wrk` benchmarking tool (`sudo apt install wrk` on Ubuntu/Debian)
- All frameworks installed (see requirements.txt)

## What's Tested

Each framework serves the same JSON endpoint: `GET /` returning `{"message": "Hello, World!"}`.
All frameworks run with the **same number of workers** on the **same machine**.

| Framework | Server | Protocol |
|-----------|--------|----------|
| Cello | Built-in (Rust/Tokio) | HTTP/1.1 |
| Robyn | Built-in (Rust) | HTTP/1.1 |
| BlackSheep + Granian | Granian (Rust) | HTTP/1.1 |
| FastAPI + Granian | Granian (Rust) | HTTP/1.1 |

## Benchmark Settings

- **Tool**: wrk
- **Threads**: 12
- **Connections**: 400
- **Duration**: 10 seconds
- **Workers**: Auto-detected (CPU count) or configurable via `--workers`
