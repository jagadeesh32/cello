//! Health Check Middleware for Kubernetes-compatible health probes.
//!
//! Provides:
//! - Liveness probe (/health/live) - Is the application running?
//! - Readiness probe (/health/ready) - Is the application ready to serve traffic?
//! - Startup probe (/health/startup) - Has the application finished starting?
//! - Combined health endpoint (/health) - Full health report
//!
//! # Example
//! ```python
//! from cello import App
//! from cello.middleware import HealthCheckConfig, HealthStatus
//!
//! app = App()
//!
//! @app.health_check("database")
//! async def check_database():
//!     await db.ping()
//!     return HealthStatus.UP
//!
//! app.enable_health_checks(HealthCheckConfig(
//!     path="/health",
//!     include_details=True
//! ))
//! ```

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Health status of a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    /// Component is healthy
    Up,
    /// Component is unhealthy
    Down,
    /// Component health is unknown
    Unknown,
    /// Component is partially healthy (degraded)
    Degraded,
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Up | HealthStatus::Degraded)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Up => "UP",
            HealthStatus::Down => "DOWN",
            HealthStatus::Unknown => "UNKNOWN",
            HealthStatus::Degraded => "DEGRADED",
        }
    }
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus::Unknown
    }
}

/// Result of a health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub name: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
    pub duration_ms: u64,
    pub timestamp: String,
}

impl HealthCheckResult {
    pub fn up(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: HealthStatus::Up,
            message: None,
            details: None,
            duration_ms: 0,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    pub fn down(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: HealthStatus::Down,
            message: Some(message.to_string()),
            details: None,
            duration_ms: 0,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    pub fn with_details(mut self, details: HashMap<String, serde_json::Value>) -> Self {
        self.details = Some(details);
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration_ms = duration.as_millis() as u64;
        self
    }
}

/// Aggregated health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub checks: HashMap<String, HealthCheckResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemInfo>,
}

impl HealthReport {
    pub fn new() -> Self {
        Self {
            status: HealthStatus::Up,
            timestamp: Utc::now().to_rfc3339(),
            version: None,
            uptime_seconds: None,
            checks: HashMap::new(),
            system: None,
        }
    }

    pub fn with_check(mut self, result: HealthCheckResult) -> Self {
        // Update overall status
        if result.status == HealthStatus::Down {
            self.status = HealthStatus::Down;
        } else if result.status == HealthStatus::Degraded && self.status != HealthStatus::Down {
            self.status = HealthStatus::Degraded;
        }
        self.checks.insert(result.name.clone(), result);
        self
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    pub fn with_uptime(mut self, uptime: Duration) -> Self {
        self.uptime_seconds = Some(uptime.as_secs());
        self
    }

    pub fn with_system_info(mut self, info: SystemInfo) -> Self {
        self.system = Some(info);
        self
    }
}

impl Default for HealthReport {
    fn default() -> Self {
        Self::new()
    }
}

/// System information for health reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
}

impl SystemInfo {
    pub fn collect() -> Self {
        use sysinfo::System;

        let mut sys = System::new_all();
        sys.refresh_all();

        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        // CPU usage calculation (using 0.0 as fallback since global_cpu_usage requires refresh)
        let cpu_usage = 0.0f64; // CPU metrics require multiple samples
        let memory_used = sys.used_memory() / 1024 / 1024;
        let memory_total = sys.total_memory() / 1024 / 1024;
        let memory_usage = if memory_total > 0 {
            (memory_used as f64 / memory_total as f64) * 100.0
        } else {
            0.0
        };

        Self {
            hostname,
            cpu_usage_percent: cpu_usage,
            memory_usage_percent: memory_usage,
            memory_used_mb: memory_used,
            memory_total_mb: memory_total,
        }
    }
}

/// Configuration for health check middleware.
#[derive(Clone)]
pub struct HealthCheckConfig {
    /// Base path for health endpoints
    pub base_path: String,
    /// Include detailed check results
    pub include_details: bool,
    /// Include system information
    pub include_system_info: bool,
    /// Application version
    pub version: Option<String>,
    /// Timeout for health checks
    pub timeout: Duration,
    /// Cache health check results
    pub cache_duration: Option<Duration>,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            base_path: "/health".to_string(),
            include_details: true,
            include_system_info: false,
            version: None,
            timeout: Duration::from_secs(5),
            cache_duration: Some(Duration::from_secs(5)),
        }
    }
}

