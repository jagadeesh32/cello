//! Blueprint system for route grouping and inheritance.
//!
//! Blueprints allow organizing routes into groups with shared prefixes,
//! middleware, and other settings.

use parking_lot::RwLock;
use pyo3::prelude::*;
use std::sync::Arc;

use crate::middleware::MiddlewareChain;

/// Route definition within a blueprint.
#[derive(Clone)]
pub struct RouteDefinition {
    pub method: String,
    pub path: String,
    pub handler: PyObject,
}

/// Blueprint for grouping routes with a common prefix.
#[pyclass]
#[derive(Clone)]
pub struct Blueprint {
    /// URL prefix for all routes in this blueprint
    #[pyo3(get)]
    pub prefix: String,
    
    /// Name of the blueprint
    #[pyo3(get)]
    pub name: String,
    
    /// Routes defined in this blueprint
    routes: Arc<RwLock<Vec<RouteDefinition>>>,
    
    /// Nested blueprints
    children: Arc<RwLock<Vec<Blueprint>>>,
    
    /// Blueprint-specific middleware
    middleware: Arc<MiddlewareChain>,
}

#[pymethods]
impl Blueprint {
    /// Create a new blueprint with the given prefix.
    #[new]
    #[pyo3(signature = (prefix, name=None))]
    pub fn new(prefix: &str, name: Option<&str>) -> Self {
        let normalized_prefix = if prefix.starts_with('/') {
            prefix.to_string()
        } else {
            format!("/{}", prefix)
        };
        
        Blueprint {
            prefix: normalized_prefix.clone(),
            name: name.unwrap_or(&normalized_prefix).to_string(),
            routes: Arc::new(RwLock::new(Vec::new())),
            children: Arc::new(RwLock::new(Vec::new())),
            middleware: Arc::new(MiddlewareChain::new()),
        }
    }

    /// Register a GET route.
    pub fn get(&self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("GET", path, handler)
    }

    /// Register a POST route.
    pub fn post(&self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("POST", path, handler)
    }

    /// Register a PUT route.
    pub fn put(&self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("PUT", path, handler)
    }

    /// Register a DELETE route.
    pub fn delete(&self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("DELETE", path, handler)
    }

    /// Register a PATCH route.
    pub fn patch(&self, path: &str, handler: PyObject) -> PyResult<()> {
        self.add_route("PATCH", path, handler)
    }

    /// Register a nested blueprint.
    pub fn register(&self, blueprint: Blueprint) {
        let mut children = self.children.write();
        children.push(blueprint);
    }

    /// Get all routes including from nested blueprints.
    pub fn get_all_routes(&self) -> Vec<(String, String, PyObject)> {
        let mut all_routes = Vec::new();
        
        // Add routes from this blueprint
        let routes = self.routes.read();
        for route in routes.iter() {
            let full_path = format!("{}{}", self.prefix, route.path);
            all_routes.push((route.method.clone(), full_path, route.handler.clone()));
        }
        
        // Add routes from nested blueprints
        let children = self.children.read();
        for child in children.iter() {
            let child_routes = child.get_all_routes();
            for (method, path, handler) in child_routes {
                let full_path = format!("{}{}", self.prefix, path);
                all_routes.push((method, full_path, handler));
            }
        }
        
        all_routes
    }

    /// Internal route registration.
    fn add_route(&self, method: &str, path: &str, handler: PyObject) -> PyResult<()> {
        let normalized_path = if path.starts_with('/') || path.is_empty() {
            path.to_string()
        } else {
            format!("/{}", path)
        };
        
        let route = RouteDefinition {
            method: method.to_string(),
            path: normalized_path,
            handler,
        };
        
        self.routes.write().push(route);
        Ok(())
    }
}

impl Blueprint {
    /// Get the middleware chain for this blueprint.
    pub fn get_middleware(&self) -> Arc<MiddlewareChain> {
        self.middleware.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blueprint_creation() {
        let bp = Blueprint::new("/api", Some("api"));
        assert_eq!(bp.prefix, "/api");
        assert_eq!(bp.name, "api");
    }

    #[test]
    fn test_blueprint_prefix_normalization() {
        let bp1 = Blueprint::new("api", None);
        assert_eq!(bp1.prefix, "/api");

        let bp2 = Blueprint::new("/api", None);
        assert_eq!(bp2.prefix, "/api");
    }
}
