---
title: Release Notes
description: Cello Framework version history and release notes
---

# Release Notes

Track the evolution of Cello Framework through its releases.

## Current Version

**Cello v0.6.0** (Latest Stable)

The latest release includes smart caching, adaptive rate limiting, DTO validation, and circuit breaker patterns.

[:octicons-arrow-right-24: v0.6.0 Release Notes](v0.6.0.md)

---

## Release History

| Version | Release Date | Highlights |
|---------|--------------|------------|
| [v0.6.0](v0.6.0.md) | 2025-12 | Smart caching, adaptive rate limiting, DTO validation |
| [v0.5.0](v0.5.0.md) | 2025-10 | Dependency injection, Guards (RBAC), Prometheus metrics |
| [v0.4.0](v0.4.0.md) | 2025-08 | JWT auth, rate limiting, sessions, cluster mode |
| [v0.3.0](v0.3.0.md) | 2025-06 | WebSocket, SSE, multipart, blueprints |

---

## Version Policy

Cello follows [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.x.x): Breaking changes
- **MINOR** (x.1.x): New features, backward compatible
- **PATCH** (x.x.1): Bug fixes, backward compatible

### Support Policy

| Version | Status | Support Until |
|---------|--------|---------------|
| 0.6.x | Active | Current |
| 0.5.x | Maintenance | 2026-04 |
| 0.4.x | Security Only | 2026-01 |
| < 0.4 | End of Life | - |

---

## Upgrade Guides

When upgrading between major versions, see our migration guides:

- [Migration Guide](migration.md) - General migration instructions
- [0.5.x to 0.6.x](migration.md#05x-to-06x) - Latest migration path

---

## Changelog

For a detailed list of all changes, see the [full changelog](changelog.md).

---

## Pre-release Versions

### Beta Releases

All 0.x versions are considered beta. The API may change between minor versions.

### Release Candidates

Release candidates are published before major releases:

```bash
pip install cello-framework==1.0.0rc1
```

---

## Getting Updates

### pip

```bash
# Upgrade to latest
pip install --upgrade cello-framework

# Install specific version
pip install cello-framework==0.6.0
```

### Watch Releases

Star and watch the [GitHub repository](https://github.com/jagadeesh32/cello) to get notified of new releases.
