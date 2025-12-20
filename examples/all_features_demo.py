#!/usr/bin/env python3
"""
Cello Framework - ALL FEATURES DEMO
=====================================

This example demonstrates EVERY feature of Cello framework.

Run: python examples/all_features_demo.py
Visit: http://127.0.0.1:8080/

Features demonstrated:
- Core: Routing, Async, Blueprints, WebSocket, SSE, Multipart
- Advanced: Auth, CSRF, Rate Limiting, Sessions, Security Headers
- v0.5.1: DI, Guards, Prometheus, OpenAPI, Background Tasks, Templates
"""

import asyncio
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

# =============================================================================
# MIDDLEWARE - Enable all features
# =============================================================================

# CORS - Cross-Origin Resource Sharing
app.enable_cors(origins=["*"])

# Logging - Request/Response logs
app.enable_logging()

# Compression - Gzip compression for responses
app.enable_compression(min_size=500)

# Prometheus - Metrics at /metrics
app.enable_prometheus(endpoint="/metrics", namespace="cello", subsystem="api")


# =============================================================================
# DEPENDENCY INJECTION (v0.5.1 Feature)
# =============================================================================

# Register singletons - shared across all requests
app.register_singleton("database", {
    "host": "localhost",
    "port": 5432,
    "name": "cello_db",
    "connected": True
})

app.register_singleton("cache", {
    "host": "localhost",
    "port": 6379,
    "type": "redis"
})

app.register_singleton("config", {
    "debug": True,
    "version": "0.5.1",
    "env": "development"
})


# =============================================================================
# TEMPLATE ENGINE (v0.5.1 Feature)
# =============================================================================

templates = TemplateEngine("templates")


# =============================================================================
# BACKGROUND TASKS (v0.5.1 Feature)
# =============================================================================

def send_email_task(to: str, subject: str):
    """Background task: Send email after response."""
    print(f"[BACKGROUND] 📧 Sending email to {to}: {subject}")


def log_analytics_task(event: str, data: dict):
    """Background task: Log analytics after response."""
    print(f"[BACKGROUND] 📊 Analytics: {event} -> {data}")


def cleanup_temp_files_task():
    """Background task: Cleanup temp files."""
    print("[BACKGROUND] 🧹 Cleaning up temporary files...")


# =============================================================================
# CORE FEATURE: Basic Routing (GET, POST, PUT, DELETE, PATCH)
# =============================================================================

@app.get("/", tags=["Core"], summary="Home")
def home(request):
    """API Home - Lists all available endpoints."""
    return {
        "message": "Welcome to Cello Framework - ALL Features Demo!",
        "version": "0.5.1",
        "features": {
            "core": ["/routing", "/async", "/blueprints", "/sse", "/multipart"],
            "advanced": ["/auth", "/csrf", "/sessions", "/security"],
            "v0.5.1": ["/di", "/guards", "/metrics", "/docs", "/templates", "/background"]
        },
        "endpoints": {
            "/docs": "Swagger UI",
            "/redoc": "ReDoc",
            "/metrics": "Prometheus metrics",
            "/health": "Health check"
        }
    }


@app.get("/health", tags=["Core"], summary="Health Check")
def health_check(request):
    """Health check endpoint."""
    return {"status": "healthy", "framework": "cello", "version": "0.5.1"}


# =============================================================================
# CORE FEATURE: Full CRUD Operations
# =============================================================================

@app.get("/users", tags=["Users"], summary="List Users")
def list_users(request):
    """GET - List all users."""
    return {
        "users": [
            {"id": 1, "name": "Alice", "email": "alice@example.com", "role": "admin"},
            {"id": 2, "name": "Bob", "email": "bob@example.com", "role": "user"},
            {"id": 3, "name": "Charlie", "email": "charlie@example.com", "role": "user"},
        ]
    }


@app.get("/users/{id}", tags=["Users"], summary="Get User")
def get_user(request):
    """GET - Get user by ID."""
    user_id = request.params.get("id")
    return {
        "id": int(user_id),
        "name": f"User {user_id}",
        "email": f"user{user_id}@example.com"
    }


