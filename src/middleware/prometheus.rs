//! Prometheus metrics middleware for Cello (inspired by Litestar).
//!
//! Provides:
//! - Request count metrics
//! - Request duration histograms
//! - Response size metrics
//! - Status code distribution
//! - Active requests gauge
//! - Custom metrics support
//! - Label support (method, path, status)

use prometheus::{
    register_counter_vec_with_registry, register_gauge_vec_with_registry,
    register_histogram_vec_with_registry, CounterVec, Encoder, GaugeVec, HistogramVec, Registry,
    TextEncoder,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Configuration
// ============================================================================

/// Prometheus middleware configuration.
#[derive(Clone)]
pub struct PrometheusConfig {
    /// Metrics endpoint path
    pub endpoint: String,
    /// Metrics namespace
    pub namespace: String,
    /// Metrics subsystem
    pub subsystem: String,
    /// Histogram buckets for request duration
    pub buckets: Vec<f64>,
    /// Paths to exclude from metrics
    pub exclude_paths: Vec<String>,
    /// Include request/response body sizes
    pub track_body_size: bool,
    /// Track method label
    pub track_method: bool,
    /// Track path label
    pub track_path: bool,
    /// Track status label
    pub track_status: bool,
    /// Max path cardinality (to prevent label explosion)
    pub max_path_cardinality: usize,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            endpoint: "/metrics".to_string(),
            namespace: "cello".to_string(),
            subsystem: "http".to_string(),
            buckets: vec![
                0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
            exclude_paths: vec!["/metrics".to_string(), "/health".to_string()],
            track_body_size: true,
            track_method: true,
            track_path: true,
            track_status: true,
            max_path_cardinality: 100,
        }
    }
}

impl PrometheusConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = endpoint.to_string();
        self
    }

    pub fn namespace(mut self, namespace: &str) -> Self {
        self.namespace = namespace.to_string();
        self
    }

    pub fn subsystem(mut self, subsystem: &str) -> Self {
        self.subsystem = subsystem.to_string();
        self
    }

    pub fn buckets(mut self, buckets: Vec<f64>) -> Self {
        self.buckets = buckets;
        self
    }

    pub fn exclude_path(mut self, path: &str) -> Self {
        self.exclude_paths.push(path.to_string());
        self
    }

    pub fn track_body_size(mut self, enabled: bool) -> Self {
        self.track_body_size = enabled;
        self
    }
}

// ============================================================================
// Metrics
// ============================================================================

/// Prometheus metrics collector.
#[derive(Clone)]
pub struct PrometheusMetrics {
    /// HTTP requests total counter
    pub http_requests_total: CounterVec,
    /// HTTP request duration histogram
    pub http_request_duration_seconds: HistogramVec,
    /// HTTP request size histogram
    pub http_request_size_bytes: HistogramVec,
    /// HTTP response size histogram
    pub http_response_size_bytes: HistogramVec,
    /// HTTP requests in progress gauge
    pub http_requests_in_progress: GaugeVec,
    /// The registry for these metrics
    pub registry: Arc<Registry>,
}

impl PrometheusMetrics {
    /// Create new metrics with the given config.
    pub fn new(config: &PrometheusConfig) -> Result<Self, prometheus::Error> {
        let registry = Arc::new(Registry::new());

        // Build label names based on config
        let mut labels_with_status = Vec::new();
        if config.track_method {
            labels_with_status.push("method");
        }
        if config.track_path {
            labels_with_status.push("path");
        }
        if config.track_status {
            labels_with_status.push("status");
        }

        let mut labels_no_status = Vec::new();
        if config.track_method {
            labels_no_status.push("method");
        }
        if config.track_path {
            labels_no_status.push("path");
        }

        // HTTP requests total
        let http_requests_total = register_counter_vec_with_registry!(
            format!("{}_{}_requests_total", config.namespace, config.subsystem),
            "Total number of HTTP requests",
            &labels_with_status,
            registry.clone()
        )?;

        // HTTP request duration
        let http_request_duration_seconds = register_histogram_vec_with_registry!(
            format!(
                "{}_{}_request_duration_seconds",
                config.namespace, config.subsystem
            ),
            "HTTP request latencies in seconds",
            &labels_with_status,
            config.buckets.clone(),
            registry.clone()
        )?;

        // HTTP request size
        let http_request_size_bytes = register_histogram_vec_with_registry!(
            format!(
                "{}_{}_request_size_bytes",
                config.namespace, config.subsystem
            ),
            "HTTP request sizes in bytes",
            &labels_no_status,
            vec![100.0, 1000.0, 10000.0, 100000.0, 1000000.0],
            registry.clone()
        )?;

        // HTTP response size
        let http_response_size_bytes = register_histogram_vec_with_registry!(
            format!(
                "{}_{}_response_size_bytes",
                config.namespace, config.subsystem
            ),
            "HTTP response sizes in bytes",
            &labels_with_status,
            vec![100.0, 1000.0, 10000.0, 100000.0, 1000000.0],
            registry.clone()
        )?;

        // HTTP requests in progress
        let http_requests_in_progress = register_gauge_vec_with_registry!(
            format!(
                "{}_{}_requests_in_progress",
                config.namespace, config.subsystem
            ),
            "Number of HTTP requests in progress",
            &labels_no_status,
            registry.clone()
        )?;

        Ok(Self {
            http_requests_total,
            http_request_duration_seconds,
            http_request_size_bytes,
            http_response_size_bytes,
            http_requests_in_progress,
            registry,
        })
    }

