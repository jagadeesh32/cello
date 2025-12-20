# Advanced Features

Cello v0.5.0 includes advanced features for production deployments.

## Overview

| Feature | Description |
|---------|-------------|
| Cluster Mode | Multi-worker process deployment |
| TLS/SSL | Native HTTPS support |
| HTTP/2 | Modern HTTP/2 protocol |
| HTTP/3 | QUIC-based HTTP/3 |
| Lifecycle Hooks | Startup/shutdown events |
| Request Context | Dependency injection |

## Cluster Mode

Run multiple worker processes for better performance:

```python
from cello import ClusterConfig

# Auto-detect CPU count
cluster = ClusterConfig.auto()

# Manual configuration
cluster = ClusterConfig(
    workers=4,                  # 4 worker processes
    cpu_affinity=True,          # Pin to CPU cores
    max_restarts=10,            # Restart failed workers
    graceful_shutdown=True,     # Graceful shutdown
    shutdown_timeout=30,        # 30 second grace period
)
```

### Running with Workers

```bash
# CLI argument
python app.py --workers 4

# Or in code (will use CLI override if specified)
app.run(workers=4)
```

### Graceful Shutdown

Cello supports graceful shutdown:

1. Stop accepting new connections
2. Wait for in-flight requests to complete
3. Run shutdown hooks
4. Exit cleanly

Handle shutdown in your app:

```python
import signal

def handle_shutdown():
    print("Shutting down gracefully...")
    # Close database connections
    # Flush caches
    # etc.

signal.signal(signal.SIGTERM, lambda s, f: handle_shutdown())
```

## TLS/SSL Configuration

Enable HTTPS with TLS:

```python
from cello import TlsConfig

tls = TlsConfig(
    cert_path="/etc/ssl/certs/server.crt",
    key_path="/etc/ssl/private/server.key",
    ca_path="/etc/ssl/certs/ca.crt",  # For client certs
    min_version="1.2",                 # TLS 1.2 minimum
    max_version="1.3",                 # TLS 1.3 maximum
    require_client_cert=False,
)
```

### Certificate Generation (for testing)

```bash
# Generate self-signed certificate
openssl req -x509 -newkey rsa:4096 \
    -keyout key.pem -out cert.pem \
    -days 365 -nodes \
    -subj "/CN=localhost"
```

### Let's Encrypt (for production)

```bash
# Using certbot
sudo certbot certonly --standalone -d example.com

# Certificate paths
# /etc/letsencrypt/live/example.com/fullchain.pem
# /etc/letsencrypt/live/example.com/privkey.pem
```

## HTTP/2 Configuration

Enable HTTP/2 for improved performance:

```python
from cello import Http2Config

http2 = Http2Config(
    max_concurrent_streams=100,     # Concurrent streams
    initial_window_size=1048576,    # 1MB flow control window
    max_frame_size=16384,           # Frame size
    enable_push=False,              # Server push (usually disabled)
)
```

### HTTP/2 Benefits

- **Multiplexing** - Multiple requests on single connection
- **Header Compression** - Reduced overhead
- **Stream Prioritization** - Important requests first
- **Binary Protocol** - Efficient parsing

## HTTP/3 Configuration

Enable HTTP/3 (QUIC) for best performance:

```python
from cello import Http3Config

http3 = Http3Config(
    max_idle_timeout=30,            # Idle timeout
    max_udp_payload_size=1350,      # UDP payload size
    initial_max_streams_bidi=100,   # Bidirectional streams
    enable_0rtt=False,              # 0-RTT (security trade-off)
)
```

### HTTP/3 Benefits

- **Built on QUIC** - UDP-based transport
- **Connection Migration** - Survives IP changes
- **Reduced Latency** - No head-of-line blocking
- **Built-in Encryption** - TLS 1.3 by default

## Timeout Configuration

Configure request/response timeouts:

```python
from cello import TimeoutConfig

timeouts = TimeoutConfig(
    read_header=10,     # 10 seconds for headers
    read_body=60,       # 60 seconds for body
    write=60,           # 60 seconds for response
    idle=120,           # 120 seconds idle
    handler=30,         # 30 seconds for handler
)
```

