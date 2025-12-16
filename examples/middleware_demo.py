#!/usr/bin/env python3
"""
Middleware System Demo for Cello v0.4.0.

This example demonstrates the middleware capabilities including:
- Built-in middleware (CORS, logging, compression)
- Middleware configuration options
- Request/response modification patterns

Run with:
    python examples/middleware_demo.py

Then test with:
    curl -v http://127.0.0.1:8000/
    curl -v http://127.0.0.1:8000/api/data
    curl -H "Origin: https://example.com" http://127.0.0.1:8000/
"""

from cello import App, Blueprint, Response

app = App()


# =============================================================================
# Middleware Configuration
# =============================================================================

# Enable CORS with specific origins
# In production, always specify allowed origins
app.enable_cors(origins=[
    "https://example.com",
    "https://app.example.com",
    "http://localhost:3000",  # For local development
])

# Enable request/response logging
# This logs method, path, status code, and timing
app.enable_logging()

# Enable gzip compression for large responses
# Only compresses responses larger than min_size bytes
app.enable_compression(min_size=1024)


# =============================================================================
# Middleware Demonstration Routes
# =============================================================================


@app.get("/")
def home(request):
    """Home endpoint showing middleware information."""
    return {
        "message": "Cello Middleware Demo",
        "version": "0.4.0",
        "enabled_middleware": [
            "CORS - Cross-Origin Resource Sharing",
            "Logging - Request/response logging",
            "Compression - Gzip compression for large responses",
        ],
        "available_middleware": [
            "Authentication (JWT, Basic, API Key)",
            "Rate Limiting (Token Bucket, Sliding Window)",
            "Session Management",
            "Security Headers (CSP, HSTS, etc.)",
            "CSRF Protection",
            "ETag Caching",
            "Request ID Tracking",
            "Body Size Limits",
            "Static File Serving",
        ],
    }


@app.get("/cors-demo")
def cors_demo(request):
    """Endpoint to test CORS headers."""
    origin = request.get_header("Origin")
    return {
        "message": "CORS is enabled on this endpoint",
        "your_origin": origin,
        "allowed_origins": [
            "https://example.com",
            "https://app.example.com",
            "http://localhost:3000",
        ],
        "cors_headers": {
            "Access-Control-Allow-Origin": "Matches your origin if allowed",
            "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
            "Access-Control-Allow-Headers": "Content-Type, Authorization",
            "Access-Control-Max-Age": "86400 (24 hours)",
        },
    }


@app.options("/cors-demo")
def cors_preflight(request):
    """Handle CORS preflight requests."""
    # CORS middleware handles this automatically
    # This shows what the preflight response looks like
    return Response.no_content()


@app.get("/compression-demo")
def compression_demo(request):
    """Endpoint to test compression."""
    # Generate a large response to trigger compression
    large_data = {
        "message": "This response will be compressed if it exceeds min_size",
        "items": [
            {"id": i, "name": f"Item {i}", "description": f"Description for item {i}" * 10}
            for i in range(100)
        ],
    }
    return large_data


@app.get("/small-response")
def small_response(request):
    """Small response that won't be compressed."""
    return {"message": "This response is too small to compress"}


@app.get("/logging-demo")
def logging_demo(request):
    """Endpoint to demonstrate request logging."""
    return {
        "message": "Check your console for request logs",
        "logged_info": {
            "method": request.method,
            "path": request.path,
            "timestamp": "Logged automatically",
            "duration": "Logged after response",
            "status": "Logged after response",
        },
    }


# =============================================================================
# Request Information Routes
# =============================================================================


@app.get("/request-info")
def request_info(request):
    """Display request information."""
    return {
        "method": request.method,
        "path": request.path,
        "query": dict(request.query) if hasattr(request.query, '__iter__') else {},
        "headers": {
            "content-type": request.get_header("Content-Type"),
            "accept": request.get_header("Accept"),
            "user-agent": request.get_header("User-Agent"),
            "origin": request.get_header("Origin"),
        },
    }


