---
title: Testing
description: Testing Cello applications with pytest, requests, and async tests
---

# Testing

This guide covers strategies and tools for testing Cello applications, from simple route tests to full integration suites.

---

## Setup

Install the testing dependencies:

```bash
pip install pytest requests pytest-asyncio
```

---

## Project Layout

```
myproject/
├── app.py
├── tests/
│   ├── conftest.py       # Shared fixtures
│   ├── test_users.py     # Route tests
│   └── test_services.py  # Unit tests
```

---

## Starting the Server for Tests

Cello applications run their own HTTP server, so integration tests send real HTTP requests. Use a fixture to start the server in a background thread.

### conftest.py

```python
import pytest
import threading
import time
from app import app

BASE_URL = "http://127.0.0.1:9000"

@pytest.fixture(scope="session", autouse=True)
def start_server():
    """Start the Cello server in a background thread."""
    server_thread = threading.Thread(
        target=lambda: app.run(host="127.0.0.1", port=9000, logs=False),
        daemon=True,
    )
    server_thread.start()
    time.sleep(1)  # Wait for the server to be ready
    yield
    # Server thread is a daemon; it stops when the process exits
```

!!! tip
    Use a dedicated port (e.g., `9000`) for tests to avoid collisions with a development server running on `8000`.

---

## Testing Routes

### Basic GET

```python
import requests

BASE_URL = "http://127.0.0.1:9000"

def test_list_books():
    resp = requests.get(f"{BASE_URL}/books")
    assert resp.status_code == 200
    data = resp.json()
    assert "books" in data
    assert isinstance(data["books"], list)
```

### POST with JSON Body

```python
def test_create_book():
    payload = {"title": "Dune", "author": "Frank Herbert"}
    resp = requests.post(f"{BASE_URL}/books", json=payload)
    assert resp.status_code == 201
    data = resp.json()
    assert data["title"] == "Dune"
    assert "id" in data
```

### Path Parameters

```python
def test_get_book_not_found():
    resp = requests.get(f"{BASE_URL}/books/99999")
    assert resp.status_code == 404
```

### Testing with Headers

```python
def test_protected_route():
    # Without auth
    resp = requests.get(f"{BASE_URL}/me")
    assert resp.status_code == 401

    # With auth
    token = get_test_token()
    resp = requests.get(
        f"{BASE_URL}/me",
        headers={"Authorization": f"Bearer {token}"},
    )
    assert resp.status_code == 200
```

---

## Mocking Dependencies

When testing handlers that rely on external services, replace the dependency with a mock.

### Using monkeypatch

```python
def test_create_order_user_not_found(monkeypatch):
    """Order creation should fail if User Service is unavailable."""
    import order_service

    def mock_fetch_user(user_id):
        return None

    monkeypatch.setattr(order_service, "fetch_user", mock_fetch_user)

    resp = requests.post(
        f"{BASE_URL}/orders",
        json={"user_id": 1, "items": ["Book"]},
    )
    assert resp.status_code == 400
```

### Using unittest.mock

```python
from unittest.mock import patch, MagicMock

def test_service_called():
    with patch("services.user_service.get_user") as mock_get:
        mock_get.return_value = {"id": 1, "name": "Alice"}
        resp = requests.get(f"{BASE_URL}/users/1")
        assert resp.status_code == 200
        mock_get.assert_called_once_with("1")
```

---

## Async Tests

If you use `pytest-asyncio`, you can test async service functions directly without going through HTTP.

```python
import pytest

@pytest.mark.asyncio
async def test_fetch_users():
    from services.user_service import fetch_users

    users = await fetch_users()
    assert isinstance(users, list)
```

### Async Fixtures

```python
@pytest.fixture
async def db_connection():
    conn = await create_test_connection()
    yield conn
    await conn.close()

@pytest.mark.asyncio
async def test_query(db_connection):
    result = await db_connection.fetch("SELECT 1 as n")
    assert result[0]["n"] == 1
```

---

## Test Fixtures

### Resetting State Between Tests

If your application uses in-memory stores, reset them between tests.

```python
@pytest.fixture(autouse=True)
def reset_state():
    """Clear the in-memory store before each test."""
    import app as app_module
    app_module.books.clear()
    app_module.next_id = 1
    yield
```

### Factory Fixtures

Create test data using factory functions.

```python
@pytest.fixture
def sample_book():
    resp = requests.post(
        f"{BASE_URL}/books",
        json={"title": "Test Book", "author": "Test Author"},
    )
    return resp.json()

def test_update_book(sample_book):
    book_id = sample_book["id"]
    resp = requests.put(
        f"{BASE_URL}/books/{book_id}",
        json={"title": "Updated Title"},
    )
    assert resp.status_code == 200
    assert resp.json()["title"] == "Updated Title"
```

---

## Integration Tests

Integration tests exercise the full stack: HTTP layer, middleware, routing, and handler logic.

```python
class TestUserWorkflow:
    """Test the complete user lifecycle."""

    def test_register_login_profile(self):
        # Register
        resp = requests.post(f"{BASE_URL}/auth/register", json={
            "email": "test@example.com",
            "password": "password123",
            "name": "Test",
        })
        assert resp.status_code == 201

        # Login
        resp = requests.post(f"{BASE_URL}/auth/login", json={
            "email": "test@example.com",
            "password": "password123",
        })
        assert resp.status_code == 200
        token = resp.json()["access_token"]

        # Profile
        resp = requests.get(
            f"{BASE_URL}/me",
            headers={"Authorization": f"Bearer {token}"},
        )
        assert resp.status_code == 200
        assert resp.json()["email"] == "test@example.com"
```

---

## Running Tests

```bash
# Run all tests
pytest tests/ -v

# Run a specific file
pytest tests/test_users.py -v

# Run with coverage
pip install pytest-cov
pytest tests/ --cov=app --cov-report=term-missing

# Stop on first failure
pytest tests/ -x
```

---

## Tips

| Tip | Details |
|-----|---------|
| Use `scope="session"` for server fixtures | Avoids restarting the server for every test |
| Assign a unique test port | Prevents conflicts with development servers |
| Test error cases | Always verify 400, 401, 404, and 500 responses |
| Use `pytest -x` during development | Stops at the first failure for faster feedback |
| Add `--tb=short` for cleaner output | Reduces traceback noise in CI logs |
