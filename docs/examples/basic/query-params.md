---
title: Query Parameters & Validation
description: Parse and validate URL query parameters
tags:
  - Query Parameters
  - REST API
  - Validation
  - HTTP
  - Examples
---

# :material-filter-check: Query Parameters & Validation

Query parameters let clients control filtering, pagination, and search behaviour without changing the URL path. This example shows how to read parameters from the request, coerce them to the correct Python types, apply sensible defaults, and return a clear `400 Bad Request` when values are invalid.

## Complete Example

```python
from cello import Cello, Request, Response

app = Cello()

# -------------------------------------------------------------------
# Constants
# -------------------------------------------------------------------
MAX_LIMIT = 100
DEFAULT_PAGE = 1
DEFAULT_LIMIT = 10

# Simulated product catalogue (replace with a real DB query in practice)
PRODUCTS = [
    {"id": i, "name": f"Product {i}", "category": "widgets" if i % 2 == 0 else "gadgets"}
    for i in range(1, 51)
]


# -------------------------------------------------------------------
# Helper — coerce a raw string query-param value to int
# -------------------------------------------------------------------
def parse_int(value: str | None, default: int, name: str, min_val: int = 1) -> tuple[int, str | None]:
    """
    Returns (parsed_int, error_message).
    error_message is None when parsing succeeds.
    """
    if value is None:
        return default, None
    try:
        result = int(value)
    except ValueError:
        return default, f"'{name}' must be an integer, got: {value!r}"
    if result < min_val:
        return default, f"'{name}' must be >= {min_val}, got: {result}"
    return result, None


# -------------------------------------------------------------------
# GET /search
#
#   Query params:
#     q      (str, optional)  — keyword to filter product names
#     page   (int, default=1) — 1-based page number
#     limit  (int, default=10, max=100) — results per page
# -------------------------------------------------------------------
@app.get("/search")
async def search_products(request: Request):
    params = request.query_params

    # --- Read raw values -----------------------------------------------
    q = params.get("q", "").strip()             # optional keyword filter
    raw_page = params.get("page")               # may be None or any string
    raw_limit = params.get("limit")

    # --- Type coercion with validation ---------------------------------
    page, page_err = parse_int(raw_page, default=DEFAULT_PAGE, name="page")
    limit, limit_err = parse_int(raw_limit, default=DEFAULT_LIMIT, name="limit")

    errors = [e for e in (page_err, limit_err) if e]
    if errors:
        return Response.json(
            {"error": "Invalid query parameters", "details": errors},
            status=400,
        )

    # Cap limit to avoid runaway queries
    if limit > MAX_LIMIT:
        return Response.json(
            {"error": f"'limit' must not exceed {MAX_LIMIT}, got: {limit}"},
            status=400,
        )

    # --- Filter --------------------------------------------------------
    results = PRODUCTS
    if q:
        results = [p for p in results if q.lower() in p["name"].lower()]

    # --- Paginate ------------------------------------------------------
    total = len(results)
    start = (page - 1) * limit
    end = start + limit
    page_results = results[start:end]

    # --- Build response ------------------------------------------------
    return Response.json({
        "query": q or None,
        "page": page,
        "limit": limit,
        "total": total,
        "total_pages": max(1, -(-total // limit)),  # ceiling division
        "results": page_results,
    })


# -------------------------------------------------------------------
# GET /products/{id}  — single item; id comes from the path, not params
# -------------------------------------------------------------------
@app.get("/products/{id}")
async def get_product(request: Request, id: int):
    match = next((p for p in PRODUCTS if p["id"] == id), None)
    if not match:
        return Response.json({"error": f"Product {id} not found"}, status=404)
    return Response.json(match)


# -------------------------------------------------------------------
# Entry point
# -------------------------------------------------------------------
if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **`request.query_params`** — a dict-like mapping of the raw URL query string. All values arrive as strings, so explicit coercion is always required.
- **Type coercion** — the `parse_int` helper wraps `int()` in a `try/except` and returns both the parsed value *and* an optional error message, keeping the route handler clean.
- **Default values** — `params.get("page")` returns `None` when the parameter is absent; `parse_int` then substitutes the configured default, so clients never need to supply these params explicitly.
- **Validation errors** — all coercion errors are collected into a list before responding, so a client with two bad params sees both problems in a single `400` response instead of having to fix them one at a time.
- **Upper-bound clamping** — the `limit` parameter is checked against `MAX_LIMIT` to prevent a client from requesting an arbitrarily large page and overloading the server.
- **Pagination arithmetic** — `ceiling division` (`-(-total // limit)`) computes the total number of pages without importing `math.ceil`, keeping the dependency footprint minimal.

## Running This Example

```bash
# Install dependencies
pip install cello

# Start the server
python examples/basic/query_params.py
```

Try the endpoints:

```bash
# Basic search with defaults (page=1, limit=10)
curl -s "http://localhost:8000/search" | jq .

# Keyword filter
curl -s "http://localhost:8000/search?q=product+1" | jq .

# Custom pagination
curl -s "http://localhost:8000/search?page=2&limit=5" | jq .

# Invalid type — expect 400
curl -s "http://localhost:8000/search?page=abc&limit=-3" | jq .

# Limit exceeds maximum — expect 400
curl -s "http://localhost:8000/search?limit=200" | jq .
```
