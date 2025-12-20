"""
Advanced Middleware Example for Cello Framework

This example demonstrates the new middleware features inspired by
Robyn, Litestar, and FastAPI:

1. Dependency Injection
2. Guards (RBAC)
3. Prometheus Metrics

Run with:
    python examples/advanced_middleware.py

Then visit:
    - http://localhost:8000/ - Public endpoint
    - http://localhost:8000/user - Requires authentication
    - http://localhost:8000/admin - Requires admin role
    - http://localhost:8000/metrics - Prometheus metrics
"""

from cello import App, Response, Depends

app = App()

# Enable Prometheus metrics
app.enable_prometheus()

# Register dependencies
app.register_singleton("database", {"url": "postgres://localhost:5432/cello"})

# RBAC Guard implementation
def rbac_guard(request):
    """
    Simulated RBAC guard.
    In production, this would check JWT roles or session data.
    """
    # Simulate a user with 'user' role but not 'admin'
    request.set_context("user", {"roles": ["user"]})
    
    path = request.path
    if path.startswith("/admin"):
        user = request.get_context("user") or {}
        roles = user.get("roles", [])
        if "admin" not in roles:
            # Returning a string indicates failure with that message
            return "Admin access required"
    
    return True # Returning True or None indicates success

app.add_guard(rbac_guard)

# ============================================================================
# Example 1: Basic Routes
# ============================================================================


@app.get("/")
def home(request):
    return {"message": "Welcome to Cello with Advanced Middleware!"}


@app.get("/health")
def health(request):
    return {"status": "healthy"}


# ============================================================================
# Example 2: Simulated Authentication (for testing guards)
# ============================================================================

# In a real app, you would use JWT or session middleware
# For this example, we'll simulate a user in the request context


@app.get("/user")
def user_endpoint(request):
    """This endpoint would require authentication via guards"""
    # In production, guards would check request.context["user"]
    return {
        "message": "User endpoint",
        "note": "In production, this would require authenticated user",
    }


@app.get("/admin")
def admin_endpoint(request):
    """This endpoint would require admin role"""
    # In production, guards would check request.context["user"]["roles"]
    return {
        "message": "Admin endpoint",
        "note": "In production, this would require admin role",
    }


@app.get("/moderator")
def moderator_endpoint(request):
    """This endpoint would require moderator permission"""
    return {
        "message": "Moderator endpoint",
        "note": "In production, this would require moderator permission",
    }


# ============================================================================
# Example 3: Multiple Guards (OR logic)
# ============================================================================


@app.get("/admin-or-moderator")
def admin_or_moderator(request):
    """This endpoint requires either admin OR moderator role"""
    return {
        "message": "Admin or Moderator endpoint",
        "note": "Requires admin OR moderator role",
    }


# ============================================================================
# Example 4: Complex Guards (AND logic)
# ============================================================================


@app.get("/admin-with-permissions")
def admin_with_perms(request):
    """This endpoint requires admin role AND specific permissions"""
    return {
        "message": "Admin with permissions endpoint",
        "note": "Requires admin role AND users:write permission",
    }


# ============================================================================
# Example 5: Custom Guard Logic
# ============================================================================


@app.get("/ip-restricted")
def ip_restricted(request):
    """This endpoint is restricted by IP address"""
    # In production, a custom guard would check the IP
    client_ip = request.headers.get("x-real-ip", "unknown")
    return {
        "message": "IP restricted endpoint",
        "client_ip": client_ip,
        "note": "In production, only whitelisted IPs can access this",
    }


# ============================================================================
# Example 6: Dependency Injection (conceptual)
# ============================================================================


@app.get("/users/{user_id}")
def get_user(request, db=Depends("database")):
    """
    This endpoint uses dependency injection:
    - Database connection injected as dependency via Depends("database")
    """
    user_id = request.params.get("user_id")

    return {
        "user_id": user_id,
        "message": f"User {user_id}",
        "database_info": db,
        "note": "This value was injected via Dependency Injection!",
    }


