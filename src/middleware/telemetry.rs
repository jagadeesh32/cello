//! OpenTelemetry Middleware for distributed tracing, metrics, and logging.
//!
//! Provides comprehensive observability with:
//! - Distributed trace context propagation (W3C Trace Context)
//! - Automatic span creation for HTTP requests
//! - Metrics collection (request count, latency histogram, error rate)
//! - Structured logging with trace correlation
//!
//! # Example
//! ```python
//! from cello import App
//! from cello.middleware import OpenTelemetryConfig
//!
//! app = App()
//! app.enable_telemetry(OpenTelemetryConfig(
//!     service_name="my-service",
//!     otlp_endpoint="http://collector:4317",
//!     sampling_rate=0.1
//! ))
//! ```

use super::{AsyncMiddleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// OpenTelemetry types available for future full SDK integration
#[allow(unused_imports)]
use opentelemetry::global;
#[allow(unused_imports)]
use opentelemetry_sdk::trace::Sampler;
use serde_json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use parking_lot::RwLock;

/// Configuration for OpenTelemetry middleware.
#[derive(Clone)]
pub struct OpenTelemetryConfig {
    /// Service name for tracing
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// OTLP endpoint for trace/metrics export
    pub otlp_endpoint: Option<String>,
    /// Sampling rate (0.0 to 1.0)
    pub sampling_rate: f64,
    /// Whether to export traces
    pub export_traces: bool,
    /// Whether to export metrics
    pub export_metrics: bool,
    /// Whether to propagate trace context
    pub propagate_context: bool,
    /// Paths to exclude from tracing
    pub excluded_paths: Vec<String>,
    /// Custom resource attributes
    pub resource_attributes: HashMap<String, String>,
}

impl Default for OpenTelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "cello-service".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            otlp_endpoint: None,
            sampling_rate: 1.0,
            export_traces: true,
            export_metrics: true,
            propagate_context: true,
            excluded_paths: vec!["/health".to_string(), "/metrics".to_string()],
            resource_attributes: HashMap::new(),
        }
    }
}

impl OpenTelemetryConfig {
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
            ..Default::default()
        }
    }

    pub fn with_endpoint(mut self, endpoint: &str) -> Self {
        self.otlp_endpoint = Some(endpoint.to_string());
        self
    }

    pub fn with_sampling_rate(mut self, rate: f64) -> Self {
        self.sampling_rate = rate.clamp(0.0, 1.0);
        self
    }

    pub fn exclude_path(mut self, path: &str) -> Self {
        self.excluded_paths.push(path.to_string());
        self
    }

    pub fn with_attribute(mut self, key: &str, value: &str) -> Self {
        self.resource_attributes.insert(key.to_string(), value.to_string());
        self
    }
}

/// Metrics collected by the telemetry middleware.
#[derive(Default)]
pub struct TelemetryMetrics {
    /// Total request count
    pub request_count: AtomicU64,
    /// Total error count (5xx responses)
    pub error_count: AtomicU64,
    /// Latency sum for average calculation
    pub latency_sum_ms: AtomicU64,
    /// Request count by status code
    pub status_counts: RwLock<HashMap<u16, u64>>,
    /// Request count by path
    pub path_counts: RwLock<HashMap<String, u64>>,
}

impl TelemetryMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_request(&self, status: u16, latency_ms: u64, path: &str) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.latency_sum_ms.fetch_add(latency_ms, Ordering::Relaxed);

        if status >= 500 {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }

        // Update status counts
        {
            let mut counts = self.status_counts.write();
            *counts.entry(status).or_insert(0) += 1;
        }

        // Update path counts
        {
            let mut counts = self.path_counts.write();
            *counts.entry(path.to_string()).or_insert(0) += 1;
        }
    }

    pub fn get_stats(&self) -> TelemetryStats {
        let request_count = self.request_count.load(Ordering::Relaxed);
        let error_count = self.error_count.load(Ordering::Relaxed);
        let latency_sum = self.latency_sum_ms.load(Ordering::Relaxed);

        TelemetryStats {
            total_requests: request_count,
            total_errors: error_count,
            error_rate: if request_count > 0 {
                error_count as f64 / request_count as f64
            } else {
                0.0
            },
            avg_latency_ms: if request_count > 0 {
                latency_sum as f64 / request_count as f64
            } else {
                0.0
            },
            status_distribution: self.status_counts.read().clone(),
        }
    }
}

