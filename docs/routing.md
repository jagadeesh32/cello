# Routing

This guide covers routing in Cello including path parameters, blueprints, and advanced routing.

## Basic Routing

### Route Registration

```python
from cello import App

app = App()

# GET request
@app.get("/")
def home(request):
    return {"message": "Hello!"}

# POST request
@app.post("/users")
def create_user(request):
    return {"id": 1}

# PUT request
@app.put("/users/{id}")
def update_user(request):
    return {"updated": True}

# DELETE request
@app.delete("/users/{id}")
def delete_user(request):
    return {"deleted": True}

# PATCH request
@app.patch("/users/{id}")
def patch_user(request):
    return {"patched": True}

# OPTIONS request
@app.options("/users")
def user_options(request):
    return Response.no_content()

# HEAD request
@app.head("/users")
def user_head(request):
    return Response.no_content()
```

### Multi-Method Routes

Register a handler for multiple HTTP methods:

```python
@app.route("/resource", methods=["GET", "POST"])
def resource_handler(request):
    if request.method == "GET":
        return {"items": []}
    elif request.method == "POST":
        return {"created": True}
```

## Path Parameters

Extract values from URL paths:

```python
# Single parameter
@app.get("/users/{id}")
def get_user(request):
    user_id = request.params["id"]
    return {"id": user_id}

# Multiple parameters
@app.get("/users/{user_id}/posts/{post_id}")
def get_post(request):
    user_id = request.params["user_id"]
    post_id = request.params["post_id"]
    return {"user_id": user_id, "post_id": post_id}

# Alternative access
@app.get("/items/{id}")
def get_item(request):
    item_id = request.get_param("id")  # Returns None if missing
    item_id = request.get_param("id", "default")  # With default
    return {"id": item_id}
```

## Query Parameters

Access URL query string:

```python
# GET /search?q=hello&limit=10
@app.get("/search")
def search(request):
    query = request.query.get("q", "")
    limit = int(request.query.get("limit", "10"))
    return {"query": query, "limit": limit}

# Alternative access
@app.get("/items")
def list_items(request):
    page = request.get_query_param("page", "1")
    per_page = request.get_query_param("per_page", "20")
    return {"page": int(page), "per_page": int(per_page)}
```

## Blueprints

Group related routes with blueprints:

```python
from cello import App, Blueprint

# Create a blueprint with URL prefix
api = Blueprint("/api/v1")

@api.get("/users")
def list_users(request):
    return {"users": []}

@api.post("/users")
def create_user(request):
    return {"id": 1}

@api.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}

# Register with app
app = App()
app.register_blueprint(api)

# Routes are now:
# GET /api/v1/users
# POST /api/v1/users
# GET /api/v1/users/{id}
```

### Named Blueprints

Give blueprints names for organization:

```python
users_bp = Blueprint("/users", name="users")
posts_bp = Blueprint("/posts", name="posts")
auth_bp = Blueprint("/auth", name="auth")

@users_bp.get("")
def list_users(request):
    return {"users": []}

@posts_bp.get("")
def list_posts(request):
    return {"posts": []}

app.register_blueprint(users_bp)
app.register_blueprint(posts_bp)
app.register_blueprint(auth_bp)
```

### Nested Blueprints

Create hierarchical routes:

```python
# API version blueprint
api_v1 = Blueprint("/api/v1")

# Feature blueprints
users = Blueprint("/users")
posts = Blueprint("/posts")

@users.get("")
def list_users(request):
    return {"users": []}

@posts.get("")
def list_posts(request):
    return {"posts": []}

# Nest blueprints
api_v1.register(users)
api_v1.register(posts)

# Register top-level blueprint
app.register_blueprint(api_v1)

# Routes:
# GET /api/v1/users
# GET /api/v1/posts
```

### Getting All Routes

Inspect registered routes:

```python
bp = Blueprint("/api")

@bp.get("/users")
def users(request):
    pass

@bp.post("/users")
def create_user(request):
    pass

# Get all routes
routes = bp.get_all_routes()
for method, path, handler in routes:
    print(f"{method} {path}")

# Output:
# GET /api/users
# POST /api/users
```

