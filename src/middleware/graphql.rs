//! GraphQL Support for Cello Framework.
//!
//! The built-in endpoint (`App.enable_graphql`). Queries are parsed with a
//! real GraphQL parser (`graphql_plan`): the operation type, operation name,
//! variables, fragments and `@skip`/`@include` come from the document, and
//! `max_depth`, `max_complexity` and `introspection` are enforced before any
//! resolver runs. Top-level fields are resolved by resolvers registered from
//! Rust; Python apps pass a `cello.graphql` schema to `App.enable_graphql`
//! instead.
//!
//! # Example
//! ```python
//! from cello import App, GraphQLConfig
//! from cello.graphql import Schema, Query, Mutation
//!
//! @Query
//! def users(info) -> list:
//!     return db.get_users()
//!
//! @Mutation
//! def create_user(info, name: str, email: str) -> dict:
//!     return db.create_user(name, email)
//!
//! app.enable_graphql(GraphQLConfig(), schema=Schema().query(users).mutation(create_user))
//! ```

use super::{AsyncMiddleware, MiddlewareAction, MiddlewareResult};
use crate::request::Request;
use crate::response::Response;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// GraphQL request body.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphQLRequest {
    pub query: String,
    #[serde(default, alias = "operationName")]
    pub operation_name: Option<String>,
    #[serde(default)]
    pub variables: Option<HashMap<String, JsonValue>>,
}

/// GraphQL response body.
#[derive(Debug, Clone, Serialize)]
pub struct GraphQLResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<GraphQLError>,
}

impl GraphQLResponse {
    pub fn data(data: JsonValue) -> Self {
        Self {
            data: Some(data),
            errors: Vec::new(),
        }
    }

    pub fn error(error: GraphQLError) -> Self {
        Self {
            data: None,
            errors: vec![error],
        }
    }

    pub fn errors(errors: Vec<GraphQLError>) -> Self {
        Self { data: None, errors }
    }
}

/// GraphQL error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<GraphQLLocation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<JsonValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, JsonValue>>,
}

impl GraphQLError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            locations: None,
            path: None,
            extensions: None,
        }
    }

    pub fn with_location(mut self, line: u32, column: u32) -> Self {
        self.locations = Some(vec![GraphQLLocation { line, column }]);
        self
    }

    pub fn with_path(mut self, path: Vec<JsonValue>) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_extension(mut self, key: &str, value: JsonValue) -> Self {
        let extensions = self.extensions.get_or_insert_with(HashMap::new);
        extensions.insert(key.to_string(), value);
        self
    }
}

/// GraphQL error location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLLocation {
    pub line: u32,
    pub column: u32,
}

/// GraphQL configuration.
#[derive(Clone)]
pub struct GraphQLConfig {
    /// GraphQL endpoint path
    pub path: String,
    /// Enable GraphQL Playground
    pub playground: bool,
    /// Playground path (default: /graphql)
    pub playground_path: Option<String>,
    /// Enable introspection
    pub introspection: bool,
    /// Maximum query depth
    pub max_depth: Option<usize>,
    /// Maximum query complexity
    pub max_complexity: Option<usize>,
    /// Enable batching
    pub batching: bool,
    /// Enable tracing extension
    pub tracing: bool,
}

impl Default for GraphQLConfig {
    fn default() -> Self {
        Self {
            path: "/graphql".to_string(),
            // Off by default: both reveal the schema to any caller.
            playground: false,
            playground_path: None,
            introspection: false,
            max_depth: Some(10),
            max_complexity: Some(1000),
            batching: false,
            tracing: false,
        }
    }
}

impl GraphQLConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(mut self, path: &str) -> Self {
        self.path = path.to_string();
        self
    }

    pub fn with_playground(mut self, enabled: bool) -> Self {
        self.playground = enabled;
        self
    }

    pub fn with_introspection(mut self, enabled: bool) -> Self {
        self.introspection = enabled;
        self
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    pub fn with_max_complexity(mut self, complexity: usize) -> Self {
        self.max_complexity = Some(complexity);
        self
    }
}

/// GraphQL field type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GraphQLType {
    Scalar,
    Object,
    Interface,
    Union,
    Enum,
    InputObject,
    List,
    NonNull,
}

