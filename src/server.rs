//! HTTP Server implementation using Tokio and Hyper.
//!
//! This is the core async HTTP server that handles all I/O in Rust.

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{body::Incoming, Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

use crate::handler::HandlerRegistry;
use crate::middleware::{MiddlewareChain, MiddlewareAction};
use crate::request::Request;
use crate::response::Response;
use crate::router::{RouteMatch, Router};
use crate::websocket::WebSocketRegistry;

/// The main HTTP server.
pub struct Server {
    host: String,
    port: u16,
    router: Router,
    handlers: HandlerRegistry,
    middleware: MiddlewareChain,
    websocket_handlers: WebSocketRegistry,
}

impl Server {
    /// Create a new server instance.
    pub fn new(
        host: String,
        port: u16,
        router: Router,
        handlers: HandlerRegistry,
        middleware: MiddlewareChain,
        websocket_handlers: WebSocketRegistry,
    ) -> Self {
        Server {
            host,
            port,
            router,
            handlers,
            middleware,
            websocket_handlers,
        }
    }

    /// Run the server (blocking).
    pub async fn run(self) -> PyResult<()> {
        let addr: SocketAddr = format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid address: {}", e)))?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to bind: {}", e)))?;

        println!("🚀 Cello v2 server running at http://{}", addr);
        println!("   Middleware: {} registered", self.middleware.len());
        println!("   Press CTRL+C to stop the server");

        let router = Arc::new(self.router);
        let handlers = Arc::new(self.handlers);
        let middleware = Arc::new(self.middleware);
        let websocket_handlers = Arc::new(self.websocket_handlers);

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("\n🛑 Shutting down gracefully...");
                    // Give active tasks a moment to finish if needed (not implemented here)
                    break;
                }
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let io = TokioIo::new(stream);
                            let router = router.clone();
                            let handlers = handlers.clone();
                            let middleware = middleware.clone();
                            let _websocket_handlers = websocket_handlers.clone();

                            tokio::task::spawn(async move {
                                let service = service_fn(move |req| {
                                    let router = router.clone();
                                    let handlers = handlers.clone();
                                    let middleware = middleware.clone();
                                    async move { handle_request(req, router, handlers, middleware).await }
                                });

                                if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                                    eprintln!("Error serving connection: {:?}", err);
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("Accept error: {}", e);
                            // Decide if we want to break or continue on accept error
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}

/// Handle an incoming HTTP request.
async fn handle_request(
    req: HyperRequest<Incoming>,
    router: Arc<Router>,
    handlers: Arc<HandlerRegistry>,
    middleware: Arc<MiddlewareChain>,
) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query_string = req.uri().query().unwrap_or("");

    // Parse query parameters with + to space conversion
    let query: HashMap<String, String> = query_string
        .split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some(key), Some(value)) => {
                    let value_with_spaces = value.replace('+', " ");
                    Some((
                        urlencoding::decode(key).unwrap_or_default().to_string(),
                        urlencoding::decode(&value_with_spaces).unwrap_or_default().to_string(),
                    ))
                }
                (Some(key), None) => Some((
                    urlencoding::decode(key).unwrap_or_default().to_string(),
                    String::new(),
                )),
                _ => None,
            }
        })
        .collect();

    // Extract headers
    let headers: HashMap<String, String> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string().to_lowercase(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // Read body
    let body_bytes = match req.collect().await {
        Ok(collected) => collected.to_bytes().to_vec(),
        Err(_) => Vec::new(),
    };

    // Match route
    let route_match = router.match_route(&method, &path);

    match route_match {
        Some(RouteMatch { handler_id, params }) => {
            // Create request object
            let mut request = Request::from_http(
                method.clone(),
                path.clone(),
                params,
                query,
                headers,
                body_bytes,
            );

            // Execute before middleware
            match middleware.execute_before(&mut request) {
                Ok(MiddlewareAction::Continue) => {}
                Ok(MiddlewareAction::Stop(response)) => {
                    return build_hyper_response(&response);
                }
                Err(e) => {
                    let response = Response::error(e.status, &e.message);
                    return build_hyper_response(&response);
                }
            }

            // Invoke handler (this will acquire GIL)
            let result = handlers.invoke(handler_id, request.clone());

            let mut response = match result {
                Ok(json_value) => Response::from_json_value(json_value, 200),
                Err(err) => Response::error(500, &err),
            };

            // Execute after middleware
            match middleware.execute_after(&request, &mut response) {
                Ok(MiddlewareAction::Continue) => {}
                Ok(MiddlewareAction::Stop(new_response)) => {
                    return build_hyper_response(&new_response);
                }
                Err(e) => {
                    let error_response = Response::error(e.status, &e.message);
                    return build_hyper_response(&error_response);
                }
            }

            build_hyper_response(&response)
        }
        None => {
            // 404 Not Found
            let response = Response::not_found(&format!("Not Found: {} {}", method, path));
            build_hyper_response(&response)
        }
    }
}

/// Build a Hyper response from our Response type.
fn build_hyper_response(response: &Response) -> Result<HyperResponse<Full<Bytes>>, Infallible> {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = HyperResponse::builder().status(status);

    for (key, value) in &response.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    let body = Full::new(Bytes::from(response.body_bytes().to_vec()));

    Ok(builder.body(body).unwrap())
}