@app.post("/echo")
def echo_request(request):
    """Echo back the request body."""
    content_type = request.get_header("Content-Type") or ""
    
    if "application/json" in content_type:
        try:
            body = request.json()
            return {"received_json": body}
        except Exception as e:
            return Response.json({"error": f"Invalid JSON: {e}"}, status=400)
    elif "application/x-www-form-urlencoded" in content_type:
        try:
            form = request.form()
            return {"received_form": form}
        except Exception as e:
            return Response.json({"error": f"Invalid form: {e}"}, status=400)
    else:
        text = request.text()
        return {"received_text": text}


# =============================================================================
# API Blueprint with Middleware
# =============================================================================

api = Blueprint("/api", name="api")


@api.get("/data")
def api_data(request):
    """API endpoint with all middleware applied."""
    return {
        "data": [
            {"id": 1, "value": "Item 1"},
            {"id": 2, "value": "Item 2"},
            {"id": 3, "value": "Item 3"},
        ],
        "middleware_active": True,
    }


@api.get("/users")
def api_users(request):
    """Get users from API."""
    return {
        "users": [
            {"id": 1, "name": "Alice", "email": "alice@example.com"},
            {"id": 2, "name": "Bob", "email": "bob@example.com"},
        ],
    }


@api.post("/users")
def create_user(request):
    """Create a new user."""
    try:
        data = request.json()
        return {
            "id": 3,
            "name": data.get("name"),
            "email": data.get("email"),
            "created": True,
        }
    except Exception as e:
        return Response.json({"error": str(e)}, status=400)


app.register_blueprint(api)


# =============================================================================
# Response Modification Examples
# =============================================================================


@app.get("/custom-headers")
def custom_headers(request):
    """Demonstrate adding custom headers to response."""
    response = Response.json({
        "message": "Response with custom headers",
        "check_headers": "X-Custom-Header, X-Request-ID, X-Response-Time",
    })
    
    # Add custom headers
    response.set_header("X-Custom-Header", "custom-value")
    response.set_header("X-Request-ID", "req-12345")
    response.set_header("X-Response-Time", "10ms")
    response.set_header("X-Powered-By", "Cello/0.4.0")
    
    return response


@app.get("/cache-headers")
def cache_headers(request):
    """Demonstrate cache control headers."""
    response = Response.json({
        "message": "Response with cache headers",
        "data": "This response can be cached",
    })
    
    # Add caching headers
    response.set_header("Cache-Control", "public, max-age=3600")
    response.set_header("ETag", '"abc123"')
    response.set_header("Last-Modified", "Mon, 16 Dec 2024 00:00:00 GMT")
    
    return response


@app.get("/no-cache")
def no_cache_headers(request):
    """Demonstrate no-cache headers."""
    response = Response.json({
        "message": "Response that should not be cached",
        "timestamp": "Dynamic content",
    })
    
    # Prevent caching
    response.set_header("Cache-Control", "no-store, no-cache, must-revalidate")
    response.set_header("Pragma", "no-cache")
    response.set_header("Expires", "0")
    
    return response


if __name__ == "__main__":
    print("🔧 Cello Middleware Demo")
    print()
    print("   Enabled middleware:")
    print("   - CORS (restricted origins)")
    print("   - Request logging")
    print("   - Gzip compression (min 1024 bytes)")
    print()
    print("   Try these endpoints:")
    print("   - GET  http://127.0.0.1:8000/")
    print("   - GET  http://127.0.0.1:8000/cors-demo")
    print("   - GET  http://127.0.0.1:8000/compression-demo")
    print("   - GET  http://127.0.0.1:8000/request-info")
    print("   - POST http://127.0.0.1:8000/echo")
    print("   - GET  http://127.0.0.1:8000/api/data")
    print("   - GET  http://127.0.0.1:8000/custom-headers")
    print()
    app.run(host="127.0.0.1", port=8000)