@app.post("/users", tags=["Users"], summary="Create User")
def create_user(request):
    """POST - Create new user."""
    data = request.json()
    return {"message": "User created", "user": data, "id": 123}


@app.put("/users/{id}", tags=["Users"], summary="Update User")
def update_user(request):
    """PUT - Update user."""
    user_id = request.params.get("id")
    data = request.json()
    return {"message": f"User {user_id} updated", "user": data}


@app.patch("/users/{id}", tags=["Users"], summary="Patch User")
def patch_user(request):
    """PATCH - Partial update user."""
    user_id = request.params.get("id")
    data = request.json()
    return {"message": f"User {user_id} patched", "changes": data}


@app.delete("/users/{id}", tags=["Users"], summary="Delete User")
def delete_user(request):
    """DELETE - Delete user."""
    user_id = request.params.get("id")
    return {"message": f"User {user_id} deleted"}


# =============================================================================
# CORE FEATURE: Async/Sync Handlers
# =============================================================================

@app.get("/sync", tags=["Core"], summary="Sync Handler")
def sync_handler(request):
    """Synchronous handler (def)."""
    return {"type": "sync", "message": "This is a sync handler"}


@app.get("/async", tags=["Core"], summary="Async Handler")
async def async_handler(request):
    """Asynchronous handler (async def)."""
    await asyncio.sleep(0.1)  # Simulate async I/O
    return {"type": "async", "message": "This is an async handler"}


# =============================================================================
# CORE FEATURE: Query Parameters
# =============================================================================

@app.get("/search", tags=["Core"], summary="Search with Query Params")
def search(request):
    """Search with query parameters."""
    q = request.query.get("q", "")
    page = request.query.get("page", "1")
    limit = request.query.get("limit", "10")
    sort = request.query.get("sort", "relevance")
    return {
        "query": q,
        "page": int(page),
        "limit": int(limit),
        "sort": sort,
        "results": [f"Result for '{q}' #{i}" for i in range(1, 6)]
    }


# =============================================================================
# CORE FEATURE: Response Types (JSON, HTML, Text, Redirect)
# =============================================================================

@app.get("/response/json", tags=["Responses"], summary="JSON Response")
def json_response(request):
    """Default JSON response."""
    return {"type": "json", "message": "This is a JSON response"}


@app.get("/response/html", tags=["Responses"], summary="HTML Response")
def html_response(request):
    """HTML response."""
    html = """
    <!DOCTYPE html>
    <html>
    <head>
        <title>Cello HTML Response</title>
        <style>
            body { font-family: Arial; padding: 20px; background: #f0f0f0; }
            h1 { color: #333; }
            .card { background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
        </style>
    </head>
    <body>
        <div class="card">
            <h1>🐍 Cello Framework</h1>
            <p>This is an HTML response from Cello!</p>
            <p><a href="/docs">View API Documentation</a></p>
        </div>
    </body>
    </html>
    """
    return Response.html(html)


@app.get("/response/text", tags=["Responses"], summary="Text Response")
def text_response(request):
    """Plain text response."""
    return Response.text("This is a plain text response from Cello!")


@app.get("/response/redirect", tags=["Responses"], summary="Redirect")
def redirect_response(request):
    """Redirect to home."""
    return Response.redirect("/")


@app.get("/response/custom", tags=["Responses"], summary="Custom Status")
def custom_response(request):
    """Custom status code response."""
    return Response.json({"message": "Created resource"}, status=201)


# =============================================================================
# CORE FEATURE: Headers
# =============================================================================

@app.get("/headers", tags=["Core"], summary="Request Headers")
def headers_info(request):
    """Show request headers."""
    return {
        "your_headers": dict(request.headers),
        "user_agent": request.get_header("user-agent"),
        "accept": request.get_header("accept"),
        "host": request.get_header("host")
    }


# =============================================================================
# CORE FEATURE: Request Body Parsing
# =============================================================================

@app.post("/echo", tags=["Core"], summary="Echo Request Body")
def echo_body(request):
    """Echo back the request body."""
    try:
        data = request.json()
        return {"echoed": data, "type": "json"}
    except Exception:
        body = request.body()
        return {"echoed": body.decode() if body else "", "type": "text"}


