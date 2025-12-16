//! Streaming response support for Cello.
//!
//! Provides:
//! - Chunked transfer encoding
//! - Server-Sent Events helpers
//! - Backpressure-aware streaming
//! - File body with sendfile support

use std::collections::VecDeque;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use parking_lot::Mutex;
use tokio::sync::mpsc;

// ============================================================================
// Stream Item
// ============================================================================

/// A single item in a stream.
#[derive(Clone, Debug)]
pub enum StreamItem {
    /// Chunk of data
    Data(Bytes),
    /// End of stream
    End,
    /// Error occurred
    Error(String),
}

impl StreamItem {
    /// Create data chunk from bytes.
    pub fn data(data: impl Into<Bytes>) -> Self {
        StreamItem::Data(data.into())
    }

    /// Create data chunk from string.
    pub fn text(text: &str) -> Self {
        StreamItem::Data(Bytes::copy_from_slice(text.as_bytes()))
    }

    /// Create end marker.
    pub fn end() -> Self {
        StreamItem::End
    }

    /// Create error item.
    pub fn error(msg: &str) -> Self {
        StreamItem::Error(msg.to_string())
    }

    /// Check if this is the end marker.
    pub fn is_end(&self) -> bool {
        matches!(self, StreamItem::End)
    }

    /// Check if this is an error.
    pub fn is_error(&self) -> bool {
        matches!(self, StreamItem::Error(_))
    }

    /// Get data if this is a data chunk.
    pub fn into_data(self) -> Option<Bytes> {
        match self {
            StreamItem::Data(data) => Some(data),
            _ => None,
        }
    }
}

// ============================================================================
// Streaming Response
// ============================================================================

/// A streaming response with backpressure support.
pub struct StreamingResponse {
    /// Receiver for stream items
    receiver: mpsc::Receiver<StreamItem>,
    /// Sender for producing items (kept for ownership)
    #[allow(dead_code)]
    sender: Option<mpsc::Sender<StreamItem>>,
    /// Whether the stream is complete
    complete: Arc<AtomicBool>,
    /// Total bytes sent
    bytes_sent: Arc<AtomicU64>,
    /// Content type
    content_type: String,
    /// HTTP status
    status: u16,
}

impl StreamingResponse {
    /// Create a new streaming response.
    pub fn new(buffer_size: usize) -> (Self, StreamProducer) {
        let (sender, receiver) = mpsc::channel(buffer_size);
        let complete = Arc::new(AtomicBool::new(false));
        let bytes_sent = Arc::new(AtomicU64::new(0));

        let response = Self {
            receiver,
            sender: Some(sender.clone()),
            complete: complete.clone(),
            bytes_sent: bytes_sent.clone(),
            content_type: "application/octet-stream".to_string(),
            status: 200,
        };

        let producer = StreamProducer {
            sender,
            complete,
            bytes_sent,
        };

        (response, producer)
    }

    /// Create with default buffer size.
    pub fn default() -> (Self, StreamProducer) {
        Self::new(16)
    }

    /// Set content type.
    pub fn content_type(mut self, content_type: &str) -> Self {
        self.content_type = content_type.to_string();
        self
    }

    /// Set status code.
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// Get content type.
    pub fn get_content_type(&self) -> &str {
        &self.content_type
    }

    /// Get status.
    pub fn get_status(&self) -> u16 {
        self.status
    }

    /// Check if stream is complete.
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::SeqCst)
    }

    /// Get total bytes sent.
    pub fn total_bytes(&self) -> u64 {
        self.bytes_sent.load(Ordering::SeqCst)
    }
}

