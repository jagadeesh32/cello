---
title: Your First App
description: Build a complete REST API step-by-step with Cello Framework
---

# Your First App

This guide walks you through building a complete REST API with Cello, from project setup to a running server with JSON endpoints, path parameters, and error handling.

---

## Prerequisites

Before starting, make sure you have:

- Python 3.12 or newer
- Rust toolchain (`rustup default stable`)
- pip and a virtual environment

---

## Project Setup

### 1. Create the Project Directory

```bash
mkdir my-cello-app
cd my-cello-app
python -m venv .venv
source .venv/bin/activate  # Linux/macOS
# .venv\Scripts\activate   # Windows
```

### 2. Install Cello

```bash
pip install maturin
git clone https://github.com/jagadeesh32/cello.git
cd cello
maturin develop
cd ..
```

### 3. Create Your Application File

Create `app.py` in your project root:

```python
from cello import App

app = App()

@app.get("/")
def home(request):
    return {"message": "Hello, Cello!"}

if __name__ == "__main__":
    app.run()
```

### 4. Run the Server

```bash
python app.py
```

Visit `http://127.0.0.1:8000/` in your browser. You should see:

```json
{"message": "Hello, Cello!"}
```

---

## Adding Routes

### GET with Path Parameters

Capture dynamic URL segments using `{parameter}` syntax:

```python
@app.get("/hello/{name}")
def greet(request):
    name = request.params["name"]
    return {"message": f"Hello, {name}!"}
```

Test it:

```bash
curl http://127.0.0.1:8000/hello/Alice
# {"message": "Hello, Alice!"}
```

### Query Parameters

Access query string values through `request.query`:

```python
@app.get("/search")
def search(request):
    query = request.query.get("q", "")
    limit = int(request.query.get("limit", "10"))
    return {
        "query": query,
        "limit": limit,
        "results": [],
    }
```

Test it:

```bash
curl "http://127.0.0.1:8000/search?q=python&limit=5"
# {"query": "python", "limit": 5, "results": []}
```

---

## Handling JSON

### Reading Request Body

Use `request.json()` to parse the JSON body (powered by SIMD JSON in Rust):

```python
@app.post("/users")
def create_user(request):
    data = request.json()
    user = {
        "id": 1,
        "name": data.get("name", "Anonymous"),
        "email": data.get("email", ""),
    }
    return user
```

Test it:

```bash
curl -X POST http://127.0.0.1:8000/users \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "email": "alice@example.com"}'
# {"id": 1, "name": "Alice", "email": "alice@example.com"}
```

### Returning JSON

Any `dict` returned from a handler is automatically serialized to JSON. Cello uses SIMD-accelerated serialization in Rust:

```python
@app.get("/status")
def status(request):
    return {
        "status": "healthy",
        "version": "1.0.1",
        "uptime_seconds": 3600,
    }
```

---

## Multiple HTTP Methods

### CRUD Operations

Build a complete set of endpoints for a resource:

```python
from cello import App, Response

app = App()

# In-memory storage (use a database in production)
items = {}
next_id = 1

@app.get("/items")
def list_items(request):
    """List all items."""
    return {"items": list(items.values())}

@app.get("/items/{id}")
def get_item(request):
    """Get a single item by ID."""
    item_id = request.params["id"]
    item = items.get(item_id)
    if not item:
        return Response.json({"error": "Item not found"}, status=404)
    return item

@app.post("/items")
def create_item(request):
    """Create a new item."""
    global next_id
    data = request.json()
    item = {
        "id": str(next_id),
        "name": data.get("name", ""),
        "description": data.get("description", ""),
    }
    items[item["id"]] = item
    next_id += 1
    return Response.json(item, status=201)

@app.put("/items/{id}")
def update_item(request):
    """Update an existing item."""
    item_id = request.params["id"]
    if item_id not in items:
        return Response.json({"error": "Item not found"}, status=404)
    data = request.json()
    items[item_id].update({
        "name": data.get("name", items[item_id]["name"]),
        "description": data.get("description", items[item_id]["description"]),
    })
    return items[item_id]

@app.delete("/items/{id}")
def delete_item(request):
    """Delete an item."""
    item_id = request.params["id"]
    if item_id not in items:
        return Response.json({"error": "Item not found"}, status=404)
    del items[item_id]
    return Response.json({"deleted": True}, status=200)
```

---

## Error Handling

### Returning Error Responses

Use `Response.json()` with a status code for error responses:

```python
from cello import Response

@app.get("/users/{id}")
def get_user(request):
    user_id = request.params["id"]

    try:
        uid = int(user_id)
    except ValueError:
        return Response.json(
            {"error": "Invalid user ID", "detail": "ID must be an integer"},
            status=400,
        )

    user = db.get_user(uid)
    if not user:
        return Response.json(
            {"error": "User not found"},
            status=404,
        )

    return user
```

### Catching Exceptions

Wrap handler logic in try/except for robustness:

```python
@app.post("/process")
def process(request):
    try:
        data = request.json()
    except Exception:
        return Response.json({"error": "Invalid JSON body"}, status=400)

    try:
        result = expensive_operation(data)
        return {"result": result}
    except ValueError as e:
        return Response.json({"error": str(e)}, status=422)
    except Exception as e:
        return Response.json({"error": "Internal server error"}, status=500)
```

---

## Adding Headers

### Reading Request Headers

```python
@app.get("/whoami")
def whoami(request):
    user_agent = request.get_header("User-Agent", "Unknown")
    content_type = request.get_header("Content-Type")
    return {
        "user_agent": user_agent,
        "content_type": content_type,
    }
```

### Setting Response Headers

```python
@app.get("/custom-headers")
def custom(request):
    response = Response.json({"data": "value"})
    response.set_header("X-Custom-Header", "my-value")
    response.set_header("X-Request-Id", "abc-123")
    return response
```

---

## Running the App

### Development Mode

```bash
python app.py
# Server starts on http://127.0.0.1:8000
```

### Custom Host and Port

```bash
python app.py --host 0.0.0.0 --port 3000
```

### Production Mode

```bash
python app.py --env production --workers 4
```

### With Hot Reload

```bash
python app.py --reload
```

---

## Complete Application

Here is the full `app.py` with everything from this guide:

```python
from cello import App, Response

app = App()

# Enable built-in features
app.enable_cors()
app.enable_logging()

# In-memory storage
items = {}
next_id = 1

@app.get("/")
def home(request):
    return {"message": "Welcome to My API", "version": "1.0.1"}

@app.get("/items")
def list_items(request):
    return {"items": list(items.values()), "count": len(items)}

@app.get("/items/{id}")
def get_item(request):
    item_id = request.params["id"]
    item = items.get(item_id)
    if not item:
        return Response.json({"error": "Not found"}, status=404)
    return item

@app.post("/items")
def create_item(request):
    global next_id
    data = request.json()
    item = {"id": str(next_id), "name": data["name"]}
    items[item["id"]] = item
    next_id += 1
    return Response.json(item, status=201)

@app.delete("/items/{id}")
def delete_item(request):
    item_id = request.params["id"]
    if item_id not in items:
        return Response.json({"error": "Not found"}, status=404)
    del items[item_id]
    return {"deleted": True}

if __name__ == "__main__":
    app.run()
```

---

## Next Steps

- [Project Structure](project-structure.md) - Organize a larger application
- [Configuration](configuration.md) - Customize app settings
- [Routing](../features/core/routing.md) - Advanced routing features
- [Middleware](../features/middleware/overview.md) - Add CORS, logging, compression
