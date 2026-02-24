---
title: Docker Deployment
description: Containerizing Cello applications with Docker and Docker Compose
---

# Docker Deployment

This guide covers building Docker images for Cello applications, using multi-stage builds for small images, and configuring Docker Compose for multi-service deployments.

---

## Dockerfile

### Multi-Stage Build

```dockerfile
# Stage 1: Install dependencies
FROM python:3.12-slim AS builder
WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir --prefix=/install -r requirements.txt

# Stage 2: Runtime
FROM python:3.12-slim
WORKDIR /app

# Copy installed packages
COPY --from=builder /install /usr/local

# Copy application code
COPY . .

# Non-root user
RUN useradd --create-home appuser
USER appuser

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health/live')"

CMD ["python", "app.py", "--host", "0.0.0.0", "--env", "production", "--workers", "4"]
```

### Key Points

- **Multi-stage build** keeps the final image small by excluding build tools.
- **Non-root user** improves container security.
- **HEALTHCHECK** enables Docker to monitor the container.
- **`--host 0.0.0.0`** is required inside containers to accept external connections.
- **Multi-worker mode** in Docker (Linux) uses `os.fork()` with `SO_REUSEPORT` for optimal performance. The Windows subprocess re-execution path is not used in Linux containers.

---

## .dockerignore

```
.git
.venv
__pycache__
target
*.pyc
.env
.env.*
*.log
```

---

## Docker Compose

### Single Service

```yaml
version: "3.8"
services:
  app:
    build: .
    ports:
      - "8000:8000"
    environment:
      CELLO_ENV: production
      JWT_SECRET: ${JWT_SECRET}
      DATABASE_URL: postgresql://user:pass@db:5432/app
    restart: unless-stopped
    depends_on:
      db:
        condition: service_healthy

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: user
      POSTGRES_PASSWORD: pass
      POSTGRES_DB: app
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U user"]
      interval: 10s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
```

### Multi-Service (Microservices)

```yaml
version: "3.8"
services:
  user-service:
    build:
      context: ./user-service
    ports:
      - "8001:8000"
    environment:
      CELLO_ENV: production
      JWT_SECRET: ${JWT_SECRET}

  order-service:
    build:
      context: ./order-service
    ports:
      - "8002:8000"
    environment:
      CELLO_ENV: production
      JWT_SECRET: ${JWT_SECRET}
      USER_SERVICE_URL: http://user-service:8000

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
    depends_on:
      - user-service
      - order-service
```

---

## Environment Variables

Pass secrets via environment variables, not build args.

```bash
# .env file (not committed to version control)
JWT_SECRET=your-production-secret
DATABASE_URL=postgresql://user:pass@db:5432/app
```

```bash
docker compose --env-file .env up -d
```

---

## Building and Running

```bash
# Build the image
docker build -t cello-app .

# Run the container
docker run -d -p 8000:8000 \
  -e CELLO_ENV=production \
  -e JWT_SECRET=secret \
  --name cello-app \
  cello-app

# View logs
docker logs -f cello-app

# Stop
docker stop cello-app
```

---

## Image Optimization

| Technique | Impact |
|-----------|--------|
| Multi-stage build | Reduces image size by 50-70% |
| `python:3.12-slim` base | Smaller than `python:3.12` |
| `--no-cache-dir` on pip | Saves ~50 MB |
| `.dockerignore` | Excludes unnecessary files from build context |
| Non-root user | Improves security posture |

---

## Next Steps

- See [Kubernetes deployment](kubernetes.md) for orchestrating containers.
- See [Health Checks](../observability/health-checks.md) for container health monitoring.
- See the [Deployment guide](../../learn/guides/deployment.md) for production configuration.
