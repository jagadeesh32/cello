//! WebSocket support for Cello.
//!
//! Provides WebSocket handling using tokio-tungstenite.

use parking_lot::RwLock;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

use crate::request::Request;

/// WebSocket message types for Python.
#[pyclass]
#[derive(Clone)]
pub struct WebSocketMessage {
    /// Message type: "text", "binary", "ping", "pong", "close"
    #[pyo3(get)]
    pub msg_type: String,

    /// Text data (for text messages)
    #[pyo3(get)]
    pub text: Option<String>,

    /// Binary data (for binary messages)
    #[pyo3(get)]
    pub data: Option<Vec<u8>>,
}

#[pymethods]
impl WebSocketMessage {
    /// Create a text message.
    #[staticmethod]
    #[pyo3(name = "text")]
    pub fn from_text(content: &str) -> Self {
        WebSocketMessage {
            msg_type: "text".to_string(),
            text: Some(content.to_string()),
            data: None,
        }
    }

    /// Create a binary message.
    #[staticmethod]
    #[pyo3(name = "binary")]
    pub fn from_binary(content: Vec<u8>) -> Self {
        WebSocketMessage {
            msg_type: "binary".to_string(),
            text: None,
            data: Some(content),
        }
    }

    /// Create a ping message.
    #[staticmethod]
    pub fn ping() -> Self {
        WebSocketMessage {
            msg_type: "ping".to_string(),
            text: None,
            data: None,
        }
    }

    /// Create a close message.
    #[staticmethod]
    pub fn close() -> Self {
        WebSocketMessage {
            msg_type: "close".to_string(),
            text: None,
            data: None,
        }
    }

    /// The payload: `str` for a text message, `bytes` for a binary one, else
    /// `None`. (`msg.text` is the `WebSocketMessage.text(...)` constructor, so
    /// the text field is read through this.)
    #[getter]
    pub fn content(&self, py: Python<'_>) -> PyObject {
        match (&self.text, &self.data) {
            (Some(t), _) => t.into_py(py),
            (None, Some(d)) => pyo3::types::PyBytes::new(py, d).into_py(py),
            _ => py.None(),
        }
    }

    /// Check if this is a text message.
    pub fn is_text(&self) -> bool {
        self.msg_type == "text"
    }

    /// Check if this is a binary message.
    pub fn is_binary(&self) -> bool {
        self.msg_type == "binary"
    }

    /// Check if this is a close message.
    pub fn is_close(&self) -> bool {
        self.msg_type == "close"
    }
}

/// A queued inbound message and the share of the inbound byte budget it holds
/// (released when the handler takes the message).
type Inbound = (WebSocketMessage, tokio::sync::OwnedSemaphorePermit);

/// The live side of an upgraded connection.
struct LiveSocket {
    /// Frames from the client, bounded by `INBOUND_BUDGET` bytes: once the
    /// handler falls that far behind, the connection task stops reading the
    /// socket (TCP backpressure) instead of queueing more.
    inbound: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<Inbound>>>,
    /// Frames to the client, written by the connection task.
    /// Bounded: a handler that outpaces a slow client waits instead of
    /// buffering without limit.
    outbound: tokio::sync::mpsc::Sender<Message>,
    /// The server runtime; drives the timer of a blocking `recv(timeout)`.
    runtime: tokio::runtime::Handle,
}

/// Next inbound message, or `None` once the client has closed or `timeout`
/// elapses.
async fn next_message(
    inbound: Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<Inbound>>>,
    timeout: Option<std::time::Duration>,
) -> Option<WebSocketMessage> {
    let next = async move {
        let mut rx = inbound.lock().await;
        rx.recv().await.map(|(msg, _budget)| msg)
    };
    match timeout {
        None => next.await,
        Some(t) => tokio::time::timeout(t, next).await.ok().flatten(),
    }
}

/// `asyncio._get_running_loop`, cached.
static GET_RUNNING_LOOP: pyo3::sync::GILOnceCell<PyObject> = pyo3::sync::GILOnceCell::new();

/// Refuse a blocking call made on a thread that runs an event loop: it would
/// stall every coroutine on that loop (the persistent loop serves all async
/// handlers). Async handlers use the `a`-prefixed awaitable methods.
fn forbid_on_event_loop(py: Python<'_>, method: &str, instead: &str) -> PyResult<()> {
    let get = GET_RUNNING_LOOP.get_or_try_init(py, || {
        py.import("asyncio")?
            .getattr("_get_running_loop")
            .map(Into::into)
    })?;
    if get.call0(py)?.is_none(py) {
        return Ok(());
    }
    Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
        "ws.{method}() blocks and cannot be called from a coroutine; use `await ws.{instead}(...)`"
    )))
}

