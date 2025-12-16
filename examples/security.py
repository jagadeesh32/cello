#!/usr/bin/env python3
"""
Security Features Example for Cello v0.4.0.

This example demonstrates security-focused features including:
- Security headers configuration
- Content Security Policy (CSP)
- Rate limiting patterns
- JWT authentication patterns
- Session security

Run with:
    python examples/security.py

Then test with:
    curl http://127.0.0.1:8000/
    curl http://127.0.0.1:8000/secure-headers
    curl http://127.0.0.1:8000/api/protected -H "Authorization: Bearer your-token"
"""

from cello import App, Response

# Security configuration imports
from cello import (
    JwtConfig,
    RateLimitConfig,
    SessionConfig,
    SecurityHeadersConfig,
    CSP,
)

app = App()

# Enable middleware
app.enable_cors(origins=["https://example.com"])  # Restrict origins in production
app.enable_logging()


# =============================================================================
# Security Configurations
# =============================================================================

# JWT Configuration
# In production, use environment variables for secrets!
jwt_config = JwtConfig(
    secret="your-256-bit-secret-change-me-in-production",
    algorithm="HS256",
    header_name="Authorization",
    cookie_name="auth_token",  # Also check cookies
    leeway=60,  # Allow 60 seconds clock skew
)

# Rate Limiting - Different strategies for different endpoints
api_rate_limit = RateLimitConfig.token_bucket(
    capacity=100,      # 100 requests
    refill_rate=10,    # 10 requests per second refill
)

login_rate_limit = RateLimitConfig.sliding_window(
    max_requests=5,    # 5 login attempts
    window_secs=300,   # per 5 minutes
)

# Session Configuration - Secure defaults
session_config = SessionConfig(
    cookie_name="cello_session",
    cookie_path="/",
    cookie_domain=None,
    cookie_secure=True,         # HTTPS only
    cookie_http_only=True,      # No JavaScript access
    cookie_same_site="Strict",  # Strict same-site policy
    max_age=3600,               # 1 hour session
)

# Security Headers - Strict configuration
security_headers = SecurityHeadersConfig.secure()

# Content Security Policy - Strict policy
csp = CSP()
csp.default_src(["'self'"])
csp.script_src(["'self'"])
csp.style_src(["'self'", "'unsafe-inline'"])  # For inline styles
csp.img_src(["'self'", "data:", "https:"])
csp.font_src(["'self'", "https://fonts.gstatic.com"])
csp_header_value = csp.build()


# =============================================================================
# Helper Functions
# =============================================================================

def add_security_headers(response):
    """Add security headers to response."""
    headers = {
        "X-Frame-Options": security_headers.x_frame_options or "DENY",
        "X-Content-Type-Options": "nosniff" if security_headers.x_content_type_options else None,
        "X-XSS-Protection": security_headers.x_xss_protection,
        "Referrer-Policy": security_headers.referrer_policy,
        "Content-Security-Policy": csp_header_value,
    }
    
    # Add HSTS if configured
    if security_headers.hsts_max_age:
        hsts_value = f"max-age={security_headers.hsts_max_age}"
        if security_headers.hsts_include_subdomains:
            hsts_value += "; includeSubDomains"
        if security_headers.hsts_preload:
            hsts_value += "; preload"
        headers["Strict-Transport-Security"] = hsts_value
    
    for key, value in headers.items():
        if value:
            response.set_header(key, value)
    
    return response


def verify_jwt_token(request):
    """
    Verify JWT token from request.
    
    In a real application, you would decode and verify the JWT.
    This is a pattern example showing where verification would happen.
    """
    auth_header = request.get_header("Authorization")
    
    if not auth_header:
        return None, "Missing Authorization header"
    
    if not auth_header.startswith("Bearer "):
        return None, "Invalid Authorization header format"
    
    token = auth_header[7:]  # Remove "Bearer " prefix
    
    # In a real app, decode and verify the JWT here
    # Example with PyJWT:
    # try:
    #     payload = jwt.decode(token, jwt_config.secret, algorithms=[jwt_config.algorithm])
    #     return payload, None
    # except jwt.ExpiredSignatureError:
    #     return None, "Token has expired"
    # except jwt.InvalidTokenError:
    #     return None, "Invalid token"
    
    # For demo purposes, just check if token exists
    if token:
        return {"user_id": 123, "role": "user"}, None
    
    return None, "Invalid token"


# =============================================================================
# Routes
# =============================================================================


@app.get("/")
def home(request):
    """Security features overview."""
    response_data = {
        "message": "Cello Security Features Demo",
        "version": "0.4.0",
        "features": {
            "jwt_auth": "JWT authentication with configurable algorithms",
            "rate_limiting": "Token bucket and sliding window algorithms",
            "session_management": "Secure cookie-based sessions",
            "security_headers": "Industry-standard security headers",
            "csp": "Content Security Policy builder",
        },
        "endpoints": [
            "GET  /                    - This overview",
            "GET  /secure-headers      - Response with security headers",
            "GET  /api/public          - Public endpoint",
            "GET  /api/protected       - Protected endpoint (needs JWT)",
            "POST /api/login           - Login endpoint (rate limited)",
            "GET  /config/security     - Security configuration info",
        ],
    }
    
    response = Response.json(response_data)
    return add_security_headers(response)


