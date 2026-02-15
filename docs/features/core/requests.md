---
title: Request Handling
description: Working with HTTP requests in Cello Framework
---

# Request Handling

Every route handler in Cello receives a `Request` object as its first argument. The `Request` is constructed in Rust with zero-copy optimizations and exposes a clean Python API for accessing path parameters, headers, query strings, and request bodies.

## The Request Object

```python
@app.get("/example")
def handler(request):
    print(request.method)        # "GET"
    print(request.path)          # "/example"
    print(request.params)        # {} (path parameters)
    print(request.query_params)  # {} (query string parameters)
    print(request.headers)       # {"host": "localhost:8000", ...}
```

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `method` | `str` | HTTP method (`GET`, `POST`, etc.) |
| `path` | `str` | Request path (e.g., `/users/123`) |
| `params` | `dict[str, str]` | Path parameters extracted from the route |
| `query_params` | `dict[str, str]` | Query string parameters |
| `query` | `dict[str, str]` | Alias for `query_params` |
| `headers` | `dict[str, str]` | Request headers (lowercase keys) |

---

## Path Parameters

Path parameters are extracted automatically by the Rust radix-tree router and are available via `request.params`:

```python
@app.get("/users/{user_id}/posts/{post_id}")
def get_post(request):
    user_id = request.params["user_id"]
    post_id = request.params["post_id"]
    return {"user_id": user_id, "post_id": post_id}
```

!!! note
    Path parameters are always strings. Cast them explicitly if you need integers or other types.

```python
@app.get("/items/{id}")
def get_item(request):
    item_id = int(request.params["id"])
    return {"id": item_id}
```

---

## Query Parameters

Access query string values from `request.query_params` or the `request.query` alias:

```python
# GET /search?q=python&limit=10&sort=desc
@app.get("/search")
def search(request):
    query = request.query_params.get("q", "")
    limit = int(request.query_params.get("limit", "10"))
    sort = request.query.get("sort", "asc")
    return {"query": query, "limit": limit, "sort": sort}
```

### Using get_query_param()

For convenience, the `get_query_param()` method provides default value support:

```python
@app.get("/search")
def search(request):
    query = request.get_query_param("q", "")
    page = request.get_query_param("page", "1")
    return {"query": query, "page": int(page)}
```

---

## Request Headers

Headers are available as a dictionary with lowercase keys:

```python
@app.get("/info")
def info(request):
    content_type = request.headers.get("content-type", "")
    auth = request.headers.get("authorization", "")
    user_agent = request.headers.get("user-agent", "")
    return {
        "content_type": content_type,
        "auth_present": bool(auth),
        "user_agent": user_agent
    }
```

Use `get_header()` for a convenient accessor with default values:

```python
@app.get("/versioned")
def versioned(request):
    version = request.get_header("API-Version", "1")
    return {"api_version": version}
```

---

## Request Body

Cello uses **lazy body parsing** -- the body is only parsed when you access it, and the result is cached for subsequent calls within the same request. All parsing is performed in Rust using SIMD-accelerated JSON.

### JSON Body

```python
@app.post("/users")
def create_user(request):
    data = request.json()  # Parsed via SIMD JSON in Rust, cached
    return {"created": data["name"]}
```

!!! tip "Performance"
    `request.json()` uses SIMD-accelerated JSON parsing in Rust, which is up to 10x faster than Python's built-in `json` module. Results are cached so repeated calls are free.

### Text Body

```python
@app.post("/notes")
def create_note(request):
    body_text = request.text()  # Body as UTF-8 string, cached
    return {"length": len(body_text)}
```

### Raw Bytes

```python
@app.post("/upload")
def upload(request):
    raw = request.body()  # Raw bytes
    return {"size": len(raw)}
```

### Form Data

```python
@app.post("/login")
def login(request):
    form = request.form()  # URL-encoded form data as dict
    username = form.get("username", "")
    password = form.get("password", "")
    return {"user": username}
```

---

## Content Type Detection

Check the content type of incoming requests:

```python
@app.post("/data")
def handle_data(request):
    if request.is_json():
        data = request.json()
        return {"format": "json", "data": data}
    elif request.is_form():
        data = request.form()
        return {"format": "form", "data": data}
    elif request.is_multipart():
        return {"format": "multipart"}
    else:
        return {"format": "unknown", "content_type": request.content_type()}
```

| Method | Checks for |
|--------|------------|
| `is_json()` | `application/json` |
| `is_form()` | `application/x-www-form-urlencoded` |
| `is_multipart()` | `multipart/form-data` |
| `content_type()` | Returns the raw `Content-Type` header value |

---

## Request Context

Middleware can attach data to the request context, which is then available in handlers:

```python
# After JWT middleware runs, claims are in context
@app.get("/profile")
def profile(request):
    claims = request.context.get("jwt_claims")
    user_id = claims["sub"]
    return {"user_id": user_id}

# After session middleware runs, session data is available
@app.get("/dashboard")
def dashboard(request):
    user = request.session.get("user_id")
    return {"user": user}
```

---

## Lazy Parsing Internals

Cello's lazy body parsing ensures minimal overhead:

```
First call to request.json()
  → Parse body bytes with SIMD JSON (Rust)
  → Cache the result in an RwLock

Second call to request.json()
  → Return cached result (no re-parsing)
```

| Operation | Overhead | Notes |
|-----------|----------|-------|
| `json()` first call | ~1us/KB | SIMD-accelerated |
| `json()` subsequent | ~10ns | Cached result |
| `text()` first call | ~100ns | UTF-8 validation |
| `form()` first call | ~500ns | URL decoding |
| `body()` | ~0ns | Zero-copy bytes |

!!! warning
    Do not parse JSON manually in Python using the `json` module. Always use `request.json()` to benefit from Rust's SIMD acceleration and caching.

---

## Next Steps

- [Response Types](responses.md) - Building HTTP responses
- [Routing](routing.md) - Route definition and parameters
- [Blueprints](blueprints.md) - Organizing routes into groups
