---
title: Simple API with OpenAPI Docs
description: A full CRUD REST API with CORS support, Swagger UI, and ReDoc, powered by a manually defined OpenAPI 3.0 specification.
---

# :material-api: Simple API with OpenAPI Docs

This example builds a complete, production-shaped REST API for two resources — **users** and **items** — and wires it up with automatic interactive documentation. It demonstrates CORS configuration, full CRUD route definitions, and how to attach a hand-authored OpenAPI 3.0 spec so that Swagger UI (`/docs`) and ReDoc (`/redoc`) are served out of the box.

## Features Demonstrated

- Full CRUD routes: `GET`, `POST`, `PUT`, and `DELETE` for `/users` and `/items`
- CORS middleware configuration
- Manually defined OpenAPI 3.0 specification passed to the `App`
- Swagger UI served at `/docs`
- ReDoc served at `/redoc`
- In-memory data store simulating a real database layer

## Complete Source Code

```python
from cello import App, Response

# ---------------------------------------------------------------------------
# In-memory "database"
# ---------------------------------------------------------------------------
users_db: dict[int, dict] = {}
items_db: dict[int, dict] = {}
next_user_id = 1
next_item_id = 1

# ---------------------------------------------------------------------------
# OpenAPI 3.0 specification (abbreviated — full spec defined in source file)
# ---------------------------------------------------------------------------
openapi_spec = {
    "openapi": "3.0.0",
    "info": {
        "title": "Simple Cello API",
        "version": "1.0.0",
        "description": "A simple REST API built with Cello",
    },
    "paths": {
        "/users": {
            "get":  {"summary": "List all users",   "responses": {"200": {"description": "OK"}}},
            "post": {"summary": "Create a user",    "responses": {"201": {"description": "Created"}}},
        },
        "/users/{id}": {
            "get":    {"summary": "Get a user",     "responses": {"200": {"description": "OK"}, "404": {"description": "Not found"}}},
            "put":    {"summary": "Update a user",  "responses": {"200": {"description": "OK"}, "404": {"description": "Not found"}}},
            "delete": {"summary": "Delete a user",  "responses": {"204": {"description": "No content"}, "404": {"description": "Not found"}}},
        },
        "/items": {
            "get":  {"summary": "List all items",   "responses": {"200": {"description": "OK"}}},
            "post": {"summary": "Create an item",   "responses": {"201": {"description": "Created"}}},
        },
        "/items/{id}": {
            "get":    {"summary": "Get an item",    "responses": {"200": {"description": "OK"}, "404": {"description": "Not found"}}},
            "put":    {"summary": "Update an item", "responses": {"200": {"description": "OK"}, "404": {"description": "Not found"}}},
            "delete": {"summary": "Delete an item", "responses": {"204": {"description": "No content"}, "404": {"description": "Not found"}}},
        },
    },
}

app = App(openapi=openapi_spec)

# ---------------------------------------------------------------------------
# CORS
# ---------------------------------------------------------------------------
app.add_middleware(
    "cors",
    allow_origins=["*"],
    allow_methods=["GET", "POST", "PUT", "DELETE"],
    allow_headers=["Content-Type", "Authorization"],
)

# ---------------------------------------------------------------------------
# User routes
# ---------------------------------------------------------------------------

@app.get("/users")
def list_users(request):
    return {"users": list(users_db.values()), "total": len(users_db)}


@app.post("/users")
def create_user(request):
    global next_user_id
    data = request.json()
    user = {
        "id": next_user_id,
        "name": data.get("name", "Anonymous"),
        "email": data.get("email", ""),
        "created_at": "2024-01-01T00:00:00Z",
    }
    users_db[next_user_id] = user
    next_user_id += 1
    return Response(user, status_code=201)


@app.get("/users/{id}")
def get_user(request):
    user_id = int(request.params["id"])
    if user_id not in users_db:
        return Response({"error": "User not found"}, status_code=404)
    return users_db[user_id]


@app.put("/users/{id}")
def update_user(request):
    user_id = int(request.params["id"])
    if user_id not in users_db:
        return Response({"error": "User not found"}, status_code=404)
    data = request.json()
    users_db[user_id].update({k: v for k, v in data.items() if k != "id"})
    return users_db[user_id]


@app.delete("/users/{id}")
def delete_user(request):
    user_id = int(request.params["id"])
    if user_id not in users_db:
        return Response({"error": "User not found"}, status_code=404)
    del users_db[user_id]
    return Response(status_code=204)

# ---------------------------------------------------------------------------
# Item routes
# ---------------------------------------------------------------------------

@app.get("/items")
def list_items(request):
    return {"items": list(items_db.values()), "total": len(items_db)}


@app.post("/items")
def create_item(request):
    global next_item_id
    data = request.json()
    item = {
        "id": next_item_id,
        "name": data.get("name", "Unnamed"),
        "price": data.get("price", 0.0),
        "in_stock": data.get("in_stock", True),
    }
    items_db[next_item_id] = item
    next_item_id += 1
    return Response(item, status_code=201)


@app.get("/items/{id}")
def get_item(request):
    item_id = int(request.params["id"])
    if item_id not in items_db:
        return Response({"error": "Item not found"}, status_code=404)
    return items_db[item_id]


@app.put("/items/{id}")
def update_item(request):
    item_id = int(request.params["id"])
    if item_id not in items_db:
        return Response({"error": "Item not found"}, status_code=404)
    data = request.json()
    items_db[item_id].update({k: v for k, v in data.items() if k != "id"})
    return items_db[item_id]


@app.delete("/items/{id}")
def delete_item(request):
    item_id = int(request.params["id"])
    if item_id not in items_db:
        return Response({"error": "Item not found"}, status_code=404)
    del items_db[item_id]
    return Response(status_code=204)

# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------

@app.get("/health")
def health(request):
    return {"status": "ok", "users": len(users_db), "items": len(items_db)}


if __name__ == "__main__":
    app.run(port=8080)
```

## Running This Example

```bash
python examples/simple_api.py
# then test it:
curl http://127.0.0.1:8080/health
curl http://127.0.0.1:8080/users
curl -X POST http://127.0.0.1:8080/users \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "email": "alice@example.com"}'
curl http://127.0.0.1:8080/users/1
curl -X PUT http://127.0.0.1:8080/users/1 \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice Smith"}'
curl -X DELETE http://127.0.0.1:8080/users/1
# open interactive docs:
# http://127.0.0.1:8080/docs   (Swagger UI)
# http://127.0.0.1:8080/redoc  (ReDoc)
```

## Key Concepts

- **OpenAPI spec injection** — Passing an `openapi=` dict to `App()` enables automatic Swagger UI and ReDoc endpoints with no extra setup; the spec is served at `/openapi.json`.
- **`Response` with status codes** — Returning `Response(body, status_code=201)` gives precise control over HTTP status, headers, and body, beyond what a plain dict return allows.
- **CORS middleware** — `app.add_middleware("cors", ...)` inserts the CORS layer globally, adding the appropriate `Access-Control-*` headers to every response.
- **In-memory store pattern** — Module-level dicts (`users_db`, `items_db`) and counters (`next_user_id`) provide a zero-dependency data layer that is straightforward to swap out for a real database driver.
