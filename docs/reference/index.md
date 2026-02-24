---
title: API Reference & Configuration
description: Complete technical reference for every class, method, and configuration option in Cello Framework
icon: material/book-open-page-variant
---

# :material-book-open-page-variant: API Reference & Configuration

<div class="grid" markdown>

!!! abstract "Everything you need to build with Cello"

    This section provides the **complete technical reference** for every public API, configuration option, CLI flag, and error code in the Cello Framework. Use it as your go-to lookup while developing.

</div>

---

## :material-api: Core API Reference

The building blocks of every Cello application -- from creating your app to handling requests and sending responses.

<div class="grid cards" markdown>

-   :material-application-braces:{ .lg .middle } **App**

    ---

    The main application class. Create instances, register routes, configure middleware, and start the server.

    **Key methods:** `get()` `post()` `put()` `delete()` `run()` `use()` `enable_cors()`

    [:octicons-arrow-right-24: App Reference](api/app.md)

-   :material-arrow-down-bold-box:{ .lg .middle } **Request**

    ---

    The HTTP request object passed to every handler. Access headers, path params, query strings, and parsed body data.

    **Key methods:** `json()` `text()` `form()` `params` `query` `get_header()`

    [:octicons-arrow-right-24: Request Reference](api/request.md)

-   :material-arrow-up-bold-box:{ .lg .middle } **Response**

    ---

    Build HTTP responses with the right content type, status code, and headers. Supports JSON, HTML, streaming, and more.

    **Key methods:** `json()` `html()` `text()` `redirect()` `no_content()` `stream()`

    [:octicons-arrow-right-24: Response Reference](api/response.md)

-   :material-file-tree:{ .lg .middle } **Blueprint**

    ---

    Group related routes under a common prefix with shared middleware and guards. Flask-inspired modular architecture.

    **Key methods:** `get()` `post()` `put()` `delete()` `use()` `register_blueprint()`

    [:octicons-arrow-right-24: Blueprint Reference](api/blueprint.md)

-   :material-layers-triple:{ .lg .middle } **Middleware**

    ---

    The full middleware pipeline -- CORS, rate limiting, caching, compression, logging, circuit breaker, and more.

    **Key concepts:** `Middleware trait` `MiddlewareResult` `priority()` `process()`

    [:octicons-arrow-right-24: Middleware Reference](api/middleware.md)

-   :material-shield-account:{ .lg .middle } **Guards**

    ---

    Role-based and permission-based access control. Composable guards that protect routes and blueprints.

    **Key classes:** `RoleGuard` `PermissionGuard` `CompositeGuard`

    [:octicons-arrow-right-24: Guards Reference](api/guards.md)

-   :material-contain:{ .lg .middle } **Context**

    ---

    Request-scoped context for dependency injection, shared state, and passing data between middleware and handlers.

    **Key methods:** `get()` `set()` `inject()` `Depends()`

    [:octicons-arrow-right-24: Context Reference](api/context.md)

</div>

---

## :material-cog: Configuration Reference

Fine-tune every aspect of your Cello server, security layer, and middleware stack.

<div class="grid cards" markdown>

-   :material-server:{ .lg .middle } **Server Configuration**

    ---

    Host, port, worker count, HTTP/2, HTTP/3, TLS certificates, cluster mode, and timeout settings.

    [:octicons-arrow-right-24: Server Config](config/server.md)

-   :material-shield-lock:{ .lg .middle } **Security Configuration**

    ---

    JWT secrets, session cookies, CSRF tokens, CSP policies, HSTS, and security header defaults.

    [:octicons-arrow-right-24: Security Config](config/security.md)

-   :material-layers-triple:{ .lg .middle } **Middleware Configuration**

    ---

    All middleware options: rate limit windows, cache TTLs, compression levels, CORS origins, and more.

    [:octicons-arrow-right-24: Middleware Config](config/middleware.md)

</div>

---

## :material-console: CLI Reference