    /// Get the metrics as a string in Prometheus text format.
    pub fn encode(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }
}

// ============================================================================
// Middleware
// ============================================================================

/// Prometheus metrics middleware.
pub struct PrometheusMiddleware {
    config: PrometheusConfig,
    metrics: Arc<PrometheusMetrics>,
    path_cache: Arc<parking_lot::RwLock<HashMap<String, String>>>,
}

impl PrometheusMiddleware {
    /// Create new Prometheus middleware.
    pub fn new() -> Result<Self, prometheus::Error> {
        let config = PrometheusConfig::default();
        let metrics = Arc::new(PrometheusMetrics::new(&config)?);

        Ok(Self {
            config,
            metrics,
            path_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        })
    }

    /// Create with custom config.
    pub fn with_config(config: PrometheusConfig) -> Result<Self, prometheus::Error> {
        let metrics = Arc::new(PrometheusMetrics::new(&config)?);

        Ok(Self {
            config,
            metrics,
            path_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        })
    }

    /// Get the metrics registry.
    pub fn metrics(&self) -> Arc<PrometheusMetrics> {
        self.metrics.clone()
    }

    /// Check if path should be excluded from metrics.
    fn should_exclude(&self, path: &str) -> bool {
        self.config.exclude_paths.iter().any(|p| path.starts_with(p))
    }

    /// Get normalized path for label (to prevent cardinality explosion).
    fn normalize_path(&self, path: &str) -> String {
        // Check cache first
        {
            let cache = self.path_cache.read();
            if let Some(normalized) = cache.get(path) {
                return normalized.clone();
            }
        }

        // Check if we've hit the cardinality limit
        {
            let cache = self.path_cache.read();
            if cache.len() >= self.config.max_path_cardinality {
                return "OTHER".to_string();
            }
        }

        // For now, just use the path as-is
        // In production, you might want to:
        // - Replace path parameters with placeholders (/users/123 -> /users/:id)
        // - Group similar paths
        let normalized = path.to_string();

        // Cache the normalized path
        self.path_cache
            .write()
            .insert(path.to_string(), normalized.clone());

        normalized
    }

    /// Build label values based on config.
    fn build_labels(&self, method: &str, path: &str, status: u16) -> Vec<String> {
        let mut labels = Vec::new();

        if self.config.track_method {
            labels.push(method.to_string());
        }
        if self.config.track_path {
            labels.push(self.normalize_path(path));
        }
        if self.config.track_status {
            labels.push(status.to_string());
        }

        labels
    }

    /// Serve metrics endpoint.
    fn serve_metrics(&self) -> Response {
        match self.metrics.encode() {
            Ok(metrics) => {
                let mut response = Response::new(200);
                response.set_header("Content-Type", "text/plain; version=0.0.4");
                response.set_body(metrics.into_bytes());
                response
            }
            Err(e) => {
                let mut response = Response::new(500);
                response.set_body(format!("Error encoding metrics: {}", e).into_bytes());
                response
            }
        }
    }
}

impl Default for PrometheusMiddleware {
    fn default() -> Self {
        Self::new().expect("Failed to create PrometheusMiddleware")
    }
}

