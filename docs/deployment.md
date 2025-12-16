# Deployment

This guide covers deploying Cello applications to production.

## Quick Start

### Basic Production Run

```bash
python app.py --env production --workers 4 --port 8080
```

### With Custom Host (for Docker)

```bash
python app.py --host 0.0.0.0 --port 8080 --workers 4
```

## CLI Options

| Argument | Default | Description |
|----------|---------|-------------|
| `--host` | 127.0.0.1 | Host to bind to |
| `--port` | 8000 | Port to bind to |
| `--env` | development | Environment mode |
| `--workers` | CPU count | Number of workers |
| `--reload` | False | Hot reload (dev only) |
| `--debug` | Auto | Debug mode |
| `--no-logs` | False | Disable logging |

## Systemd Service

Create `/etc/systemd/system/cello.service`:

```ini
[Unit]
Description=Cello Web Application
After=network.target

[Service]
Type=simple
User=www-data
Group=www-data
WorkingDirectory=/opt/myapp
Environment=PATH=/opt/myapp/venv/bin
ExecStart=/opt/myapp/venv/bin/python app.py --env production --workers 4 --port 8080
Restart=always
RestartSec=5

# Resource limits
LimitNOFILE=65535

# Security
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/opt/myapp/data

[Install]
WantedBy=multi-user.target
```

### Manage the Service

```bash
# Enable and start
sudo systemctl enable cello
sudo systemctl start cello

# Check status
sudo systemctl status cello

# View logs
sudo journalctl -u cello -f

# Restart
sudo systemctl restart cello
```

## Supervisor

Create `/etc/supervisor/conf.d/cello.conf`:

```ini
[program:cello]
command=/opt/myapp/venv/bin/python app.py --env production --workers 4
directory=/opt/myapp
user=www-data
autostart=true
autorestart=true
stopasgroup=true
killasgroup=true
stdout_logfile=/var/log/cello/stdout.log
stderr_logfile=/var/log/cello/stderr.log
environment=PATH="/opt/myapp/venv/bin"
```

### Manage with Supervisor

```bash
sudo supervisorctl reread
sudo supervisorctl update
sudo supervisorctl start cello
sudo supervisorctl status
```

## Docker

### Dockerfile

```dockerfile
FROM python:3.12-slim

WORKDIR /app

# Install build dependencies (for Rust extension)
RUN apt-get update && apt-get install -y \
    curl \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install Rust (if building from source)
# RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
# ENV PATH="/root/.cargo/bin:${PATH}"

# Install Python dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application
COPY . .

# Expose port
EXPOSE 8080

# Run application
CMD ["python", "app.py", "--host", "0.0.0.0", "--port", "8080", "--workers", "4"]
```

### requirements.txt

```
cello-framework>=0.4.0
```

### Docker Compose

```yaml
version: '3.8'

services:
  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      - CELLO_ENV=production
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
    restart: always
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
```

### Build and Run

```bash
# Build
docker build -t myapp .

# Run
docker run -d -p 8080:8080 --name myapp myapp

# With compose
docker-compose up -d
```

## Kubernetes

### Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cello-app
  labels:
    app: cello-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: cello-app
  template:
    metadata:
      labels:
        app: cello-app
    spec:
      containers:
      - name: cello
        image: myapp:latest
        ports:
        - containerPort: 8080
        env:
        - name: CELLO_ENV
          value: "production"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
```

### Service

```yaml
apiVersion: v1
kind: Service
metadata:
  name: cello-service
spec:
  selector:
    app: cello-app
  ports:
  - port: 80
    targetPort: 8080
  type: ClusterIP
```

### Ingress

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: cello-ingress
  annotations:
    kubernetes.io/ingress.class: nginx
    cert-manager.io/cluster-issuer: letsencrypt-prod
spec:
  tls:
  - hosts:
    - example.com
    secretName: cello-tls
  rules:
  - host: example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: cello-service
            port:
              number: 80
```

## Nginx Reverse Proxy

### Configuration

```nginx
upstream cello {
    server 127.0.0.1:8080;
    keepalive 32;
}

server {
    listen 80;
    server_name example.com;
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name example.com;

    ssl_certificate /etc/letsencrypt/live/example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/example.com/privkey.pem;

    # Security headers (if not handled by Cello)
    add_header X-Frame-Options DENY;
    add_header X-Content-Type-Options nosniff;

    location / {
        proxy_pass http://cello;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_cache_bypass $http_upgrade;
        
        # Timeouts
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }

    # Static files (optional)
    location /static {
        alias /opt/myapp/static;
        expires 30d;
        add_header Cache-Control "public, immutable";
    }

    # Health check
    location /health {
        proxy_pass http://cello;
        access_log off;
    }
}
```

## Health Checks

Implement health endpoints:

```python
@app.get("/health")
def health_check(request):
    """Liveness probe - is the server alive?"""
    return {"status": "healthy"}

@app.get("/ready")
def readiness_check(request):
    """Readiness probe - can handle traffic?"""
    # Check dependencies
    db_ok = check_database()
    cache_ok = check_cache()
    
    if db_ok and cache_ok:
        return {"status": "ready"}
    else:
        return Response.json({"status": "not_ready"}, status=503)
```

## Logging

### Structured Logging

```python
import logging
import json

class JSONFormatter(logging.Formatter):
    def format(self, record):
        return json.dumps({
            "timestamp": self.formatTime(record),
            "level": record.levelname,
            "message": record.getMessage(),
            "module": record.module,
        })

# Configure logging
handler = logging.StreamHandler()
handler.setFormatter(JSONFormatter())
logging.basicConfig(handlers=[handler], level=logging.INFO)
```

### Log to File

```python
import logging
from logging.handlers import RotatingFileHandler

handler = RotatingFileHandler(
    "/var/log/cello/app.log",
    maxBytes=10_000_000,  # 10MB
    backupCount=5
)
logging.basicConfig(handlers=[handler], level=logging.INFO)
```

## Environment Variables

```python
import os

# Configuration from environment
config = {
    "host": os.environ.get("CELLO_HOST", "0.0.0.0"),
    "port": int(os.environ.get("CELLO_PORT", "8080")),
    "workers": int(os.environ.get("CELLO_WORKERS", "4")),
    "debug": os.environ.get("CELLO_DEBUG", "false").lower() == "true",
    "db_url": os.environ.get("DATABASE_URL"),
    "secret_key": os.environ.get("SECRET_KEY"),
}

# Validate required settings
if not config["secret_key"]:
    raise RuntimeError("SECRET_KEY environment variable required")

app.run(
    host=config["host"],
    port=config["port"],
    workers=config["workers"],
    debug=config["debug"],
)
```

## Production Checklist

### Before Deployment

- [ ] Set `--env production`
- [ ] Configure appropriate workers
- [ ] Set up TLS/HTTPS
- [ ] Configure security headers
- [ ] Set up rate limiting
- [ ] Configure logging
- [ ] Set up monitoring

### Security

- [ ] Use HTTPS in production
- [ ] Set secure cookie attributes
- [ ] Configure CORS properly
- [ ] Use environment variables for secrets
- [ ] Enable security headers

### Performance

- [ ] Enable gzip compression
- [ ] Set appropriate timeouts
- [ ] Configure worker count
- [ ] Enable HTTP/2 if supported

### Reliability

- [ ] Set up health checks
- [ ] Configure graceful shutdown
- [ ] Set up log rotation
- [ ] Configure restart policies
- [ ] Set up monitoring alerts
