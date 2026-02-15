---
title: Dependency Injection
description: FastAPI-inspired dependency injection system in Cello Framework
---

# Dependency Injection

Cello provides a FastAPI-inspired dependency injection (DI) system that lets you declare dependencies for your route handlers. Dependencies are resolved automatically at request time, with support for scoping, chaining, and testing overrides.

---

## The `Depends()` Marker

Use the `Depends` class to declare that a handler parameter should be injected:

```python
from cello import App, Depends

app = App()

def get_db():
    """Provide a database connection."""
    return DatabaseConnection(url="postgres://localhost/mydb")

@app.get("/users")
def list_users(request, db=Depends("database")):
    users = db.query("SELECT * FROM users")
    return {"users": users}
```

When a request arrives at `/users`, Cello resolves the `"database"` dependency and passes it as the `db` argument to your handler.

---

## Registering Dependencies

### Singleton Registration

Register a dependency that is shared across all requests for the lifetime of the application:

```python
from cello import App

app = App()

# Create the singleton instance
database = DatabaseConnection(url="postgres://localhost/mydb")

# Register it by name
app.register_singleton("database", database)

@app.get("/users")
def list_users(request, db=Depends("database")):
    return {"users": db.get_all()}
```

### Common Patterns

A typical application registers several singletons at startup:

```python
app = App()

# Database
db = Database(url="postgres://localhost/mydb")
app.register_singleton("database", db)

# Cache client
cache = RedisClient(url="redis://localhost:6379")
app.register_singleton("cache", cache)

# Configuration
config = AppConfig.from_env()
app.register_singleton("config", config)
```

---

## Dependency Scopes

Cello supports three dependency scopes, each controlling the lifetime of resolved instances:

| Scope | Lifetime | Use Case |
|-------|----------|----------|
| **Singleton** | One instance for the entire application | Database pools, configuration, clients |
| **Request** | One instance per HTTP request, cached within that request | User context, request-specific state |
| **Transient** | New instance every time the dependency is resolved | Timestamps, unique IDs, stateless utilities |

### Singleton Scope

Singleton dependencies are created once and reused across every request:

```python
# Registered once at startup
app.register_singleton("database", Database())

# Every handler gets the same Database instance
@app.get("/a")
def handler_a(request, db=Depends("database")):
    return db.query(...)

@app.get("/b")
def handler_b(request, db=Depends("database")):
    return db.query(...)  # Same instance as handler_a
```

### Request Scope

Request-scoped dependencies are created once per request. If multiple parts of the handler chain resolve the same dependency, they receive the same instance:

```python
def get_current_user(request):
    token = request.get_header("Authorization")
    return auth_service.verify(token)

# Both the guard and the handler see the same user object
@app.get("/profile")
def profile(request, user=Depends("current_user")):
    return {"name": user.name, "email": user.email}
```

### Transient Scope

Transient dependencies produce a fresh instance every time they are resolved:

```python
import uuid

def generate_request_id():
    return str(uuid.uuid4())

# Each call to Depends produces a new UUID
```

---

## Dependency Chains (Sub-Dependencies)

Dependencies can depend on other dependencies, forming a resolution chain:

```python
from cello import App, Depends

app = App()

# Level 1: Database connection
def get_db():
    return DatabaseConnection(url="postgres://localhost/mydb")

# Level 2: Repository depends on database
def get_user_repo(db=Depends("database")):
    return UserRepository(db)

# Level 3: Service depends on repository
def get_user_service(repo=Depends("user_repo")):
    return UserService(repo)

# Handler depends on service
@app.get("/users/{id}")
def get_user(request, service=Depends("user_service")):
    user_id = int(request.params["id"])
    user = service.get_user(user_id)
    return {"id": user.id, "name": user.name}
```

Cello resolves the full chain automatically: `get_db` -> `get_user_repo` -> `get_user_service` -> handler.

!!! warning "Circular Dependencies"
    Cello detects circular dependency chains and raises a `DependencyError` at resolution time. For example, if `A` depends on `B` and `B` depends on `A`, the framework will report the cycle.

---

## Common Dependency Patterns

### Database + Current User

The most common pattern combines a database connection with user authentication:

```python
from cello import App, Depends

app = App()

database = Database(url="postgres://localhost/mydb")
app.register_singleton("database", database)

def get_current_user(request):
    """Extract and verify the current user from the request."""
    token = request.get_header("Authorization", "").replace("Bearer ", "")
    if not token:
        raise ValueError("Missing authorization token")
    user = database.get_user_by_token(token)
    if not user:
        raise ValueError("Invalid token")
    return user

@app.get("/profile")
def get_profile(request, user=Depends("current_user")):
    return {
        "id": user.id,
        "name": user.name,
        "email": user.email,
    }

@app.put("/profile")
def update_profile(request, user=Depends("current_user")):
    data = request.json()
    database.update_user(user.id, data)
    return {"updated": True}
```

### Configuration Injection

```python
import os

class AppConfig:
    def __init__(self):
        self.debug = os.getenv("DEBUG", "false") == "true"
        self.api_key = os.getenv("API_KEY", "")
        self.max_upload_size = int(os.getenv("MAX_UPLOAD_MB", "10"))

config = AppConfig()
app.register_singleton("config", config)

@app.post("/upload")
def upload(request, cfg=Depends("config")):
    if request.content_length > cfg.max_upload_size * 1024 * 1024:
        return Response.json({"error": "File too large"}, status=413)
    # Process upload...
```

---

## Overriding Dependencies for Testing

Cello's DI container supports overrides, making it straightforward to swap real dependencies with test doubles:

```python
# production code
app.register_singleton("database", ProductionDatabase())

# test setup
class MockDatabase:
    def __init__(self):
        self.users = [{"id": 1, "name": "Test User"}]

    def get_all(self):
        return self.users

# Override for testing
app.register_singleton("database", MockDatabase())

# Now all handlers receive MockDatabase instead of ProductionDatabase
```

In Rust, the `DependencyContainer` provides explicit override methods:

```python
# Override a specific dependency
container.override_provider(mock_provider)

# Clear a single override
container.clear_override()

# Clear all overrides
container.clear_all_overrides()
```

---

## Error Handling

Dependency resolution can fail for several reasons. Cello raises descriptive errors:

| Error | Cause |
|-------|-------|
| `DependencyError.NotFound` | No provider registered for the requested dependency |
| `DependencyError.CircularDependency` | Two or more dependencies form a cycle |
| `DependencyError.ProviderFailed` | The provider function raised an exception |
| `DependencyError.TypeMismatch` | The resolved value does not match the expected type |

When a dependency fails, the request receives a `500 Internal Server Error` response with details in debug mode.

---

## Performance

| Operation | Overhead | Notes |
|-----------|----------|-------|
| Singleton lookup | ~50ns | Lock-free read from `DashMap` cache |
| Request-scoped lookup | ~100ns | Cached within request context |
| Transient creation | Varies | Depends on the provider function |
| Chain resolution | ~100ns per level | Each level adds one lookup |

!!! tip "Best Practices"
    - Use **Singleton** scope for database pools, HTTP clients, and configuration.
    - Use **Request** scope for user sessions and per-request state.
    - Use **Transient** scope only when every call truly needs a fresh instance.
    - Keep dependency chains shallow (3 levels or fewer) for clarity.

---

## Next Steps

- [Background Tasks](background-tasks.md) - Run tasks after the response is sent
- [Guards (RBAC)](../security/guards.md) - Combine DI with role-based access control
- [Templates](templates.md) - Inject template engines into handlers