impl Stream for StreamingResponse {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.receiver).poll_recv(cx) {
            Poll::Ready(Some(item)) => match item {
                StreamItem::Data(data) => {
                    self.bytes_sent.fetch_add(data.len() as u64, Ordering::SeqCst);
                    Poll::Ready(Some(Ok(data)))
                }
                StreamItem::End => {
                    self.complete.store(true, Ordering::SeqCst);
                    Poll::Ready(None)
                }
                StreamItem::Error(msg) => {
                    self.complete.store(true, Ordering::SeqCst);
                    Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        msg,
                    ))))
                }
            },
            Poll::Ready(None) => {
                self.complete.store(true, Ordering::SeqCst);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Producer handle for streaming response.
pub struct StreamProducer {
    sender: mpsc::Sender<StreamItem>,
    complete: Arc<AtomicBool>,
    bytes_sent: Arc<AtomicU64>,
}

impl StreamProducer {
    /// Send a chunk of data.
    pub async fn send(&self, data: impl Into<Bytes>) -> Result<(), StreamError> {
        if self.complete.load(Ordering::SeqCst) {
            return Err(StreamError::Closed);
        }

        self.sender
            .send(StreamItem::Data(data.into()))
            .await
            .map_err(|_| StreamError::Closed)
    }

    /// Send text data.
    pub async fn send_text(&self, text: &str) -> Result<(), StreamError> {
        self.send(Bytes::copy_from_slice(text.as_bytes())).await
    }

    /// Send and complete the stream.
    pub async fn finish(self) -> Result<(), StreamError> {
        self.sender
            .send(StreamItem::End)
            .await
            .map_err(|_| StreamError::Closed)?;
        self.complete.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Signal an error and close the stream.
    pub async fn error(self, message: &str) -> Result<(), StreamError> {
        self.sender
            .send(StreamItem::Error(message.to_string()))
            .await
            .map_err(|_| StreamError::Closed)?;
        self.complete.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Check if the stream is still open.
    pub fn is_open(&self) -> bool {
        !self.complete.load(Ordering::SeqCst)
    }

    /// Get total bytes sent so far.
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::SeqCst)
    }
}

/// Stream producer error.
#[derive(Debug, Clone)]
pub enum StreamError {
    /// Stream was closed
    Closed,
    /// Buffer is full (backpressure)
    BufferFull,
    /// Other error
    Other(String),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Closed => write!(f, "Stream closed"),
            StreamError::BufferFull => write!(f, "Buffer full"),
            StreamError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for StreamError {}

// ============================================================================
// Chunked Body
// ============================================================================

/// A chunked transfer encoding body.
pub struct ChunkedBody {
    chunks: Arc<Mutex<VecDeque<Bytes>>>,
    complete: Arc<AtomicBool>,
}

impl ChunkedBody {
    /// Create a new chunked body.
    pub fn new() -> (Self, ChunkedWriter) {
        let chunks = Arc::new(Mutex::new(VecDeque::new()));
        let complete = Arc::new(AtomicBool::new(false));

        let body = Self {
            chunks: chunks.clone(),
            complete: complete.clone(),
        };

        let writer = ChunkedWriter { chunks, complete };

        (body, writer)
    }

    /// Get next chunk.
    pub fn next_chunk(&self) -> Option<Bytes> {
        self.chunks.lock().pop_front()
    }

    /// Check if body is complete.
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::SeqCst) && self.chunks.lock().is_empty()
    }

    /// Check if there are pending chunks.
    pub fn has_chunks(&self) -> bool {
        !self.chunks.lock().is_empty()
    }
}

impl Default for ChunkedBody {
    fn default() -> Self {
        Self::new().0
    }
}

/// Writer handle for chunked body.
pub struct ChunkedWriter {
    chunks: Arc<Mutex<VecDeque<Bytes>>>,
    complete: Arc<AtomicBool>,
}

impl ChunkedWriter {
    /// Write a chunk.
    pub fn write(&self, data: impl Into<Bytes>) {
        if !self.complete.load(Ordering::SeqCst) {
            self.chunks.lock().push_back(data.into());
        }
    }

    /// Write text chunk.
    pub fn write_text(&self, text: &str) {
        self.write(Bytes::copy_from_slice(text.as_bytes()));
    }

    /// Finish writing.
    pub fn finish(self) {
        self.complete.store(true, Ordering::SeqCst);
    }
}

// ============================================================================
// File Body
// ============================================================================

/// A file body for zero-copy sendfile responses.
#[derive(Clone, Debug)]
pub struct FileBody {
    /// Path to the file
    pub path: PathBuf,
    /// Start offset
    pub offset: u64,
    /// Length to send (None = entire file from offset)
    pub length: Option<u64>,
    /// Content type
    pub content_type: String,
}

impl FileBody {
    /// Create a new file body.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path: PathBuf = path.into();
        let content_type = mime_guess::from_path(&path)
            .first_or_octet_stream()
            .to_string();

