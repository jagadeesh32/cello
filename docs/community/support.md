---
title: Support
description: Getting help with Cello - community channels, issue reporting, and FAQ
---

# Support

Need help with Cello? This page lists all the ways to get support, report issues, and connect with the community.

---

## Getting Help

### GitHub Issues

The primary channel for bug reports, feature requests, and technical questions.

- **Repository:** [github.com/jagadeesh32/cello](https://github.com/jagadeesh32/cello)
- **Bug reports:** Use the "Bug Report" issue template
- **Feature requests:** Use the "Feature Request" issue template
- **Questions:** Use the "Question" label

Before opening a new issue:

1. Search existing issues to see if your question has already been answered.
2. Include your Cello version (`python -c "import cello; print(cello.__version__)"`).
3. Include a minimal reproducible example.
4. Include the full error traceback if applicable.

### GitHub Discussions

For open-ended questions, ideas, and community conversations, use [GitHub Discussions](https://github.com/jagadeesh32/cello/discussions).

---

## Documentation

- **Getting Started:** [Installation and basics](../learn/tutorials/rest-api.md)
- **API Reference:** [Complete API docs](../reference/api/response.md)
- **Guides:** [Best practices](../learn/guides/best-practices.md), [deployment](../learn/guides/deployment.md), [performance](../learn/guides/performance.md)
- **Examples:** See the `examples/` directory in the repository

---

## Reporting Security Vulnerabilities

If you discover a security vulnerability, please report it **privately**. Do not open a public issue.

Contact the maintainers directly via email or use GitHub's private vulnerability reporting feature on the repository.

---

## Contributing

We welcome contributions of all kinds:

- **Bug fixes** -- Find an issue labeled `good first issue` and submit a PR
- **Documentation** -- Improve guides, fix typos, add examples
- **Features** -- Discuss in an issue first, then implement
- **Tests** -- Increase test coverage

See the [contribution guidelines](https://github.com/jagadeesh32/cello/blob/main/CONTRIBUTING.md) for details on the development workflow.

---

## FAQ

### What Python versions does Cello support?

Python 3.12 and later. Cello uses the PyO3 abi3 stable ABI, so a single binary works across Python 3.12+.

### How do I install Cello?

```bash
pip install cello-framework
```

For development, build from source with Maturin:

```bash
pip install maturin
maturin develop
```

### How do I report a bug?

Open an issue on [GitHub](https://github.com/jagadeesh32/cello/issues) with:

- Cello version
- Python version
- Operating system
- Steps to reproduce
- Expected vs. actual behavior

### Can I use Cello in production?

Cello is in active development. Check the current version and release notes before deploying to production. Evaluate the framework against your specific requirements.

### How does Cello compare to FastAPI / Flask?

Cello's Rust core handles HTTP parsing, routing, and JSON serialization, which are the most performance-critical parts of a web server. Python is used only for business logic. This architecture provides significantly higher throughput than pure-Python frameworks.

### Where can I find examples?

The `examples/` directory in the repository contains 20+ example applications covering all major features. Start with `examples/hello.py` for the basics.

---

## Useful Links

| Resource | URL |
|----------|-----|
| GitHub Repository | [github.com/jagadeesh32/cello](https://github.com/jagadeesh32/cello) |
| Issue Tracker | [github.com/jagadeesh32/cello/issues](https://github.com/jagadeesh32/cello/issues) |
| Release Notes | [Releases](../releases/v1.0.0.md) |
| License | MIT |
