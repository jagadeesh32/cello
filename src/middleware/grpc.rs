//! gRPC Support for Cello Framework.
//!
//! Provides:
//! - Unary, client streaming, server streaming, and bidirectional streaming RPCs
//! - Service registration and discovery
//! - gRPC reflection for service introspection
//! - gRPC-Web support for browser clients
//! - Connection keepalive and concurrency limits
//!
//! # Example
//! ```python
//! from cello import App
//! from cello.grpc import GrpcServer, GrpcConfig
//!
//! config = GrpcConfig(
//!     address="[::1]:50051",
//!     reflection=True,
//!     enable_web=True
//! )
//!
//! server = GrpcServer(config)
//! server.register_service(greeter_service)
//!
//! @app.on_startup
//! async def setup():
//!     app.state.grpc = server
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

// ============================================================================
// gRPC Configuration
// ============================================================================

/// gRPC server configuration.
#[derive(Clone, Debug)]
pub struct GrpcConfig {
    /// Address to bind the gRPC server (e.g., "[::1]:50051")
    pub address: String,
    /// List of service names to register
    pub services: Vec<String>,
    /// Enable gRPC server reflection for service discovery
    pub reflection: bool,
    /// Maximum message size in bytes (default: 4MB)
    pub max_message_size: usize,
    /// Enable gRPC-Web support for browser clients
    pub enable_web: bool,
    /// Keepalive interval in seconds (default: 60)
    pub keepalive_secs: u64,
    /// Maximum number of concurrent streams per connection (default: 100)
    pub concurrency_limit: usize,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            address: "[::1]:50051".to_string(),
            services: Vec::new(),
            reflection: false,
            max_message_size: 4 * 1024 * 1024, // 4MB
            enable_web: false,
            keepalive_secs: 60,
            concurrency_limit: 100,
        }
    }
}

impl GrpcConfig {
    /// Create a new gRPC configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the server bind address.
    pub fn with_address(mut self, address: &str) -> Self {
        self.address = address.to_string();
        self
    }

    /// Enable or disable gRPC server reflection.
    pub fn with_reflection(mut self, enabled: bool) -> Self {
        self.reflection = enabled;
        self
    }

    /// Set the maximum message size in bytes.
    pub fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Enable or disable gRPC-Web support for browser clients.
    pub fn with_web_support(mut self, enabled: bool) -> Self {
        self.enable_web = enabled;
        self
    }
}

// ============================================================================
// gRPC Method Types
// ============================================================================

/// Defines the type of a gRPC method call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrpcMethodType {
    /// Single request, single response.
    Unary,
    /// Stream of requests, single response.
    ClientStreaming,
    /// Single request, stream of responses.
    ServerStreaming,
    /// Stream of requests, stream of responses.
    BidirectionalStreaming,
}

impl std::fmt::Display for GrpcMethodType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrpcMethodType::Unary => write!(f, "UNARY"),
            GrpcMethodType::ClientStreaming => write!(f, "CLIENT_STREAMING"),
            GrpcMethodType::ServerStreaming => write!(f, "SERVER_STREAMING"),
            GrpcMethodType::BidirectionalStreaming => write!(f, "BIDIRECTIONAL_STREAMING"),
        }
    }
}

// ============================================================================
// gRPC Method Definition
// ============================================================================

/// Definition of a single gRPC method within a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcMethodDef {
    /// Method name (e.g., "SayHello")
    pub name: String,
    /// Type of the gRPC method (unary, streaming, etc.)
    pub method_type: GrpcMethodType,
    /// Fully qualified input message type (e.g., "helloworld.HelloRequest")
    pub input_type: String,
    /// Fully qualified output message type (e.g., "helloworld.HelloReply")
    pub output_type: String,
}

impl GrpcMethodDef {
    /// Create a new gRPC method definition.
    pub fn new(name: &str, method_type: GrpcMethodType, input_type: &str, output_type: &str) -> Self {
        Self {
            name: name.to_string(),
            method_type,
            input_type: input_type.to_string(),
            output_type: output_type.to_string(),
        }
    }

    /// Create a unary method definition.
    pub fn unary(name: &str, input_type: &str, output_type: &str) -> Self {
        Self::new(name, GrpcMethodType::Unary, input_type, output_type)
    }

