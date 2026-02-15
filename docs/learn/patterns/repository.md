---
title: Repository Pattern
description: Abstracting data access with the repository pattern in Cello applications
---

# Repository Pattern

The repository pattern provides an abstraction layer between your business logic and data storage. It decouples handlers and services from specific database implementations, making your code easier to test and change.

---

## Why Use Repositories?

- **Testability** -- Replace the real repository with a mock during tests.
- **Flexibility** -- Switch from PostgreSQL to DynamoDB without changing business logic.
- **Single Responsibility** -- Data access logic lives in one place, not scattered across handlers.

---

## Defining a Repository Interface

Use Python's abstract base classes to define the contract.

```python
# repositories/base.py
from abc import ABC, abstractmethod
from typing import List, Optional

class UserRepository(ABC):
    """Interface for user data access."""

    @abstractmethod
    async def get_by_id(self, user_id: int) -> Optional[dict]:
        ...

    @abstractmethod
    async def get_all(self) -> List[dict]:
        ...

    @abstractmethod
    async def create(self, data: dict) -> dict:
        ...

    @abstractmethod
    async def update(self, user_id: int, data: dict) -> Optional[dict]:
        ...

    @abstractmethod
    async def delete(self, user_id: int) -> bool:
        ...
```

---

## Database Implementation

Implement the interface for your specific data store.

```python
# repositories/postgres_user_repo.py
from repositories.base import UserRepository
from typing import List, Optional

class PostgresUserRepository(UserRepository):
    def __init__(self, db):
        self.db = db

    async def get_by_id(self, user_id: int) -> Optional[dict]:
        row = await self.db.fetch_one(
            "SELECT id, name, email FROM users WHERE id = $1", user_id
        )
        return dict(row) if row else None

    async def get_all(self) -> List[dict]:
        rows = await self.db.fetch_all("SELECT id, name, email FROM users")
        return [dict(r) for r in rows]

    async def create(self, data: dict) -> dict:
        row = await self.db.fetch_one(
            "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id, name, email",
            data["name"], data["email"],
        )
        return dict(row)

    async def update(self, user_id: int, data: dict) -> Optional[dict]:
        row = await self.db.fetch_one(
            "UPDATE users SET name = $1, email = $2 WHERE id = $3 RETURNING id, name, email",
            data.get("name"), data.get("email"), user_id,
        )
        return dict(row) if row else None

    async def delete(self, user_id: int) -> bool:
        result = await self.db.execute(
            "DELETE FROM users WHERE id = $1", user_id
        )
        return result > 0
```

---

## In-Memory Implementation (for Development)

```python
# repositories/memory_user_repo.py
from repositories.base import UserRepository
from typing import List, Optional

class InMemoryUserRepository(UserRepository):
    def __init__(self):
        self._store: dict[int, dict] = {}
        self._next_id = 1

    async def get_by_id(self, user_id: int) -> Optional[dict]:
        return self._store.get(user_id)

    async def get_all(self) -> List[dict]:
        return list(self._store.values())

    async def create(self, data: dict) -> dict:
        user = {"id": self._next_id, **data}
        self._store[self._next_id] = user
        self._next_id += 1
        return user

    async def update(self, user_id: int, data: dict) -> Optional[dict]:
        if user_id not in self._store:
            return None
        self._store[user_id].update(data)
        return self._store[user_id]

    async def delete(self, user_id: int) -> bool:
        return self._store.pop(user_id, None) is not None
```

---

## Injecting Repositories with Depends

Register the repository as a singleton and inject it into handlers.

```python
# app.py
from cello import App, Depends, Response
from repositories.postgres_user_repo import PostgresUserRepository

app = App()

# Create and register the repository
user_repo = PostgresUserRepository(db=get_database())
app.register_singleton("user_repo", user_repo)

@app.get("/users")
async def list_users(request, repo=Depends("user_repo")):
    users = await repo.get_all()
    return {"users": users}

@app.get("/users/{id}")
async def get_user(request, repo=Depends("user_repo")):
    user = await repo.get_by_id(int(request.params["id"]))
    if not user:
        return Response.json({"error": "Not found"}, status=404)
    return user

@app.post("/users")
async def create_user(request, repo=Depends("user_repo")):
    data = request.json()
    user = await repo.create(data)
    return Response.json(user, status=201)
```

---

## Testing with Mock Repositories

In tests, swap the real repository for the in-memory implementation.

```python
# tests/conftest.py
import pytest
from repositories.memory_user_repo import InMemoryUserRepository

@pytest.fixture
def user_repo():
    return InMemoryUserRepository()

@pytest.mark.asyncio
async def test_create_user(user_repo):
    user = await user_repo.create({"name": "Alice", "email": "alice@example.com"})
    assert user["id"] == 1
    assert user["name"] == "Alice"

@pytest.mark.asyncio
async def test_get_nonexistent_user(user_repo):
    result = await user_repo.get_by_id(999)
    assert result is None
```

This approach tests your business logic without needing a running database.

---

## Project Structure

```
myproject/
├── app.py
├── repositories/
│   ├── __init__.py
│   ├── base.py                  # Abstract interfaces
│   ├── postgres_user_repo.py    # Production implementation
│   └── memory_user_repo.py      # Test/dev implementation
├── services/
│   └── user_service.py          # Business logic (uses repository)
└── tests/
    └── test_users.py
```

---

## Summary

| Component | Responsibility |
|-----------|---------------|
| **Repository interface** (`base.py`) | Defines the data access contract |
| **Database repository** | Implements the contract for a real database |
| **In-memory repository** | Implements the contract for testing |
| **Depends** | Injects the repository into handlers and services |
| **Service layer** | Contains business logic, receives repository via DI |

See also: [Service Layer pattern](service-layer.md) for organizing business logic on top of repositories.
