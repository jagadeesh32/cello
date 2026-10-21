//! Response bodies: an in-memory buffer, or a file streamed from disk.
//!
//! Most responses are a single buffer (`Full`). A large file (static files,
//! `Response.sendfile`, `Response.file_range`) is streamed in fixed-size chunks
//! instead, so serving it costs one chunk of memory per connection rather than
//! the whole file.

use bytes::Bytes;
use futures_core::Stream;
use http_body_util::{Either, Full};
use hyper::body::{Body, Frame, SizeHint};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_util::io::ReaderStream;

/// The body type every response is built with.
pub type ResBody = Either<Full<Bytes>, FileBody>;

/// Wrap a buffer.
#[inline]
pub fn full(bytes: impl Into<Bytes>) -> ResBody {
    Either::Left(Full::new(bytes.into()))
}

/// Files at most this large are read into memory (so compression, ETag and
/// caching middleware see the bytes); larger ones are streamed.
pub const STREAM_THRESHOLD: u64 = 1024 * 1024;

/// Read size per chunk when streaming.
const CHUNK: usize = 64 * 1024;

/// A file (or a byte range of one) streamed in `CHUNK`-sized frames. The exact
/// length is known up front, so hyper sends a `Content-Length`.
pub struct FileBody {
    stream: ReaderStream<tokio::io::Take<tokio::fs::File>>,
    remaining: u64,
}

impl FileBody {
    /// Stream `len` bytes of `file` from its current position.
    pub fn new(file: tokio::fs::File, len: u64) -> Self {
        use tokio::io::AsyncReadExt;
        FileBody {
            stream: ReaderStream::with_capacity(file.take(len), CHUNK),
            remaining: len,
        }
    }

    /// Bytes still to be sent.
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
}

impl Body for FileBody {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        if self.remaining == 0 {
            return Poll::Ready(None);
        }
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.remaining = self.remaining.saturating_sub(chunk.len() as u64);
                Poll::Ready(Some(Ok(Frame::data(chunk))))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            // The file shrank after its length was taken: fail the body rather
            // than end it short of the declared `Content-Length`.
            Poll::Ready(None) => Poll::Ready(Some(Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "file truncated while streaming",
            )))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.remaining == 0
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.remaining)
    }
}