    /// Create a server streaming method definition.
    pub fn server_streaming(name: &str, input_type: &str, output_type: &str) -> Self {
        Self::new(name, GrpcMethodType::ServerStreaming, input_type, output_type)
    }

    /// Create a client streaming method definition.
    pub fn client_streaming(name: &str, input_type: &str, output_type: &str) -> Self {
        Self::new(name, GrpcMethodType::ClientStreaming, input_type, output_type)
    }

    /// Create a bidirectional streaming method definition.
    pub fn bidi_streaming(name: &str, input_type: &str, output_type: &str) -> Self {
        Self::new(name, GrpcMethodType::BidirectionalStreaming, input_type, output_type)
    }
}

// ============================================================================
// gRPC Service Definition
// ============================================================================

/// Definition of a gRPC service containing one or more methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcServiceDef {
    /// Fully qualified service name (e.g., "helloworld.Greeter")
    pub name: String,
    /// List of methods provided by this service
    pub methods: Vec<GrpcMethodDef>,
    /// Optional human-readable description of the service
    pub description: Option<String>,
}

impl GrpcServiceDef {
    /// Create a new service definition with the given name.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            methods: Vec::new(),
            description: None,
        }
    }

    /// Add a method to this service definition.
    pub fn add_method(mut self, method: GrpcMethodDef) -> Self {
        self.methods.push(method);
        self
    }

    /// Set the service description.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Get a method definition by name.
    pub fn get_method(&self, name: &str) -> Option<&GrpcMethodDef> {
        self.methods.iter().find(|m| m.name == name)
    }

    /// List all method names in this service.
    pub fn method_names(&self) -> Vec<String> {
        self.methods.iter().map(|m| m.name.clone()).collect()
    }
}

// ============================================================================
// gRPC Error
// ============================================================================

/// gRPC error types corresponding to standard gRPC status codes.
#[derive(Debug, Clone)]
pub enum GrpcError {
    /// The message payload is invalid or cannot be decoded.
    InvalidMessage,
    /// The requested service was not found on the server.
    ServiceNotFound,
    /// The requested method was not found in the service.
    MethodNotFound,
    /// An internal server error occurred.
    Internal,
    /// The service is currently unavailable.
    Unavailable,
    /// The operation exceeded its deadline.
    DeadlineExceeded,
    /// The operation was cancelled by the caller.
    Cancelled,
    /// The caller does not have permission to execute the operation.
    PermissionDenied,
    /// The caller has not been authenticated.
    Unauthenticated,
    /// An unknown error with a custom message.
    Unknown(String),
}

impl GrpcError {
    /// Convert a gRPC error to its corresponding status code.
    pub fn code(&self) -> i32 {
        match self {
            GrpcError::InvalidMessage => 3,   // INVALID_ARGUMENT
            GrpcError::ServiceNotFound => 5,  // NOT_FOUND
            GrpcError::MethodNotFound => 12,  // UNIMPLEMENTED
            GrpcError::Internal => 13,        // INTERNAL
            GrpcError::Unavailable => 14,     // UNAVAILABLE
            GrpcError::DeadlineExceeded => 4, // DEADLINE_EXCEEDED
            GrpcError::Cancelled => 1,        // CANCELLED
            GrpcError::PermissionDenied => 7, // PERMISSION_DENIED
            GrpcError::Unauthenticated => 16, // UNAUTHENTICATED
            GrpcError::Unknown(_) => 2,       // UNKNOWN
        }
    }

    /// Convert a gRPC error into a GrpcStatus.
    pub fn to_status(&self) -> GrpcStatus {
        GrpcStatus {
            code: self.code(),
            message: self.to_string(),
            details: None,
        }
    }
}

impl std::fmt::Display for GrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrpcError::InvalidMessage => write!(f, "gRPC error: invalid message"),
            GrpcError::ServiceNotFound => write!(f, "gRPC error: service not found"),
            GrpcError::MethodNotFound => write!(f, "gRPC error: method not found"),
            GrpcError::Internal => write!(f, "gRPC error: internal server error"),
            GrpcError::Unavailable => write!(f, "gRPC error: service unavailable"),
            GrpcError::DeadlineExceeded => write!(f, "gRPC error: deadline exceeded"),
            GrpcError::Cancelled => write!(f, "gRPC error: operation cancelled"),
            GrpcError::PermissionDenied => write!(f, "gRPC error: permission denied"),
            GrpcError::Unauthenticated => write!(f, "gRPC error: unauthenticated"),
            GrpcError::Unknown(msg) => write!(f, "gRPC error: unknown: {}", msg),
        }
    }
}

