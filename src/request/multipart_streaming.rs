//! Streaming multipart parser for Cello.
//!
//! Provides:
//! - Memory-efficient multipart parsing
//! - Streaming file uploads
//! - Part-by-part iteration

use bytes::{Buf, Bytes, BytesMut};
use std::collections::HashMap;
use tokio::io::AsyncRead;

// ============================================================================
// Multipart Part
// ============================================================================

/// A single part of a multipart request.
#[derive(Debug, Clone)]
pub struct MultipartPart {
    /// Content-Disposition name
    pub name: String,
    /// Filename (if file upload)
    pub filename: Option<String>,
    /// Content-Type
    pub content_type: Option<String>,
    /// Headers for this part
    pub headers: HashMap<String, String>,
    /// Part data (for buffered parsing)
    pub data: Vec<u8>,
}

impl MultipartPart {
    /// Create new part.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            filename: None,
            content_type: None,
            headers: HashMap::new(),
            data: Vec::new(),
        }
    }

    /// Check if this is a file upload.
    pub fn is_file(&self) -> bool {
        self.filename.is_some()
    }

    /// Get data as string.
    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.data)
    }

    /// Get data as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Get extension from filename.
    pub fn extension(&self) -> Option<String> {
        self.filename
            .as_ref()
            .and_then(|f| f.rsplit('.').next())
            .map(|e| e.to_lowercase())
    }

    /// Get the size of the data.
    pub fn size(&self) -> usize {
        self.data.len()
    }
}

// ============================================================================
// Streaming Multipart Parser
// ============================================================================

/// State of the multipart parser.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ParserState {
    /// Looking for first boundary
    Preamble,
    /// Reading part headers
    Headers,
    /// Reading part body
    Body,
    /// Finished parsing
    Done,
}

/// Streaming multipart parser.
pub struct StreamingMultipart {
    boundary: Vec<u8>,
    state: ParserState,
    buffer: BytesMut,
    current_part: Option<MultipartPart>,
    parts: Vec<MultipartPart>,
}

impl StreamingMultipart {
    /// Create new streaming multipart parser.
    pub fn new(boundary: &str) -> Self {
        Self {
            boundary: format!("--{}", boundary).into_bytes(),
            state: ParserState::Preamble,
            buffer: BytesMut::new(),
            current_part: None,
            parts: Vec::new(),
        }
    }

    /// Create from Content-Type header.
    pub fn from_content_type(content_type: &str) -> Option<Self> {
        let boundary = content_type
            .split("boundary=")
            .nth(1)?
            .trim_matches('"')
            .to_string();
        Some(Self::new(&boundary))
    }

