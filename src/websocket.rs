//! WebSocket support for Cello.
//!
//! Provides WebSocket handling using tokio-tungstenite.

use parking_lot::RwLock;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

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

/// WebSocket connection handler for Python.
/// 
/// Note: This is a placeholder for the WebSocket API.
/// Full WebSocket support requires protocol upgrade handling.
#[pyclass]
pub struct WebSocket {
    /// Connection state
    #[pyo3(get)]
    pub connected: bool,
    
    /// Internal message queue (simulated)
    messages: Arc<RwLock<Vec<WebSocketMessage>>>,
}

#[pymethods]
impl WebSocket {
    /// Create a new WebSocket (for testing/mocking).
    #[new]
    pub fn new() -> Self {
        WebSocket {
            connected: true,
            messages: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Send a text message (queues for sending).
    pub fn send_text(&self, text: &str) -> PyResult<()> {
        let msg = WebSocketMessage::from_text(text);
        self.messages.write().push(msg);
        Ok(())
    }

    /// Send a binary message (queues for sending).
    pub fn send_binary(&self, data: Vec<u8>) -> PyResult<()> {
        let msg = WebSocketMessage::from_binary(data);
        self.messages.write().push(msg);
        Ok(())
    }

    /// Send a message (queues for sending).
    pub fn send(&self, message: WebSocketMessage) -> PyResult<()> {
        self.messages.write().push(message);
        Ok(())
    }

    /// Get queued messages (for testing).
    pub fn get_queued_messages(&self) -> Vec<WebSocketMessage> {
        self.messages.read().clone()
    }

    /// Close the WebSocket connection.
    pub fn close(&self) -> PyResult<()> {
        self.messages.write().push(WebSocketMessage::close());
        Ok(())
    }
}

impl Default for WebSocket {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket handler registry.
pub struct WebSocketRegistry {
    handlers: Arc<RwLock<HashMap<String, PyObject>>>,
}

impl WebSocketRegistry {
    pub fn new() -> Self {
        WebSocketRegistry {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, path: &str, handler: PyObject) {
        self.handlers.write().insert(path.to_string(), handler);
    }

    pub fn get(&self, path: &str) -> Option<PyObject> {
        self.handlers.read().get(path).cloned()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.handlers.read().contains_key(path)
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
    fn test_websocket_registry() {
        let registry = WebSocketRegistry::new();
        assert!(!registry.contains("/ws"));
    }
}
