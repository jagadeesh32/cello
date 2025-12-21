//! Python handler registry and invocation.
//!
//! Manages Python function handlers with minimal GIL overhead.
//! Supports both synchronous `def` and asynchronous `async def` handlers.

use parking_lot::RwLock;
use pyo3::prelude::*;
use std::sync::Arc;

use crate::request::Request;
use crate::json::python_to_json;

/// Registry for Python handler functions.
#[derive(Clone)]
pub struct HandlerRegistry {
    /// Store handlers as PyObject since PyFunction is not available in abi3 mode
    handlers: Arc<RwLock<Vec<PyObject>>>,
}

impl HandlerRegistry {
    /// Create a new empty handler registry.
    pub fn new() -> Self {
        HandlerRegistry {
            handlers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a Python handler function.
    ///
    /// # Returns
    /// The unique handler ID for this function.
    pub fn register(&mut self, handler: PyObject) -> usize {
        let mut handlers = self.handlers.write();
        let id = handlers.len();
        handlers.push(handler);
        id
    }

    /// Get a handler by its ID.
    pub fn get(&self, id: usize) -> Option<PyObject> {
        let handlers = self.handlers.read();
        handlers.get(id).cloned()
    }

    /// Invoke a handler with the given request (async-aware).
    ///
    /// This method automatically detects if the handler returns a coroutine
    /// (async def) or a regular value (def), and handles both appropriately.
    ///
    /// OPTIMIZED: Skips expensive DI introspection when no dependencies exist.
    pub async fn invoke_async(
        &self,
        handler_id: usize,
        request: Request,
        dependency_container: Arc<crate::dependency::DependencyContainer>,
    ) -> Result<serde_json::Value, String> {
        let handler = self
            .get(handler_id)
            .ok_or_else(|| format!("Handler {} not found", handler_id))?;

        // FAST PATH: Skip expensive DI introspection if no dependencies registered
        let has_dependencies = dependency_container.has_py_singletons();

        Python::with_gil(|py| {
            let call_result = if has_dependencies {
                // SLOW PATH: DI resolution needed
                let inspect = py.import("inspect")
                    .map_err(|e| format!("Failed to import inspect: {}", e))?;
                
                let sig = inspect.call_method1("signature", (handler.as_ref(py),))
                    .map_err(|e| format!("Failed to get signature: {}", e))?;
                
                let parameters = sig.getattr("parameters")
                    .map_err(|e| format!("Failed to get parameters: {}", e))?;
                
                let kwargs = pyo3::types::PyDict::new(py);
                let cello_module = py.import("cello").ok();
                let depends_type = cello_module.and_then(|m| m.getattr("Depends").ok());

                if let Ok(items) = parameters.call_method0("items") {
                    if let Ok(iter) = items.iter() {
                        for item in iter.flatten() {
                            if let (Ok(name), Ok(param)) = (
                                item.get_item(0).and_then(|v| v.extract::<String>()),
                                item.get_item(1)
                            ) {
                                if let Ok(default) = param.getattr("default") {
                                    if let Some(dt) = &depends_type {
                                        if default.is_instance(dt).unwrap_or(false) {
                                            if let Ok(dep_name) = default.getattr("dependency")
                                                .and_then(|d| d.extract::<String>())
                                            {
                                                if let Some(dep_value) = dependency_container.get_py_singleton(&dep_name) {
                                                    let _ = kwargs.set_item(name, dep_value);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                handler.call(py, (request,), Some(kwargs))
                    .map_err(|e| format!("Handler error: {}", e))?
            } else {
                // FAST PATH: Direct call without DI
                handler.call1(py, (request,))
                    .map_err(|e| format!("Handler error: {}", e))?
            };

            // Check if async
            let is_coroutine = py.import("inspect")
                .and_then(|inspect| inspect.call_method1("iscoroutine", (call_result.as_ref(py),)))
                .and_then(|r| r.is_true())
                .unwrap_or(false);

            let final_result = if is_coroutine {
                py.import("asyncio")
                    .and_then(|asyncio| asyncio.call_method1("run", (call_result.as_ref(py),)))
                    .map_err(|e| format!("Async handler error: {}", e))?
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
    pub fn invoke(
        &self,
        handler_id: usize,
        request: Request,
    ) -> Result<serde_json::Value, String> {
        let handler = self
            .get(handler_id)
            .ok_or_else(|| format!("Handler {} not found", handler_id))?;

        Python::with_gil(|py| {
            // Call the Python handler with the request
            let result = handler
                .call1(py, (request,))
                .map_err(|e| format!("Handler error: {}", e))?;

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