### Per-Route Timeouts

Configure different timeouts for specific routes:

```python
# Use longer timeout for file uploads
@app.post("/upload")
def upload_file(request):
    # This route might need longer timeout
    # Configure at server level or handle in middleware
    file_data = request.body()
    return {"size": len(file_data)}
```

## Limits Configuration

Configure connection and request limits:

```python
from cello import LimitsConfig

limits = LimitsConfig(
    max_header_size=16384,          # 16KB headers
    max_body_size=52428800,         # 50MB body
    max_connections=50000,          # 50k connections
    max_requests_per_connection=1000,
)
```

## Request Context

Store request-scoped data:

```python
# In Rust, the context module provides:
# - RequestContext: Request-scoped storage
# - AppState: Application-wide singletons
# - Container: Dependency injection

# Python usage pattern:
@app.get("/api/data")
def with_context(request):
    # Access request metadata
    method = request.method
    path = request.path
    
    # Store custom data in request
    # (available through middleware patterns)
    
    return {"method": method, "path": path}
```

## Lifecycle Hooks

The lifecycle module provides hooks for:

### Startup Hooks
Run code when server starts:

```python
# Startup hook pattern
def on_startup():
    print("Server starting...")
    # Initialize database connections
    # Load configuration
    # Warm up caches

# Run before app.run()
on_startup()
```

### Shutdown Hooks
Run code when server stops:

```python
import atexit

def on_shutdown():
    print("Server stopping...")
    # Close database connections
    # Flush logs
    # Cleanup resources

atexit.register(on_shutdown)
```

### Signal Handlers
Handle Unix signals:

```python
import signal

def handle_sighup(signum, frame):
    print("Received SIGHUP - reloading config...")
    # Reload configuration

def handle_sigusr1(signum, frame):
    print("Received SIGUSR1 - dumping stats...")
    # Dump statistics

signal.signal(signal.SIGHUP, handle_sighup)
signal.signal(signal.SIGUSR1, handle_sigusr1)
```

## Health Checks

Implement health checks for load balancers:

```python
@app.get("/health")
def health_check(request):
    """Liveness probe - is the server running?"""
    return {"status": "healthy"}

@app.get("/ready")
def readiness_check(request):
    """Readiness probe - can it handle traffic?"""
    # Check dependencies
    checks = {
        "database": check_database(),
        "cache": check_cache(),
    }
    
    if all(checks.values()):
        return {"status": "ready", "checks": checks}
    else:
        return Response.json(
            {"status": "not_ready", "checks": checks},
            status=503
        )
```

## Metrics

Expose metrics for monitoring:

```python
import os
import time

start_time = time.time()
request_count = 0

@app.get("/metrics")
def metrics(request):
    """Prometheus-style metrics."""
    global request_count
    request_count += 1
    
    uptime = time.time() - start_time
    
    metrics_text = f"""# HELP cello_requests_total Total requests
# TYPE cello_requests_total counter
cello_requests_total {request_count}

# HELP cello_uptime_seconds Server uptime
# TYPE cello_uptime_seconds gauge
cello_uptime_seconds {uptime:.2f}

# HELP cello_worker_pid Worker process ID
# TYPE cello_worker_pid gauge
cello_worker_pid {os.getpid()}
"""
    
    response = Response.text(metrics_text)
    response.set_header("Content-Type", "text/plain; version=0.0.4")
    return response
```

## Production Checklist

### Security
- [ ] Enable TLS/HTTPS
- [ ] Configure security headers
- [ ] Set up rate limiting
- [ ] Validate all input
- [ ] Use strong secrets

### Performance
- [ ] Enable compression
- [ ] Configure workers (CPU count)
- [ ] Set appropriate timeouts
- [ ] Enable HTTP/2

### Reliability
- [ ] Enable graceful shutdown
- [ ] Set up health checks
- [ ] Configure monitoring
- [ ] Set up logging
- [ ] Plan for restarts

### Deployment
- [ ] Use systemd/supervisor
- [ ] Configure reverse proxy
- [ ] Set up SSL certificates
- [ ] Configure firewalls