impl HealthCheckConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(mut self, path: &str) -> Self {
        self.base_path = path.to_string();
        self
    }

    pub fn with_details(mut self, include: bool) -> Self {
        self.include_details = include;
        self
    }

    pub fn with_system_info(mut self, include: bool) -> Self {
        self.include_system_info = include;
        self
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// A health check function.
pub type HealthCheckFn = Box<dyn Fn() -> HealthCheckResult + Send + Sync>;

/// Cached health check result.
struct CachedResult {
    result: HealthCheckResult,
    cached_at: Instant,
}

/// Health check middleware.
pub struct HealthCheckMiddleware {
    config: HealthCheckConfig,
    start_time: Instant,
    ready: AtomicBool,
    checks: RwLock<HashMap<String, HealthCheckFn>>,
    cache: RwLock<HashMap<String, CachedResult>>,
}

impl HealthCheckMiddleware {
    /// Create a new health check middleware.
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            start_time: Instant::now(),
            ready: AtomicBool::new(true),
            checks: RwLock::new(HashMap::new()),
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Register a health check.
    pub fn register_check<F>(&self, name: &str, check: F)
    where
        F: Fn() -> HealthCheckResult + Send + Sync + 'static,
    {
        let mut checks = self.checks.write();
        checks.insert(name.to_string(), Box::new(check));
    }

    /// Set readiness status.
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
    }

    /// Check if application is ready.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    /// Run all registered health checks.
    pub fn run_checks(&self) -> HealthReport {
        let mut report = HealthReport::new()
            .with_uptime(self.start_time.elapsed());

        if let Some(ref version) = self.config.version {
            report = report.with_version(version);
        }

        if self.config.include_system_info {
            report = report.with_system_info(SystemInfo::collect());
        }

        let checks = self.checks.read();
        for (name, check) in checks.iter() {
            let start = Instant::now();

            // Check cache
            if let Some(cache_duration) = self.config.cache_duration {
                let cache = self.cache.read();
                if let Some(cached) = cache.get(name) {
                    if cached.cached_at.elapsed() < cache_duration {
                        report = report.with_check(cached.result.clone());
                        continue;
                    }
                }
            }

            // Run the check
            let mut result = check();
            result = result.with_duration(start.elapsed());

            // Update cache
            if self.config.cache_duration.is_some() {
                let mut cache = self.cache.write();
                cache.insert(
                    name.clone(),
                    CachedResult {
                        result: result.clone(),
                        cached_at: Instant::now(),
                    },
                );
            }

            if self.config.include_details {
                report = report.with_check(result);
            } else if result.status == HealthStatus::Down {
                // Always include failed checks
                report.status = HealthStatus::Down;
            }
        }

        report
    }

    /// Generate liveness response.
    fn liveness_response(&self) -> Response {
        let status = HealthStatus::Up;
        let body = serde_json::json!({
            "status": status.as_str(),
            "timestamp": Utc::now().to_rfc3339()
        });
        Response::from_json_value(body, 200)
    }

    /// Generate readiness response.
    fn readiness_response(&self) -> Response {
        let ready = self.is_ready();
        let status = if ready { HealthStatus::Up } else { HealthStatus::Down };
        let http_status = if ready { 200 } else { 503 };

        let body = serde_json::json!({
            "status": status.as_str(),
            "ready": ready,
            "timestamp": Utc::now().to_rfc3339()
        });
        Response::from_json_value(body, http_status)
    }

    /// Generate startup response (same as readiness for now).
    fn startup_response(&self) -> Response {
        self.readiness_response()
    }

    /// Generate full health response.
    fn full_health_response(&self) -> Response {
        let report = self.run_checks();
        let http_status = if report.status.is_healthy() { 200 } else { 503 };
        let body = serde_json::to_value(&report).unwrap_or_else(|_| serde_json::json!({"status": "ERROR"}));
        Response::from_json_value(body, http_status)
    }
}

