---
title: OpenTelemetry Integration
description: Distributed tracing and metrics with OpenTelemetry in Cello (v0.7.0)
---

# OpenTelemetry Integration

Cello provides native OpenTelemetry support for distributed tracing, metrics, and context propagation across microservices.

---

## Overview

OpenTelemetry (OTel) is the industry standard for observability. Cello's integration is implemented in Rust for minimal overhead and supports:

- **Distributed tracing** with automatic span creation for each request
- **OTLP export** to any compatible collector (Jaeger, Zipkin, Grafana Tempo, Datadog)
- **Trace context propagation** via W3C `traceparent` headers
- **Configurable sampling** to control trace volume

---

## Configuration

```python
from cello import App, OpenTelemetryConfig

app = App()

app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317",
    sampling_rate=0.1,
))
```

### `OpenTelemetryConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `service_name` | `str` | Required | Name identifying this service in traces |
| `otlp_endpoint` | `str` | `"http://localhost:4317"` | OTLP gRPC endpoint for the collector |
| `sampling_rate` | `float` | `1.0` | Fraction of requests to trace (0.0 to 1.0) |
| `propagation` | `str` | `"w3c"` | Context propagation format (`"w3c"`, `"b3"`, `"jaeger"`) |
| `export_timeout_ms` | `int` | `10000` | Export timeout in milliseconds |
| `batch_size` | `int` | `512` | Maximum batch size for span export |

---

## Automatic Instrumentation

Once enabled, every HTTP request automatically creates a span with the following attributes:

| Attribute | Example |
|-----------|---------|
| `http.method` | `GET` |
| `http.url` | `/api/users/42` |
| `http.status_code` | `200` |
| `http.route` | `/api/users/{id}` |
| `http.request.duration_ms` | `12.5` |
| `service.name` | `my-service` |

---

## Trace Context Propagation

Cello automatically reads and writes the W3C `traceparent` header. When service A calls service B, the trace context is propagated so both requests appear in the same trace.

```
Service A                          Service B
  |                                  |
  |-- POST /orders ----------------->|
  |   traceparent: 00-abc...         |
  |                                  |-- GET /users/1 ---------> Service C
  |                                  |   traceparent: 00-abc...
  |<----- 201 Created --------------|
```

---

## Sampling

Control trace volume with the `sampling_rate` parameter:

| Rate | Behavior |
|------|----------|
| `1.0` | Trace every request (development) |
| `0.1` | Trace 10% of requests (staging) |
| `0.01` | Trace 1% of requests (high-traffic production) |

```python
# Production: trace 5% of requests
app.enable_telemetry(OpenTelemetryConfig(
    service_name="api-gateway",
    otlp_endpoint="http://collector:4317",
    sampling_rate=0.05,
))
```

---

## Collector Setup

### Docker Compose with Jaeger

```yaml
services:
  collector:
    image: otel/opentelemetry-collector:latest
    ports:
      - "4317:4317"   # OTLP gRPC
      - "4318:4318"   # OTLP HTTP
    volumes:
      - ./otel-config.yaml:/etc/otel/config.yaml

  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # Jaeger UI
```

### otel-config.yaml

```yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

exporters:
  jaeger:
    endpoint: jaeger:14250
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [jaeger]
```

---

## Custom Spans

Add custom spans in your handler code for fine-grained tracing:

```python
@app.get("/orders/{id}")
async def get_order(request):
    # The framework creates the parent span automatically
    order = await db.fetch_order(request.params["id"])
    items = await db.fetch_order_items(order["id"])
    return {"order": order, "items": items}
```

Each database call creates a child span if your database driver supports OpenTelemetry.

---

## Combining with Prometheus

OpenTelemetry and Prometheus metrics can run simultaneously. Use OTel for distributed tracing and Prometheus for metrics dashboards.

```python
app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317",
))
app.enable_prometheus(endpoint="/metrics")
```

---

## Next Steps

- See [Distributed Tracing](tracing.md) for details on viewing traces.
- See [Metrics](metrics.md) for Prometheus-based monitoring.
- See [Health Checks](health-checks.md) for Kubernetes integration.
