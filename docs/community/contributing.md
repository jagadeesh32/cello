---
title: Contributing
description: How to contribute to Cello Framework
---

# Contributing to Cello

Thank you for your interest in contributing to Cello! This guide will help you get started.

## Prerequisites

Before contributing, ensure you have:

- **Python 3.12+**
- **Rust (stable)**
- **Git**
- **maturin** (Python package builder for Rust)

## Setting Up the Development Environment

### 1. Fork and Clone

```bash
# Fork the repository on GitHub, then:
git clone https://github.com/YOUR_USERNAME/cello.git
cd cello
```

### 2. Create Virtual Environment

```bash
python -m venv .venv
source .venv/bin/activate  # Linux/macOS
# or
.venv\Scripts\activate     # Windows
```

### 3. Install Dependencies

```bash
pip install maturin pytest requests ruff
```

### 4. Build the Project

```bash
maturin develop
```

### 5. Run Tests

```bash
pytest tests/ -v
```

---

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/my-feature
# or
git checkout -b fix/my-bugfix
```

### 2. Make Changes

- Write code following our coding standards
- Add tests for new functionality
- Update documentation if needed

### 3. Run Quality Checks

```bash
# Rust checks
cargo fmt --check
cargo clippy --all-targets

# Python checks
ruff check python/
ruff format python/ --check

# Tests
pytest tests/ -v
```

### 4. Commit Changes

```bash
git add .
git commit -m "feat: add new feature"
```

Use [conventional commits](https://www.conventionalcommits.org/):

- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation changes
- `test:` - Test changes
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks

### 5. Push and Create PR

```bash
git push origin feature/my-feature
```

Then create a Pull Request on GitHub.

---

## Coding Standards

### Rust Code

```rust
// Use rustfmt for formatting
// Follow Clippy suggestions
// Document public APIs

/// Creates a new request handler.
///
/// # Arguments
///
/// * `method` - HTTP method
/// * `path` - Route path
///
/// # Returns
///
/// A configured handler instance.
pub fn new_handler(method: &str, path: &str) -> Handler {
    // Implementation
}
```

### Python Code

```python
# Use type hints
# Follow PEP 8 (enforced by ruff)
# Document public APIs

def create_user(request: Request) -> dict:
    """Create a new user.

    Args:
        request: The incoming HTTP request.

    Returns:
        The created user data.

    Raises:
        ValidationError: If request data is invalid.
    """
    data = request.json()
    # Implementation
    return {"id": 1, **data}
```

---

## Testing

### Writing Tests

```python
# tests/test_feature.py

import pytest
from cello import App

def test_my_feature():
    app = App()

    @app.get("/test")
    def handler(request):
        return {"status": "ok"}

    # Test the handler
    # (use your preferred testing approach)
    assert True  # Replace with actual assertions
```

### Running Tests

```bash
# All tests
pytest tests/ -v

# Specific test
pytest tests/test_feature.py -v

# With coverage
pytest tests/ -v --cov=cello
```

---

## Documentation

### Building Documentation

```bash
pip install mkdocs-material
mkdocs serve
```

Visit `http://localhost:8000` to preview.

### Documentation Guidelines

- Use clear, concise language
- Include code examples
- Add appropriate admonitions (tip, warning, note)
- Link to related documentation

---

## Pull Request Guidelines

### Before Submitting

- [ ] Code follows our coding standards
- [ ] Tests pass locally
- [ ] Documentation updated (if needed)
- [ ] Commit messages follow conventional commits
- [ ] Branch is up to date with `main`

### PR Description Template

```markdown
## Summary
Brief description of changes.

## Changes
- Change 1
- Change 2

## Testing
How to test these changes.

## Related Issues
Fixes #123
```

---

## Issue Guidelines

### Bug Reports

Include:
- Cello version
- Python version
- Operating system
- Minimal reproduction code
- Expected vs actual behavior
- Error messages/stack traces

### Feature Requests

Include:
- Use case description
- Proposed API (if applicable)
- Alternative solutions considered
- Willingness to implement

---

## Code Review

All PRs are reviewed by maintainers. Expect feedback on:

- Code quality and style
- Test coverage
- Documentation
- Performance implications
- API design

Be responsive to feedback and willing to make changes.

---

## Getting Help

- :material-discord: [Discord](https://discord.gg/cello) - Development chat
- :material-github: [GitHub Discussions](https://github.com/jagadeesh32/cello/discussions) - Questions
- :material-email: [Email](mailto:dev@cello-framework.dev) - Direct contact

---

## Recognition

Contributors are recognized in:
- Release notes
- Contributors page
- GitHub contributors list

Thank you for contributing to Cello!