/// Convert a message into a frame the client can be sent.
fn to_frame(message: WebSocketMessage) -> PyResult<Message> {
    Ok(match message.msg_type.as_str() {
        "text" => Message::Text(message.text.unwrap_or_default()),
        "binary" => Message::Binary(message.data.unwrap_or_default()),
        "ping" => Message::Ping(message.data.unwrap_or_default()),
        "close" => Message::Close(None),
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "cannot send a {other:?} message"
            )))
        }
    })
}

fn closed_error() -> PyErr {
    pyo3::exceptions::PyConnectionError::new_err("WebSocket is closed")
}

/// A negative, NaN, infinite or absurdly large timeout means "wait".
fn timeout_duration(timeout: Option<f64>) -> Option<std::time::Duration> {
    timeout.and_then(|t| std::time::Duration::try_from_secs_f64(t.max(0.0)).ok())
}

/// A WebSocket connection, passed to `@app.websocket(path)` handlers.
///
/// A plain `def` handler runs on the blocking thread pool and uses the
/// blocking methods:
///
/// ```python
/// @app.websocket("/ws")
/// def echo(ws):
///     while (msg := ws.recv()) is not None:
///         ws.send_text(f"Echo: {msg.content}")
/// ```
///
/// An `async def` handler runs on the application's event loop and awaits the
/// `a`-prefixed methods (`arecv`, `asend`, `asend_text`, `asend_binary`).
///
/// `WebSocket()` constructed directly is an offline stand-in for tests: sends
/// are queued and visible through `get_queued_messages()`.
#[pyclass]
pub struct WebSocket {
    connected: Arc<AtomicBool>,
    /// Offline queue (for `WebSocket()` constructed in tests).
    messages: Arc<RwLock<Vec<WebSocketMessage>>>,
    live: Option<LiveSocket>,
    request: Option<Request>,
}

#[pymethods]
impl WebSocket {
    /// Create a new WebSocket (for testing/mocking).
    #[new]
    pub fn new() -> Self {
        WebSocket {
            connected: Arc::new(AtomicBool::new(true)),
            messages: Arc::new(RwLock::new(Vec::new())),
            live: None,
            request: None,
        }
    }

    /// Whether the connection is open.
    #[getter]
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// The upgrade request (headers, query, path, and context set by
    /// middleware such as JWT claims). `None` for an offline socket.
    #[getter]
    pub fn request(&self) -> Option<Request> {
        self.request.clone()
    }