    /// Feed data to the parser.
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.parse();
    }

    /// Parse buffered data.
    fn parse(&mut self) {
        loop {
            match self.state {
                ParserState::Preamble => {
                    if !self.skip_to_boundary() {
                        break;
                    }
                    self.state = ParserState::Headers;
                }
                ParserState::Headers => {
                    if !self.parse_headers() {
                        break;
                    }
                    self.state = ParserState::Body;
                }
                ParserState::Body => {
                    if !self.parse_body() {
                        break;
                    }
                    // After body, we either go to headers (next part) or done
                    if self.check_final_boundary() {
                        self.state = ParserState::Done;
                        break;
                    }
                    self.state = ParserState::Headers;
                }
                ParserState::Done => break,
            }
        }
    }

    /// Skip to first boundary.
    fn skip_to_boundary(&mut self) -> bool {
        if let Some(pos) = self.find_boundary() {
            // Skip boundary and CRLF
            let skip_len = pos + self.boundary.len();
            if self.buffer.len() > skip_len + 2 {
                self.buffer.advance(skip_len);
                // Skip CRLF after boundary
                if self.buffer.starts_with(b"\r\n") {
                    self.buffer.advance(2);
                }
                return true;
            }
        }
        false
    }

    /// Parse headers for current part.
    fn parse_headers(&mut self) -> bool {
        // Find end of headers (blank line)
        let header_end = self.find_header_end();
        if header_end.is_none() {
            return false;
        }
        let header_end = header_end.unwrap();

        let header_bytes = self.buffer.split_to(header_end);
        // Skip CRLF CRLF
        self.buffer.advance(4);

        let header_str = String::from_utf8_lossy(&header_bytes);
        let mut part = MultipartPart::new("");

        for line in header_str.lines() {
            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim().to_lowercase();
                let value = line[colon_pos + 1..].trim();

                if name == "content-disposition" {
                    // Parse Content-Disposition
                    if let Some(n) = extract_param(value, "name") {
                        part.name = n;
                    }
                    if let Some(f) = extract_param(value, "filename") {
                        part.filename = Some(f);
                    }
                } else if name == "content-type" {
                    part.content_type = Some(value.to_string());
                }

                part.headers.insert(name, value.to_string());
            }
        }

        self.current_part = Some(part);
        true
    }

    /// Parse body for current part.
    fn parse_body(&mut self) -> bool {
        // Find next boundary
        if let Some(pos) = self.find_boundary() {
            // Data before boundary is the body
            // Account for CRLF before boundary
            let body_len = if pos >= 2 { pos - 2 } else { pos };

            if let Some(ref mut part) = self.current_part {
                part.data = self.buffer[..body_len].to_vec();
            }

            // Skip body + CRLF + boundary
            self.buffer.advance(pos + self.boundary.len());

            // Skip trailing CRLF or check for final boundary
            if self.buffer.starts_with(b"--") {
                // This is the final boundary
                self.buffer.advance(2);
            } else if self.buffer.starts_with(b"\r\n") {
                self.buffer.advance(2);
            }

            // Save the part
            if let Some(part) = self.current_part.take() {
                self.parts.push(part);
            }

            return true;
        }

        false
    }

    /// Find boundary position in buffer.
    fn find_boundary(&self) -> Option<usize> {
        self.buffer
            .windows(self.boundary.len())
            .position(|w| w == self.boundary.as_slice())
    }

    /// Find end of headers (CRLF CRLF).
    fn find_header_end(&self) -> Option<usize> {
        self.buffer
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
    }

    /// Check for final boundary.
    fn check_final_boundary(&self) -> bool {
        self.buffer.starts_with(b"--")
    }

    /// Get all parsed parts.
    pub fn parts(&self) -> &[MultipartPart] {
        &self.parts
    }

    /// Take all parsed parts.
    pub fn into_parts(self) -> Vec<MultipartPart> {
        self.parts
    }

    /// Check if parsing is complete.
    pub fn is_complete(&self) -> bool {
        self.state == ParserState::Done
    }

    /// Get part by name.
    pub fn get(&self, name: &str) -> Option<&MultipartPart> {
        self.parts.iter().find(|p| p.name == name)
    }

    /// Get all file parts.
    pub fn files(&self) -> Vec<&MultipartPart> {
        self.parts.iter().filter(|p| p.is_file()).collect()
    }

    /// Get all non-file parts.
    pub fn fields(&self) -> Vec<&MultipartPart> {
        self.parts.iter().filter(|p| !p.is_file()).collect()
    }
}

/// Extract parameter from header value.
fn extract_param(value: &str, param: &str) -> Option<String> {
    let search = format!("{}=", param);
    value.find(&search).map(|pos| {
        let start = pos + search.len();
        let remaining = &value[start..];

        if remaining.starts_with('"') {
            // Quoted value
            remaining[1..]
                .find('"')
                .map(|end| remaining[1..end + 1].to_string())
                .unwrap_or_default()
        } else {
            // Unquoted value
            remaining
                .find(|c: char| c == ';' || c.is_whitespace())
                .map(|end| remaining[..end].to_string())
                .unwrap_or_else(|| remaining.to_string())
        }
    })
}

// ============================================================================
// Streaming Part Reader
// ============================================================================

