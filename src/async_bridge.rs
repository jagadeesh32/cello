//! Rust future → Python awaitable bridge for native async I/O (Postgres, Redis,
//! HTTP client).
//!
//! `future_into_py(py, fut)` returns an `asyncio.Future` bound to the caller's
//! running loop and resolves it once `fut` completes. It replaces
//! `pyo3_asyncio::tokio::future_into_py`, whose per-call cost on this
//! architecture was dominated by cross-thread wakeups and GIL hand-offs rather
//! than by the I/O itself. Three changes:
//!
//! 1. **One I/O thread.** All native I/O runs on a single current-thread Tokio
//!    runtime (`cello-io`). The pooled connections' driver tasks live on the
//!    same thread as the queries using them, so a query no longer bounces
//!    between worker threads (each bounce is a futex wake) — it costs what the
//!    raw driver costs. The I/O work itself is a few µs of CPU per operation;
//!    Python, not this thread, is the throughput limit of a process.
//! 2. **I/O threads never take the GIL.** A finished operation is pushed onto
//!    its loop's completion queue and the loop is woken through a pipe it
//!    watches with `add_reader` (a raw `write(2)`, no GIL). The loop thread —
//!    which owns the GIL anyway when it runs — converts the Rust result into
//!    Python objects and resolves the futures. Previously each completion took
//!    the GIL twice on a Tokio worker and then woke the loop with
//!    `call_soon_threadsafe`, which also had to wait for the GIL to come back.
//!    Under load one wake drains every completion that arrived meanwhile.
//! 3. **Exit safety.** Because no Rust-owned thread ever enters Python, none can
//!    be caught in CPython's "thread tried to take the GIL during finalization"
//!    path, which terminates the thread with `pthread_exit`; that forced unwind
//!    through Rust frames aborted the process with "panic in a function that
//!    cannot unwind" (an intermittent crash at interpreter exit after scripts
//!    that used the database or Redis).
//!
//! Semantics kept from the previous bridge:
//! * the future belongs to the loop running when the method is called (a
//!   `RuntimeError` is raised when there is none);
//! * cancelling the asyncio future aborts the Rust task (the I/O future is
//!   dropped) and any late result is discarded;
//! * a `PyErr` produced by the Rust future is raised from the `await`; a panic
//!   becomes `RustPanic` (only reachable in unwinding builds).
//!
//! On non-Unix platforms the completion pipe is not available and completions
//! fall back to `call_soon_threadsafe` from the I/O thread (under the GIL).

use std::any::Any;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;

use futures_util::FutureExt;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use tokio::runtime::Handle;
use tokio::task::AbortHandle;

/// Turns a finished operation's Rust value into a Python object. Runs on the
/// loop thread, with the GIL held.
type Convert = Box<dyn FnOnce(Python<'_>) -> PyResult<PyObject> + Send>;

// ── I/O runtime ──────────────────────────────────────────────────────────────

static IO: OnceLock<Handle> = OnceLock::new();

/// Handle to the `cello-io` runtime, started on first use. The thread is never
/// joined; it holds no Python state, so process exit simply ends it.
fn io_handle() -> PyResult<&'static Handle> {
    if let Some(h) = IO.get() {
        return Ok(h);
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("cello-io runtime: {e}")))?;
    let handle = rt.handle().clone();
    // Lost a (rare) initialisation race: discard ours, use the winner's.
    if IO.set(handle).is_err() {
        return Ok(IO.get().expect("set by the winning thread"));
    }
    std::thread::Builder::new()
        .name("cello-io".into())
        .spawn(move || rt.block_on(std::future::pending::<()>()))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("cello-io thread: {e}")))?;
    Ok(IO.get().expect("just set"))
}

// ── Python helpers ───────────────────────────────────────────────────────────

struct Helpers {
    get_running_loop: PyObject,
    #[cfg(unix)]
    finalize: PyObject,
}

static HELPERS: GILOnceCell<Helpers> = GILOnceCell::new();

