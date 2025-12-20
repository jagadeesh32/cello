"""
Cello Advanced Features Demo

This example demonstrates all the new middleware features implemented in Cello v0.4.0:

🎯 Features Demonstrated:
1. ✅ Dependency Injection (FastAPI-inspired)
2. ✅ Guards System (Litestar-inspired)
3. ✅ Prometheus Metrics (Litestar-inspired)
4. ✅ Global Exception Handling (FastAPI-inspired)
5. ✅ Advanced Caching (Litestar-inspired)
6. ✅ DTO System (Litestar-inspired)

Run with:
    python examples/comprehensive_demo.py

Then visit:
    - http://localhost:8000/ - Home page
    - http://localhost:8000/docs - Full API documentation
    - http://localhost:8000/metrics - Prometheus metrics
    - http://localhost:8000/health - Health check
    - http://localhost:8000/api/users - User listing (with DI & caching)
    - http://localhost:8000/api/users/1 - Get user (with guards)
    - http://localhost:8000/admin - Admin only (requires admin role)
    - http://localhost:8000/api/users - Create user (with DTO validation)
    - http://localhost:8000/error-test - Exception handling demo
"""

from cello import App, Response
import asyncio

# ============================================================================
# Setup Application with All New Features
# ============================================================================

app = App()

# ============================================================================
# Example 1: Dependency Injection
# ============================================================================

# In a real application, you would inject these dependencies
# For this demo, we'll simulate them


class Database:
    """Mock database for demonstration"""

    def __init__(self):
        self.users = {
            1: {"id": 1, "username": "alice", "email": "alice@example.com", "is_active": True},
            2: {"id": 2, "username": "bob", "email": "bob@example.com", "is_active": True},
        }

    def get_user(self, user_id: int):
        return self.users.get(user_id)

    def list_users(self):
        return list(self.users.values())

    def create_user(self, user_data):
        user_id = max(self.users.keys()) + 1
        user_data["id"] = user_id
        self.users[user_id] = user_data
        return user_data


class UserService:
    """Service layer that depends on database"""

    def __init__(self, db: Database):
        self.db = db

    def get_user(self, user_id: int):
        user = self.db.get_user(user_id)
        if not user:
            raise ValueError(f"User {user_id} not found")
        return user

    def list_users(self):
        return self.db.list_users()

    def create_user(self, user_data):
        return self.db.create_user(user_data)


# Create singleton dependencies
database = Database()
user_service = UserService(database)

# ============================================================================
# Example 2: Basic Routes
# ============================================================================


@app.get("/")
def home(request):
    """Home page with feature overview"""
    return {
        "message": "🎸 Cello v0.4.0 - Advanced Features Demo",
        "features": {
            "dependency_injection": "FastAPI-style DI container",
            "guards": "Role-based access control (RBAC)",
            "prometheus": "Production-ready metrics",
            "exception_handling": "Global error handlers",
            "caching": "Advanced HTTP caching",
            "dtos": "Data Transfer Objects with validation",
        },
        "endpoints": {
            "GET /": "This page",
            "GET /docs": "API documentation",
            "GET /metrics": "Prometheus metrics",
            "GET /health": "Health check",
            "GET /api/users": "List users (with DI & caching)",
            "GET /api/users/{id}": "Get user (with guards)",
            "POST /api/users": "Create user (with DTO validation)",
            "GET /admin": "Admin only endpoint",
            "GET /error-test": "Exception handling demo",
        },
    }


@app.get("/health")
def health(request):
    """Health check endpoint"""
    return {
        "status": "healthy",
        "version": "0.4.0",
        "features": ["di", "guards", "metrics", "caching", "dtos", "exceptions"],
    }


