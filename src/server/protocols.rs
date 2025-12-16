//! Protocol support for Cello server.
//!
//! Provides:
//! - TLS configuration
//! - HTTP/2 support
//! - HTTP/3 (QUIC) support (placeholder)

use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// TLS Configuration
// ============================================================================

/// TLS/SSL configuration.
#[derive(Clone, Debug)]
pub struct TlsConfig {
    /// Path to certificate file (PEM format)
    pub cert_path: PathBuf,
    /// Path to private key file (PEM format)
    pub key_path: PathBuf,
    /// Optional CA certificate path for client verification
    pub ca_path: Option<PathBuf>,
    /// Minimum TLS version
    pub min_version: TlsVersion,
    /// Maximum TLS version
    pub max_version: TlsVersion,
    /// ALPN protocols
    pub alpn_protocols: Vec<String>,
    /// Require client certificate
    pub client_auth: ClientAuth,
    /// Session timeout
    pub session_timeout: Duration,
    /// Enable session tickets
    pub session_tickets: bool,
    /// Cipher suites (empty = use defaults)
    pub cipher_suites: Vec<String>,
}

impl TlsConfig {
    /// Create new TLS config with certificate and key paths.
    pub fn new(cert_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        Self {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
            ca_path: None,
            min_version: TlsVersion::Tls12,
            max_version: TlsVersion::Tls13,
            alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
            client_auth: ClientAuth::None,
            session_timeout: Duration::from_secs(43200), // 12 hours
            session_tickets: true,
            cipher_suites: Vec::new(),
        }
    }

    /// Set CA certificate path for client verification.
    pub fn ca_cert(mut self, path: impl Into<PathBuf>) -> Self {
        self.ca_path = Some(path.into());
        self
    }

    /// Set minimum TLS version.
    pub fn min_version(mut self, version: TlsVersion) -> Self {
        self.min_version = version;
        self
    }

    /// Set maximum TLS version.
    pub fn max_version(mut self, version: TlsVersion) -> Self {
        self.max_version = version;
        self
    }

    /// Set ALPN protocols.
    pub fn alpn(mut self, protocols: Vec<&str>) -> Self {
        self.alpn_protocols = protocols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Enable HTTP/2 only.
    pub fn http2_only(mut self) -> Self {
        self.alpn_protocols = vec!["h2".to_string()];
        self
    }

    /// Require client certificate.
    pub fn require_client_cert(mut self) -> Self {
        self.client_auth = ClientAuth::Required;
        self
    }

    /// Optional client certificate.
    pub fn optional_client_cert(mut self) -> Self {
        self.client_auth = ClientAuth::Optional;
        self
    }

    /// Set session timeout.
    pub fn session_timeout(mut self, duration: Duration) -> Self {
        self.session_timeout = duration;
        self
    }

    /// Disable session tickets.
    pub fn no_session_tickets(mut self) -> Self {
        self.session_tickets = false;
        self
    }

    /// Set cipher suites.
    pub fn cipher_suites(mut self, suites: Vec<&str>) -> Self {
        self.cipher_suites = suites.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// TLS version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsVersion {
    Tls10,
    Tls11,
    Tls12,
    Tls13,
}

impl TlsVersion {
    /// Get version string.
    pub fn as_str(&self) -> &'static str {
        match self {
            TlsVersion::Tls10 => "TLS 1.0",
            TlsVersion::Tls11 => "TLS 1.1",
            TlsVersion::Tls12 => "TLS 1.2",
            TlsVersion::Tls13 => "TLS 1.3",
        }
    }
}

/// Client authentication mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientAuth {
    /// No client certificate required
    None,
    /// Client certificate optional
    Optional,
    /// Client certificate required
    Required,
}

// ============================================================================
// HTTP/2 Configuration
// ============================================================================

/// HTTP/2 specific configuration.
#[derive(Clone, Debug)]
pub struct Http2Config {
    /// Maximum concurrent streams
    pub max_concurrent_streams: u32,
    /// Initial connection window size
    pub initial_connection_window_size: u32,
    /// Initial stream window size
    pub initial_stream_window_size: u32,
    /// Maximum frame size
    pub max_frame_size: u32,
    /// Maximum header list size
    pub max_header_list_size: u32,
    /// Enable server push
    pub enable_push: bool,
    /// Ping interval for keep-alive
    pub ping_interval: Option<Duration>,
    /// Ping timeout
    pub ping_timeout: Duration,
}

