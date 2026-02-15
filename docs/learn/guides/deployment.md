---
title: Deployment
description: Deploying Cello applications to production with Docker, TLS, workers, and reverse proxies
---

# Deployment

This guide covers everything you need to deploy a Cello application in production: server configuration, worker processes, Docker packaging, TLS/HTTPS, reverse proxy setup, health checks, and monitoring.

---

## Production Settings

### Command-Line Options

```bash
python app.py \
  --host 0.0.0.0 \
  --port 8000 \
  --env production \
  --workers 4 \
  --no-logs
```

| Flag | Default | Description |
|------|---------|-------------|
| `--host` | `127.0.0.1` | Bind address. Use `0.0.0.0` in containers. |
| `--port` | `8000` | Listening port |
| `--env` | `development` | Set to `production` to disable debug mode |
| `--workers` | CPU count | Number of worker threads |
| `--debug` | Off in prod | Enable verbose error pages |
| `--no-logs` | Off | Disable request logging |

### Programmatic Configuration

```python
app.run(
    host="0.0.0.0",
    port=8000,
    env="production",
    workers=8,
    logs=False,
)
```

---

## Workers Configuration

Cello uses Tokio worker threads inside Rust. The `--workers` flag controls how many OS threads handle requests concurrently.

**Rule of thumb:** Set workers to the number of CPU cores.

```bash
python app.py --workers $(nproc)
```

For I/O-bound workloads (database queries, external API calls), you can increase workers to 2x the CPU count.

---

## Environment Variables

Use environment variables for configuration that changes between environments.

```bash
export CELLO_ENV=production
export DATABASE_URL=postgresql://user:pass@db:5432/app
export JWT_SECRET=$(python -c "import secrets; print(secrets.token_urlsafe(64))")
export WORKERS=4
```

Read them in your application:

```python
import os

app.run(
    host=os.environ.get("HOST", "0.0.0.0"),
    port=int(os.environ.get("PORT", "8000")),
    env=os.environ.get("CELLO_ENV", "production"),
    workers=int(os.environ.get("WORKERS", "4")),
)
```

---

## TLS / HTTPS

Cello supports TLS natively through Rustls (no OpenSSL dependency).

```python
from cello import App, TlsConfig

app = App()

tls = TlsConfig(
    cert_path="/etc/ssl/certs/server.crt",
    key_path="/etc/ssl/private/server.key",
)

app.run(host="0.0.0.0", port=443, tls=tls)
```

For Let's Encrypt certificates, point `cert_path` and `key_path` to the fullchain and private key files.

---

## Docker Deployment

### Dockerfile

```dockerfile
# Build stage
FROM python:3.12-slim AS builder
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Runtime stage
FROM python:3.12-slim
WORKDIR /app
COPY --from=builder /usr/local/lib/python3.12/site-packages /usr/local/lib/python3.12/site-packages
COPY . .

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
  CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health/live')"

CMD ["python", "app.py", "--host", "0.0.0.0", "--env", "production", "--workers", "4"]
```

### docker-compose.yml

```yaml
version: "3.8"
services:
  app:
    build: .
    ports:
      - "8000:8000"
    environment:
      CELLO_ENV: production
      DATABASE_URL: postgresql://user:pass@db:5432/app
      JWT_SECRET: change-me-in-production
    depends_on:
      - db
    restart: unless-stopped

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: user
      POSTGRES_PASSWORD: pass
      POSTGRES_DB: app
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
```

---

## Reverse Proxy (nginx)

In production, place nginx in front of Cello for TLS termination, static files, and load balancing.

### nginx.conf

```nginx
upstream cello {
    server 127.0.0.1:8000;
}

server {
    listen 443 ssl http2;
    server_name api.example.com;

    ssl_certificate     /etc/ssl/certs/fullchain.pem;
    ssl_certificate_key /etc/ssl/private/privkey.pem;

    location / {
        proxy_pass http://cello;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /ws/ {
        proxy_pass http://cello;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

!!! note
    The `/ws/` location block is required for WebSocket connections. Without the `Upgrade` headers, WebSocket handshakes fail.

---

## Health Checks

Enable health check endpoints for orchestrators like Kubernetes and Docker.

```python
from cello import HealthCheckConfig

app.enable_health_checks(HealthCheckConfig(
    base_path="/health",
    include_system_info=True,
))
```

This registers:

| Endpoint | Purpose |
|----------|---------|
| `GET /health/live` | Liveness probe -- is the process alive? |
| `GET /health/ready` | Readiness probe -- is it ready for traffic? |
| `GET /health/startup` | Startup probe -- has initialization completed? |
| `GET /health` | Full health report with system info |

---

## Monitoring

### Prometheus Metrics

```python
app.enable_prometheus(endpoint="/metrics")
```

Scrape `/metrics` from your Prometheus server. The endpoint exposes request count, latency histograms, and error rates.

### OpenTelemetry

For distributed tracing across microservices:

```python
from cello import OpenTelemetryConfig

app.enable_telemetry(OpenTelemetryConfig(
    service_name="my-service",
    otlp_endpoint="http://collector:4317",
))
```

See the [OpenTelemetry guide](../../enterprise/observability/opentelemetry.md) for details.

---

## Pre-Deployment Checklist

- [ ] Set `--env production`
- [ ] Configure `--workers` based on CPU count
- [ ] Enable TLS (directly or via reverse proxy)
- [ ] Set `JWT_SECRET` and other secrets via environment variables
- [ ] Enable health checks
- [ ] Enable Prometheus metrics
- [ ] Enable rate limiting on public endpoints
- [ ] Enable security headers
- [ ] Test with production-like load before going live
- [ ] Set up log aggregation (stdout logs are collected by Docker/K8s)

---

## Next Steps

- See the [Docker guide](../../enterprise/deployment/docker.md) for multi-stage builds and optimization.
- See the [Kubernetes guide](../../enterprise/deployment/kubernetes.md) for Deployment manifests and HPA.
- Read the [Performance guide](performance.md) for tuning tips.
