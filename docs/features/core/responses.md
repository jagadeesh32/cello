---
title: Response Types
description: HTTP response types and builders in Cello Framework
---

# Response Types

Cello provides multiple ways to return HTTP responses. The simplest approach is to return a Python `dict` from your handler -- Cello automatically serializes it to JSON using SIMD-accelerated serialization in Rust. For more control, use the `Response` class.

## Quick Reference

```python
from cello import App, Response

app = App()

# Return a dict -- auto-serialized to JSON
@app.get("/simple")
def simple(request):
    return {"message": "Hello, World!"}

# Return an explicit Response object
@app.get("/explicit")
def explicit(request):
    return Response.json({"message": "Hello"}, status=200)
```

---

## Dict Returns (Auto JSON)

The simplest way to return data. Cello serializes the dictionary to JSON in Rust:

```python
@app.get("/users")
def list_users(request):
    return {"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}
```

The response will have:

- Status code: `200`
- Content-Type: `application/json`
- Body: SIMD-serialized JSON

!!! tip "Performance"
    Returning a `dict` is the fastest response path. Cello serializes it in Rust using SIMD JSON, bypassing Python's `json` module entirely.

---

## Response.json()

Create a JSON response with an explicit status code:

```python
@app.post("/users")
def create_user(request):
    data = request.json()
    return Response.json({"id": 1, **data}, status=201)

@app.get("/not-found")
def not_found(request):
    return Response.json({"error": "Not found"}, status=404)
```

---

## Response.text()

Return a plain text response:

```python
@app.get("/health")
def health(request):
    return Response.text("OK")

@app.get("/error")
def error(request):
    return Response.text("Something went wrong", status=500)
```

---

## Response.html()

Return an HTML response:

```python
@app.get("/page")
def page(request):
    return Response.html("""
        <html>
            <body>
                <h1>Welcome to Cello</h1>
                <p>Ultra-fast Python web framework</p>
            </body>
        </html>
    """)
```

---

## Response.file()

Serve a file as a download with automatic content type detection:

```python
@app.get("/download/report")
def download_report(request):
    return Response.file("/path/to/report.pdf")

# With custom filename
@app.get("/download/data")
def download_data(request):
    return Response.file(
        "/path/to/export.csv",
        filename="monthly-report.csv"
    )

# With explicit content type
@app.get("/download/archive")
def download_archive(request):
    return Response.file(
        "/path/to/archive.tar.gz",
        content_type="application/gzip"
    )
```

The `Content-Disposition` header is set automatically so browsers prompt a download dialog.

---

## Response.redirect()

Redirect the client to another URL:

```python
# Temporary redirect (302)
@app.get("/old-page")
def old_page(request):
    return Response.redirect("/new-page")

# Permanent redirect (301)
@app.get("/legacy")
def legacy(request):
    return Response.redirect("/modern", permanent=True)
```

---

## Response.no_content()

Return a `204 No Content` response with an empty body:

```python
@app.delete("/items/{id}")
def delete_item(request):
    item_id = request.params["id"]
    # ... delete logic ...
    return Response.no_content()
```

---

## Response.created()

Return a `201 Created` response, optionally with a body and `Location` header:

```python
@app.post("/items")
def create_item(request):
    data = request.json()
    return Response.created(
        {"id": 42, **data},
        location="/items/42"
    )
```

---

## Response.binary()

Return raw bytes with a custom content type:

```python
@app.get("/image")
def image(request):
    with open("logo.png", "rb") as f:
        data = f.read()
    return Response.binary(data, content_type="image/png")

# Default content type is application/octet-stream
@app.get("/raw")
def raw(request):
    return Response.binary(b"\x00\x01\x02\x03")
```

---

## Response.xml()

Return an XML response. Cello converts Python dicts to XML in Rust:

```python
@app.get("/data.xml")
def xml_data(request):
    return Response.xml(
        {"name": "Cello", "version": "1.0.1"},
        root_name="framework"
    )
    # Produces: <framework><name>Cello</name><version>1.0.1</version></framework>
```

---

## Custom Headers

Set custom headers on any `Response` object:

```python
@app.get("/custom")
def custom(request):
    resp = Response.json({"data": "value"})
    resp.set_header("X-Custom-Header", "my-value")
    resp.set_header("X-Request-Id", "abc-123")
    return resp
```

---

## Status Codes

All factory methods accept an optional `status` parameter:

```python
Response.json(data, status=201)       # Created
Response.json(data, status=400)       # Bad Request
Response.text("Not found", status=404)  # Not Found
Response.html(page, status=200)       # OK
```

### Common Status Code Patterns

```python
# Success responses
return {"data": result}                            # 200 OK (dict auto)
return Response.created(data, location="/items/1") # 201 Created
return Response.no_content()                       # 204 No Content

# Client error responses
return Response.json({"error": "Bad input"}, status=400)
return Response.json({"error": "Unauthorized"}, status=401)
return Response.json({"error": "Forbidden"}, status=403)
return Response.json({"error": "Not found"}, status=404)

# Redirect responses
return Response.redirect("/new-url")               # 302 Found
return Response.redirect("/new-url", permanent=True) # 301 Moved
```

---

## Streaming Responses

For large payloads, use Cello's streaming response support to avoid loading the entire body into memory:

```python
@app.get("/stream")
def stream(request):
    return Response.sendfile("/path/to/large-file.bin")
```

`Response.sendfile()` uses zero-copy I/O in Rust for maximum throughput.

### Partial Content (Range Requests)

Cello supports HTTP range requests for resumable downloads and media streaming:

```python
@app.get("/video/{name}")
def video(request):
    name = request.params["name"]
    range_header = request.headers.get("range")
    if range_header:
        return Response.file_range(f"/media/{name}", range_header)
    return Response.sendfile(f"/media/{name}", content_type="video/mp4")
```

---

## Response Method Summary

| Method | Content-Type | Status | Use Case |
|--------|-------------|--------|----------|
| `dict` return | `application/json` | 200 | Simple JSON responses |
| `Response.json()` | `application/json` | configurable | JSON with custom status |
| `Response.text()` | `text/plain` | configurable | Plain text |
| `Response.html()` | `text/html` | configurable | HTML pages |
| `Response.file()` | auto-detected | 200 | File downloads |
| `Response.sendfile()` | auto-detected | 200 | Zero-copy file serving |
| `Response.redirect()` | -- | 301/302 | URL redirects |
| `Response.no_content()` | -- | 204 | Empty responses |
| `Response.created()` | `application/json` | 201 | Resource creation |
| `Response.binary()` | configurable | configurable | Raw bytes |
| `Response.xml()` | `application/xml` | configurable | XML responses |

---

## Next Steps

- [Request Handling](requests.md) - Working with incoming requests
- [Routing](routing.md) - Defining routes and parameters
- [Middleware Overview](../middleware/overview.md) - Adding middleware to responses
