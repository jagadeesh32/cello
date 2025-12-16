#!/usr/bin/env python3
"""
Cluster Mode & Production Deployment Demo for Cello v0.4.0.

This example demonstrates production deployment configurations including:
- Cluster mode (multi-worker processes)
- Graceful shutdown handling
- Protocol configuration (HTTP/2, HTTP/3)
- TLS/SSL configuration patterns
- Production-ready settings

Run with:
    python examples/cluster_demo.py

For production with multiple workers:
    python examples/cluster_demo.py --workers 4

Note: Some features like TLS require proper certificates.
"""

from cello import App, Response

# Production configuration imports
from cello import (
    TimeoutConfig,
    LimitsConfig,
    ClusterConfig,
    TlsConfig,
    Http2Config,
    Http3Config,
)

app = App()

# Enable middleware for production
app.enable_cors()
app.enable_logging()
app.enable_compression(min_size=512)


# =============================================================================
# Production Configurations
# =============================================================================

# Timeout configuration - tuned for production
production_timeouts = TimeoutConfig(
    read_header=10,      # 10 seconds for headers
    read_body=60,        # 60 seconds for large uploads
    write=60,            # 60 seconds for large responses
    idle=120,            # 120 seconds idle timeout
    handler=30,          # 30 seconds handler timeout
)

# Limits configuration - production settings
production_limits = LimitsConfig(
    max_header_size=16384,          # 16KB headers
    max_body_size=52428800,         # 50MB body (for file uploads)
    max_connections=50000,          # 50k concurrent connections
    max_requests_per_connection=1000,  # Keep-alive limit
)

# Cluster configuration - auto-detect CPUs
cluster_config = ClusterConfig.auto()

# Cluster configuration - manual settings
cluster_config_manual = ClusterConfig(
    workers=4,                  # 4 worker processes
    cpu_affinity=True,          # Pin to CPU cores for performance
    max_restarts=10,            # Restart worker up to 10 times
    graceful_shutdown=True,     # Enable graceful shutdown
    shutdown_timeout=30,        # 30 second grace period
)

# TLS configuration - example (requires actual certificates)
tls_config = TlsConfig(
    cert_path="/etc/ssl/certs/server.crt",
    key_path="/etc/ssl/private/server.key",
    ca_path="/etc/ssl/certs/ca.crt",  # For client cert verification
    min_version="1.2",
    max_version="1.3",
    require_client_cert=False,
)

# HTTP/2 configuration - optimized for performance
http2_config = Http2Config(
    max_concurrent_streams=250,     # More concurrent streams
    initial_window_size=2097152,    # 2MB window
    max_frame_size=32768,           # 32KB frames
    enable_push=False,              # Server push disabled
)

# HTTP/3 configuration - QUIC settings
http3_config = Http3Config(
    max_idle_timeout=60,
    max_udp_payload_size=1350,
    initial_max_streams_bidi=200,
    enable_0rtt=False,  # More secure without 0-RTT
)


# =============================================================================
# Routes
# =============================================================================


@app.get("/")
def home(request):
    """Home endpoint with cluster information."""
    return {
        "message": "Cello Cluster Mode Demo",
        "version": "0.4.0",
        "deployment": {
            "mode": "cluster",
            "features": [
                "Multi-worker processes",
                "Graceful shutdown",
                "CPU affinity",
                "Auto-restart on failure",
            ],
        },
        "endpoints": [
            "GET  /              - This overview",
            "GET  /health        - Health check",
            "GET  /ready         - Readiness probe",
            "GET  /config        - Configuration info",
            "GET  /worker-info   - Worker process info",
        ],
    }


@app.get("/health")
def health_check(request):
    """
    Health check endpoint for load balancers.
    
    Returns 200 if the service is running.
    Use this for Kubernetes liveness probes.
    """
    return {"status": "healthy", "service": "cello"}


@app.get("/ready")
def readiness_check(request):
    """
    Readiness check endpoint.
    
    Returns 200 if the service is ready to accept traffic.
    Use this for Kubernetes readiness probes.
    
    In a real app, check database connections, cache connections, etc.
    """
    # Example readiness checks
    checks = {
        "database": True,  # Would check actual DB connection
        "cache": True,     # Would check actual cache connection
        "dependencies": True,
    }
    
    all_ready = all(checks.values())
    
    if all_ready:
        return {"status": "ready", "checks": checks}
    else:
        return Response.json(
            {"status": "not_ready", "checks": checks},
            status=503
        )


@app.get("/config")
def show_config(request):
    """Display production configuration."""
    return {
        "timeouts": {
            "read_header": production_timeouts.read_header_timeout,
            "read_body": production_timeouts.read_body_timeout,
            "write": production_timeouts.write_timeout,
            "idle": production_timeouts.idle_timeout,
            "handler": production_timeouts.handler_timeout,
        },
        "limits": {
            "max_header_size": production_limits.max_header_size,
            "max_body_size": production_limits.max_body_size,
            "max_connections": production_limits.max_connections,
            "max_requests_per_connection": production_limits.max_requests_per_connection,
        },
        "cluster": {
            "workers": cluster_config.workers,
            "cpu_affinity": cluster_config.cpu_affinity,
            "graceful_shutdown": cluster_config.graceful_shutdown,
            "shutdown_timeout": cluster_config.shutdown_timeout,
        },
        "http2": {
            "max_concurrent_streams": http2_config.max_concurrent_streams,
            "initial_window_size": http2_config.initial_window_size,
            "max_frame_size": http2_config.max_frame_size,
            "enable_push": http2_config.enable_push,
        },
        "http3": {
            "max_idle_timeout": http3_config.max_idle_timeout,
            "max_udp_payload_size": http3_config.max_udp_payload_size,
            "initial_max_streams_bidi": http3_config.initial_max_streams_bidi,
            "enable_0rtt": http3_config.enable_0rtt,
        },
    }