/// GraphQL field definition.
#[derive(Clone)]
pub struct FieldDef {
    pub name: String,
    pub field_type: String,
    pub description: Option<String>,
    pub arguments: Vec<ArgumentDef>,
    pub resolver: Option<ResolverFn>,
}

/// GraphQL argument definition.
#[derive(Debug, Clone)]
pub struct ArgumentDef {
    pub name: String,
    pub arg_type: String,
    pub default_value: Option<JsonValue>,
    pub description: Option<String>,
}

/// Resolver function type.
pub type ResolverFn = Arc<dyn Fn(&ResolverContext) -> ResolverResult + Send + Sync>;

/// Result type for resolvers.
pub type ResolverResult = Result<JsonValue, GraphQLError>;

/// Resolver context passed to resolver functions.
#[derive(Clone)]
pub struct ResolverContext {
    pub parent: Option<JsonValue>,
    pub arguments: HashMap<String, JsonValue>,
    pub variables: HashMap<String, JsonValue>,
    pub request: Arc<Request>,
}

impl ResolverContext {
    pub fn new(request: Arc<Request>) -> Self {
        Self {
            parent: None,
            arguments: HashMap::new(),
            variables: HashMap::new(),
            request,
        }
    }

    pub fn with_parent(mut self, parent: JsonValue) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn with_arguments(mut self, args: HashMap<String, JsonValue>) -> Self {
        self.arguments = args;
        self
    }

    pub fn with_variables(mut self, vars: HashMap<String, JsonValue>) -> Self {
        self.variables = vars;
        self
    }

