# Contributing to Cello

Thank you for your interest in contributing to Cello! 🐍

## Getting Started

### Prerequisites

- Python 3.12+
- Rust 1.70+
- maturin (`pip install maturin`)

### Development Setup

```bash
# Clone the repository
git clone https://github.com/jagadeeshkatla/cello.git
cd cello

# Create virtual environment
python -m venv .venv
source .venv/bin/activate  # On Windows: .venv\Scripts\activate

# Install development dependencies
pip install maturin pytest ruff requests

# Build the project
maturin develop

# Run tests
pytest tests/ -v
```

## Making Changes

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Your Changes

- **Rust code** → `src/` directory
- **Python wrapper** → `python/cello/` directory
- **Tests** → `tests/` directory

### 3. Test Your Changes

```bash
# Rebuild after Rust changes
maturin develop

# Run Python tests
pytest tests/ -v

# Run linters
ruff check python/ tests/
cargo clippy
cargo fmt --check
```

### 4. Commit Your Changes

```bash
git add .
git commit -m "feat: add your feature description"
```

Follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation
- `refactor:` Code refactoring
- `test:` Adding tests
- `chore:` Maintenance

### 5. Push and Create PR

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub.

## Code Style

### Rust

- Follow Rust standard style (`cargo fmt`)
- No clippy warnings (`cargo clippy -- -D warnings`)
- Document public APIs with `///` comments

### Python

- Follow PEP 8
- Use ruff for linting
- Type hints encouraged

## Project Structure

```
cello/
├── src/                    # Rust source code
│   ├── lib.rs             # Main entry, Python module
│   ├── request.rs         # Request handling
│   ├── response.rs        # Response types
│   ├── router.rs          # URL routing
│   ├── handler.rs         # Handler registry
│   ├── middleware.rs      # Middleware system
│   ├── blueprint.rs       # Route grouping
│   ├── websocket.rs       # WebSocket support
│   ├── sse.rs             # Server-Sent Events
│   ├── multipart.rs       # File uploads
│   ├── json.rs            # SIMD JSON
│   ├── arena.rs           # Arena allocators
│   └── server.rs          # HTTP server
├── python/cello/          # Python package
│   └── __init__.py        # Python API wrapper
├── tests/                  # Python tests
├── examples/              # Example applications
├── Cargo.toml             # Rust dependencies
└── pyproject.toml         # Python project config
```

## Questions?

Open an issue on GitHub!