impl Http2Config {
    /// Create new HTTP/2 config with defaults.
    pub fn new() -> Self {
        Self {
            max_concurrent_streams: 100,
            initial_connection_window_size: 1024 * 1024,      // 1MB
            initial_stream_window_size: 1024 * 1024,          // 1MB
            max_frame_size: 16384,                            // 16KB (minimum)
            max_header_list_size: 16 * 1024,                  // 16KB
            enable_push: false,
            ping_interval: Some(Duration::from_secs(30)),
            ping_timeout: Duration::from_secs(10),
        }
    }

    /// Set maximum concurrent streams.
    pub fn max_concurrent_streams(mut self, max: u32) -> Self {
        self.max_concurrent_streams = max;
        self
    }

    /// Set initial window sizes.
    pub fn window_size(mut self, size: u32) -> Self {
        self.initial_connection_window_size = size;
        self.initial_stream_window_size = size;
        self
    }

    /// Set maximum frame size.
    pub fn max_frame_size(mut self, size: u32) -> Self {
        self.max_frame_size = size.max(16384).min(16777215); // 16KB to 16MB
        self
    }

    /// Set maximum header list size.
    pub fn max_header_list_size(mut self, size: u32) -> Self {
        self.max_header_list_size = size;
        self
    }

    /// Enable server push.
    pub fn enable_push(mut self) -> Self {
        self.enable_push = true;
        self
    }

    /// Disable server push.
    pub fn disable_push(mut self) -> Self {
        self.enable_push = false;
        self
    }

    /// Set ping interval.
    pub fn ping_interval(mut self, interval: Duration) -> Self {
        self.ping_interval = Some(interval);
        self
    }

    /// Disable ping keep-alive.
    pub fn no_ping(mut self) -> Self {
        self.ping_interval = None;
        self
    }

    /// Set ping timeout.
    pub fn ping_timeout(mut self, timeout: Duration) -> Self {
        self.ping_timeout = timeout;
        self
    }
}

impl Default for Http2Config {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// HTTP/3 Configuration (QUIC)
// ============================================================================

/// HTTP/3 (QUIC) specific configuration.
#[derive(Clone, Debug)]
pub struct Http3Config {
    /// Maximum idle timeout
    pub max_idle_timeout: Duration,
    /// Maximum UDP payload size
    pub max_udp_payload_size: u16,
    /// Initial maximum data (connection level)
    pub initial_max_data: u64,
    /// Initial maximum data per bidirectional stream
    pub initial_max_stream_data_bidi: u64,
    /// Initial maximum data per unidirectional stream
    pub initial_max_stream_data_uni: u64,
    /// Initial maximum bidirectional streams
    pub initial_max_streams_bidi: u64,
    /// Initial maximum unidirectional streams
    pub initial_max_streams_uni: u64,
    /// Enable 0-RTT
    pub enable_0rtt: bool,
    /// QPACK maximum table capacity
    pub qpack_max_table_capacity: u64,
    /// QPACK blocked streams
    pub qpack_blocked_streams: u64,
}

impl Http3Config {
    /// Create new HTTP/3 config with defaults.
    pub fn new() -> Self {
        Self {
            max_idle_timeout: Duration::from_secs(30),
            max_udp_payload_size: 1350,
            initial_max_data: 10 * 1024 * 1024,           // 10MB
            initial_max_stream_data_bidi: 1024 * 1024,    // 1MB
            initial_max_stream_data_uni: 1024 * 1024,     // 1MB
            initial_max_streams_bidi: 100,
            initial_max_streams_uni: 100,
            enable_0rtt: false,
            qpack_max_table_capacity: 4096,
            qpack_blocked_streams: 100,
        }
    }

    /// Set maximum idle timeout.
    pub fn max_idle_timeout(mut self, timeout: Duration) -> Self {
        self.max_idle_timeout = timeout;
        self
    }

    /// Set maximum UDP payload size.
    pub fn max_udp_payload_size(mut self, size: u16) -> Self {
        self.max_udp_payload_size = size;
        self
    }

    /// Set initial maximum data.
    pub fn initial_max_data(mut self, size: u64) -> Self {
        self.initial_max_data = size;
        self
    }

    /// Set maximum concurrent bidirectional streams.
    pub fn max_streams_bidi(mut self, max: u64) -> Self {
        self.initial_max_streams_bidi = max;
        self
    }

    /// Set maximum concurrent unidirectional streams.
    pub fn max_streams_uni(mut self, max: u64) -> Self {
        self.initial_max_streams_uni = max;
        self
    }

    /// Enable 0-RTT (zero round-trip time resumption).
    pub fn enable_0rtt(mut self) -> Self {
        self.enable_0rtt = true;
        self
    }

