---
title: Database Integration
description: Async database queries with connection pooling
tags:
  - Database
  - PostgreSQL
  - Async
  - Connection Pool
  - CRUD
  - Examples
---

# :material-database: Database Integration

Connect your Cello application to a PostgreSQL database using an async connection pool. This example covers pool setup and teardown, full CRUD endpoints for a `products` resource, and robust error handling so database failures are surfaced cleanly to API clients.

## Complete Example

```python
import asyncpg
from cello import Cello, Request, Response

app = Cello()

# -------------------------------------------------------------------
# Connection pool — created once on startup, shared across requests
# -------------------------------------------------------------------
DB_URL = "postgresql://user:password@localhost:5432/mydb"

pool: asyncpg.Pool | None = None


@app.on_startup
async def startup():
    global pool
    pool = await asyncpg.create_pool(
        dsn=DB_URL,
        min_size=2,       # Keep at least 2 connections warm
        max_size=10,      # Never exceed 10 simultaneous connections
        command_timeout=30,
    )
    # Ensure the products table exists
    async with pool.acquire() as conn:
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS products (
                id      SERIAL PRIMARY KEY,
                name    TEXT    NOT NULL,
                price   NUMERIC(10, 2) NOT NULL,
                stock   INTEGER NOT NULL DEFAULT 0,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
        """)


@app.on_shutdown
async def shutdown():
    if pool:
        await pool.close()


# -------------------------------------------------------------------
# Helper
# -------------------------------------------------------------------
def row_to_dict(record: asyncpg.Record) -> dict:
    """Convert an asyncpg Record to a plain dict (JSON-serialisable)."""
    return {
        "id": record["id"],
        "name": record["name"],
        "price": float(record["price"]),
        "stock": record["stock"],
        "created_at": record["created_at"].isoformat(),
    }


# -------------------------------------------------------------------
# GET /products  — list all products
# -------------------------------------------------------------------
@app.get("/products")
async def list_products(request: Request):
    try:
        async with pool.acquire() as conn:
            rows = await conn.fetch(
                "SELECT id, name, price, stock, created_at FROM products ORDER BY id"
            )
        return Response.json([row_to_dict(r) for r in rows])
    except Exception as exc:
        return Response.json(
            {"error": "Failed to fetch products", "detail": str(exc)},
            status=500,
        )


# -------------------------------------------------------------------
# POST /products  — create a new product
# -------------------------------------------------------------------
@app.post("/products")
async def create_product(request: Request):
    body = await request.json()

    name = body.get("name")
    price = body.get("price")
    stock = body.get("stock", 0)

    if not name or price is None:
        return Response.json(
            {"error": "Both 'name' and 'price' are required"},
            status=400,
        )

    try:
        price = float(price)
        stock = int(stock)
    except (TypeError, ValueError):
        return Response.json(
            {"error": "'price' must be a number and 'stock' must be an integer"},
            status=400,
        )

    try:
        async with pool.acquire() as conn:
            row = await conn.fetchrow(
                """
                INSERT INTO products (name, price, stock)
                VALUES ($1, $2, $3)
                RETURNING id, name, price, stock, created_at
                """,
                name, price, stock,
            )
        return Response.json(row_to_dict(row), status=201)
    except Exception as exc:
        return Response.json(
            {"error": "Failed to create product", "detail": str(exc)},
            status=500,
        )


# -------------------------------------------------------------------
# DELETE /products/{id}  — remove a product by primary key
# -------------------------------------------------------------------
@app.delete("/products/{id}")
async def delete_product(request: Request, id: int):
    try:
        async with pool.acquire() as conn:
            result = await conn.execute(
                "DELETE FROM products WHERE id = $1", id
            )
        # asyncpg returns "DELETE <count>" as the status string
        deleted_count = int(result.split()[-1])
        if deleted_count == 0:
            return Response.json(
                {"error": f"Product {id} not found"},
                status=404,
            )
        return Response.json({"message": f"Product {id} deleted"})
    except Exception as exc:
        return Response.json(
            {"error": "Failed to delete product", "detail": str(exc)},
            status=500,
        )


# -------------------------------------------------------------------
# Entry point
# -------------------------------------------------------------------
if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **`asyncpg.create_pool`** — creates an async connection pool with configurable `min_size` and `max_size`. The pool is shared across all requests, avoiding the overhead of opening a new connection per request.
- **`@app.on_startup` / `@app.on_shutdown`** — lifecycle hooks that run once when the server starts and stops, perfect for initialising (and cleanly closing) the pool.
- **`pool.acquire()`** — borrows a connection from the pool for the duration of the `async with` block, then returns it automatically. The pool handles queuing if all connections are in use.
- **Parameterised queries (`$1`, `$2`, …)** — asyncpg uses positional placeholders. Passing values as arguments (never via string formatting) prevents SQL injection.
- **`RETURNING` clause** — lets `INSERT` hand back the newly created row in one round-trip, avoiding a second `SELECT`.
- **Typed error responses** — each endpoint wraps its database call in a `try/except` and returns a structured JSON error with an appropriate HTTP status code (`400`, `404`, or `500`).

## Running This Example

```bash
# Install dependencies
pip install cello asyncpg

# Ensure PostgreSQL is running and update DB_URL in the script, then:
python examples/basic/database.py
```

Try the endpoints:

```bash
# Create a product
curl -s -X POST http://localhost:8000/products \
  -H "Content-Type: application/json" \
  -d '{"name": "Widget", "price": 9.99, "stock": 42}' | jq .

# List all products
curl -s http://localhost:8000/products | jq .

# Delete a product (replace 1 with the actual id)
curl -s -X DELETE http://localhost:8000/products/1 | jq .
```
