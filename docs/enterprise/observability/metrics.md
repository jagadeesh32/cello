---
title: Metrics
description: Prometheus metrics collection and Grafana dashboards for Cello
---

# Metrics

Cello includes a built-in Prometheus metrics middleware that exposes request counts, latency histograms, and active connection gauges at a configurable endpoint.

---

## Enabling Prometheus Metrics

```python
from cello import App

app = App()

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

---

## Exposed Metrics

All metric names follow the pattern `{namespace}_{subsystem}_{metric_name}`.

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `cello_http_requests_total` | Counter | Total number of requests | `method`, `status`, `path` |
| `cello_http_request_duration_seconds` | Histogram | Request latency distribution | `method`, `path` |
| `cello_http_requests_in_flight` | Gauge | Currently active requests | None |
| `cello_http_response_size_bytes` | Histogram | Response body size distribution | `method`, `path` |

---

## Scraping with Prometheus

Add the Cello application to your Prometheus configuration:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: cello-app
    static_configs:
      - targets: ["app:8000"]
    metrics_path: /metrics
    scrape_interval: 15s
```

### Docker Compose Example

```yaml
services:
  app:
    build: .
    ports:
      - "8000:8000"

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      GF_SECURITY_ADMIN_PASSWORD: admin
```

---

## Grafana Dashboards

### Request Rate

```
rate(cello_http_requests_total[5m])
```

### Error Rate

```
rate(cello_http_requests_total{status=~"5.."}[5m])
```

### P99 Latency

```
histogram_quantile(0.99, rate(cello_http_request_duration_seconds_bucket[5m]))
```

### Active Connections

```
cello_http_requests_in_flight
```

### Request Rate by Endpoint

```
sum by (path) (rate(cello_http_requests_total[5m]))
```

---

## Custom Metrics

While Cello's built-in metrics cover HTTP-level observability, you can add application-specific metrics using the `prometheus_client` Python library alongside Cello's Rust metrics.

```python
from prometheus_client import Counter, generate_latest

orders_created = Counter("orders_created_total", "Total orders created")

@app.post("/orders")
def create_order(request):
    order = process_order(request.json())
    orders_created.inc()
    return order
```

---

## Alerting Rules

Example Prometheus alerting rules for a Cello application:

```yaml
groups:
  - name: cello-alerts
    rules:
      - alert: HighErrorRate
        expr: rate(cello_http_requests_total{status=~"5.."}[5m]) > 0.05
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High error rate on {{ $labels.instance }}"

      - alert: HighLatency
        expr: histogram_quantile(0.95, rate(cello_http_request_duration_seconds_bucket[5m])) > 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P95 latency above 1s on {{ $labels.instance }}"
```

---

## Excluding Paths

The `/metrics` endpoint itself is excluded from metrics collection to avoid self-referential loops. Health check endpoints are also typically excluded:

```python
app.enable_caching(exclude_paths=["/metrics", "/health"])
```

---

## Next Steps

- See [OpenTelemetry](opentelemetry.md) for distributed tracing.
- See [Health Checks](health-checks.md) for Kubernetes probes.
- See the [Deployment guide](../../learn/guides/deployment.md) for production monitoring setup.
