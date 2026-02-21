from typing import Any, List, Union, Callable, Dict
import hmac
import inspect
from functools import wraps


def constant_time_compare(a: str, b: str) -> bool:
    """Compare two strings in constant time to prevent timing attacks.

    SECURITY: Always use this function (or hmac.compare_digest) instead of
    ``==`` when comparing auth tokens, passwords, API keys, or any other
    secret values. Direct ``==`` comparison leaks information about how
    many leading characters match, enabling timing-based attacks.

    Args:
        a: First string to compare.
        b: Second string to compare.

    Returns:
        True if the strings are equal, False otherwise.
    """
    return hmac.compare_digest(a.encode("utf-8"), b.encode("utf-8"))

class GuardError(Exception):
    """Base class for guard errors."""
    def __init__(self, message: str, status_code: int = 403):
        self.message = message
        self.status_code = status_code
        super().__init__(message)

class ForbiddenError(GuardError):
    """Raised when access is forbidden."""
    pass

class UnauthorizedError(GuardError):
    """Raised when authentication is required."""
    def __init__(self, message: str = "Authentication required"):
        super().__init__(message, 401)

class Guard:
    """Base class for guards."""
    def __call__(self, request: Any) -> Union[bool, None, str]:
        raise NotImplementedError

class Role(Guard):
    """
    Check if user has required roles.
    Assumes `request.context["user"]["roles"]` exists and is a list of strings.
    """
    def __init__(self, roles: List[str], require_all: bool = False, user_key: str = "user", role_key: str = "roles"):
        self.roles = set(roles)
        self.require_all = require_all
        self.user_key = user_key
        self.role_key = role_key

    def __call__(self, request: Any) -> Union[bool, None, str]:
        user = getattr(request, "context", {}).get(self.user_key)
        if not user:
             raise UnauthorizedError()
        
        user_roles = user.get(self.role_key, [])
        if not isinstance(user_roles, list):
             # Try to handle single role string
             if isinstance(user_roles, str):
                 user_roles = [user_roles]
             else:
                 user_roles = []
        
        user_roles_set = set(user_roles)

        if self.require_all:
            if not self.roles.issubset(user_roles_set):
                 missing = self.roles - user_roles_set
                 raise ForbiddenError(f"Missing required roles: {', '.join(missing)}")
        else:
            if not self.roles.intersection(user_roles_set):
                 raise ForbiddenError(f"Requires one of roles: {', '.join(self.roles)}")
        
        return True

class Permission(Guard):
    """
    Check if user has required permissions.
    Assumes `request.context["user"]["permissions"]` exists and is a list of strings.
    """
    def __init__(self, permissions: List[str], require_all: bool = True, user_key: str = "user", perm_key: str = "permissions"):
        self.permissions = set(permissions)
        self.require_all = require_all
        self.user_key = user_key
        self.perm_key = perm_key

    def __call__(self, request: Any) -> Union[bool, None, str]:
        user = getattr(request, "context", {}).get(self.user_key)
        if not user:
             raise UnauthorizedError()
        
        user_perms = user.get(self.perm_key, [])
        if not isinstance(user_perms, list):
             if isinstance(user_perms, str):
                 user_perms = [user_perms]
             else:
                 user_perms = []
        
        user_perms_set = set(user_perms)

        if self.require_all:
            if not self.permissions.issubset(user_perms_set):
                 missing = self.permissions - user_perms_set
                 raise ForbiddenError(f"Missing required permissions: {', '.join(missing)}")
        else:
            if not self.permissions.intersection(user_perms_set):
                 raise ForbiddenError(f"Requires one of permissions: {', '.join(self.permissions)}")
        
        return True

class Authenticated(Guard):
    """Ensure user is authenticated (present in context)."""
    def __init__(self, user_key: str = "user"):
        self.user_key = user_key

    def __call__(self, request: Any) -> Union[bool, None, str]:
        if not getattr(request, "context", {}).get(self.user_key):
             raise UnauthorizedError()
        return True

class And(Guard):
    """Pass only if ALL guards pass."""
    def __init__(self, guards: List[Callable]):
        self.guards = guards

    def __call__(self, request: Any):
        for guard in self.guards:
             result = guard(request)
             # If guard returns False or string, it failed (though our guards raise exceptions)
             if result is False:
                 raise ForbiddenError("Guard check failed")
             if isinstance(result, str):
                 raise ForbiddenError(result)
        return True

class Or(Guard):
    """Pass if ANY guard passes."""
    def __init__(self, guards: List[Callable]):
        self.guards = guards

    def __call__(self, request: Any):
        last_error = None
        for guard in self.guards:
            try:
                result = guard(request)
                if result is not False and not isinstance(result, str):
                    return True
            except GuardError as e:
                last_error = e

        # If we get here, all guards failed
        if last_error:
            raise last_error
        raise ForbiddenError("All guards failed")

class Not(Guard):
    """Invert the result of a guard."""
    def __init__(self, guard: Callable):
        self.guard = guard

    def __call__(self, request: Any):
        try:
            result = self.guard(request)
            if result is False or isinstance(result, str):
                 return True # Guard failed, so NOT Guard passes
        except GuardError:
             return True # Guard raised GuardError, so NOT Guard passes

        raise ForbiddenError("Guard succeeded but was expected to fail")

def verify_guards(guards: List[Callable], request: Any):
    """Helper to verify a list of guards (AND logic by default)."""
    for guard in guards:
        try:
             result = guard(request)
             if result is False:
                 raise ForbiddenError("Access denied")
             if isinstance(result, str):
                 raise ForbiddenError(result)
        except GuardError:
             raise
        except Exception as e:
             # Wrap other exceptions? Or just let them bubble?
             # For now, treat generic errors often as unexpected 500s, 
             # but if a guard crashes, maybe we should fail closed (Forbidden).
             raise ForbiddenError(f"Guard error: {str(e)}")