fn helpers(py: Python<'_>) -> PyResult<&'static Helpers> {
    HELPERS.get_or_try_init(py, || {
        Ok(Helpers {
            get_running_loop: py.import("asyncio")?.getattr("get_running_loop")?.into(),
            #[cfg(unix)]
            finalize: py.import("weakref")?.getattr("finalize")?.into(),
        })
    })
}

/// Resolve `fut` with the converted result unless it was cancelled meanwhile.
fn resolve(py: Python<'_>, fut: &PyAny, result: PyResult<Convert>) {
    match fut.call_method0("cancelled").and_then(|c| c.is_true()) {
        Ok(false) => {}
        Ok(true) => return,
        Err(e) => return e.print(py),
    }
    let outcome = match result.and_then(|convert| convert(py)) {
        Ok(v) => fut.call_method1("set_result", (v,)),
        Err(e) => fut.call_method1("set_exception", (e.into_value(py),)),
    };
    if let Err(e) = outcome {
        e.print(py);
    }
}

// ── Completion port (Unix) ───────────────────────────────────────────────────

#[cfg(unix)]
mod port {
    use super::*;
    use std::collections::HashMap;
    use std::os::fd::RawFd;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    struct Inner {
        queue: Vec<(PyObject, PyResult<Convert>)>,
        /// The loop was garbage-collected; late results are dropped.
        closed: bool,
    }

    /// Per-event-loop completion queue plus the non-blocking pipe that wakes
    /// the loop. Shared by the loop's reader callback and every in-flight task;
    /// the pipe is closed when the last of them lets go, so a task can never
    /// write into a recycled descriptor.
    pub(super) struct Port {
        inner: Mutex<Inner>,
        /// A wake byte is in flight; skip the syscall until the loop drains.
        armed: AtomicBool,
        rfd: RawFd,
        wfd: RawFd,
    }

    impl Drop for Port {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.rfd);
                libc::close(self.wfd);
            }
        }
    }

    impl Port {
        /// Called on the I/O thread. Never touches Python.
        pub(super) fn complete(&self, fut: PyObject, result: PyResult<Convert>) {
            {
                let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                if inner.closed {
                    // Dropped without the GIL: pyo3 defers the decref.
                    return;
                }
                inner.queue.push((fut, result));
            }
            if !self.armed.swap(true, Ordering::AcqRel) {
                let byte = 1u8;
                // EAGAIN (pipe full) means a wake is already pending.
                unsafe { libc::write(self.wfd, &byte as *const u8 as *const libc::c_void, 1) };
            }
        }

        fn drain(&self, py: Python<'_>) {
            let mut buf = [0u8; 256];
            while unsafe { libc::read(self.rfd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) }
                > 0
            {}
            // Disarm before taking the queue: a producer that pushes after the
            // swap below re-arms and triggers another (possibly empty) drain.
            self.armed.store(false, Ordering::Release);
            let batch = {
                let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                std::mem::take(&mut inner.queue)
            };
            for (fut, result) in batch {
                resolve(py, fut.as_ref(py), result);
            }
        }
    }

    /// `loop.add_reader` callback: resolves every queued completion.
    #[pyclass]
    struct PortReader {
        port: Arc<Port>,
    }

    #[pymethods]
    impl PortReader {
        fn __call__(&self, py: Python<'_>) {
            self.port.drain(py);
        }
    }

    /// `weakref.finalize` callback for the loop: forget its port.
    #[pyclass]
    struct PortFinalizer {
        key: usize,
        port: Arc<Port>,
    }

    #[pymethods]
    impl PortFinalizer {
        fn __call__(&self) {
            self.port
                .inner
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .closed = true;
            let mut ports = ports();
            if ports
                .get(&self.key)
                .is_some_and(|p| Arc::ptr_eq(p, &self.port))
            {
                ports.remove(&self.key);
            }
        }
    }

    /// Live loops (keyed by object address) → their port. An entry is removed
    /// by the loop's finalizer before the address can be reused.
    fn ports() -> std::sync::MutexGuard<'static, HashMap<usize, Arc<Port>>> {
        static PORTS: OnceLock<Mutex<HashMap<usize, Arc<Port>>>> = OnceLock::new();
        PORTS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn os_err(what: &str) -> PyErr {
        pyo3::exceptions::PyOSError::new_err(format!("{what}: {}", std::io::Error::last_os_error()))
    }

    fn new_pipe() -> PyResult<(RawFd, RawFd)> {
        let mut fds = [0 as RawFd; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(os_err("cello async bridge pipe"));
        }
        for fd in fds {
            unsafe {
                let fl = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK);
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        Ok((fds[0], fds[1]))
    }

    /// The port for `event_loop`, creating and registering it on first use.
    /// Must be called on the loop's thread (it calls `add_reader`).
    pub(super) fn port_for(py: Python<'_>, event_loop: &PyAny) -> PyResult<Arc<Port>> {
        let key = event_loop.as_ptr() as usize;
        if let Some(p) = ports().get(&key) {
            return Ok(p.clone());
        }
        let (rfd, wfd) = new_pipe()?;
        let port = Arc::new(Port {
            inner: Mutex::new(Inner {
                queue: Vec::new(),
                closed: false,
            }),
            armed: AtomicBool::new(false),
            rfd,
            wfd,
        });
        event_loop.call_method1("add_reader", (rfd, PortReader { port: port.clone() }))?;
        helpers(py)?.finalize.call1(
            py,
            (
                event_loop,
                PortFinalizer {
                    key,
                    port: port.clone(),
                },
            ),
        )?;
        ports().insert(key, port.clone());
        Ok(port)
    }
}

