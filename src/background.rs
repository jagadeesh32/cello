//! Background Tasks for Cello Framework
//!
//! This module provides FastAPI-style background task execution.
//! Tasks are executed after the response is sent to the client.

use pyo3::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::sync::mpsc;

// ============================================================================
// Background Task Types
// ============================================================================

/// A background task that can be executed after response.
pub trait BackgroundTask: Send + Sync + 'static {
    /// Execute the task.
    fn execute(&self);
    
    /// Get task name for logging.
    fn name(&self) -> &str {
        "anonymous_task"
    }
}

/// A Python background task.
pub struct PythonBackgroundTask {
    handler: PyObject,
    args: Vec<PyObject>,
    name: String,
}

impl PythonBackgroundTask {
    pub fn new(handler: PyObject, args: Vec<PyObject>) -> Self {
        Self {
            handler,
            args,
            name: "python_task".to_string(),
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }
}

impl BackgroundTask for PythonBackgroundTask {
    fn execute(&self) {
        Python::with_gil(|py| {
            let args_tuple = pyo3::types::PyTuple::new(py, &self.args);
            if let Err(e) = self.handler.call1(py, args_tuple) {
                eprintln!("Background task '{}' failed: {}", self.name, e);
            }
        });
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A closure-based background task.
pub struct FnBackgroundTask<F>
where
    F: Fn() + Send + Sync + 'static,
{
    func: F,
    name: String,
}

impl<F> FnBackgroundTask<F>
where
    F: Fn() + Send + Sync + 'static,
{
    pub fn new(name: &str, func: F) -> Self {
        Self {
            func,
            name: name.to_string(),
        }
    }
}

impl<F> BackgroundTask for FnBackgroundTask<F>
where
    F: Fn() + Send + Sync + 'static,
{
    fn execute(&self) {
        (self.func)();
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Background Task Queue
// ============================================================================

/// Queue for background tasks.
#[derive(Clone)]
pub struct BackgroundTasks {
    tasks: Arc<Mutex<VecDeque<Box<dyn BackgroundTask>>>>,
}

impl Default for BackgroundTasks {
    fn default() -> Self {
        Self::new()
    }
}

impl BackgroundTasks {
    /// Create a new background task queue.
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Add a task to the queue.
    pub fn add<T: BackgroundTask>(&self, task: T) {
        self.tasks.lock().push_back(Box::new(task));
    }

    /// Add a Python function as a background task.
    pub fn add_python(&self, handler: PyObject, args: Vec<PyObject>) {
        self.add(PythonBackgroundTask::new(handler, args));
    }

    /// Add a closure as a background task.
    pub fn add_fn<F>(&self, name: &str, func: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.add(FnBackgroundTask::new(name, func));
    }

    /// Execute all queued tasks.
    pub fn execute_all(&self) {
        let tasks: Vec<_> = {
            let mut queue = self.tasks.lock();
            queue.drain(..).collect()
        };

        for task in tasks {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task.execute();
            })) {
                eprintln!("Background task '{}' panicked: {:?}", task.name(), e);
            }
        }
    }

    /// Check if queue is empty.
    pub fn is_empty(&self) -> bool {
        self.tasks.lock().is_empty()
    }

    /// Get number of pending tasks.
    pub fn len(&self) -> usize {
        self.tasks.lock().len()
    }
}

// ============================================================================
// Async Background Task Runner
// ============================================================================

/// Message for background task channel.
pub enum TaskMessage {
    Execute(Box<dyn BackgroundTask>),
    Shutdown,
}

/// Async background task runner using channels.
pub struct BackgroundTaskRunner {
    sender: mpsc::UnboundedSender<TaskMessage>,
}

impl BackgroundTaskRunner {
    /// Create a new background task runner.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<TaskMessage>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    /// Schedule a task for execution.
    pub fn schedule<T: BackgroundTask>(&self, task: T) -> Result<(), String> {
        self.sender
            .send(TaskMessage::Execute(Box::new(task)))
            .map_err(|e| format!("Failed to schedule task: {}", e))
    }

    /// Shutdown the runner.
    pub fn shutdown(&self) -> Result<(), String> {
        self.sender
            .send(TaskMessage::Shutdown)
            .map_err(|e| format!("Failed to shutdown: {}", e))
    }
}

impl Default for BackgroundTaskRunner {
    fn default() -> Self {
        Self::new().0
    }
}

/// Run the background task executor loop.
pub async fn run_task_executor(mut receiver: mpsc::UnboundedReceiver<TaskMessage>) {
    while let Some(msg) = receiver.recv().await {
        match msg {
            TaskMessage::Execute(task) => {
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        task.execute();
                    })) {
                        eprintln!("Background task panicked: {:?}", e);
                    }
                });
            }
            TaskMessage::Shutdown => {
                break;
            }
        }
    }
}

// ============================================================================
// Python Integration
// ============================================================================

/// Python-exposed background tasks manager.
#[pyclass]
#[derive(Clone)]
pub struct PyBackgroundTasks {
    inner: BackgroundTasks,
}

#[pymethods]
impl PyBackgroundTasks {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: BackgroundTasks::new(),
        }
    }

    /// Add a task function with arguments.
    pub fn add_task(&self, func: PyObject, args: Vec<PyObject>) {
        self.inner.add_python(func, args);
    }

    /// Execute all pending tasks.
    pub fn run_all(&self) {
        self.inner.execute_all();
    }

    /// Get number of pending tasks.
    pub fn pending_count(&self) -> usize {
        self.inner.len()
    }
}

impl Default for PyBackgroundTasks {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_background_tasks() {
        let counter = Arc::new(AtomicUsize::new(0));
        let tasks = BackgroundTasks::new();

        let counter_clone = counter.clone();
        tasks.add_fn("increment", move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(tasks.len(), 1);
        tasks.execute_all();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_multiple_tasks() {
        let counter = Arc::new(AtomicUsize::new(0));
        let tasks = BackgroundTasks::new();

        for _ in 0..5 {
            let counter_clone = counter.clone();
            tasks.add_fn("increment", move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
        }

        assert_eq!(tasks.len(), 5);
        tasks.execute_all();
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_panic_handling() {
        let tasks = BackgroundTasks::new();
        
        tasks.add_fn("panicking", || {
            panic!("Test panic");
        });

        // Should not propagate panic
        tasks.execute_all();
    }
}
