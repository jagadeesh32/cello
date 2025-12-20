#!/usr/bin/env python3
"""
Cello Framework - Auto-Generated OpenAPI Demo
===============================================

This example shows automatic OpenAPI spec generation from routes.
No manual spec needed - just define your routes and call enable_openapi()!

Run with: python examples/auto_openapi_demo.py
Then visit: http://127.0.0.1:8080/docs
"""

from cello import App, Response

app = App()

# Enable CORS and logging
app.enable_cors(origins=["*"])
app.enable_logging()


# =============================================================================
# Define your API routes - OpenAPI will be auto-generated!
# =============================================================================

@app.get("/", tags=["General"], summary="API Home")
def home(request):
    """Returns welcome message and API info."""
    return {
        "message": "Welcome to Cello API!",
        "version": "1.0.0",
        "docs": "/docs"
    }


@app.get("/health", tags=["General"], summary="Health Check")
def health_check(request):
    """Check server health status."""
    return {"status": "healthy", "framework": "cello"}


@app.get("/users", tags=["Users"], summary="List Users")
def list_users(request):
    """Get all users from the database."""
    return {
        "users": [
            {"id": 1, "name": "Alice", "email": "alice@example.com"},
            {"id": 2, "name": "Bob", "email": "bob@example.com"},
        ]
    }


@app.get("/users/{id}", tags=["Users"], summary="Get User")
def get_user(request):
    """Get a specific user by their ID."""
    user_id = request.params.get("id")
    return {"id": int(user_id), "name": f"User {user_id}"}


@app.post("/users", tags=["Users"], summary="Create User")
def create_user(request):
    """Create a new user in the database."""
    data = request.json()
    return {"message": "User created", "user": data, "id": 123}


@app.put("/users/{id}", tags=["Users"], summary="Update User")
def update_user(request):
    """Update an existing user."""
    user_id = request.params.get("id")
    data = request.json()
    return {"message": f"User {user_id} updated", "user": data}


@app.delete("/users/{id}", tags=["Users"], summary="Delete User")
def delete_user(request):
    """Delete a user from the database."""
    user_id = request.params.get("id")
    return {"message": f"User {user_id} deleted"}


@app.get("/items", tags=["Items"], summary="List Items")
def list_items(request):
    """Get all items with optional pagination."""
    limit = request.query.get("limit", "10")
    return {
        "items": [
            {"id": 1, "name": "Laptop", "price": 999.99},
            {"id": 2, "name": "Mouse", "price": 29.99},
        ],
        "limit": int(limit)
    }


@app.post("/items", tags=["Items"], summary="Create Item")
def create_item(request):
    """Create a new item in the catalog."""
    data = request.json()
    return {"message": "Item created", "item": data}


# =============================================================================
# Enable OpenAPI - spec will be auto-generated from routes above!
# =============================================================================

app.enable_openapi(title="My Auto-Generated API", version="1.0.0")



# =============================================================================
# Run Server
# =============================================================================

if __name__ == "__main__":
    print("\n" + "="*60)
    print("  AUTO-GENERATED OPENAPI DEMO")
    print("="*60)
    print("\n  Routes are automatically documented!")
    print("  No manual OpenAPI spec needed!")
    print("\n  Visit: http://127.0.0.1:8080/docs")
    print("\n" + "="*60 + "\n")
    
    app.run(host="127.0.0.1", port=8080)
