---
title: Examples
description: Code examples and use cases for Cello Framework
---

# Examples

Explore working code examples for various use cases.

## Example Categories

<div class="grid cards" markdown>

-   :material-code-tags:{ .lg .middle } **Basic Examples**

    ---

    Simple examples to get started quickly

    [:octicons-arrow-right-24: Basic](basic/hello-world.md)

-   :material-cog:{ .lg .middle } **Advanced Examples**

    ---

    Complex applications and patterns

    [:octicons-arrow-right-24: Advanced](advanced/fullstack.md)

-   :material-office-building:{ .lg .middle } **Enterprise Examples**

    ---

    Production-ready enterprise patterns

    [:octicons-arrow-right-24: Enterprise](enterprise/multi-tenant.md)

</div>

---

## Quick Reference

### Basic Examples

| Example | Description | Features |
|---------|-------------|----------|
| [Hello World](basic/hello-world.md) | Minimal Cello app | Routing |
| [REST API](basic/rest-api.md) | CRUD operations | JSON, validation |
| [Form Handling](basic/forms.md) | Form submission | Multipart, files |

### Advanced Examples

| Example | Description | Features |
|---------|-------------|----------|
| [Full-Stack App](advanced/fullstack.md) | Complete web app | Templates, static files |
| [Microservices](advanced/microservices.md) | Service architecture | gRPC, messaging |
| [Real-time Dashboard](advanced/realtime-dashboard.md) | Live updates | WebSocket, SSE |

### Enterprise Examples

| Example | Description | Features |
|---------|-------------|----------|
| [Multi-tenant SaaS](enterprise/multi-tenant.md) | Tenant isolation | RBAC, data partitioning |
| [API Gateway](enterprise/api-gateway.md) | API management | Rate limiting, auth |
| [Event Sourcing](enterprise/event-sourcing.md) | Event-driven architecture | CQRS, event store |

---

## Running Examples

All examples are available in the `examples/` directory of the repository.

```bash
# Clone the repository
git clone https://github.com/jagadeesh32/cello.git
cd cello

# Install dependencies
pip install -e .

# Run an example
python examples/hello.py
```

---

## Example: Hello World

The simplest Cello application:

```python title="examples/hello.py"
from cello import App

app = App()

@app.get("/")
def hello(request):
    return {"message": "Hello, World!"}

@app.get("/greet/{name}")
def greet(request):
    name = request.params["name"]
    return {"message": f"Hello, {name}!"}

if __name__ == "__main__":
    app.run()
```

---

## Example: REST API

Complete CRUD API with validation:

```python title="examples/simple_api.py"
from cello import App, Response, Blueprint
from cello.middleware import RateLimitConfig

app = App()

# Enable middleware
app.enable_cors()
app.enable_logging()
app.enable_rate_limit(requests=100, window=60)

# In-memory database
users = {
    "1": {"id": "1", "name": "Alice", "email": "alice@example.com"},
    "2": {"id": "2", "name": "Bob", "email": "bob@example.com"},
}

# API Blueprint
api = Blueprint("/api/v1")

@api.get("/users")
def list_users(request):
    """List all users"""
    return {"users": list(users.values())}

@api.get("/users/{id}")
def get_user(request):
    """Get a specific user"""
    user_id = request.params["id"]
    if user_id not in users:
        return Response.json({"error": "User not found"}, status=404)
    return users[user_id]

@api.post("/users")
def create_user(request):
    """Create a new user"""
    data = request.json()

    # Validation
    if not data.get("name"):
        return Response.json({"error": "Name is required"}, status=400)
    if not data.get("email"):
        return Response.json({"error": "Email is required"}, status=400)

    # Create user
    user_id = str(len(users) + 1)
    user = {"id": user_id, "name": data["name"], "email": data["email"]}
    users[user_id] = user

    return Response.json(user, status=201)

@api.put("/users/{id}")
def update_user(request):
    """Update a user"""
    user_id = request.params["id"]
    if user_id not in users:
        return Response.json({"error": "User not found"}, status=404)

    data = request.json()
    users[user_id].update({
        "name": data.get("name", users[user_id]["name"]),
        "email": data.get("email", users[user_id]["email"]),
    })

    return users[user_id]

@api.delete("/users/{id}")
def delete_user(request):
    """Delete a user"""
    user_id = request.params["id"]
    if user_id in users:
        del users[user_id]
    return Response.no_content()

# Register blueprint
app.register_blueprint(api)

if __name__ == "__main__":
    app.run(port=8000)
```

