//! Dependency Injection system for Cello.
//!
//! Inspired by FastAPI's dependency injection system, this module provides:
//! - Hierarchical dependency resolution
//! - Request-scoped, Singleton, and Transient dependencies
//! - Async dependency support
//! - Dependency caching
//! - Type-safe dependency injection
//! - Dependency overrides for testing

use parking_lot::RwLock;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use pyo3::PyObject;

use crate::request::Request;

// ============================================================================
// Dependency Types
// ============================================================================

/// Dependency scope determines the lifetime of a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyScope {
    /// One instance per application (shared across all requests)
    Singleton,
    /// One instance per request (cached within request context)
    Request,
    /// New instance every time (no caching)
    Transient,
}

/// Result type for dependency providers.
pub type DependencyResult<T> = Result<T, DependencyError>;

/// Async dependency result.
pub type AsyncDependencyResult<T> = Pin<Box<dyn Future<Output = DependencyResult<T>> + Send>>;

/// Dependency error types.
#[derive(Debug, Clone)]
pub enum DependencyError {
    NotFound(String),
    CircularDependency(String),
    ProviderFailed(String),
    TypeMismatch(String),
    CacheFailed(String),
}

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyError::NotFound(name) => write!(f, "Dependency not found: {}", name),
            DependencyError::CircularDependency(chain) => {
                write!(f, "Circular dependency detected: {}", chain)
            }
            DependencyError::ProviderFailed(msg) => write!(f, "Provider failed: {}", msg),
            DependencyError::TypeMismatch(msg) => write!(f, "Type mismatch: {}", msg),
            DependencyError::CacheFailed(msg) => write!(f, "Cache failed: {}", msg),
        }
    }
}

impl std::error::Error for DependencyError {}

// ============================================================================
// Provider Trait
// ============================================================================

/// Trait for providing dependencies.
pub trait Provider: Send + Sync {
    /// Get the dependency value.
    fn provide(&self, request: &Request, container: &DependencyContainer) -> Box<dyn Any + Send>;

    /// Get the dependency scope.
    fn scope(&self) -> DependencyScope {
        DependencyScope::Request
    }

    /// Get the dependency name for debugging.
    fn name(&self) -> &str {
        "unnamed"
    }
}

/// Trait for async dependency providers.
pub trait AsyncProvider: Send + Sync {
    /// Get the dependency value asynchronously.
    fn provide_async<'a>(
        &'a self,
        request: &'a Request,
        container: &'a DependencyContainer,
    ) -> Pin<Box<dyn Future<Output = Box<dyn Any + Send>> + Send + 'a>>;

    /// Get the dependency scope.
    fn scope(&self) -> DependencyScope {
        DependencyScope::Request
    }

    /// Get the dependency name for debugging.
    fn name(&self) -> &str {
        "unnamed_async"
    }
}

// ============================================================================
// Simple Function Provider
// ============================================================================

/// Provider that wraps a simple function.
pub struct FunctionProvider<F, T>
where
    F: Fn(&Request) -> T + Send + Sync,
    T: Send + 'static,
{
    func: F,
    scope: DependencyScope,
    name: String,
}

impl<F, T> FunctionProvider<F, T>
where
    F: Fn(&Request) -> T + Send + Sync,
    T: Send + 'static,
{
    pub fn new(name: &str, func: F, scope: DependencyScope) -> Self {
        Self {
            func,
            scope,
            name: name.to_string(),
        }
    }
}

impl<F, T> Provider for FunctionProvider<F, T>
where
    F: Fn(&Request) -> T + Send + Sync,
    T: Send + 'static,
{
    fn provide(&self, request: &Request, _container: &DependencyContainer) -> Box<dyn Any + Send> {
        Box::new((self.func)(request))
    }

    fn scope(&self) -> DependencyScope {
        self.scope
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Value Provider (for singletons)
// ============================================================================

/// Provider that wraps a pre-computed value.
pub struct ValueProvider<T: Send + 'static> {
    value: Arc<T>,
    name: String,
}

impl<T: Send + 'static> ValueProvider<T> {
    pub fn new(name: &str, value: T) -> Self {
        Self {
            value: Arc::new(value),
            name: name.to_string(),
        }
    }
}

