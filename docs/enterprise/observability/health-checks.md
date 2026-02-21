---
title: Health Checks
description: Kubernetes-compatible health check endpoints for Cello (v0.7.0)
---

# Health Checks

Cello provides built-in health check endpoints compatible with Kubernetes liveness, readiness, and startup probes.

---

## Enabling Health Checks

```python
from cello import App, HealthCheckConfig

app = App()

app.enable_health_checks(HealthCheckConfig(
    base_path="/health",
    include_system_info=True,
))
```

### `HealthCheckConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_path` | `str` | `"/health"` | Base URL path for health endpoints |
| `include_system_info` | `bool` | `False` | Include system metrics (uptime, memory) in the full report |

---

## Endpoints

### `GET /health/live` -- Liveness Probe

Returns `200 OK` if the process is running. This is the simplest check.

```json
{"status": "ok"}
```

Use this for Kubernetes liveness probes. If it fails, the container is restarted.

### `GET /health/ready` -- Readiness Probe

Returns `200 OK` when the service is ready to accept traffic.

```json
{"status": "ok"}
```

Use this for Kubernetes readiness probes. If it fails, the pod is removed from the load balancer.

### `GET /health/startup` -- Startup Probe

Returns `200 OK` when the service has completed initialization.

```json
{"status": "ok"}
```

Use this for Kubernetes startup probes. Prevents liveness checks from running during slow startup.

### `GET /health` -- Full Health Report

Returns a detailed health report with all checks and optional system information.

```json
{
    "status": "healthy",
    "checks": {
        "liveness": "ok",
        "readiness": "ok",
        "startup": "ok"
    },
    "system": {
        "uptime_seconds": 3600,
        "memory_mb": 45
    }
}
```

The `system` section is only included when `include_system_info=True`.

---

## Kubernetes Integration

### Deployment YAML

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cello-app
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: app
          image: myapp:latest
          ports:
            - containerPort: 8000
          livenessProbe:
            httpGet:
              path: /health/live
              port: 8000
            initialDelaySeconds: 5
            periodSeconds: 10
            failureThreshold: 3
          readinessProbe:
            httpGet:
              path: /health/ready
              port: 8000
            initialDelaySeconds: 5
            periodSeconds: 5
            failureThreshold: 3
          startupProbe:
            httpGet:
              path: /health/startup
              port: 8000
            initialDelaySeconds: 0
            periodSeconds: 2
            failureThreshold: 30
```

---

## Docker Health Checks

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health/live')"
```

---

## Custom Health Indicators

Add custom checks for dependencies like databases, caches, or external services:

```python
@app.get("/health/db")
async def db_health(request):
    try:
        await db.execute("SELECT 1")
        return {"database": "ok"}
    except Exception as e:
        return Response.json({"database": "error", "detail": str(e)}, status=503)
```

---

## Health Check Response Codes

| Endpoint | Healthy | Unhealthy |
|----------|---------|-----------|
| `/health/live` | `200` | `503` |
| `/health/ready` | `200` | `503` |
| `/health/startup` | `200` | `503` |
| `/health` | `200` | `503` |

---

## Next Steps

- See the [Kubernetes deployment guide](../deployment/kubernetes.md) for full manifests.
- See [Metrics](metrics.md) to monitor health check latency.
- See the [Docker deployment guide](../deployment/docker.md) for container health checks.
