---
title: Distributed Tracing
description: Trace context, span creation, and viewing traces in Jaeger and Zipkin
---

# Distributed Tracing

Distributed tracing follows a request as it flows through multiple services. Cello creates spans automatically for each HTTP request and propagates trace context to downstream services.

---

## How It Works

1. A request arrives at the first service. Cello creates a **root span**.
2. The span ID is attached to the response and propagated to any outgoing HTTP calls via the `traceparent` header.
3. Downstream services read the `traceparent` header and create **child spans** under the same trace.
4. All spans are exported to a collector and visualized in a tracing backend.

```
Client --> [API Gateway] --> [User Service] --> [Database]
              span A            span B            span C
              \___________________________________/
                        single trace
```

---

## Enabling Tracing

```python
from cello import App, OpenTelemetryConfig

app = App()
app.enable_telemetry(OpenTelemetryConfig(
    service_name="api-gateway",
    otlp_endpoint="http://collector:4317",
    sampling_rate=0.1,
))
```

See the [OpenTelemetry page](opentelemetry.md) for full configuration options.

---

## Trace Context Propagation

Cello supports the W3C `traceparent` header format by default:

```
traceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
```

| Field | Description |
|-------|-------------|
| Version | Always `00` |
| Trace ID | 32-character hex string identifying the trace |
| Parent Span ID | 16-character hex string for the parent span |
| Trace Flags | `01` = sampled |

When making outgoing HTTP calls from your handler, include the trace context:

```python
import urllib.request

@app.get("/orders/{id}")
async def get_order(request):
    # Forward trace context to downstream service
    traceparent = request.get_header("traceparent")
    req = urllib.request.Request(f"http://user-service:8001/users/{user_id}")
    if traceparent:
        req.add_header("traceparent", traceparent)
    # ...
```

---

## Span Attributes

Each automatically created span includes:

| Attribute | Description | Example |
|-----------|-------------|---------|
| `http.method` | HTTP method | `GET` |
| `http.route` | Route pattern | `/users/{id}` |
| `http.url` | Full URL path | `/users/42` |
| `http.status_code` | Response status | `200` |
| `http.request_content_length` | Request body size | `256` |
| `http.response_content_length` | Response body size | `1024` |
| `net.host.name` | Server hostname | `api-gateway` |
| `service.name` | Service name from config | `api-gateway` |

---

## Viewing Traces in Jaeger

1. Start Jaeger:

```bash
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 14250:14250 \
  jaegertracing/all-in-one:latest
```

2. Configure the OTel collector to export to Jaeger.

3. Open the Jaeger UI at `http://localhost:16686`.

4. Select your service name from the dropdown and click "Find Traces".

Each trace shows a waterfall diagram of all spans, their durations, and parent-child relationships.

---

## Viewing Traces in Zipkin

1. Start Zipkin:

```bash
docker run -d --name zipkin -p 9411:9411 openzipkin/zipkin
```

2. Configure the OTel collector with a Zipkin exporter.

3. Open the Zipkin UI at `http://localhost:9411`.

---

## Request ID Integration

When both tracing and request ID middleware are enabled, the request ID is included as a span attribute. This lets you correlate log entries with specific spans.

```python
app.enable_request_id()
app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317",
))
```

---

## Performance Impact

Tracing is implemented in Rust with asynchronous span export. The overhead per request is typically under 50 microseconds. Use `sampling_rate` to reduce the volume of exported spans in high-traffic environments.

| Sampling Rate | Overhead | Trace Volume |
|--------------|----------|--------------|
| `1.0` | Highest | All requests |
| `0.1` | Low | 10% of requests |
| `0.01` | Minimal | 1% of requests |

---

## Next Steps

- See [OpenTelemetry](opentelemetry.md) for collector setup and configuration.
- See [Metrics](metrics.md) for Prometheus-based dashboards.
- See the [Microservices tutorial](../../learn/tutorials/microservices.md) for a multi-service tracing example.
