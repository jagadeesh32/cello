---
title: Getting Started
description: Get up and running with Cello Framework in minutes
---

# Getting Started with Cello

Welcome to Cello! This guide will help you get up and running with the world's fastest Python web framework.

## Overview

Cello is a Rust-powered Python async web framework that combines:

- **Python's Developer Experience** - Clean, intuitive API
- **Rust's Performance** - Native speed without the complexity
- **Enterprise Features** - Security, scalability, observability

## Prerequisites

Before you begin, ensure you have:

- **Python 3.12+** - Cello requires Python 3.12 or later
- **pip** - Python package manager
- **Optional**: Rust toolchain (only for building from source)

## Quick Navigation

<div class="grid cards" markdown>

-   :material-download:{ .lg .middle } **Installation**

    ---

    Install Cello via pip or from source

    [:octicons-arrow-right-24: Install Guide](installation.md)

-   :material-rocket-launch:{ .lg .middle } **Quick Start**

    ---

    Create your first Cello application in 5 minutes

    [:octicons-arrow-right-24: Quick Start](quickstart.md)

-   :material-application:{ .lg .middle } **First Application**

    ---

    Build a complete REST API step by step

    [:octicons-arrow-right-24: First App](first-app.md)

-   :material-folder-outline:{ .lg .middle } **Project Structure**

    ---

    Learn how to organize your Cello project

    [:octicons-arrow-right-24: Structure](project-structure.md)

-   :material-cog:{ .lg .middle } **Configuration**

    ---

    Configure your application for different environments

    [:octicons-arrow-right-24: Configuration](configuration.md)

</div>

## Installation Overview

=== "pip (Recommended)"

    ```bash
    pip install cello-framework
    ```

=== "From Source"

    ```bash
    git clone https://github.com/jagadeesh32/cello.git
    cd cello
    pip install maturin
    maturin develop
    ```

## Hello World

```python title="app.py"
from cello import App

app = App()

@app.get("/")
def hello(request):
    return {"message": "Hello, World!"}

if __name__ == "__main__":
    app.run()
```

```bash
python app.py
# Cello running at http://127.0.0.1:8000
```

Visit `http://127.0.0.1:8000` to see your first Cello response!

## What's Next?

1. **[Installation](installation.md)** - Detailed installation instructions
2. **[Quick Start](quickstart.md)** - Learn the basics in 5 minutes
3. **[First Application](first-app.md)** - Build a complete REST API
4. **[Features](../features/index.md)** - Explore all features

## Need Help?

- :material-book: [Documentation](../index.md)
- :material-github: [GitHub Issues](https://github.com/jagadeesh32/cello/issues)
- :material-discord: [Discord Community](https://discord.gg/cello)
