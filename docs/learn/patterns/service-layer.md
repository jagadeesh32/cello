---
title: Service Layer Pattern
description: Separating business logic from HTTP handlers using a service layer in Cello
---

# Service Layer Pattern

The service layer pattern separates business logic from HTTP handlers. Handlers are responsible only for parsing requests and returning responses. All domain logic, validation, and orchestration lives in service classes.

---

## Why Use a Service Layer?

- **Thin handlers** -- Handlers become simple glue between HTTP and business logic.
- **Reusability** -- The same service can be called from REST endpoints, WebSocket handlers, CLI tools, or background tasks.
- **Testability** -- Services can be unit-tested without HTTP overhead.
- **Transaction management** -- Services can coordinate database transactions across multiple repository calls.

---

## Basic Service Class

```python
# services/user_service.py
from repositories.base import UserRepository
from typing import Optional

class UserService:
    """Business logic for user operations."""

    def __init__(self, user_repo: UserRepository):
        self.user_repo = user_repo

    async def get_user(self, user_id: int) -> Optional[dict]:
        user = await self.user_repo.get_by_id(user_id)
        if not user:
            raise ValueError(f"User {user_id} not found")
        return user

    async def list_users(self) -> list:
        return await self.user_repo.get_all()

    async def create_user(self, data: dict) -> dict:
        # Business validation
        if not data.get("email"):
            raise ValueError("Email is required")
        if not data.get("name"):
            raise ValueError("Name is required")

        # Check for duplicates
        existing = await self.user_repo.get_all()
        if any(u["email"] == data["email"] for u in existing):
            raise ValueError(f"Email {data['email']} is already registered")

        return await self.user_repo.create(data)

    async def update_user(self, user_id: int, data: dict) -> dict:
        user = await self.user_repo.get_by_id(user_id)
        if not user:
            raise ValueError(f"User {user_id} not found")

        return await self.user_repo.update(user_id, data)

    async def delete_user(self, user_id: int) -> bool:
        user = await self.user_repo.get_by_id(user_id)
        if not user:
            raise ValueError(f"User {user_id} not found")

        return await self.user_repo.delete(user_id)
```

---

## Injecting Services via Depends

Register the service as a singleton and inject it into handlers.

```python
# app.py
from cello import App, Depends, Response
from services.user_service import UserService
from repositories.postgres_user_repo import PostgresUserRepository

app = App()

# Wire up dependencies
user_repo = PostgresUserRepository(db=get_database())
user_service = UserService(user_repo)
app.register_singleton("user_service", user_service)

@app.get("/users")
async def list_users(request, svc=Depends("user_service")):
    users = await svc.list_users()
    return {"users": users}

@app.get("/users/{id}")
async def get_user(request, svc=Depends("user_service")):
    try:
        user = await svc.get_user(int(request.params["id"]))
        return user
    except ValueError as e:
        return Response.json({"error": str(e)}, status=404)

@app.post("/users")
async def create_user(request, svc=Depends("user_service")):
    try:
        data = request.json()
        user = await svc.create_user(data)
        return Response.json(user, status=201)
    except ValueError as e:
        return Response.json({"error": str(e)}, status=400)
```

!!! note
    Handlers catch `ValueError` from the service and translate it into the appropriate HTTP status code. The service itself knows nothing about HTTP.

---

## Transaction Management

When a service method needs to update multiple repositories atomically, wrap the calls in a transaction.

```python
# services/order_service.py
class OrderService:
    def __init__(self, order_repo, inventory_repo, db):
        self.order_repo = order_repo
        self.inventory_repo = inventory_repo
        self.db = db

    async def place_order(self, user_id: int, items: list) -> dict:
        async with self.db.transaction() as tx:
            # Reserve inventory
            for item in items:
                available = await self.inventory_repo.check_stock(item["product_id"], tx=tx)
                if available < item["quantity"]:
                    raise ValueError(f"Insufficient stock for {item['product_id']}")
                await self.inventory_repo.decrement(item["product_id"], item["quantity"], tx=tx)

            # Create the order
            order = await self.order_repo.create(
                {"user_id": user_id, "items": items, "status": "confirmed"},
                tx=tx,
            )

        return order
```

If any step fails, the transaction rolls back and inventory is not decremented.

---

## Composing Services

Services can depend on other services for cross-domain operations.

```python
class NotificationService:
    async def send_welcome_email(self, email: str, name: str):
        # ... send email ...
        pass

class UserService:
    def __init__(self, user_repo, notification_service: NotificationService):
        self.user_repo = user_repo
        self.notifications = notification_service

    async def create_user(self, data: dict) -> dict:
        user = await self.user_repo.create(data)
        await self.notifications.send_welcome_email(user["email"], user["name"])
        return user
```

---

## Testing Services

Test services with mock repositories -- no database or HTTP server needed.

```python
import pytest
from services.user_service import UserService
from repositories.memory_user_repo import InMemoryUserRepository

@pytest.fixture
def user_service():
    repo = InMemoryUserRepository()
    return UserService(repo)

@pytest.mark.asyncio
async def test_create_user(user_service):
    user = await user_service.create_user({"name": "Alice", "email": "alice@example.com"})
    assert user["name"] == "Alice"

@pytest.mark.asyncio
async def test_create_duplicate_email(user_service):
    await user_service.create_user({"name": "Alice", "email": "a@b.com"})
    with pytest.raises(ValueError, match="already registered"):
        await user_service.create_user({"name": "Bob", "email": "a@b.com"})

@pytest.mark.asyncio
async def test_get_nonexistent(user_service):
    with pytest.raises(ValueError, match="not found"):
        await user_service.get_user(999)
```

---

## Project Structure

```
myproject/
├── app.py                 # Wires services and registers routes
├── services/
│   ├── __init__.py
│   ├── user_service.py    # User business logic
│   ├── order_service.py   # Order business logic
│   └── notification.py    # Cross-cutting notification logic
├── repositories/
│   ├── base.py            # Abstract interfaces
│   ├── postgres_user_repo.py
│   └── memory_user_repo.py
└── tests/
    ├── test_user_service.py
    └── test_order_service.py
```

---

## Summary

| Layer | Responsibility |
|-------|---------------|
| **Handler** | Parse HTTP input, call service, return HTTP response |
| **Service** | Business logic, validation, orchestration |
| **Repository** | Data access (database queries) |

The service layer is the heart of your application. Keep it free of HTTP and framework concerns so it remains portable and testable.
