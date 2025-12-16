//! Hooks and lifecycle events for Cello.
//!
//! This module provides:
//! - Startup/shutdown hooks (async)
//! - Before/after request hooks
//! - Exception hooks
//! - Signal handlers (SIGTERM, SIGHUP, etc.)
//! - Worker lifecycle hooks for cluster mode

use parking_lot::RwLock;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::AppError;
use crate::request::Request;
use crate::response::Response;

/// Result of hook execution.
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Continue normal processing.
    Continue,
    /// Skip remaining hooks in the chain.
    Skip,
    /// Replace the response (short-circuit).
    Replace(Response),
}

/// Hook execution result type.
pub type HookResult = Result<HookAction, String>;

/// Async hook function type.
pub type AsyncHookFn = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + Sync,
>;

/// Sync hook function type.
pub type SyncHookFn = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

/// Request hook function type (before/after).
pub type RequestHookFn = Arc<dyn Fn(&mut Request) -> HookResult + Send + Sync>;

/// Response hook function type.
pub type ResponseHookFn = Arc<dyn Fn(&Request, &mut Response) -> HookResult + Send + Sync>;

/// Exception hook function type.
pub type ExceptionHookFn = Arc<dyn Fn(&AppError, &Request) + Send + Sync>;

/// Signal types supported for handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Signal {
    /// SIGTERM - termination signal.
    SIGTERM,
    /// SIGINT - interrupt signal (Ctrl+C).
    SIGINT,
    /// SIGHUP - hangup signal (reload config).
    SIGHUP,
    /// SIGUSR1 - user-defined signal 1.
    SIGUSR1,
    /// SIGUSR2 - user-defined signal 2.
    SIGUSR2,
}

impl Signal {
    /// Get the raw signal number.
    #[cfg(unix)]
    pub fn as_raw(&self) -> i32 {
        match self {
            Signal::SIGTERM => libc::SIGTERM,
            Signal::SIGINT => libc::SIGINT,
            Signal::SIGHUP => libc::SIGHUP,
            Signal::SIGUSR1 => libc::SIGUSR1,
            Signal::SIGUSR2 => libc::SIGUSR2,
        }
    }

    #[cfg(not(unix))]
    pub fn as_raw(&self) -> i32 {
        match self {
            Signal::SIGTERM => 15,
            Signal::SIGINT => 2,
            Signal::SIGHUP => 1,
            Signal::SIGUSR1 => 10,
            Signal::SIGUSR2 => 12,
        }
    }

    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "SIGTERM" | "TERM" => Some(Signal::SIGTERM),
            "SIGINT" | "INT" => Some(Signal::SIGINT),
            "SIGHUP" | "HUP" => Some(Signal::SIGHUP),
            "SIGUSR1" | "USR1" => Some(Signal::SIGUSR1),
            "SIGUSR2" | "USR2" => Some(Signal::SIGUSR2),
            _ => None,
        }
    }
}

/// Python hook wrapper for async execution.
pub struct PyHook {
    handler: PyObject,
    is_async: bool,
}

impl PyHook {
    pub fn new(handler: PyObject) -> Self {
        let is_async = Python::with_gil(|py| {
            let inspect = py.import("inspect").ok();
            inspect
                .and_then(|i| i.call_method1("iscoroutinefunction", (&handler,)).ok())
                .and_then(|r| r.is_true().ok())
                .unwrap_or(false)
        });
        Self { handler, is_async }
    }

    pub fn is_async(&self) -> bool {
        self.is_async
    }

    /// Execute the hook (sync or async).
    pub fn execute(&self) -> Result<(), String> {
        Python::with_gil(|py| {
            let result = self.handler.call0(py).map_err(|e| e.to_string())?;

            if self.is_async {
                // Run async hook with asyncio.run()
                let asyncio = py.import("asyncio").map_err(|e| e.to_string())?;
                asyncio
                    .call_method1("run", (result,))
                    .map_err(|e| e.to_string())?;
            }

            Ok(())
        })
    }

    /// Execute with request argument.
    pub fn execute_with_request(&self, request: &mut Request) -> HookResult {
        Python::with_gil(|py| {
            let result = self
                .handler
                .call1(py, (request.clone(),))
                .map_err(|e| e.to_string())?;

            if self.is_async {
                let asyncio = py.import("asyncio").map_err(|e| e.to_string())?;
                let awaited = asyncio
                    .call_method1("run", (result,))
                    .map_err(|e| e.to_string())?;

                // Check return value
                return Self::parse_hook_result(py, awaited);
            }

            Self::parse_hook_result(py, result.as_ref(py))
        })
    }

