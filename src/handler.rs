//! Python handler registry and invocation.
//!
//! Manages Python function handlers with minimal GIL overhead.
//! Supports both synchronous `def` and asynchronous `async def` handlers.
//!
//! PERF: Caches handler metadata (is_async, DI requirements) at registration time
//! to avoid expensive Python introspection on every request.

use parking_lot::RwLock;
use pyo3::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::json::python_to_json;
use crate::request::Request;

/// Cached metadata for a handler to avoid per-request introspection.
struct HandlerMeta {
    /// The Python handler callable
    handler: PyObject,
    /// Whether this handler is async (returns coroutine)
    is_async: AtomicBool,
    /// Whether we've determined the async status yet
    async_checked: AtomicBool,
    /// Cached DI parameter names -> dependency names (empty if no DI needed)
    di_params: RwLock<Option<Vec<(String, String)>>>,
    /// Whether DI params have been resolved
    di_checked: AtomicBool,
}

/// Registry for Python handler functions.
#[derive(Clone)]
pub struct HandlerRegistry {
    /// Store handlers with cached metadata
    handlers: Arc<RwLock<Vec<Arc<HandlerMeta>>>>,
    /// PERF: Cached flag for whether any DI singletons exist (avoids lock per request)
    has_dependencies: Arc<AtomicBool>,
}