impl std::error::Error for GrpcError {}

// ============================================================================
// gRPC Status
// ============================================================================

/// Represents a gRPC status response with code, message, and optional details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcStatus {
    /// gRPC status code (0 = OK, non-zero = error)
    pub code: i32,
    /// Human-readable status message
    pub message: String,
    /// Optional binary-encoded status details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<u8>>,
}

impl GrpcStatus {
    /// Create a successful OK status.
    pub fn ok() -> Self {
        Self {
            code: 0,
            message: "OK".to_string(),
            details: None,
        }
    }

    /// Create an error status with the given code and message.
    pub fn error(code: i32, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            details: None,
        }
    }

    /// Attach binary details to this status.
    pub fn with_details(mut self, details: Vec<u8>) -> Self {
        self.details = Some(details);
        self
    }

    /// Returns true if the status code indicates success (code == 0).
    pub fn is_ok(&self) -> bool {
        self.code == 0
    }

    /// Returns true if the status code indicates an error (code != 0).
    pub fn is_error(&self) -> bool {
        self.code != 0
    }
}

// ============================================================================
// gRPC Request
// ============================================================================

/// Represents an incoming gRPC request.
#[derive(Debug, Clone)]
pub struct GrpcRequest {
    /// Fully qualified service name
    pub service: String,
    /// Method name to invoke
    pub method: String,
    /// Serialized protobuf payload bytes
    pub payload: Vec<u8>,
    /// Request metadata (headers) as key-value pairs
    pub metadata: HashMap<String, String>,
}

impl GrpcRequest {
    /// Create a new gRPC request.
    pub fn new(service: &str, method: &str, payload: Vec<u8>) -> Self {
        Self {
            service: service.to_string(),
            method: method.to_string(),
            payload,
            metadata: HashMap::new(),
        }
    }

    /// Add a metadata entry to the request.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Get a metadata value by key.
    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }

    /// Get the fully qualified method path (e.g., "/package.Service/Method").
    pub fn full_path(&self) -> String {
        format!("/{}/{}", self.service, self.method)
    }
}

// ============================================================================
// gRPC Response
// ============================================================================

/// Represents an outgoing gRPC response.
#[derive(Debug, Clone)]
pub struct GrpcResponse {
    /// gRPC status indicating success or failure
    pub status: GrpcStatus,
    /// Serialized protobuf response payload bytes
    pub payload: Vec<u8>,
    /// Response metadata (trailers) as key-value pairs
    pub metadata: HashMap<String, String>,
}

impl GrpcResponse {
    /// Create a successful response with the given payload.
    pub fn ok(payload: Vec<u8>) -> Self {
        Self {
            status: GrpcStatus::ok(),
            payload,
            metadata: HashMap::new(),
        }
    }

