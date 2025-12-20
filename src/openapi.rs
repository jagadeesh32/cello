//! OpenAPI Schema Generation for Cello Framework
//!
//! This module provides automatic OpenAPI 3.0 schema generation from routes,
//! similar to FastAPI's approach but implemented in Rust for maximum performance.

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;

// ============================================================================
// OpenAPI Schema Types
// ============================================================================

/// OpenAPI 3.0 specification root object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAPISpec {
    pub openapi: String,
    pub info: Info,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<Server>>,
    pub paths: HashMap<String, PathItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<Tag>>,
}

/// API metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
}

/// Contact information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// License information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Server information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Path item containing operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
}

/// HTTP operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<Parameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    pub responses: HashMap<String, Response>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
}

/// Parameter definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String, // path, query, header, cookie
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
}

/// Request body definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content: HashMap<String, MediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Media type definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
}

/// Response definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<HashMap<String, MediaType>>,
}

/// JSON Schema definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
}

/// Components object for reusable schemas.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Components {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schemas: Option<HashMap<String, Schema>>,
    #[serde(rename = "securitySchemes", skip_serializing_if = "Option::is_none")]
    pub security_schemes: Option<HashMap<String, SecurityScheme>>,
}

/// Security scheme definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(rename = "bearerFormat", skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Tag for grouping operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ============================================================================
// OpenAPI Generator
// ============================================================================

/// Route metadata for OpenAPI generation.
#[derive(Debug, Clone)]
pub struct RouteMetadata {
    pub method: String,
    pub path: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub deprecated: bool,
}

/// OpenAPI schema generator.
#[derive(Clone)]
pub struct OpenAPIGenerator {
    info: Info,
    routes: Arc<RwLock<Vec<RouteMetadata>>>,
    servers: Vec<Server>,
    security_schemes: HashMap<String, SecurityScheme>,
}

impl OpenAPIGenerator {
    /// Create a new OpenAPI generator with default info.
    pub fn new(title: &str, version: &str) -> Self {
        Self {
            info: Info {
                title: title.to_string(),
                description: Some(format!("{} - Powered by Cello Framework", title)),
                version: version.to_string(),
                contact: None,
                license: Some(License {
                    name: "MIT".to_string(),
                    url: None,
                }),
            },
            routes: Arc::new(RwLock::new(Vec::new())),
            servers: vec![Server {
                url: "http://localhost:8000".to_string(),
                description: Some("Development server".to_string()),
            }],
            security_schemes: HashMap::new(),
        }
    }

    /// Set the API description.
    pub fn description(mut self, desc: &str) -> Self {
        self.info.description = Some(desc.to_string());
        self
    }

    /// Add a server.
    pub fn add_server(mut self, url: &str, description: Option<&str>) -> Self {
        self.servers.push(Server {
            url: url.to_string(),
            description: description.map(|s| s.to_string()),
        });
        self
    }

    /// Add JWT bearer authentication scheme.
    pub fn add_jwt_auth(mut self) -> Self {
        self.security_schemes.insert(
            "bearerAuth".to_string(),
            SecurityScheme {
                scheme_type: "http".to_string(),
                scheme: Some("bearer".to_string()),
                bearer_format: Some("JWT".to_string()),
                name: None,
                location: None,
            },
        );
        self
    }

    /// Add API key authentication scheme.
    pub fn add_api_key_auth(mut self, name: &str, location: &str) -> Self {
        self.security_schemes.insert(
            "apiKey".to_string(),
            SecurityScheme {
                scheme_type: "apiKey".to_string(),
                scheme: None,
                bearer_format: None,
                name: Some(name.to_string()),
                location: Some(location.to_string()),
            },
        );
        self
    }

    /// Register a route for OpenAPI documentation.
    pub fn register_route(&self, metadata: RouteMetadata) {
        self.routes.write().push(metadata);
    }

