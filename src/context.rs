//! Request context and dependency injection for Cello.
//!
//! This module provides:
//! - Request-scoped state storage
//! - Application-wide singleton management
//! - Dependency injection container
//! - Task-local context for async handlers

use parking_lot::RwLock;
use pyo3::prelude::*;
use serde_json::Value as JsonValue;
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Request-scoped context for storing arbitrary data.
///
/// This context is created for each request and can be used to:
/// - Pass data from middleware to handlers
/// - Store request-specific state
/// - Inject dependencies
#[derive(Default)]
pub struct RequestContext {
    /// Type-erased storage for Rust types
    typed_data: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
    /// String-keyed storage for Python interop
    named_data: HashMap<String, JsonValue>,
}

impl RequestContext {
    /// Create a new empty request context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a typed value from the context.
    pub fn get<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.typed_data
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }

    /// Get a mutable typed value from the context.
    pub fn get_mut<T: 'static + Send + Sync>(&mut self) -> Option<&mut T> {
        self.typed_data
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut::<T>())
    }

    /// Set a typed value in the context.
    pub fn set<T: 'static + Send + Sync>(&mut self, value: T) {
        self.typed_data.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Remove a typed value from the context.
    pub fn remove<T: 'static + Send + Sync>(&mut self) -> Option<T> {
        self.typed_data
            .remove(&TypeId::of::<T>())
            .and_then(|v| v.downcast::<T>().ok())
            .map(|v| *v)
    }

    /// Check if a typed value exists.
    pub fn contains<T: 'static + Send + Sync>(&self) -> bool {
        self.typed_data.contains_key(&TypeId::of::<T>())
    }

    /// Get a named value (for Python interop).
    pub fn get_named(&self, key: &str) -> Option<&JsonValue> {
        self.named_data.get(key)
    }

    /// Set a named value (for Python interop).
    pub fn set_named(&mut self, key: impl Into<String>, value: JsonValue) {
        self.named_data.insert(key.into(), value);
    }

    /// Remove a named value.
    pub fn remove_named(&mut self, key: &str) -> Option<JsonValue> {
        self.named_data.remove(key)
    }

    /// Check if a named value exists.
    pub fn contains_named(&self, key: &str) -> bool {
        self.named_data.contains_key(key)
    }

    /// Get all named keys.
    pub fn named_keys(&self) -> impl Iterator<Item = &String> {
        self.named_data.keys()
    }

    /// Clear all context data.
    pub fn clear(&mut self) {
        self.typed_data.clear();
        self.named_data.clear();
    }
}

impl Clone for RequestContext {
    fn clone(&self) -> Self {
        // Only clone named data; typed data is not cloneable
        Self {
            typed_data: HashMap::new(),
            named_data: self.named_data.clone(),
        }
    }
}

/// Python-exposed context wrapper.
#[pyclass]
#[derive(Clone)]
pub struct PyContext {
    inner: Arc<RwLock<RequestContext>>,
}

#[pymethods]
impl PyContext {
    /// Create a new context.
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RequestContext::new())),
        }
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<PyObject> {
        let ctx = self.inner.read();
        ctx.get_named(key).map(|v| {
            Python::with_gil(|py| crate::json::json_to_python(py, v).unwrap_or_else(|_| py.None()))
        })
    }

    /// Set a value by key.
    pub fn set(&self, py: Python<'_>, key: &str, value: &PyAny) -> PyResult<()> {
        let json_value = crate::json::python_to_json(py, value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))?;
        self.inner.write().set_named(key, json_value);
        Ok(())
    }

    /// Remove a value by key.
    pub fn remove(&self, key: &str) -> Option<PyObject> {
        let mut ctx = self.inner.write();
        ctx.remove_named(key).map(|v| {
            Python::with_gil(|py| crate::json::json_to_python(py, &v).unwrap_or_else(|_| py.None()))
        })
    }

    /// Check if a key exists.
    pub fn contains(&self, key: &str) -> bool {
        self.inner.read().contains_named(key)
    }

    /// Get all keys.
    pub fn keys(&self) -> Vec<String> {
        self.inner.read().named_keys().cloned().collect()
    }

    /// Clear all values.
    pub fn clear(&self) {
        self.inner.write().clear();
    }

    fn __getitem__(&self, key: &str) -> PyResult<PyObject> {
        self.get(key)
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }

    fn __setitem__(&self, py: Python<'_>, key: &str, value: &PyAny) -> PyResult<()> {
        self.set(py, key, value)
    }

    fn __delitem__(&self, key: &str) -> PyResult<()> {
        self.inner.write().remove_named(key);
        Ok(())
    }

    fn __contains__(&self, key: &str) -> bool {
        self.contains(key)
    }

    fn __len__(&self) -> usize {
        self.inner.read().named_data.len()
    }
}

impl Default for PyContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Application-wide state container for singletons and factories.
pub struct AppState {
    /// Singleton services (created once, shared across requests)
    singletons: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    /// Named singletons for Python interop
    named_singletons: RwLock<HashMap<String, Arc<dyn Any + Send + Sync>>>,
    /// Configuration values
    config: RwLock<HashMap<String, JsonValue>>,
}

impl AppState {
    /// Create a new application state.
    pub fn new() -> Self {
        Self {
            singletons: RwLock::new(HashMap::new()),
            named_singletons: RwLock::new(HashMap::new()),
            config: RwLock::new(HashMap::new()),
        }
    }

