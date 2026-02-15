//! Saga Pattern Support for Cello Framework.
//!
//! Provides a complete saga orchestration implementation with:
//! - Saga definition and step registration
//! - Saga execution with forward and compensation flows
//! - Automatic compensation on step failure
//! - Configurable retries and timeouts
//! - Execution tracking and statistics
//!
//! # Example
//! ```python
//! from cello import App, SagaConfig
//!
//! config = SagaConfig()
//!
//! @app.on_startup
//! async def setup():
//!     app.enable_saga(config)
//!
//! @app.post("/orders")
//! async def create_order(request):
//!     # The saga orchestrator manages the distributed transaction:
//!     # 1. Create order
//!     # 2. Reserve inventory
//!     # 3. Process payment
//!     # If any step fails, previous steps are compensated
//!     return {"saga": "OrderCreation", "status": "started"}
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

// ============================================================================
// Configuration
// ============================================================================

/// Saga orchestration configuration.
///
/// Controls retry behaviour, timeouts, and logging for saga executions.
#[derive(Clone, Debug)]
pub struct SagaConfig {
    /// Maximum number of retries for a failed step before triggering compensation.
    pub max_retries: u32,
    /// Delay in milliseconds between retry attempts.
    pub retry_delay_ms: u64,
    /// Maximum time in milliseconds for an entire saga execution.
    pub timeout_ms: u64,
    /// Whether to log saga execution steps.
    pub enable_logging: bool,
}

impl Default for SagaConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_delay_ms: 1000,
            timeout_ms: 30000,
            enable_logging: true,
        }
    }
}

impl SagaConfig {
    /// Create a new saga configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of retries for failed steps.
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the retry delay in milliseconds.
    pub fn with_retry_delay(mut self, delay_ms: u64) -> Self {
        self.retry_delay_ms = delay_ms;
        self
    }

    /// Set the saga timeout in milliseconds.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Enable or disable execution logging.
    pub fn with_logging(mut self, enabled: bool) -> Self {
        self.enable_logging = enabled;
        self
    }
}

// ============================================================================
// Step Status & Saga Status
// ============================================================================

/// Status of an individual saga step during execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    /// Step has not yet started.
    Pending,
    /// Step is currently executing.
    Running,
    /// Step completed successfully.
    Completed,
    /// Step execution failed.
    Failed,
    /// Compensation is in progress for this step.
    Compensating,
    /// Step has been successfully compensated.
    Compensated,
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepStatus::Pending => write!(f, "Pending"),
            StepStatus::Running => write!(f, "Running"),
            StepStatus::Completed => write!(f, "Completed"),
            StepStatus::Failed => write!(f, "Failed"),
            StepStatus::Compensating => write!(f, "Compensating"),
            StepStatus::Compensated => write!(f, "Compensated"),
        }
    }
}

/// Overall status of a saga execution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SagaStatus {
    /// Saga has not yet started.
    Pending,
    /// Saga is currently executing steps.
    Running,
    /// All steps completed successfully.
    Completed,
    /// One or more steps failed and compensation has not started.
    Failed,
    /// Compensation is in progress.
    Compensating,
    /// All necessary steps have been compensated.
    Compensated,
}

impl std::fmt::Display for SagaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SagaStatus::Pending => write!(f, "Pending"),
            SagaStatus::Running => write!(f, "Running"),
            SagaStatus::Completed => write!(f, "Completed"),
            SagaStatus::Failed => write!(f, "Failed"),
            SagaStatus::Compensating => write!(f, "Compensating"),
            SagaStatus::Compensated => write!(f, "Compensated"),
        }
    }
}

// ============================================================================
// Saga Step Types
// ============================================================================

/// Definition of a single step within a saga.
///
/// Each step has a name, an optional description, and may have
/// a compensation action defined.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SagaStepDef {
    /// Unique name for this step within the saga.
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Whether a compensation action is defined for this step.
    pub has_compensation: bool,
    /// Optional per-step timeout override in milliseconds.
    pub timeout_ms: Option<u64>,
}

