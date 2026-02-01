use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use dashmap::DashMap;

use super::{Middleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

// ============================================================================
// Circuit Breaker State
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Closed,
    Open,
    HalfOpen,
}

struct CircuitBreakerState {
    state: State,
    failure_count: u32,
    last_failure: Option<Instant>,
    opened_at: Option<Instant>,
    half_open_successes: u32,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self {
            state: State::Closed,
            failure_count: 0,
            last_failure: None,
            opened_at: None,
            half_open_successes: 0,
        }
    }
}

// ============================================================================
// Configuration
// ============================================================================

#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit
    pub failure_threshold: u32,
    /// Time to wait before attempting to close the circuit (Half-Open)
    pub reset_timeout: Duration,
    /// Number of successes required in Half-Open to Close
    pub half_open_target: u32,
    /// Define what HTTP codes constitute a failure (e.g., 500, 503)
    pub failure_codes: Vec<u16>,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            half_open_target: 3,
            failure_codes: vec![500, 502, 503, 504],
        }
    }
}

// ============================================================================
// Middleware
// ============================================================================

pub struct CircuitBreakerMiddleware {
    config: CircuitBreakerConfig,
    /// State keyed by route/path
    states: Arc<DashMap<String, RwLock<CircuitBreakerState>>>,
}

impl CircuitBreakerMiddleware {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            states: Arc::new(DashMap::new()),
        }
    }

    /// Extract key for circuit breaker (e.g. route path)
    fn get_key(&self, request: &Request) -> String {
        // Simple strategy: Use method + path
        // Grouping logic could be added here
        format!("{} {}", request.method, request.path)
    }
}

impl Middleware for CircuitBreakerMiddleware {
    fn before(&self, request: &mut Request) -> MiddlewareResult {
        let key = self.get_key(request);
        
        // Check if key exists, if not continue (state created in after or lazily?)
        // Better to check state.
        
        // This relies on states being present. If new, default is Closed.
        // DashMap entry API avoids lock text contention on misses if we don't insert.
        // But if we want to BLOCK, we need to read state.
        
        if let Some(state_entry) = self.states.get(&key) {
            let mut state = state_entry.write();
            
            match state.state {
                State::Open => {
                    // Check if reset timeout passed
                    if let Some(opened_at) = state.opened_at {
                        if opened_at.elapsed() >= self.config.reset_timeout {
                            // Transition to Half-Open
                            state.state = State::HalfOpen;
                            state.half_open_successes = 0;
                            return Ok(MiddlewareAction::Continue); // Allow probe
                        }
                    }
                    
                    // Circuit is Open -> Fail Fast
                    let response = Response::error(503, "Service Unavailable (Circuit Open)");
                    return Ok(MiddlewareAction::Stop(response));
                },
                State::HalfOpen => {
                    // In Half-Open, allow requests. (Maybe limit concurrency?)
                    // For simplicity, we allow all.
                    // Failures will reopen it. Successes will close it.
                    return Ok(MiddlewareAction::Continue); 
                },
                State::Closed => {
                    return Ok(MiddlewareAction::Continue);
                }
            }
        }

        Ok(MiddlewareAction::Continue)
    }

    fn after(&self, request: &Request, response: &mut Response) -> MiddlewareResult {
        let key = self.get_key(request);
        let is_failure = self.config.failure_codes.contains(&response.status);

        // We insert on first access here
        let state_entry = self.states.entry(key).or_insert_with(|| RwLock::new(CircuitBreakerState::default()));
        let mut state = state_entry.write();

        match state.state {
            State::Closed => {
                if is_failure {
                    state.failure_count += 1;
                    state.last_failure = Some(Instant::now());
                    
                    if state.failure_count >= self.config.failure_threshold {
                        state.state = State::Open;
                        state.opened_at = Some(Instant::now());
                        // Log transition
                        println!("Circuit Breaker OPEN for {}", state_entry.key());
                    }
                } else {
                    // Success, maybe decay failure count?
                    // Simple logic: reset on success? 
                    // Usually we reset only after a Time Window, but here we keep count.
                    // Let's reset count on success if we want STRICT consecutive failures?
                    // Or keep count accumulating until timeout?
                    // Config says "Number of failures". Usually implies consecutive or within window.
                    // For simplicity: Consecutive failures logic.
                    state.failure_count = 0;
                }
            }
            State::Open => {
                // If we are here, it means we allowed a request while Open?
                // Only possible if before() transitioned it due to race or timeout.
                // But before() handles timeout transition to HalfOpen.
                // It's possible status changed between before/after.
            }
            State::HalfOpen => {
                if is_failure {
                    // Probe failed -> Re-Open
                    state.state = State::Open;
                    state.opened_at = Some(Instant::now());
                    println!("Circuit Breaker RE-OPEN for {}", state_entry.key());
                } else {
                    // Probe succeeded
                    state.half_open_successes += 1;
                    if state.half_open_successes >= self.config.half_open_target {
                        state.state = State::Closed;
                        state.failure_count = 0;
                        state.opened_at = None;
                        println!("Circuit Breaker CLOSED for {}", state_entry.key());
                    }
                }
            }
        }

        Ok(MiddlewareAction::Continue)
    }

    fn name(&self) -> &str {
        "circuit_breaker"
    }
}