    /// Generate the OpenAPI specification.
    pub fn generate(&self) -> OpenAPISpec {
        let routes = self.routes.read();
        let mut paths: HashMap<String, PathItem> = HashMap::new();

        for route in routes.iter() {
            // Convert path parameters from {param} to OpenAPI format
            let openapi_path = route.path.clone();

            let path_item = paths.entry(openapi_path.clone()).or_default();

            // Extract path parameters
            let path_params: Vec<Parameter> = extract_path_params(&route.path);

            let operation = Operation {
                summary: route.summary.clone(),
                description: route.description.clone(),
                operation_id: Some(generate_operation_id(&route.method, &route.path)),
                tags: if route.tags.is_empty() {
                    None
                } else {
                    Some(route.tags.clone())
                },
                parameters: if path_params.is_empty() {
                    None
                } else {
                    Some(path_params)
                },
                request_body: if route.method == "POST" || route.method == "PUT" || route.method == "PATCH" {
                    Some(RequestBody {
                        description: Some("Request body".to_string()),
                        content: {
                            let mut content = HashMap::new();
                            content.insert(
                                "application/json".to_string(),
                                MediaType {
                                    schema: Some(Schema {
                                        schema_type: Some("object".to_string()),
                                        format: None,
                                        properties: None,
                                        items: None,
                                        reference: None,
                                        required: None,
                                        example: None,
                                    }),
                                },
                            );
                            content
                        },
                        required: Some(true),
                    })
                } else {
                    None
                },
                responses: {
                    let mut responses = HashMap::new();
                    responses.insert(
                        "200".to_string(),
                        Response {
                            description: "Successful response".to_string(),
                            content: Some({
                                let mut content = HashMap::new();
                                content.insert(
                                    "application/json".to_string(),
                                    MediaType {
                                        schema: Some(Schema {
                                            schema_type: Some("object".to_string()),
                                            format: None,
                                            properties: None,
                                            items: None,
                                            reference: None,
                                            required: None,
                                            example: None,
                                        }),
                                    },
                                );
                                content
                            }),
                        },
                    );
                    responses
                },
                security: None,
                deprecated: if route.deprecated { Some(true) } else { None },
            };

            match route.method.as_str() {
                "GET" => path_item.get = Some(operation),
                "POST" => path_item.post = Some(operation),
                "PUT" => path_item.put = Some(operation),
                "DELETE" => path_item.delete = Some(operation),
                "PATCH" => path_item.patch = Some(operation),
                "OPTIONS" => path_item.options = Some(operation),
                "HEAD" => path_item.head = Some(operation),
                _ => {}
            }
        }

        OpenAPISpec {
            openapi: "3.0.3".to_string(),
            info: self.info.clone(),
            servers: Some(self.servers.clone()),
            paths,
            components: if self.security_schemes.is_empty() {
                None
            } else {
                Some(Components {
                    schemas: None,
                    security_schemes: Some(self.security_schemes.clone()),
                })
            },
            tags: None,
        }
    }

    /// Generate JSON string of the OpenAPI spec.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.generate())
    }
}

/// Extract path parameters from a route path.
fn extract_path_params(path: &str) -> Vec<Parameter> {
    let mut params = Vec::new();
    let re = regex::Regex::new(r"\{([^}]+)\}").unwrap();
    
    for cap in re.captures_iter(path) {
        params.push(Parameter {
            name: cap[1].to_string(),
            location: "path".to_string(),
            description: Some(format!("Path parameter: {}", &cap[1])),
            required: Some(true),
            schema: Some(Schema {
                schema_type: Some("string".to_string()),
                format: None,
                properties: None,
                items: None,
                reference: None,
                required: None,
                example: None,
            }),
        });
    }
    
    params
}

/// Generate an operation ID from method and path.
fn generate_operation_id(method: &str, path: &str) -> String {
    let clean_path = path
        .replace('/', "_")
        .replace('{', "")
        .replace('}', "")
        .trim_matches('_')
        .to_string();
    
    format!("{}_{}", method.to_lowercase(), clean_path)
}

// ============================================================================
// Swagger UI HTML Generator
// ============================================================================

/// Generate Swagger UI HTML page.
pub fn swagger_ui_html(openapi_url: &str, title: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
    <style>
        body {{ margin: 0; padding: 0; }}
        .swagger-ui .topbar {{ display: none; }}
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
    <script>
        window.onload = () => {{
            window.ui = SwaggerUIBundle({{
                url: "{openapi_url}",
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.SwaggerUIStandalonePreset],
                layout: "StandaloneLayout"
            }});
        }};
    </script>
</body>
</html>"#,
        title = title,
        openapi_url = openapi_url
    )
}

/// Generate ReDoc HTML page.
pub fn redoc_html(openapi_url: &str, title: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - API Documentation</title>
    <link href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700" rel="stylesheet">
    <style>body {{ margin: 0; padding: 0; }}</style>
</head>
<body>
    <redoc spec-url="{openapi_url}"></redoc>
    <script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
</body>
</html>"#,
        title = title,
        openapi_url = openapi_url
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_generator() {
        let generator = OpenAPIGenerator::new("Test API", "1.0.0")
            .description("A test API")
            .add_jwt_auth();

        generator.register_route(RouteMetadata {
            method: "GET".to_string(),
            path: "/users/{id}".to_string(),
            summary: Some("Get user by ID".to_string()),
            description: Some("Retrieves a user by their unique identifier".to_string()),
            tags: vec!["users".to_string()],
            deprecated: false,
        });

        let spec = generator.generate();
        assert_eq!(spec.openapi, "3.0.3");
        assert_eq!(spec.info.title, "Test API");
        assert!(spec.paths.contains_key("/users/{id}"));
    }

    #[test]
    fn test_extract_path_params() {
        let params = extract_path_params("/users/{user_id}/posts/{post_id}");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "user_id");
        assert_eq!(params[1].name, "post_id");
    }

    #[test]
    fn test_swagger_ui_html() {
        let html = swagger_ui_html("/openapi.json", "My API");
        assert!(html.contains("swagger-ui"));
        assert!(html.contains("/openapi.json"));
    }
}
