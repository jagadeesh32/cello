//! Cluster mode for Cello server.
//!
//! Provides:
//! - Pre-fork multi-process model
//! - Worker management
//! - CPU affinity (optional)
//! - Signal handling for cluster

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;


// ============================================================================
// Cluster Configuration
// ============================================================================

/// Cluster mode configuration.
#[derive(Clone, Debug)]
pub struct ClusterConfig {
    /// Number of worker processes
    pub workers: usize,
    /// Enable CPU affinity (pin workers to cores)
    pub cpu_affinity: bool,
    /// Worker restart delay
    pub restart_delay: Duration,
    /// Maximum restart attempts per worker
    pub max_restarts: u32,
    /// Restart window (reset restart count after this duration)
    pub restart_window: Duration,
    /// Enable graceful worker shutdown
    pub graceful_shutdown: bool,
    /// Worker shutdown timeout
    pub shutdown_timeout: Duration,
}

impl ClusterConfig {
    /// Create new cluster config.
    pub fn new(workers: usize) -> Self {
        Self {
            workers,
            cpu_affinity: false,
            restart_delay: Duration::from_secs(1),
            max_restarts: 5,
            restart_window: Duration::from_secs(60),
            graceful_shutdown: true,
            shutdown_timeout: Duration::from_secs(30),
        }
    }

    /// Auto-detect worker count based on CPU cores.
    pub fn auto() -> Self {
        let workers = num_cpus::get();
        Self::new(workers)
    }

    /// Enable CPU affinity.
    pub fn with_cpu_affinity(mut self) -> Self {
        self.cpu_affinity = true;
        self
    }

    /// Set restart delay.
    pub fn restart_delay(mut self, delay: Duration) -> Self {
        self.restart_delay = delay;
        self
    }

    /// Set max restarts.
    pub fn max_restarts(mut self, max: u32) -> Self {
        self.max_restarts = max;
        self
    }

    /// Set shutdown timeout.
    pub fn shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self::auto()
    }
}

// ============================================================================
// Worker State
// ============================================================================

/// State of a worker process.
#[derive(Clone, Debug)]
pub enum WorkerState {
    /// Worker is starting
    Starting,
    /// Worker is running
    Running,
    /// Worker is stopping
    Stopping,
    /// Worker has stopped
    Stopped,
    /// Worker has crashed
    Crashed,
}

/// Information about a worker process.
#[derive(Clone, Debug)]
pub struct WorkerInfo {
    /// Worker ID
    pub id: u32,
    /// Process ID (if running)
    pub pid: Option<u32>,
    /// Current state
    pub state: WorkerState,
    /// Number of restarts
    pub restarts: u32,
    /// Last restart time
    pub last_restart: Option<std::time::Instant>,
    /// Assigned CPU core (if CPU affinity enabled)
    pub cpu_core: Option<usize>,
}

impl WorkerInfo {
    /// Create new worker info.
    pub fn new(id: u32) -> Self {
        Self {
            id,
            pid: None,
            state: WorkerState::Starting,
            restarts: 0,
            last_restart: None,
            cpu_core: None,
        }
    }

    /// Check if worker can be restarted.
    pub fn can_restart(&self, config: &ClusterConfig) -> bool {
        if self.restarts >= config.max_restarts {
            // Check if restart window has passed
            if let Some(last) = self.last_restart {
                if last.elapsed() > config.restart_window {
                    return true; // Window passed, reset restart count
                }
            }
            return false;
        }
        true
    }
}

// ============================================================================
// Cluster Manager
// ============================================================================

/// Manages cluster of worker processes.
pub struct ClusterManager {
    config: ClusterConfig,
    workers: RwLock<HashMap<u32, WorkerInfo>>,
    next_worker_id: AtomicU32,
    running: AtomicBool,
    is_master: bool,
}

impl ClusterManager {
    /// Create new cluster manager.
    pub fn new(config: ClusterConfig) -> Self {
        Self {
            config,
            workers: RwLock::new(HashMap::new()),
            next_worker_id: AtomicU32::new(0),
            running: AtomicBool::new(false),
            is_master: true,
        }
    }

