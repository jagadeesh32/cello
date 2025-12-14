//! Server-Sent Events (SSE) support for Cello.
//!
//! Provides streaming event support for real-time updates.

use bytes::Bytes;
use futures_util::stream::{self, Stream};
use pyo3::prelude::*;

/// SSE Event structure.
#[pyclass]
#[derive(Clone, Debug)]
pub struct SseEvent {
    /// Event type (optional)
    #[pyo3(get)]
    pub event: Option<String>,
    
    /// Event data (required)
    #[pyo3(get)]
    pub data: String,
    
    /// Event ID (optional)
    #[pyo3(get)]
    pub id: Option<String>,
    
    /// Retry interval in milliseconds (optional)
    #[pyo3(get)]
    pub retry: Option<u32>,
}

#[pymethods]
impl SseEvent {
    /// Create a new SSE event with data.
    #[new]
    #[pyo3(signature = (data, event=None, id=None, retry=None))]
    pub fn new(
        data: &str,
        event: Option<&str>,
        id: Option<&str>,
        retry: Option<u32>,
    ) -> Self {
        SseEvent {
            event: event.map(|s| s.to_string()),
            data: data.to_string(),
            id: id.map(|s| s.to_string()),
            retry,
        }
    }

    /// Create a simple data-only event (alias for new constructor).
    #[staticmethod]
    #[pyo3(name = "data")]
    pub fn from_data(content: &str) -> Self {
        SseEvent {
            event: None,
            data: content.to_string(),
            id: None,
            retry: None,
        }
    }

    /// Create an event with type and data.
    #[staticmethod]
    #[pyo3(name = "with_event")]
    pub fn from_event(event_type: &str, content: &str) -> Self {
        SseEvent {
            event: Some(event_type.to_string()),
            data: content.to_string(),
            id: None,
            retry: None,
        }
    }

    /// Format the event as SSE text.
    pub fn to_sse_string(&self) -> String {
        let mut result = String::new();
        
        if let Some(ref id) = self.id {
            result.push_str(&format!("id: {}\n", id));
        }
        
        if let Some(ref event) = self.event {
            result.push_str(&format!("event: {}\n", event));
        }
        
        if let Some(retry) = self.retry {
            result.push_str(&format!("retry: {}\n", retry));
        }
        
        // Data can be multi-line, each line prefixed with "data: "
        for line in self.data.lines() {
            result.push_str(&format!("data: {}\n", line));
        }
        
        // Empty line to end the event
        result.push('\n');
        
        result
    }
}

impl SseEvent {
    /// Convert to bytes for streaming.
    pub fn to_bytes(&self) -> Bytes {
        Bytes::from(self.to_sse_string())
    }
}

/// SSE stream builder for creating event streams.
#[pyclass]
pub struct SseStream {
    /// Events to stream
    events: Vec<SseEvent>,
}

#[pymethods]
impl SseStream {
    /// Create a new empty SSE stream.
    #[new]
    pub fn new() -> Self {
        SseStream { events: Vec::new() }
    }

    /// Add an event to the stream.
    pub fn add(&mut self, event: SseEvent) {
        self.events.push(event);
    }

    /// Add a data-only event.
    pub fn add_data(&mut self, data: &str) {
        self.events.push(SseEvent::from_data(data));
    }

    /// Add an event with type and data.
    pub fn add_event(&mut self, event_type: &str, data: &str) {
        self.events.push(SseEvent::from_event(event_type, data));
    }

    /// Get the number of events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if the stream is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for SseStream {
    fn default() -> Self {
        Self::new()
    }
}

impl SseStream {
    /// Get all events.
    pub fn events(&self) -> &[SseEvent] {
        &self.events
    }

    /// Convert to a stream of bytes for HTTP response.
    pub fn into_byte_stream(self) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
        let events: Vec<_> = self.events.into_iter().map(|e| Ok(e.to_bytes())).collect();
        stream::iter(events)
    }
}

/// Keep-alive event for SSE connections.
pub fn sse_keepalive() -> SseEvent {
    SseEvent {
        event: None,
        data: "".to_string(),
        id: None,
        retry: None,
    }
}

/// Create SSE headers for HTTP response.
pub fn sse_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Content-Type", "text/event-stream"),
        ("Cache-Control", "no-cache"),
        ("Connection", "keep-alive"),
        ("X-Accel-Buffering", "no"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_simple() {
        let event = SseEvent::from_data("Hello, World!");
        let sse_str = event.to_sse_string();
        assert!(sse_str.contains("data: Hello, World!"));
        assert!(sse_str.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_event_with_type() {
        let event = SseEvent::from_event("message", "Test data");
        let sse_str = event.to_sse_string();
        assert!(sse_str.contains("event: message"));
        assert!(sse_str.contains("data: Test data"));
    }

    #[test]
    fn test_sse_event_full() {
        let event = SseEvent::new("data", Some("update"), Some("123"), Some(3000));
        let sse_str = event.to_sse_string();
        assert!(sse_str.contains("id: 123"));
        assert!(sse_str.contains("event: update"));
        assert!(sse_str.contains("retry: 3000"));
        assert!(sse_str.contains("data: data"));
    }

    #[test]
    fn test_sse_event_multiline() {
        let event = SseEvent::from_data("Line 1\nLine 2\nLine 3");
        let sse_str = event.to_sse_string();
        assert!(sse_str.contains("data: Line 1\n"));
        assert!(sse_str.contains("data: Line 2\n"));
        assert!(sse_str.contains("data: Line 3\n"));
    }

    #[test]
    fn test_sse_stream() {
        let mut stream = SseStream::new();
        stream.add_data("event1");
        stream.add_event("update", "event2");
        
        assert_eq!(stream.len(), 2);
        assert!(!stream.is_empty());
    }
}