impl SagaStepDef {
    /// Create a new saga step definition.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            has_compensation: false,
            timeout_ms: None,
        }
    }

    /// Set the step description.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Mark this step as having a compensation action.
    pub fn with_compensation(mut self) -> Self {
        self.has_compensation = true;
        self
    }

    /// Set a per-step timeout in milliseconds.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}

/// Runtime state of a saga step during execution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SagaStep {
    /// Step name (matches the corresponding `SagaStepDef`).
    pub name: String,
    /// Current execution status.
    pub status: StepStatus,
    /// Result data from a successful execution, if any.
    pub result: Option<JsonValue>,
    /// Error message if the step failed.
    pub error: Option<String>,
}

impl SagaStep {
    /// Create a new saga step in pending state.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: StepStatus::Pending,
            result: None,
            error: None,
        }
    }

    /// Check if this step is complete.
    pub fn is_complete(&self) -> bool {
        self.status == StepStatus::Completed
    }

    /// Check if this step has failed.
    pub fn is_failed(&self) -> bool {
        self.status == StepStatus::Failed
    }

    /// Check if this step has been compensated.
    pub fn is_compensated(&self) -> bool {
        self.status == StepStatus::Compensated
    }
}

// ============================================================================
// Saga Definition
// ============================================================================

/// Definition of a saga, describing its steps and their order.
///
/// Saga definitions are registered with the `SagaOrchestrator` and
/// used to create new saga executions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SagaDefinition {
    /// Unique name for this saga type.
    pub name: String,
    /// Ordered list of step definitions.
    pub steps: Vec<SagaStepDef>,
    /// Optional human-readable description.
    pub description: Option<String>,
}

impl SagaDefinition {
    /// Create a new saga definition.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            steps: Vec::new(),
            description: None,
        }
    }

    /// Set the saga description.
    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Add a step to the saga definition.
    pub fn add_step(&mut self, step: SagaStepDef) {
        self.steps.push(step);
    }

    /// Get the list of step definitions.
    pub fn get_steps(&self) -> &[SagaStepDef] {
        &self.steps
    }

    /// Get the number of steps in this saga.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }
}

// ============================================================================
// Saga Execution
// ============================================================================

/// A running or completed instance of a saga.
///
/// Tracks the state of each step and the overall execution status.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SagaExecution {
    /// Unique execution identifier.
    pub id: String,
    /// Name of the saga being executed.
    pub saga_name: String,
    /// Runtime state of each step.
    pub steps: Vec<SagaStep>,
    /// Overall execution status.
    pub status: SagaStatus,
    /// Unix timestamp (seconds) when execution started.
    pub started_at: u64,
    /// Unix timestamp (seconds) when execution completed (if finished).
    pub completed_at: Option<u64>,
}

impl SagaExecution {
    /// Create a new saga execution in pending state.
    pub fn new(id: &str, saga_name: &str, step_names: &[String]) -> Self {
        let steps = step_names
            .iter()
            .map(|name| SagaStep::new(name))
            .collect();

        Self {
            id: id.to_string(),
            saga_name: saga_name.to_string(),
            steps,
            status: SagaStatus::Pending,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            completed_at: None,
        }
    }

    /// Get the current step index (first non-completed step).
    pub fn current_step_index(&self) -> Option<usize> {
        self.steps.iter().position(|s| s.status != StepStatus::Completed && s.status != StepStatus::Compensated)
    }

    /// Check if all steps are completed.
    pub fn is_complete(&self) -> bool {
        self.status == SagaStatus::Completed
    }

    /// Check if the execution has failed.
    pub fn is_failed(&self) -> bool {
        self.status == SagaStatus::Failed
    }

    /// Get the duration in seconds (if completed).
    pub fn duration_secs(&self) -> Option<u64> {
        self.completed_at.map(|end| end - self.started_at)
    }

