---
title: Release Notes
description: Version history, changelogs, and migration guides for every Cello Framework release
icon: material/tag-multiple
tags:
  - Release Notes
  - Changelog
  - v1.4.1
  - v1.4.0
  - v1.3.0
  - Migration
---

# :material-tag-multiple: Release Notes

---

## :material-new-box: Latest Release — v1.4.1

!!! warning "Cello v1.4.1 — Review Fixes (October 2026)"

    Security and correctness fixes from the 1.4.0 review: auth on `/health`/`/metrics`/`/graphql`, trusted-proxy client IPs, a cache that never mixes users, working sessions and CSRF form tokens, safe error defaults, served WebSockets. Several defaults changed — see [the release notes](v1.4.1.md#deliberate-behavior-changes).

## Previous Release — v1.4.0

!!! success "Cello v1.4.0 — The Native Core (October 2026)"

    One model declaration, compiled once in Rust, drives the ORM, validation, serialization and OpenAPI.

    **Highlights:**

    - :material-speedometer: **Up to 19.7× faster ORM list endpoints** — return a queryset and the response JSON is built in Rust straight from Postgres rows; single-row get/create are 4–5× faster (paired benchmark, see the notes)
    - :material-sync: **New async bridge** — database and Redis calls no longer hop through extra threads; async handlers go straight to the persistent loop; fixes the exit-time crash and lost Ctrl-C
    - :material-shield-lock: **Security** — ORM identifier injection closed, `write_only`/`read_only` fields, Postgres TLS (`sslmode`, `verify-full`)
    - :material-check-decagram: **Native `body=Model` validation** and OpenAPI request schemas generated from the model
    - :material-database: **Real Postgres types** — NUMERIC, arrays, intervals, datetimes/UUID binding, `DatabaseError` hierarchy
    - :material-source-branch: **ORM features** — Q objects, aggregates, `select_related`/`prefetch_related`, `bulk_create`/`bulk_update`, experimental `cello migrate`
    - :material-alert: **Deliberate breaks** — see the migration notes in the release page

[:material-arrow-right: Read the v1.4.0 release notes](v1.4.0.md)

---

## Previous Release — v1.3.0

!!! success "Cello v1.3.0 — Async Rework, Native Data Layer, Plugin Audit & Three-Pillar Upgrades (July 2026)"

    A large release built around Cello's three pillars — **Speed, Simplicity, Security**.

    **Highlights:**

    - :material-sync: **Persistent asyncio loop** — `async def` handlers run on one long-lived loop (loop-bound resources survive; GIL released during I/O)
    - :material-database: **Native data layer + ORM** — real Postgres pool & async Redis client (incl. `rediss://` TLS) plus a built-in async ORM (replaces the old mock layer)
    - :material-check-decagram: **Full `enable_*` plugin audit** — health-check/GraphQL/Prometheus 404s, BasicAuth challenge, JWT `iat`, security-headers config, and a timeout panic all fixed
    - :material-tune: **Three-pillar upgrades** — full security headers (CSP, Permissions-Policy, cross-origin isolation), gzip-compressed cache HITs, declarative `body=` request validation
    - :material-shield-lock: **Security & DoS hardening** — CSRF/skip-path/CORS/BasicAuth/SSE fixes, body-size limits & header/body/handler timeouts
    - :material-arrow-up: **Upgrade from v1.2.x** — additive API; async apps relying on a fresh loop per request should review the migration note

    [:octicons-arrow-right-24: Full v1.3.0 Release Notes](v1.3.0.md){ .md-button .md-button--primary }
    [:octicons-arrow-right-24: Migration Guide](migration.md){ .md-button }

---

## :material-timeline: Version Timeline

```mermaid
timeline
    title Cello Framework Releases
    2026-10 : v1.4.0 - The Native Core - Rust Schema Registry, Row to JSON Pipeline, Native Validation, Postgres TLS
    2026-07 : v1.3.0 - Async Rework, Native Data Layer, Plugin Audit & Three-Pillar Upgrades
    2026-06 : v1.2.4 - Critical Fix: async def Handlers
            : v1.2.3 - Full Middleware Python API & Docs Fixes
            : v1.2.2 - Security & Bug Fixes (CSRF, skip_path, rate limiter)
            : v1.2.1 - Critical Bug Fixes (server bind, ProblemDetails export)
            : v1.2.0 - Redis Lua Scripting & Rust-Native AsyncClient
            : v1.1.0 - MiniJinja Jinja2-Compatible Templates
    2026-02 : v1.0.1 - Cross-Platform & Compatibility Patch
            : v1.0.0 - Production Ready (Stable)
            : v0.10.0 - Event Sourcing, CQRS, Saga
            : v0.9.0 - GraphQL, gRPC, Message Queues
            : v0.8.0 - Database Pooling, Redis, Transactions
    2026-01 : v0.7.0 - OpenTelemetry, Health Checks
    2025-12 : v0.6.0 - Smart Caching, Adaptive Rate Limiting
    2025-10 : v0.5.0 - DI, Guards (RBAC), Prometheus
    2025-08 : v0.4.0 - JWT, Sessions, Cluster Mode
    2025-06 : v0.3.0 - WebSocket, SSE, Blueprints
```

---

## :material-history: All Releases

<div class="grid cards" markdown>

-   :material-engine:{ .lg .middle } **v1.4.0** — The Native Core

    ---

    One model declaration drives the ORM, validation, serialization and OpenAPI. Prepared-statement cache, new async bridge, Rust row → JSON pipeline (up to 19.7× faster list endpoints), native `body=Model` validation, Postgres TLS, Q objects / aggregates / relations / bulk operations, experimental `cello migrate`.

    :material-calendar: October 2026

    [:octicons-arrow-right-24: Release Notes](v1.4.0.md)

-   :material-sync:{ .lg .middle } **v1.3.0** — Async Rework, Data Layer & Upgrades

    ---

    Persistent asyncio loop, native data layer + ORM, full plugin audit, three-pillar upgrades (full security headers, compressed cache, `body=` validation), and DoS hardening.

    :material-calendar: July 2026

    [:octicons-arrow-right-24: Release Notes](v1.3.0.md)

-   :material-bug-check:{ .lg .middle } **v1.2.4** — Critical Async Fix

    ---

    Fixes all `async def` handlers returning 500 since v1.2.1. Coroutines now driven via `spawn_blocking + asyncio.run()`.

    :material-calendar: June 2026

    [:octicons-arrow-right-24: Release Notes](v1.2.4.md)

-   :material-api:{ .lg .middle } **v1.2.3** — Full Middleware Python API

    ---

    `cello.middleware` module, `app.use()` dispatcher, 6 new `enable_*` methods, all doc import paths corrected.

    :material-calendar: June 2026

    [:octicons-arrow-right-24: Release Notes](v1.2.3.md)

-   :material-shield-bug:{ .lg .middle } **v1.2.2** — Security & Bug Fixes

    ---

    Critical CSRF `HttpOnly` fix (broke all AJAX apps), auth `skip_path` prefix bypass fix, rate limiter fixed-window reset bug.

    :material-calendar: June 2026

    [:octicons-arrow-right-24: Release Notes](v1.2.2.md)

-   :material-bug-check:{ .lg .middle } **v1.2.1** — Critical Bug Fixes

    ---

    Server port never bound, `ProblemDetails` missing from Python export, `And`/`Or` guard `*args` style, `HttpOnly` removed from CSRF cookie.

    :material-calendar: June 2026

    [:octicons-arrow-right-24: Release Notes](v1.2.1.md)

-   :material-database-sync:{ .lg .middle } **v1.2.0** — Redis Lua & AsyncClient

    ---

    Redis Lua scripting (`eval`, `evalsha`, `script_load`), Rust-native `AsyncClient` backed by `reqwest + Tokio` — GIL never held during HTTP I/O.

    :material-calendar: June 2026

    [:octicons-arrow-right-24: Release Notes](v1.2.0.md)

-   :material-file-code:{ .lg .middle } **v1.1.0** — MiniJinja Templates

    ---

    Jinja2-compatible template engine via `minijinja` Rust crate. `app.enable_templates()`, `app.render()`, globals, auto HTML-escaping.

    :material-calendar: June 2026

    [:octicons-arrow-right-24: Release Notes](v1.1.0.md)

-   :material-wrench:{ .lg .middle } **v1.0.1** — Cross-Platform & Compatibility Patch

    ---

    Windows multi-worker subprocess mode, ARM JSON fallback, async guard/cache/blueprint compatibility, export completeness.

    :material-calendar: February 2026

    [:octicons-arrow-right-24: Release Notes](v1.0.1.md)

-   :material-check-decagram:{ .lg .middle } **v1.0.0** — Production Ready

    ---

    Stable release with performance optimizations, API stability guarantees, and the complete feature set.

    :material-calendar: February 2026

    [:octicons-arrow-right-24: Release Notes](v1.0.0.md)

-   :material-star-shooting:{ .lg .middle } **v0.10.0** — Advanced Patterns

    ---

    Event Sourcing, CQRS, and the Saga Pattern for distributed transaction coordination.

    :material-calendar: February 2026

    [:octicons-arrow-right-24: Release Notes](v0.10.0.md)

-   :material-api:{ .lg .middle } **v0.9.0** — API Protocols

    ---

    GraphQL support, gRPC integration, and message queue adapters for Kafka and RabbitMQ.

    :material-calendar: February 2026

    [:octicons-arrow-right-24: Release Notes](v0.9.0.md)

-   :material-database:{ .lg .middle } **v0.8.0** — Data Layer

    ---

    Database connection pooling, Redis integration, and transaction support with automatic rollback.

    :material-calendar: February 2026

    [:octicons-arrow-right-24: Release Notes](v0.8.0.md)

-   :material-eye:{ .lg .middle } **v0.7.0** — Enterprise Observability

    ---

    OpenTelemetry distributed tracing, structured health check endpoints, and GraphQL support.

    :material-calendar: January 2026

    [:octicons-arrow-right-24: Release Notes](v0.7.0.md)

-   :material-speedometer:{ .lg .middle } **v0.6.0** — Smart Middleware

    ---

    Intelligent caching with TTL, adaptive rate limiting based on system load, DTO validation, and circuit breaker.

    :material-calendar: December 2025

    [:octicons-arrow-right-24: Release Notes](v0.6.0.md)

-   :material-shield-check:{ .lg .middle } **v0.5.0** — Security & DI

    ---

    Dependency injection, composable RBAC guards, Prometheus metrics, and OpenAPI generation.

    :material-calendar: October 2025

    [:octicons-arrow-right-24: Release Notes](v0.5.0.md)

-   :material-lock:{ .lg .middle } **v0.4.0** — Auth & Sessions

    ---

    JWT authentication, token-bucket rate limiting, secure cookie sessions, security headers, and cluster mode.

    :material-calendar: August 2025

    [:octicons-arrow-right-24: Release Notes](v0.4.0.md)

-   :material-access-point:{ .lg .middle } **v0.3.0** — Real-time

    ---

    WebSocket support, Server-Sent Events, multipart form handling, and modular blueprints.

    :material-calendar: June 2025

    [:octicons-arrow-right-24: Release Notes](v0.3.0.md)

</div>

---

## :material-shield-half-full: Support Policy

| Version | Status | Support Until |
|:--------|:-------|:--------------|
| **1.4.x** | :material-check-circle:{ style="color: #4caf50" } **Active** | Current |
| 1.3.x, 1.2.x | :material-wrench:{ style="color: #ffab40" } Maintenance | To be announced |
| 1.1.x | :material-wrench:{ style="color: #ffab40" } Maintenance | December 2026 |
| 1.0.x | :material-wrench:{ style="color: #ffab40" } Maintenance | October 2026 |
| 0.10.x | :material-shield-alert:{ style="color: #ff9800" } Security Only | August 2026 |
| < 0.10 | :material-close-circle:{ style="color: #f44336" } End of Life | — |

!!! info "Version policy"

    Cello follows [Semantic Versioning](https://semver.org/). Starting with **v1.0.0**, the public API is stable — no breaking changes until v2.0. **Maintenance** releases receive bug fixes. **Security Only** releases receive critical security patches only.

---

## :material-arrow-up-bold-circle: Upgrading

=== "pip"

    ```bash
    # Upgrade to the latest stable release
    pip install --upgrade cello-framework

    # Pin to a specific version
    pip install cello-framework==1.4.0
    ```

=== "requirements.txt"

    ```text
    cello-framework>=1.4.0,<2.0.0
    ```

=== "pyproject.toml"

    ```toml
    [project]
    dependencies = [
        "cello-framework>=1.4.0,<2.0.0",
    ]
    ```

!!! warning "Read the migration guide before upgrading"

    Breaking changes are documented in the [Migration Guide](migration.md). Always review it before bumping a minor version.

---

## :material-file-document-multiple: Additional Resources

<div class="grid cards" markdown>

-   :material-format-list-bulleted:{ .lg .middle } **Full Changelog**

    ---

    Every commit, bug fix, and improvement in one place.

    [:octicons-arrow-right-24: Changelog](changelog.md)

-   :material-directions-fork:{ .lg .middle } **Migration Guide**

    ---

    Step-by-step instructions for upgrading between versions.

    [:octicons-arrow-right-24: Migration Guide](migration.md)

</div>

---

## :material-bell-ring: Stay Updated

- :material-star: [Star the repo on GitHub](https://github.com/jagadeesh32/cello) to show support and get notified
- :material-rss: [Releases RSS Feed](https://github.com/jagadeesh32/cello/releases.atom) for automated notifications
- :fontawesome-brands-discord: [Join Discord](https://discord.gg/cello) for release announcements and discussion
