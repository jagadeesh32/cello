"""
Advanced Cello example demonstrating v2 features.

Features demonstrated:
- Blueprint routing with nesting
- Middleware (CORS, logging, compression)
- Multiple response types
- Form data handling
- Path and query parameters

Run with:
    maturin develop
    python examples/advanced.py
"""

from cello import App, Blueprint, Response


# Create the application
app = App()

# Enable middleware
app.enable_cors(origins=["*"])
app.enable_logging()
app.enable_compression(min_size=1024)


# =============================================================================
# Root Routes
# =============================================================================


@app.get("/")
def home(request):
    """Welcome endpoint."""
    return {
        "message": "Welcome to Cello v2!",
        "version": "0.4.0",
        "features": [
            "SIMD JSON",
            "Middleware",
            "Blueprints",
            "WebSockets",
            "SSE",
            "File uploads",
        ],
    }


@app.get("/health")
def health(request):
    """Health check endpoint."""
    return {"status": "healthy"}


# =============================================================================
# API Blueprint (v1)
# =============================================================================


api_v1 = Blueprint("/api/v1", name="api_v1")


@api_v1.get("/users")
def list_users(request):
    """List all users."""
    limit = int(request.get_query_param("limit", "10"))
    offset = int(request.get_query_param("offset", "0"))
    return {
        "users": [
            {"id": i, "name": f"User {i}", "email": f"user{i}@example.com"}
            for i in range(offset, offset + limit)
        ],
        "total": 100,
        "limit": limit,
        "offset": offset,
    }


@api_v1.get("/users/{id}")
def get_user(request):
    """Get a specific user."""
    user_id = request.params.get("id")
    return {
        "id": user_id,
        "name": f"User {user_id}",
        "email": f"user{user_id}@example.com",
    }


@api_v1.post("/users")
def create_user(request):
    """Create a new user."""
    data = request.json()
    return {
        "id": 1,
        "name": data.get("name"),
        "email": data.get("email"),
        "created": True,
    }


@api_v1.put("/users/{id}")
def update_user(request):
    """Update a user."""
    user_id = request.params.get("id")
    data = request.json()
    return {
        "id": user_id,
        "name": data.get("name"),
        "email": data.get("email"),
        "updated": True,
    }


@api_v1.delete("/users/{id}")
def delete_user(request):
    """Delete a user."""
    user_id = request.params.get("id")
    return {"id": user_id, "deleted": True}


# =============================================================================
# Different Response Types
# =============================================================================


responses_bp = Blueprint("/responses", name="responses")


@responses_bp.get("/json")
def json_response(request):
    """JSON response (default)."""
    return {"type": "json", "data": {"key": "value"}}


@responses_bp.get("/text")
def text_response(request):
    """Plain text response."""
    return Response.text("This is plain text response.")


@responses_bp.get("/html")
def html_response(request):
    """HTML response."""
    html = """
    <!DOCTYPE html>
    <html>
    <head><title>Cello</title></head>
    <body>
        <h1>Hello from Cello!</h1>
        <p>This is an HTML response.</p>
    </body>
    </html>
    """
    return Response.html(html)


@responses_bp.get("/redirect")
def redirect_response(request):
    """Redirect response."""
    return Response.redirect("/")


@responses_bp.get("/no-content")
def no_content_response(request):
    """204 No Content response."""
    return Response.no_content()


# =============================================================================
# Form Handling
# =============================================================================


forms_bp = Blueprint("/forms", name="forms")


@forms_bp.post("/urlencoded")
def urlencoded_form(request):
    """Handle URL-encoded form data."""
    form = request.form()
    return {"received": form}


@forms_bp.post("/contact")
def contact_form(request):
    """Contact form handler."""
    if request.is_form():
        form = request.form()
        return {
            "success": True,
            "message": f"Thanks {form.get('name', 'Anonymous')}! We'll contact you at {form.get('email', 'N/A')}.",
        }
    elif request.is_json():
        data = request.json()
        return {
            "success": True,
            "message": f"Thanks {data.get('name', 'Anonymous')}!",
        }
    else:
        return {"error": "Unsupported content type"}


# =============================================================================
# Register Blueprints
# =============================================================================


app.register_blueprint(api_v1)
app.register_blueprint(responses_bp)
app.register_blueprint(forms_bp)


# =============================================================================
# Main
# =============================================================================

if __name__ == "__main__":
    print("🚀 Starting Cello v2 Advanced Example")
    print("   Try these endpoints:")
    print("   - GET  http://127.0.0.1:8000/")
    print("   - GET  http://127.0.0.1:8000/health")
    print("   - GET  http://127.0.0.1:8000/api/v1/users")
    print("   - GET  http://127.0.0.1:8000/api/v1/users/123")
    print("   - POST http://127.0.0.1:8000/api/v1/users")
    print("   - GET  http://127.0.0.1:8000/responses/html")
    print("   - POST http://127.0.0.1:8000/forms/contact")
    print()
    app.run(host="127.0.0.1", port=8000)