impl Middleware for PrometheusMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        // Serve metrics endpoint
        if request.path == self.config.endpoint {
            return Ok(MiddlewareAction::Stop(self.serve_metrics()));
        }

        // Skip excluded paths
        if self.should_exclude(&request.path) {
            return Ok(MiddlewareAction::Continue);
        }

        // Increment in-progress requests
        let labels = self.build_labels(&request.method, &request.path, 0);
        self.metrics
            .http_requests_in_progress
            .with_label_values(
                &labels
                    .iter()
                    .take(labels.len().saturating_sub(1))
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>(),
            )
            .inc();

        // Track request size
        if self.config.track_body_size && !request.body.is_empty() {
            self.metrics
                .http_request_size_bytes
                .with_label_values(
                    &labels
                        .iter()
                        .take(labels.len().saturating_sub(1))
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                )
                .observe(request.body.len() as f64);
        }

        // Store start time in request context
        request.context.insert(
            "__prometheus_start_time__".to_string(),
            serde_json::json!(Instant::now().elapsed().as_secs_f64()),
        );

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        // Skip excluded paths
        if self.should_exclude(&request.path) {
            return Ok(MiddlewareAction::Continue);
        }

        let labels = self.build_labels(&request.method, &request.path, response.status);
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();

        // Increment request counter
        self.metrics
            .http_requests_total
            .with_label_values(&label_refs)
            .inc();

        // Decrement in-progress requests
        let in_progress_labels: Vec<&str> = labels
            .iter()
            .take(labels.len().saturating_sub(1))
            .map(|s| s.as_str())
            .collect();
        self.metrics
            .http_requests_in_progress
            .with_label_values(&in_progress_labels)
            .dec();

        // Track response size
        if self.config.track_body_size {
            self.metrics
                .http_response_size_bytes
                .with_label_values(&label_refs)
                .observe(response.body_bytes().len() as f64);
        }

        // Track request duration
        if let Some(start_time) = request.context.get("__prometheus_start_time__") {
            if let Some(start) = start_time.as_f64() {
                let duration = Instant::now().elapsed().as_secs_f64() - start;
                self.metrics
                    .http_request_duration_seconds
                    .with_label_values(&label_refs)
                    .observe(duration);
            }
        }

        Ok(MiddlewareAction::Continue)
    }

    fn priority(&self) -> i32 {
        -200 // Run very early
    }

    fn name(&self) -> &str {
        "prometheus"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode() {
        let config = PrometheusConfig::default();
        let metrics = PrometheusMetrics::new(&config).unwrap();
        
        // Record something
        metrics.http_requests_total
            .with_label_values(&["GET", "/", "200"])
            .inc();
            
        let encoded = metrics.encode().unwrap();
        println!("Encoded metrics:\n{}", encoded);
        assert!(encoded.contains("http_requests_total"));
        assert!(encoded.contains("method=\"GET\""));
    }

    #[test]
    fn test_config() {
        let config = PrometheusConfig::new()
            .namespace("myapp")
            .subsystem("api")
            .endpoint("/custom_metrics")
            .exclude_path("/health");

        assert_eq!(config.namespace, "myapp");
        assert_eq!(config.subsystem, "api");
        assert_eq!(config.endpoint, "/custom_metrics");
        assert!(config.exclude_paths.contains(&"/health".to_string()));
    }

    #[test]
    fn test_metrics_creation() {
        let config = PrometheusConfig::default();
        let metrics = PrometheusMetrics::new(&config);
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_middleware_creation() {
        let middleware = PrometheusMiddleware::new();
        assert!(middleware.is_ok());
    }

    #[test]
    fn test_metrics_encode() {
        let config = PrometheusConfig::default();
        let metrics = PrometheusMetrics::new(&config).unwrap();
        let encoded = metrics.encode();
        assert!(encoded.is_ok());

        let text = encoded.unwrap();
        assert!(text.contains("cello_http_requests_total"));
    }

    #[test]
    fn test_should_exclude() {
        let middleware = PrometheusMiddleware::new().unwrap();
        assert!(middleware.should_exclude("/metrics"));
        assert!(middleware.should_exclude("/health"));
        assert!(!middleware.should_exclude("/api/users"));
    }

    #[test]
    fn test_normalize_path() {
        let middleware = PrometheusMiddleware::new().unwrap();
        let normalized = middleware.normalize_path("/api/users/123");
        assert_eq!(normalized, "/api/users/123");
    }
}
