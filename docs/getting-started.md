# Getting Started

This guide will help you install Cello and build your first application.

## Installation

### From PyPI (Recommended)

```bash
pip install cello-framework
```

### From Source

```bash
pip install maturin
git clone https://github.com/jagadeesh32/cello.git
cd cello
maturin develop
```

## Requirements

- Python 3.12 or higher
- pip or uv package manager

## Your First Application

Create a file called `app.py`:

```python
from cello import App

app = App()

@app.get("/")
def home(request):
    return {"message": "Hello, Cello!"}

@app.get("/hello/{name}")
def hello(request):
    name = request.params["name"]
    return {"message": f"Hello, {name}!"}

if __name__ == "__main__":
    app.run()
```

Run the application:

```bash
python app.py
```

Test it:

```bash
curl http://127.0.0.1:8000/
# {"message": "Hello, Cello!"}

curl http://127.0.0.1:8000/hello/World
# {"message": "Hello, World!"}
```

## Adding Middleware

Enable built-in middleware for common functionality:

```python
from cello import App

app = App()

# Enable CORS for cross-origin requests
app.enable_cors()

# Enable request logging
app.enable_logging()

# Enable gzip compression
app.enable_compression()

@app.get("/")
def home(request):
    return {"message": "Hello with middleware!"}

if __name__ == "__main__":
    app.run()
```

## Using Blueprints

Organize your routes with blueprints:

```python
from cello import App, Blueprint

# Create a blueprint for API routes
api = Blueprint("/api/v1")

@api.get("/users")
def list_users(request):
    return {"users": []}

@api.get("/users/{id}")
def get_user(request):
    user_id = request.params["id"]
    return {"id": user_id, "name": f"User {user_id}"}

@api.post("/users")
def create_user(request):
    data = request.json()
    return {"id": 1, "name": data.get("name"), "created": True}

# Create the app and register the blueprint
app = App()
app.register_blueprint(api)

if __name__ == "__main__":
    app.run()
```

## Async Handlers

Cello supports both sync and async handlers:

```python
from cello import App

app = App()

# Sync handler - for simple operations
@app.get("/sync")
def sync_handler(request):
    return {"message": "Hello from sync!"}

# Async handler - for I/O operations
@app.get("/async")
async def async_handler(request):
    # Use async libraries like asyncpg, httpx, aiofiles
    data = await fetch_from_database()
    return {"data": data}

if __name__ == "__main__":
    app.run()
```

## Request Object

The request object provides access to:

```python
@app.get("/demo")
def demo(request):
    # Request information
    method = request.method          # "GET", "POST", etc.
    path = request.path              # "/demo"
    
    # Path parameters
    user_id = request.params["id"]   # From "/users/{id}"
    
    # Query parameters
    search = request.query.get("q")  # From "?q=search"
    
    # Headers
    auth = request.get_header("Authorization")
    content_type = request.get_header("Content-Type")
    
    # Body
    raw_bytes = request.body()       # bytes
    text = request.text()            # str
    json_data = request.json()       # dict (parsed JSON)
    form_data = request.form()       # dict (form data)
    
    # Content type helpers
    is_json = request.is_json()
    is_form = request.is_form()
    
    return {"method": method, "path": path}
```

## Response Types

Cello supports multiple response types:

```python
from cello import Response

# JSON response (default - just return a dict)
@app.get("/json")
def json_response(request):
    return {"data": "value"}

# Text response
@app.get("/text")
def text_response(request):
    return Response.text("Hello, World!")

# HTML response
@app.get("/html")
def html_response(request):
    return Response.html("<h1>Hello!</h1>")

# Redirect
@app.get("/old")
def redirect_response(request):
    return Response.redirect("/new")

# Custom response
@app.get("/custom")
def custom_response(request):
    response = Response.json({"ok": True}, status=201)
    response.set_header("X-Custom", "value")
    return response

# No content (204)
@app.delete("/item/{id}")
def delete_response(request):
    return Response.no_content()
```

## Running in Production

For production deployments:

```bash
# With multiple workers
python app.py --env production --workers 4 --port 8080

# Bind to all interfaces (for Docker)
python app.py --host 0.0.0.0 --port 8080 --workers 4
```

## Next Steps

- [Configuration](configuration.md) - All configuration options
- [Middleware](middleware.md) - Built-in and custom middleware
- [Security](security.md) - Authentication and security features
- [Routing](routing.md) - Advanced routing features
- [Deployment](deployment.md) - Production deployment guide