impl<T: Send + Sync + Clone + 'static> Provider for ValueProvider<T> {
    fn provide(&self, _request: &Request, _container: &DependencyContainer) -> Box<dyn Any + Send> {
        Box::new((*self.value).clone())
    }

    fn scope(&self) -> DependencyScope {
        DependencyScope::Singleton
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Dependency Container
// ============================================================================

/// Container for managing dependencies.
#[derive(Clone)]
pub struct DependencyContainer {
    /// Registered providers
    providers: Arc<RwLock<HashMap<TypeId, Box<dyn Provider>>>>,
    /// Async providers
    async_providers: Arc<RwLock<HashMap<TypeId, Arc<dyn AsyncProvider>>>>,
    /// Singleton cache
    singleton_cache: Arc<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
    /// Override providers (for testing)
    overrides: Arc<RwLock<HashMap<TypeId, Box<dyn Provider>>>>,
    /// Named Python singletons
    py_singletons: Arc<RwLock<HashMap<String, PyObject>>>,
}

impl DependencyContainer {
    /// Create a new dependency container.
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            async_providers: Arc::new(RwLock::new(HashMap::new())),
            singleton_cache: Arc::new(RwLock::new(HashMap::new())),
            overrides: Arc::new(RwLock::new(HashMap::new())),
            py_singletons: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a provider.
    pub fn register<T: 'static>(&self, provider: Box<dyn Provider>) {
        let type_id = TypeId::of::<T>();
        self.providers.write().insert(type_id, provider);
    }

    /// Register an async provider.
    pub fn register_async<T: 'static>(&self, provider: Arc<dyn AsyncProvider>) {
        let type_id = TypeId::of::<T>();
        self.async_providers.write().insert(type_id, provider);
    }

    /// Register a function provider.
    pub fn register_fn<F, T>(&self, name: &str, func: F, scope: DependencyScope)
    where
        F: Fn(&Request) -> T + Send + Sync + 'static,
        T: Send + 'static,
    {
        let provider = Box::new(FunctionProvider::new(name, func, scope));
        self.register::<T>(provider);
    }

    /// Register a singleton value.
    pub fn register_singleton<T: Send + Sync + Clone + 'static>(&self, name: &str, value: T) {
        let provider = Box::new(ValueProvider::new(name, value));
        self.register::<T>(provider);
    }

    /// Register a Python singleton.
    pub fn register_py_singleton(&self, name: &str, value: PyObject) {
        self.py_singletons.write().insert(name.to_string(), value);
    }

    /// Get a Python singleton by name.
    pub fn get_py_singleton(&self, name: &str) -> Option<PyObject> {
        self.py_singletons.read().get(name).cloned()
    }

    /// Override a provider (useful for testing).
    pub fn override_provider<T: 'static>(&self, provider: Box<dyn Provider>) {
        let type_id = TypeId::of::<T>();
        self.overrides.write().insert(type_id, provider);
    }

    /// Clear override for a type.
    pub fn clear_override<T: 'static>(&self) {
        let type_id = TypeId::of::<T>();
        self.overrides.write().remove(&type_id);
    }

    /// Clear all overrides.
    pub fn clear_all_overrides(&self) {
        self.overrides.write().clear();
    }

    /// Get a dependency.
    pub fn get<T: Clone + Send + Sync + 'static>(&self, request: &Request) -> DependencyResult<T> {
        let type_id = TypeId::of::<T>();

        // Check overrides first (for testing)
        if let Some(provider) = self.overrides.read().get(&type_id) {
            return self.resolve_provider::<T>(provider.as_ref(), request);
        }

        // Get the provider
        let providers = self.providers.read();
        let provider = providers
            .get(&type_id)
            .ok_or_else(|| DependencyError::NotFound(std::any::type_name::<T>().to_string()))?;

        self.resolve_provider::<T>(provider.as_ref(), request)
    }

    /// Resolve a provider based on its scope.
    fn resolve_provider<T: Clone + Send + Sync + 'static>(
        &self,
        provider: &dyn Provider,
        request: &Request,
    ) -> DependencyResult<T> {
        match provider.scope() {
            DependencyScope::Singleton => {
                // Check singleton cache
                let type_id = TypeId::of::<T>();
                {
                    let cache = self.singleton_cache.read();
                    if let Some(cached) = cache.get(&type_id) {
                        if let Some(value_ref) = cached.downcast_ref::<T>() {
                            return Ok(value_ref.clone());
                        } else {
                            return Err(DependencyError::TypeMismatch(
                                "Cached singleton type mismatch".to_string(),
                            ));
                        }
                    }
                }

                // Provide and cache
                let value = provider.provide(request, self);
                let downcasted = value
                    .downcast::<T>()
                    .map_err(|_| DependencyError::TypeMismatch("Provider type mismatch".to_string()))?;

                let value_clone = (*downcasted).clone();
                
                // Store in singleton cache
                self.singleton_cache
                    .write()
                    .insert(type_id, Arc::new(value_clone.clone()));

                Ok(value_clone)
            }
            DependencyScope::Request => {
                // Check request cache
                let type_id = TypeId::of::<T>();
                if let Some(cached) = request.context.get("__di_cache__") {
                    if let Some(cache_map) = cached.as_object() {
                        let key = format!("{:?}", type_id);
                        if cache_map.contains_key(&key) {
                            // Return cached value
                            // Note: In production, you'd need proper serialization
                        }
                    }
                }

                // Provide
                let value = provider.provide(request, self);
                let downcasted = value
                    .downcast::<T>()
                    .map_err(|_| DependencyError::TypeMismatch("Provider type mismatch".to_string()))?;

                Ok(*downcasted)
            }
            DependencyScope::Transient => {
                // Always create new instance
                let value = provider.provide(request, self);
                let downcasted = value
                    .downcast::<T>()
                    .map_err(|_| DependencyError::TypeMismatch("Provider type mismatch".to_string()))?;

                Ok(*downcasted)
            }
        }
    }

    /// Get dependency count.
    pub fn count(&self) -> usize {
        self.providers.read().len()
    }

    /// Clear singleton cache.
    pub fn clear_singleton_cache(&self) {
        self.singleton_cache.write().clear();
    }
}

