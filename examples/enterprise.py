#!/usr/bin/env python3
"""
Enterprise Configuration Example for Cello v0.4.0.

This example demonstrates the enterprise-grade configuration classes
available in Cello for production deployments.

Run with:
    python examples/enterprise.py

Features demonstrated:
    - Timeout and limits configuration
    - Cluster mode configuration
    - TLS/SSL configuration
    - HTTP/2 and HTTP/3 configuration
    - JWT authentication configuration
    - Rate limiting configuration
    - Session management configuration
    - Security headers configuration
    - Static file serving configuration
"""

from cello import App, Response

# Enterprise configuration imports
from cello import (
    TimeoutConfig,
    LimitsConfig,
    ClusterConfig,
    TlsConfig,
    Http2Config,
    Http3Config,
    JwtConfig,
    RateLimitConfig,
    SessionConfig,
    SecurityHeadersConfig,
    CSP,
    StaticFilesConfig,
)

app = App()

# Enable built-in middleware
app.enable_cors()
app.enable_logging()
app.enable_compression()


# =============================================================================
# Configuration Examples (for reference - these show available options)
# =============================================================================

# Timeout configuration
timeout_config = TimeoutConfig(
    read_header=5,      # 5 seconds to read headers
    read_body=30,       # 30 seconds to read body
    write=30,           # 30 seconds to write response
    idle=60,            # 60 seconds idle timeout
    handler=30,         # 30 seconds handler execution timeout
)

# Limits configuration
limits_config = LimitsConfig(
    max_header_size=8192,           # 8KB max header size
    max_body_size=10485760,         # 10MB max body size
    max_connections=10000,          # Max concurrent connections
    max_requests_per_connection=1000,  # Max requests per keep-alive connection
)

# Cluster configuration for multi-worker deployment
cluster_config = ClusterConfig(
    workers=4,                  # Number of worker processes
    cpu_affinity=False,         # Pin workers to CPU cores
    max_restarts=5,             # Max worker restarts before giving up
    graceful_shutdown=True,     # Enable graceful shutdown
    shutdown_timeout=30,        # Seconds to wait for graceful shutdown
)

# Auto-detect cluster configuration
cluster_config_auto = ClusterConfig.auto()

# TLS configuration for HTTPS
tls_config = TlsConfig(
    cert_path="/path/to/cert.pem",
    key_path="/path/to/key.pem",
    ca_path=None,               # Optional CA certificate
    min_version="1.2",          # Minimum TLS version
    max_version="1.3",          # Maximum TLS version
    require_client_cert=False,  # Require client certificate
)

# HTTP/2 configuration
http2_config = Http2Config(
    max_concurrent_streams=100,     # Max concurrent streams per connection
    initial_window_size=1048576,    # Initial flow control window (1MB)
    max_frame_size=16384,           # Max frame size
    enable_push=False,              # Server push (generally disabled)
)

# HTTP/3 (QUIC) configuration
http3_config = Http3Config(
    max_idle_timeout=30,            # Max idle timeout in seconds
    max_udp_payload_size=1350,      # Max UDP payload size
    initial_max_streams_bidi=100,   # Initial max bidirectional streams
    enable_0rtt=False,              # 0-RTT resumption (security trade-off)
)

# JWT authentication configuration
jwt_config = JwtConfig(
    secret="your-super-secret-key-change-in-production",
    algorithm="HS256",              # HS256, HS384, HS512, RS256, etc.
    header_name="Authorization",    # Where to look for token
    cookie_name=None,               # Optional: also check cookies
    leeway=0,                       # Clock skew tolerance in seconds
)

# Rate limiting configuration - Token Bucket algorithm
rate_limit_token_bucket = RateLimitConfig.token_bucket(
    capacity=100,       # Max tokens (requests)
    refill_rate=10,     # Tokens added per second
)

# Rate limiting configuration - Sliding Window algorithm
rate_limit_sliding = RateLimitConfig.sliding_window(
    max_requests=100,   # Max requests in window
    window_secs=60,     # Window duration in seconds
)

# Custom rate limiting configuration
rate_limit_custom = RateLimitConfig(
    algorithm="token_bucket",   # "token_bucket" or "sliding_window"
    capacity=100,               # Max capacity
    refill_rate=10,             # Refill rate (token bucket)
    window_secs=60,             # Window duration (sliding window)
    key_by="ip",                # Rate limit key: "ip", "user", "api_key"
)

# Session configuration
session_config = SessionConfig(
    cookie_name="session_id",
    cookie_path="/",
    cookie_domain=None,         # None = current domain
    cookie_secure=True,         # Require HTTPS
    cookie_http_only=True,      # Not accessible via JavaScript
    cookie_same_site="Lax",     # "Strict", "Lax", or "None"
    max_age=86400,              # Session duration in seconds (24 hours)
)

# Security headers configuration
security_headers = SecurityHeadersConfig(
    x_frame_options="DENY",
    x_content_type_options=True,
    x_xss_protection="1; mode=block",
    referrer_policy="strict-origin-when-cross-origin",
    hsts_max_age=31536000,      # 1 year
    hsts_include_subdomains=True,
    hsts_preload=False,
)

# Pre-configured secure headers
security_headers_secure = SecurityHeadersConfig.secure()

# Content Security Policy builder
csp = CSP()
csp.default_src(["'self'"])
csp.script_src(["'self'", "https://cdn.example.com"])
csp.style_src(["'self'", "'unsafe-inline'"])
csp.img_src(["'self'", "data:", "https:"])
csp_header = csp.build()

# Static files configuration
static_config = StaticFilesConfig(
    root="./static",            # Root directory
    prefix="/static",           # URL prefix
    index_file="index.html",    # Default index file
    enable_etag=True,           # Enable ETag caching
    enable_last_modified=True,  # Enable Last-Modified header
    cache_control=None,         # Custom Cache-Control header
    directory_listing=False,    # Disable directory listing
)