impl Middleware for HealthCheckMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        let base = &self.config.base_path;

        // Check if this is a health endpoint
        if !request.path.starts_with(base) {
            return Ok(MiddlewareAction::Continue);
        }

        let response = if request.path == format!("{}/live", base)
            || request.path == format!("{}/liveness", base)
        {
            self.liveness_response()
        } else if request.path == format!("{}/ready", base)
            || request.path == format!("{}/readiness", base)
        {
            self.readiness_response()
        } else if request.path == format!("{}/startup", base) {
            self.startup_response()
        } else if request.path == *base || request.path == format!("{}/", base) {
            self.full_health_response()
        } else {
            return Ok(MiddlewareAction::Continue);
        };

        Ok(MiddlewareAction::Stop(response))
    }

    fn priority(&self) -> i32 {
        -400 // Run very early
    }

    fn name(&self) -> &str {
        "health_check"
    }
}

/// Built-in health checks.
pub mod builtin {
    use super::*;

    /// Memory health check - fails if memory usage exceeds threshold.
    pub fn memory_check(threshold_percent: f64) -> impl Fn() -> HealthCheckResult + Send + Sync {
        move || {
            let sys = sysinfo::System::new_all();
            let used = sys.used_memory();
            let total = sys.total_memory();
            let usage = if total > 0 {
                (used as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            let mut details = HashMap::new();
            details.insert("usage_percent".to_string(), serde_json::json!(usage));
            details.insert("threshold_percent".to_string(), serde_json::json!(threshold_percent));
            details.insert("used_mb".to_string(), serde_json::json!(used / 1024 / 1024));
            details.insert("total_mb".to_string(), serde_json::json!(total / 1024 / 1024));

            if usage > threshold_percent {
                HealthCheckResult::down("memory", &format!("Memory usage {:.1}% exceeds threshold {:.1}%", usage, threshold_percent))
                    .with_details(details)
            } else {
                HealthCheckResult::up("memory")
                    .with_details(details)
            }
        }
    }

    /// Disk health check - fails if disk usage exceeds threshold.
    pub fn disk_check(path: &str, threshold_percent: f64) -> impl Fn() -> HealthCheckResult + Send + Sync {
        let path = path.to_string();
        move || {
            use sysinfo::Disks;

            let disks = Disks::new_with_refreshed_list();

            for disk in disks.list() {
                if disk.mount_point().to_string_lossy().starts_with(&path) {
                    let total = disk.total_space();
                    let available = disk.available_space();
                    let used = total - available;
                    let usage = if total > 0 {
                        (used as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };

                    let mut details = HashMap::new();
                    details.insert("path".to_string(), serde_json::json!(path));
                    details.insert("usage_percent".to_string(), serde_json::json!(usage));
                    details.insert("threshold_percent".to_string(), serde_json::json!(threshold_percent));

                    if usage > threshold_percent {
                        return HealthCheckResult::down("disk", &format!("Disk usage {:.1}% exceeds threshold {:.1}%", usage, threshold_percent))
                            .with_details(details);
                    } else {
                        return HealthCheckResult::up("disk")
                            .with_details(details);
                    }
                }
            }

            HealthCheckResult::down("disk", "Disk not found")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status() {
        assert!(HealthStatus::Up.is_healthy());
        assert!(HealthStatus::Degraded.is_healthy());
        assert!(!HealthStatus::Down.is_healthy());
        assert!(!HealthStatus::Unknown.is_healthy());
    }

    #[test]
    fn test_health_report() {
        let report = HealthReport::new()
            .with_version("1.0.0")
            .with_check(HealthCheckResult::up("test"));

        assert_eq!(report.status, HealthStatus::Up);
        assert_eq!(report.version, Some("1.0.0".to_string()));
        assert!(report.checks.contains_key("test"));
    }

    #[test]
    fn test_health_report_degradation() {
        let report = HealthReport::new()
            .with_check(HealthCheckResult::up("service1"))
            .with_check(HealthCheckResult::down("service2", "Connection failed"));

        assert_eq!(report.status, HealthStatus::Down);
    }

    #[test]
    fn test_config_builder() {
        let config = HealthCheckConfig::new()
            .with_path("/healthz")
            .with_details(true)
            .with_version("2.0.0");

        assert_eq!(config.base_path, "/healthz");
        assert!(config.include_details);
        assert_eq!(config.version, Some("2.0.0".to_string()));
    }
}