    /// Execute with request and response arguments.
    pub fn execute_with_response(&self, request: &Request, response: &mut Response) -> HookResult {
        Python::with_gil(|py| {
            let result = self
                .handler
                .call1(py, (request.clone(), response.clone()))
                .map_err(|e| e.to_string())?;

            if self.is_async {
                let asyncio = py.import("asyncio").map_err(|e| e.to_string())?;
                let awaited = asyncio
                    .call_method1("run", (result,))
                    .map_err(|e| e.to_string())?;

                return Self::parse_hook_result(py, awaited);
            }

            Self::parse_hook_result(py, result.as_ref(py))
        })
    }

    /// Parse hook result from Python return value.
    fn parse_hook_result(py: Python<'_>, result: &PyAny) -> HookResult {
        // None means continue
        if result.is_none() {
            return Ok(HookAction::Continue);
        }

        // Response means replace
        if let Ok(response) = result.extract::<Response>() {
            return Ok(HookAction::Replace(response));
        }

        // Bool: true = continue, false = skip
        if let Ok(b) = result.extract::<bool>() {
            return Ok(if b {
                HookAction::Continue
            } else {
                HookAction::Skip
            });
        }

        // String "skip" means skip
        if let Ok(s) = result.extract::<String>() {
            if s.to_lowercase() == "skip" {
                return Ok(HookAction::Skip);
            }
        }

        // Dict means JSON response
        if let Ok(_dict) = result.downcast::<pyo3::types::PyDict>() {
            let json_value = crate::json::python_to_json(py, result).map_err(|e| e)?;
            let body = serde_json::to_vec(&json_value).unwrap_or_default();
            let mut response = Response::new(200);
            response.set_body(body);
            response.set_header("Content-Type", "application/json");
            return Ok(HookAction::Replace(response));
        }

        Ok(HookAction::Continue)
    }
}

/// Lifecycle hooks container.
pub struct LifecycleHooks {
    /// Startup hooks (run before server starts accepting connections).
    on_startup: RwLock<Vec<Arc<PyHook>>>,
    /// Shutdown hooks (run after server stops accepting connections).
    on_shutdown: RwLock<Vec<Arc<PyHook>>>,
    /// Before request hooks (run before each request).
    before_request: RwLock<Vec<Arc<PyHook>>>,
    /// After request hooks (run after each request).
    after_request: RwLock<Vec<Arc<PyHook>>>,
    /// Exception hooks (run when an error occurs).
    on_exception: RwLock<Vec<Arc<PyHook>>>,
    /// Worker start hooks (for cluster mode).
    on_worker_start: RwLock<Vec<Arc<PyHook>>>,
    /// Worker shutdown hooks (for cluster mode).
    on_worker_shutdown: RwLock<Vec<Arc<PyHook>>>,
}

impl LifecycleHooks {
    /// Create a new lifecycle hooks container.
    pub fn new() -> Self {
        Self {
            on_startup: RwLock::new(Vec::new()),
            on_shutdown: RwLock::new(Vec::new()),
            before_request: RwLock::new(Vec::new()),
            after_request: RwLock::new(Vec::new()),
            on_exception: RwLock::new(Vec::new()),
            on_worker_start: RwLock::new(Vec::new()),
            on_worker_shutdown: RwLock::new(Vec::new()),
        }
    }

    /// Register a startup hook.
    pub fn add_startup_hook(&self, handler: PyObject) {
        self.on_startup.write().push(Arc::new(PyHook::new(handler)));
    }

    /// Register a shutdown hook.
    pub fn add_shutdown_hook(&self, handler: PyObject) {
        self.on_shutdown.write().push(Arc::new(PyHook::new(handler)));
    }

    /// Register a before request hook.
    pub fn add_before_request_hook(&self, handler: PyObject) {
        self.before_request.write().push(Arc::new(PyHook::new(handler)));
    }

    /// Register an after request hook.
    pub fn add_after_request_hook(&self, handler: PyObject) {
        self.after_request.write().push(Arc::new(PyHook::new(handler)));
    }

    /// Register an exception hook.
    pub fn add_exception_hook(&self, handler: PyObject) {
        self.on_exception.write().push(Arc::new(PyHook::new(handler)));
    }

