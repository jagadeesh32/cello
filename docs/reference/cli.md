---
title: CLI Reference
description: Command-line options and environment variables for running Cello applications
---

# CLI Reference

Cello applications are run directly with `python app.py`. Command-line arguments configure the server at startup and override any values passed to `app.run()` in code.

---

## Usage

```bash
python app.py [OPTIONS]
```

---

## Options

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--host HOST` | `str` | `127.0.0.1` | IP address to bind to. Use `0.0.0.0` to accept connections on all interfaces. |
| `--port PORT` | `int` | `8000` | TCP port to listen on. |
| `--env ENV` | `str` | `development` | Environment name. Use `production` to disable debug mode and verbose logging. |
| `--workers N` | `int` | CPU count | Number of Tokio worker threads. |
| `--debug` | flag | Off in prod | Enable debug mode with verbose error pages. |
| `--reload` | flag | Off | Enable hot reload. Watches `.py` files and restarts on changes. |
| `--no-logs` | flag | Off | Disable request logging output. |

---

## Examples

### Development Server

```bash
python app.py
```

Starts on `127.0.0.1:8000` with debug mode and logging enabled.

### Production Server

```bash
python app.py --host 0.0.0.0 --port 8000 --env production --workers 8
```

### Hot Reload (Development)

```bash
python app.py --reload
```

Watches all `.py` files in the current directory. When a change is detected, the server restarts automatically.

### Custom Port

```bash
python app.py --port 3000
```

### Disable Logging

```bash
python app.py --no-logs
```

---

## Environment Variables

You can use environment variables instead of (or in addition to) command-line flags. Read them in your application code:

```python
import os

app.run(
    host=os.environ.get("HOST", "0.0.0.0"),
    port=int(os.environ.get("PORT", "8000")),
    env=os.environ.get("CELLO_ENV", "development"),
    workers=int(os.environ.get("WORKERS", "4")),
)
```

### Common Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `CELLO_ENV` | Environment name | `production` |
| `HOST` | Bind address | `0.0.0.0` |
| `PORT` | Bind port | `8000` |
| `WORKERS` | Worker thread count | `8` |
| `JWT_SECRET` | JWT signing secret | Random string |
| `DATABASE_URL` | Database connection string | `postgresql://...` |
| `CELLO_RUN_MAIN` | Internal: set by the reloader | `true` |

---

## Precedence

When the same setting is specified in multiple places, the following order applies (highest priority first):

1. Command-line arguments (`--port 3000`)
2. Values passed to `app.run()` (`port=3000`)
3. Defaults

---

## Programmatic Equivalent

Every CLI flag corresponds to a parameter on `app.run()`:

```python
app.run(
    host="0.0.0.0",       # --host
    port=8000,             # --port
    env="production",      # --env
    workers=8,             # --workers
    debug=False,           # --debug
    reload=False,          # --reload
    logs=False,            # --no-logs
)
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Clean shutdown (Ctrl+C) |
| `1` | Startup error (port in use, invalid config, etc.) |