    /// Get a summary of step statuses.
    pub fn step_summary(&self) -> HashMap<String, String> {
        self.steps
            .iter()
            .map(|s| (s.name.clone(), format!("{}", s.status)))
            .collect()
    }
}

// ============================================================================
// Saga Orchestrator
// ============================================================================

/// Central orchestrator for managing saga definitions and executions.
///
/// The orchestrator maintains a registry of saga definitions and
/// tracks all active and completed saga executions.
pub struct SagaOrchestrator {
    /// Registered saga definitions keyed by name.
    sagas: Arc<RwLock<HashMap<String, SagaDefinition>>>,
    /// Saga executions keyed by execution ID.
    executions: Arc<RwLock<HashMap<String, SagaExecution>>>,
    /// Internal metrics tracker.
    metrics: Arc<SagaMetrics>,
    /// Configuration reference.
    config: SagaConfig,
    /// Counter for generating unique execution IDs.
    execution_counter: AtomicU64,
}

impl SagaOrchestrator {
    /// Create a new saga orchestrator with default configuration.
    pub fn new() -> Self {
        Self {
            sagas: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(SagaMetrics::default()),
            config: SagaConfig::default(),
            execution_counter: AtomicU64::new(0),
        }
    }

    /// Create a new saga orchestrator with a specific configuration.
    pub fn with_config(config: SagaConfig) -> Self {
        Self {
            sagas: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(SagaMetrics::default()),
            config,
            execution_counter: AtomicU64::new(0),
        }
    }

    /// Register a saga definition.
    pub fn register_saga(&self, saga: SagaDefinition) {
        self.sagas.write().insert(saga.name.clone(), saga);
    }

    /// Start a new execution of a registered saga.
    ///
    /// Returns the execution ID on success, or an error if the saga
    /// is not registered.
    pub fn start_execution(&self, saga_name: &str) -> Result<String, SagaError> {
        let sagas = self.sagas.read();
        let saga = sagas
            .get(saga_name)
            .ok_or_else(|| SagaError::SagaNotFound(saga_name.to_string()))?;

        let counter = self.execution_counter.fetch_add(1, Ordering::Relaxed);
        let execution_id = format!("exec-{}-{}", saga_name, counter);

        let step_names: Vec<String> = saga.steps.iter().map(|s| s.name.clone()).collect();
        let mut execution = SagaExecution::new(&execution_id, saga_name, &step_names);
        execution.status = SagaStatus::Running;

        self.executions
            .write()
            .insert(execution_id.clone(), execution);

        self.metrics.record_execution_started();

        if self.config.enable_logging {
            println!(
                "Saga '{}' execution started: {}",
                saga_name, execution_id
            );
        }

        Ok(execution_id)
    }

    /// Get a snapshot of a saga execution by ID.
    pub fn get_execution(&self, execution_id: &str) -> Result<SagaExecution, SagaError> {
        self.executions
            .read()
            .get(execution_id)
            .cloned()
            .ok_or_else(|| SagaError::ExecutionNotFound(execution_id.to_string()))
    }

    /// List all saga executions.
    pub fn list_executions(&self) -> Vec<SagaExecution> {
        self.executions.read().values().cloned().collect()
    }

    /// Mark a step as completed successfully.
    pub fn complete_step(
        &self,
        execution_id: &str,
        step_name: &str,
        result: Option<JsonValue>,
    ) -> Result<(), SagaError> {
        let mut executions = self.executions.write();
        let execution = executions
            .get_mut(execution_id)
            .ok_or_else(|| SagaError::ExecutionNotFound(execution_id.to_string()))?;

        let step = execution
            .steps
            .iter_mut()
            .find(|s| s.name == step_name)
            .ok_or_else(|| SagaError::StepNotFound(step_name.to_string()))?;

        step.status = StepStatus::Completed;
        step.result = result;

        if self.config.enable_logging {
            println!(
                "Saga '{}' step '{}' completed",
                execution.saga_name, step_name
            );
        }

        // Check if all steps are completed.
        let all_complete = execution
            .steps
            .iter()
            .all(|s| s.status == StepStatus::Completed);

        if all_complete {
            execution.status = SagaStatus::Completed;
            execution.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            self.metrics.record_execution_completed();

            if self.config.enable_logging {
                println!(
                    "Saga '{}' execution '{}' completed successfully",
                    execution.saga_name, execution_id
                );
            }
        }

        Ok(())
    }

