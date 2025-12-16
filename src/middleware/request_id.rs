//! Request ID middleware for Cello.
//!
//! Provides:
//! - Unique request ID generation
//! - Request ID propagation
//! - Custom ID formats

use rand::Rng;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// ID Generation
// ============================================================================

/// Request ID format.
#[derive(Clone)]
pub enum IdFormat {
    /// UUID v4 format (e.g., "550e8400-e29b-41d4-a716-446655440000")
    Uuid,
    /// Short hex format (e.g., "a1b2c3d4e5f6")
    ShortHex(usize),
    /// Timestamp + counter format (e.g., "1700000000-000001")
    TimestampCounter,
    /// Prefix + UUID (e.g., "req_550e8400...")
    PrefixedUuid(String),
    /// Custom format generator
    Custom(fn() -> String),
}

impl Default for IdFormat {
    fn default() -> Self {
        IdFormat::Uuid
    }
}

/// Counter for timestamp-based IDs.
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate UUID v4.
pub fn generate_uuid() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();

    // Set version (4) and variant (RFC 4122)
    let mut uuid_bytes = bytes;
    uuid_bytes[6] = (uuid_bytes[6] & 0x0f) | 0x40; // Version 4
    uuid_bytes[8] = (uuid_bytes[8] & 0x3f) | 0x80; // Variant

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([uuid_bytes[0], uuid_bytes[1], uuid_bytes[2], uuid_bytes[3]]),
        u16::from_be_bytes([uuid_bytes[4], uuid_bytes[5]]),
        u16::from_be_bytes([uuid_bytes[6], uuid_bytes[7]]),
        u16::from_be_bytes([uuid_bytes[8], uuid_bytes[9]]),
        u64::from_be_bytes([
            0,
            0,
            uuid_bytes[10],
            uuid_bytes[11],
            uuid_bytes[12],
            uuid_bytes[13],
            uuid_bytes[14],
            uuid_bytes[15]
        ])
    )
}

/// Generate short hex ID.
pub fn generate_short_hex(length: usize) -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..(length / 2 + 1)).map(|_| rng.gen()).collect();
    hex::encode(&bytes)[..length].to_string()
}

/// Generate timestamp + counter ID.
pub fn generate_timestamp_counter() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let counter = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{}-{:06}", timestamp, counter % 1_000_000)
}

impl IdFormat {
    /// Generate an ID using this format.
    pub fn generate(&self) -> String {
        match self {
            IdFormat::Uuid => generate_uuid(),
            IdFormat::ShortHex(len) => generate_short_hex(*len),
            IdFormat::TimestampCounter => generate_timestamp_counter(),
            IdFormat::PrefixedUuid(prefix) => format!("{}_{}", prefix, generate_uuid()),
            IdFormat::Custom(generator) => generator(),
        }
    }
}

// ============================================================================
// Request ID Middleware
// ============================================================================

/// Request ID middleware.
pub struct RequestIdMiddleware {
    /// Header name for incoming request ID
    incoming_header: String,
    /// Header name for outgoing response ID
    outgoing_header: String,
    /// ID format for generation
    format: IdFormat,
    /// Whether to trust incoming request IDs
    trust_incoming: bool,
    /// Context key for storing request ID
    context_key: String,
    /// Whether to add to response headers
    expose_in_response: bool,
}

impl RequestIdMiddleware {
    /// Create new request ID middleware.
    pub fn new() -> Self {
        Self {
            incoming_header: "X-Request-ID".to_string(),
            outgoing_header: "X-Request-ID".to_string(),
            format: IdFormat::default(),
            trust_incoming: false,
            context_key: "request_id".to_string(),
            expose_in_response: true,
        }
    }

    /// Set incoming header name.
    pub fn incoming_header(mut self, name: &str) -> Self {
        self.incoming_header = name.to_string();
        self
    }

    /// Set outgoing header name.
    pub fn outgoing_header(mut self, name: &str) -> Self {
        self.outgoing_header = name.to_string();
        self
    }