@app.get("/worker-info")
def worker_info(request):
    """Display worker process information."""
    import os
    
    return {
        "worker": {
            "pid": os.getpid(),
            "ppid": os.getppid(),
        },
        "cluster_config": {
            "workers": cluster_config.workers,
            "cpu_affinity": cluster_config.cpu_affinity,
            "max_restarts": cluster_config.max_restarts,
        },
    }


@app.get("/metrics")
def metrics(request):
    """
    Prometheus-style metrics endpoint.
    
    In production, you would integrate with actual metrics collection.
    """
    import os
    
    # Example metrics (would be real values in production)
    metrics_text = f"""# HELP cello_requests_total Total number of requests
# TYPE cello_requests_total counter
cello_requests_total 1000

# HELP cello_request_duration_seconds Request duration
# TYPE cello_request_duration_seconds histogram
cello_request_duration_seconds_bucket{{le="0.01"}} 800
cello_request_duration_seconds_bucket{{le="0.1"}} 950
cello_request_duration_seconds_bucket{{le="1"}} 990
cello_request_duration_seconds_bucket{{le="+Inf"}} 1000
cello_request_duration_seconds_count 1000
cello_request_duration_seconds_sum 50

# HELP cello_active_connections Current active connections
# TYPE cello_active_connections gauge
cello_active_connections 42

# HELP cello_worker_pid Worker process ID
# TYPE cello_worker_pid gauge
cello_worker_pid {os.getpid()}
"""
    
    response = Response.text(metrics_text)
    response.set_header("Content-Type", "text/plain; version=0.0.4")
    return response


# =============================================================================
# API Routes
# =============================================================================


@app.post("/api/data")
def handle_data(request):
    """Handle data uploads."""
    try:
        data = request.json()
        return {
            "received": True,
            "size": len(str(data)),
            "message": "Data processed successfully",
        }
    except Exception as e:
        return Response.json({"error": str(e)}, status=400)


@app.get("/api/slow")
def slow_endpoint(request):
    """
    Simulates a slow endpoint.
    
    Useful for testing timeout configurations.
    In production, configure appropriate handler timeouts.
    """
    import time
    time.sleep(2)  # Simulate slow operation
    return {"message": "Slow response completed", "delay_seconds": 2}


# =============================================================================
# Deployment Examples (as comments)
# =============================================================================

"""
Production Deployment Examples:

1. Basic Production Run:
   python app.py --env production --workers 4 --port 8080

2. With Custom Host (for Docker):
   python app.py --host 0.0.0.0 --port 8080 --workers 4

3. Systemd Service Example (/etc/systemd/system/cello.service):
   [Unit]
   Description=Cello Web Application
   After=network.target

   [Service]
   Type=simple
   User=www-data
   Group=www-data
   WorkingDirectory=/opt/myapp
   ExecStart=/opt/myapp/venv/bin/python app.py --env production --workers 4
   Restart=always
   RestartSec=5

   [Install]
   WantedBy=multi-user.target

4. Docker Compose Example:
   version: '3.8'
   services:
     app:
       build: .
       ports:
         - "8080:8080"
       environment:
         - CELLO_ENV=production
       command: python app.py --host 0.0.0.0 --port 8080 --workers 4
       deploy:
         resources:
           limits:
             cpus: '2'
             memory: 1G

5. Kubernetes Deployment:
   apiVersion: apps/v1
   kind: Deployment
   metadata:
     name: cello-app
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
           livenessProbe:
             httpGet:
               path: /health
               port: 8080
             initialDelaySeconds: 5
             periodSeconds: 10
           readinessProbe:
             httpGet:
               path: /ready
               port: 8080
             initialDelaySeconds: 5
             periodSeconds: 5
"""


if __name__ == "__main__":
    print("🏭 Cello Cluster Mode Demo")
    print()
    print("   Configuration:")
    print(f"   - Workers: {cluster_config.workers} (auto-detected)")
    print(f"   - Graceful shutdown: {cluster_config.graceful_shutdown}")
    print(f"   - Max connections: {production_limits.max_connections}")
    print()
    print("   Endpoints:")
    print("   - GET  http://127.0.0.1:8000/health   - Health check")
    print("   - GET  http://127.0.0.1:8000/ready    - Readiness check")
    print("   - GET  http://127.0.0.1:8000/config   - Configuration")
    print("   - GET  http://127.0.0.1:8000/metrics  - Prometheus metrics")
    print()
    print("   Run with workers: python cluster_demo.py --workers 4")
    print()
    app.run(host="127.0.0.1", port=8000)