@app.post("/users")
def create_user(request):
    """Create a user with DI for database and validation"""
    data = request.json()

    # In production with DI:
    # db = depends(get_database)
    # validator = depends(UserValidator)
    # user = validator.validate(data)
    # db.users.create(user)

    return {
        "message": "User created (simulated)",
        "data": data,
        "note": "In production, this would use DI for validation and DB",
    }


# ============================================================================
# Example 7: Metrics Endpoints
# ============================================================================


@app.get("/api/stats")
def api_stats(request):
    """
    Custom application stats endpoint
    Prometheus metrics are available at /metrics
    """
    return {
        "message": "Application statistics",
        "note": "Visit /metrics for Prometheus metrics",
        "endpoints": {
            "public": ["/", "/health"],
            "protected": ["/user", "/admin", "/moderator"],
            "metrics": "/metrics",
        },
    }


# ============================================================================
# Example 8: Error Handling
# ============================================================================


@app.get("/error-test")
def error_test(request):
    """Test endpoint that raises an error"""
    # In production, global exception handlers would catch this
    raise ValueError("This is a test error")


@app.get("/not-found-test")
def not_found_test(request):
    """This will trigger a 404 if the path doesn't match"""
    return Response.json({"error": "This should not be reached"}, status=404)


# ============================================================================
# Example 9: Documentation Endpoint
# ============================================================================


@app.get("/docs")
def documentation(request):
    """API documentation endpoint"""
    return {
        "title": "Cello Advanced Middleware API",
        "version": "0.4.0",
        "features": {
            "dependency_injection": {
                "description": "FastAPI-style dependency injection",
                "scopes": ["Singleton", "Request", "Transient"],
                "features": ["Caching", "Override for testing", "Hierarchical resolution"],
            },
            "guards": {
                "description": "Litestar-style permission guards",
                "types": [
                    "RoleGuard - Role-based access control",
                    "PermissionGuard - Permission-based access",
                    "AuthenticatedGuard - Authentication check",
                    "CustomGuard - Custom logic",
                    "Composable - AND, OR, NOT logic",
                ],
            },
            "prometheus": {
                "description": "Prometheus metrics integration",
                "metrics": [
                    "http_requests_total - Request counter",
                    "http_request_duration_seconds - Latency histogram",
                    "http_request_size_bytes - Request size",
                    "http_response_size_bytes - Response size",
                    "http_requests_in_progress - Active requests gauge",
                ],
                "endpoint": "/metrics",
            },
        },
        "endpoints": {
            "/": "Home page",
            "/health": "Health check",
            "/user": "User endpoint (requires auth)",
            "/admin": "Admin endpoint (requires admin role)",
            "/moderator": "Moderator endpoint (requires moderator permission)",
            "/admin-or-moderator": "Requires admin OR moderator",
            "/admin-with-permissions": "Requires admin AND permissions",
            "/ip-restricted": "IP whitelist restricted",
            "/users/{user_id}": "Get user (with DI)",
            "/users": "Create user (with DI)",
            "/api/stats": "Application statistics",
            "/metrics": "Prometheus metrics",
            "/docs": "This endpoint",
        },
    }


# ============================================================================
# Run the Application
# ============================================================================

if __name__ == "__main__":
    print("\n" + "=" * 70)
    print("Cello Framework - Advanced Middleware Example")
    print("=" * 70)
    print("\n✨ New Features Demonstrated:")
    print("   1. Dependency Injection (FastAPI-inspired)")
    print("   2. Guards/RBAC (Litestar-inspired)")
    print("   3. Prometheus Metrics")
    print("\n📊 Endpoints:")
    print("   - http://localhost:8000/ - Home page")
    print("   - http://localhost:8000/docs - Full API documentation")
    print("   - http://localhost:8000/metrics - Prometheus metrics")
    print("   - http://localhost:8000/user - Protected endpoint")
    print("   - http://localhost:8000/admin - Admin-only endpoint")
    print("\n🚀 Starting server...")
    print("=" * 70 + "\n")

    app.run(host="0.0.0.0", port=8000)