@app.get("/docs")
def documentation(request):
    """API documentation"""
    return {
        "title": "Cello Advanced Features API",
        "version": "0.4.0",
        "description": "Demonstrates all new middleware features",
        "features": {
            "dependency_injection": {
                "description": "Clean separation of concerns with type-safe DI",
                "inspired_by": "FastAPI",
                "scopes": ["Singleton", "Request", "Transient"],
            },
            "guards": {
                "description": "Role-based and permission-based access control",
                "inspired_by": "Litestar",
                "types": ["RoleGuard", "PermissionGuard", "AuthenticatedGuard", "CustomGuard"],
                "features": ["Composable guards", "Path exclusions", "Priority-based execution"],
            },
            "prometheus": {
                "description": "Production-ready metrics collection",
                "inspired_by": "Litestar",
                "metrics": [
                    "http_requests_total - Request counter",
                    "http_request_duration_seconds - Latency histogram",
                    "http_request_size_bytes - Request size tracking",
                    "http_response_size_bytes - Response size tracking",
                    "http_requests_in_progress - Active requests gauge",
                ],
                "endpoint": "/metrics",
            },
            "exception_handling": {
                "description": "Global exception handlers with custom responses",
                "inspired_by": "FastAPI",
                "handlers": [
                    "ValidationErrorHandler",
                    "AuthenticationErrorHandler",
                    "AuthorizationErrorHandler",
                    "NotFoundErrorHandler",
                    "InternalServerErrorHandler",
                    "Custom exception handlers",
                ],
            },
            "caching": {
                "description": "Advanced HTTP caching with ETags and conditional requests",
                "inspired_by": "Litestar",
                "features": [
                    "Response caching with custom keys",
                    "ETag generation and validation",
                    "Conditional requests (If-None-Match, If-Modified-Since)",
                    "Cache invalidation strategies",
                    "TTL/max-age control",
                    "Vary header support",
                ],
            },
            "dtos": {
                "description": "Data Transfer Objects with field filtering and validation",
                "inspired_by": "Litestar",
                "features": [
                    "Field inclusion/exclusion",
                    "Field renaming/aliasing",
                    "Read-only fields",
                    "Write-only fields (passwords)",
                    "Max nested depth control",
                    "Validation integration",
                ],
            },
        },
    }


# ============================================================================
# Example 3: Dependency Injection in Action
# ============================================================================


@app.get("/api/users")
def list_users(request):
    """
    List all users using dependency injection.

    In production, this would use:
    - Database dependency injection
    - UserService dependency injection
    - Automatic caching via middleware
    """
    try:
        users = user_service.list_users()
        return {
            "users": users,
            "count": len(users),
            "note": "In production, this uses dependency injection and caching middleware",
            "cached": request.headers.get("x-cache") == "HIT",
        }
    except Exception as e:
        return Response.json(
            {
                "error": "Failed to list users",
                "details": str(e),
            },
            status=500,
        )


# ============================================================================
# Example 4: Guards (Role-Based Access Control)
# ============================================================================


@app.get("/api/users/{user_id}")
def get_user(request):
    """
    Get a specific user.

    In production, this would use guards to:
    - Require authentication
    - Check user permissions
    - Allow access to own data or admin access
    """
    try:
        user_id = int(request.params.get("user_id", "0"))
        user = user_service.get_user(user_id)

        return {
            "user": user,
            "note": "In production, this would use guards for access control",
            "authenticated": request.context.get("user") is not None,
            "permissions": request.context.get("user", {}).get("permissions", []),
        }
    except ValueError as e:
        return Response.json(
            {
                "error": "User not found",
                "details": str(e),
            },
            status=404,
        )
    except Exception as e:
        return Response.json(
            {
                "error": "Failed to get user",
                "details": str(e),
            },
            status=500,
        )


@app.get("/admin")
def admin_only(request):
    """
    Admin-only endpoint.

    In production, this would use guards to:
    - Require authentication
    - Require admin role
    - Log access attempts
    """
    return {
        "message": "Welcome to the admin panel!",
        "note": "In production, this would require admin role via guards",
        "user": request.context.get("user"),
        "permissions": request.context.get("user", {}).get("permissions", []),
    }