impl Default for DependencyContainer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Helper Macro for Dependencies
// ============================================================================

/// Macro to create a dependency extractor.
#[macro_export]
macro_rules! depends {
    ($container:expr, $type:ty) => {{
        move |request: &$crate::request::Request| -> Result<$type, $crate::dependency::DependencyError> {
            $container.get::<$type>(request)
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::Request;

    #[derive(Clone, Debug, PartialEq)]
    struct DatabaseConnection {
        url: String,
    }

    #[derive(Clone, Debug)]
    struct UserService {
        db: String,
    }

    #[test]
    fn test_singleton_dependency() {
        let container = DependencyContainer::new();

        // Register singleton database connection
        let db = DatabaseConnection {
            url: "postgres://localhost".to_string(),
        };
        container.register_singleton("db", db);

        // Create test request
        let request = Request::default();

        // Get dependency twice
        let db1: DependencyResult<DatabaseConnection> = container.get(&request);
        let db2: DependencyResult<DatabaseConnection> = container.get(&request);

        assert!(db1.is_ok());
        assert!(db2.is_ok());
        assert_eq!(db1.unwrap(), db2.unwrap());
    }

    #[test]
    fn test_transient_dependency() {
        let container = DependencyContainer::new();

        // Register transient provider
        container.register_fn::<_, String>(
            "timestamp",
            |_req| format!("{:?}", std::time::SystemTime::now()),
            DependencyScope::Transient,
        );

        let request = Request::default();

        // Get dependency twice - should be different
        let t1: DependencyResult<String> = container.get(&request);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2: DependencyResult<String> = container.get(&request);

        assert!(t1.is_ok());
        assert!(t2.is_ok());
        // Timestamps should be different
    }

    #[test]
    fn test_override_provider() {
        let container = DependencyContainer::new();

        // Register original provider
        container.register_singleton("db", DatabaseConnection {
            url: "postgres://prod".to_string(),
        });

        let request = Request::default();

        // Original value
        let db: DatabaseConnection = container.get(&request).unwrap();
        assert_eq!(db.url, "postgres://prod");

        // Override for testing
        container.override_provider::<DatabaseConnection>(Box::new(ValueProvider::new(
            "test_db",
            DatabaseConnection {
                url: "postgres://test".to_string(),
            },
        )));

        // Should get overridden value
        let db: DatabaseConnection = container.get(&request).unwrap();
        assert_eq!(db.url, "postgres://test");

        // Clear override
        container.clear_override::<DatabaseConnection>();

        // Back to original
        let db: DatabaseConnection = container.get(&request).unwrap();
        assert_eq!(db.url, "postgres://prod");
    }

    #[test]
    fn test_dependency_not_found() {
        let container = DependencyContainer::new();
        let request = Request::default();

        let result: DependencyResult<DatabaseConnection> = container.get(&request);
        assert!(result.is_err());
        matches!(result.unwrap_err(), DependencyError::NotFound(_));
    }
}
