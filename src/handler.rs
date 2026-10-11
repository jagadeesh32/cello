//! Python handler registry and invocation.
//!
//! Manages Python function handlers with minimal GIL overhead.
//! Supports both synchronous `def` and asynchronous `async def` handlers.
//!
//! PERF: Caches handler metadata (is_async, DI requirements) at registration time
//! to avoid expensive Python introspection on every request.

use parking_lot::RwLock;
use pyo3::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::RawJson;
use crate::json::{python_to_json, python_to_json_bytes_direct, Holes};
use crate::request::Request;

/// Offload decision is made per call by timing the handler (default).
pub const POLICY_AUTO: u8 = 0;
/// Handler always runs inline on the server thread (`blocking=False`).
pub const POLICY_NEVER: u8 = 1;
/// Handler always runs on the blocking pool (`blocking=True`).
pub const POLICY_ALWAYS: u8 = 2;

/// Default threshold above which a sync handler is deemed blocking (microseconds).
pub const DEFAULT_OFFLOAD_THRESHOLD_US: u64 = 1_000;

/// Consecutive over-threshold calls required before a handler is offloaded.
/// Must be > 1 so one-time warmup on the first call cannot promote a cheap handler.
const SLOW_STREAK_TO_OFFLOAD: u8 = 2;

/// Result from handler invocation - either pre-serialized JSON bytes or a serde_json::Value.
/// PERF: The bytes variant skips the intermediate serde_json::Value allocation for the common
/// case where a handler returns a plain dict (not a Response object).
pub enum HandlerResult {
    /// Pre-serialized JSON bytes (common case: dict/list/primitive return)
    JsonBytes(Vec<u8>),
    /// serde_json::Value (Response objects that need special handling)
    JsonValue(serde_json::Value),
}

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
    /// Sticky flag: route this handler to the blocking pool. Set one-way (never
    /// cleared) once a sync call is observed to exceed the offload threshold, so
    /// the decision cannot flap under load.
    offload: AtomicBool,
    /// Consecutive slow observations. A handler's *first* call pays one-time
    /// warmup (importing `inspect`, the coroutine probe, cold code paths) that can
    /// exceed the threshold on its own, so a single sample would misclassify cheap
    /// handlers. Requiring consecutive slow calls filters that out; a fast call
    /// resets the streak.
    slow_streak: AtomicU8,
    /// Explicit `blocking=` override; see the `POLICY_*` constants.
    offload_policy: AtomicU8,
}

/// Registry for Python handler functions.
#[derive(Clone)]
pub struct HandlerRegistry {
    /// Store handlers with cached metadata
    handlers: Arc<RwLock<Vec<Arc<HandlerMeta>>>>,
    /// PERF: Cached flag for whether any DI singletons exist (avoids lock per request)
    has_dependencies: Arc<AtomicBool>,
    /// Sync-handler duration (microseconds) above which a handler is promoted to
    /// the blocking pool. `u64::MAX` disables adaptive offload entirely.
    offload_threshold_us: Arc<AtomicU64>,
}

impl HandlerRegistry {
    /// Create a new empty handler registry.
    pub fn new() -> Self {
        HandlerRegistry {
            handlers: Arc::new(RwLock::new(Vec::new())),
            has_dependencies: Arc::new(AtomicBool::new(false)),
            offload_threshold_us: Arc::new(AtomicU64::new(DEFAULT_OFFLOAD_THRESHOLD_US)),
        }
    }

    /// Set the adaptive-offload threshold in microseconds.
    ///
    /// `u64::MAX` disables adaptive promotion (explicit `blocking=True` still works).
    pub fn set_offload_threshold_us(&self, us: u64) {
        self.offload_threshold_us.store(us, Ordering::Relaxed);
    }

