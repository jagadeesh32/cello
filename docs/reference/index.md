---
title: Reference
description: Complete API reference for Cello Framework
---

# Reference

Complete technical reference for Cello Framework.

## API Reference

<div class="grid cards" markdown>

-   :material-application:{ .lg .middle } **App**

    ---

    Main application class and configuration

    [:octicons-arrow-right-24: App Reference](api/app.md)

-   :material-arrow-down-bold-box:{ .lg .middle } **Request**

    ---

    HTTP request object and methods

    [:octicons-arrow-right-24: Request Reference](api/request.md)

-   :material-arrow-up-bold-box:{ .lg .middle } **Response**

    ---

    HTTP response types and methods

    [:octicons-arrow-right-24: Response Reference](api/response.md)

-   :material-file-tree:{ .lg .middle } **Blueprint**

    ---

    Route grouping and organization

    [:octicons-arrow-right-24: Blueprint Reference](api/blueprint.md)

-   :material-middleware:{ .lg .middle } **Middleware**

    ---

    Built-in middleware configuration

    [:octicons-arrow-right-24: Middleware Reference](api/middleware.md)

-   :material-shield-account:{ .lg .middle } **Guards**

    ---

    Authorization guards and RBAC

    [:octicons-arrow-right-24: Guards Reference](api/guards.md)

</div>

---

## Configuration Reference

| Section | Description |
|---------|-------------|
| [Server Config](config/server.md) | Host, port, workers, protocols |
| [Security Config](config/security.md) | Auth, sessions, headers |
| [Middleware Config](config/middleware.md) | All middleware options |

---

## CLI Reference

[:octicons-arrow-right-24: CLI Reference](cli.md)

| Command | Description |
|---------|-------------|
| `--host` | Host address to bind |
| `--port` | Port number |
| `--workers` | Number of worker processes |
| `--env` | Environment (development/production) |
| `--reload` | Enable hot reload |
| `--debug` | Enable debug logging |

---

## Error Codes

[:octicons-arrow-right-24: Error Codes Reference](errors.md)

Standard HTTP error codes and Cello-specific error handling.