        Self {
            path,
            offset: 0,
            length: None,
            content_type,
        }
    }

    /// Set offset for range requests.
    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = offset;
        self
    }

    /// Set length for range requests.
    pub fn length(mut self, length: u64) -> Self {
        self.length = Some(length);
        self
    }

    /// Set content type.
    pub fn content_type(mut self, content_type: &str) -> Self {
        self.content_type = content_type.to_string();
        self
    }

    /// Get the file size.
    pub fn file_size(&self) -> std::io::Result<u64> {
        Ok(std::fs::metadata(&self.path)?.len())
    }

    /// Get the content length to send.
    pub fn content_length(&self) -> std::io::Result<u64> {
        let file_size = self.file_size()?;
        let remaining = file_size.saturating_sub(self.offset);
        Ok(self.length.unwrap_or(remaining).min(remaining))
    }

    /// Check if the file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

// ============================================================================
// Server-Sent Events Helpers
// ============================================================================

/// Format data for Server-Sent Events.
pub fn sse_event(event: Option<&str>, data: &str, id: Option<&str>) -> String {
    let mut output = String::new();

    if let Some(event_name) = event {
        output.push_str(&format!("event: {}\n", event_name));
    }

    if let Some(event_id) = id {
        output.push_str(&format!("id: {}\n", event_id));
    }

    // Data can be multi-line
    for line in data.lines() {
        output.push_str(&format!("data: {}\n", line));
    }

    output.push('\n');
    output
}

/// Format JSON data for Server-Sent Events.
pub fn sse_json(event: Option<&str>, data: &serde_json::Value, id: Option<&str>) -> String {
    let json_str = serde_json::to_string(data).unwrap_or_default();
    sse_event(event, &json_str, id)
}

/// Format a retry directive for SSE.
pub fn sse_retry(milliseconds: u64) -> String {
    format!("retry: {}\n\n", milliseconds)
}

/// Format a comment for SSE (used as keepalive).
pub fn sse_comment(comment: &str) -> String {
    format!(": {}\n\n", comment)
}

// ============================================================================
// Streaming Helpers
// ============================================================================

/// Create a streaming response from an async iterator.
pub async fn stream_from_iter<I, T, E>(iter: I, buffer_size: usize) -> StreamingResponse
where
    I: IntoIterator<Item = Result<T, E>> + Send + 'static,
    I::IntoIter: Send,
    T: Into<Bytes> + Send,
    E: std::fmt::Display + Send,
{
    let (response, producer) = StreamingResponse::new(buffer_size);

    tokio::spawn(async move {
        for item in iter {
            match item {
                Ok(data) => {
                    if producer.send(data.into()).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = producer.error(&e.to_string()).await;
                    return;
                }
            }
        }
        let _ = producer.finish().await;
    });

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_item() {
        let item = StreamItem::data(vec![1, 2, 3]);
        assert!(!item.is_end());
        assert!(!item.is_error());

        let end = StreamItem::end();
        assert!(end.is_end());

        let error = StreamItem::error("test error");
        assert!(error.is_error());
    }

    #[test]
    fn test_sse_formatting() {
        let event = sse_event(Some("message"), "Hello, World!", Some("1"));
        assert!(event.contains("event: message\n"));
        assert!(event.contains("id: 1\n"));
        assert!(event.contains("data: Hello, World!\n"));
        assert!(event.ends_with("\n\n"));

        let retry = sse_retry(3000);
        assert_eq!(retry, "retry: 3000\n\n");

        let comment = sse_comment("keepalive");
        assert_eq!(comment, ": keepalive\n\n");
    }

    #[test]
    fn test_file_body() {
        let body = FileBody::new("/tmp/test.txt")
            .offset(100)
            .length(500)
            .content_type("text/plain");

        assert_eq!(body.offset, 100);
        assert_eq!(body.length, Some(500));
        assert_eq!(body.content_type, "text/plain");
    }

    #[tokio::test]
    async fn test_streaming_response() {
        let (mut response, producer) = StreamingResponse::new(4);

        tokio::spawn(async move {
            producer.send_text("chunk1").await.unwrap();
            producer.send_text("chunk2").await.unwrap();
            producer.finish().await.unwrap();
        });

        use futures_util::StreamExt;

        let mut chunks = Vec::new();
        while let Some(result) = response.next().await {
            if let Ok(data) = result {
                chunks.push(data);
            }
        }

        assert_eq!(chunks.len(), 2);
        assert!(response.is_complete());
    }
}
