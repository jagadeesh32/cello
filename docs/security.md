# Security

This guide covers security features in Cello including authentication, rate limiting, and security headers.

## Authentication

### JWT Authentication

Configure JWT authentication:

```python
from cello import JwtConfig

jwt_config = JwtConfig(
    secret="your-256-bit-secret-key",  # Use strong secret in production
    algorithm="HS256",                  # HS256, HS384, HS512, RS256, etc.
    header_name="Authorization",        # Header to check
    cookie_name="auth_token",           # Also check cookies
    leeway=60,                          # Clock skew tolerance (seconds)
)
```

#### JWT Verification Pattern

```python
from cello import Response

def verify_jwt(request):
    """Verify JWT token from request."""
    auth_header = request.get_header("Authorization")
    
    if not auth_header:
        return None, "Missing Authorization header"
    
    if not auth_header.startswith("Bearer "):
        return None, "Invalid Authorization format"
    
    token = auth_header[7:]  # Remove "Bearer " prefix
    
    # Use PyJWT or similar library to decode
    # import jwt
    # try:
    #     payload = jwt.decode(token, secret, algorithms=[algorithm])
    #     return payload, None
    # except jwt.InvalidTokenError as e:
    #     return None, str(e)
    
    return {"user_id": 123}, None  # Example

@app.get("/protected")
def protected_endpoint(request):
    user, error = verify_jwt(request)
    
    if error:
        response = Response.json({"error": error}, status=401)
        response.set_header("WWW-Authenticate", "Bearer")
        return response
    
    return {"message": "Secret data", "user": user}
```

### API Key Authentication

```python
# Store API keys securely (database, environment variables)
API_KEYS = {"key-123": "user1", "key-456": "user2"}

def verify_api_key(request):
    """Verify API key from header."""
    api_key = request.get_header("X-API-Key")
    
    if not api_key:
        return None, "Missing API key"
    
    user = API_KEYS.get(api_key)
    if not user:
        return None, "Invalid API key"
    
    return user, None

@app.get("/api/data")
def api_endpoint(request):
    user, error = verify_api_key(request)
    
    if error:
        return Response.json({"error": error}, status=401)
    
    return {"data": "secret", "user": user}
```

### Basic Authentication

```python
import base64

def verify_basic_auth(request):
    """Verify HTTP Basic authentication."""
    auth_header = request.get_header("Authorization")
    
    if not auth_header or not auth_header.startswith("Basic "):
        return None, "Missing Basic auth"
    
    try:
        encoded = auth_header[6:]
        decoded = base64.b64decode(encoded).decode("utf-8")
        username, password = decoded.split(":", 1)
        
        # Verify credentials (use secure comparison!)
        if username == "admin" and password == "secret":
            return username, None
        
        return None, "Invalid credentials"
    except Exception:
        return None, "Invalid auth header"

@app.get("/admin")
def admin_endpoint(request):
    user, error = verify_basic_auth(request)
    
    if error:
        response = Response.json({"error": error}, status=401)
        response.set_header("WWW-Authenticate", 'Basic realm="Admin"')
        return response
    
    return {"message": "Welcome, admin!"}
```

## Rate Limiting

### Token Bucket Algorithm

Best for APIs with burst tolerance:

```python
from cello import RateLimitConfig

# 100 requests capacity, refill 10 per second
api_rate_limit = RateLimitConfig.token_bucket(
    capacity=100,
    refill_rate=10,
)
```

### Sliding Window Algorithm

Best for strict rate limits:

```python
# 100 requests per 60 seconds
api_rate_limit = RateLimitConfig.sliding_window(
    max_requests=100,
    window_secs=60,
)
```

### Rate Limiting Pattern

```python
from collections import defaultdict
import time

# Simple in-memory rate limiter
class RateLimiter:
    def __init__(self, max_requests, window_seconds):
        self.max_requests = max_requests
        self.window = window_seconds
        self.requests = defaultdict(list)
    
    def is_allowed(self, key):
        now = time.time()
        cutoff = now - self.window
        
        # Remove old requests
        self.requests[key] = [
            t for t in self.requests[key] if t > cutoff
        ]
        
        if len(self.requests[key]) >= self.max_requests:
            return False
        
        self.requests[key].append(now)
        return True

# Create limiters for different purposes
api_limiter = RateLimiter(100, 60)      # 100/minute
login_limiter = RateLimiter(5, 300)     # 5/5 minutes

@app.post("/api/login")
def login(request):
    client_ip = request.get_header("X-Forwarded-For") or "unknown"
    
    if not login_limiter.is_allowed(client_ip):
        response = Response.json(
            {"error": "Too many login attempts"},
            status=429
        )
        response.set_header("Retry-After", "300")
        return response
    
    # Process login...
    return {"token": "..."}
```

## Security Headers

### SecurityHeadersConfig

```python
from cello import SecurityHeadersConfig

# Pre-configured secure defaults
headers = SecurityHeadersConfig.secure()

# Custom configuration
headers = SecurityHeadersConfig(
    x_frame_options="DENY",              # Prevent clickjacking
    x_content_type_options=True,         # Prevent MIME sniffing
    x_xss_protection="1; mode=block",    # XSS filter
    referrer_policy="strict-origin-when-cross-origin",
    hsts_max_age=31536000,               # 1 year HSTS
    hsts_include_subdomains=True,
    hsts_preload=False,
)
```