    /// Set both header names.
    pub fn header(mut self, name: &str) -> Self {
        self.incoming_header = name.to_string();
        self.outgoing_header = name.to_string();
        self
    }

    /// Set ID format.
    pub fn format(mut self, format: IdFormat) -> Self {
        self.format = format;
        self
    }

    /// Use UUID format.
    pub fn uuid(mut self) -> Self {
        self.format = IdFormat::Uuid;
        self
    }

    /// Use short hex format.
    pub fn short_hex(mut self, length: usize) -> Self {
        self.format = IdFormat::ShortHex(length);
        self
    }

    /// Use timestamp + counter format.
    pub fn timestamp_counter(mut self) -> Self {
        self.format = IdFormat::TimestampCounter;
        self
    }

    /// Use prefixed UUID format.
    pub fn prefixed_uuid(mut self, prefix: &str) -> Self {
        self.format = IdFormat::PrefixedUuid(prefix.to_string());
        self
    }

    /// Trust incoming request IDs from clients.
    pub fn trust_incoming(mut self, trust: bool) -> Self {
        self.trust_incoming = trust;
        self
    }

    /// Set context key for storing request ID.
    pub fn context_key(mut self, key: &str) -> Self {
        self.context_key = key.to_string();
        self
    }

    /// Enable/disable exposing request ID in response.
    pub fn expose_in_response(mut self, expose: bool) -> Self {
        self.expose_in_response = expose;
        self
    }
}

impl Default for RequestIdMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for RequestIdMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Try to get existing request ID if trusted
        let request_id = if self.trust_incoming {
            request
                .headers
                .get(&self.incoming_header.to_lowercase())
                .cloned()
        } else {
            None
        };

        // Generate new ID if not available
        let request_id = request_id.unwrap_or_else(|| self.format.generate());

        // Store in context
        request.context.insert(
            self.context_key.clone(),
            serde_json::Value::String(request_id),
        );

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Add request ID to response header
        if self.expose_in_response {
            if let Some(request_id) = request.context.get(&self.context_key) {
                if let Some(id_str) = request_id.as_str() {
                    response.set_header(&self.outgoing_header, id_str);
                }
            }
        }
        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -150 // Run very early, before logging
    }

    fn name(&self) -> &str {
        "request_id"
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get request ID from request context.
pub fn get_request_id(request: &Request) -> Option<String> {
    request
        .context
        .get("request_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Get request ID with custom context key.
pub fn get_request_id_with_key(request: &Request, key: &str) -> Option<String> {
    request
        .context
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_uuid() {
        let uuid = generate_uuid();
        assert_eq!(uuid.len(), 36);
        assert!(uuid.contains('-'));

        // Verify format
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn test_generate_short_hex() {
        let hex8 = generate_short_hex(8);
        assert_eq!(hex8.len(), 8);
        assert!(hex8.chars().all(|c| c.is_ascii_hexdigit()));

        let hex16 = generate_short_hex(16);
        assert_eq!(hex16.len(), 16);
    }

    #[test]
    fn test_generate_timestamp_counter() {
        let id1 = generate_timestamp_counter();
        let id2 = generate_timestamp_counter();

        // Should be unique
        assert_ne!(id1, id2);

        // Should have format timestamp-counter
        assert!(id1.contains('-'));
    }

    #[test]
    fn test_id_format() {
        let uuid = IdFormat::Uuid.generate();
        assert_eq!(uuid.len(), 36);

        let short = IdFormat::ShortHex(12).generate();
        assert_eq!(short.len(), 12);

        let prefixed = IdFormat::PrefixedUuid("req".to_string()).generate();
        assert!(prefixed.starts_with("req_"));
    }

    #[test]
    fn test_request_id_middleware() {
        let middleware = RequestIdMiddleware::new()
            .header("X-Correlation-ID")
            .uuid()
            .trust_incoming(true);

        assert_eq!(middleware.incoming_header, "X-Correlation-ID");
        assert_eq!(middleware.outgoing_header, "X-Correlation-ID");
        assert!(middleware.trust_incoming);
    }
}
