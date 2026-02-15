---
title: Compression Middleware
description: Response compression in Cello Framework
---

# Compression Middleware

Cello provides built-in gzip compression for HTTP responses, implemented in Rust for high throughput. Compression reduces bandwidth usage and improves load times for clients, especially for JSON and HTML responses.

## Quick Start

```python
from cello import App

app = App()

# Enable with defaults (compress responses > 1024 bytes)
app.enable_compression()

# Custom minimum size
app.enable_compression(min_size=512)
```

---

## How It Works

```
Handler returns response body
         │
         ▼
  Is body size >= min_size?
         │
    Yes  │  No
    ▼    │   ▼
 Gzip    │  Pass through
 compress│  uncompressed
    │    │
    ▼    ▼
 Add Content-Encoding: gzip
 Send response
```

The middleware:

1. Checks the response body size against `min_size`.
2. Checks the `Accept-Encoding` request header for `gzip` support.
3. Compresses the body in Rust using native gzip.
4. Sets the `Content-Encoding: gzip` response header.
5. Updates the `Content-Length` header.

---

## Configuration

### Default Settings

```python
app.enable_compression()
```

| Setting | Default | Description |
|---------|---------|-------------|
| `min_size` | `1024` | Minimum body size in bytes to trigger compression |

### Custom Minimum Size

```python
# Compress responses larger than 256 bytes
app.enable_compression(min_size=256)

# Compress responses larger than 4KB
app.enable_compression(min_size=4096)
```

!!! tip "Choosing min_size"
    Small responses (under 1KB) often do not benefit from compression because the gzip overhead can make the output the same size or larger. The default of 1024 bytes is a good starting point for most APIs.

---

## What Gets Compressed

The middleware compresses responses that meet **all** of these criteria:

- Body size is greater than or equal to `min_size`
- The client sends `Accept-Encoding: gzip` in the request
- The response content type is compressible (text, JSON, HTML, XML, etc.)

### Compressible Content Types

| Content Type | Compressed |
|-------------|-----------|
| `application/json` | Yes |
| `text/html` | Yes |
| `text/plain` | Yes |
| `text/css` | Yes |
| `application/javascript` | Yes |
| `application/xml` | Yes |
| `image/png` | No (already compressed) |
| `image/jpeg` | No (already compressed) |
| `application/gzip` | No (already compressed) |
| `application/octet-stream` | No |

---

## Example

```python
from cello import App

app = App()
app.enable_compression(min_size=512)

@app.get("/large-data")
def large_data(request):
    # This response is ~2KB of JSON -- will be compressed
    return {
        "items": [{"id": i, "name": f"Item {i}", "description": "A" * 100}
                  for i in range(10)]
    }

@app.get("/small-data")
def small_data(request):
    # This response is ~30 bytes -- will NOT be compressed
    return {"status": "ok"}
```

---

## Verifying Compression

Check the response headers to confirm compression is active:

```bash
# With compression
curl -H "Accept-Encoding: gzip" -v http://localhost:8000/large-data 2>&1 | grep Content-Encoding
# Content-Encoding: gzip

# Without Accept-Encoding header
curl -v http://localhost:8000/large-data 2>&1 | grep Content-Encoding
# (no Content-Encoding header -- response sent uncompressed)
```

---

## Performance

Cello's compression is implemented in Rust using a native gzip encoder:

| Metric | Value |
|--------|-------|
| Compression speed | ~1us per KB |
| Typical ratio (JSON) | 5:1 to 10:1 |
| Overhead (skip) | ~10ns (size check only) |

!!! info "When to Skip Compression"
    If you are serving binary data, pre-compressed files, or images, compression adds overhead with no benefit. These content types are automatically excluded by the middleware.

---

## Interaction with Other Middleware

Compression should be one of the last middleware in the chain, so it compresses the final response:

```python
app = App()

app.enable_cors()            # 1. CORS headers first
app.enable_logging()         # 2. Log before compression
app.enable_compression()     # 3. Compress last
```

Caching and compression work well together -- cached responses store the compressed version:

```python
app.enable_caching(ttl=300)
app.enable_compression()
```

---

## Next Steps

- [Middleware Overview](overview.md) - Full middleware system
- [Caching](caching.md) - Cache compressed responses
- [Logging](logging.md) - Request/response logging