    /// Disable 0-RTT.
    pub fn disable_0rtt(mut self) -> Self {
        self.enable_0rtt = false;
        self
    }
}

impl Default for Http3Config {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Protocol Detection
// ============================================================================

/// Detected protocol from ALPN negotiation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Http1,
    Http2,
    Http3,
}

impl Protocol {
    /// Get protocol from ALPN string.
    pub fn from_alpn(alpn: &[u8]) -> Option<Self> {
        match alpn {
            b"h2" => Some(Protocol::Http2),
            b"http/1.1" => Some(Protocol::Http1),
            b"h3" => Some(Protocol::Http3),
            _ => None,
        }
    }

    /// Get ALPN string for protocol.
    pub fn to_alpn(&self) -> &'static [u8] {
        match self {
            Protocol::Http1 => b"http/1.1",
            Protocol::Http2 => b"h2",
            Protocol::Http3 => b"h3",
        }
    }

    /// Get protocol name.
    pub fn name(&self) -> &'static str {
        match self {
            Protocol::Http1 => "HTTP/1.1",
            Protocol::Http2 => "HTTP/2",
            Protocol::Http3 => "HTTP/3",
        }
    }
}

// ============================================================================
// Server Push (HTTP/2)
// ============================================================================

/// HTTP/2 server push configuration.
#[derive(Clone, Debug)]
pub struct PushConfig {
    /// Enable push
    pub enabled: bool,
    /// Maximum concurrent push streams
    pub max_concurrent: u32,
    /// Push promise timeout
    pub timeout: Duration,
}

impl PushConfig {
    /// Create new push config.
    pub fn new() -> Self {
        Self {
            enabled: false,
            max_concurrent: 10,
            timeout: Duration::from_secs(5),
        }
    }

    /// Enable push.
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Set maximum concurrent push streams.
    pub fn max_concurrent(mut self, max: u32) -> Self {
        self.max_concurrent = max;
        self
    }

    /// Set push promise timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for PushConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Server push promise.
#[derive(Clone, Debug)]
pub struct PushPromise {
    /// Path to push
    pub path: String,
    /// Method (usually GET)
    pub method: String,
    /// Headers
    pub headers: Vec<(String, String)>,
}

impl PushPromise {
    /// Create new push promise for a path.
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
        }
    }

    /// Add header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config() {
        let config = TlsConfig::new("cert.pem", "key.pem")
            .min_version(TlsVersion::Tls12)
            .max_version(TlsVersion::Tls13)
            .http2_only();

        assert_eq!(config.cert_path, PathBuf::from("cert.pem"));
        assert_eq!(config.key_path, PathBuf::from("key.pem"));
        assert_eq!(config.min_version, TlsVersion::Tls12);
        assert_eq!(config.alpn_protocols, vec!["h2"]);
    }

    #[test]
    fn test_http2_config() {
        let config = Http2Config::new()
            .max_concurrent_streams(200)
            .window_size(2 * 1024 * 1024)
            .enable_push();

        assert_eq!(config.max_concurrent_streams, 200);
        assert_eq!(config.initial_connection_window_size, 2 * 1024 * 1024);
        assert!(config.enable_push);
    }

    #[test]
    fn test_http3_config() {
        let config = Http3Config::new()
            .max_idle_timeout(Duration::from_secs(60))
            .max_streams_bidi(200)
            .enable_0rtt();

        assert_eq!(config.max_idle_timeout, Duration::from_secs(60));
        assert_eq!(config.initial_max_streams_bidi, 200);
        assert!(config.enable_0rtt);
    }

    #[test]
    fn test_protocol_detection() {
        assert_eq!(Protocol::from_alpn(b"h2"), Some(Protocol::Http2));
        assert_eq!(Protocol::from_alpn(b"http/1.1"), Some(Protocol::Http1));
        assert_eq!(Protocol::from_alpn(b"h3"), Some(Protocol::Http3));
        assert_eq!(Protocol::from_alpn(b"unknown"), None);
    }

    #[test]
    fn test_push_promise() {
        let promise = PushPromise::new("/style.css")
            .header("Content-Type", "text/css");

        assert_eq!(promise.path, "/style.css");
        assert_eq!(promise.method, "GET");
        assert_eq!(promise.headers.len(), 1);
    }

    #[test]
    fn test_tls_version() {
        assert_eq!(TlsVersion::Tls12.as_str(), "TLS 1.2");
        assert_eq!(TlsVersion::Tls13.as_str(), "TLS 1.3");
    }
}
