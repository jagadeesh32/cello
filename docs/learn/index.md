---
title: Learn
description: Tutorials, guides, and patterns for Cello Framework
---

# Learn Cello

Master Cello through hands-on tutorials, practical guides, and proven patterns.

## Learning Paths

<div class="grid cards" markdown>

-   :material-school:{ .lg .middle } **Beginner**

    ---

    New to Cello? Start here with the fundamentals.

    1. [Installation](../getting-started/installation.md)
    2. [Quick Start](../getting-started/quickstart.md)
    3. [First Application](../getting-started/first-app.md)
    4. [Build a REST API](tutorials/rest-api.md)

-   :material-book-open-variant:{ .lg .middle } **Intermediate**

    ---

    Ready for more? Learn advanced patterns.

    1. [Authentication System](tutorials/auth-system.md)
    2. [Real-time Chat App](tutorials/chat-app.md)
    3. [Testing Guide](guides/testing.md)
    4. [Error Handling](guides/error-handling.md)

-   :material-rocket-launch:{ .lg .middle } **Advanced**

    ---

    Building production systems? Go deeper.

    1. [Microservices](tutorials/microservices.md)
    2. [Performance Tuning](guides/performance.md)
    3. [Production Deployment](guides/deployment.md)
    4. [Enterprise Patterns](patterns/cqrs.md)

</div>

---

## Tutorials

Step-by-step guides to build complete applications.

| Tutorial | Level | Duration | Description |
|----------|-------|----------|-------------|
| [Build a REST API](tutorials/rest-api.md) | Beginner | 30 min | CRUD API with validation |
| [Build a Chat App](tutorials/chat-app.md) | Intermediate | 45 min | Real-time WebSocket chat |
| [Authentication System](tutorials/auth-system.md) | Intermediate | 60 min | JWT auth with refresh tokens |
| [Microservices](tutorials/microservices.md) | Advanced | 90 min | Service-to-service communication |

---

## Guides

In-depth guides on specific topics.

| Guide | Description |
|-------|-------------|
| [Best Practices](guides/best-practices.md) | Code organization, security, performance |
| [Error Handling](guides/error-handling.md) | RFC 7807, exception handlers, logging |
| [Testing](guides/testing.md) | Unit tests, integration tests, mocking |
| [Performance Tuning](guides/performance.md) | Optimization, profiling, benchmarks |
| [Production Deployment](guides/deployment.md) | Docker, Kubernetes, monitoring |

---

## Patterns

Architectural patterns for building robust applications.

| Pattern | Use Case |
|---------|----------|
| [Repository Pattern](patterns/repository.md) | Data access abstraction |
| [Service Layer](patterns/service-layer.md) | Business logic organization |
| [Event-Driven](patterns/event-driven.md) | Decoupled components |
| [CQRS](patterns/cqrs.md) | Separate read/write models |

---

## Quick Examples

### Basic CRUD API

```python
from cello import App, Response

app = App()
items = {}

@app.get("/items")
def list_items(request):
    return {"items": list(items.values())}

@app.get("/items/{id}")
def get_item(request):
    item_id = request.params["id"]
    if item_id not in items:
        return Response.json({"error": "Not found"}, status=404)
    return items[item_id]

@app.post("/items")
def create_item(request):
    data = request.json()
    item_id = str(len(items) + 1)
    items[item_id] = {"id": item_id, **data}
    return Response.json(items[item_id], status=201)

@app.delete("/items/{id}")
def delete_item(request):
    item_id = request.params["id"]
    if item_id in items:
        del items[item_id]
    return Response.no_content()

if __name__ == "__main__":
    app.run()
```

### JWT Authentication

```python
from cello import App, Response
from cello.middleware import JwtConfig, JwtAuth, create_token

app = App()

jwt_config = JwtConfig(
    secret=b"your-secret-key-minimum-32-bytes-long",
    algorithm="HS256",
    expiration=3600
)

# Apply JWT auth to all routes except /login
jwt_auth = JwtAuth(jwt_config).skip_path("/login")
app.use(jwt_auth)

@app.post("/login")
def login(request):
    data = request.json()
    # Verify credentials (simplified)
    if data.get("username") == "admin" and data.get("password") == "secret":
        token = create_token(jwt_config, {"sub": "admin", "role": "admin"})
        return {"token": token}
    return Response.json({"error": "Invalid credentials"}, status=401)

@app.get("/profile")
def profile(request):
    claims = request.context.get("jwt_claims")
    return {"user": claims["sub"], "role": claims.get("role")}

if __name__ == "__main__":
    app.run()
```

### WebSocket Chat

```python
from cello import App

app = App()
clients = set()

@app.websocket("/ws/chat")
def chat(ws):
    clients.add(ws)
    ws.send_text("Welcome to the chat!")

    try:
        while True:
            message = ws.recv()
            if message is None:
                break

            # Broadcast to all clients
            for client in clients:
                if client != ws:
                    client.send_text(f"User: {message.text}")
    finally:
        clients.discard(ws)

if __name__ == "__main__":
    app.run()
```

---

## Community Resources

- :material-github: [GitHub Discussions](https://github.com/jagadeesh32/cello/discussions)
- :material-discord: [Discord Server](https://discord.gg/cello)
- :material-stack-overflow: [Stack Overflow](https://stackoverflow.com/questions/tagged/cello-framework)

---

## Contributing Tutorials

Want to contribute a tutorial or guide? See our [contribution guidelines](../community/contributing.md).