---

## Example: WebSocket Chat

Real-time chat application:

```python title="examples/chat.py"
from cello import App, Response
import json

app = App()

# Connected clients
clients = {}

@app.get("/")
def index(request):
    return Response.html("""
    <!DOCTYPE html>
    <html>
    <head><title>Chat</title></head>
    <body>
        <div id="messages"></div>
        <input type="text" id="input" placeholder="Type a message...">
        <button onclick="send()">Send</button>
        <script>
            const ws = new WebSocket('ws://' + location.host + '/ws');
            const messages = document.getElementById('messages');
            const input = document.getElementById('input');

            ws.onmessage = (e) => {
                const data = JSON.parse(e.data);
                messages.innerHTML += '<p><b>' + data.user + ':</b> ' + data.text + '</p>';
            };

            function send() {
                ws.send(JSON.stringify({text: input.value}));
                input.value = '';
            }

            input.onkeypress = (e) => { if (e.key === 'Enter') send(); };
        </script>
    </body>
    </html>
    """)

@app.websocket("/ws")
def chat(ws):
    user_id = str(id(ws))
    clients[user_id] = ws

    # Notify others
    broadcast({"user": "System", "text": f"User {user_id[:4]} joined"})

    try:
        while True:
            message = ws.recv()
            if message is None:
                break

            data = json.loads(message.text)
            broadcast({
                "user": user_id[:4],
                "text": data.get("text", "")
            })
    finally:
        del clients[user_id]
        broadcast({"user": "System", "text": f"User {user_id[:4]} left"})

def broadcast(message):
    data = json.dumps(message)
    for client in clients.values():
        client.send_text(data)

if __name__ == "__main__":
    app.run()
```

---

## Example: Enterprise App

Full enterprise configuration:

```python title="examples/enterprise.py"
from cello import App, Blueprint, Response, Depends
from cello.middleware import (
    JwtConfig, JwtAuth,
    RateLimitConfig, AdaptiveRateLimitConfig,
    SecurityHeadersConfig, CSP,
    SessionConfig,
    PrometheusConfig
)
from cello.guards import RoleGuard, PermissionGuard

# Configuration
app = App(name="enterprise-api", env="production")

# Security Headers
app.enable_security_headers(SecurityHeadersConfig(
    csp=CSP(
        default_src=["'self'"],
        script_src=["'self'"],
        style_src=["'self'", "'unsafe-inline'"]
    ),
    hsts_max_age=31536000,
    hsts_include_subdomains=True
))

# JWT Authentication
jwt_config = JwtConfig(
    secret=b"production-secret-key-minimum-32-bytes",
    algorithm="HS256",
    expiration=3600
)
jwt_auth = JwtAuth(jwt_config)
jwt_auth.skip_path("/health")
jwt_auth.skip_path("/metrics")
jwt_auth.skip_path("/api/v1/auth/login")
app.use(jwt_auth)

# Adaptive Rate Limiting
app.enable_rate_limit(AdaptiveRateLimitConfig(
    base_requests=1000,
    window=60,
    cpu_threshold=0.8,
    memory_threshold=0.9
))

# Prometheus Metrics
app.enable_metrics(PrometheusConfig(path="/metrics"))

# Health Check
@app.get("/health")
def health(request):
    return {"status": "healthy", "version": "1.0.0"}

# Auth Blueprint
auth_bp = Blueprint("/api/v1/auth")

@auth_bp.post("/login")
def login(request):
    # Authentication logic
    data = request.json()
    # ... validate credentials
    token = jwt_auth.create_token({"sub": data["username"], "role": "user"})
    return {"token": token}

# Admin Blueprint with Guards
admin_bp = Blueprint("/api/v1/admin", guards=[RoleGuard(["admin"])])

@admin_bp.get("/users")
def list_users(request):
    return {"users": []}

@admin_bp.delete("/users/{id}", guards=[PermissionGuard(["users:delete"])])
def delete_user(request):
    return Response.no_content()

# Register blueprints
app.register_blueprint(auth_bp)
app.register_blueprint(admin_bp)

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000, workers=4)
```

---

## More Examples

Browse the full collection in the [GitHub repository](https://github.com/jagadeesh32/cello/tree/main/examples).