    /// Wait for the next text or binary message. Returns `None` once the client
    /// has closed the connection, or when `timeout` (seconds) elapses.
    /// Blocking: for `def` handlers. Async handlers use `await ws.arecv()`.
    #[pyo3(signature = (timeout=None))]
    pub fn recv(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Option<WebSocketMessage>> {
        let Some(live) = self.live.as_ref() else {
            return Ok(None);
        };
        forbid_on_event_loop(py, "recv", "arecv")?;
        let inbound = live.inbound.clone();
        let runtime = live.runtime.clone();
        let timeout = timeout_duration(timeout);
        Ok(py.allow_threads(move || runtime.block_on(next_message(inbound, timeout))))
    }

    /// Awaitable `recv`: resolves to the next message, or `None` once the
    /// client has closed or `timeout` (seconds) elapses.
    #[pyo3(signature = (timeout=None))]
    pub fn arecv<'py>(&self, py: Python<'py>, timeout: Option<f64>) -> PyResult<&'py PyAny> {
        let Some(live) = self.live.as_ref() else {
            return crate::async_bridge::ready(py, py.None());
        };
        let inbound = live.inbound.clone();
        let timeout = timeout_duration(timeout);
        crate::async_bridge::future_into_py(
            py,
            async move { Ok(next_message(inbound, timeout).await) },
        )
    }

    /// Send a text message.
    pub fn send_text(&self, py: Python<'_>, text: &str) -> PyResult<()> {
        self.send(py, WebSocketMessage::from_text(text))
    }

    /// Send a binary message.
    pub fn send_binary(&self, py: Python<'_>, data: Vec<u8>) -> PyResult<()> {
        self.send(py, WebSocketMessage::from_binary(data))
    }

    /// Send a message. Blocking (waits while the client is slow): for `def`
    /// handlers. Async handlers use `await ws.asend(...)`.
    pub fn send(&self, py: Python<'_>, message: WebSocketMessage) -> PyResult<()> {
        let Some(live) = &self.live else {
            self.messages.write().push(message);
            return Ok(());
        };
        forbid_on_event_loop(py, "send", "asend")?;
        let frame = to_frame(message)?;
        let outbound = live.outbound.clone();
        if !self.connected()
            || py
                .allow_threads(move || outbound.blocking_send(frame))
                .is_err()
        {
            return Err(closed_error());
        }
        Ok(())
    }

    /// Awaitable `send`.
    pub fn asend<'py>(&self, py: Python<'py>, message: WebSocketMessage) -> PyResult<&'py PyAny> {
        let Some(live) = &self.live else {
            self.messages.write().push(message);
            return crate::async_bridge::ready(py, py.None());
        };
        let frame = to_frame(message)?;
        let outbound = live.outbound.clone();
        let connected = self.connected.clone();
        crate::async_bridge::future_into_py(py, async move {
            if !connected.load(Ordering::Relaxed) || outbound.send(frame).await.is_err() {
                return Err(closed_error());
            }
            Ok(())
        })
    }

    /// Awaitable `send_text`.
    pub fn asend_text<'py>(&self, py: Python<'py>, text: &str) -> PyResult<&'py PyAny> {
        self.asend(py, WebSocketMessage::from_text(text))
    }

    /// Awaitable `send_binary`.
    pub fn asend_binary<'py>(&self, py: Python<'py>, data: Vec<u8>) -> PyResult<&'py PyAny> {
        self.asend(py, WebSocketMessage::from_binary(data))
    }

    /// Get queued messages (offline sockets, for testing).
    pub fn get_queued_messages(&self) -> Vec<WebSocketMessage> {
        self.messages.read().clone()
    }

    /// Close the WebSocket connection (never blocks).
    pub fn close(&self) -> PyResult<()> {
        match &self.live {
            Some(live) => {
                let _ = live.outbound.try_send(Message::Close(None));
            }
            None => self.messages.write().push(WebSocketMessage::close()),
        }
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }
}

impl Default for WebSocket {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a request asks for a WebSocket upgrade.
pub fn is_upgrade_request(headers: &hyper::HeaderMap) -> bool {
    let has_token = |name: hyper::header::HeaderName, token: &str| {
        headers.get_all(name).iter().any(|v| {
            v.to_str()
                .map(|s| s.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
                .unwrap_or(false)
        })
    };
    has_token(hyper::header::UPGRADE, "websocket")
        && has_token(hyper::header::CONNECTION, "upgrade")
}

/// Whether a handshake's `Origin` may open this socket.
///
/// Browsers always send `Origin` on a WebSocket handshake and attach the
/// site's cookies (unless `SameSite` stops them), so a cross-site page could
/// otherwise open a cookie-authenticated socket on the user's behalf. Without
/// an `Origin` the client is not a browser and carries no ambient
/// credentials. A browser handshake is accepted when it is same-origin (the
/// `Origin` authority equals `host`) or its origin is in `allowed` (`"*"`
/// allows any).
pub fn origin_allowed(origin: Option<&str>, host: Option<&str>, allowed: &[String]) -> bool {
    let Some(origin) = origin.map(str::trim) else {
        return true;
    };
    if allowed
        .iter()
        .any(|o| o == "*" || o.trim_end_matches('/').eq_ignore_ascii_case(origin))
    {
        return true;
    }
    let authority = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"));
    match (authority, host) {
        (Some(a), Some(h)) => !h.is_empty() && a.eq_ignore_ascii_case(h.trim()),
        _ => false,
    }
}

/// Frames a handler may queue ahead of a slow client before `send` waits.
const OUTBOUND_QUEUE: usize = 64;

/// Bytes of client messages queued ahead of the handler before the
/// connection stops reading. A single larger message (up to
/// `MAX_MESSAGE_SIZE`) is still accepted, one at a time.
const INBOUND_BUDGET: usize = 4 * 1024 * 1024;

/// Accounting overhead charged per queued message, so a flood of empty frames
/// is bounded too.
const MESSAGE_OVERHEAD: usize = 64;

/// Largest message (and frame) accepted from a client.
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Budget permits a message takes.
fn budget_cost(msg: Option<&WebSocketMessage>) -> u32 {
    let Some(msg) = msg else { return 0 };
    let len = msg.text.as_ref().map_or(0, String::len) + msg.data.as_ref().map_or(0, Vec::len);
    (len + MESSAGE_OVERHEAD).min(INBOUND_BUDGET) as u32
}

/// Drive an upgraded connection: run the Python handler and pump frames
/// between the socket and the handler's `WebSocket` object until either side
/// is done. A `def` handler runs on the blocking pool (one pool thread for the
/// life of the socket); an `async def` handler runs on the application's
/// persistent event loop and holds no thread while it awaits.
pub async fn serve_socket(upgraded: hyper::upgrade::Upgraded, handler: PyObject, request: Request) {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};

    let config = WebSocketConfig {
        max_message_size: Some(MAX_MESSAGE_SIZE),
        max_frame_size: Some(MAX_MESSAGE_SIZE),
        ..Default::default()
    };
    let stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
        hyper_util::rt::TokioIo::new(upgraded),
        Role::Server,
        Some(config),
    )
    .await;
    let (mut sink, mut frames) = stream.split();