    /// Check if this is the master process.
    pub fn is_master(&self) -> bool {
        self.is_master
    }

    /// Check if cluster is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get number of configured workers.
    pub fn worker_count(&self) -> usize {
        self.config.workers
    }

    /// Get all worker info.
    pub fn workers(&self) -> Vec<WorkerInfo> {
        self.workers.read().values().cloned().collect()
    }

    /// Get worker by ID.
    pub fn get_worker(&self, id: u32) -> Option<WorkerInfo> {
        self.workers.read().get(&id).cloned()
    }

    /// Get worker PIDs.
    pub fn worker_pids(&self) -> Vec<u32> {
        self.workers
            .read()
            .values()
            .filter_map(|w| w.pid)
            .collect()
    }

    /// Start the cluster.
    pub fn start(&self) -> Result<(), ClusterError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(ClusterError::AlreadyRunning);
        }

        println!("Starting cluster with {} workers", self.config.workers);

        for i in 0..self.config.workers {
            let worker_id = self.next_worker_id.fetch_add(1, Ordering::SeqCst);
            let mut info = WorkerInfo::new(worker_id);

            if self.config.cpu_affinity {
                info.cpu_core = Some(i % num_cpus::get());
            }

            self.workers.write().insert(worker_id, info);

            // In a real implementation, we would fork here
            // For now, we just track the worker info
        }

        Ok(())
    }

    /// Stop the cluster.
    pub fn stop(&self) -> Result<(), ClusterError> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Err(ClusterError::NotRunning);
        }

        println!("Stopping cluster...");

        // Signal all workers to stop
        for (_, worker) in self.workers.write().iter_mut() {
            worker.state = WorkerState::Stopping;
        }

        Ok(())
    }

    /// Restart a worker.
    pub fn restart_worker(&self, worker_id: u32) -> Result<(), ClusterError> {
        let mut workers = self.workers.write();
        let worker = workers
            .get_mut(&worker_id)
            .ok_or(ClusterError::WorkerNotFound(worker_id))?;

        if !worker.can_restart(&self.config) {
            return Err(ClusterError::MaxRestartsExceeded(worker_id));
        }

        worker.restarts += 1;
        worker.last_restart = Some(std::time::Instant::now());
        worker.state = WorkerState::Starting;

        println!("Restarting worker {} (restart #{})", worker_id, worker.restarts);

        Ok(())
    }

    /// Update worker state.
    pub fn set_worker_state(&self, worker_id: u32, state: WorkerState) {
        if let Some(worker) = self.workers.write().get_mut(&worker_id) {
            worker.state = state;
        }
    }

    /// Update worker PID.
    pub fn set_worker_pid(&self, worker_id: u32, pid: u32) {
        if let Some(worker) = self.workers.write().get_mut(&worker_id) {
            worker.pid = Some(pid);
        }
    }

    /// Get cluster statistics.
    pub fn stats(&self) -> ClusterStats {
        let workers = self.workers.read();

        let running_count = workers
            .values()
            .filter(|w| matches!(w.state, WorkerState::Running))
            .count();

        let crashed_count = workers
            .values()
            .filter(|w| matches!(w.state, WorkerState::Crashed))
            .count();

        let total_restarts: u32 = workers.values().map(|w| w.restarts).sum();

        ClusterStats {
            total_workers: self.config.workers,
            running_workers: running_count,
            crashed_workers: crashed_count,
            total_restarts,
            is_running: self.is_running(),
        }
    }
}

/// Cluster statistics.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ClusterStats {
    pub total_workers: usize,
    pub running_workers: usize,
    pub crashed_workers: usize,
    pub total_restarts: u32,
    pub is_running: bool,
}

/// Cluster errors.
#[derive(Debug, Clone)]
pub enum ClusterError {
    /// Cluster is already running
    AlreadyRunning,
    /// Cluster is not running
    NotRunning,
    /// Worker not found
    WorkerNotFound(u32),
    /// Max restarts exceeded
    MaxRestartsExceeded(u32),
    /// Fork failed
    ForkFailed(String),
    /// Signal failed
    SignalFailed(String),
}