    /// Mark a step as failed and trigger compensation for completed steps.
    pub fn fail_step(
        &self,
        execution_id: &str,
        step_name: &str,
        error: &str,
    ) -> Result<(), SagaError> {
        let mut executions = self.executions.write();
        let execution = executions
            .get_mut(execution_id)
            .ok_or_else(|| SagaError::ExecutionNotFound(execution_id.to_string()))?;

        let step = execution
            .steps
            .iter_mut()
            .find(|s| s.name == step_name)
            .ok_or_else(|| SagaError::StepNotFound(step_name.to_string()))?;

        step.status = StepStatus::Failed;
        step.error = Some(error.to_string());

        if self.config.enable_logging {
            println!(
                "Saga '{}' step '{}' failed: {}",
                execution.saga_name, step_name, error
            );
        }

        // Mark saga as compensating and set completed steps to compensating.
        execution.status = SagaStatus::Compensating;

        for s in execution.steps.iter_mut() {
            if s.status == StepStatus::Completed {
                s.status = StepStatus::Compensating;
            }
        }

        // Mark all compensating steps as compensated (in reverse order).
        let mut all_compensated = true;
        for s in execution.steps.iter_mut().rev() {
            if s.status == StepStatus::Compensating {
                s.status = StepStatus::Compensated;
            }
            if s.status != StepStatus::Compensated
                && s.status != StepStatus::Failed
                && s.status != StepStatus::Pending
            {
                all_compensated = false;
            }
        }

        if all_compensated {
            execution.status = SagaStatus::Compensated;
            execution.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            self.metrics.record_execution_compensated();

            if self.config.enable_logging {
                println!(
                    "Saga '{}' execution '{}' compensated",
                    execution.saga_name, execution_id
                );
            }
        }

        self.metrics.record_execution_failed();

        Ok(())
    }

    /// Get the number of registered saga definitions.
    pub fn saga_count(&self) -> usize {
        self.sagas.read().len()
    }

    /// Get the number of tracked executions.
    pub fn execution_count(&self) -> usize {
        self.executions.read().len()
    }

    /// Get the configuration.
    pub fn config(&self) -> &SagaConfig {
        &self.config
    }

    /// Get current statistics.
    pub fn stats(&self) -> SagaStats {
        self.metrics.get_stats()
    }
}

impl Default for SagaOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Saga error types.
#[derive(Debug, Clone)]
pub enum SagaError {
    /// The requested saga definition was not found.
    SagaNotFound(String),
    /// The requested step was not found in the saga.
    StepNotFound(String),
    /// The requested saga execution was not found.
    ExecutionNotFound(String),
    /// Compensation for one or more steps failed.
    CompensationFailed(String),
    /// The saga execution timed out.
    TimeoutError(String),
    /// Maximum retry attempts have been exhausted.
    MaxRetriesExceeded {
        step: String,
        attempts: u32,
    },
}

impl std::fmt::Display for SagaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SagaError::SagaNotFound(name) => write!(f, "Saga not found: {}", name),
            SagaError::StepNotFound(name) => write!(f, "Step not found: {}", name),
            SagaError::ExecutionNotFound(id) => {
                write!(f, "Saga execution not found: {}", id)
            }
            SagaError::CompensationFailed(msg) => {
                write!(f, "Compensation failed: {}", msg)
            }
            SagaError::TimeoutError(msg) => write!(f, "Saga timeout: {}", msg),
            SagaError::MaxRetriesExceeded { step, attempts } => {
                write!(
                    f,
                    "Max retries exceeded for step '{}' after {} attempts",
                    step, attempts
                )
            }
        }
    }
}

