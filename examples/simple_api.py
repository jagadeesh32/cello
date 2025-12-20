#!/usr/bin/env python3
"""
Cello Framework - Simple API with OpenAPI Documentation
=========================================================

Run with: python examples/simple_api.py
Then visit: 
  - http://127.0.0.1:8080/docs (Swagger UI)
  - http://127.0.0.1:8080/redoc (ReDoc)
"""

from cello import App, Response, Blueprint

app = App()

# Enable middleware
app.enable_cors(origins=["*"])
app.enable_logging()


# =============================================================================
# OpenAPI Spec with actual operations
# =============================================================================

OPENAPI_SPEC = {
    "openapi": "3.0.3",
    "info": {
        "title": "Cello Sample API",
        "version": "1.0.0",
        "description": "A sample REST API built with Cello Framework"
    },
    "paths": {
        "/": {
            "get": {
                "summary": "Home",
                "description": "Returns API welcome message",
                "operationId": "get_home",
                "tags": ["General"],
                "responses": {
                    "200": {
                        "description": "Welcome message",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "message": {"type": "string"},
                                        "version": {"type": "string"}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "/health": {
            "get": {
                "summary": "Health Check",
                "description": "Returns server health status",
                "operationId": "get_health",
                "tags": ["General"],
                "responses": {
                    "200": {
                        "description": "Health status",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "status": {"type": "string"}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "/users": {
            "get": {
                "summary": "List Users",
                "description": "Get all users",
                "operationId": "list_users",
                "tags": ["Users"],
                "responses": {
                    "200": {
                        "description": "List of users",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "properties": {
                                        "users": {
                                            "type": "array",
                                            "items": {
                                                "type": "object",
                                                "properties": {
                                                    "id": {"type": "integer"},
                                                    "name": {"type": "string"},
                                                    "email": {"type": "string"}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "post": {
                "summary": "Create User",
                "description": "Create a new user",
                "operationId": "create_user",
                "tags": ["Users"],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "name": {"type": "string"},
                                    "email": {"type": "string"}
                                },
                                "required": ["name", "email"]
                            }
                        }
                    }
                },
                "responses": {
                    "201": {
                        "description": "User created successfully"
                    }
                }
            }
        },
        "/users/{id}": {
            "get": {
                "summary": "Get User",
                "description": "Get user by ID",
                "operationId": "get_user",
                "tags": ["Users"],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "integer"}
                    }
                ],
                "responses": {
                    "200": {
                        "description": "User details"
                    },
                    "404": {
                        "description": "User not found"
                    }
                }
            },
            "put": {
                "summary": "Update User",
                "description": "Update an existing user",
                "operationId": "update_user",
                "tags": ["Users"],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "integer"}
                    }
                ],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "name": {"type": "string"},
                                    "email": {"type": "string"}
                                }
                            }
                        }
                    }
                },
                "responses": {
                    "200": {
                        "description": "User updated"
                    }
                }
            },
            "delete": {
                "summary": "Delete User",
                "description": "Delete a user",
                "operationId": "delete_user",
                "tags": ["Users"],
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": True,
                        "schema": {"type": "integer"}
                    }
                ],
                "responses": {
                    "204": {
                        "description": "User deleted"
                    }
                }
            }
        },
        "/items": {
            "get": {
                "summary": "List Items",
                "description": "Get all items",
                "operationId": "list_items",
                "tags": ["Items"],
                "parameters": [
                    {
                        "name": "limit",
                        "in": "query",
                        "schema": {"type": "integer", "default": 10}
                    },
                    {
                        "name": "offset",
                        "in": "query",
                        "schema": {"type": "integer", "default": 0}
                    }
                ],
                "responses": {
                    "200": {
                        "description": "List of items"
                    }
                }
            },
            "post": {
                "summary": "Create Item",
                "description": "Create a new item",
                "operationId": "create_item",
                "tags": ["Items"],
                "requestBody": {
                    "required": True,
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "properties": {
                                    "name": {"type": "string"},
                                    "price": {"type": "number"}
                                }
                            }
                        }
                    }
                },
                "responses": {
                    "201": {
                        "description": "Item created"
                    }
                }
            }
        }
    },
    "tags": [
        {"name": "General", "description": "General endpoints"},
        {"name": "Users", "description": "User management"},
        {"name": "Items", "description": "Item management"}
    ]
}