impl std::fmt::Display for ClusterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClusterError::AlreadyRunning => write!(f, "Cluster is already running"),
            ClusterError::NotRunning => write!(f, "Cluster is not running"),
            ClusterError::WorkerNotFound(id) => write!(f, "Worker {} not found", id),
            ClusterError::MaxRestartsExceeded(id) => {
                write!(f, "Maximum restarts exceeded for worker {}", id)
            }
            ClusterError::ForkFailed(e) => write!(f, "Fork failed: {}", e),
            ClusterError::SignalFailed(e) => write!(f, "Signal failed: {}", e),
        }
    }
}

impl std::error::Error for ClusterError {}

// ============================================================================
// Signal Handling
// ============================================================================

/// Signal handler for cluster management.
#[cfg(unix)]
pub struct ClusterSignals {
    _manager: Arc<ClusterManager>,
}

#[cfg(unix)]
impl ClusterSignals {
    /// Create new signal handler.
    pub fn new(manager: Arc<ClusterManager>) -> Self {
        Self { _manager: manager }
    }

    /// Install signal handlers.
    pub fn install(&self) {
        // In a real implementation, we would set up signal handlers
        // for SIGTERM, SIGINT, SIGHUP, SIGUSR1, SIGUSR2
    }
}

// ============================================================================
// IPC (Inter-Process Communication)
// ============================================================================

/// Message types for cluster IPC.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ClusterMessage {
    /// Worker started
    WorkerStarted { worker_id: u32, pid: u32 },
    /// Worker ready
    WorkerReady { worker_id: u32 },
    /// Worker stopping
    WorkerStopping { worker_id: u32 },
    /// Worker stopped
    WorkerStopped { worker_id: u32, exit_code: i32 },
    /// Worker crashed
    WorkerCrashed { worker_id: u32, error: String },
    /// Shutdown request
    Shutdown,
    /// Reload request
    Reload,
    /// Health check
    HealthCheck,
    /// Health response
    HealthResponse { worker_id: u32, healthy: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_config() {
        let config = ClusterConfig::new(4)
            .with_cpu_affinity()
            .max_restarts(10)
            .shutdown_timeout(Duration::from_secs(60));

        assert_eq!(config.workers, 4);
        assert!(config.cpu_affinity);
        assert_eq!(config.max_restarts, 10);
    }

    #[test]
    fn test_cluster_config_auto() {
        let config = ClusterConfig::auto();
        assert!(config.workers > 0);
    }

    #[test]
    fn test_worker_info() {
        let mut worker = WorkerInfo::new(1);
        assert!(matches!(worker.state, WorkerState::Starting));

        worker.state = WorkerState::Running;
        worker.pid = Some(12345);

        assert!(matches!(worker.state, WorkerState::Running));
        assert_eq!(worker.pid, Some(12345));
    }

    #[test]
    fn test_worker_restart_limit() {
        let config = ClusterConfig::new(1).max_restarts(3);
        let mut worker = WorkerInfo::new(1);

        // Should be able to restart initially
        assert!(worker.can_restart(&config));

        // Exhaust restart count
        worker.restarts = 3;
        worker.last_restart = Some(std::time::Instant::now());

        // Should not be able to restart
        assert!(!worker.can_restart(&config));
    }

    #[test]
    fn test_cluster_manager() {
        let config = ClusterConfig::new(2);
        let manager = ClusterManager::new(config);

        assert!(manager.is_master());
        assert!(!manager.is_running());
        assert_eq!(manager.worker_count(), 2);

        // Start cluster
        manager.start().unwrap();
        assert!(manager.is_running());

        // Check workers
        let workers = manager.workers();
        assert_eq!(workers.len(), 2);

        // Stop cluster
        manager.stop().unwrap();
        assert!(!manager.is_running());
    }

    #[test]
    fn test_cluster_stats() {
        let config = ClusterConfig::new(4);
        let manager = ClusterManager::new(config);
        manager.start().unwrap();

        let stats = manager.stats();
        assert_eq!(stats.total_workers, 4);
        assert!(stats.is_running);
    }
}
