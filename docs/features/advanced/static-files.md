---
title: Static Files
description: Efficient static file serving in Cello Framework
---

# Static Files

Cello serves static files directly from Rust with automatic MIME type detection, caching headers, ETag support, and path traversal protection. Use it to serve CSS, JavaScript, images, fonts, and other assets alongside your API.

---

## Quick Start

```python
from cello import App, StaticFilesConfig

app = App()

# Serve files from ./public at /static/*
config = StaticFilesConfig("/static", "./public")
app.enable_static_files(config)
```

With this configuration, a file at `./public/css/style.css` is accessible at `http://localhost:8000/static/css/style.css`.

---

## `StaticFilesConfig`

### Constructor

```python
StaticFilesConfig(url_path, root_dir)
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `url_path` | `str` | URL prefix for static files (e.g., `"/static"`) |
| `root_dir` | `str` | Filesystem directory containing the files (e.g., `"./public"`) |

### Configuration Options

Chain methods to customize behavior:

```python
from cello import StaticFilesConfig

config = (
    StaticFilesConfig("/static", "./public")
    .cache_str("30d")           # Cache for 30 days
    .etag(True)                 # Enable ETag headers
    .dir_listing(False)         # Disable directory listing
    .index("index.html")       # Serve index.html for directories
    .precompressed(True)        # Serve .gz/.br files when available
    .header("X-Served-By", "Cello")  # Add custom headers
)
```

### Full Option Reference

| Method | Default | Description |
|--------|---------|-------------|
| `.cache(CacheControl)` | `Public(1 day)` | Set cache-control directive |
| `.cache_str(str)` | -- | Set cache duration from string (`"1y"`, `"30d"`, `"1h"`) |
| `.cache_ext(ext, CacheControl)` | -- | Set cache-control per file extension |
| `.etag(bool)` | `True` | Generate ETag headers for conditional requests |
| `.dir_listing(bool)` | `False` | Enable directory listing pages |
| `.index(str)` | `"index.html"` | Index file to serve for directory requests |
| `.no_index()` | -- | Disable index file serving |
| `.precompressed(bool)` | `True` | Serve pre-compressed `.gz`/`.br` files |
| `.header(name, value)` | -- | Add a custom response header |
| `.hide_pattern(str)` | `"."`, `".."` | Block files matching the pattern |

---

## Serving a Directory

### Project Layout

```
myproject/
    app.py
    public/
        index.html
        css/
            style.css
        js/
            app.js
        images/
            logo.png
```

### Configuration

```python
from cello import App, StaticFilesConfig

app = App()

config = StaticFilesConfig("/static", "./public")
app.enable_static_files(config)

# Files are now available at:
#   /static/index.html
#   /static/css/style.css
#   /static/js/app.js
#   /static/images/logo.png
```

---

## Path Prefix

The `url_path` parameter defines the URL prefix. Only requests starting with this prefix are handled by the static file middleware:

```python
# Serve at /assets/*
StaticFilesConfig("/assets", "./public")

# Serve at root /*
StaticFilesConfig("/", "./public")

# Serve at /api/v1/docs/*
StaticFilesConfig("/api/v1/docs", "./documentation")
```

---

## MIME Type Detection

Cello automatically detects content types from file extensions. Supported types include:

| Extension | Content-Type |
|-----------|-------------|
| `.html`, `.htm` | `text/html; charset=utf-8` |
| `.css` | `text/css; charset=utf-8` |
| `.js`, `.mjs` | `text/javascript; charset=utf-8` |
| `.json` | `application/json; charset=utf-8` |
| `.png` | `image/png` |
| `.jpg`, `.jpeg` | `image/jpeg` |
| `.gif` | `image/gif` |
| `.svg` | `image/svg+xml` |
| `.webp` | `image/webp` |
| `.woff2` | `font/woff2` |
| `.pdf` | `application/pdf` |
| `.mp4` | `video/mp4` |
| Unknown | `application/octet-stream` |

---

## Caching Headers

### Default Caching

By default, static files are served with `Cache-Control: public, max-age=86400` (1 day):

```
Cache-Control: public, max-age=86400
```

### Custom Cache Duration

```python
# Cache for 1 year (versioned assets)
config = StaticFilesConfig("/static", "./public").cache_str("1y")

