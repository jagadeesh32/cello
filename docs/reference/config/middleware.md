---
title: Middleware Configuration
description: Configuration classes for CORS, compression, logging, static files, body limits, and Prometheus
---

# Middleware Configuration

This reference covers configuration classes for non-security middleware in Cello: CORS, compression, logging, static files, body limits, and Prometheus metrics.

---

## CORS Configuration

CORS is enabled via `app.enable_cors()`. For simple use cases, pass a list of origins directly.

```python
app.enable_cors(origins=["https://app.example.com", "https://admin.example.com"])
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `origins` | `list[str]` | `["*"]` | Allowed origin domains |

The CORS middleware automatically handles:

- `Access-Control-Allow-Origin` header
- `Access-Control-Allow-Methods` header (all methods enabled)
- `Access-Control-Allow-Headers` header (common headers)
- `Access-Control-Max-Age` header (preflight caching)
- OPTIONS preflight requests

### Wildcard Origins

Using `["*"]` allows any origin. This is convenient for development but should be restricted in production.

```python
# Development
app.enable_cors(origins=["*"])

# Production
app.enable_cors(origins=["https://myapp.com"])
```

---

## Compression Configuration

Enable gzip compression for responses above a size threshold.

```python
app.enable_compression(min_size=1024)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `min_size` | `int` | `1024` | Minimum response body size in bytes to trigger compression |

### Behavior

- Only compresses responses where `Content-Length >= min_size`.
- Only compresses when the client sends `Accept-Encoding: gzip`.
- Skips already-compressed content types (images, videos).
- Adds `Content-Encoding: gzip` to compressed responses.

---

## Logging Configuration

Enable request/response logging.

```python
app.enable_logging()
```

No configuration parameters. Log output includes:

| Field | Example |
|-------|---------|
| Method | `GET` |
| Path | `/api/users` |
| Status | `200` |
| Latency | `1.23ms` |
| Timestamp | ISO 8601 |

### Disabling Logs

```bash
# Via command line
python app.py --no-logs

# Via app.run()
app.run(logs=False)
```

---

## Static Files Configuration

```python
from cello import StaticFilesConfig

app.enable_static_files(StaticFilesConfig(
    directory="./static",
    prefix="/static",
    index_file="index.html",
    cache_max_age=3600,
))
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `directory` | `str` | Required | Path to the directory containing static files |
| `prefix` | `str` | `"/static"` | URL prefix for static file requests |
| `index_file` | `str` | `"index.html"` | Default file served for directory requests |
| `cache_max_age` | `int` | `3600` | `Cache-Control: max-age` value in seconds |

### Example

With `directory="./static"` and `prefix="/static"`:

- `GET /static/css/app.css` serves `./static/css/app.css`
- `GET /static/js/main.js` serves `./static/js/main.js`
- `GET /static/` serves `./static/index.html`

---

## Body Limit Configuration

Restrict the maximum request body size.

```python
app.enable_body_limit(max_size=10 * 1024 * 1024)  # 10 MB
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_size` | `int` | `10485760` | Maximum body size in bytes (10 MB) |

Requests exceeding the limit receive a `413 Payload Too Large` response before the body is fully read.

---

## Prometheus Configuration

Expose Prometheus-compatible metrics.

```python
app.enable_prometheus(
    endpoint="/metrics",
    namespace="cello",
    subsystem="http",
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `endpoint` | `str` | `"/metrics"` | URL path for the metrics endpoint |
| `namespace` | `str` | `"cello"` | Prometheus metric namespace |
| `subsystem` | `str` | `"http"` | Prometheus metric subsystem |

### Exposed Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `{ns}_{sub}_requests_total` | Counter | Total request count by method and status |
| `{ns}_{sub}_request_duration_seconds` | Histogram | Request latency distribution |
| `{ns}_{sub}_requests_in_flight` | Gauge | Currently active requests |
| `{ns}_{sub}_response_size_bytes` | Histogram | Response body size distribution |

Where `{ns}` is the namespace and `{sub}` is the subsystem.

### Scraping

Add the metrics endpoint to your Prometheus configuration:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: cello-app
    static_configs:
      - targets: ["app:8000"]
    metrics_path: /metrics
    scrape_interval: 15s
```

---

## Caching Configuration

```python
app.enable_caching(
    ttl=300,
    methods=["GET", "HEAD"],
    exclude_paths=["/health", "/metrics"],
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `ttl` | `int` | `300` | Default cache lifetime in seconds |
| `methods` | `list[str]` | `["GET", "HEAD"]` | HTTP methods to cache |
| `exclude_paths` | `list[str]` | `[]` | Paths excluded from caching |

---

## ETag Configuration

```python
app.enable_etag()
```

No configuration parameters. The middleware automatically:

- Generates an ETag hash for every response body.
- Adds the `ETag` header to responses.
- Returns `304 Not Modified` when the client sends a matching `If-None-Match` header.

---

## Request ID Configuration

```python
app.enable_request_id()
```

No configuration parameters. Each request receives a UUID v4 identifier in:

- `request.context["request_id"]`
- `X-Request-ID` response header

If the incoming request already has an `X-Request-ID` header, that value is preserved.

---

## Summary

| Configuration | Method | Key Parameters |
|--------------|--------|----------------|
| CORS | `enable_cors()` | `origins` |
| Compression | `enable_compression()` | `min_size` |
| Logging | `enable_logging()` | None |
| Static Files | `enable_static_files()` | `directory`, `prefix` |
| Body Limit | `enable_body_limit()` | `max_size` |
| Prometheus | `enable_prometheus()` | `endpoint`, `namespace` |
| Caching | `enable_caching()` | `ttl`, `exclude_paths` |
| ETag | `enable_etag()` | None |
| Request ID | `enable_request_id()` | None |
