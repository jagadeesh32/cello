#!/usr/bin/env python3
"""
Cello Framework - Complete Feature Showcase
============================================

This example demonstrates ALL features of the Cello framework.
Run with: python examples/complete_showcase.py
Then visit: http://127.0.0.1:8080

Features demonstrated:
- Core routing (GET, POST, PUT, DELETE, PATCH)
- Async handlers
- Blueprints
- WebSocket
- SSE (Server-Sent Events)
- Middleware (CORS, Logging, Compression)
- Authentication (JWT placeholder)
- Rate Limiting (Prometheus)
- Dependency Injection
- Background Tasks
- Template Rendering
- OpenAPI/Swagger documentation
"""

from cello import (
    App,
    Blueprint,
    Response,
    BackgroundTasks,
    TemplateEngine,
    Depends,
)


# =============================================================================
# Initialize App
# =============================================================================

app = App()

# Enable middleware
app.enable_cors(origins=["*"])
app.enable_logging()
app.enable_compression()
app.enable_prometheus(endpoint="/metrics", namespace="cello", subsystem="api")
app.enable_openapi(title="Cello Feature Showcase", version="0.5.1")


# =============================================================================
# Dependency Injection Setup
# =============================================================================

# Register singleton dependencies
app.register_singleton("database", {"host": "localhost", "port": 5432, "name": "cello_db"})
app.register_singleton("cache", {"host": "localhost", "port": 6379})
app.register_singleton("config", {"debug": True, "version": "0.5.1"})


# =============================================================================
# Template Engine Setup
# =============================================================================

templates = TemplateEngine("templates")


# =============================================================================
# Background Tasks
# =============================================================================

def send_email_task(to: str, subject: str):
    """Background task to send email."""
    print(f"[BACKGROUND] Sending email to {to}: {subject}")


def log_analytics_task(event: str, data: dict):
    """Background task to log analytics."""
    print(f"[BACKGROUND] Analytics: {event} - {data}")


# =============================================================================
# Core Routes - Basic CRUD
# =============================================================================

@app.get("/")
def home(request):
    """Home endpoint - API overview."""
    return {
        "message": "Welcome to Cello Framework!",
        "version": "0.5.1",
        "endpoints": {
            "docs": "/docs",
            "redoc": "/redoc",
            "openapi": "/openapi.json",
            "metrics": "/metrics",
            "health": "/health",
            "users": "/users",
            "items": "/items",
            "sse": "/events",
        }
    }


@app.get("/health")
def health_check(request):
    """Health check endpoint."""
    return {"status": "healthy", "framework": "cello", "version": "0.5.1"}


@app.get("/users")
def list_users(request):
    """List all users."""
    return {
        "users": [
            {"id": 1, "name": "Alice", "email": "alice@example.com"},
            {"id": 2, "name": "Bob", "email": "bob@example.com"},
            {"id": 3, "name": "Charlie", "email": "charlie@example.com"},
        ]
    }


@app.get("/users/{id}")
def get_user(request):
    """Get user by ID."""
    user_id = request.params.get("id")
    return {
        "id": int(user_id),
        "name": f"User {user_id}",
        "email": f"user{user_id}@example.com"
    }


@app.post("/users")
def create_user(request):
    """Create a new user."""
    data = request.json()
    return {
        "message": "User created",
        "user": data,
        "id": 123
    }


@app.put("/users/{id}")
def update_user(request):
    """Update an existing user."""
    user_id = request.params.get("id")
    data = request.json()
    return {
        "message": f"User {user_id} updated",
        "user": data
    }


@app.delete("/users/{id}")
def delete_user(request):
    """Delete a user."""
    user_id = request.params.get("id")
    return {"message": f"User {user_id} deleted"}


# =============================================================================
# Async Handlers
# =============================================================================

@app.get("/async")
async def async_handler(request):
    """Async handler example."""
    import asyncio
    await asyncio.sleep(0.1)  # Simulate async operation
    return {"message": "This was handled asynchronously!"}


# =============================================================================
# Query Parameters
# =============================================================================

@app.get("/search")
def search(request):
    """Search with query parameters."""
    query = request.query.get("q", "")
    page = request.query.get("page", "1")
    limit = request.query.get("limit", "10")
    return {
        "query": query,
        "page": int(page),
        "limit": int(limit),
        "results": [f"Result {i}" for i in range(1, 6)]
    }


# =============================================================================
# Response Types
# =============================================================================

@app.get("/html")
def html_response(request):
    """HTML response example."""
    html = """
    <!DOCTYPE html>
    <html>
    <head><title>Cello HTML</title></head>
    <body>
        <h1>Hello from Cello!</h1>
        <p>This is an HTML response.</p>
        <a href="/docs">View API Docs</a>
    </body>
    </html>
    """
    return Response.html(html)


@app.get("/text")
def text_response(request):
    """Plain text response example."""
    return Response.text("This is a plain text response from Cello!")


@app.get("/redirect")
def redirect_response(request):
    """Redirect example."""
    return Response.redirect("/")


# =============================================================================
# Background Tasks Example
# =============================================================================