# Cache for 1 hour
config = StaticFilesConfig("/static", "./public").cache_str("1h")
```

### Per-Extension Caching

Set different cache policies for different file types:

```python
from cello._cello import CacheControl
from datetime import timedelta

config = (
    StaticFilesConfig("/static", "./public")
    .cache_str("1d")                                        # Default: 1 day
    .cache_ext("html", CacheControl.NoCache)                # HTML: no cache
    .cache_ext("js", CacheControl.Immutable(31536000))      # JS: immutable, 1 year
    .cache_ext("css", CacheControl.Immutable(31536000))     # CSS: immutable, 1 year
    .cache_ext("png", CacheControl.Public(2592000))         # Images: 30 days
)
```

### ETag Support

When enabled (default), Cello generates ETag headers from file modification time and size. Clients can send `If-None-Match` to receive `304 Not Modified` responses, saving bandwidth:

```
HTTP/1.1 200 OK
ETag: "186a0-3f2"
Cache-Control: public, max-age=86400

HTTP/1.1 304 Not Modified    <-- subsequent request with matching ETag
ETag: "186a0-3f2"
```

---

## Path Traversal Protection

Cello prevents directory traversal attacks at multiple levels:

1. **Double-dot blocking**: Paths containing `..` are rejected immediately
2. **Canonicalization**: Resolved paths are verified to be within the root directory
3. **Hidden file patterns**: Files matching hidden patterns (default: `.` prefix) are blocked

```python
# These requests are all blocked:
# /static/../../../etc/passwd  -> rejected (contains "..")
# /static/.env                 -> rejected (hidden file pattern)
# /static/..%2F..%2Fetc/passwd -> rejected (URL-decoded traversal)
```

### Custom Hidden Patterns

```python
config = (
    StaticFilesConfig("/static", "./public")
    .hide_pattern(".env")
    .hide_pattern(".git")
    .hide_pattern("__pycache__")
)
```

---

## Pre-Compressed Files

When `precompressed` is enabled (default), Cello checks for pre-compressed versions of files before serving the original. If the client accepts `br` or `gzip` encoding, Cello serves the compressed file with the appropriate `Content-Encoding` header:

```
public/
    app.js        (100 KB)
    app.js.br     (25 KB)  <-- served if client accepts Brotli
    app.js.gz     (30 KB)  <-- served if client accepts gzip
```

Preference order: Brotli (`.br`) > Gzip (`.gz`) > Original.

---

## Directory Listing

Optionally enable browseable directory listings:

```python
config = (
    StaticFilesConfig("/files", "./shared")
    .dir_listing(True)
    .no_index()  # Disable index.html so listing shows instead
)
```

This generates an HTML page listing all files and subdirectories, with hidden files excluded.

---

## Complete Example

```python
from cello import App, StaticFilesConfig

app = App()

# Serve static assets
static_config = StaticFilesConfig("/static", "./public").cache_str("30d").etag(True)
app.enable_static_files(static_config)

# Serve uploaded files (no cache)
uploads_config = (
    StaticFilesConfig("/uploads", "./uploads")
    .cache_str("0s")
    .etag(False)
    .hide_pattern(".gitkeep")
)
app.enable_static_files(uploads_config)

@app.get("/")
def home(request):
    return Response.html("""
    <html>
    <head><link rel="stylesheet" href="/static/css/style.css"></head>
    <body>
        <img src="/static/images/logo.png" alt="Logo">
        <script src="/static/js/app.js"></script>
    </body>
    </html>
    """)

if __name__ == "__main__":
    app.run()
```

---

## Performance

Static file serving runs entirely in Rust:

| Operation | Overhead |
|-----------|----------|
| Path resolution | ~200ns |
| MIME type lookup | ~50ns |
| ETag generation | ~100ns |
| File read | System I/O |
| 304 response | ~500ns (no file read) |

---

## Next Steps

- [Templates](templates.md) - Render dynamic HTML alongside static assets
- [File Uploads](file-uploads.md) - Handle file uploads from forms
- [Compression](../middleware/overview.md) - Enable gzip compression for dynamic responses
