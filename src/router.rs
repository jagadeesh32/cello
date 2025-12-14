//! Radix-tree based HTTP router.
//!
//! Uses the `matchit` crate for fast O(log n) route matching.

use matchit::Router as MatchitRouter;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Route information containing the handler ID and extracted parameters.
#[derive(Clone, Debug)]
pub struct RouteMatch {
    pub handler_id: usize,
    pub params: HashMap<String, String>,
}

/// HTTP method-based router using radix trees.
#[derive(Clone)]
pub struct Router {
    /// Separate router for each HTTP method
    routes: Arc<RwLock<HashMap<String, MatchitRouter<usize>>>>,
}

impl Router {
    /// Create a new empty router.
    pub fn new() -> Self {
        Router {
            routes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Convert Python-style path params {param} to matchit-style :param
    fn convert_path_params(path: &str) -> String {
        let mut result = String::with_capacity(path.len());
        
        for c in path.chars() {
            if c == '{' {
                result.push(':');
            } else if c == '}' {
                // Skip closing brace
            } else {
                result.push(c);
            }
        }
        
        result
    }

    /// Add a route for a specific HTTP method.
    ///
    /// # Arguments
    /// * `method` - HTTP method (GET, POST, etc.)
    /// * `path` - URL path pattern with optional parameters (e.g., "/users/{id}")
    /// * `handler_id` - ID of the registered handler
    pub fn add_route(&mut self, method: &str, path: &str, handler_id: usize) -> Result<(), String> {
        let mut routes = self.routes.write();
        let method_router = routes
            .entry(method.to_uppercase())
            .or_insert_with(MatchitRouter::new);

        // Convert {param} to :param for matchit compatibility
        let converted_path = Self::convert_path_params(path);
        
        method_router
            .insert(&converted_path, handler_id)
            .map_err(|e| format!("Failed to add route: {}", e))
    }

    /// Match a request path against registered routes.
    ///
    /// # Arguments
    /// * `method` - HTTP method of the request
    /// * `path` - URL path to match
    ///
    /// # Returns
    /// * `Some(RouteMatch)` if a matching route is found
    /// * `None` if no route matches
    pub fn match_route(&self, method: &str, path: &str) -> Option<RouteMatch> {
        let routes = self.routes.read();
        let method_router = routes.get(&method.to_uppercase())?;

        match method_router.at(path) {
            Ok(matched) => {
                let params: HashMap<String, String> = matched
                    .params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();

                Some(RouteMatch {
                    handler_id: *matched.value,
                    params,
                })
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_routing() {
        let mut router = Router::new();
        router.add_route("GET", "/", 0).unwrap();
        router.add_route("GET", "/hello", 1).unwrap();
        router.add_route("POST", "/users", 2).unwrap();

        let match1 = router.match_route("GET", "/").unwrap();
        assert_eq!(match1.handler_id, 0);

        let match2 = router.match_route("GET", "/hello").unwrap();
        assert_eq!(match2.handler_id, 1);

        let match3 = router.match_route("POST", "/users").unwrap();
        assert_eq!(match3.handler_id, 2);

        assert!(router.match_route("DELETE", "/").is_none());
    }

    #[test]
    fn test_path_parameters() {
        let mut router = Router::new();
        
        // Test with curly-brace style params - these get converted to :param
        router.add_route("GET", "/users/{id}", 0).unwrap();
        router.add_route("GET", "/posts/{post_id}/comments/{comment_id}", 1).unwrap();

        // Verify the router matches
        let match1 = router.match_route("GET", "/users/123");
        assert!(match1.is_some(), "Route should match /users/123");
        let match1 = match1.unwrap();
        assert_eq!(match1.handler_id, 0);
        assert_eq!(match1.params.get("id"), Some(&"123".to_string()));

        let match2 = router.match_route("GET", "/posts/456/comments/789");
        assert!(match2.is_some(), "Route should match /posts/456/comments/789");
        let match2 = match2.unwrap();
        assert_eq!(match2.handler_id, 1);
        assert_eq!(match2.params.get("post_id"), Some(&"456".to_string()));
        assert_eq!(match2.params.get("comment_id"), Some(&"789".to_string()));
    }
}
