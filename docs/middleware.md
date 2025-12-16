# Middleware

Cello provides a powerful middleware system for request/response processing.

## Built-in Middleware

### CORS Middleware

Enable Cross-Origin Resource Sharing:

```python
from cello import App

app = App()

# Enable with default settings (allow all origins)
app.enable_cors()

# Enable with specific origins
app.enable_cors(origins=[
    "https://example.com",
    "https://app.example.com",
    "http://localhost:3000",
])
```

CORS headers added:
- `Access-Control-Allow-Origin`
- `Access-Control-Allow-Methods`
- `Access-Control-Allow-Headers`
- `Access-Control-Max-Age`

### Logging Middleware

Enable request/response logging:

```python
app.enable_logging()
```

Logs include:
- HTTP method
- Request path
- Response status code
- Request duration

Example output:
```
GET /api/users -> 200 (15ms)
POST /api/users -> 201 (42ms)
```

### Compression Middleware

Enable gzip compression for large responses:

```python
# Enable with default min size (1024 bytes)
app.enable_compression()

# Enable with custom min size
app.enable_compression(min_size=512)
```

The middleware:
- Checks `Accept-Encoding` header
- Compresses responses larger than `min_size`
- Adds `Content-Encoding: gzip` header

## Middleware Order

Middleware executes in the order they're added:

```python
app = App()
app.enable_cors()        # 1. CORS (first)
app.enable_logging()     # 2. Logging
app.enable_compression() # 3. Compression (last)
```

For responses, the order is reversed:
1. Handler generates response
2. Compression (if applicable)
3. Logging records request/response
4. CORS headers added

## Available Middleware Modules (Rust)

The following middleware is available in the Rust core:

### Authentication
- **JWT Authentication** - JSON Web Token validation
- **Basic Auth** - HTTP Basic authentication
- **API Key** - API key validation

### Rate Limiting
- **Token Bucket** - Token bucket algorithm
- **Sliding Window** - Sliding window counter

### Security
- **Security Headers** - X-Frame-Options, CSP, HSTS, etc.
- **CSRF Protection** - Cross-Site Request Forgery protection

### Caching
- **ETag** - ETag header for caching
- **Static Files** - Static file serving with caching

### Request Processing
- **Body Limit** - Request body size limits
- **Request ID** - Unique request ID generation

## Adding Custom Headers

You can add custom headers in your handlers:

```python
from cello import Response

@app.get("/api/data")
def data_with_headers(request):
    response = Response.json({"data": "value"})
    
    # Add custom headers
    response.set_header("X-Custom-Header", "value")
    response.set_header("X-Request-ID", "req-12345")
    response.set_header("X-Response-Time", "10ms")
    
    return response
```

## Caching Headers

Add cache control headers:

```python
@app.get("/api/cached")
def cached_response(request):
    response = Response.json({"data": "cacheable"})
    
    # Enable caching
    response.set_header("Cache-Control", "public, max-age=3600")
    response.set_header("ETag", '"abc123"')
    
    return response

@app.get("/api/no-cache")
def no_cache_response(request):
    response = Response.json({"data": "dynamic"})
    
    # Prevent caching
    response.set_header("Cache-Control", "no-store, no-cache, must-revalidate")
    response.set_header("Pragma", "no-cache")
    
    return response
```

## Security Headers Pattern

Add security headers to responses:

```python
from cello import SecurityHeadersConfig, CSP

# Configure security headers
security = SecurityHeadersConfig.secure()

# Build CSP
csp = CSP()
csp.default_src(["'self'"])
csp.script_src(["'self'"])
csp_value = csp.build()

def add_security_headers(response):
    """Add security headers to response."""
    response.set_header("X-Frame-Options", security.x_frame_options)
    response.set_header("X-Content-Type-Options", "nosniff")
    response.set_header("X-XSS-Protection", security.x_xss_protection)
    response.set_header("Referrer-Policy", security.referrer_policy)
    response.set_header("Content-Security-Policy", csp_value)
    
    if security.hsts_max_age:
        hsts = f"max-age={security.hsts_max_age}"
        if security.hsts_include_subdomains:
            hsts += "; includeSubDomains"
        response.set_header("Strict-Transport-Security", hsts)
    
    return response

@app.get("/secure")
def secure_endpoint(request):
    response = Response.json({"secure": True})
    return add_security_headers(response)
```

## Middleware Examples

### Request Logging Pattern

```python
import time

def with_timing(handler):
    """Decorator to add timing to handlers."""
    def wrapper(request):
        start = time.time()
        response = handler(request)
        duration = (time.time() - start) * 1000  # ms
        
        response.set_header("X-Response-Time", f"{duration:.2f}ms")
        return response
    return wrapper

@app.get("/timed")
@with_timing
def timed_endpoint(request):
    return {"message": "Hello!"}
```

### Authentication Pattern

```python
from cello import Response

def require_auth(handler):
    """Decorator to require authentication."""
    def wrapper(request):
        token = request.get_header("Authorization")
        
        if not token:
            return Response.json(
                {"error": "Unauthorized"},
                status=401
            )
        
        # Verify token here
        # ...
        
        return handler(request)
    return wrapper

@app.get("/protected")
@require_auth
def protected_endpoint(request):
    return {"message": "Secret data"}
```

### Rate Limiting Pattern

```python
from cello import RateLimitConfig

rate_limit = RateLimitConfig.token_bucket(100, 10)

# Simple in-memory rate limiter
request_counts = {}

def rate_limited(handler):
    """Simple rate limiting decorator."""
    def wrapper(request):
        # Get client IP
        client_ip = request.get_header("X-Forwarded-For") or "unknown"
        
        # Check rate limit
        count = request_counts.get(client_ip, 0)
        if count >= rate_limit.capacity:
            return Response.json(
                {"error": "Rate limit exceeded"},
                status=429
            )
        
        request_counts[client_ip] = count + 1
        return handler(request)
    return wrapper

@app.get("/limited")
@rate_limited
def limited_endpoint(request):
    return {"message": "Success"}
```

## Best Practices

1. **Order matters** - Add security middleware first
2. **Be specific with CORS** - Don't use `*` in production
3. **Set reasonable limits** - Configure body size limits
4. **Use compression wisely** - Set appropriate min_size
5. **Log appropriately** - Disable verbose logging in production