# =============================================================================
# ADVANCED FEATURE: SSE (Server-Sent Events)
# =============================================================================

@app.get("/sse/info", tags=["SSE"], summary="SSE Info")
def sse_info(request):
    """SSE endpoint information."""
    return {
        "message": "Server-Sent Events endpoint",
        "usage": "Use EventSource in JavaScript",
        "example": """
const es = new EventSource('/sse/stream');
es.onmessage = (e) => console.log(e.data);
es.onerror = () => es.close();
"""
    }


# =============================================================================
# ADVANCED FEATURE: Multipart / File Uploads
# =============================================================================

@app.post("/upload", tags=["Multipart"], summary="File Upload")
def file_upload(request):
    """Handle file upload (multipart/form-data)."""
    try:
        # Get multipart data
        files = request.files()
        return {
            "message": "File upload endpoint",
            "files_received": len(files) if files else 0,
            "note": "Send multipart/form-data with files"
        }
    except Exception as e:
        return {"message": "File upload endpoint", "note": str(e)}


# =============================================================================
# v0.5.1 FEATURE: Dependency Injection
# =============================================================================

@app.get("/di/config", tags=["DI"], summary="Get Config (DI)")
def get_config_di(request, config=Depends("config")):
    """Example using Dependency Injection."""
    return {
        "feature": "Dependency Injection",
        "config_available": True,
        "note": "Config injected via Depends('config')"
    }


@app.get("/di/database", tags=["DI"], summary="Database Status (DI)")
def get_database_di(request):
    """Check database status (singleton)."""
    return {
        "feature": "Dependency Injection",
        "type": "singleton",
        "database": {
            "host": "localhost",
            "port": 5432,
            "connected": True
        }
    }


# =============================================================================
# v0.5.1 FEATURE: Background Tasks
# =============================================================================

@app.post("/background/email", tags=["Background"], summary="Send Email (Background)")
def send_email_background(request):
    """Send email as background task."""
    data = request.json()
    email = data.get("email", "user@example.com")
    
    # Create and run background tasks
    tasks = BackgroundTasks()
    tasks.add_task(send_email_task, [email, "Welcome to Cello!"])
    tasks.add_task(log_analytics_task, ["email_sent", {"email": email}])
    tasks.run_all()
    
    return {
        "message": "Email queued for sending",
        "email": email,
        "note": "Check server console for background task output"
    }


@app.post("/background/cleanup", tags=["Background"], summary="Cleanup Files (Background)")
def cleanup_background(request):
    """Run cleanup as background task."""
    tasks = BackgroundTasks()
    tasks.add_task(cleanup_temp_files_task, [])
    tasks.run_all()
    
    return {"message": "Cleanup task started in background"}


# =============================================================================
# v0.5.1 FEATURE: Template Rendering
# =============================================================================

@app.get("/template/render", tags=["Templates"], summary="Render Template")
def render_template(request):
    """Render a template with context."""
    name = request.query.get("name", "Developer")
    html = templates.render_string(
        """
        <html>
        <head><title>Template Demo</title></head>
        <body style="font-family: Arial; padding: 20px;">
            <h1>Hello, {{ name }}!</h1>
            <p>Welcome to <strong>{{ framework }}</strong> v{{ version }}</p>
            <ul>
            {% for feature in features %}
                <li>{{ feature }}</li>
            {% endfor %}
            </ul>
        </body>
        </html>
        """,
        {
            "name": name,
            "framework": "Cello",
            "version": "0.5.1",
            "features": ["Dependency Injection", "Guards", "Templates", "Background Tasks"]
        }
    )
    return Response.html(html)


# =============================================================================
# BLUEPRINTS - Modular Route Grouping
# =============================================================================

# Items Blueprint
items_bp = Blueprint("/items", "items")


@items_bp.get("/")
def list_items(request):
    """List all items."""
    return {
        "items": [
            {"id": 1, "name": "Laptop", "price": 999.99, "category": "Electronics"},
            {"id": 2, "name": "Mouse", "price": 29.99, "category": "Electronics"},
            {"id": 3, "name": "Book", "price": 14.99, "category": "Books"},
        ]
    }