/// A streaming reader for a single multipart part.
pub struct StreamingPartReader {
    boundary: Vec<u8>,
    buffer: BytesMut,
    done: bool,
}

impl StreamingPartReader {
    /// Create new streaming part reader.
    pub fn new(boundary: &[u8]) -> Self {
        Self {
            boundary: boundary.to_vec(),
            buffer: BytesMut::new(),
            done: false,
        }
    }

    /// Feed data and get available chunk.
    pub fn feed(&mut self, data: &[u8]) -> Option<Bytes> {
        if self.done {
            return None;
        }

        self.buffer.extend_from_slice(data);

        // Look for boundary
        if let Some(pos) = self.find_boundary() {
            // Found boundary - return data before it
            self.done = true;
            let body_len = if pos >= 2 { pos - 2 } else { pos };
            Some(self.buffer.split_to(body_len).freeze())
        } else {
            // No boundary found - return buffered data minus potential boundary
            let safe_len = self
                .buffer
                .len()
                .saturating_sub(self.boundary.len() + 2);
            if safe_len > 0 {
                Some(self.buffer.split_to(safe_len).freeze())
            } else {
                None
            }
        }
    }

    fn find_boundary(&self) -> Option<usize> {
        self.buffer
            .windows(self.boundary.len())
            .position(|w| w == self.boundary.as_slice())
    }

    /// Check if reading is complete.
    pub fn is_done(&self) -> bool {
        self.done
    }
}

// ============================================================================
// Async Multipart Parser
// ============================================================================

/// Async multipart stream.
#[allow(dead_code)]
pub struct AsyncMultipart<R> {
    reader: R,
    boundary: Vec<u8>,
    buffer: BytesMut,
}

impl<R: AsyncRead + Unpin> AsyncMultipart<R> {
    /// Create new async multipart parser.
    pub fn new(reader: R, boundary: &str) -> Self {
        Self {
            reader,
            boundary: format!("--{}", boundary).into_bytes(),
            buffer: BytesMut::with_capacity(8192),
        }
    }

    /// Read next part headers.
    pub async fn next_part(&mut self) -> Option<MultipartPart> {
        // Implementation would read from async reader
        // This is a simplified placeholder
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_param() {
        let value = r#"form-data; name="file"; filename="test.txt""#;
        assert_eq!(extract_param(value, "name"), Some("file".to_string()));
        assert_eq!(
            extract_param(value, "filename"),
            Some("test.txt".to_string())
        );
    }

    #[test]
    fn test_multipart_part() {
        let mut part = MultipartPart::new("test");
        part.filename = Some("image.png".to_string());
        part.content_type = Some("image/png".to_string());
        part.data = vec![1, 2, 3];

        assert!(part.is_file());
        assert_eq!(part.extension(), Some("png".to_string()));
        assert_eq!(part.size(), 3);
    }

    #[test]
    fn test_streaming_multipart() {
        let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
        let body = format!(
            "------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
             Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
             value1\r\n\
             ------WebKitFormBoundary7MA4YWxkTrZu0gW\r\n\
             Content-Disposition: form-data; name=\"file1\"; filename=\"test.txt\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             file content\r\n\
             ------WebKitFormBoundary7MA4YWxkTrZu0gW--\r\n"
        );

        let mut parser = StreamingMultipart::new(boundary);
        parser.feed(body.as_bytes());

        assert!(parser.is_complete());
        assert_eq!(parser.parts().len(), 2);

        let field = parser.get("field1").unwrap();
        assert_eq!(field.as_str().unwrap(), "value1");

        let file = parser.get("file1").unwrap();
        assert!(file.is_file());
        assert_eq!(file.filename, Some("test.txt".to_string()));
        assert_eq!(file.as_str().unwrap(), "file content");
    }

    #[test]
    fn test_from_content_type() {
        let ct = "multipart/form-data; boundary=----WebKitFormBoundary123";
        let parser = StreamingMultipart::from_content_type(ct).unwrap();
        assert_eq!(parser.boundary, b"------WebKitFormBoundary123");
    }
}
