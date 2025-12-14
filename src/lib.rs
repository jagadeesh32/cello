//! Cello - Ultra-fast Rust-powered Python web framework
//!
//! This module provides the core HTTP server and routing functionality
//! that powers the Cello Python framework.
//!
//! ## Features
//! - SIMD-accelerated JSON parsing
//! - Arena allocators for zero-copy operations
//! - Middleware system with hooks
//! - WebSocket and SSE support
//! - Blueprint-based routing

// Silence PyO3 macro warning from older version
#![allow(non_local_definitions)]

pub mod arena;
pub mod blueprint;
pub mod handler;
pub mod json;
pub mod middleware;
pub mod multipart;
pub mod request;
pub mod response;
pub mod router;
pub mod server;
pub mod sse;
pub mod websocket;

use pyo3::prelude::*;

use blueprint::Blueprint;
use handler::HandlerRegistry;
use middleware::{MiddlewareChain, CorsMiddleware, LoggingMiddleware, CompressionMiddleware};
use router::Router;
use server::Server;
use sse::{SseEvent, SseStream};
use websocket::{WebSocket, WebSocketMessage, WebSocketRegistry};

/// The main Cello application class exposed to Python.
///
/// This class manages routes, middleware, and starts the HTTP server.
#[pyclass]
pub struct Cello {
    router: Router,
    handlers: HandlerRegistry,
    middleware: MiddlewareChain,
    websocket_handlers: WebSocketRegistry,
}

#[pymethods]
impl Cello {
    /// Create a new Cello application instance.
    #[new]
    pub fn new() -> Self {
        Cello {
            router: Router::new(),
            handlers: HandlerRegistry::new(),
            middleware: MiddlewareChain::new(),
            websocket_handlers: WebSocketRegistry::new(),
        }
    }

    /// Register a GET route.
    pub fn get(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("GET", path, handler)
    }

    /// Register a POST route.
    pub fn post(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("POST", path, handler)
    }

    /// Register a PUT route.
    pub fn put(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("PUT", path, handler)
    }

    /// Register a DELETE route.
    pub fn delete(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("DELETE", path, handler)
    }

    /// Register a PATCH route.
    pub fn patch(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("PATCH", path, handler)
    }

    /// Register an OPTIONS route.
    pub fn options(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("OPTIONS", path, handler)
    }

    /// Register a HEAD route.
    pub fn head(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("HEAD", path, handler)
    }

    /// Register a WebSocket route.
    pub fn websocket(&mut self, path: &str, handler: PyObject) -> PyResult<()> {
        self.websocket_handlers.register(path, handler);
        Ok(())
    }

    /// Register a blueprint.
    pub fn register_blueprint(&mut self, blueprint: &Blueprint) -> PyResult<()> {
        let routes = blueprint.get_all_routes();
        for (method, path, handler) in routes {
            self.add_route(&method, &path, handler)?;
        }
        Ok(())
    }

    /// Enable CORS middleware.
    #[pyo3(signature = (origins=None))]
    pub fn enable_cors(&mut self, origins: Option<Vec<String>>) {
        let mut cors = CorsMiddleware::new();
        if let Some(origins) = origins {
            cors.allow_origins = origins;
        }
        self.middleware.add(cors);
    }

    /// Enable logging middleware.
    pub fn enable_logging(&mut self) {
        self.middleware.add(LoggingMiddleware::new());
    }

    /// Enable compression middleware.
    #[pyo3(signature = (min_size=None))]
    pub fn enable_compression(&mut self, min_size: Option<usize>) {
        let mut compression = CompressionMiddleware::new();
        if let Some(size) = min_size {
            compression.min_size = size;
        }
        self.middleware.add(compression);
    }

    /// Start the HTTP server.
    /// Start the HTTP server.
    #[pyo3(signature = (host=None, port=None, workers=None))]
    pub fn run(&self, py: Python<'_>, host: Option<&str>, port: Option<u16>, workers: Option<usize>) -> PyResult<()> {
        let host = host.unwrap_or("127.0.0.1");
        let port = port.unwrap_or(8000);

        println!("🐍 Cello v2 server starting at http://{}:{}", host, port);
        if let Some(w) = workers {
            println!("   Workers: {}", w);
        }

        // Release the GIL and run the server
        py.allow_threads(|| {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            builder.enable_all();

            if let Some(w) = workers {
                builder.worker_threads(w);
            }

            let rt = builder.build()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            rt.block_on(async {
                let server = Server::new(
                    host.to_string(),
                    port,
                    self.router.clone(),
                    self.handlers.clone(),
                    self.middleware.clone(),
                    self.websocket_handlers.clone(),
                );
                server.run().await
            })
        })
    }

    /// Internal route registration.
    fn add_route(&mut self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let handler_id = self.handlers.register(handler);
        self.router
            .add_route(method, path, handler_id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }
}

impl Default for Cello {
    fn default() -> Self {
        Self::new()
    }
}

/// Python module definition.
#[pymodule]
fn _cello(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    // Core classes
    m.add_class::<Cello>()?;
    m.add_class::<request::Request>()?;
    m.add_class::<response::Response>()?;
    
    // Blueprint
    m.add_class::<Blueprint>()?;
    
    // WebSocket
    m.add_class::<WebSocket>()?;
    m.add_class::<WebSocketMessage>()?;
    
    // SSE
    m.add_class::<SseEvent>()?;
    m.add_class::<SseStream>()?;
    
    // Multipart
    m.add_class::<multipart::FormData>()?;
    m.add_class::<multipart::UploadedFile>()?;
    
    Ok(())
}