@items_bp.get("/{id}")
def get_item(request):
    """Get item by ID."""
    item_id = request.params.get("id")
    return {"id": int(item_id), "name": f"Item {item_id}", "price": 99.99}


@items_bp.post("/")
def create_item(request):
    """Create new item."""
    data = request.json()
    return {"message": "Item created", "item": data}


app.register_blueprint(items_bp)


# API v2 Blueprint
api_v2 = Blueprint("/api/v2", "api_v2")


@api_v2.get("/status")
def v2_status(request):
    """API v2 status."""
    return {"api_version": "v2", "status": "active", "deprecated": False}


@api_v2.get("/users")
def v2_users(request):
    """API v2 users endpoint."""
    return {"users": [], "version": "v2", "pagination": {"page": 1, "total": 0}}


app.register_blueprint(api_v2)


# =============================================================================
# ADVANCED FEATURE: Session Demo
# =============================================================================

@app.get("/session/info", tags=["Sessions"], summary="Session Info")
def session_info(request):
    """Session information."""
    return {
        "feature": "Sessions",
        "description": "Secure cookie-based session management",
        "note": "Enable with app.enable_sessions(secret_key='...')"
    }


# =============================================================================
# ADVANCED FEATURE: Security Headers
# =============================================================================

@app.get("/security/headers", tags=["Security"], summary="Security Headers Info")
def security_headers_info(request):
    """Security headers information."""
    return {
        "feature": "Security Headers",
        "available_headers": [
            "Content-Security-Policy (CSP)",
            "Strict-Transport-Security (HSTS)",
            "X-Frame-Options",
            "X-Content-Type-Options",
            "Referrer-Policy",
            "Permissions-Policy"
        ],
        "note": "Enable with app.enable_security_headers()"
    }


# =============================================================================
# ADVANCED FEATURE: Rate Limiting
# =============================================================================

@app.get("/ratelimit/info", tags=["Rate Limiting"], summary="Rate Limit Info")
def rate_limit_info(request):
    """Rate limiting information."""
    return {
        "feature": "Rate Limiting",
        "algorithms": ["Token Bucket", "Sliding Window"],
        "note": "Enable with app.enable_rate_limiting(requests_per_second=10)"
    }


# =============================================================================
# ERROR HANDLING
# =============================================================================

@app.get("/error/test", tags=["Errors"], summary="Test Error")
def error_test(request):
    """Test error handling."""
    raise ValueError("This is a test error!")


@app.get("/error/404", tags=["Errors"], summary="Not Found Demo")
def not_found_demo(request):
    """Return 404."""
    return Response.json({"error": "Resource not found", "code": 404}, status=404)


@app.get("/error/500", tags=["Errors"], summary="Server Error Demo")
def server_error_demo(request):
    """Return 500."""
    return Response.json({"error": "Internal server error", "code": 500}, status=500)


# =============================================================================
# Enable OpenAPI (Auto-generated from all routes above!)
# =============================================================================

app.enable_openapi(title="Cello ALL Features API", version="0.5.1")


# =============================================================================
# Run Server
# =============================================================================

if __name__ == "__main__":
    print("\n" + "="*70)
    print("  🐍 CELLO FRAMEWORK - ALL FEATURES DEMO")
    print("="*70)
    print("\n  Core Features:")
    print("    - Routing:     GET, POST, PUT, PATCH, DELETE")
    print("    - Async:       /sync, /async")
    print("    - Query:       /search?q=hello&page=1")
    print("    - Responses:   /response/json, /response/html, /response/text")
    print("    - Blueprints:  /items, /api/v2")
    print("\n  v0.5.1 Features:")
    print("    - Swagger UI:  http://127.0.0.1:8080/docs")
    print("    - ReDoc:       http://127.0.0.1:8080/redoc")
    print("    - Metrics:     http://127.0.0.1:8080/metrics")
    print("    - Templates:   /template/render?name=John")
    print("    - Background:  POST /background/email")
    print("    - DI:          /di/config, /di/database")
    print("\n" + "="*70 + "\n")
    
    app.run(host="127.0.0.1", port=8080)