@app.get("/secure-headers")
def secure_headers_demo(request):
    """Demonstrate security headers."""
    response_data = {
        "message": "This response includes security headers",
        "headers_added": {
            "X-Frame-Options": "Prevents clickjacking attacks",
            "X-Content-Type-Options": "Prevents MIME type sniffing",
            "X-XSS-Protection": "Enables XSS filter in older browsers",
            "Referrer-Policy": "Controls referrer information",
            "Content-Security-Policy": "Prevents XSS and data injection",
            "Strict-Transport-Security": "Enforces HTTPS (when enabled)",
        },
    }
    
    response = Response.json(response_data)
    return add_security_headers(response)


@app.get("/api/public")
def public_endpoint(request):
    """Public API endpoint - no authentication required."""
    return {
        "message": "This is a public endpoint",
        "authenticated": False,
    }


@app.get("/api/protected")
def protected_endpoint(request):
    """Protected API endpoint - requires JWT authentication."""
    user, error = verify_jwt_token(request)
    
    if error:
        response = Response.json(
            {"error": "Unauthorized", "detail": error},
            status=401
        )
        response.set_header("WWW-Authenticate", "Bearer")
        return response
    
    return {
        "message": "This is a protected endpoint",
        "authenticated": True,
        "user": user,
    }


@app.post("/api/login")
def login_endpoint(request):
    """
    Login endpoint with rate limiting pattern.
    
    Rate limiting would be enforced at the middleware level.
    This shows the pattern for handling login requests.
    """
    try:
        data = request.json()
    except Exception:
        return Response.json({"error": "Invalid JSON"}, status=400)
    
    username = data.get("username")
    password = data.get("password")
    
    if not username or not password:
        return Response.json(
            {"error": "Missing username or password"},
            status=400
        )
    
    # In a real app, verify credentials against database
    # This is a demo - never do this in production!
    if username == "demo" and password == "password":
        # Generate JWT token here
        return {
            "message": "Login successful",
            "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.demo",
            "expires_in": 3600,
        }
    
    return Response.json(
        {"error": "Invalid credentials"},
        status=401
    )


@app.get("/config/security")
def security_config_info(request):
    """Display security configuration (safe values only)."""
    return {
        "jwt": {
            "algorithm": jwt_config.algorithm,
            "header_name": jwt_config.header_name,
            "cookie_name": jwt_config.cookie_name,
            "leeway_seconds": jwt_config.leeway,
        },
        "rate_limits": {
            "api": {
                "algorithm": api_rate_limit.algorithm,
                "capacity": api_rate_limit.capacity,
                "refill_rate": api_rate_limit.refill_rate,
            },
            "login": {
                "algorithm": login_rate_limit.algorithm,
                "max_requests": login_rate_limit.capacity,
                "window_seconds": login_rate_limit.window_secs,
            },
        },
        "session": {
            "cookie_name": session_config.cookie_name,
            "cookie_secure": session_config.cookie_secure,
            "cookie_http_only": session_config.cookie_http_only,
            "cookie_same_site": session_config.cookie_same_site,
            "max_age_seconds": session_config.max_age,
        },
        "csp_policy": csp_header_value,
    }


# =============================================================================
# Error Handlers (Pattern Examples)
# =============================================================================


@app.get("/error/unauthorized")
def unauthorized_example(request):
    """Example of 401 Unauthorized response."""
    response = Response.json(
        {
            "type": "/errors/unauthorized",
            "title": "Unauthorized",
            "status": 401,
            "detail": "Authentication is required to access this resource",
        },
        status=401
    )
    response.set_header("Content-Type", "application/problem+json")
    response.set_header("WWW-Authenticate", "Bearer")
    return response


@app.get("/error/forbidden")
def forbidden_example(request):
    """Example of 403 Forbidden response."""
    response = Response.json(
        {
            "type": "/errors/forbidden",
            "title": "Forbidden",
            "status": 403,
            "detail": "You don't have permission to access this resource",
        },
        status=403
    )
    response.set_header("Content-Type", "application/problem+json")
    return response


@app.get("/error/rate-limited")
def rate_limited_example(request):
    """Example of 429 Too Many Requests response."""
    response = Response.json(
        {
            "type": "/errors/rate-limited",
            "title": "Too Many Requests",
            "status": 429,
            "detail": "Rate limit exceeded. Please try again later.",
            "retry_after": 60,
        },
        status=429
    )
    response.set_header("Content-Type", "application/problem+json")
    response.set_header("Retry-After", "60")
    return response


if __name__ == "__main__":
    print("🔐 Cello Security Features Demo")
    print()
    print("   Try these endpoints:")
    print("   - GET  http://127.0.0.1:8000/")
    print("   - GET  http://127.0.0.1:8000/secure-headers")
    print("   - GET  http://127.0.0.1:8000/api/public")
    print("   - GET  http://127.0.0.1:8000/api/protected")
    print("   - POST http://127.0.0.1:8000/api/login")
    print()
    print("   Test protected endpoint with:")
    print('   curl -H "Authorization: Bearer test-token" http://127.0.0.1:8000/api/protected')
    print()
    app.run(host="127.0.0.1", port=8000)