impl HandlerRegistry {
    /// Create a new empty handler registry.
    pub fn new() -> Self {
        HandlerRegistry {
            handlers: Arc::new(RwLock::new(Vec::new())),
            has_dependencies: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Register a Python handler function.
    ///
    /// # Returns
    /// The unique handler ID for this function.
    pub fn register(&mut self, handler: PyObject) -> usize {
        let meta = Arc::new(HandlerMeta {
            handler,
            is_async: AtomicBool::new(false),
            async_checked: AtomicBool::new(false),
            di_params: RwLock::new(None),
            di_checked: AtomicBool::new(false),
        });
        let mut handlers = self.handlers.write();
        let id = handlers.len();
        handlers.push(meta);
        id
    }

    /// Get a handler by its ID.
    #[inline]
    pub fn get(&self, id: usize) -> Option<PyObject> {
        let handlers = self.handlers.read();
        handlers.get(id).map(|m| m.handler.clone())
    }

    /// Get handler metadata by ID.
    #[inline]
    fn get_meta(&self, id: usize) -> Option<Arc<HandlerMeta>> {
        let handlers = self.handlers.read();
        handlers.get(id).cloned()
    }

    /// Notify that dependencies have been registered.
    pub fn set_has_dependencies(&self, has: bool) {
        self.has_dependencies.store(has, Ordering::Relaxed);
    }

    /// Invoke a handler with the given request (async-aware).
    ///
    /// PERF OPTIMIZATIONS:
    /// 1. Caches async detection per handler (avoid inspect.iscoroutine every call)
    /// 2. Caches DI parameter resolution per handler (avoid inspect.signature every call)
    /// 3. Skips DI entirely when no singletons registered (atomic check, no lock)
    /// 4. Single Python::with_gil call per request
    pub async fn invoke_async(
        &self,
        handler_id: usize,
        request: Request,
        dependency_container: Arc<crate::dependency::DependencyContainer>,
    ) -> Result<serde_json::Value, String> {
        let meta = self
            .get_meta(handler_id)
            .ok_or_else(|| format!("Handler {handler_id} not found"))?;

        // PERF: Fast atomic check instead of RwLock read on dependency container
        let has_dependencies = self.has_dependencies.load(Ordering::Relaxed)
            && dependency_container.has_py_singletons();

        Python::with_gil(|py| {
            let call_result = if has_dependencies {
                // DI resolution needed - but cache the parameter info
                if !meta.di_checked.load(Ordering::Relaxed) {
                    // First call: introspect and cache DI params
                    let mut di_params = Vec::new();
                    if let Ok(inspect) = py.import("inspect") {
                        if let Ok(sig) =
                            inspect.call_method1("signature", (meta.handler.as_ref(py),))
                        {
                            if let Ok(parameters) = sig.getattr("parameters") {
                                let cello_module = py.import("cello").ok();
                                let depends_type =
                                    cello_module.and_then(|m| m.getattr("Depends").ok());

                                if let Ok(items) = parameters.call_method0("items") {
                                    if let Ok(iter) = items.iter() {
                                        for item in iter.flatten() {
                                            if let (Ok(name), Ok(param)) = (
                                                item.get_item(0)
                                                    .and_then(|v| v.extract::<String>()),
                                                item.get_item(1),
                                            ) {
                                                if let Ok(default) = param.getattr("default") {
                                                    if let Some(dt) = &depends_type {
                                                        if default.is_instance(dt).unwrap_or(false)
                                                        {
                                                            if let Ok(dep_name) = default
                                                                .getattr("dependency")
                                                                .and_then(|d| d.extract::<String>())
                                                            {
                                                                di_params.push((name, dep_name));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    *meta.di_params.write() = Some(di_params);
                    meta.di_checked.store(true, Ordering::Relaxed);
                }

                // Use cached DI params
                let di_guard = meta.di_params.read();
                let di_params = match di_guard.as_ref() {
                    Some(params) => params,
                    None => {
                        // Fallback: DI params not yet cached (should not happen), use fast path
                        return meta
                            .handler
                            .call1(py, (request,))
                            .map_err(|e| format!("Handler error: {e}"))
                            .and_then(|result| python_to_json(py, result.as_ref(py)));
                    }
                };

                if di_params.is_empty() {
                    // No DI params found - fast path
                    meta.handler
                        .call1(py, (request,))
                        .map_err(|e| format!("Handler error: {e}"))?
                } else {
                    let kwargs = pyo3::types::PyDict::new(py);
                    for (param_name, dep_name) in di_params {
                        if let Some(dep_value) = dependency_container.get_py_singleton(dep_name) {
                            let _ = kwargs.set_item(param_name, dep_value);
                        }
                    }
                    meta.handler
                        .call(py, (request,), Some(kwargs))
                        .map_err(|e| format!("Handler error: {e}"))?
                }
            } else {
                // FAST PATH: Direct call without DI - no locks, no introspection
                meta.handler
                    .call1(py, (request,))
                    .map_err(|e| format!("Handler error: {e}"))?
            };

            // PERF: Cache async detection per handler
            let is_coroutine = if meta.async_checked.load(Ordering::Relaxed) {
                meta.is_async.load(Ordering::Relaxed)
            } else {
                // First call: detect and cache
                let is_async = py
                    .import("inspect")
                    .and_then(|inspect| {
                        inspect.call_method1("iscoroutine", (call_result.as_ref(py),))
                    })
                    .and_then(|r| r.is_true())
                    .unwrap_or(false);
                meta.is_async.store(is_async, Ordering::Relaxed);
                meta.async_checked.store(true, Ordering::Relaxed);
                is_async
            };

            let final_result = if is_coroutine {
                py.import("asyncio")
                    .and_then(|asyncio| asyncio.call_method1("run", (call_result.as_ref(py),)))
                    .map_err(|e| format!("Async handler error: {e}"))?
            } else {
                call_result.as_ref(py)
            };

            python_to_json(py, final_result)
        })
    }

    /// Invoke a handler synchronously (legacy method for compatibility).
    ///
    /// This acquires the GIL, calls the Python function, and returns
    /// the result as a JSON-serializable value.
    ///
    /// Note: This does NOT support async handlers. Use invoke_async instead.
    pub fn invoke(&self, handler_id: usize, request: Request) -> Result<serde_json::Value, String> {
        let handler = self
            .get(handler_id)
            .ok_or_else(|| format!("Handler {handler_id} not found"))?;

        Python::with_gil(|py| {
            // Call the Python handler with the request
            let result = handler
                .call1(py, (request,))
                .map_err(|e| format!("Handler error: {e}"))?;

            // Convert the result to a JSON value using SIMD-accelerated conversion
            python_to_json(py, result.as_ref(py))
        })
    }

    /// Get the number of registered handlers.
    pub fn len(&self) -> usize {
        self.handlers.read().len()
    }

    /// Check if there are no registered handlers.
    pub fn is_empty(&self) -> bool {
        self.handlers.read().is_empty()
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
