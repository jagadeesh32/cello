---
title: Installation
description: Install Cello Framework on your system
---

# Installation

This guide covers all installation methods for Cello Framework.

## Requirements

| Requirement | Version | Notes |
|-------------|---------|-------|
| Python | 3.12+ | Required |
| pip | Latest | Package manager |
| OS | Linux, macOS, Windows | All platforms supported |

## Installation Methods

### Via pip (Recommended)

The easiest way to install Cello is via pip:

```bash
pip install cello-framework
```

!!! tip "Virtual Environment"
    We recommend using a virtual environment:

    ```bash
    python -m venv .venv
    source .venv/bin/activate  # Linux/macOS
    # or
    .venv\Scripts\activate     # Windows
    pip install cello-framework
    ```

### From Source

For the latest development version or to contribute:

```bash
# Clone the repository
git clone https://github.com/jagadeesh32/cello.git
cd cello

# Create virtual environment
python -m venv .venv
source .venv/bin/activate

# Install build tools
pip install maturin

# Build and install
maturin develop
```

For release builds with optimizations:

```bash
maturin develop --release
```

### From GitHub

Install directly from GitHub:

```bash
pip install git+https://github.com/jagadeesh32/cello.git
```

## Verify Installation

After installation, verify everything works:

```python
import cello
print(cello.__version__)  # Should print version number
```

Or create a test application:

```python title="test_install.py"
from cello import App

app = App()

@app.get("/")
def hello(request):
    return {"status": "Cello is working!"}

if __name__ == "__main__":
    app.run(port=8080)
```

```bash
python test_install.py
# Visit http://127.0.0.1:8080
```

## Optional Dependencies

For additional features, install optional dependencies:

=== "Testing"

    ```bash
    pip install pytest requests
    ```

=== "Development"

    ```bash
    pip install maturin pytest requests ruff
    ```

=== "Documentation"

    ```bash
    pip install mkdocs-material mkdocstrings
    ```

## Platform-Specific Notes

### Linux

No additional requirements. Works out of the box.

```bash
pip install cello-framework
```

### macOS

No additional requirements. Works on both Intel and Apple Silicon.

```bash
pip install cello-framework
```

### Windows

Works on Windows 10/11. Ensure you have:

- Python 3.12+ from python.org
- Visual C++ Build Tools (for source builds only)

```powershell
pip install cello-framework
```

## Docker

Run Cello in a Docker container:

```dockerfile title="Dockerfile"
FROM python:3.12-slim

WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .
EXPOSE 8000

CMD ["python", "app.py", "--host", "0.0.0.0"]
```

```yaml title="docker-compose.yml"
version: '3.8'
services:
  web:
    build: .
    ports:
      - "8000:8000"
    environment:
      - ENV=production
```

## Troubleshooting

### Import Error

If you get an import error:

```bash
# Ensure you're in the correct virtual environment
which python

# Reinstall
pip uninstall cello-framework
pip install cello-framework
```

### Build Errors (from source)

For source builds, ensure Rust is installed:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify
rustc --version
cargo --version
```

### Permission Errors

Use `--user` flag or a virtual environment:

```bash
pip install --user cello-framework
```

## Next Steps

- :material-rocket-launch: [Quick Start](quickstart.md) - Build your first app
- :material-cog: [Configuration](configuration.md) - Configure your app
- :material-feature-search: [Features](../features/index.md) - Explore features
