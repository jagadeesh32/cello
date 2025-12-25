
import pytest
from cello import App
from cello.guards import (
    Role, Permission, Authenticated, 
    And, Or, Not, 
    ForbiddenError, UnauthorizedError
)

# Mock Request class for testing guards directly
class MockRequest:
    def __init__(self, context=None, headers=None):
        self.context = context or {}
        self.headers = headers or {}
        self.params = {}

def test_role_guard():
    guard = Role(["admin"])
    
    # Fail: No user
    req = MockRequest()
    with pytest.raises(UnauthorizedError):
        guard(req)
        
    # Fail: Wrong role
    req = MockRequest(context={"user": {"roles": ["user"]}})
    with pytest.raises(ForbiddenError):
        guard(req)
        
    # Pass: Correct role
    req = MockRequest(context={"user": {"roles": ["admin"]}})
    assert guard(req) is True

def test_permission_guard():
    guard = Permission(["read", "write"])
    
    # Fail: Missing permission
    req = MockRequest(context={"user": {"permissions": ["read"]}})
    with pytest.raises(ForbiddenError):
        guard(req)
        
    # Pass: All permissions
    req = MockRequest(context={"user": {"permissions": ["read", "write"]}})
    assert guard(req) is True

def test_or_guard():
    guard = Or([Role(["admin"]), Role(["mod"])])
    
    # Pass: Is admin
    req = MockRequest(context={"user": {"roles": ["admin"]}})
    assert guard(req) is True
    
    # Pass: Is mod
    req = MockRequest(context={"user": {"roles": ["mod"]}})
    assert guard(req) is True
    
    # Fail: Is user
    req = MockRequest(context={"user": {"roles": ["user"]}})
    with pytest.raises(ForbiddenError):
        guard(req)

def test_route_integration():
    """Test full integration with App routing."""
    app = App()
    
    # Setup test routes
    @app.get("/admin", guards=[Role(["admin"])])
    def admin_only(request):
        return {"status": "ok"}
        
    @app.get("/public")
    def public(request):
        return {"status": "ok"}
        
    # We need to manually simulate request context injection
    # since we don't have the auth middleware running in this unit test.
    # But wait, guards check 'request.context'.
    # In a real request, how do we inject context?
    # Usually AuthMiddleware does it.
    
    # For this integration test, we might need to mock the request handling 
    # or rely on the fact that Cello's test client (if available) passes request.
    # Cello doesn't have a Python-native test client exposed easily that bypasses Rust server?
    # Actually, we can just call the decorated function directly with a mock request!
    # Because `app.get` returns the decorated function which includes the wrapper.
    
    # 1. Test Admin Route (Success)
    req = MockRequest(context={"user": {"roles": ["admin"]}})
    result = admin_only(req)
    assert result == {"status": "ok"}
    
    # 2. Test Admin Route (Fail)
    req = MockRequest(context={"user": {"roles": ["user"]}})
    with pytest.raises(ForbiddenError):
        admin_only(req)

if __name__ == "__main__":
    # verification script usage
    test_role_guard()
    test_permission_guard()
    test_or_guard()
    test_route_integration()
    print("All Guard tests passed!")