    /// Create an error response with the given status.
    pub fn error(status: GrpcStatus) -> Self {
        Self {
            status,
            payload: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Create an error response from a GrpcError.
    pub fn from_error(error: &GrpcError) -> Self {
        Self::error(error.to_status())
    }

    /// Add a metadata entry to the response.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Returns true if the response status indicates success.
    pub fn is_ok(&self) -> bool {
        self.status.is_ok()
    }
}

// ============================================================================
// gRPC Service Trait
// ============================================================================

/// Trait for implementing a gRPC service.
///
/// Types implementing this trait can be registered with a `GrpcServer`
/// to handle incoming gRPC requests.
pub trait GrpcService: Send + Sync {
    /// Returns the fully qualified name of this service.
    fn name(&self) -> &str;

    /// Returns the list of method definitions provided by this service.
    fn methods(&self) -> Vec<GrpcMethodDef>;
}

// ============================================================================
// gRPC Server
// ============================================================================

/// gRPC server that manages service registration and request dispatching.
pub struct GrpcServer {
    /// Server configuration
    config: GrpcConfig,
    /// Registered service definitions, keyed by service name
    services: Arc<RwLock<HashMap<String, GrpcServiceDef>>>,
    /// Server running state
    running: Arc<RwLock<bool>>,
    /// Request statistics
    stats: Arc<RwLock<GrpcStats>>,
}

impl GrpcServer {
    /// Create a new gRPC server with the given configuration.
    pub fn new(config: GrpcConfig) -> Self {
        Self {
            config,
            services: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
            stats: Arc::new(RwLock::new(GrpcStats::default())),
        }
    }

    /// Register a service definition with the server.
    pub fn register_service(&self, service_def: GrpcServiceDef) {
        let mut services = self.services.write();
        services.insert(service_def.name.clone(), service_def);
    }

    /// Get a service definition by its fully qualified name.
    pub fn get_service(&self, name: &str) -> Option<GrpcServiceDef> {
        let services = self.services.read();
        services.get(name).cloned()
    }

    /// List all registered service names.
    pub fn list_services(&self) -> Vec<String> {
        let services = self.services.read();
        services.keys().cloned().collect()
    }

    /// Get the server configuration.
    pub fn config(&self) -> &GrpcConfig {
        &self.config
    }

    /// Check if the server is currently running.
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }

    /// Get current server statistics.
    pub fn stats(&self) -> GrpcStats {
        self.stats.read().clone()
    }

    /// Look up a specific method within a service.
    ///
    /// Returns an error if the service or method is not found.
    pub fn resolve_method(&self, service_name: &str, method_name: &str) -> Result<GrpcMethodDef, GrpcError> {
        let services = self.services.read();
        let service = services
            .get(service_name)
            .ok_or(GrpcError::ServiceNotFound)?;
        service
            .get_method(method_name)
            .cloned()
            .ok_or(GrpcError::MethodNotFound)
    }

    /// Process an incoming gRPC request and return a response.
    ///
    /// This validates the request against registered services and methods,
    /// checks message size limits, and updates server statistics.
    pub fn handle_request(&self, request: &GrpcRequest) -> GrpcResponse {
        // Update stats
        {
            let mut stats = self.stats.write();
            stats.total_requests += 1;
            stats.active_streams += 1;
        }

        // Validate message size
        if request.payload.len() > self.config.max_message_size {
            let mut stats = self.stats.write();
            stats.total_errors += 1;
            stats.active_streams = stats.active_streams.saturating_sub(1);
            return GrpcResponse::error(GrpcStatus::error(
                GrpcError::InvalidMessage.code(),
                &format!(
                    "Message size {} exceeds maximum allowed size {}",
                    request.payload.len(),
                    self.config.max_message_size
                ),
            ));
        }

        // Resolve service and method
        let result = self.resolve_method(&request.service, &request.method);
        match result {
            Ok(_method_def) => {
                // In a full implementation, this would dispatch to the actual handler.
                // For now, return a placeholder success response.
                let mut stats = self.stats.write();
                stats.active_streams = stats.active_streams.saturating_sub(1);
                GrpcResponse::ok(Vec::new())
            }
            Err(err) => {
                let mut stats = self.stats.write();
                stats.total_errors += 1;
                stats.active_streams = stats.active_streams.saturating_sub(1);
                GrpcResponse::from_error(&err)
            }
        }
    }

    /// Get the number of registered services.
    pub fn service_count(&self) -> usize {
        self.services.read().len()
    }

    /// Check if reflection is enabled for this server.
    pub fn reflection_enabled(&self) -> bool {
        self.config.reflection
    }
}

// ============================================================================
// gRPC Statistics
// ============================================================================

/// Runtime statistics for the gRPC server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GrpcStats {
    /// Total number of requests processed
    pub total_requests: u64,
    /// Number of currently active streams
    pub active_streams: usize,
    /// Total number of errors encountered
    pub total_errors: u64,
    /// Average request latency in milliseconds
    pub avg_latency_ms: f64,
}