// ── Cancellation ─────────────────────────────────────────────────────────────

/// `add_done_callback` target: aborts the Rust task if the asyncio future was
/// cancelled (task cancellation, `asyncio.wait_for` timeout, ...).
#[pyclass]
struct AbortOnCancel {
    handle: AbortHandle,
}

#[pymethods]
impl AbortOnCancel {
    fn __call__(&self, fut: &PyAny) -> PyResult<()> {
        if fut.call_method0("cancelled")?.is_true()? {
            self.handle.abort();
        }
        Ok(())
    }
}

fn panic_message(panic: &(dyn Any + Send)) -> &str {
    if let Some(s) = panic.downcast_ref::<&str>() {
        s
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s
    } else {
        "unknown error"
    }
}

// ── Bridge ───────────────────────────────────────────────────────────────────

/// Run `fut` on the I/O thread and return an awaitable for its result.
///
/// `fut` yields a conversion closure that builds the Python value; it runs on
/// the loop thread (GIL held) when the future is resolved. Use this when the
/// result needs Python objects to build (rows → dicts).
pub fn future_into_py_with<F, C>(py: Python<'_>, fut: F) -> PyResult<&PyAny>
where
    F: Future<Output = PyResult<C>> + Send + 'static,
    C: FnOnce(Python<'_>) -> PyResult<PyObject> + Send + 'static,
{
    let event_loop = helpers(py)?.get_running_loop.as_ref(py).call0()?;
    let py_fut = event_loop.call_method0("create_future")?;
    let fut_obj: PyObject = py_fut.into();

    #[cfg(unix)]
    let port = port::port_for(py, event_loop)?;
    #[cfg(not(unix))]
    let event_loop: PyObject = event_loop.into();

    let task = io_handle()?.spawn(async move {
        // Map a panic to an exception rather than leaving the awaiter hanging
        // (only reachable in unwinding builds; release uses `panic = "abort"`).
        let result: PyResult<Convert> = match AssertUnwindSafe(fut).catch_unwind().await {
            Ok(Ok(convert)) => Ok(Box::new(convert)),
            Ok(Err(e)) => Err(e),
            Err(panic) => Err(pyo3_asyncio::err::RustPanic::new_err(format!(
                "rust future panicked: {}",
                panic_message(&*panic)
            ))),
        };
        #[cfg(unix)]
        port.complete(fut_obj, result);
        #[cfg(not(unix))]
        Python::with_gil(move |py| {
            let cb = ResolveOnLoop {
                payload: std::sync::Mutex::new(Some((fut_obj, result))),
            };
            // Fails only once the loop is closed and nobody can await anymore.
            let _ = event_loop.call_method1(py, "call_soon_threadsafe", (cb,));
        });
    });

    py_fut.call_method1(
        "add_done_callback",
        (AbortOnCancel {
            handle: task.abort_handle(),
        },),
    )?;
    Ok(py_fut)
}

/// Non-Unix completion: run on the loop via `call_soon_threadsafe`.
#[cfg(not(unix))]
#[pyclass]
struct ResolveOnLoop {
    payload: std::sync::Mutex<Option<(PyObject, PyResult<Convert>)>>,
}

#[cfg(not(unix))]
#[pymethods]
impl ResolveOnLoop {
    fn __call__(&self, py: Python<'_>) {
        if let Some((fut, result)) = self
            .payload
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            resolve(py, fut.as_ref(py), result);
        }
    }
}

/// Run `fut` on the I/O thread and return an awaitable for its result
/// (drop-in for `pyo3_asyncio::tokio::future_into_py`). The value is converted
/// with `IntoPy` on the loop thread.
pub fn future_into_py<F, T>(py: Python<'_>, fut: F) -> PyResult<&PyAny>
where
    F: Future<Output = PyResult<T>> + Send + 'static,
    T: IntoPy<PyObject> + Send + 'static,
{
    future_into_py_with(py, async move {
        let v = fut.await?;
        Ok(move |py: Python<'_>| Ok(v.into_py(py)))
    })
}

/// An already-resolved awaitable for `value` on the running loop (for
/// `__aenter__`/`__aexit__`-style methods that do no I/O).
pub fn ready(py: Python<'_>, value: PyObject) -> PyResult<&PyAny> {
    let event_loop = helpers(py)?.get_running_loop.as_ref(py).call0()?;
    let fut = event_loop.call_method0("create_future")?;
    fut.call_method1("set_result", (value,))?;
    Ok(fut)
}