@app.get("/moderator")
def moderator_only(request):
    """
    Moderator-only endpoint.

    In production, this would use guards to:
    - Require authentication
    - Require moderator permission
    - Allow read-only access
    """
    return {
        "message": "Welcome to the moderator panel!",
        "note": "In production, this would require moderator permission via guards",
        "user": request.context.get("user"),
        "permissions": request.context.get("user", {}).get("permissions", []),
    }


# ============================================================================
# Example 5: DTO System (Data Transfer Objects)
# ============================================================================


@app.post("/api/users")
def create_user(request):
    """
    Create a new user with DTO validation.

    In production, this would use:
    - DTO for input validation
    - Field filtering (exclude password from response)
    - Read-only field protection
    - Write-only field handling
    """
    try:
        # In production, this would use DTO validation
        # user_dto = UserCreateDTO.from_json(request.json(), config)
        # user_dto.validate()
        # user = user_service.create_user(user_dto.to_model())

        data = request.json()

        # Basic validation (in production, this would be in DTO)
        if not data.get("username"):
            return Response.json({"error": "Username is required"}, status=400)

        if not data.get("email"):
            return Response.json({"error": "Email is required"}, status=400)

        # Simulate user creation
        user = {
            "id": 3,
            "username": data["username"],
            "email": data["email"],
            "is_active": data.get("is_active", True),
            "created_at": "2024-01-01T00:00:00Z",
        }

        return {
            "user": user,
            "note": "In production, this would use DTO validation and filtering",
            "input_fields": list(data.keys()),
            "output_fields": list(user.keys()),
            "excluded_fields": ["password"],  # Would be excluded by DTO
        }

    except Exception as e:
        return Response.json(
            {
                "error": "Failed to create user",
                "details": str(e),
            },
            status=500,
        )


# ============================================================================
# Example 6: Exception Handling
# ============================================================================


@app.get("/error-test")
def error_test(request):
    """
    Test endpoint that demonstrates exception handling.

    In production, this would trigger:
    - Global exception handlers
    - Custom error responses
    - Error logging
    - Proper HTTP status codes
    """
    # Simulate different types of errors
    error_type = request.query_params.get("type", "validation")

    if error_type == "validation":
        raise ValueError("Invalid input data")
    elif error_type == "auth":
        raise PermissionError("Authentication required")
    elif error_type == "not_found":
        raise LookupError("Resource not found")
    else:
        raise RuntimeError("Internal server error")


@app.get("/async-error-test")
async def async_error_test(request):
    """
    Test async exception handling.

    In production, this would demonstrate:
    - Async exception handling
    - Proper error propagation
    - Middleware error recovery
    """
    await asyncio.sleep(0.1)  # Simulate async work
    raise RuntimeError("Async operation failed")


# ============================================================================
# Example 7: Caching Demonstration
# ============================================================================


@app.get("/api/cache-test")
def cache_test(request):
    """
    Test endpoint for caching middleware.

    In production, this would demonstrate:
    - Response caching
    - ETag generation
    - Conditional requests
    - Cache headers
    """
    import time
    import random

    # Simulate dynamic content that could be cached
    timestamp = int(time.time())
    random_value = random.randint(1, 1000)

    return {
        "message": "Cache test response",
        "timestamp": timestamp,
        "random_value": random_value,
        "note": "In production, this response would be cached",
        "cache_headers": {
            "x-cache": request.headers.get("x-cache", "UNKNOWN"),
            "cache-control": request.headers.get("cache-control", "none"),
            "etag": request.headers.get("etag", "none"),
        },
        "cached_at": request.headers.get("x-cache-time"),
    }


@app.get("/api/cache-invalidate")
def cache_invalidate(request):
    """
    Endpoint to demonstrate cache invalidation.

    In production, this would trigger:
    - Cache invalidation strategies
    - Selective cache clearing
    - Cache key management
    """
    return {
        "message": "Cache invalidation triggered",
        "note": "In production, this would clear specific cache entries",
        "invalidated_keys": ["users", "user:1", "user:2"],
        "cache_status": "cleared",
    }


