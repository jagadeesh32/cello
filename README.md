# Cello 🐍

[![CI](https://github.com/jagadeeshkatla/cello/actions/workflows/ci.yml/badge.svg)](https://github.com/jagadeeshkatla/cello/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/cello-framework.svg)](https://pypi.org/project/cello-framework/)
[![Python](https://img.shields.io/pypi/pyversions/cello-framework.svg)](https://pypi.org/project/cello-framework/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Ultra-fast Rust-powered Python async web framework**

Cello is a high-performance web framework that combines Python's developer experience with Rust's raw speed. All HTTP handling, routing, and JSON serialization happen in Rust—Python handles only your business logic.

## ✨ Features

| Feature | Description |
|---------|-------------|
| 🚀 **Blazing Fast** | Tokio + Hyper HTTP engine in pure Rust |
| 📦 **SIMD JSON** | SIMD-accelerated JSON with simd-json |
| 🛡️ **Middleware** | Built-in CORS, logging, gzip compression |
| 🗺️ **Blueprints** | Flask-like route grouping |
| 🌐 **WebSocket** | Real-time bidirectional communication |
| 📡 **SSE** | Server-Sent Events streaming |
| 📁 **File Uploads** | Multipart form data handling |
| 🐍 **Pythonic** | Decorator-based routing like Flask |

## 📦 Installation

```bash
pip install cello-framework
```

**From source:**
```bash
pip install maturin
git clone https://github.com/jagadeeshkatla/cello.git
cd cello
maturin develop
```

## 🚀 Quick Start

```python
from cello import App, Response

app = App()

# Enable middleware
app.enable_cors()
app.enable_logging()
app.enable_compression()

@app.get("/")
def home(request):
    return {"message": "Hello, Cello!"}

@app.get("/users/{id}")
def get_user(request):
    user_id = request.params["id"]
    return {"id": user_id, "name": "John"}

@app.post("/users")
def create_user(request):
    data = request.json()
    return {"id": 1, "name": data["name"]}

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

## 📖 Documentation

### Blueprints

Group routes with shared prefixes:

```python
from cello import App, Blueprint

api = Blueprint("/api/v1")

@api.get("/users")
def list_users(request):
    return {"users": []}

@api.get("/users/{id}")
def get_user(request):
    return {"id": request.params["id"]}

app = App()
app.register_blueprint(api)
app.run()
```

### Request Object

```python
def handler(request):
    request.method              # "GET", "POST", etc.
    request.path                # "/users/123"
    request.params["id"]        # Path parameters
    request.query["search"]     # Query parameters
    request.get_header("auth")  # Headers
    request.body()              # Raw bytes
    request.text()              # String body
    request.json()              # Parsed JSON
    request.form()              # Form data dict
```

### Response Types

```python
from cello import Response

# JSON (default)
return {"data": "value"}

# Custom responses
return Response.json({"ok": True}, status=201)
return Response.text("Hello!")
return Response.html("<h1>Hello</h1>")
return Response.file("/path/to/file.pdf")
return Response.redirect("/new-url")
return Response.no_content()
```

### Middleware

```python
app = App()

# CORS - allow cross-origin requests
app.enable_cors()
app.enable_cors(origins=["https://example.com"])

# Logging - log all requests
app.enable_logging()

# Compression - gzip responses
app.enable_compression()
app.enable_compression(min_size=1024)
```

### WebSocket

```python
@app.websocket("/ws")
def websocket_handler(ws):
    ws.send_text("Welcome!")
    while True:
        msg = ws.recv()
        if msg is None:
            break
        ws.send_text(f"Echo: {msg.text}")
```

### Server-Sent Events

```python
from cello import SseEvent, SseStream

@app.get("/events")
def events(request):
    stream = SseStream()
    stream.add_data("Hello")
    stream.add_event("update", '{"count": 1}')
    return stream
```

## 🏗️ Architecture

```
Request → Rust HTTP Engine → Python Handler → Rust Response
              │                    │
              ├─ SIMD JSON         ├─ Return dict
              ├─ Radix routing     └─ Return Response
              └─ Middleware
```

## 🛠️ Development

```bash
# Setup
python -m venv .venv
source .venv/bin/activate
pip install maturin pytest ruff

# Build & Test
maturin develop
pytest tests/ -v

# Lint
ruff check python/ tests/
cargo clippy
```

## 📊 Tech Stack

| Component | Technology |
|-----------|------------|
| HTTP Server | Tokio + Hyper |
| JSON | simd-json + serde |
| Routing | matchit (radix tree) |
| Python Bindings | PyO3 |
| Compression | flate2 (gzip) |

## 📄 License

MIT License - see [LICENSE](LICENSE)

## 👤 Author

**Jagadeesh Katla**

- GitHub: [@jagadeeshkatla](https://github.com/jagadeeshkatla)
# cello