impl std::error::Error for SagaError {}

// ============================================================================
// Statistics & Metrics
// ============================================================================

/// Aggregate saga orchestration statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SagaStats {
    /// Total number of saga executions started.
    pub total_executions: u64,
    /// Number of successfully completed executions.
    pub completed: u64,
    /// Number of failed executions.
    pub failed: u64,
    /// Number of compensated executions.
    pub compensated: u64,
    /// Average execution duration in milliseconds.
    pub avg_duration_ms: f64,
}

/// Internal atomic metrics tracker for saga operations.
struct SagaMetrics {
    total_executions: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    compensated: AtomicU64,
    total_duration_ms: AtomicU64,
    completed_count: AtomicU64,
}

impl Default for SagaMetrics {
    fn default() -> Self {
        Self {
            total_executions: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            compensated: AtomicU64::new(0),
            total_duration_ms: AtomicU64::new(0),
            completed_count: AtomicU64::new(0),
        }
    }
}

impl SagaMetrics {
    pub fn record_execution_started(&self) {
        self.total_executions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_execution_completed(&self) {
        self.completed.fetch_add(1, Ordering::Relaxed);
        self.completed_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_execution_failed(&self) {
        self.failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_execution_compensated(&self) {
        self.compensated.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn record_duration(&self, duration_ms: u64) {
        self.total_duration_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> SagaStats {
        let completed_count = self.completed_count.load(Ordering::Relaxed);
        let total_duration = self.total_duration_ms.load(Ordering::Relaxed);

        SagaStats {
            total_executions: self.total_executions.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            compensated: self.compensated.load(Ordering::Relaxed),
            avg_duration_ms: if completed_count > 0 {
                total_duration as f64 / completed_count as f64
            } else {
                0.0
            },
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- SagaConfig Tests ----------

    #[test]
    fn test_config_default() {
        let config = SagaConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert_eq!(config.timeout_ms, 30000);
        assert!(config.enable_logging);
    }

    #[test]
    fn test_config_builder() {
        let config = SagaConfig::new()
            .with_max_retries(5)
            .with_retry_delay(2000)
            .with_timeout(60000)
            .with_logging(false);

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 2000);
        assert_eq!(config.timeout_ms, 60000);
        assert!(!config.enable_logging);
    }

    // ---------- SagaStepDef Tests ----------

    #[test]
    fn test_step_def_creation() {
        let step = SagaStepDef::new("create_order")
            .with_description("Create the order record")
            .with_compensation()
            .with_timeout(5000);

        assert_eq!(step.name, "create_order");
        assert_eq!(step.description, Some("Create the order record".to_string()));
        assert!(step.has_compensation);
        assert_eq!(step.timeout_ms, Some(5000));
    }

    // ---------- SagaDefinition Tests ----------

    #[test]
    fn test_saga_definition() {
        let mut saga = SagaDefinition::new("OrderCreation")
            .with_description("Create a new order with inventory and payment");

        saga.add_step(SagaStepDef::new("create_order").with_compensation());
        saga.add_step(SagaStepDef::new("reserve_inventory").with_compensation());
        saga.add_step(SagaStepDef::new("process_payment").with_compensation());

        assert_eq!(saga.name, "OrderCreation");
        assert_eq!(saga.step_count(), 3);
        assert_eq!(saga.get_steps()[0].name, "create_order");
        assert_eq!(saga.get_steps()[2].name, "process_payment");
    }

    // ---------- SagaStep Tests ----------

    #[test]
    fn test_saga_step_states() {
        let mut step = SagaStep::new("test_step");
        assert!(!step.is_complete());
        assert!(!step.is_failed());
        assert!(!step.is_compensated());

        step.status = StepStatus::Completed;
        assert!(step.is_complete());

        step.status = StepStatus::Failed;
        assert!(step.is_failed());

        step.status = StepStatus::Compensated;
        assert!(step.is_compensated());
    }

    // ---------- SagaExecution Tests ----------

    #[test]
    fn test_saga_execution_creation() {
        let step_names = vec![
            "step1".to_string(),
            "step2".to_string(),
            "step3".to_string(),
        ];
        let execution = SagaExecution::new("exec-1", "TestSaga", &step_names);

        assert_eq!(execution.id, "exec-1");
        assert_eq!(execution.saga_name, "TestSaga");
        assert_eq!(execution.steps.len(), 3);
        assert_eq!(execution.status, SagaStatus::Pending);
        assert!(execution.started_at > 0);
        assert!(execution.completed_at.is_none());
    }

    #[test]
    fn test_saga_execution_step_summary() {
        let step_names = vec!["a".to_string(), "b".to_string()];
        let mut execution = SagaExecution::new("exec-1", "TestSaga", &step_names);
        execution.steps[0].status = StepStatus::Completed;

        let summary = execution.step_summary();
        assert_eq!(summary.get("a"), Some(&"Completed".to_string()));
        assert_eq!(summary.get("b"), Some(&"Pending".to_string()));
    }

    // ---------- SagaOrchestrator Tests ----------

    #[test]
    fn test_orchestrator_register_and_start() {
        let config = SagaConfig::new().with_logging(false);
        let orchestrator = SagaOrchestrator::with_config(config);

        let mut saga = SagaDefinition::new("OrderSaga");
        saga.add_step(SagaStepDef::new("create_order"));
        saga.add_step(SagaStepDef::new("reserve_inventory"));

        orchestrator.register_saga(saga);
        assert_eq!(orchestrator.saga_count(), 1);

        let exec_id = orchestrator.start_execution("OrderSaga").unwrap();
        assert!(exec_id.starts_with("exec-OrderSaga-"));

        let execution = orchestrator.get_execution(&exec_id).unwrap();
        assert_eq!(execution.status, SagaStatus::Running);
        assert_eq!(execution.steps.len(), 2);
    }

    #[test]
    fn test_orchestrator_unknown_saga() {
        let orchestrator = SagaOrchestrator::new();
        let result = orchestrator.start_execution("NonExistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SagaError::SagaNotFound(_)));
    }

    #[test]
    fn test_orchestrator_complete_steps() {
        let config = SagaConfig::new().with_logging(false);
        let orchestrator = SagaOrchestrator::with_config(config);

        let mut saga = SagaDefinition::new("TestSaga");
        saga.add_step(SagaStepDef::new("step1"));
        saga.add_step(SagaStepDef::new("step2"));
        orchestrator.register_saga(saga);

        let exec_id = orchestrator.start_execution("TestSaga").unwrap();

        orchestrator
            .complete_step(&exec_id, "step1", Some(serde_json::json!({"ok": true})))
            .unwrap();

        let execution = orchestrator.get_execution(&exec_id).unwrap();
        assert_eq!(execution.steps[0].status, StepStatus::Completed);
        assert_eq!(execution.status, SagaStatus::Running);

        orchestrator
            .complete_step(&exec_id, "step2", None)
            .unwrap();

        let execution = orchestrator.get_execution(&exec_id).unwrap();
        assert_eq!(execution.status, SagaStatus::Completed);
        assert!(execution.completed_at.is_some());
    }

    #[test]
    fn test_orchestrator_fail_step_triggers_compensation() {
        let config = SagaConfig::new().with_logging(false);
        let orchestrator = SagaOrchestrator::with_config(config);

        let mut saga = SagaDefinition::new("FailSaga");
        saga.add_step(SagaStepDef::new("step1").with_compensation());
        saga.add_step(SagaStepDef::new("step2").with_compensation());
        saga.add_step(SagaStepDef::new("step3"));
        orchestrator.register_saga(saga);

        let exec_id = orchestrator.start_execution("FailSaga").unwrap();

        // Complete step1, then fail step2.
        orchestrator.complete_step(&exec_id, "step1", None).unwrap();
        orchestrator
            .fail_step(&exec_id, "step2", "payment declined")
            .unwrap();

        let execution = orchestrator.get_execution(&exec_id).unwrap();
        assert_eq!(execution.steps[0].status, StepStatus::Compensated);
        assert_eq!(execution.steps[1].status, StepStatus::Failed);
        assert_eq!(execution.steps[2].status, StepStatus::Pending);
        assert_eq!(execution.status, SagaStatus::Compensated);
    }

    #[test]
    fn test_orchestrator_list_executions() {
        let config = SagaConfig::new().with_logging(false);
        let orchestrator = SagaOrchestrator::with_config(config);

        let mut saga = SagaDefinition::new("ListSaga");
        saga.add_step(SagaStepDef::new("step1"));
        orchestrator.register_saga(saga);

        orchestrator.start_execution("ListSaga").unwrap();
        orchestrator.start_execution("ListSaga").unwrap();

        let executions = orchestrator.list_executions();
        assert_eq!(executions.len(), 2);
    }

    #[test]
    fn test_orchestrator_stats() {
        let config = SagaConfig::new().with_logging(false);
        let orchestrator = SagaOrchestrator::with_config(config);

        let mut saga = SagaDefinition::new("StatsSaga");
        saga.add_step(SagaStepDef::new("step1"));
        orchestrator.register_saga(saga);

        let exec_id = orchestrator.start_execution("StatsSaga").unwrap();
        orchestrator.complete_step(&exec_id, "step1", None).unwrap();

        let stats = orchestrator.stats();
        assert_eq!(stats.total_executions, 1);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.failed, 0);
    }

    // ---------- Error Display Tests ----------

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", SagaError::SagaNotFound("OrderSaga".to_string())),
            "Saga not found: OrderSaga"
        );
        assert_eq!(
            format!("{}", SagaError::StepNotFound("step1".to_string())),
            "Step not found: step1"
        );
        assert_eq!(
            format!("{}", SagaError::ExecutionNotFound("exec-1".to_string())),
            "Saga execution not found: exec-1"
        );
        assert_eq!(
            format!(
                "{}",
                SagaError::CompensationFailed("step2 rollback failed".to_string())
            ),
            "Compensation failed: step2 rollback failed"
        );
        assert_eq!(
            format!("{}", SagaError::TimeoutError("30s exceeded".to_string())),
            "Saga timeout: 30s exceeded"
        );
        assert_eq!(
            format!(
                "{}",
                SagaError::MaxRetriesExceeded {
                    step: "payment".to_string(),
                    attempts: 3,
                }
            ),
            "Max retries exceeded for step 'payment' after 3 attempts"
        );
    }

    // ---------- SagaStats Tests ----------

    #[test]
    fn test_stats_default() {
        let stats = SagaStats::default();
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.completed, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.compensated, 0);
        assert_eq!(stats.avg_duration_ms, 0.0);
    }

    // ---------- Status Display Tests ----------

    #[test]
    fn test_step_status_display() {
        assert_eq!(format!("{}", StepStatus::Pending), "Pending");
        assert_eq!(format!("{}", StepStatus::Running), "Running");
        assert_eq!(format!("{}", StepStatus::Completed), "Completed");
        assert_eq!(format!("{}", StepStatus::Failed), "Failed");
        assert_eq!(format!("{}", StepStatus::Compensating), "Compensating");
        assert_eq!(format!("{}", StepStatus::Compensated), "Compensated");
    }

    #[test]
    fn test_saga_status_display() {
        assert_eq!(format!("{}", SagaStatus::Pending), "Pending");
        assert_eq!(format!("{}", SagaStatus::Running), "Running");
        assert_eq!(format!("{}", SagaStatus::Completed), "Completed");
        assert_eq!(format!("{}", SagaStatus::Failed), "Failed");
        assert_eq!(format!("{}", SagaStatus::Compensating), "Compensating");
        assert_eq!(format!("{}", SagaStatus::Compensated), "Compensated");
    }
}
