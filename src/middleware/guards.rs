//! Guards middleware for Cello (inspired by Litestar).
//!
//! Provides:
//! - Role-based access control (RBAC)
//! - Permission-based guards
//! - Composable guards (AND, OR, NOT)
//! - Route-level and controller-level guards
//! - Custom guard logic

use std::collections::HashSet;
use std::sync::Arc;

use super::{Middleware, MiddlewareAction, MiddlewareError, MiddlewareResult};
use crate::request::Request;
use pyo3::prelude::*;

// ============================================================================
// Guard Types
// ============================================================================

/// Result type for guard checks.
pub type GuardResult = Result<(), GuardError>;

/// Guard error types.
#[derive(Debug, Clone)]
pub enum GuardError {
    Forbidden(String),
    Unauthorized(String),
    Custom(String, u16),
}

impl std::fmt::Display for GuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GuardError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            GuardError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            GuardError::Custom(msg, code) => write!(f, "Guard error ({}): {}", code, msg),
        }
    }
}

impl std::error::Error for GuardError {}

impl From<GuardError> for MiddlewareError {
    fn from(err: GuardError) -> Self {
        match err {
            GuardError::Forbidden(msg) => MiddlewareError::forbidden(&msg),
            GuardError::Unauthorized(msg) => MiddlewareError::unauthorized(&msg),
            GuardError::Custom(msg, code) => MiddlewareError::new(&msg, code),
        }
    }
}

// ============================================================================
// Guard Trait
// ============================================================================

/// Trait for implementing guards.
pub trait Guard: Send + Sync {
    /// Check if the request passes this guard.
    fn check(&self, request: &Request) -> GuardResult;

    /// Guard priority (lower = runs first).
    fn priority(&self) -> i32 {
        0
    }

    /// Guard name for debugging.
    fn name(&self) -> &str {
        "unnamed_guard"
    }

    /// Whether this guard should run for a given path.
    fn should_run(&self, _path: &str) -> bool {
        true
    }
}

// ============================================================================
// Built-in Guards
// ============================================================================

/// Guard that checks for specific roles.
pub struct RoleGuard {
    allowed_roles: HashSet<String>,
    require_all: bool,
    user_key: String,
    role_key: String,
}

impl RoleGuard {
    /// Create a new role guard.
    pub fn new(allowed_roles: Vec<&str>) -> Self {
        Self {
            allowed_roles: allowed_roles.iter().map(|s| s.to_string()).collect(),
            require_all: false,
            user_key: "user".to_string(),
            role_key: "roles".to_string(),
        }
    }

    /// Require all roles (default: require any).
    pub fn require_all(mut self) -> Self {
        self.require_all = true;
        self
    }

    /// Set custom user context key.
    pub fn user_key(mut self, key: &str) -> Self {
        self.user_key = key.to_string();
        self
    }

    /// Set custom role key within user object.
    pub fn role_key(mut self, key: &str) -> Self {
        self.role_key = key.to_string();
        self
    }