## Route Constraints (Rust)

The Rust core supports advanced route constraints:

### Integer Constraints
```
/users/{id:int}
```
Only matches if `id` is a valid integer.

### UUID Constraints
```
/items/{uuid:uuid}
```
Only matches valid UUIDs.

### Regex Constraints
```
/files/{path:regex([a-z]+)}
```
Matches based on regex pattern.

> Note: Constraint syntax is configured in Rust. Check the routing module for implementation details.

## API Versioning

### URL-Based Versioning

```python
v1 = Blueprint("/api/v1")
v2 = Blueprint("/api/v2")

@v1.get("/users")
def v1_users(request):
    return {"version": 1, "users": []}

@v2.get("/users")
def v2_users(request):
    return {"version": 2, "users": [], "meta": {}}

app.register_blueprint(v1)
app.register_blueprint(v2)
```

### Header-Based Versioning

```python
@app.get("/api/users")
def users(request):
    version = request.get_header("Accept-Version") or "1"
    
    if version == "1":
        return {"users": []}
    elif version == "2":
        return {"users": [], "meta": {}}
    else:
        return Response.json(
            {"error": f"Unsupported version: {version}"},
            status=400
        )
```

## Response Types

Different responses for different routes:

```python
from cello import Response

# JSON (default)
@app.get("/api/data")
def json_data(request):
    return {"data": "value"}

# HTML
@app.get("/page")
def html_page(request):
    return Response.html("<h1>Welcome</h1>")

# Plain text
@app.get("/text")
def plain_text(request):
    return Response.text("Hello, World!")

# Redirect
@app.get("/old-path")
def redirect(request):
    return Response.redirect("/new-path")

# With status code
@app.post("/items")
def create_item(request):
    return Response.json({"id": 1}, status=201)

# No content
@app.delete("/items/{id}")
def delete_item(request):
    return Response.no_content()
```

## Error Handling

Handle errors in routes:

```python
@app.get("/users/{id}")
def get_user(request):
    user_id = request.params.get("id")
    
    if not user_id:
        return Response.json(
            {"error": "User ID required"},
            status=400
        )
    
    # Simulate user not found
    user = None  # database.get_user(user_id)
    
    if not user:
        return Response.json(
            {"error": f"User {user_id} not found"},
            status=404
        )
    
    return {"id": user_id, "name": "John"}
```

## WebSocket Routes

Register WebSocket handlers:

```python
@app.websocket("/ws")
def websocket_handler(ws):
    ws.send_text("Welcome!")
    
    while True:
        msg = ws.recv()
        if msg is None:
            break
        
        if msg.is_text():
            ws.send_text(f"Echo: {msg.text}")
        elif msg.is_binary():
            ws.send_binary(msg.data)
```

## Best Practices

### Route Organization

1. **Group by feature** - Use blueprints for related routes
2. **Version APIs** - Use `/api/v1/`, `/api/v2/` prefixes
3. **Be consistent** - Use same naming conventions
4. **Keep it RESTful** - Use HTTP methods correctly

### Naming Conventions

```python
# Good - RESTful naming
GET    /users           # List users
GET    /users/{id}      # Get user
POST   /users           # Create user
PUT    /users/{id}      # Update user
DELETE /users/{id}      # Delete user

# Good - nested resources
GET    /users/{id}/posts     # User's posts
POST   /users/{id}/posts     # Create post for user

# Avoid - verbs in URLs
GET    /getUser/{id}         # Bad
POST   /createUser           # Bad
```

### Handler Organization

```python
# Separate handlers by feature
# users.py
users_bp = Blueprint("/users")

@users_bp.get("")
def list_users(request):
    ...

# posts.py
posts_bp = Blueprint("/posts")

@posts_bp.get("")
def list_posts(request):
    ...

# app.py
from users import users_bp
from posts import posts_bp

app.register_blueprint(users_bp)
app.register_blueprint(posts_bp)
```