    /// Register a worker start hook.
    pub fn add_worker_start_hook(&self, handler: PyObject) {
        self.on_worker_start.write().push(Arc::new(PyHook::new(handler)));
    }

    /// Register a worker shutdown hook.
    pub fn add_worker_shutdown_hook(&self, handler: PyObject) {
        self.on_worker_shutdown.write().push(Arc::new(PyHook::new(handler)));
    }

    /// Execute all startup hooks.
    pub fn execute_startup(&self) -> Result<(), String> {
        for hook in self.on_startup.read().iter() {
            hook.execute()?;
        }
        Ok(())
    }

    /// Execute all shutdown hooks.
    pub fn execute_shutdown(&self) -> Result<(), String> {
        for hook in self.on_shutdown.read().iter() {
            if let Err(e) = hook.execute() {
                // Log but don't fail on shutdown hook errors
                eprintln!("Shutdown hook error: {}", e);
            }
        }
        Ok(())
    }

    /// Execute before request hooks.
    pub fn execute_before_request(&self, request: &mut Request) -> HookResult {
        for hook in self.before_request.read().iter() {
            match hook.execute_with_request(request)? {
                HookAction::Continue => continue,
                HookAction::Skip => return Ok(HookAction::Skip),
                HookAction::Replace(response) => return Ok(HookAction::Replace(response)),
            }
        }
        Ok(HookAction::Continue)
    }

    /// Execute after request hooks.
    pub fn execute_after_request(&self, request: &Request, response: &mut Response) -> HookResult {
        // Execute in reverse order for proper layering
        for hook in self.after_request.read().iter().rev() {
            match hook.execute_with_response(request, response)? {
                HookAction::Continue => continue,
                HookAction::Skip => return Ok(HookAction::Skip),
                HookAction::Replace(new_response) => {
                    *response = new_response;
                    return Ok(HookAction::Continue);
                }
            }
        }
        Ok(HookAction::Continue)
    }

    /// Execute exception hooks.
    pub fn execute_exception(&self, error: &AppError, request: &Request) {
        for hook in self.on_exception.read().iter() {
            Python::with_gil(|py| {
                let error_dict = pyo3::types::PyDict::new(py);
                let _ = error_dict.set_item("message", error.to_string());
                let _ = error_dict.set_item("status", error.status_code());

                let _ = hook.handler.call1(py, (error_dict, request.clone()));
            });
        }
    }

    /// Execute worker start hooks.
    pub fn execute_worker_start(&self, worker_id: usize) -> Result<(), String> {
        for hook in self.on_worker_start.read().iter() {
            Python::with_gil(|py| {
                hook.handler
                    .call1(py, (worker_id,))
                    .map_err(|e| e.to_string())
            })?;
        }
        Ok(())
    }

    /// Execute worker shutdown hooks.
    pub fn execute_worker_shutdown(&self, worker_id: usize) -> Result<(), String> {
        for hook in self.on_worker_shutdown.read().iter() {
            Python::with_gil(|py| {
                let _ = hook.handler.call1(py, (worker_id,));
            });
        }
        Ok(())
    }
}

impl Default for LifecycleHooks {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for LifecycleHooks {
    fn clone(&self) -> Self {
        Self {
            on_startup: RwLock::new(self.on_startup.read().clone()),
            on_shutdown: RwLock::new(self.on_shutdown.read().clone()),
            before_request: RwLock::new(self.before_request.read().clone()),
            after_request: RwLock::new(self.after_request.read().clone()),
            on_exception: RwLock::new(self.on_exception.read().clone()),
            on_worker_start: RwLock::new(self.on_worker_start.read().clone()),
            on_worker_shutdown: RwLock::new(self.on_worker_shutdown.read().clone()),
        }
    }
}

/// Signal handler registry.
pub struct SignalHandlers {
    handlers: RwLock<HashMap<Signal, Vec<Arc<PyHook>>>>,
}

impl SignalHandlers {
    /// Create a new signal handler registry.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a signal handler.
    pub fn register(&self, signal: Signal, handler: PyObject) {
        self.handlers
            .write()
            .entry(signal)
            .or_default()
            .push(Arc::new(PyHook::new(handler)));
    }

