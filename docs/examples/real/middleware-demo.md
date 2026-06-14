---
title: Middleware System Demo
description: Learn how to enable and compose CORS, request logging, and response compression middleware in a Cello application.
---

# :material-layers-triple: Middleware System Demo

Cello ships with a suite of built-in middleware that you can activate with a single method call on the `App` instance. This example shows how to layer CORS handling, structured logging, and gzip compression together while exploring the effect each middleware has on real HTTP requests. It also demonstrates how `Blueprint` groups share the same middleware stack automatically.

## Features Demonstrated

- `app.enable_cors()` — allowlist-based CORS with automatic preflight handling
- `app.enable_logging()` — per-request structured logging to stdout
- `app.enable_compression(min_size=…)` — transparent gzip compression for large responses
- Custom response headers via `Response.set_header()`
- Cache-control and ETag headers for cacheable endpoints
- `Blueprint` route grouping under a shared `/api` prefix
- Manual `OPTIONS` preflight route with `Response.no_content()`

## Complete Source Code

```python
#!/usr/bin/env python3
"""
Middleware System Demo for Cello v1.0.1.
Run with: python examples/middleware_demo.py
Then test with:
    curl -v http://127.0.0.1:8000/
    curl -H "Origin: https://example.com" http://127.0.0.1:8000/
"""

from cello import App, Blueprint, Response

app = App()
app.enable_cors(origins=["https://example.com", "https://app.example.com", "http://localhost:3000"])
app.enable_logging()
app.enable_compression(min_size=1024)

@app.get("/")
def home(request):
    return {"message": "Cello Middleware Demo", "version": "1.0.1",
            "enabled_middleware": ["CORS", "Logging", "Compression"]}

@app.get("/cors-demo")
def cors_demo(request):
    origin = request.get_header("Origin")
    return {"message": "CORS is enabled", "your_origin": origin,
            "allowed_origins": ["https://example.com", "http://localhost:3000"]}

@app.options("/cors-demo")
def cors_preflight(request):
    return Response.no_content()

@app.get("/compression-demo")
def compression_demo(request):
    return {"items": [{"id": i, "name": f"Item {i}", "description": f"Desc for item {i}" * 10} for i in range(100)]}

@app.get("/request-info")
def request_info(request):
    return {"method": request.method, "path": request.path,
            "headers": {"content-type": request.get_header("Content-Type"), "user-agent": request.get_header("User-Agent")}}

@app.post("/echo")
def echo_request(request):
    content_type = request.get_header("Content-Type") or ""
    if "application/json" in content_type:
        return {"received_json": request.json()}
    return {"received_text": request.text()}

api = Blueprint("/api", name="api")

@api.get("/data")
def api_data(request):
    return {"data": [{"id": 1, "value": "Item 1"}, {"id": 2, "value": "Item 2"}], "middleware_active": True}

@api.get("/users")
def api_users(request):
    return {"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}

app.register_blueprint(api)

@app.get("/custom-headers")
def custom_headers(request):
    response = Response.json({"message": "Response with custom headers"})
    response.set_header("X-Custom-Header", "custom-value")
    response.set_header("X-Powered-By", "Cello/1.0.1")
    return response

@app.get("/cache-headers")
def cache_headers(request):
    response = Response.json({"message": "Cacheable response"})
    response.set_header("Cache-Control", "public, max-age=3600")
    response.set_header("ETag", '"abc123"')
    return response

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

## Running This Example

```bash
python examples/middleware_demo.py
# Test:
curl http://127.0.0.1:8000/
```

## Key Concepts

- **Middleware order matters** — CORS headers are injected before logging writes its output, so the log line always reflects the final response status.
- **`enable_compression(min_size=1024)`** — responses smaller than 1 KB are sent as-is; only larger payloads are gzip-encoded, keeping small API calls lightweight.
- **Blueprint registration** — calling `app.register_blueprint(api)` mounts all routes defined on the blueprint under `/api` and ensures they inherit every middleware already attached to `app`.
- **`Response.no_content()`** — returns a `204 No Content` response, which is the correct reply to a CORS preflight `OPTIONS` request.
- **`Response.set_header()`** — lets you attach arbitrary HTTP headers to a response object without leaving the handler function, keeping header logic close to the business logic that produces it.