# =============================================================================
# Application Routes
# =============================================================================


@app.get("/")
def home(request):
    """Display available enterprise configurations."""
    return {
        "message": "Cello Enterprise Configuration Demo",
        "version": "0.4.0",
        "configurations": [
            "TimeoutConfig - Request/response timeouts",
            "LimitsConfig - Connection and body limits",
            "ClusterConfig - Multi-worker deployment",
            "TlsConfig - TLS/SSL security",
            "Http2Config - HTTP/2 protocol settings",
            "Http3Config - HTTP/3 (QUIC) settings",
            "JwtConfig - JWT authentication",
            "RateLimitConfig - Rate limiting policies",
            "SessionConfig - Cookie session management",
            "SecurityHeadersConfig - Security headers",
            "CSP - Content Security Policy builder",
            "StaticFilesConfig - Static file serving",
        ],
    }


@app.get("/config/timeout")
def show_timeout_config(request):
    """Show timeout configuration values."""
    return {
        "config": "TimeoutConfig",
        "values": {
            "read_header_timeout": timeout_config.read_header_timeout,
            "read_body_timeout": timeout_config.read_body_timeout,
            "write_timeout": timeout_config.write_timeout,
            "idle_timeout": timeout_config.idle_timeout,
            "handler_timeout": timeout_config.handler_timeout,
        },
        "description": "Controls various timeout settings for request handling",
    }


@app.get("/config/limits")
def show_limits_config(request):
    """Show limits configuration values."""
    return {
        "config": "LimitsConfig",
        "values": {
            "max_header_size": limits_config.max_header_size,
            "max_body_size": limits_config.max_body_size,
            "max_connections": limits_config.max_connections,
            "max_requests_per_connection": limits_config.max_requests_per_connection,
        },
        "description": "Controls connection and request size limits",
    }


@app.get("/config/cluster")
def show_cluster_config(request):
    """Show cluster configuration values."""
    return {
        "config": "ClusterConfig",
        "values": {
            "workers": cluster_config.workers,
            "cpu_affinity": cluster_config.cpu_affinity,
            "max_restarts": cluster_config.max_restarts,
            "graceful_shutdown": cluster_config.graceful_shutdown,
            "shutdown_timeout": cluster_config.shutdown_timeout,
        },
        "description": "Controls multi-worker process deployment",
    }


@app.get("/config/jwt")
def show_jwt_config(request):
    """Show JWT configuration values (secret redacted)."""
    return {
        "config": "JwtConfig",
        "values": {
            "secret": "***REDACTED***",
            "algorithm": jwt_config.algorithm,
            "header_name": jwt_config.header_name,
            "cookie_name": jwt_config.cookie_name,
            "leeway": jwt_config.leeway,
        },
        "description": "JWT authentication settings",
    }


@app.get("/config/rate-limit")
def show_rate_limit_config(request):
    """Show rate limit configuration values."""
    return {
        "config": "RateLimitConfig",
        "examples": {
            "token_bucket": {
                "algorithm": rate_limit_token_bucket.algorithm,
                "capacity": rate_limit_token_bucket.capacity,
                "refill_rate": rate_limit_token_bucket.refill_rate,
            },
            "sliding_window": {
                "algorithm": rate_limit_sliding.algorithm,
                "capacity": rate_limit_sliding.capacity,
                "window_secs": rate_limit_sliding.window_secs,
            },
        },
        "description": "Rate limiting with token bucket or sliding window algorithms",
    }


@app.get("/config/security-headers")
def show_security_headers(request):
    """Show security headers configuration."""
    return {
        "config": "SecurityHeadersConfig",
        "values": {
            "x_frame_options": security_headers.x_frame_options,
            "x_content_type_options": security_headers.x_content_type_options,
            "x_xss_protection": security_headers.x_xss_protection,
            "referrer_policy": security_headers.referrer_policy,
            "hsts_max_age": security_headers.hsts_max_age,
            "hsts_include_subdomains": security_headers.hsts_include_subdomains,
            "hsts_preload": security_headers.hsts_preload,
        },
        "description": "Security headers for protection against common attacks",
    }


@app.get("/config/csp")
def show_csp_config(request):
    """Show Content Security Policy configuration."""
    return {
        "config": "CSP",
        "header_value": csp_header,
        "description": "Content Security Policy builder for XSS protection",
    }


@app.get("/config/session")
def show_session_config(request):
    """Show session configuration values."""
    return {
        "config": "SessionConfig",
        "values": {
            "cookie_name": session_config.cookie_name,
            "cookie_path": session_config.cookie_path,
            "cookie_domain": session_config.cookie_domain,
            "cookie_secure": session_config.cookie_secure,
            "cookie_http_only": session_config.cookie_http_only,
            "cookie_same_site": session_config.cookie_same_site,
            "max_age": session_config.max_age,
        },
        "description": "Cookie-based session management settings",
    }


if __name__ == "__main__":
    print("🏢 Cello Enterprise Configuration Demo")
    print()
    print("   Available endpoints:")
    print("   - GET  /                         - Configuration overview")
    print("   - GET  /config/timeout           - Timeout configuration")
    print("   - GET  /config/limits            - Limits configuration")
    print("   - GET  /config/cluster           - Cluster configuration")
    print("   - GET  /config/jwt               - JWT configuration")
    print("   - GET  /config/rate-limit        - Rate limit configuration")
    print("   - GET  /config/security-headers  - Security headers")
    print("   - GET  /config/csp               - CSP configuration")
    print("   - GET  /config/session           - Session configuration")
    print()
    app.run(host="127.0.0.1", port=8000)