    let (in_tx, in_rx) = tokio::sync::mpsc::unbounded_channel::<Inbound>();
    let budget = Arc::new(tokio::sync::Semaphore::new(INBOUND_BUDGET));
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(OUTBOUND_QUEUE);
    let connected = Arc::new(AtomicBool::new(true));
    let socket = WebSocket {
        connected: connected.clone(),
        messages: Arc::new(RwLock::new(Vec::new())),
        live: Some(LiveSocket {
            inbound: Arc::new(tokio::sync::Mutex::new(in_rx)),
            outbound: out_tx,
            runtime: tokio::runtime::Handle::current(),
        }),
        request: Some(request),
    };

    let handler_task = match start_handler(handler, socket) {
        Ok(task) => task,
        Err(e) => {
            eprintln!("[cello] WebSocket handler error: {e}");
            let _ = sink.send(Message::Close(None)).await;
            return;
        }
    };
    tokio::pin!(handler_task);

    let mut in_tx = Some(in_tx);
    // A message read from the socket that waits for budget. While it waits,
    // the socket is not read.
    let mut pending: Option<WebSocketMessage> = None;
    let deliver = |in_tx: &mut Option<tokio::sync::mpsc::UnboundedSender<Inbound>>,
                   msg: WebSocketMessage,
                   permit: tokio::sync::OwnedSemaphorePermit| {
        if let Some(tx) = in_tx.as_ref() {
            if tx.send((msg, permit)).is_err() {
                // The handler dropped its socket; stop queueing.
                *in_tx = None;
            }
        }
    };
    loop {
        tokio::select! {
            permit = budget.clone().acquire_many_owned(budget_cost(pending.as_ref())), if pending.is_some() => {
                let msg = pending.take().expect("guarded by pending.is_some()");
                match permit {
                    Ok(permit) => deliver(&mut in_tx, msg, permit),
                    Err(_) => break,
                }
            }
            frame = frames.next(), if pending.is_none() => {
                let msg = match frame {
                    Some(Ok(Message::Text(t))) => WebSocketMessage::from_text(&t),
                    Some(Ok(Message::Binary(b))) => WebSocketMessage::from_binary(b),
                    Some(Ok(Message::Ping(_))) => {
                        // tungstenite queued the Pong itself; push it out now.
                        let _ = sink.flush().await;
                        continue;
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Frame(_))) => continue,
                    // Client closed (or the connection failed): `recv()` now
                    // returns None and later sends raise.
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                };
                if in_tx.is_none() {
                    continue;
                }
                match budget.clone().try_acquire_many_owned(budget_cost(Some(&msg))) {
                    Ok(permit) => deliver(&mut in_tx, msg, permit),
                    Err(_) => pending = Some(msg),
                }
            }
            out = out_rx.recv() => match out {
                Some(msg) => {
                    let closing = matches!(msg, Message::Close(_));
                    if sink.send(msg).await.is_err() || closing {
                        break;
                    }
                }
                None => break,
            },
            result = &mut handler_task => {
                if let Err(e) = result {
                    eprintln!("[cello] WebSocket handler error: {e}");
                }
                // Flush what the handler sent before returning, then close.
                while let Ok(msg) = out_rx.try_recv() {
                    if sink.send(msg).await.is_err() {
                        break;
                    }
                }
                let _ = sink.send(Message::Close(None)).await;
                break;
            }
        }
    }
    connected.store(false, Ordering::Relaxed);
    in_tx.take();
    budget.close();
    let _ = sink.close().await;
}

