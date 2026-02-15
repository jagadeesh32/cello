---
title: "Tutorial: Build a REST API"
description: Step-by-step guide to building a complete REST API with Cello
---

# Tutorial: Build a REST API

In this tutorial you will build a complete REST API for managing a **books** resource. You will learn how to set up a Cello project, define CRUD routes, handle JSON requests and responses, work with path and query parameters, and add error handling.

---

## Prerequisites

- Python 3.12 or later
- Cello installed (`pip install cello-framework`)
- A terminal and a text editor

---

## Step 1: Project Setup

Create a project directory and install dependencies.

```bash
mkdir bookstore-api && cd bookstore-api
python -m venv .venv
source .venv/bin/activate
pip install cello-framework
```

Create the main application file.

```bash
touch app.py
```

---

## Step 2: Create the Application

Open `app.py` and initialize a Cello application.

```python
from cello import App, Response

app = App()

# In-memory data store
books = {}
next_id = 1
```

The `books` dictionary acts as our database for this tutorial. Each book is stored by its integer ID.

---

## Step 3: List All Books (GET)

Add a route that returns every book in the store. Support an optional `genre` query parameter for filtering.

```python
@app.get("/books")
def list_books(request):
    """List all books, optionally filtered by genre."""
    genre = request.query.get("genre")

    result = list(books.values())
    if genre:
        result = [b for b in result if b.get("genre", "").lower() == genre.lower()]

    return {"books": result, "total": len(result)}
```

!!! tip
    Returning a plain `dict` is the fastest option. Cello serializes it to JSON using its Rust SIMD engine -- no need to call `json.dumps` yourself.

---

## Step 4: Get a Single Book (GET with Path Parameter)

```python
@app.get("/books/{id}")
def get_book(request):
    """Retrieve a book by ID."""
    book_id = int(request.params["id"])
    book = books.get(book_id)

    if not book:
        return Response.json(
            {"error": "Book not found", "id": book_id},
            status=404,
        )

    return book
```

Path parameters are extracted from `request.params` as strings. Cast them to the expected type yourself.

---

## Step 5: Create a Book (POST)

```python
@app.post("/books")
def create_book(request):
    """Create a new book."""
    global next_id

    data = request.json()
    if not data or "title" not in data:
        return Response.json(
            {"error": "Field 'title' is required"},
            status=400,
        )

    book = {
        "id": next_id,
        "title": data["title"],
        "author": data.get("author", "Unknown"),
        "genre": data.get("genre", "General"),
        "year": data.get("year"),
    }
    books[next_id] = book
    next_id += 1

    return Response.json(book, status=201)
```

`request.json()` parses the request body through Cello's Rust SIMD JSON parser, which is significantly faster than Python's built-in `json` module.

---

## Step 6: Update a Book (PUT)

```python
@app.put("/books/{id}")
def update_book(request):
    """Update an existing book."""
    book_id = int(request.params["id"])
    if book_id not in books:
        return Response.json({"error": "Book not found"}, status=404)

    data = request.json()
    book = books[book_id]
    book["title"] = data.get("title", book["title"])
    book["author"] = data.get("author", book["author"])
    book["genre"] = data.get("genre", book["genre"])
    book["year"] = data.get("year", book["year"])

    return book
```

---

## Step 7: Delete a Book (DELETE)

```python
@app.delete("/books/{id}")
def delete_book(request):
    """Delete a book by ID."""
    book_id = int(request.params["id"])
    if book_id not in books:
        return Response.json({"error": "Book not found"}, status=404)

    del books[book_id]
    return Response.json({"deleted": True, "id": book_id}, status=200)
```

---

## Step 8: Add Error Handling

Register a global exception handler so unexpected errors return a clean JSON response instead of a stack trace.

```python
@app.exception_handler(ValueError)
def handle_value_error(request, exc):
    return Response.json(
        {"error": "Invalid value", "detail": str(exc)},
        status=400,
    )

@app.exception_handler(Exception)
def handle_generic_error(request, exc):
    return Response.json(
        {"error": "Internal server error"},
        status=500,
    )
```

---

## Step 9: Run the Server

Add the entry point at the bottom of `app.py`.

```python
if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

Start the server:

```bash
python app.py
```

---

## Step 10: Test with curl

Open a second terminal and run the following commands.

```bash
# Create a book
curl -X POST http://127.0.0.1:8000/books \
  -H "Content-Type: application/json" \
  -d '{"title": "Dune", "author": "Frank Herbert", "genre": "Science Fiction", "year": 1965}'

# List all books
curl http://127.0.0.1:8000/books

# Filter by genre
curl "http://127.0.0.1:8000/books?genre=Science+Fiction"

# Get a single book
curl http://127.0.0.1:8000/books/1

# Update the book
curl -X PUT http://127.0.0.1:8000/books/1 \
  -H "Content-Type: application/json" \
  -d '{"year": 1965, "author": "Frank Herbert"}'

# Delete the book
curl -X DELETE http://127.0.0.1:8000/books/1

# Verify deletion
curl http://127.0.0.1:8000/books/1
```

---

## Complete Source Code

??? example "Full `app.py`"
    ```python
    from cello import App, Response

    app = App()
    books = {}
    next_id = 1

    @app.get("/books")
    def list_books(request):
        genre = request.query.get("genre")
        result = list(books.values())
        if genre:
            result = [b for b in result if b.get("genre", "").lower() == genre.lower()]
        return {"books": result, "total": len(result)}

    @app.get("/books/{id}")
    def get_book(request):
        book_id = int(request.params["id"])
        book = books.get(book_id)
        if not book:
            return Response.json({"error": "Book not found"}, status=404)
        return book

    @app.post("/books")
    def create_book(request):
        global next_id
        data = request.json()
        if not data or "title" not in data:
            return Response.json({"error": "'title' is required"}, status=400)
        book = {
            "id": next_id,
            "title": data["title"],
            "author": data.get("author", "Unknown"),
            "genre": data.get("genre", "General"),
            "year": data.get("year"),
        }
        books[next_id] = book
        next_id += 1
        return Response.json(book, status=201)

    @app.put("/books/{id}")
    def update_book(request):
        book_id = int(request.params["id"])
        if book_id not in books:
            return Response.json({"error": "Book not found"}, status=404)
        data = request.json()
        book = books[book_id]
        for key in ("title", "author", "genre", "year"):
            if key in data:
                book[key] = data[key]
        return book

    @app.delete("/books/{id}")
    def delete_book(request):
        book_id = int(request.params["id"])
        if book_id not in books:
            return Response.json({"error": "Book not found"}, status=404)
        del books[book_id]
        return {"deleted": True, "id": book_id}

    if __name__ == "__main__":
        app.run(host="127.0.0.1", port=8000)
    ```

---

## Next Steps

- Add [authentication](auth-system.md) to protect write endpoints.
- Explore [Blueprints](../../reference/api/blueprint.md) to organize routes into modules.
- Enable [OpenAPI documentation](../../reference/api/response.md) with `app.enable_openapi()`.