### Content Security Policy (CSP)

```python
from cello import CSP

csp = CSP()
csp.default_src(["'self'"])
csp.script_src(["'self'", "https://cdn.example.com"])
csp.style_src(["'self'", "'unsafe-inline'"])
csp.img_src(["'self'", "data:", "https:"])

header_value = csp.build()
# "default-src 'self'; script-src 'self' https://cdn.example.com; ..."
```

### Adding Security Headers

```python
def add_security_headers(response):
    """Add security headers to response."""
    response.set_header("X-Frame-Options", "DENY")
    response.set_header("X-Content-Type-Options", "nosniff")
    response.set_header("X-XSS-Protection", "1; mode=block")
    response.set_header("Referrer-Policy", "strict-origin-when-cross-origin")
    response.set_header("Content-Security-Policy", csp.build())
    
    # HTTPS only (enable when using TLS)
    # response.set_header(
    #     "Strict-Transport-Security",
    #     "max-age=31536000; includeSubDomains"
    # )
    
    return response

@app.get("/secure")
def secure_page(request):
    response = Response.html("<h1>Secure Page</h1>")
    return add_security_headers(response)
```

## Session Management

### SessionConfig

```python
from cello import SessionConfig

session = SessionConfig(
    cookie_name="session_id",
    cookie_path="/",
    cookie_domain=None,           # Current domain
    cookie_secure=True,           # HTTPS only
    cookie_http_only=True,        # No JavaScript access
    cookie_same_site="Strict",    # Strict same-site policy
    max_age=3600,                 # 1 hour
)
```

### Session Pattern

```python
import uuid
import time

# In-memory session store (use Redis in production)
sessions = {}

def create_session(user_id):
    """Create a new session."""
    session_id = str(uuid.uuid4())
    sessions[session_id] = {
        "user_id": user_id,
        "created": time.time(),
    }
    return session_id

def get_session(request):
    """Get session from cookie."""
    # Parse cookie header
    cookies = request.get_header("Cookie") or ""
    for part in cookies.split(";"):
        if "=" in part:
            name, value = part.strip().split("=", 1)
            if name == "session_id":
                return sessions.get(value)
    return None

@app.post("/login")
def login(request):
    # Verify credentials...
    user_id = 123
    
    session_id = create_session(user_id)
    
    response = Response.json({"message": "Logged in"})
    response.set_header(
        "Set-Cookie",
        f"session_id={session_id}; Path=/; HttpOnly; Secure; SameSite=Strict"
    )
    return response

@app.get("/dashboard")
def dashboard(request):
    session = get_session(request)
    
    if not session:
        return Response.redirect("/login")
    
    return {"user_id": session["user_id"]}
```

## Error Responses (RFC 7807)

Return structured error responses:

```python
@app.get("/error-examples/unauthorized")
def unauthorized_example(request):
    response = Response.json({
        "type": "/errors/unauthorized",
        "title": "Unauthorized",
        "status": 401,
        "detail": "Authentication required",
    }, status=401)
    response.set_header("Content-Type", "application/problem+json")
    response.set_header("WWW-Authenticate", "Bearer")
    return response

@app.get("/error-examples/rate-limited")
def rate_limited_example(request):
    response = Response.json({
        "type": "/errors/rate-limited",
        "title": "Too Many Requests",
        "status": 429,
        "detail": "Rate limit exceeded",
        "retry_after": 60,
    }, status=429)
    response.set_header("Content-Type", "application/problem+json")
    response.set_header("Retry-After", "60")
    return response
```

## Best Practices

### Secrets Management

1. **Never hardcode secrets** - Use environment variables
2. **Use strong secrets** - At least 256 bits for JWT
3. **Rotate secrets regularly** - Implement key rotation

```python
import os

# Load from environment
JWT_SECRET = os.environ.get("JWT_SECRET")
if not JWT_SECRET:
    raise RuntimeError("JWT_SECRET not set")
```

### Password Security

1. **Hash passwords** - Use bcrypt or argon2
2. **Salt passwords** - Each password needs unique salt
3. **Never log passwords** - Exclude from logs

```python
# Use passlib or bcrypt
# from passlib.hash import bcrypt
# hashed = bcrypt.hash(password)
# if bcrypt.verify(password, hashed):
#     ...
```

### CORS Security

1. **Be specific** - Don't use `*` in production
2. **Validate origins** - Whitelist allowed origins
3. **Limit methods** - Only allow needed methods

```python
# Good
app.enable_cors(origins=["https://myapp.com"])

# Bad - allows any origin
app.enable_cors()  # Only for development
```

### Input Validation

1. **Validate all input** - Never trust user data
2. **Sanitize output** - Escape HTML/SQL
3. **Use type hints** - For documentation and tooling

```python
@app.post("/users")
def create_user(request):
    try:
        data = request.json()
    except Exception:
        return Response.json({"error": "Invalid JSON"}, status=400)
    
    # Validate required fields
    name = data.get("name")
    email = data.get("email")
    
    if not name or not isinstance(name, str):
        return Response.json({"error": "Invalid name"}, status=400)
    
    if not email or "@" not in email:
        return Response.json({"error": "Invalid email"}, status=400)
    
    # Process valid data...
    return {"id": 1, "name": name, "email": email}
```
