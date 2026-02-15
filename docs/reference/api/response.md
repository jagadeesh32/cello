---
title: Response API
description: Complete reference for the Cello Response class and its factory methods
---

# Response API

The `Response` class represents an HTTP response. It provides factory class methods for common content types and allows setting headers, status codes, and cookies.

---

## Importing

```python
from cello import Response
```

---

## Factory Methods

### `Response.json(data, status=200)`

Create a JSON response. The data is serialized using Cello's Rust SIMD JSON engine.

```python
Response.json({"message": "Hello"})
Response.json({"error": "Not found"}, status=404)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `data` | `dict` or `list` | Required | Data to serialize as JSON |
| `status` | `int` | `200` | HTTP status code |

**Returns:** `Response`

---

### `Response.text(body, status=200)`

Create a plain text response with `Content-Type: text/plain`.

```python
Response.text("Hello, world!")
Response.text("Not allowed", status=403)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `body` | `str` | Required | Response body text |
| `status` | `int` | `200` | HTTP status code |

**Returns:** `Response`

---

### `Response.html(body, status=200)`

Create an HTML response with `Content-Type: text/html`.

```python
Response.html("<h1>Hello</h1>")
Response.html("<p>Not found</p>", status=404)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `body` | `str` | Required | HTML content |
| `status` | `int` | `200` | HTTP status code |

**Returns:** `Response`

---

### `Response.binary(data, content_type="application/octet-stream", status=200)`

Create a binary response for file downloads or arbitrary byte data.

```python
Response.binary(pdf_bytes, content_type="application/pdf")
Response.binary(image_data, content_type="image/png")
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `data` | `bytes` | Required | Binary content |
| `content_type` | `str` | `"application/octet-stream"` | MIME type |
| `status` | `int` | `200` | HTTP status code |

**Returns:** `Response`

---

### `Response.redirect(url, status=302)`

Create a redirect response.

```python
Response.redirect("/login")
Response.redirect("/new-url", status=301)  # Permanent redirect
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `url` | `str` | Required | Target URL |
| `status` | `int` | `302` | Redirect status code (301, 302, 303, 307, 308) |

**Returns:** `Response`

---

### `Response.xml(body, status=200)`

Create an XML response with `Content-Type: application/xml`.

```python
Response.xml("<root><item>1</item></root>")
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `body` | `str` | Required | XML content |
| `status` | `int` | `200` | HTTP status code |

**Returns:** `Response`

---

## Instance Methods

### `response.set_header(name, value)`

Set a response header. If the header already exists, it is overwritten.

```python
resp = Response.json({"ok": True})
resp.set_header("X-Custom", "value")
resp.set_header("Cache-Control", "no-cache")
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `str` | Header name |
| `value` | `str` | Header value |

---

### `response.set_cookie(name, value, **options)`

Set a cookie on the response.

```python
resp = Response.json({"logged_in": True})
resp.set_cookie("session_id", "abc123", httponly=True, secure=True, max_age=3600)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `str` | Cookie name |
| `value` | `str` | Cookie value |
| `max_age` | `int` | Lifetime in seconds |
| `path` | `str` | Cookie path (default: `"/"`) |
| `domain` | `str` | Cookie domain |
| `secure` | `bool` | Require HTTPS |
| `httponly` | `bool` | Prevent JavaScript access |
| `samesite` | `str` | `"Strict"`, `"Lax"`, or `"None"` |

---

## Returning Responses from Handlers

### Returning a dict (fastest)

```python
@app.get("/users")
def list_users(request):
    return {"users": []}  # Cello serializes via Rust SIMD
```

### Returning a Response object

```python
@app.post("/users")
def create_user(request):
    return Response.json({"created": True}, status=201)
```

### Setting custom headers

```python
@app.get("/data")
def get_data(request):
    resp = Response.json({"data": "value"})
    resp.set_header("X-Request-ID", "abc-123")
    return resp
```

---

## Status Code Helpers

Cello does not provide named status code constants. Use integer literals directly.

```python
Response.json(data, status=200)   # OK
Response.json(data, status=201)   # Created
Response.json(data, status=204)   # No Content
Response.json(data, status=400)   # Bad Request
Response.json(data, status=401)   # Unauthorized
Response.json(data, status=403)   # Forbidden
Response.json(data, status=404)   # Not Found
Response.json(data, status=422)   # Unprocessable Entity
Response.json(data, status=429)   # Too Many Requests
Response.json(data, status=500)   # Internal Server Error
```

---

## Summary

| Method | Content-Type | Use Case |
|--------|-------------|----------|
| `Response.json()` | `application/json` | API responses |
| `Response.text()` | `text/plain` | Plain text |
| `Response.html()` | `text/html` | HTML pages |
| `Response.binary()` | Custom | File downloads, images |
| `Response.redirect()` | N/A | URL redirects |
| `Response.xml()` | `application/xml` | XML APIs |
