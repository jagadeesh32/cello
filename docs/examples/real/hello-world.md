---
title: Hello World
description: A basic Hello World example demonstrating routing, path parameters, query strings, and JSON request/response handling in Cello.
---

# :material-hand-wave: Hello World

This example introduces the fundamentals of building a web API with Cello. It demonstrates how to define GET and POST routes, extract path parameters and query string values, and parse JSON request bodies — all with minimal boilerplate.

## Features Demonstrated

- Registering `GET` and `POST` route handlers with `@app.get()` and `@app.post()`
- Extracting path parameters via `request.params`
- Reading query string values via `request.query`
- Parsing JSON request bodies with `request.json()`
- Returning plain Python dicts as JSON responses

## Complete Source Code

```python
#!/usr/bin/env python3
"""
Basic Hello World example for Cello.

Run with:
    python examples/hello.py

Then test with:
    curl http://127.0.0.1:8000/
    curl http://127.0.0.1:8000/hello/World
    curl http://127.0.0.1:8000/users/123
    curl -X POST http://127.0.0.1:8000/echo -d '{"message": "test"}'
"""

from cello import App

app = App()


@app.get("/")
def home(request):
    return {
        "message": "Welcome to Cello!",
        "version": "1.0.1",
        "docs": "/docs",
    }


@app.get("/hello/{name}")
def hello(request):
    name = request.params.get("name", "World")
    return {"message": f"Hello, {name}!"}


@app.get("/users/{id}")
def get_user(request):
    user_id = request.params.get("id")
    return {"id": user_id, "name": f"User {user_id}", "email": f"user{user_id}@example.com"}


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


@app.post("/users")
def create_user(request):
    try:
        data = request.json()
        return {"id": 1, "name": data.get("name", "Anonymous"), "email": data.get("email", ""), "created": True}
    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print("Starting Cello example server...")
    app.run(host="127.0.0.1", port=8000)
```

## Running This Example

```bash
python examples/hello.py
# then test it:
curl http://127.0.0.1:8000/
curl http://127.0.0.1:8000/hello/World
curl http://127.0.0.1:8000/users/123
curl -X POST http://127.0.0.1:8000/echo -d '{"message": "test"}'
```

## Key Concepts

- **`App()`** — The central object that wires together all routes and middleware. Call `app.run()` to start the ASGI server.
- **Path parameters** — Curly-brace tokens like `{name}` in a route pattern are captured and exposed through `request.params`.
- **Query strings** — Key-value pairs after the `?` in a URL are available via `request.query`, with safe `.get()` defaults.
- **JSON I/O** — Handlers can return any JSON-serialisable Python dict; incoming JSON bodies are parsed on demand with `request.json()`.