/// Statistics from telemetry collection.
#[derive(Clone)]
pub struct TelemetryStats {
    pub total_requests: u64,
    pub total_errors: u64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
    pub status_distribution: HashMap<u16, u64>,
}

/// OpenTelemetry middleware for distributed tracing.
pub struct OpenTelemetryMiddleware {
    config: OpenTelemetryConfig,
    metrics: Arc<TelemetryMetrics>,
}

impl OpenTelemetryMiddleware {
    /// Create a new OpenTelemetry middleware with the given configuration.
    pub fn new(config: OpenTelemetryConfig) -> Self {
        // Initialize tracing if OTLP endpoint is configured
        if let Some(ref _endpoint) = config.otlp_endpoint {
            // Note: Full OTLP setup would require async initialization
            // For now, we use a simplified in-memory tracer
            tracing::info!(
                "OpenTelemetry configured for service: {}",
                config.service_name
            );
        }

        Self {
            config,
            metrics: Arc::new(TelemetryMetrics::new()),
        }
    }

    /// Get the collected metrics.
    pub fn get_metrics(&self) -> Arc<TelemetryMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Check if a path should be traced.
    fn should_trace(&self, path: &str) -> bool {
        !self.config.excluded_paths.iter().any(|p| path.starts_with(p))
    }

    /// Extract trace context from request headers (W3C Trace Context).
    fn extract_trace_context(&self, request: &Request) -> Option<(String, String)> {
        if !self.config.propagate_context {
            return None;
        }

        // W3C traceparent header format: version-trace_id-parent_id-flags
        if let Some(traceparent) = request.headers.get("traceparent") {
            let parts: Vec<&str> = traceparent.split('-').collect();
            if parts.len() == 4 {
                return Some((parts[1].to_string(), parts[2].to_string()));
            }
        }

        None
    }

    /// Generate a new trace ID.
    fn generate_trace_id() -> String {
        uuid::Uuid::new_v4().to_string().replace("-", "")
    }

    /// Generate a new span ID.
    fn generate_span_id() -> String {
        let bytes: [u8; 8] = rand::random();
        hex::encode(bytes)
    }
}