    /// Register a singleton service.
    pub fn register_singleton<T: 'static + Send + Sync>(&self, instance: T) {
        self.singletons
            .write()
            .insert(TypeId::of::<T>(), Arc::new(instance));
    }

    /// Get a singleton service.
    pub fn get_singleton<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        self.singletons
            .read()
            .get(&TypeId::of::<T>())
            .and_then(|v| v.clone().downcast::<T>().ok())
    }

    /// Register a named singleton (for Python).
    pub fn register_named_singleton(&self, name: impl Into<String>, instance: Arc<dyn Any + Send + Sync>) {
        self.named_singletons.write().insert(name.into(), instance);
    }

    /// Get a named singleton.
    pub fn get_named_singleton(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.named_singletons.read().get(name).cloned()
    }

    /// Set a configuration value.
    pub fn set_config(&self, key: impl Into<String>, value: JsonValue) {
        self.config.write().insert(key.into(), value);
    }

    /// Get a configuration value.
    pub fn get_config(&self, key: &str) -> Option<JsonValue> {
        self.config.read().get(key).cloned()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Dependency injection container.
///
/// Combines application state with request context for resolving dependencies.
pub struct Container {
    /// Application-level state (singletons)
    app_state: Arc<AppState>,
    /// Request-level context
    request_context: RefCell<RequestContext>,
}

impl Container {
    /// Create a new container with the given app state.
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self {
            app_state,
            request_context: RefCell::new(RequestContext::new()),
        }
    }

    /// Get the application state.
    pub fn app_state(&self) -> &Arc<AppState> {
        &self.app_state
    }

    /// Get a reference to the request context.
    pub fn context(&self) -> std::cell::Ref<'_, RequestContext> {
        self.request_context.borrow()
    }

    /// Get a mutable reference to the request context.
    pub fn context_mut(&self) -> std::cell::RefMut<'_, RequestContext> {
        self.request_context.borrow_mut()
    }

    /// Resolve a dependency by type.
    ///
    /// Resolution order:
    /// 1. Request context (request-scoped)
    /// 2. App state singletons (application-scoped)
    pub fn resolve<T: 'static + Send + Sync + Clone>(&self) -> Option<T> {
        // First check request context
        if let Some(value) = self.request_context.borrow().get::<T>() {
            return Some(value.clone());
        }

        // Then check app state singletons
        if let Some(arc) = self.app_state.get_singleton::<T>() {
            return Some((*arc).clone());
        }

        None
    }

    /// Set a request-scoped value.
    pub fn set<T: 'static + Send + Sync>(&self, value: T) {
        self.request_context.borrow_mut().set(value);
    }
}

// Task-local storage for async context access
tokio::task_local! {
    /// Current request context for async handlers.
    pub static CURRENT_CONTEXT: RefCell<RequestContext>;
}

/// Run a future with the given request context.
pub async fn with_context<F, T>(context: RequestContext, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    CURRENT_CONTEXT.scope(RefCell::new(context), f).await
}

/// Get a value from the current task-local context.
pub fn current_context_get<T: 'static + Send + Sync + Clone>() -> Option<T> {
    CURRENT_CONTEXT
        .try_with(|ctx| ctx.borrow().get::<T>().cloned())
        .ok()
        .flatten()
}

/// Set a value in the current task-local context.
pub fn current_context_set<T: 'static + Send + Sync>(value: T) {
    let _ = CURRENT_CONTEXT.try_with(|ctx| {
        ctx.borrow_mut().set(value);
    });
}

/// Get a named value from the current task-local context.
pub fn current_context_get_named(key: &str) -> Option<JsonValue> {
    CURRENT_CONTEXT
        .try_with(|ctx| ctx.borrow().get_named(key).cloned())
        .ok()
        .flatten()
}

/// Set a named value in the current task-local context.
pub fn current_context_set_named(key: impl Into<String>, value: JsonValue) {
    let _ = CURRENT_CONTEXT.try_with(|ctx| {
        ctx.borrow_mut().set_named(key, value);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_typed() {
        let mut ctx = RequestContext::new();
        ctx.set(42i32);
        ctx.set("hello".to_string());

        assert_eq!(ctx.get::<i32>(), Some(&42));
        assert_eq!(ctx.get::<String>(), Some(&"hello".to_string()));
        assert_eq!(ctx.get::<f64>(), None);
    }

    #[test]
    fn test_request_context_named() {
        let mut ctx = RequestContext::new();
        ctx.set_named("user_id", serde_json::json!(123));
        ctx.set_named("name", serde_json::json!("John"));

        assert_eq!(ctx.get_named("user_id"), Some(&serde_json::json!(123)));
        assert_eq!(ctx.get_named("name"), Some(&serde_json::json!("John")));
        assert_eq!(ctx.get_named("missing"), None);
    }

    #[test]
    fn test_app_state_singleton() {
        let state = AppState::new();
        state.register_singleton(42i32);

        let value = state.get_singleton::<i32>();
        assert_eq!(value.map(|v| *v), Some(42));
    }

    #[test]
    fn test_container_resolution() {
        let state = Arc::new(AppState::new());
        state.register_singleton(100i32);

        let container = Container::new(state);
        container.set(42i32); // Request-scoped overrides app-scoped

        // Request-scoped takes precedence
        assert_eq!(container.resolve::<i32>(), Some(42));
    }
}
