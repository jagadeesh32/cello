---
title: Migration Guide
---

# Migration Guide

This guide helps you migrate between major versions of Cello.

## 0.6.x to 0.7.x {#06x-to-07x}

### New Features

Version 0.7.0 adds enterprise features:

- OpenTelemetry distributed tracing
- Kubernetes-compatible health checks
- Database connection pooling
- GraphQL support

### Breaking Changes

No breaking changes in 0.7.0. All existing code continues to work.

### New APIs

```python
from cello import App, OpenTelemetryConfig, HealthCheckConfig, GraphQLConfig

app = App()

# Enable telemetry
app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317"
))

# Enable health checks
app.enable_health_checks(HealthCheckConfig(
    base_path="/health",
    include_details=True
))

# Enable GraphQL
app.enable_graphql(GraphQLConfig(
    path="/graphql",
    playground=True
))
```

## 0.5.x to 0.6.x {#05x-to-06x}

### New Features

Version 0.6.0 introduced:

- Guards and RBAC
- Rate limiting with multiple algorithms
- Circuit breaker pattern
- Prometheus metrics
- Caching middleware

### Breaking Changes

No breaking changes in 0.6.0.

### New APIs

```python
from cello import App, RateLimitConfig

app = App()

# Enable rate limiting
app.enable_rate_limit(RateLimitConfig.token_bucket(
    capacity=100,
    refill_rate=10
))

# Enable Prometheus metrics
app.enable_prometheus(endpoint="/metrics")

# Add guards
@app.add_guard
def require_auth(request):
    return request.headers.get("Authorization") is not None
```

## 0.4.x to 0.5.x {#04x-to-05x}

### New Features

Version 0.5.0 introduced:

- Background tasks
- Template engine
- OpenAPI documentation

### Breaking Changes

No breaking changes in 0.5.0.