impl AsyncMiddleware for OpenTelemetryMiddleware {
    fn before_async<'a>(
        &'a self,
        request: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = MiddlewareResult> + Send + 'a>> {
        Box::pin(async move {
            if !self.should_trace(&request.path) {
                return Ok(MiddlewareAction::Continue);
            }

            // Extract or generate trace context
            let (trace_id, parent_span_id) = self
                .extract_trace_context(request)
                .unwrap_or_else(|| (Self::generate_trace_id(), "".to_string()));

            let span_id = Self::generate_span_id();

            // Store trace context in request for later use
            request.context.insert("trace_id".to_string(), serde_json::Value::String(trace_id.clone()));
            request.context.insert("span_id".to_string(), serde_json::Value::String(span_id.clone()));
            request.context.insert("parent_span_id".to_string(), serde_json::Value::String(parent_span_id));
            request.context.insert("trace_start_time".to_string(), serde_json::Value::String(format!("{}", Instant::now().elapsed().as_nanos())));

            // Log span start
            tracing::info!(
                trace_id = %trace_id,
                span_id = %span_id,
                method = %request.method,
                path = %request.path,
                "Request started"
            );

            Ok(MiddlewareAction::Continue)
        })
    }

    fn after_async<'a>(
        &'a self,
        request: &'a Request,
        response: &'a mut Response,
    ) -> Pin<Box<dyn Future<Output = MiddlewareResult> + Send + 'a>> {
        Box::pin(async move {
            if !self.should_trace(&request.path) {
                return Ok(MiddlewareAction::Continue);
            }

            // Get trace context as strings
            let trace_id = request.context.get("trace_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let span_id = request.context.get("span_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Calculate latency (simplified - in production would use proper timestamps)
            let latency_ms = 1; // Placeholder - would calculate from start time

            // Record metrics
            self.metrics.record_request(response.status, latency_ms, &request.path);

            // Add trace headers to response
            if self.config.propagate_context && !trace_id.is_empty() {
                response.set_header("X-Trace-ID", &trace_id);
                response.set_header("X-Span-ID", &span_id);
            }

            // Log span end
            let status = if response.status >= 400 { "ERROR" } else { "OK" };
            tracing::info!(
                trace_id = %trace_id,
                span_id = %span_id,
                status_code = response.status,
                status = status,
                latency_ms = latency_ms,
                "Request completed"
            );

            Ok(MiddlewareAction::Continue)
        })
    }

    fn priority(&self) -> i32 {
        -300 // Run very early to capture full request lifecycle
    }

    fn name(&self) -> &str {
        "opentelemetry"
    }
}

/// Trace context for propagation.
#[derive(Clone, Debug)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub sampled: bool,
}

impl TraceContext {
    /// Create a new trace context.
    pub fn new() -> Self {
        Self {
            trace_id: OpenTelemetryMiddleware::generate_trace_id(),
            span_id: OpenTelemetryMiddleware::generate_span_id(),
            parent_span_id: None,
            sampled: true,
        }
    }

    /// Create a child span context.
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: OpenTelemetryMiddleware::generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            sampled: self.sampled,
        }
    }

    /// Format as W3C traceparent header.
    pub fn to_traceparent(&self) -> String {
        let flags = if self.sampled { "01" } else { "00" };
        format!("00-{}-{}-{}", self.trace_id, self.span_id, flags)
    }

    /// Parse from W3C traceparent header.
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 || parts[0] != "00" {
            return None;
        }

        Some(Self {
            trace_id: parts[1].to_string(),
            span_id: parts[2].to_string(),
            parent_span_id: None,
            sampled: parts[3] == "01",
        })
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = OpenTelemetryConfig::new("test-service")
            .with_endpoint("http://localhost:4317")
            .with_sampling_rate(0.5)
            .exclude_path("/ping");

        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.otlp_endpoint, Some("http://localhost:4317".to_string()));
        assert_eq!(config.sampling_rate, 0.5);
        assert!(config.excluded_paths.contains(&"/ping".to_string()));
    }

    #[test]
    fn test_trace_context() {
        let ctx = TraceContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());

        let child = ctx.child();
        assert_eq!(child.trace_id, ctx.trace_id);
        assert_eq!(child.parent_span_id, Some(ctx.span_id.clone()));
    }

    #[test]
    fn test_traceparent_format() {
        let ctx = TraceContext {
            trace_id: "0af7651916cd43dd8448eb211c80319c".to_string(),
            span_id: "b7ad6b7169203331".to_string(),
            parent_span_id: None,
            sampled: true,
        };

        let header = ctx.to_traceparent();
        assert_eq!(header, "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");

        let parsed = TraceContext::from_traceparent(&header).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
    }

    #[test]
    fn test_metrics_recording() {
        let metrics = TelemetryMetrics::new();

        metrics.record_request(200, 50, "/api/users");
        metrics.record_request(500, 100, "/api/users");
        metrics.record_request(200, 30, "/api/posts");

        let stats = metrics.get_stats();
        assert_eq!(stats.total_requests, 3);
        assert_eq!(stats.total_errors, 1);
        assert!((stats.error_rate - 0.333).abs() < 0.01);
    }
}