    pub fn arg<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        self.arguments
            .get(name)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn var<T: serde::de::DeserializeOwned>(&self, name: &str) -> Option<T> {
        self.variables
            .get(name)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Simple GraphQL schema.
pub struct GraphQLSchema {
    pub query_type: String,
    pub mutation_type: Option<String>,
    pub subscription_type: Option<String>,
    pub types: HashMap<String, TypeDef>,
}

/// Type definition.
#[derive(Clone)]
pub struct TypeDef {
    pub name: String,
    pub kind: GraphQLType,
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
}

impl GraphQLSchema {
    pub fn new() -> Self {
        Self {
            query_type: "Query".to_string(),
            mutation_type: None,
            subscription_type: None,
            types: HashMap::new(),
        }
    }

    pub fn with_query_type(mut self, name: &str) -> Self {
        self.query_type = name.to_string();
        self
    }

    pub fn with_mutation_type(mut self, name: &str) -> Self {
        self.mutation_type = Some(name.to_string());
        self
    }

    pub fn add_type(mut self, type_def: TypeDef) -> Self {
        self.types.insert(type_def.name.clone(), type_def);
        self
    }
}

impl Default for GraphQLSchema {
    fn default() -> Self {
        Self::new()
    }
}

/// GraphQL middleware for handling GraphQL requests.
pub struct GraphQLMiddleware {
    config: GraphQLConfig,
    schema: Arc<RwLock<GraphQLSchema>>,
    resolvers: Arc<RwLock<HashMap<String, ResolverFn>>>,
}

impl GraphQLMiddleware {
    pub fn new(config: GraphQLConfig) -> Self {
        Self {
            config,
            schema: Arc::new(RwLock::new(GraphQLSchema::new())),
            resolvers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_schema(self, schema: GraphQLSchema) -> Self {
        *self.schema.write() = schema;
        self
    }

    /// Register a resolver function.
    pub fn register_resolver<F>(&self, type_name: &str, field_name: &str, resolver: F)
    where
        F: Fn(&ResolverContext) -> ResolverResult + Send + Sync + 'static,
    {
        let key = format!("{type_name}.{field_name}");
        let mut resolvers = self.resolvers.write();
        resolvers.insert(key, Arc::new(resolver));
    }

    /// Execute a GraphQL query: parse it, select the operation, enforce
    /// `max_depth` / `max_complexity` / `introspection`, then resolve each
    /// top-level field of the operation's root type.
    ///
    /// Returns the HTTP status with the response: 400 when the document is
    /// rejected before execution, 200 otherwise.
    pub fn execute(
        &self,
        request: &GraphQLRequest,
        http_request: Arc<Request>,
    ) -> (u16, GraphQLResponse) {
        let variables: serde_json::Map<String, JsonValue> = request
            .variables
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let limits = super::graphql_plan::Limits {
            max_depth: self.config.max_depth,
            max_complexity: self.config.max_complexity,
            introspection: self.config.introspection,
        };
        let plan = match super::graphql_plan::plan(
            &request.query,
            request.operation_name.as_deref(),
            &variables,
            limits,
        ) {
            Ok(plan) => plan,
            Err(message) => return (400, GraphQLResponse::error(GraphQLError::new(&message))),
        };
        if plan.operation == "subscription" {
            return (
                400,
                GraphQLResponse::error(GraphQLError::new(
                    "Subscriptions are not supported over HTTP",
                )),
            );
        }
        if plan.introspection {
            return (200, self.handle_introspection(&request.query));
        }

        let type_name = {
            let schema = self.schema.read();
            if plan.operation == "mutation" {
                match &schema.mutation_type {
                    Some(t) => t.clone(),
                    None => {
                        return (
                            400,
                            GraphQLResponse::error(GraphQLError::new(
                                "The schema has no mutation type",
                            )),
                        )
                    }
                }
            } else {
                schema.query_type.clone()
            }
        };

        let base =
            ResolverContext::new(http_request).with_variables(variables.into_iter().collect());
        let resolvers = self.resolvers.read();
        let mut data = serde_json::Map::new();
        let mut errors = Vec::new();
        for field in plan.fields {
            if field.name == "__typename" {
                data.insert(field.alias, JsonValue::String(type_name.clone()));
                continue;
            }
            let key = format!("{type_name}.{}", field.name);
            let value = match resolvers.get(&key) {
                Some(resolver) => {
                    let ctx = base
                        .clone()
                        .with_arguments(field.arguments.into_iter().collect());
                    match resolver(&ctx) {
                        Ok(v) => v,
                        Err(e) => {
                            errors.push(e.with_path(vec![JsonValue::String(field.alias.clone())]));
                            JsonValue::Null
                        }
                    }
                }
                None => {
                    errors.push(
                        GraphQLError::new(&format!(
                            "Cannot query field \"{}\" on type \"{type_name}\"",
                            field.name
                        ))
                        .with_path(vec![JsonValue::String(field.alias.clone())]),
                    );
                    JsonValue::Null
                }
            };
            data.insert(field.alias, value);
        }
        (
            200,
            GraphQLResponse {
                data: Some(JsonValue::Object(data)),
                errors,
            },
        )
    }

    /// Handle introspection queries.
    fn handle_introspection(&self, _query: &str) -> GraphQLResponse {
        let schema = self.schema.read();

        let types: Vec<JsonValue> = schema
            .types
            .values()
            .map(|t| {
                json!({
                    "name": t.name,
                    "kind": format!("{:?}", t.kind).to_uppercase(),
                    "description": t.description,
                    "fields": t.fields.iter().map(|f| json!({
                        "name": f.name,
                        "type": {"name": f.field_type},
                        "description": f.description
                    })).collect::<Vec<_>>()
                })
            })
            .collect();

        GraphQLResponse::data(json!({
            "__schema": {
                "queryType": {"name": schema.query_type},
                "mutationType": schema.mutation_type.as_ref().map(|n| json!({"name": n})),
                "subscriptionType": schema.subscription_type.as_ref().map(|n| json!({"name": n})),
                "types": types
            }
        }))
    }

    /// Generate GraphQL Playground HTML.
    fn playground_html(&self) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset=utf-8/>
    <meta name="viewport" content="user-scalable=no, initial-scale=1.0, minimum-scale=1.0, maximum-scale=1.0, minimal-ui">
    <title>GraphQL Playground</title>
    <link rel="stylesheet" href="//cdn.jsdelivr.net/npm/graphql-playground-react/build/static/css/index.css" />
    <link rel="shortcut icon" href="//cdn.jsdelivr.net/npm/graphql-playground-react/build/favicon.png" />
    <script src="//cdn.jsdelivr.net/npm/graphql-playground-react/build/static/js/middleware.js"></script>
</head>
<body>
    <div id="root">
        <style>
            body {{
                background-color: rgb(23, 42, 58);
                font-family: Open Sans, sans-serif;
                height: 90vh;
            }}
            #root {{
                height: 100%;
                width: 100%;
                display: flex;
                align-items: center;
                justify-content: center;
            }}
            .loading {{
                font-size: 32px;
                font-weight: 200;
                color: rgba(255, 255, 255, .6);
                margin-left: 28px;
            }}
        </style>
        <span class="loading">Loading GraphQL Playground...</span>
    </div>
    <script>
        window.addEventListener('load', function (event) {{
            GraphQLPlayground.init(document.getElementById('root'), {{
                endpoint: '{}',
                settings: {{
                    'request.credentials': 'same-origin',
                }}
            }})
        }})
    </script>
</body>
</html>"#,
            self.config.path
        )
    }
}