    /// Extract user roles from request context.
    fn get_user_roles(&self, request: &Request) -> Vec<String> {
        request
            .context
            .get(&self.user_key)
            .and_then(|user| user.get(&self.role_key))
            .and_then(|roles| {
                if let Some(arr) = roles.as_array() {
                    Some(
                        arr.iter()
                            .filter_map(|r| r.as_str())
                            .map(|s| s.to_string())
                            .collect(),
                    )
                } else if let Some(role) = roles.as_str() {
                    Some(vec![role.to_string()])
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }
}

impl Guard for RoleGuard {
    fn check(&self, request: &Request) -> GuardResult {
        let user_roles = self.get_user_roles(request);

        if user_roles.is_empty() {
            return Err(GuardError::Unauthorized(
                "User must be authenticated".to_string(),
            ));
        }

        let has_access = if self.require_all {
            // User must have all required roles
            self.allowed_roles.iter().all(|r| user_roles.contains(r))
        } else {
            // User must have at least one required role
            self.allowed_roles.iter().any(|r| user_roles.contains(r))
        };

        if has_access {
            Ok(())
        } else {
            Err(GuardError::Forbidden(format!(
                "User does not have required role(s): {:?}",
                self.allowed_roles
            )))
        }
    }

    fn name(&self) -> &str {
        "role_guard"
    }

    fn priority(&self) -> i32 {
        -30 // Run after auth but early
    }
}

/// Guard that checks for specific permissions.
pub struct PermissionGuard {
    required_permissions: Vec<String>,
    require_all: bool,
    user_key: String,
    permission_key: String,
}

impl PermissionGuard {
    /// Create a new permission guard.
    pub fn new(required_permissions: Vec<&str>) -> Self {
        Self {
            required_permissions: required_permissions.iter().map(|s| s.to_string()).collect(),
            require_all: true,
            user_key: "user".to_string(),
            permission_key: "permissions".to_string(),
        }
    }

    /// Require any permission (default: require all).
    pub fn require_any(mut self) -> Self {
        self.require_all = false;
        self
    }

    /// Set custom user context key.
    pub fn user_key(mut self, key: &str) -> Self {
        self.user_key = key.to_string();
        self
    }

    /// Set custom permission key.
    pub fn permission_key(mut self, key: &str) -> Self {
        self.permission_key = key.to_string();
        self
    }

    /// Extract user permissions from request context.
    fn get_user_permissions(&self, request: &Request) -> Vec<String> {
        request
            .context
            .get(&self.user_key)
            .and_then(|user| user.get(&self.permission_key))
            .and_then(|perms| {
                perms
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| p.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    })
            })
            .unwrap_or_default()
    }
}

impl Guard for PermissionGuard {
    fn check(&self, request: &Request) -> GuardResult {
        let user_perms = self.get_user_permissions(request);

        if user_perms.is_empty() {
            return Err(GuardError::Unauthorized(
                "User must be authenticated".to_string(),
            ));
        }

        let has_access = if self.require_all {
            self.required_permissions.iter().all(|p| user_perms.contains(p))
        } else {
            self.required_permissions.iter().any(|p| user_perms.contains(p))
        };

        if has_access {
            Ok(())
        } else {
            Err(GuardError::Forbidden(format!(
                "User does not have required permission(s): {:?}",
                self.required_permissions
            )))
        }
    }

    fn name(&self) -> &str {
        "permission_guard"
    }

    fn priority(&self) -> i32 {
        -30
    }
}

/// Guard that checks if user is authenticated.
pub struct AuthenticatedGuard {
    user_key: String,
}

impl AuthenticatedGuard {
    pub fn new() -> Self {
        Self {
            user_key: "user".to_string(),
        }
    }

    pub fn user_key(mut self, key: &str) -> Self {
        self.user_key = key.to_string();
        self
    }
}

impl Default for AuthenticatedGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Guard for AuthenticatedGuard {
    fn check(&self, request: &Request) -> GuardResult {
        if request.context.contains_key(&self.user_key) {
            Ok(())
        } else {
            Err(GuardError::Unauthorized(
                "Authentication required".to_string(),
            ))
        }
    }

    fn name(&self) -> &str {
        "authenticated_guard"
    }

