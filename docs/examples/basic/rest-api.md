---
title: REST API
description: Complete REST API example with CRUD operations in Cello Framework
---

# REST API Example

This example builds a complete REST API for managing a collection of books, with full CRUD operations, proper HTTP status codes, error handling, and JSON responses.

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
REST API example - Book management service.

Run: python rest_api.py
Docs: http://127.0.0.1:8000/docs
"""

from cello import App, Response

app = App()
app.enable_cors()
app.enable_logging()
app.enable_openapi(title="Bookstore API", version="1.0.1")

# In-memory database
books = {
    "1": {"id": "1", "title": "The Rust Programming Language", "author": "Steve Klabnik", "year": 2019, "isbn": "978-1718500440"},
    "2": {"id": "2", "title": "Python Crash Course", "author": "Eric Matthes", "year": 2023, "isbn": "978-1718502703"},
}
next_id = 3


@app.get("/api/books", tags=["Books"], summary="List all books")
def list_books(request):
    """Return all books with optional filtering."""
    author = request.query.get("author")
    year = request.query.get("year")

    result = list(books.values())

    if author:
        result = [b for b in result if author.lower() in b["author"].lower()]
    if year:
        result = [b for b in result if b["year"] == int(year)]

    return {"books": result, "count": len(result)}


@app.get("/api/books/{id}", tags=["Books"], summary="Get a book by ID")
def get_book(request):
    """Return a single book by its ID."""
    book_id = request.params["id"]
    book = books.get(book_id)
    if not book:
        return Response.json({"error": "Book not found", "id": book_id}, status=404)
    return book


@app.post("/api/books", tags=["Books"], summary="Create a new book")
def create_book(request):
    """Create a new book from JSON body."""
    global next_id
    data = request.json()

    # Validate required fields
    required = ["title", "author"]
    missing = [f for f in required if f not in data]
    if missing:
        return Response.json(
            {"error": "Missing required fields", "fields": missing},
            status=400,
        )

    book = {
        "id": str(next_id),
        "title": data["title"],
        "author": data["author"],
        "year": data.get("year"),
        "isbn": data.get("isbn"),
    }
    books[book["id"]] = book
    next_id += 1

    return Response.json(book, status=201)


@app.put("/api/books/{id}", tags=["Books"], summary="Update a book")
def update_book(request):
    """Replace a book entirely."""
    book_id = request.params["id"]
    if book_id not in books:
        return Response.json({"error": "Book not found"}, status=404)

    data = request.json()
    books[book_id] = {
        "id": book_id,
        "title": data.get("title", books[book_id]["title"]),
        "author": data.get("author", books[book_id]["author"]),
        "year": data.get("year", books[book_id]["year"]),
        "isbn": data.get("isbn", books[book_id]["isbn"]),
    }
    return books[book_id]


@app.patch("/api/books/{id}", tags=["Books"], summary="Partially update a book")
def patch_book(request):
    """Update specific fields of a book."""
    book_id = request.params["id"]
    if book_id not in books:
        return Response.json({"error": "Book not found"}, status=404)

    data = request.json()
    for key in ["title", "author", "year", "isbn"]:
        if key in data:
            books[book_id][key] = data[key]

    return books[book_id]


@app.delete("/api/books/{id}", tags=["Books"], summary="Delete a book")
def delete_book(request):
    """Remove a book by ID."""
    book_id = request.params["id"]
    if book_id not in books:
        return Response.json({"error": "Book not found"}, status=404)

    deleted = books.pop(book_id)
    return {"deleted": True, "book": deleted}


@app.get("/api/stats", tags=["Stats"], summary="API statistics")
def stats(request):
    """Return API usage statistics."""
    authors = set(b["author"] for b in books.values())
    years = [b["year"] for b in books.values() if b["year"]]
    return {
        "total_books": len(books),
        "total_authors": len(authors),
        "year_range": {"min": min(years), "max": max(years)} if years else None,
    }


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Testing the API

### List Books

```bash
curl http://127.0.0.1:8000/api/books
```

### Filter by Author

```bash
curl "http://127.0.0.1:8000/api/books?author=klabnik"
```

### Get a Single Book

```bash
curl http://127.0.0.1:8000/api/books/1
```

### Create a Book

```bash
curl -X POST http://127.0.0.1:8000/api/books \
  -H "Content-Type: application/json" \
  -d '{"title": "Zero To Production", "author": "Luca Palmieri", "year": 2022}'
```

### Update a Book

```bash
curl -X PUT http://127.0.0.1:8000/api/books/1 \
  -H "Content-Type: application/json" \
  -d '{"title": "The Rust Programming Language (2nd Ed)", "author": "Steve Klabnik", "year": 2023}'
```

### Partial Update

```bash
curl -X PATCH http://127.0.0.1:8000/api/books/1 \
  -H "Content-Type: application/json" \
  -d '{"year": 2024}'
```

### Delete a Book

```bash
curl -X DELETE http://127.0.0.1:8000/api/books/2
```

---

## Key Concepts

### HTTP Status Codes

| Code | Usage |
|------|-------|
| `200` | Successful GET, PUT, PATCH, DELETE |
| `201` | Successful POST (resource created) |
| `400` | Bad request (missing fields, invalid data) |
| `404` | Resource not found |

### Response Patterns

Return a `dict` for automatic 200 JSON responses. Use `Response.json(data, status=code)` when you need a different status code.

### OpenAPI Documentation

With `app.enable_openapi()`, browse the auto-generated API docs at `/docs` (Swagger UI) or `/redoc` (ReDoc).

---

## Next Steps

- [Form Handling](forms.md) - Handle multipart uploads and form data
- [Full-stack App](../advanced/fullstack.md) - Add templates and static files
- [DTOs & Validation](../../features/advanced/dto-validation.md) - Add Pydantic validation