    /// Register a Python handler function.
    ///
    /// Reads the optional `__cello_blocking__` attribute set by the route decorators
    /// to seed the offload policy: `True` pools the handler from the first request,
    /// `False` pins it inline, absent leaves it adaptive.
    ///
    /// # Returns
    /// The unique handler ID for this function.
    pub fn register(&mut self, handler: PyObject) -> usize {
        let policy = Python::with_gil(|py| {
            match handler
                .as_ref(py)
                .getattr("__cello_blocking__")
                .ok()
                .and_then(|v| if v.is_none() { None } else { v.is_true().ok() })
            {
                Some(true) => POLICY_ALWAYS,
                Some(false) => POLICY_NEVER,
                None => POLICY_AUTO,
            }
        });

        let meta = Arc::new(HandlerMeta {
            handler,
            is_async: AtomicBool::new(false),
            async_checked: AtomicBool::new(false),
            di_params: RwLock::new(None),
            di_checked: AtomicBool::new(false),
            offload: AtomicBool::new(false),
            slow_streak: AtomicU8::new(0),
            offload_policy: AtomicU8::new(policy),
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
    /// 4. Async handlers: one GIL acquisition on the server thread (call + submit)
    /// 5. Returns HandlerResult::JsonBytes for common dict returns (skips serde_json::Value)
    ///
    /// GIL RELEASE ARCHITECTURE:
    ///   Phase 1 (GIL):    Call Python handler, do DI resolution, detect coroutine.
    ///                      A coroutine is submitted to the persistent asyncio loop
    ///                      under the same GIL hold.
    ///   Phase 2 (no GIL): The server thread awaits a oneshot channel. The loop runs
    ///                      the coroutine; its done-callback serializes the result on
    ///                      the loop thread (which holds the GIL there anyway) and
    ///                      sends the `HandlerResult` back.
    ///   Sync handlers serialize inline right after Phase 1.
    pub async fn invoke_async(
        &self,
        handler_id: usize,
        request: Request,
        dependency_container: Arc<crate::dependency::DependencyContainer>,
    ) -> Result<HandlerResult, String> {
        let meta = self
            .get_meta(handler_id)
            .ok_or_else(|| format!("Handler {handler_id} not found"))?;

        // PERF: Fast atomic check instead of RwLock read on dependency container
        let has_dependencies = self.has_dependencies.load(Ordering::Relaxed)
            && dependency_container.has_py_singletons();

        let policy = meta.offload_policy.load(Ordering::Relaxed);
        let should_offload = match policy {
            POLICY_NEVER => false,
            POLICY_ALWAYS => true,
            _ => meta.offload.load(Ordering::Relaxed),
        };

        // ── Offloaded path: call + drive + serialize on one blocking thread ────
        //
        // Sync handlers that block (sleep, DB drivers, socket I/O) would otherwise
        // pin the single-threaded Tokio runtime for their whole duration, stalling
        // accept and HTTP parsing for every other connection. Running them on the
        // blocking pool frees the server thread; the blocking call itself releases
        // the GIL, so they genuinely overlap.
        if should_offload {
            let (tx, rx) = tokio::sync::oneshot::channel::<Result<HandlerResult, String>>();
            let meta = Arc::clone(&meta);
            let deps = Arc::clone(&dependency_container);
            tokio::task::spawn_blocking(move || {
                let result = (|| {
                    let (raw, is_coro, _) = Python::with_gil(|py| {
                        call_handler(py, &meta, request, &deps, has_dependencies, false)
                    })?;
                    // Safety net: a handler misclassified as sync (callable object,
                    // partial, decorator that drops __wrapped__) still returns a
                    // coroutine. Driving it here is legal — we are already blocking.
                    let final_result = if is_coro {
                        Python::with_gil(|py| {
                            crate::async_loop::run_coroutine_blocking(py, raw.as_ref(py))
                        })
                        .map_err(|e| format!("Async handler error: {e}"))?
                    } else {
                        raw
                    };
                    let pending = Python::with_gil(|py| {
                        Ok::<_, String>(match serialize(py, final_result.as_ref(py))? {
                            Serialized::Done(r) => return Ok(Err(r)),
                            Serialized::Pending(body, holes) => {
                                let (tx, rx) = tokio::sync::oneshot::channel();
                                fill_holes(py, body, holes, tx);
                                Ok(rx)
                            }
                        })
                    })?;
                    match pending {
                        Err(done) => Ok(done),
                        // We are on a blocking-pool thread: waiting here is legal.
                        Ok(rx) => rx
                            .blocking_recv()
                            .map_err(|e| format!("Handler channel error: {e}"))?,
                    }
                })();
                let _ = tx.send(result);
            });
            return rx
                .await
                .map_err(|e| format!("Handler channel error: {e}"))?;
        }

        // ── Phase 1 (GIL): call handler, detect coroutine ──────────────────────
        let threshold = self.offload_threshold_us.load(Ordering::Relaxed);
        let adaptive = policy == POLICY_AUTO && threshold != u64::MAX;
        let (raw_result, is_coroutine, call_time, pending) = Python::with_gil(|py| {
            let (raw, is_coro, t) = call_handler(
                py,
                &meta,
                request,
                &dependency_container,
                has_dependencies,
                adaptive,
            )?;
            if !is_coro {
                return Ok((Some(raw), false, t, None));
            }
            // Hand the coroutine to the persistent loop under this same GIL hold.
            let pending = submit_and_serialize(py, raw.as_ref(py))?;
            Ok::<_, String>((None, true, t, Some(pending)))
        })?;

        // Adaptive promotion: a sync handler that repeatedly exceeds the threshold is
        // blocking, so every subsequent call goes to the pool. One-way, so at most
        // the first couple of calls pay the inline cost. Async handlers are exempt —
        // `call_time` only covers building the coroutine, not awaiting it.
        if adaptive && !is_coroutine {
            if call_time.as_micros() as u64 >= threshold {
                if meta.slow_streak.fetch_add(1, Ordering::Relaxed) + 1 >= SLOW_STREAK_TO_OFFLOAD {
                    meta.offload.store(true, Ordering::Relaxed);
                }
            } else {
                meta.slow_streak.store(0, Ordering::Relaxed);
            }
        }

        // HOT PATH: a sync handler is already done — serialize here rather than
        // awaiting another async fn, so the common case has no extra state machine.
        match (raw_result, pending) {
            (Some(raw), _) => {
                let rx = Python::with_gil(|py| match serialize(py, raw.as_ref(py))? {
                    Serialized::Done(r) => Ok(Err(r)),
                    Serialized::Pending(body, holes) => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        fill_holes(py, body, holes, tx);
                        Ok::<_, String>(Ok(rx))
                    }
                })?;
                match rx {
                    Err(done) => Ok(done),
                    Ok(rx) => rx
                        .await
                        .map_err(|e| format!("Handler channel error: {e}"))?,
                }
            }
            (None, Some(rx)) => rx
                .await
                .map_err(|e| format!("Handler channel error: {e}"))?,
            (None, None) => unreachable!("a coroutine is always submitted"),
        }
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

/// Submit a handler's coroutine to the persistent asyncio loop and return a
/// channel that yields its serialized result.
///
/// The coroutine runs on the single PERSISTENT loop (see `crate::async_loop`)
/// rather than a fresh `asyncio.run()` per request, so loop-bound resources
/// (client sessions, pools, asyncio locks/queues) stay valid across requests,
/// and nothing holds the GIL while it awaits I/O. When it finishes, the result
/// is serialized right there on the loop thread — which holds the GIL at that
/// point anyway — so the server thread only receives plain bytes and never has
/// to wait for the GIL again. (Previously a blocking-pool thread was parked on
/// `Future.result()` for every request and both it and the server thread
/// re-acquired the GIL afterwards: two extra thread hand-offs per request.)
fn submit_and_serialize(
    py: Python<'_>,
    coro: &PyAny,
) -> Result<tokio::sync::oneshot::Receiver<Result<HandlerResult, String>>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::async_loop::submit_coroutine(
        py,
        coro,
        Box::new(move |py, outcome| {
            let result = match outcome {
                Ok(value) => match serialize(py, value) {
                    Ok(Serialized::Done(r)) => Ok(r),
                    Ok(Serialized::Pending(body, holes)) => return fill_holes(py, body, holes, tx),
                    Err(e) => Err(e),
                },
                Err(e) => Err(format!("Async handler error: {e}")),
            };
            // The receiver is gone only if the request was abandoned (timeout).
            let _ = tx.send(result);
        }),
    )
    .map_err(|e| format!("Async handler error: {e}"))?;
    Ok(rx)
}

/// A serialized handler return value.
enum Serialized {
    Done(HandlerResult),
    /// The value contains querysets: `body` has a hole for each (see
    /// [`crate::json::Hole`]); [`fill_holes`] completes it.
    Pending(Vec<u8>, Holes),
}

/// Serialize a handler return value.
///
/// PERF: Try direct-to-bytes first (skips the intermediate serde_json::Value).
fn serialize(py: Python<'_>, obj: &PyAny) -> Result<Serialized, String> {
    match python_to_json_bytes_direct(py, obj)? {
        Some((bytes, holes)) if holes.is_empty() => {
            Ok(Serialized::Done(HandlerResult::JsonBytes(bytes)))
        }
        Some((bytes, holes)) => Ok(Serialized::Pending(bytes, holes)),
        None => python_to_json(py, obj).map(|v| Serialized::Done(HandlerResult::JsonValue(v))),
    }
}

/// Run the querysets a handler returned and send the completed body to `tx`.
///
/// The querysets run on the persistent loop through `cello.orm._run_querysets`
/// (each one `QuerySet.json()`: rows are encoded to JSON on the I/O thread,
/// with no Python object per row). The body is spliced together on the loop
/// thread when they finish. This happens after the handler has returned, so
/// the queries run outside any transaction the handler had open.
fn fill_holes(
    py: Python<'_>,
    body: Vec<u8>,
    holes: Holes,
    tx: tokio::sync::oneshot::Sender<Result<HandlerResult, String>>,
) {
    let at: Vec<usize> = holes.iter().map(|h| h.at).collect();
    let querysets: Vec<PyObject> = holes.into_iter().map(|h| h.queryset).collect();
    let awaitable = match crate::schema::runtime::run_querysets(py, querysets) {
        Ok(a) => a,
        Err(e) => {
            let _ = tx.send(Err(format!("Handler error: {e}")));
            return;
        }
    };
    let submitted = crate::async_loop::submit_coroutine(
        py,
        awaitable,
        Box::new(move |_py, outcome| {
            let result = match outcome {
                Ok(frags) => splice(&body, &at, frags).map(HandlerResult::JsonBytes),
                Err(e) => Err(format!("Handler error: {e}")),
            };
            let _ = tx.send(result);
        }),
    );
    if let Err(e) = submitted {
        // `tx` went down with the callback; the receiver reports the failure.
        e.print(py);
    }
}

/// `body` with `frags[i]` (a list of `RawJSON`) inserted at `at[i]`.
fn splice(body: &[u8], at: &[usize], frags: &PyAny) -> Result<Vec<u8>, String> {
    let err = |e: PyErr| format!("Handler error: {e}");
    let frags: Vec<&PyCell<RawJson>> = frags
        .iter()
        .map_err(err)?
        .map(|f| f.map_err(err)?.downcast().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    if frags.len() != at.len() {
        return Err("Handler error: queryset results do not match the response".into());
    }
    let extra: usize = frags.iter().map(|f| f.get().bytes.len()).sum();
    let mut out = Vec::with_capacity(body.len() + extra);
    let mut prev = 0;
    for (pos, frag) in at.iter().zip(frags) {
        out.extend_from_slice(&body[prev..*pos]);
        out.extend_from_slice(&frag.get().bytes);
        prev = *pos;
    }
    out.extend_from_slice(&body[prev..]);
    Ok(out)
}

/// Call the Python handler under the GIL, resolving DI and detecting a coroutine.
///
/// Returns the raw return value and whether it is a coroutine that still needs
/// driving. Shared by the inline and offloaded paths so DI caching and async
/// detection behave identically on both.
fn call_handler(
    py: Python<'_>,
    meta: &HandlerMeta,
    request: Request,
    dependency_container: &crate::dependency::DependencyContainer,
    has_dependencies: bool,
    time_it: bool,
) -> Result<(PyObject, bool, Duration), String> {
    // Time the Python call only, measured *inside* the GIL. Wall-clock around
    // `Python::with_gil` would also count time spent waiting to acquire the GIL,
    // which under concurrency misclassifies cheap handlers as blocking. Skipped
    // entirely when the handler's policy is fixed and no promotion can result.
    let started = time_it.then(Instant::now);
    let call_result: PyObject = if has_dependencies {
        // DI resolution — cache parameter info on first call
        if !meta.di_checked.load(Ordering::Relaxed) {
            let mut di_params = Vec::new();
            if let Ok(inspect) = py.import("inspect") {
                if let Ok(sig) = inspect.call_method1("signature", (meta.handler.as_ref(py),)) {
                    if let Ok(parameters) = sig.getattr("parameters") {
                        let cello_module = py.import("cello").ok();
                        let depends_type = cello_module.and_then(|m| m.getattr("Depends").ok());

                        if let Ok(items) = parameters.call_method0("items") {
                            if let Ok(iter) = items.iter() {
                                for item in iter.flatten() {
                                    if let (Ok(name), Ok(param)) = (
                                        item.get_item(0).and_then(|v| v.extract::<String>()),
                                        item.get_item(1),
                                    ) {
                                        if let Ok(default) = param.getattr("default") {
                                            if let Some(dt) = &depends_type {
                                                if default.is_instance(dt).unwrap_or(false) {
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

        let di_guard = meta.di_params.read();
        match di_guard.as_ref() {
            Some(params) if params.is_empty() => {
                // No DI params — fast path
                meta.handler
                    .call1(py, (request,))
                    .map_err(|e| format!("Handler error: {e}"))?
            }
            Some(params) => {
                let kwargs = pyo3::types::PyDict::new(py);
                for (param_name, dep_name) in params {
                    if let Some(dep_value) = dependency_container.get_py_singleton(dep_name) {
                        let _ = kwargs.set_item(param_name, dep_value);
                    }
                }
                meta.handler
                    .call(py, (request,), Some(kwargs))
                    .map_err(|e| format!("Handler error: {e}"))?
            }
            None => {
                // Params not cached yet (should not happen); use fast path
                meta.handler
                    .call1(py, (request,))
                    .map_err(|e| format!("Handler error: {e}"))?
            }
        }
    } else {
        // FAST PATH: direct call, no DI, no locks
        meta.handler
            .call1(py, (request,))
            .map_err(|e| format!("Handler error: {e}"))?
    };

    // Cache async detection per handler (first call probes, then reads atomically)
    let is_coro = if meta.async_checked.load(Ordering::Relaxed) {
        meta.is_async.load(Ordering::Relaxed)
    } else {
        let is_async = py
            .import("inspect")
            .and_then(|inspect| inspect.call_method1("iscoroutine", (call_result.as_ref(py),)))
            .and_then(|r| r.is_true())
            .unwrap_or(false);
        meta.is_async.store(is_async, Ordering::Relaxed);
        meta.async_checked.store(true, Ordering::Relaxed);
        is_async
    };

    // PyObject (Py<PyAny>) is Send — safe to move out of GIL closure
    Ok((
        call_result,
        is_coro,
        started.map(|s| s.elapsed()).unwrap_or_default(),
    ))
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