    fn priority(&self) -> i32 {
        -40 // Run very early
    }
}

/// Guard with custom check function.
pub struct CustomGuard<F>
where
    F: Fn(&Request) -> GuardResult + Send + Sync,
{
    check_fn: F,
    name: String,
    priority: i32,
}

impl<F> CustomGuard<F>
where
    F: Fn(&Request) -> GuardResult + Send + Sync,
{
    pub fn new(name: &str, check_fn: F) -> Self {
        Self {
            check_fn,
            name: name.to_string(),
            priority: 0,
        }
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

impl<F> Guard for CustomGuard<F>
where
    F: Fn(&Request) -> GuardResult + Send + Sync,
{
    fn check(&self, request: &Request) -> GuardResult {
        (self.check_fn)(request)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn priority(&self) -> i32 {
        self.priority
    }
}

// ============================================================================
// Composable Guards
// ============================================================================

/// Guard that requires all sub-guards to pass (AND logic).
pub struct AndGuard {
    guards: Vec<Arc<dyn Guard>>,
}

impl AndGuard {
    pub fn new(guards: Vec<Arc<dyn Guard>>) -> Self {
        Self { guards }
    }
}

impl Guard for AndGuard {
    fn check(&self, request: &Request) -> GuardResult {
        for guard in &self.guards {
            guard.check(request)?;
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "and_guard"
    }
}

/// Guard that requires at least one sub-guard to pass (OR logic).
pub struct OrGuard {
    guards: Vec<Arc<dyn Guard>>,
}

impl OrGuard {
    pub fn new(guards: Vec<Arc<dyn Guard>>) -> Self {
        Self { guards }
    }
}

impl Guard for OrGuard {
    fn check(&self, request: &Request) -> GuardResult {
        let mut last_error = None;
        for guard in &self.guards {
            match guard.check(request) {
                Ok(()) => return Ok(()),
                Err(e) => last_error = Some(e),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            GuardError::Forbidden("No guards passed".to_string())
        }))
    }

    fn name(&self) -> &str {
        "or_guard"
    }
}

/// Guard that inverts another guard's result (NOT logic).
pub struct NotGuard {
    guard: Arc<dyn Guard>,
}

impl NotGuard {
    pub fn new(guard: Arc<dyn Guard>) -> Self {
        Self { guard }
    }
}

impl Guard for NotGuard {
    fn check(&self, request: &Request) -> GuardResult {
        match self.guard.check(request) {
            Ok(()) => Err(GuardError::Forbidden("Guard check inverted".to_string())),
            Err(_) => Ok(()),
        }
    }

    fn name(&self) -> &str {
        "not_guard"
    }
}

/// Guard that calls a Python function.
pub struct PythonGuard {
    handler: PyObject,
}

impl PythonGuard {
    pub fn new(handler: PyObject) -> Self {
        Self { handler }
    }
}

impl Guard for PythonGuard {
    fn check(&self, request: &Request) -> GuardResult {
        Python::with_gil(|py| {
            // Call the Python guard with the request
            let result = self.handler.call1(py, (request.clone(),))
                .map_err(|e| GuardError::Custom(format!("Python guard error: {}", e), 500))?;
            
            // Check if result is None (pass) or raises error (fail) or returns False (fail)
            if result.is_none(py) {
                return Ok(());
            }
            
            if let Ok(passed) = result.extract::<bool>(py) {
                if passed {
                    return Ok(());
                } else {
                    return Err(GuardError::Forbidden("Python guard returned False".to_string()));
                }
            }
            
            // If it returns a string, it's a failure message
            if let Ok(msg) = result.extract::<String>(py) {
                return Err(GuardError::Forbidden(msg));
            }

            Ok(())
        })
    }

    fn name(&self) -> &str {
        "python_guard"
    }
}

// ============================================================================
// Guards Middleware
// ============================================================================

/// Middleware that executes guards.
pub struct GuardsMiddleware {
    guards: parking_lot::RwLock<Vec<Arc<dyn Guard>>>,
    skip_paths: Vec<String>,
}

impl GuardsMiddleware {
    pub fn new() -> Self {
        Self {
            guards: parking_lot::RwLock::new(Vec::new()),
            skip_paths: Vec::new(),
        }
    }

    /// Add a guard.
    pub fn add_guard<G: Guard + 'static>(&self, guard: G) {
        let mut guards = self.guards.write();
        guards.push(Arc::new(guard));
        // Sort by priority
        guards.sort_by_key(|g| g.priority());
    }

    /// Skip guard checks for specific paths.
    pub fn skip_path(mut self, path: &str) -> Self {
        self.skip_paths.push(path.to_string());
        self
    }
}

impl Default for GuardsMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for GuardsMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Check if path should be skipped
        for skip_path in &self.skip_paths {
            if request.path.starts_with(skip_path) {
                return Ok(MiddlewareAction::Continue);
            }
        }

        // Execute guards in priority order
        let guards = self.guards.read();
        for guard in &*guards {
            if !guard.should_run(&request.path) {
                continue;
            }

            match guard.check(request) {
                Ok(()) => continue,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -25 // Run after auth
    }

    fn name(&self) -> &str {
        "guards"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_guard() {
        let guard = RoleGuard::new(vec!["admin", "moderator"]);

        // No user in context
        let mut request = Request::default();
        assert!(guard.check(&request).is_err());

        // User with correct role
        request.context.insert(
            "user".to_string(),
            serde_json::json!({
                "roles": ["admin"]
            }),
        );
        assert!(guard.check(&request).is_ok());

        // User with wrong role
        request.context.insert(
            "user".to_string(),
            serde_json::json!({
                "roles": ["user"]
            }),
        );
        assert!(guard.check(&request).is_err());
    }

    #[test]
    fn test_permission_guard() {
        let guard = PermissionGuard::new(vec!["users:read", "users:write"]);

        let mut request = Request::default();
        request.context.insert(
            "user".to_string(),
            serde_json::json!({
                "permissions": ["users:read", "users:write", "posts:read"]
            }),
        );

        assert!(guard.check(&request).is_ok());

        // Missing one permission
        request.context.insert(
            "user".to_string(),
            serde_json::json!({
                "permissions": ["users:read"]
            }),
        );
        assert!(guard.check(&request).is_err());

        // With require_any
        let guard = PermissionGuard::new(vec!["users:read", "users:write"]).require_any();
        assert!(guard.check(&request).is_ok());
    }

    #[test]
    fn test_authenticated_guard() {
        let guard = AuthenticatedGuard::new();

        // No user
        let request = Request::default();
        assert!(guard.check(&request).is_err());

        // With user
        let mut request = Request::default();
        request.context.insert(
            "user".to_string(),
            serde_json::json!({"id": 123}),
        );
        assert!(guard.check(&request).is_ok());
    }

    #[test]
    fn test_and_guard() {
        let guard = AndGuard::new(vec![
            Arc::new(AuthenticatedGuard::new()),
            Arc::new(RoleGuard::new(vec!["admin"])),
        ]);

        let mut request = Request::default();

        // No user - fails
        assert!(guard.check(&request).is_err());

        // User without admin role - fails
        request.context.insert(
            "user".to_string(),
            serde_json::json!({"roles": ["user"]}),
        );
        assert!(guard.check(&request).is_err());

        // User with admin role - passes
        request.context.insert(
            "user".to_string(),
            serde_json::json!({"roles": ["admin"]}),
        );
        assert!(guard.check(&request).is_ok());
    }

    #[test]
    fn test_or_guard() {
        let guard = OrGuard::new(vec![
            Arc::new(RoleGuard::new(vec!["admin"])),
            Arc::new(RoleGuard::new(vec!["moderator"])),
        ]);

        let mut request = Request::default();
        request.context.insert(
            "user".to_string(),
            serde_json::json!({"roles": ["moderator"]}),
        );

        assert!(guard.check(&request).is_ok());
    }

    #[test]
    fn test_custom_guard() {
        let guard = CustomGuard::new("ip_whitelist", |request: &Request| {
            let ip = request
                .headers
                .get("x-forwarded-for")
                .or_else(|| request.headers.get("x-real-ip"))
                .map(|s| s.as_str())
                .unwrap_or("unknown");

            if ip == "127.0.0.1" || ip == "::1" {
                Ok(())
            } else {
                Err(GuardError::Forbidden(format!("IP {} not whitelisted", ip)))
            }
        });

        let mut request = Request::default();
        request.headers.insert("x-real-ip".to_string(), "127.0.0.1".to_string());

        assert!(guard.check(&request).is_ok());

        request.headers.insert("x-real-ip".to_string(), "192.168.1.1".to_string());
        assert!(guard.check(&request).is_err());
    }
}
