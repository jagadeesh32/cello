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
    """Root endpoint returning a welcome message."""
    return {
        "message": "Welcome to Cello!",
        "version": "0.4.0",
        "docs": "/docs",
    }


@app.get("/hello/{name}")
def hello(request):
    """Greet a user by name from path parameter."""
    name = request.params.get("name", "World")
    return {"message": f"Hello, {name}!"}


@app.get("/users/{id}")
def get_user(request):
    """Get a user by ID."""
    user_id = request.params.get("id")
    return {
        "id": user_id,
        "name": f"User {user_id}",
        "email": f"user{user_id}@example.com",
    }


@app.get("/search")
def search(request):
    """Search endpoint demonstrating query parameters."""
    query = request.query.get("q", "")
    limit = request.query.get("limit", "10")
    return {
        "query": query,
        "limit": int(limit),
        "results": [],
    }


@app.post("/echo")
def echo(request):
    """Echo back the request body as JSON."""
    try:
        body = request.json()
        return {"received": body}
    except Exception as e:
        return {"error": str(e)}


@app.post("/users")
def create_user(request):
    """Create a new user."""
    try:
        data = request.json()
        return {
            "id": 1,
            "name": data.get("name", "Anonymous"),
            "email": data.get("email", ""),
            "created": True,
        }
    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print("Starting Cello example server...")
    app.run(host="127.0.0.1", port=8000)