Control your Cello application from the command line.

[:octicons-arrow-right-24: Full CLI Reference](cli.md)

| Flag | Description | Default |
|:-----|:------------|:--------|
| `--host` | Host address to bind | `127.0.0.1` |
| `--port` | Port number | `8000` |
| `--workers` | Number of worker processes | `1` |
| `--env` | Environment (`development` / `production`) | `development` |
| `--reload` | Enable hot reload on file changes | `false` |
| `--debug` | Enable debug-level logging | `false` |

---

## :material-alert-circle: Error Codes

[:octicons-arrow-right-24: Full Error Codes Reference](errors.md)

Cello uses [RFC 7807 Problem Details](https://datatracker.ietf.org/doc/html/rfc7807) for structured error responses. See the full reference for every error type, HTTP status mapping, and custom error handling patterns.

---

## :material-lightning-bolt: Quick Lookup

The most commonly used classes and functions at a glance.

| Class / Function | Module | Description |
|:-----------------|:-------|:------------|
| [`App`](api/app.md) | `cello` | Main application entry point |
| [`Request`](api/request.md) | `cello` | HTTP request object |
| [`Response`](api/response.md) | `cello` | HTTP response builder |
| [`Blueprint`](api/blueprint.md) | `cello` | Route grouping |
| [`Depends`](api/context.md) | `cello` | Dependency injection marker |
| [`RoleGuard`](api/guards.md) | `cello.guards` | Role-based access guard |
| [`PermissionGuard`](api/guards.md) | `cello.guards` | Permission-based access guard |
| [`JwtConfig`](config/security.md) | `cello.middleware` | JWT authentication config |
| [`RateLimitConfig`](config/middleware.md) | `cello.middleware` | Rate limiting config |
| [`SessionConfig`](config/security.md) | `cello.middleware` | Session management config |
| [`Response.json()`](api/response.md) | `cello` | RFC 7807 error response |

---

## :material-code-braces: Common API Patterns

=== "Route Handlers"

    ```python
    from cello import App, Response

    app = App()

    @app.get("/users/{id}")
    def get_user(request):
        user_id = request.params["id"]
        return {"id": user_id, "name": "Alice"}  # (1)!

    @app.post("/users")
    def create_user(request):
        data = request.json()
        return Response.json({"created": True, **data}, status=201)
    ```

    1.  Returning a `dict` is the fastest path -- Rust handles JSON serialization via SIMD.

=== "Blueprints & Middleware"

    ```python
    from cello import App, Blueprint

    app = App()
    app.enable_cors()
    app.enable_logging()
    app.enable_rate_limit(requests=100, window=60)

    api = Blueprint("/api/v1")

    @api.get("/items")
    def list_items(request):
        return {"items": []}

    app.register_blueprint(api)
    ```

=== "Guards & Auth"

    ```python
    from cello import App
    from cello.guards import RoleGuard, PermissionGuard
    from cello.middleware import JwtConfig, JwtAuth

    app = App()

    jwt = JwtAuth(JwtConfig(
        secret=b"your-secret-key-minimum-32-bytes!",
        algorithm="HS256",
        expiration=3600,
    ))
    app.use(jwt)

    @app.get("/admin", guards=[RoleGuard(["admin"])])
    def admin_panel(request):
        return {"admin": True}
    ```

=== "Dependency Injection"

    ```python
    from cello import App, Depends

    def get_db():
        return DatabaseConnection()

    def get_current_user(request, db=Depends(get_db)):
        token = request.get_header("Authorization")
        return db.get_user_by_token(token)

    @app.get("/profile")
    def profile(request, user=Depends(get_current_user)):
        return {"user": user.name}
    ```

---

<div class="grid" markdown>

!!! tip "Can't find what you're looking for?"

    Try the **search bar** at the top of the page, or browse the full navigation tree on the left. You can also check the [Examples](../examples/index.md) section for working code.

</div>
