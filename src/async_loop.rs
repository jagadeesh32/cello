//! A single persistent asyncio event loop running on a dedicated daemon thread.
//!
//! Async Python handlers are submitted to this loop with [`submit_coroutine`]
//! (a done-callback hands the serialized result back through a channel) and
//! lifecycle hooks via `asyncio.run_coroutine_threadsafe`, instead of a fresh
//! `asyncio.run()` per call.
//!
//! Why this matters:
//! - **Loop-bound resources survive.** A module-level `aiohttp.ClientSession`,
//!   async database driver pools, `asyncio.Lock`/`Queue`, etc. are bound to the
//!   loop that created them. With `asyncio.run()` (a fresh loop per request) the 2nd
//!   request raised `RuntimeError: ... attached to a different loop`. One persistent
//!   loop fixes that.
//! - **Real concurrency.** Nothing holds the GIL while a coroutine awaits I/O, so
//!   the loop runs other coroutines meanwhile.

use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyModule;
use std::sync::{Mutex, OnceLock};

/// The process-wide persistent event loop (a Python `asyncio` loop object).
static EVENT_LOOP: OnceLock<Py<PyAny>> = OnceLock::new();

/// The helper module's `submit` function.
static SUBMIT: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

/// Python helpers: `start_loop` creates a new event loop and runs it forever on
/// a daemon thread, returning the loop object so coroutines can be scheduled
/// onto it; `submit` schedules a coroutine on it with a completion callback.
const RUNNER_SRC: &str = r#"
import asyncio
import atexit
import threading


def start_loop():
    loop = asyncio.new_event_loop()

    def _run():
        asyncio.set_event_loop(loop)
        loop.run_forever()

    thread = threading.Thread(target=_run, name="cello-asyncio", daemon=True)
    thread.start()

    # Stop the loop before the interpreter finalizes. A daemon thread that is
    # inside native code when finalization starts is terminated the next time it
    # asks for the GIL, which aborts the process if native frames are on its
    # stack; a stopped loop thread has simply returned.
    def _stop():
        if loop.is_running():
            loop.call_soon_threadsafe(loop.stop)
            thread.join(1.0)

    atexit.register(_stop)
    return loop


def _start(coro, done):
    # Runs on the loop thread (scheduled with call_soon_threadsafe).
    try:
        task = asyncio.ensure_future(coro)
    except BaseException as exc:
        done.fail(exc)
        return
    task.add_done_callback(done)


def submit(loop, coro, done):
    loop.call_soon_threadsafe(_start, coro, done)
"#;

fn runner(py: Python<'_>) -> PyResult<&PyModule> {
    static MODULE: GILOnceCell<Py<PyModule>> = GILOnceCell::new();
    MODULE
        .get_or_try_init(py, || {
            PyModule::from_code(py, RUNNER_SRC, "cello_async_loop.py", "cello_async_loop")
                .map(Into::into)
        })
        .map(|m| m.as_ref(py))
}

fn create_loop(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let loop_obj = runner(py)?.getattr("start_loop")?.call0()?;
    Ok(loop_obj.into_py(py))
}

/// Return the persistent event loop, starting it on first use.
pub fn get_loop(py: Python<'_>) -> PyResult<Py<PyAny>> {
    if let Some(l) = EVENT_LOOP.get() {
        return Ok(l.clone_ref(py));
    }
    // Initialisation runs while holding the GIL, so concurrent callers are largely
    // serialised; in the worst case a rare race starts a second (idle) loop whose
    // handle is dropped. `OnceLock` guarantees a single winner is ever observed.
    let loop_obj = create_loop(py)?;
    match EVENT_LOOP.set(loop_obj.clone_ref(py)) {
        Ok(()) => Ok(loop_obj),
        Err(_) => Ok(EVENT_LOOP
            .get()
            .expect("event loop set by the winning thread")
            .clone_ref(py)),
    }
}

/// Eagerly start the loop. Call once at server startup (while single-threaded) so
/// the first request never has to pay initialisation cost or race on it.
pub fn ensure_started(py: Python<'_>) {
    if let Err(e) = get_loop(py) {
        eprintln!("Warning: failed to start persistent asyncio loop: {e}");
    }
}

/// Drive a coroutine on the persistent loop to completion and return its result.
///
/// MUST be called from a blocking context (e.g. `tokio::task::spawn_blocking`): the
/// wait inside `Future.result()` releases the GIL so the loop and other coroutines
/// keep running concurrently.
pub fn run_coroutine_blocking(py: Python<'_>, coro: &PyAny) -> Result<PyObject, String> {
    let event_loop = get_loop(py).map_err(|e| e.to_string())?;
    let asyncio = py.import("asyncio").map_err(|e| e.to_string())?;
    let cfut = asyncio
        .call_method1("run_coroutine_threadsafe", (coro, event_loop.as_ref(py)))
        .map_err(|e| e.to_string())?;
    // Blocks until the coroutine finishes; the internal wait releases the GIL.
    cfut.call_method0("result")
        .map(|r| r.into_py(py))
        .map_err(|e| e.to_string())
}

/// Receives the outcome of a coroutine submitted with [`submit_coroutine`].
/// Runs on the loop thread with the GIL held.
pub type Finish = Box<dyn FnOnce(Python<'_>, PyResult<&PyAny>) + Send>;

/// `Task.add_done_callback` target for [`submit_coroutine`].
#[pyclass]
struct CoroutineDone {
    finish: Mutex<Option<Finish>>,
}

impl CoroutineDone {
    fn finish(&self, py: Python<'_>, outcome: PyResult<&PyAny>) {
        let finish = self.finish.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(finish) = finish {
            finish(py, outcome);
        }
    }
}

#[pymethods]
impl CoroutineDone {
    fn __call__(&self, py: Python<'_>, task: &PyAny) {
        // `result()` returns the value or raises the coroutine's exception
        // (`CancelledError` if the task was cancelled), like `Future.result()`.
        self.finish(py, task.call_method0("result"));
    }

    fn fail(&self, py: Python<'_>, exc: &PyAny) {
        self.finish(py, Err(PyErr::from_value(exc)));
    }
}

/// Schedule `coro` on the persistent loop without blocking a thread on it.
///
/// `finish` is called on the loop thread, with the GIL held, once the coroutine
/// completes: convert the result there and hand it back through a channel.
/// Compared with [`run_coroutine_blocking`] this parks no blocking-pool thread
/// on `Future.result()` and the waiter never re-acquires the GIL, which removes
/// two thread hand-offs (and their GIL hand-offs) per async request.
pub fn submit_coroutine(py: Python<'_>, coro: &PyAny, finish: Finish) -> PyResult<()> {
    let event_loop = get_loop(py)?;
    let submit = SUBMIT.get_or_try_init(py, || runner(py)?.getattr("submit").map(Into::into))?;
    let done = CoroutineDone {
        finish: Mutex::new(Some(finish)),
    };
    submit.call1(py, (event_loop, coro, done))?;
    Ok(())
}