    /// Execute handlers for a signal.
    pub fn execute(&self, signal: Signal) {
        if let Some(handlers) = self.handlers.read().get(&signal) {
            for hook in handlers {
                if let Err(e) = hook.execute() {
                    eprintln!("Signal handler error: {}", e);
                }
            }
        }
    }

    /// Check if any handlers are registered for a signal.
    pub fn has_handlers(&self, signal: Signal) -> bool {
        self.handlers
            .read()
            .get(&signal)
            .map(|h| !h.is_empty())
            .unwrap_or(false)
    }
}

impl Default for SignalHandlers {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SignalHandlers {
    fn clone(&self) -> Self {
        Self {
            handlers: RwLock::new(self.handlers.read().clone()),
        }
    }
}

/// Route-specific hook overrides.
#[derive(Clone, Default)]
pub struct RouteHooks {
    /// Before request hooks for this route.
    pub before: Option<Vec<Arc<PyHook>>>,
    /// After request hooks for this route.
    pub after: Option<Vec<Arc<PyHook>>>,
    /// Skip global hooks.
    pub skip_global: bool,
}

impl RouteHooks {
    /// Create new route hooks with skip_global option.
    pub fn new(skip_global: bool) -> Self {
        Self {
            before: None,
            after: None,
            skip_global,
        }
    }

    /// Add a before hook.
    pub fn add_before(&mut self, handler: PyObject) {
        self.before
            .get_or_insert_with(Vec::new)
            .push(Arc::new(PyHook::new(handler)));
    }

    /// Add an after hook.
    pub fn add_after(&mut self, handler: PyObject) {
        self.after
            .get_or_insert_with(Vec::new)
            .push(Arc::new(PyHook::new(handler)));
    }
}

/// Python-exposed lifecycle hooks.
#[pyclass]
pub struct PyLifecycleHooks {
    inner: Arc<LifecycleHooks>,
    signals: Arc<SignalHandlers>,
}

#[pymethods]
impl PyLifecycleHooks {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(LifecycleHooks::new()),
            signals: Arc::new(SignalHandlers::new()),
        }
    }

    /// Register a startup hook.
    pub fn on_startup(&self, handler: PyObject) {
        self.inner.add_startup_hook(handler);
    }

    /// Register a shutdown hook.
    pub fn on_shutdown(&self, handler: PyObject) {
        self.inner.add_shutdown_hook(handler);
    }

    /// Register a before request hook.
    pub fn before_request(&self, handler: PyObject) {
        self.inner.add_before_request_hook(handler);
    }

    /// Register an after request hook.
    pub fn after_request(&self, handler: PyObject) {
        self.inner.add_after_request_hook(handler);
    }

    /// Register an exception hook.
    pub fn on_exception(&self, handler: PyObject) {
        self.inner.add_exception_hook(handler);
    }

    /// Register a signal handler.
    pub fn on_signal(&self, signal: &str, handler: PyObject) -> PyResult<()> {
        let sig = Signal::from_str(signal)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(format!("Unknown signal: {}", signal)))?;
        self.signals.register(sig, handler);
        Ok(())
    }

    /// Register a worker start hook.
    pub fn on_worker_start(&self, handler: PyObject) {
        self.inner.add_worker_start_hook(handler);
    }

    /// Register a worker shutdown hook.
    pub fn on_worker_shutdown(&self, handler: PyObject) {
        self.inner.add_worker_shutdown_hook(handler);
    }
}

impl Default for PyLifecycleHooks {
    fn default() -> Self {
        Self::new()
    }
}

impl PyLifecycleHooks {
    /// Get the inner lifecycle hooks.
    pub fn hooks(&self) -> &Arc<LifecycleHooks> {
        &self.inner
    }

    /// Get the signal handlers.
    pub fn signal_handlers(&self) -> &Arc<SignalHandlers> {
        &self.signals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_from_str() {
        assert_eq!(Signal::from_str("SIGTERM"), Some(Signal::SIGTERM));
        assert_eq!(Signal::from_str("TERM"), Some(Signal::SIGTERM));
        assert_eq!(Signal::from_str("HUP"), Some(Signal::SIGHUP));
        assert_eq!(Signal::from_str("invalid"), None);
    }

    #[test]
    fn test_signal_raw() {
        #[cfg(unix)]
        {
            assert_eq!(Signal::SIGTERM.as_raw(), libc::SIGTERM);
            assert_eq!(Signal::SIGINT.as_raw(), libc::SIGINT);
        }
    }
}