/// Resolves when a WebSocket handler returns (`Err` carries its error).
type HandlerDone = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// Start the handler: `async def` on the persistent loop, `def` on the
/// blocking pool. The future resolves when the handler returns.
fn start_handler(handler: PyObject, socket: WebSocket) -> PyResult<HandlerDone> {
    Python::with_gil(|py| {
        let is_async = py
            .import("inspect")?
            .call_method1("iscoroutinefunction", (handler.as_ref(py),))?
            .is_true()?;
        if is_async {
            let ws = Py::new(py, socket)?;
            let coro = handler.call1(py, (ws,))?;
            let (tx, rx) = tokio::sync::oneshot::channel();
            crate::async_loop::submit_coroutine(
                py,
                coro.as_ref(py),
                Box::new(move |_py, outcome| {
                    let _ = tx.send(outcome.map(|_| ()).map_err(|e| e.to_string()));
                }),
            )?;
            let done: HandlerDone = Box::pin(async move {
                rx.await
                    .unwrap_or_else(|_| Err("handler task dropped".to_string()))
            });
            return Ok(done);
        }
        let task = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| -> PyResult<()> {
                let ws = Py::new(py, socket)?;
                let result = handler.call1(py, (ws,))?;
                if result.as_ref(py).hasattr("__await__")? {
                    // A sync callable that returned an awaitable: run it on the
                    // persistent loop rather than a private one.
                    crate::async_loop::run_coroutine_blocking(py, result.as_ref(py))
                        .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
                }
                Ok(())
            })
        });
        let done: HandlerDone = Box::pin(async move {
            match task.await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("panicked: {e}")),
            }
        });
        Ok(done)
    })
}

/// A registered WebSocket endpoint.
#[derive(Clone)]
pub struct WebSocketRoute {
    pub handler: PyObject,
    /// Cross-origin browser origins allowed besides same-origin (`"*"` = any).
    pub allowed_origins: Arc<Vec<String>>,
}

/// WebSocket handler registry.
pub struct WebSocketRegistry {
    handlers: Arc<RwLock<HashMap<String, WebSocketRoute>>>,
}

impl WebSocketRegistry {
    pub fn new() -> Self {
        WebSocketRegistry {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, path: &str, handler: PyObject, allowed_origins: Vec<String>) {
        self.handlers.write().insert(
            path.to_string(),
            WebSocketRoute {
                handler,
                allowed_origins: Arc::new(allowed_origins),
            },
        );
    }

    pub fn get(&self, path: &str) -> Option<WebSocketRoute> {
        self.handlers.read().get(path).cloned()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.handlers.read().contains_key(path)
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.read().is_empty()
    }
}

impl Default for WebSocketRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for WebSocketRegistry {
    fn clone(&self) -> Self {
        WebSocketRegistry {
            handlers: self.handlers.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_message_text() {
        let msg = WebSocketMessage::from_text("Hello");
        assert!(msg.is_text());
        assert!(!msg.is_binary());
        assert_eq!(msg.text, Some("Hello".to_string()));
    }

    #[test]
    fn test_websocket_message_binary() {
        let msg = WebSocketMessage::from_binary(vec![1, 2, 3]);
        assert!(msg.is_binary());
        assert!(!msg.is_text());
    }

    #[test]
    fn origin_check() {
        let none: Vec<String> = Vec::new();
        // Non-browser client.
        assert!(origin_allowed(None, Some("example.com"), &none));
        // Same-origin.
        assert!(origin_allowed(
            Some("https://example.com"),
            Some("example.com"),
            &none
        ));
        assert!(origin_allowed(
            Some("http://localhost:8000"),
            Some("localhost:8000"),
            &none
        ));
        // Cross-site.
        assert!(!origin_allowed(
            Some("https://evil.com"),
            Some("example.com"),
            &none
        ));
        assert!(!origin_allowed(
            Some("https://example.com.evil.com"),
            Some("example.com"),
            &none
        ));
        assert!(!origin_allowed(Some("null"), Some("example.com"), &none));
        assert!(!origin_allowed(Some("https://example.com"), None, &none));
        // Allow-list.
        let list = vec!["https://app.example.com".to_string()];
        assert!(origin_allowed(
            Some("https://app.example.com"),
            Some("api.example.com"),
            &list
        ));
        assert!(!origin_allowed(
            Some("https://other.example.com"),
            Some("api.example.com"),
            &list
        ));
        assert!(origin_allowed(
            Some("https://x.test"),
            Some("y"),
            &["*".to_string()]
        ));
    }

    #[test]
    fn test_websocket_registry() {
        let registry = WebSocketRegistry::new();
        assert!(!registry.contains("/ws"));
    }
}