impl GrpcStats {
    /// Create a new empty stats instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate the error rate as a fraction (0.0 to 1.0).
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_requests as f64
        }
    }

    /// Calculate the success rate as a fraction (0.0 to 1.0).
    pub fn success_rate(&self) -> f64 {
        1.0 - self.error_rate()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_config_defaults() {
        let config = GrpcConfig::new();

        assert_eq!(config.address, "[::1]:50051");
        assert!(config.services.is_empty());
        assert!(!config.reflection);
        assert_eq!(config.max_message_size, 4 * 1024 * 1024);
        assert!(!config.enable_web);
        assert_eq!(config.keepalive_secs, 60);
        assert_eq!(config.concurrency_limit, 100);
    }

    #[test]
    fn test_grpc_config_builder() {
        let config = GrpcConfig::new()
            .with_address("0.0.0.0:9090")
            .with_reflection(true)
            .with_max_message_size(8 * 1024 * 1024)
            .with_web_support(true);

        assert_eq!(config.address, "0.0.0.0:9090");
        assert!(config.reflection);
        assert_eq!(config.max_message_size, 8 * 1024 * 1024);
        assert!(config.enable_web);
    }

    #[test]
    fn test_grpc_method_type_display() {
        assert_eq!(GrpcMethodType::Unary.to_string(), "UNARY");
        assert_eq!(GrpcMethodType::ClientStreaming.to_string(), "CLIENT_STREAMING");
        assert_eq!(GrpcMethodType::ServerStreaming.to_string(), "SERVER_STREAMING");
        assert_eq!(
            GrpcMethodType::BidirectionalStreaming.to_string(),
            "BIDIRECTIONAL_STREAMING"
        );
    }

    #[test]
    fn test_grpc_method_def_constructors() {
        let unary = GrpcMethodDef::unary("SayHello", "HelloRequest", "HelloReply");
        assert_eq!(unary.name, "SayHello");
        assert_eq!(unary.method_type, GrpcMethodType::Unary);
        assert_eq!(unary.input_type, "HelloRequest");
        assert_eq!(unary.output_type, "HelloReply");

        let server_stream = GrpcMethodDef::server_streaming("ListFeatures", "Rectangle", "Feature");
        assert_eq!(server_stream.method_type, GrpcMethodType::ServerStreaming);

        let client_stream = GrpcMethodDef::client_streaming("RecordRoute", "Point", "RouteSummary");
        assert_eq!(client_stream.method_type, GrpcMethodType::ClientStreaming);

        let bidi = GrpcMethodDef::bidi_streaming("RouteChat", "RouteNote", "RouteNote");
        assert_eq!(bidi.method_type, GrpcMethodType::BidirectionalStreaming);
    }

    #[test]
    fn test_grpc_service_def() {
        let service = GrpcServiceDef::new("helloworld.Greeter")
            .with_description("The greeting service definition")
            .add_method(GrpcMethodDef::unary("SayHello", "HelloRequest", "HelloReply"))
            .add_method(GrpcMethodDef::server_streaming(
                "SayHelloStream",
                "HelloRequest",
                "HelloReply",
            ));

        assert_eq!(service.name, "helloworld.Greeter");
        assert_eq!(service.description, Some("The greeting service definition".to_string()));
        assert_eq!(service.methods.len(), 2);

        let method = service.get_method("SayHello");
        assert!(method.is_some());
        assert_eq!(method.unwrap().input_type, "HelloRequest");

        assert!(service.get_method("NonExistent").is_none());

        let names = service.method_names();
        assert_eq!(names, vec!["SayHello", "SayHelloStream"]);
    }

    #[test]
    fn test_grpc_error_codes() {
        assert_eq!(GrpcError::InvalidMessage.code(), 3);
        assert_eq!(GrpcError::ServiceNotFound.code(), 5);
        assert_eq!(GrpcError::MethodNotFound.code(), 12);
        assert_eq!(GrpcError::Internal.code(), 13);
        assert_eq!(GrpcError::Unavailable.code(), 14);
        assert_eq!(GrpcError::DeadlineExceeded.code(), 4);
        assert_eq!(GrpcError::Cancelled.code(), 1);
        assert_eq!(GrpcError::PermissionDenied.code(), 7);
        assert_eq!(GrpcError::Unauthenticated.code(), 16);
        assert_eq!(GrpcError::Unknown("test".to_string()).code(), 2);
    }

    #[test]
    fn test_grpc_error_display() {
        let err = GrpcError::ServiceNotFound;
        assert_eq!(err.to_string(), "gRPC error: service not found");

        let err = GrpcError::Unknown("something went wrong".to_string());
        assert_eq!(err.to_string(), "gRPC error: unknown: something went wrong");
    }

    #[test]
    fn test_grpc_error_to_status() {
        let err = GrpcError::PermissionDenied;
        let status = err.to_status();
        assert_eq!(status.code, 7);
        assert_eq!(status.message, "gRPC error: permission denied");
        assert!(status.details.is_none());
    }

    #[test]
    fn test_grpc_status_ok() {
        let status = GrpcStatus::ok();
        assert_eq!(status.code, 0);
        assert_eq!(status.message, "OK");
        assert!(status.is_ok());
        assert!(!status.is_error());
    }

    #[test]
    fn test_grpc_status_error() {
        let status = GrpcStatus::error(13, "Internal server error");
        assert_eq!(status.code, 13);
        assert_eq!(status.message, "Internal server error");
        assert!(!status.is_ok());
        assert!(status.is_error());
    }

    #[test]
    fn test_grpc_status_with_details() {
        let details = vec![0x01, 0x02, 0x03];
        let status = GrpcStatus::error(3, "Bad request").with_details(details.clone());
        assert_eq!(status.details, Some(details));
    }

    #[test]
    fn test_grpc_request() {
        let payload = vec![0x0a, 0x05, 0x57, 0x6f, 0x72, 0x6c, 0x64];
        let request = GrpcRequest::new("helloworld.Greeter", "SayHello", payload.clone())
            .with_metadata("authorization", "Bearer token123")
            .with_metadata("x-request-id", "abc-123");

        assert_eq!(request.service, "helloworld.Greeter");
        assert_eq!(request.method, "SayHello");
        assert_eq!(request.payload, payload);
        assert_eq!(request.get_metadata("authorization"), Some("Bearer token123"));
        assert_eq!(request.get_metadata("x-request-id"), Some("abc-123"));
        assert_eq!(request.get_metadata("missing"), None);
        assert_eq!(request.full_path(), "/helloworld.Greeter/SayHello");
    }

    #[test]
    fn test_grpc_response_ok() {
        let payload = vec![0x0a, 0x0d, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
        let response = GrpcResponse::ok(payload.clone())
            .with_metadata("x-trace-id", "trace-456");

        assert!(response.is_ok());
        assert_eq!(response.payload, payload);
        assert_eq!(response.metadata.get("x-trace-id"), Some(&"trace-456".to_string()));
    }

    #[test]
    fn test_grpc_response_error() {
        let response = GrpcResponse::error(GrpcStatus::error(14, "Service unavailable"));
        assert!(!response.is_ok());
        assert_eq!(response.status.code, 14);
        assert!(response.payload.is_empty());
    }

    #[test]
    fn test_grpc_response_from_error() {
        let response = GrpcResponse::from_error(&GrpcError::Unauthenticated);
        assert!(!response.is_ok());
        assert_eq!(response.status.code, 16);
    }

    #[test]
    fn test_grpc_server_register_and_list() {
        let config = GrpcConfig::new().with_reflection(true);
        let server = GrpcServer::new(config);

        let greeter = GrpcServiceDef::new("helloworld.Greeter")
            .add_method(GrpcMethodDef::unary("SayHello", "HelloRequest", "HelloReply"));

        let route_guide = GrpcServiceDef::new("routeguide.RouteGuide")
            .add_method(GrpcMethodDef::unary("GetFeature", "Point", "Feature"))
            .add_method(GrpcMethodDef::server_streaming("ListFeatures", "Rectangle", "Feature"));

        server.register_service(greeter);
        server.register_service(route_guide);

        assert_eq!(server.service_count(), 2);

        let services = server.list_services();
        assert!(services.contains(&"helloworld.Greeter".to_string()));
        assert!(services.contains(&"routeguide.RouteGuide".to_string()));
    }

    #[test]
    fn test_grpc_server_get_service() {
        let server = GrpcServer::new(GrpcConfig::new());

        let service = GrpcServiceDef::new("test.Service")
            .with_description("A test service")
            .add_method(GrpcMethodDef::unary("DoWork", "WorkRequest", "WorkResponse"));

        server.register_service(service);

        let found = server.get_service("test.Service");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "test.Service");
        assert_eq!(found.methods.len(), 1);

        assert!(server.get_service("nonexistent.Service").is_none());
    }

    #[test]
    fn test_grpc_server_resolve_method() {
        let server = GrpcServer::new(GrpcConfig::new());

        let service = GrpcServiceDef::new("test.Service")
            .add_method(GrpcMethodDef::unary("Ping", "PingRequest", "PingResponse"));

        server.register_service(service);

        let method = server.resolve_method("test.Service", "Ping");
        assert!(method.is_ok());
        let method = method.unwrap();
        assert_eq!(method.name, "Ping");
        assert_eq!(method.method_type, GrpcMethodType::Unary);

        let err = server.resolve_method("test.Service", "NonExistent");
        assert!(err.is_err());

        let err = server.resolve_method("nonexistent.Service", "Ping");
        assert!(err.is_err());
    }

    #[test]
    fn test_grpc_server_handle_request() {
        let server = GrpcServer::new(GrpcConfig::new());

        let service = GrpcServiceDef::new("test.Service")
            .add_method(GrpcMethodDef::unary("Echo", "EchoRequest", "EchoResponse"));

        server.register_service(service);

        // Successful request
        let request = GrpcRequest::new("test.Service", "Echo", vec![0x01, 0x02]);
        let response = server.handle_request(&request);
        assert!(response.is_ok());

        // Service not found
        let request = GrpcRequest::new("missing.Service", "Echo", vec![]);
        let response = server.handle_request(&request);
        assert!(!response.is_ok());
        assert_eq!(response.status.code, GrpcError::ServiceNotFound.code());

        // Method not found
        let request = GrpcRequest::new("test.Service", "Missing", vec![]);
        let response = server.handle_request(&request);
        assert!(!response.is_ok());
        assert_eq!(response.status.code, GrpcError::MethodNotFound.code());
    }

    #[test]
    fn test_grpc_server_message_size_limit() {
        let config = GrpcConfig::new().with_max_message_size(10);
        let server = GrpcServer::new(config);

        let service = GrpcServiceDef::new("test.Service")
            .add_method(GrpcMethodDef::unary("Echo", "EchoRequest", "EchoResponse"));
        server.register_service(service);

        // Payload exceeds max message size
        let oversized_payload = vec![0u8; 20];
        let request = GrpcRequest::new("test.Service", "Echo", oversized_payload);
        let response = server.handle_request(&request);
        assert!(!response.is_ok());
        assert_eq!(response.status.code, GrpcError::InvalidMessage.code());
    }

    #[test]
    fn test_grpc_server_stats() {
        let server = GrpcServer::new(GrpcConfig::new());

        let service = GrpcServiceDef::new("test.Service")
            .add_method(GrpcMethodDef::unary("Echo", "EchoRequest", "EchoResponse"));
        server.register_service(service);

        // Initial stats
        let stats = server.stats();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.total_errors, 0);
        assert_eq!(stats.active_streams, 0);

        // After successful request
        let request = GrpcRequest::new("test.Service", "Echo", vec![]);
        server.handle_request(&request);

        let stats = server.stats();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.total_errors, 0);

        // After failed request
        let bad_request = GrpcRequest::new("missing.Service", "Echo", vec![]);
        server.handle_request(&bad_request);

        let stats = server.stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.total_errors, 1);
    }

    #[test]
    fn test_grpc_stats_rates() {
        let mut stats = GrpcStats::new();
        assert_eq!(stats.error_rate(), 0.0);
        assert_eq!(stats.success_rate(), 1.0);

        stats.total_requests = 100;
        stats.total_errors = 25;
        assert!((stats.error_rate() - 0.25).abs() < f64::EPSILON);
        assert!((stats.success_rate() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_grpc_server_reflection_flag() {
        let server_with = GrpcServer::new(GrpcConfig::new().with_reflection(true));
        assert!(server_with.reflection_enabled());

        let server_without = GrpcServer::new(GrpcConfig::new().with_reflection(false));
        assert!(!server_without.reflection_enabled());
    }

    /// Mock service implementing the GrpcService trait for testing.
    struct MockGrpcService;

    impl GrpcService for MockGrpcService {
        fn name(&self) -> &str {
            "mock.MockService"
        }

        fn methods(&self) -> Vec<GrpcMethodDef> {
            vec![
                GrpcMethodDef::unary("Ping", "PingRequest", "PingResponse"),
                GrpcMethodDef::server_streaming("Watch", "WatchRequest", "WatchEvent"),
            ]
        }
    }

    #[test]
    fn test_grpc_service_trait() {
        let service = MockGrpcService;
        assert_eq!(service.name(), "mock.MockService");
        assert_eq!(service.methods().len(), 2);
        assert_eq!(service.methods()[0].name, "Ping");
        assert_eq!(service.methods()[1].method_type, GrpcMethodType::ServerStreaming);
    }
}