impl AsyncMiddleware for GraphQLMiddleware {
    fn before_async<'a>(
        &'a self,
        request: &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = MiddlewareResult> + Send + 'a>> {
        Box::pin(async move {
            // Check if this is a GraphQL request
            if !request.path.starts_with(&self.config.path) {
                return Ok(MiddlewareAction::Continue);
            }

            // Handle GET for playground
            if request.method == "GET" && self.config.playground {
                let html = self.playground_html();
                let response = Response::html(&html, None);
                return Ok(MiddlewareAction::Stop(response));
            }

            // Handle POST for queries
            if request.method == "POST" {
                // Parse the GraphQL request
                let body = request.text().unwrap_or_default();
                let gql_request: Result<GraphQLRequest, _> = serde_json::from_str(&body);

                match gql_request {
                    Ok(gql_req) => {
                        // Clone the request for use in resolver context
                        let request_arc = Arc::new(request.clone());

                        let (status, gql_response) = self.execute(&gql_req, request_arc);
                        let body = serde_json::to_value(&gql_response).unwrap_or_else(
                            |_| json!({"errors": [{"message": "Serialization error"}]}),
                        );
                        let response = Response::from_json_value(body, status);
                        return Ok(MiddlewareAction::Stop(response));
                    }
                    Err(e) => {
                        let error = GraphQLError::new(&format!("Invalid GraphQL request: {e}"));
                        let body = serde_json::to_value(GraphQLResponse::error(error))
                            .unwrap_or_else(|_| json!({"errors": [{"message": "Error"}]}));
                        let response = Response::from_json_value(body, 400);
                        return Ok(MiddlewareAction::Stop(response));
                    }
                }
            }

            // Method not allowed
            let response = Response::from_json_value(json!({"error": "Method not allowed"}), 405);
            Ok(MiddlewareAction::Stop(response))
        })
    }

    fn priority(&self) -> i32 {
        -100 // Run before most middleware
    }

    fn name(&self) -> &str {
        "graphql"
    }

    fn serves_unrouted(&self, _method: &str, path: &str) -> bool {
        // The GraphQL endpoint/playground is not a registered route; claim it so
        // the router's fast-404 path lets `before_async` serve it.
        path == self.config.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_request_parsing() {
        let json = r#"{"query": "{ users { id name } }", "variables": {"limit": 10}}"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.query, "{ users { id name } }");
        assert!(request.variables.is_some());
    }

    #[test]
    fn test_graphql_response() {
        let response = GraphQLResponse::data(json!({"users": []}));
        assert!(response.data.is_some());
        assert!(response.errors.is_empty());

        let error_response = GraphQLResponse::error(GraphQLError::new("Test error"));
        assert!(error_response.data.is_none());
        assert_eq!(error_response.errors.len(), 1);
    }

    #[test]
    fn test_graphql_error() {
        let error = GraphQLError::new("Field not found")
            .with_location(1, 10)
            .with_path(vec![json!("users"), json!(0), json!("name")])
            .with_extension("code", json!("FIELD_NOT_FOUND"));

        assert_eq!(error.message, "Field not found");
        assert!(error.locations.is_some());
        assert!(error.path.is_some());
        assert!(error.extensions.is_some());
    }

    #[test]
    fn test_config_builder() {
        let config = GraphQLConfig::new()
            .with_path("/api/graphql")
            .with_playground(false)
            .with_max_depth(5);

        assert_eq!(config.path, "/api/graphql");
        assert!(!config.playground);
        assert_eq!(config.max_depth, Some(5));
    }
}
