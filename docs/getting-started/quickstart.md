---
title: Quick Start
description: Create your first Cello application in 5 minutes
---

# Quick Start

Learn the basics of Cello in 5 minutes.

## Create Your First App

### 1. Create a file

```python title="app.py"
from cello import App, Response

app = App()

@app.get("/")
def home(request):
    return {"message": "Hello, Cello!"}

@app.get("/users/{id}")
def get_user(request):
    user_id = request.params["id"]
    return {"id": user_id, "name": "John Doe"}

@app.post("/users")
def create_user(request):
    data = request.json()
    return Response.json({"id": 1, **data}, status=201)

if __name__ == "__main__":
    app.run()
```

### 2. Run the app

```bash
python app.py
```

Output:
```
🚀 Cello running at http://127.0.0.1:8000
```

### 3. Test it

```bash
# GET request
curl http://127.0.0.1:8000/
# {"message": "Hello, Cello!"}

# GET with path parameter
curl http://127.0.0.1:8000/users/123
# {"id": "123", "name": "John Doe"}

# POST request
curl -X POST http://127.0.0.1:8000/users \
  -H "Content-Type: application/json" \
  -d '{"name": "Jane"}'
# {"id": 1, "name": "Jane"}
```

## Core Concepts

### Routes

Define routes using decorators:

```python
@app.get("/path")      # GET request
@app.post("/path")     # POST request
@app.put("/path")      # PUT request
@app.patch("/path")    # PATCH request
@app.delete("/path")   # DELETE request
```

### Path Parameters

Capture dynamic parts of the URL:

```python
@app.get("/users/{user_id}/posts/{post_id}")
def get_post(request):
    user_id = request.params["user_id"]
    post_id = request.params["post_id"]
    return {"user_id": user_id, "post_id": post_id}
```

### Query Parameters

Access query string values:

```python
# GET /search?q=python&limit=10
@app.get("/search")
def search(request):
    query = request.query.get("q", "")
    limit = int(request.query.get("limit", "10"))
    return {"query": query, "limit": limit}
```

### Request Body

Parse JSON request bodies:

```python
@app.post("/items")
def create_item(request):
    data = request.json()  # Parse JSON body
    name = data.get("name")
    price = data.get("price")
    return {"name": name, "price": price}
```

### Response Types

Return different response types:

```python
# JSON (default - just return a dict)
return {"key": "value"}

# Explicit JSON with status
return Response.json({"created": True}, status=201)

# Text
return Response.text("Hello, World!")

# HTML
return Response.html("<h1>Welcome</h1>")

# Redirect
return Response.redirect("/new-location")

# No content (204)
return Response.no_content()
```

### Headers

Access and set headers:

```python
@app.get("/headers")
def headers(request):
    # Read header
    auth = request.get_header("Authorization")

    # Set header in response
    response = Response.json({"auth": auth})
    response.set_header("X-Custom", "value")
    return response
```

## Async Handlers

Use async handlers for I/O operations:

```python
@app.get("/async")
async def async_handler(request):
    # Await async operations
    data = await fetch_from_database()
    return {"data": data}
```

## Middleware

Enable built-in middleware:

```python
app = App()

# CORS
app.enable_cors()

# Logging
app.enable_logging()

# Compression
app.enable_compression()

# Rate limiting (100 requests per minute)
app.enable_rate_limit(requests=100, window=60)
```

## Blueprints

Organize routes with blueprints:

```python
from cello import App, Blueprint

# Create blueprint
api = Blueprint("/api/v1")

@api.get("/users")
def list_users(request):
    return {"users": []}

@api.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}

# Register blueprint
app = App()
app.register_blueprint(api)
```

## CLI Options

Run with different options:

```bash
# Custom host and port
python app.py --host 0.0.0.0 --port 8080

# Production mode with workers
python app.py --env production --workers 4

# Development with hot reload
python app.py --env development --reload

# Debug mode
python app.py --debug
```

## What's Next?

<div class="grid cards" markdown>

-   :material-application:{ .lg .middle } **First Application**

    ---

    Build a complete REST API step by step

    [:octicons-arrow-right-24: First App](first-app.md)

-   :material-feature-search:{ .lg .middle } **Features**

    ---

    Explore all Cello features

    [:octicons-arrow-right-24: Features](../features/index.md)

-   :material-shield:{ .lg .middle } **Security**

    ---

    Add authentication and security

    [:octicons-arrow-right-24: Security](../features/security/overview.md)

-   :material-school:{ .lg .middle } **Tutorials**

    ---

    Learn through hands-on tutorials

    [:octicons-arrow-right-24: Learn](../learn/index.md)

</div>