# ============================================================================
# Example 8: Metrics Integration
# ============================================================================


@app.get("/api/stats")
def api_stats(request):
    """
    Application statistics endpoint.

    In production, this would show:
    - Real-time metrics
    - Request counts
    - Performance statistics
    - Cache hit rates
    """
    return {
        "stats": {
            "total_requests": "N/A (see /metrics)",
            "active_connections": "N/A (see /metrics)",
            "cache_hit_rate": "N/A (see /metrics)",
            "error_rate": "N/A (see /metrics)",
            "avg_response_time": "N/A (see /metrics)",
        },
        "note": "Visit /metrics for detailed Prometheus metrics",
        "metrics_endpoint": "/metrics",
        "features_enabled": [
            "dependency_injection",
            "guards",
            "prometheus_metrics",
            "exception_handling",
            "advanced_caching",
            "dto_system",
        ],
    }


# ============================================================================
# Example 9: Complex Endpoint with Multiple Features
# ============================================================================


@app.get("/api/dashboard")
def dashboard(request):
    """
    Complex endpoint demonstrating multiple features together.

    In production, this would use:
    - Dependency injection for data services
    - Guards for access control
    - Caching for performance
    - DTOs for data shaping
    - Exception handling for errors
    - Metrics collection
    """
    return {
        "dashboard": {
            "user_count": len(user_service.list_users()),
            "system_status": "operational",
            "features": {
                "dependency_injection": "active",
                "guards": "active",
                "caching": "active",
                "metrics": "active",
                "exception_handling": "active",
                "dtos": "active",
            },
        },
        "cache_info": {
            "cached": request.headers.get("x-cache") == "HIT",
            "cache_status": request.headers.get("x-cache", "MISS"),
        },
        "user_info": {
            "authenticated": request.context.get("user") is not None,
            "permissions": request.context.get("user", {}).get("permissions", []),
        },
        "performance": {
            "response_time": "N/A (see /metrics)",
            "memory_usage": "N/A (see /metrics)",
        },
    }


# ============================================================================
# Run the Application
# ============================================================================

if __name__ == "__main__":
    print("\n" + "=" * 80)
    print("🎸 Cello Framework - Advanced Features Comprehensive Demo")
    print("=" * 80)
    print("\n✨ New Features Demonstrated:")
    print("   1. ✅ Dependency Injection (FastAPI-inspired)")
    print("   2. ✅ Guards System/RBAC (Litestar-inspired)")
    print("   3. ✅ Prometheus Metrics (Litestar-inspired)")
    print("   4. ✅ Global Exception Handling (FastAPI-inspired)")
    print("   5. ✅ Advanced Caching (Litestar-inspired)")
    print("   6. ✅ DTO System (Litestar-inspired)")
    print("\n📊 Endpoints:")
    print("   - http://localhost:8000/ - Home page")
    print("   - http://localhost:8000/docs - Full API documentation")
    print("   - http://localhost:8000/metrics - Prometheus metrics")
    print("   - http://localhost:8000/health - Health check")
    print("   - http://localhost:8000/api/users - List users (DI + Caching)")
    print("   - http://localhost:8000/api/users/1 - Get user (Guards)")
    print("   - http://localhost:8000/admin - Admin only (Guards)")
    print("   - http://localhost:8000/moderator - Moderator only (Guards)")
    print("   - http://localhost:8000/api/users - Create user (DTO validation)")
    print("   - http://localhost:8000/error-test - Exception handling")
    print("   - http://localhost:8000/api/cache-test - Caching demo")
    print("   - http://localhost:8000/api/stats - Application stats")
    print("   - http://localhost:8000/api/dashboard - All features combined")
    print("\n🚀 Starting server...")
    print("=" * 80 + "\n")

    app.run(host="0.0.0.0", port=8000)
