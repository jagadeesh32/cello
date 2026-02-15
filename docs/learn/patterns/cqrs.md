---
title: CQRS Pattern
description: Separating reads and writes with Command Query Responsibility Segregation in Cello
---

# CQRS Pattern

Command Query Responsibility Segregation (CQRS) separates read operations (queries) from write operations (commands) into distinct models. This allows you to optimize each path independently and scale reads and writes separately.

---

## When to Use CQRS

CQRS adds architectural complexity. Use it when:

- Read and write workloads have very different scaling requirements.
- The read model needs to be denormalized for performance (e.g., materialized views).
- You need different authorization rules for reads vs. writes.
- You are using event sourcing and need to project events into read models.

For simple CRUD applications, the [service layer pattern](service-layer.md) is usually sufficient.

---

## Core Concepts

| Term | Description |
|------|-------------|
| **Command** | A request to change state (create, update, delete) |
| **Query** | A request to read state (no side effects) |
| **CommandBus** | Dispatches commands to the appropriate handler |
| **QueryBus** | Dispatches queries to the appropriate handler |

---

## Defining Commands and Queries

```python
# cqrs/messages.py
from dataclasses import dataclass
from typing import Any

@dataclass
class Command:
    """Base class for all commands."""
    pass

@dataclass
class Query:
    """Base class for all queries."""
    pass

@dataclass
class CreateUser(Command):
    name: str
    email: str

@dataclass
class UpdateUserName(Command):
    user_id: int
    new_name: str

@dataclass
class GetUserById(Query):
    user_id: int

@dataclass
class ListUsers(Query):
    page: int = 1
    page_size: int = 20
```

---

## Command Bus

The command bus routes each command to its registered handler.

```python
# cqrs/bus.py
from typing import Callable, Dict, Type

class CommandBus:
    def __init__(self):
        self._handlers: Dict[Type, Callable] = {}

    def register(self, command_type: Type, handler: Callable):
        self._handlers[command_type] = handler

    async def dispatch(self, command) -> any:
        handler = self._handlers.get(type(command))
        if not handler:
            raise ValueError(f"No handler registered for {type(command).__name__}")
        return await handler(command)

class QueryBus:
    def __init__(self):
        self._handlers: Dict[Type, Callable] = {}

    def register(self, query_type: Type, handler: Callable):
        self._handlers[query_type] = handler

    async def dispatch(self, query) -> any:
        handler = self._handlers.get(type(query))
        if not handler:
            raise ValueError(f"No handler registered for {type(query).__name__}")
        return await handler(query)
```

---

## Command Handlers (Write Side)

Command handlers contain the write logic and business rules.

```python
# cqrs/handlers/user_commands.py

async def handle_create_user(cmd: CreateUser) -> dict:
    """Validate and persist a new user."""
    if not cmd.email:
        raise ValueError("Email is required")

    user = await write_db.execute(
        "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id, name, email",
        cmd.name, cmd.email,
    )
    return dict(user)

async def handle_update_name(cmd: UpdateUserName) -> dict:
    """Update a user's name."""
    user = await write_db.execute(
        "UPDATE users SET name = $1 WHERE id = $2 RETURNING id, name, email",
        cmd.new_name, cmd.user_id,
    )
    if not user:
        raise ValueError(f"User {cmd.user_id} not found")
    return dict(user)
```

---

## Query Handlers (Read Side)

Query handlers read from an optimized read store (which may be a different database or a materialized view).

```python
# cqrs/handlers/user_queries.py

async def handle_get_user(query: GetUserById) -> dict:
    """Fetch a single user from the read model."""
    user = await read_db.fetch_one(
        "SELECT id, name, email, created_at FROM users_view WHERE id = $1",
        query.user_id,
    )
    if not user:
        raise ValueError(f"User {query.user_id} not found")
    return dict(user)

async def handle_list_users(query: ListUsers) -> list:
    """Fetch a paginated list of users from the read model."""
    offset = (query.page - 1) * query.page_size
    rows = await read_db.fetch_all(
        "SELECT id, name, email FROM users_view ORDER BY id LIMIT $1 OFFSET $2",
        query.page_size, offset,
    )
    return [dict(r) for r in rows]
```

---

## Wiring It All Together

```python
# app.py
from cello import App, Depends, Response
from cqrs.bus import CommandBus, QueryBus
from cqrs.messages import CreateUser, GetUserById, ListUsers
from cqrs.handlers.user_commands import handle_create_user
from cqrs.handlers.user_queries import handle_get_user, handle_list_users

app = App()

# Create and configure buses
command_bus = CommandBus()
command_bus.register(CreateUser, handle_create_user)

query_bus = QueryBus()
query_bus.register(GetUserById, handle_get_user)
query_bus.register(ListUsers, handle_list_users)

app.register_singleton("command_bus", command_bus)
app.register_singleton("query_bus", query_bus)

# Write endpoint
@app.post("/users")
async def create_user(request, bus=Depends("command_bus")):
    data = request.json()
    try:
        user = await bus.dispatch(CreateUser(name=data["name"], email=data["email"]))
        return Response.json(user, status=201)
    except ValueError as e:
        return Response.json({"error": str(e)}, status=400)

# Read endpoints
@app.get("/users/{id}")
async def get_user(request, bus=Depends("query_bus")):
    try:
        user = await bus.dispatch(GetUserById(user_id=int(request.params["id"])))
        return user
    except ValueError as e:
        return Response.json({"error": str(e)}, status=404)

@app.get("/users")
async def list_users(request, bus=Depends("query_bus")):
    page = int(request.query.get("page", "1"))
    users = await bus.dispatch(ListUsers(page=page))
    return {"users": users}
```

---

## Benefits and Tradeoffs

### Benefits

| Benefit | Description |
|---------|-------------|
| Independent scaling | Scale the read side (e.g., replicas, caches) separately from writes |
| Optimized models | Denormalize the read model for fast queries |
| Clear boundaries | Business rules live in command handlers; query handlers are pure reads |
| Audit trail | Combine with event sourcing for full change history |

### Tradeoffs

| Tradeoff | Description |
|----------|-------------|
| Complexity | More code and more moving parts than simple CRUD |
| Eventual consistency | The read model may lag behind the write model |
| Synchronization | You need a mechanism (events, CDC) to keep models in sync |

---

## CQRS with Event Sourcing

Combine CQRS with the [event-driven pattern](event-driven.md) to keep the read model synchronized via events.

```
Command --> Command Handler --> Write DB --> Publish Event
                                                  |
                                                  v
Query <-- Query Handler <-- Read DB <-- Event Subscriber (updates read model)
```

The event subscriber listens for domain events and updates the read-side projections.

---

## Project Structure

```
myproject/
├── app.py
├── cqrs/
│   ├── __init__.py
│   ├── bus.py              # CommandBus, QueryBus
│   ├── messages.py          # Command and Query dataclasses
│   └── handlers/
│       ├── user_commands.py # Write handlers
│       └── user_queries.py  # Read handlers
├── services/
└── tests/
    ├── test_commands.py
    └── test_queries.py
```

---

## Next Steps

- See the [Event-Driven pattern](event-driven.md) for publishing domain events from command handlers.
- See the [Repository pattern](repository.md) for abstracting data access inside handlers.
- See the [Service Layer pattern](service-layer.md) as a simpler alternative for CRUD applications.
