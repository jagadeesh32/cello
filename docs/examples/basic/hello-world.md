---
title: Hello World
description: Hello World example with full code walkthrough for Cello Framework
---

# Hello World

This example walks through a minimal Cello application step by step, explaining every line.

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
Basic Hello World example for Cello.

Run: python hello.py
Test: curl http://127.0.0.1:8000/
"""

from cello import App

app = App()

@app.get("/")
def home(request):
    return {
        "message": "Welcome to Cello!",
        "version": "0.9.0",
    }

@app.get("/hello/{name}")
def hello(request):
    name = request.params.get("name", "World")
    return {"message": f"Hello, {name}!"}

@app.get("/search")
def search(request):
    query = request.query.get("q", "")
    limit = request.query.get("limit", "10")
    return {"query": query, "limit": int(limit), "results": []}

@app.post("/echo")
def echo(request):
    try:
        body = request.json()
        return {"received": body}
    except Exception as e:
        return {"error": str(e)}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Line-by-Line Walkthrough

### Import and Create the App

```python
from cello import App

app = App()
```

`App` is the main application class. Creating an instance initializes the Rust HTTP engine, the radix tree router, and the middleware pipeline. No configuration is required at construction time.

### Define a GET Route

```python
@app.get("/")
def home(request):
    return {
        "message": "Welcome to Cello!",
        "version": "0.9.0",
    }
```

The `@app.get("/")` decorator registers `home` as the handler for `GET /`. The handler receives a `request` object and returns a Python `dict`, which Cello serializes to JSON using SIMD-accelerated Rust code.

### Path Parameters

```python
@app.get("/hello/{name}")
def hello(request):
    name = request.params.get("name", "World")
    return {"message": f"Hello, {name}!"}
```

`{name}` in the path captures a dynamic URL segment. Access it via `request.params["name"]`. The radix tree router resolves path parameters in approximately 100 nanoseconds.

### Query Parameters

```python
@app.get("/search")
def search(request):
    query = request.query.get("q", "")
    limit = request.query.get("limit", "10")
    return {"query": query, "limit": int(limit), "results": []}
```

Query string values are available through `request.query`, which behaves like a dictionary. All values are strings, so convert numeric values with `int()` or `float()`.

### POST with JSON Body

```python
@app.post("/echo")
def echo(request):
    try:
        body = request.json()
        return {"received": body}
    except Exception as e:
        return {"error": str(e)}
```

`request.json()` parses the request body as JSON using Rust's SIMD JSON parser. Wrapping it in try/except handles cases where the body is missing or malformed.

### Start the Server

```python
if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

`app.run()` starts the Tokio async runtime and begins accepting HTTP connections. The server runs until interrupted with Ctrl+C.

---

## Testing

```bash
# Root endpoint
curl http://127.0.0.1:8000/
# {"message": "Welcome to Cello!", "version": "0.9.0"}

# Path parameter
curl http://127.0.0.1:8000/hello/Alice
# {"message": "Hello, Alice!"}

# Query parameters
curl "http://127.0.0.1:8000/search?q=cello&limit=5"
# {"query": "cello", "limit": 5, "results": []}

# POST with JSON
curl -X POST http://127.0.0.1:8000/echo \
  -H "Content-Type: application/json" \
  -d '{"key": "value"}'
# {"received": {"key": "value"}}
```

---

## What Happens Under the Hood

```
1. Python registers routes with the Rust router at import time
2. app.run() starts the Tokio async runtime
3. Rust accepts TCP connections via hyper
4. Each request is routed through the radix tree (~100ns)
5. The matched Python handler is called
6. The returned dict is serialized via SIMD JSON (~1us/KB)
7. Rust sends the HTTP response
```

All I/O, routing, and serialization happen in Rust. Python only executes your business logic.

---

## Next Steps

- [REST API Example](rest-api.md) - Build a full CRUD API
- [Your First App](../../getting-started/first-app.md) - Detailed step-by-step guide
- [Routing](../../features/core/routing.md) - Advanced routing features