# =============================================================================
# Custom OpenAPI Handler
# =============================================================================

@app.get("/openapi.json")
def openapi_spec(request):
    return OPENAPI_SPEC


@app.get("/docs")
def swagger_ui(request):
    html = '''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Cello API - Swagger UI</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
    <style>
        body { margin: 0; padding: 0; }
        .swagger-ui .topbar { display: none; }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = () => {
            window.ui = SwaggerUIBundle({
                url: "/openapi.json",
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [SwaggerUIBundle.presets.apis, SwaggerUIStandalonePreset],
                layout: "StandaloneLayout"
            });
        };
    </script>
</body>
</html>'''
    return Response.html(html)


@app.get("/redoc")
def redoc_ui(request):
    html = '''<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Cello API - ReDoc</title>
    <link href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700" rel="stylesheet">
    <style>body { margin: 0; padding: 0; }</style>
</head>
<body>
    <redoc spec-url="/openapi.json"></redoc>
    <script src="https://cdn.redoc.ly/redoc/latest/bundles/redoc.standalone.js"></script>
</body>
</html>'''
    return Response.html(html)


# =============================================================================
# API Endpoints
# =============================================================================

@app.get("/")
def home(request):
    return {"message": "Welcome to Cello API!", "version": "1.0.0"}


@app.get("/health")
def health(request):
    return {"status": "healthy"}


# Users endpoints
@app.get("/users")
def list_users(request):
    return {
        "users": [
            {"id": 1, "name": "Alice", "email": "alice@example.com"},
            {"id": 2, "name": "Bob", "email": "bob@example.com"},
            {"id": 3, "name": "Charlie", "email": "charlie@example.com"}
        ]
    }


@app.get("/users/{id}")
def get_user(request):
    user_id = request.params.get("id")
    return {"id": int(user_id), "name": f"User {user_id}", "email": f"user{user_id}@example.com"}


@app.post("/users")
def create_user(request):
    data = request.json()
    return {"message": "User created", "user": data, "id": 123}


@app.put("/users/{id}")
def update_user(request):
    user_id = request.params.get("id")
    data = request.json()
    return {"message": f"User {user_id} updated", "user": data}


@app.delete("/users/{id}")
def delete_user(request):
    user_id = request.params.get("id")
    return {"message": f"User {user_id} deleted"}


# Items endpoints
@app.get("/items")
def list_items(request):
    limit = int(request.query.get("limit", "10"))
    offset = int(request.query.get("offset", "0"))
    return {
        "items": [
            {"id": 1, "name": "Laptop", "price": 999.99},
            {"id": 2, "name": "Mouse", "price": 29.99},
            {"id": 3, "name": "Keyboard", "price": 79.99}
        ],
        "limit": limit,
        "offset": offset,
        "total": 3
    }


@app.post("/items")
def create_item(request):
    data = request.json()
    return {"message": "Item created", "item": data, "id": 456}


# =============================================================================
# Run Server
# =============================================================================

if __name__ == "__main__":
    print("\n" + "="*60)
    print("  CELLO API SERVER")
    print("="*60)
    print("\n  Endpoints:")
    print("    - Swagger UI:  http://127.0.0.1:8080/docs")
    print("    - ReDoc:       http://127.0.0.1:8080/redoc")
    print("    - OpenAPI:     http://127.0.0.1:8080/openapi.json")
    print("    - Home:        http://127.0.0.1:8080/")
    print("    - Users:       http://127.0.0.1:8080/users")
    print("    - Items:       http://127.0.0.1:8080/items")
    print("\n" + "="*60 + "\n")
    
    app.run(host="127.0.0.1", port=8080)
