from cello import App, Request
from cello.guards import Role, Permission, Authenticated, And, Or, Not, ForbiddenError

app = App()

# --- Mock Authentication Middleware ---
# In a real app, this would verify a token and set request.context["user"]
@app.post("/login/{username}")
def login(request: Request):
    username = request.params["username"]
    
    # Mock user data
    if username == "admin":
        user_data = {
            "id": 1, 
            "username": "admin", 
            "roles": ["admin"], 
            "permissions": ["users:read", "users:write", "users:delete"]
        }
    elif username == "mod":
        user_data = {
            "id": 2, 
            "username": "mod", 
            "roles": ["moderator"], 
            "permissions": ["users:read", "users:write"]
        }
    else:
        user_data = {
            "id": 3, 
            "username": "user", 
            "roles": ["user"], 
            "permissions": ["users:read"]
        }
        
    return {"token": "mock-token", "user": user_data}

# Helper to simulate auth for testing
def mock_auth_middleware(handler):
    def wrapper(request):
        # Check header for mock user type
        user_type = request.headers.get("X-Mock-User")
        if user_type:
            if user_type == "admin":
                 request.context["user"] = {
                    "roles": ["admin"], 
                    "permissions": ["users:read", "users:write", "users:delete"]
                }
            elif user_type == "mod":
                 request.context["user"] = {
                    "roles": ["moderator"], 
                    "permissions": ["users:read", "users:write"]
                }
            elif user_type == "user":
                 request.context["user"] = {
                    "roles": ["user"], 
                    "permissions": ["users:read"]
                }
        return handler(request)
    return wrapper

# Apply mock auth globally (for this example)
original_add_route = app._app.get
# This is a bit hacky for the example, normally you'd use app.enable_auth()
# or a proper middleware class. But for simplicity:
# We rely on the tests sending X-Mock-User header.

# --- Guards Examples ---

# 1. Role-based Guard
@app.get("/admin", guards=[Role(["admin"])])
def admin_only(request):
    return {"message": "Welcome Admin"}

# 2. Permission-based Guard
@app.post("/users", guards=[Permission(["users:write"])])
def create_user(request):
    return {"message": "User created"}

# 3. Multiple Guards (AND logic)
# Must be Admin AND have 'users:delete' permission
@app.delete("/users/{id}", guards=[
    Role(["admin"]),
    Permission(["users:delete"])
])
def delete_user(request):
    return {"message": f"User {request.params['id']} deleted"}

# 4. Composed Guards (OR logic)
# Admin OR Moderator
@app.get("/reports", guards=[
    Or([
        Role(["admin"]),
        Role(["moderator"])
    ])
])
def view_reports(request):
    return {"message": "Reports view"}

# 5. Custom Guard
class IPAllowlist:
    def __init__(self, allowed_ips):
        self.allowed_ips = allowed_ips
        
    def __call__(self, request):
        # Mocking IP check
        client_ip = request.headers.get("X-Real-IP", "127.0.0.1")
        if client_ip not in self.allowed_ips:
            raise ForbiddenError(f"IP {client_ip} not allowed")
        return True

@app.get("/internal", guards=[IPAllowlist(["127.0.0.1"])])
def internal_api(request):
    return {"message": "Internal API access granted"}

if __name__ == "__main__":
    print("Run with: python examples/guards.py")
    app.run(port=8080)
