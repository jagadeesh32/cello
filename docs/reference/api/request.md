---
title: Request Reference
description: Cello Request class API reference
---

# Request

The `Request` object represents an incoming HTTP request.

## Properties

### `request.method`

HTTP method (GET, POST, etc.).

```python
@app.route("/resource", methods=["GET", "POST"])
def handler(request):
    if request.method == "GET":
        return {"action": "get"}
    elif request.method == "POST":
        return {"action": "create"}
```

**Type:** `str`

---

### `request.path`

Request path without query string.

```python
@app.get("/users/{id}")
def handler(request):
    path = request.path  # "/users/123"
    return {"path": path}
```

**Type:** `str`

---

### `request.params`

Path parameters extracted from the URL.

```python
@app.get("/users/{user_id}/posts/{post_id}")
def handler(request):
    user_id = request.params["user_id"]
    post_id = request.params["post_id"]
    return {"user_id": user_id, "post_id": post_id}
```

**Type:** `dict[str, str]`

---

### `request.query`

Query string parameters.

```python
# GET /search?q=python&limit=10
@app.get("/search")
def handler(request):
    query = request.query.get("q", "")
    limit = request.query.get("limit", "10")
    return {"query": query, "limit": int(limit)}
```

**Type:** `QueryParams`

#### QueryParams Methods

| Method | Description |
|--------|-------------|
| `get(key, default=None)` | Get single value |
| `get_all(key)` | Get all values for key |
| `keys()` | Get all parameter keys |
| `items()` | Get all key-value pairs |

---

### `request.headers`

HTTP headers dictionary.

```python
@app.get("/")
def handler(request):
    content_type = request.headers.get("Content-Type")
    return {"content_type": content_type}
```

**Type:** `dict[str, str]`

---

### `request.context`

Request context for storing data across middleware.

```python
@app.get("/protected")
def handler(request):
    # Set by auth middleware
    user = request.context.get("user")
    claims = request.context.get("jwt_claims")
    return {"user": user}
```

**Type:** `Context`

---

### `request.session`

Session data (if sessions enabled).

```python
@app.get("/profile")
def handler(request):
    user_id = request.session.get("user_id")
    return {"user_id": user_id}
```

**Type:** `Session`

---

### `request.client_ip`

Client IP address.

```python
@app.get("/")
def handler(request):
    ip = request.client_ip
    return {"ip": ip}
```

**Type:** `str`

---

### `request.url`

Full request URL.

```python
@app.get("/")
def handler(request):
    url = request.url  # "http://localhost:8000/path?query=value"
    return {"url": url}
```

**Type:** `str`

---

### `request.scheme`

URL scheme (http or https).

```python
@app.get("/")
def handler(request):
    scheme = request.scheme  # "http" or "https"
    return {"scheme": scheme}
```

**Type:** `str`

---

### `request.host`

Host header value.

```python
@app.get("/")
def handler(request):
    host = request.host  # "example.com:8000"
    return {"host": host}
```

**Type:** `str`

---

### `request.content_type`

Content-Type header value.

```python
@app.post("/")
def handler(request):
    content_type = request.content_type
    return {"content_type": content_type}
```

**Type:** `str | None`

---

### `request.content_length`

Content-Length header value.

```python
@app.post("/")
def handler(request):
    length = request.content_length
    return {"content_length": length}
```

**Type:** `int | None`

---

## Methods

### `request.get_header(name, default=None)`

Get a specific header value.

```python
@app.get("/")
def handler(request):
    auth = request.get_header("Authorization")
    custom = request.get_header("X-Custom", "default")
    return {"auth": auth, "custom": custom}
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `str` | Header name (case-insensitive) |
| `default` | `str | None` | Default value if not found |

**Returns:** `str | None`

---

### `request.json()`

Parse request body as JSON.

```python
@app.post("/users")
def handler(request):
    data = request.json()
    name = data.get("name")
    email = data.get("email")
    return {"name": name, "email": email}
```

**Returns:** `dict | list`

**Raises:** `ValueError` if body is not valid JSON

!!! note "Lazy Parsing"
    JSON is parsed lazily on first access and cached for subsequent calls.

---

### `request.text()`

Get request body as text.

```python
@app.post("/text")
def handler(request):
    body = request.text()
    return {"body": body}
```

**Returns:** `str`

---

### `request.body()`

Get raw request body as bytes.

```python
@app.post("/binary")
def handler(request):
    data = request.body()
    return Response.binary(data)
```

**Returns:** `bytes`

---

### `request.form()`

Parse request body as form data.

```python
@app.post("/form")
def handler(request):
    form = request.form()
    name = form.get("name")
    email = form.get("email")
    return {"name": name, "email": email}
```

**Returns:** `dict[str, str]`

---

### `request.files()`

Get uploaded files from multipart form data.

```python
@app.post("/upload")
async def handler(request):
    files = await request.files()
    for file in files:
        filename = file.filename
        content = await file.read()
        # Process file
    return {"uploaded": len(files)}
```

**Returns:** `list[UploadFile]`

#### UploadFile Properties

| Property | Type | Description |
|----------|------|-------------|
| `filename` | `str` | Original filename |
| `content_type` | `str` | MIME type |
| `size` | `int` | File size in bytes |

#### UploadFile Methods

| Method | Description |
|--------|-------------|
| `read()` | Read entire file as bytes |
| `read_text()` | Read file as text |
| `save(path)` | Save file to disk |

---

### `request.is_json()`

Check if request has JSON content type.

```python
@app.post("/")
def handler(request):
    if request.is_json():
        data = request.json()
    else:
        data = request.form()
    return data
```

**Returns:** `bool`

---

### `request.is_form()`

Check if request has form content type.

```python
@app.post("/")
def handler(request):
    if request.is_form():
        data = request.form()
    else:
        data = request.json()
    return data
```

**Returns:** `bool`

---

## Async Methods

For async handlers, some methods return awaitables:

```python
@app.post("/async")
async def handler(request):
    # Async body methods
    body = await request.body_async()
    text = await request.text_async()
    json_data = await request.json_async()
    files = await request.files()

    return {"received": True}
```

---

## Request Context

The `request.context` object allows middleware to pass data to handlers:

```python
# In middleware
request.context.set("user_id", "123")
request.context.set("permissions", ["read", "write"])

# In handler
@app.get("/")
def handler(request):
    user_id = request.context.get("user_id")
    permissions = request.context.get("permissions", [])
    return {"user_id": user_id, "permissions": permissions}
```

### Context Methods

| Method | Description |
|--------|-------------|
| `get(key, default=None)` | Get value |
| `set(key, value)` | Set value |
| `has(key)` | Check if key exists |
| `delete(key)` | Remove key |
| `clear()` | Clear all values |