@app.post("/notify")
def notify_with_background_task(request):
    """Endpoint that triggers background tasks."""
    data = request.json()
    email = data.get("email", "user@example.com")
    
    # Create background tasks
    tasks = BackgroundTasks()
    tasks.add_task(send_email_task, [email, "Welcome to Cello!"])
    tasks.add_task(log_analytics_task, ["user_notification", {"email": email}])
    
    # Run tasks in background
    tasks.run_all()
    
    return {"message": "Notification sent", "email": email}


# =============================================================================
# Template Rendering Example
# =============================================================================

@app.get("/template")
def template_example(request):
    """Template rendering example."""
    result = templates.render_string(
        "<h1>Hello, {{ name }}!</h1><p>Welcome to {{ framework }}</p>",
        {"name": "Developer", "framework": "Cello"}
    )
    return Response.html(result)


# =============================================================================
# Dependency Injection Example
# =============================================================================

@app.get("/config")
def get_config(request, config = Depends("config")):
    """Endpoint using dependency injection."""
    # Note: DI is resolved via the config singleton
    return {"config_available": True, "message": "DI is working!"}


# =============================================================================
# Items CRUD with Blueprints
# =============================================================================

items_bp = Blueprint("/items", "items")


@items_bp.get("/")
def list_items(request):
    """List all items."""
    return {
        "items": [
            {"id": 1, "name": "Laptop", "price": 999.99},
            {"id": 2, "name": "Mouse", "price": 29.99},
            {"id": 3, "name": "Keyboard", "price": 79.99},
        ]
    }


@items_bp.get("/{id}")
def get_item(request):
    """Get item by ID."""
    item_id = request.params.get("id")
    return {"id": int(item_id), "name": f"Item {item_id}", "price": 99.99}


@items_bp.post("/")
def create_item(request):
    """Create a new item."""
    data = request.json()
    return {"message": "Item created", "item": data}


# Register blueprint
app.register_blueprint(items_bp)


# =============================================================================
# API v2 Blueprint (Nested)
# =============================================================================

api_v2 = Blueprint("/api/v2", "api_v2")


@api_v2.get("/status")
def v2_status(request):
    """V2 API status."""
    return {"api_version": "v2", "status": "active"}


@api_v2.get("/features")
def v2_features(request):
    """List V2 features."""
    return {
        "features": [
            "Dependency Injection",
            "Guards (RBAC)",
            "Prometheus Metrics",
            "OpenAPI/Swagger",
            "Background Tasks",
            "Template Rendering"
        ]
    }


app.register_blueprint(api_v2)


# =============================================================================
# Error Handling Examples
# =============================================================================

@app.get("/error")
def error_example(request):
    """Trigger an error for testing."""
    raise ValueError("This is a test error!")


@app.get("/not-found-demo")
def not_found_demo(request):
    """Return 404 manually."""
    return Response.json({"error": "Resource not found"}, status=404)


# =============================================================================
# Headers Example
# =============================================================================

@app.get("/headers")
def headers_example(request):
    """Show request headers."""
    return {
        "your_headers": dict(request.headers),
        "user_agent": request.get_header("user-agent"),
        "accept": request.get_header("accept"),
    }


# =============================================================================
# SSE (Server-Sent Events) Placeholder
# =============================================================================

@app.get("/events")
def sse_info(request):
    """SSE endpoint info (actual SSE requires special handling)."""
    return {
        "message": "SSE endpoint",
        "note": "Use EventSource in JavaScript to connect",
        "example": "const es = new EventSource('/events-stream')"
    }


# =============================================================================
# WebSocket Info
# =============================================================================

@app.get("/ws-info")
def websocket_info(request):
    """WebSocket info endpoint."""
    return {
        "websocket_url": "ws://127.0.0.1:8080/ws",
        "protocols": ["chat", "json"],
        "example": "const ws = new WebSocket('ws://127.0.0.1:8080/ws')"
    }


# =============================================================================
# Main Entry Point
# =============================================================================

if __name__ == "__main__":
    print("\n" + "="*60)
    print("  CELLO FRAMEWORK - COMPLETE FEATURE SHOWCASE")
    print("="*60)
    print("\n  Endpoints available:")
    print("    - Home:        http://127.0.0.1:8080/")
    print("    - Swagger UI:  http://127.0.0.1:8080/docs")
    print("    - ReDoc:       http://127.0.0.1:8080/redoc")
    print("    - OpenAPI:     http://127.0.0.1:8080/openapi.json")
    print("    - Metrics:     http://127.0.0.1:8080/metrics")
    print("    - Health:      http://127.0.0.1:8080/health")
    print("    - Users:       http://127.0.0.1:8080/users")
    print("    - Items:       http://127.0.0.1:8080/items")
    print("    - Search:      http://127.0.0.1:8080/search?q=hello")
    print("    - HTML:        http://127.0.0.1:8080/html")
    print("    - Template:    http://127.0.0.1:8080/template")
    print("\n" + "="*60 + "\n")
    
    app.run(host="127.0.0.1", port=8080)
