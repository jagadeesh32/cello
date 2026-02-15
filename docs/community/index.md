---
title: Community
description: Join the Cello Framework community -- contribute, get support, and connect with other developers
icon: material/account-group
---

# :material-account-group: Welcome to the Cello Community

<div class="grid" markdown>

!!! quote "Built together, by developers, for developers"

    Cello is an **open-source project** that thrives on community contributions. Whether you are filing your first issue, answering a question on Discord, or submitting a Rust optimization -- every contribution matters. We are glad you are here.

</div>

---

## :material-cards: Get Involved

<div class="grid cards" markdown>

-   :material-source-pull:{ .lg .middle } **Contributing**

    ---

    Read the contributing guide to learn how to set up your development environment, submit pull requests, and follow our coding conventions.

    [:octicons-arrow-right-24: Contributing Guide](contributing.md)

-   :material-scale-balance:{ .lg .middle } **Code of Conduct**

    ---

    We are committed to providing a welcoming, inclusive, and harassment-free experience for everyone. Please read and follow our code of conduct.

    [:octicons-arrow-right-24: Code of Conduct](code-of-conduct.md)

-   :material-lifebuoy:{ .lg .middle } **Support**

    ---

    Stuck on a problem? Browse common solutions, ask the community, or explore enterprise support options for production deployments.

    [:octicons-arrow-right-24: Get Support](support.md)

-   :fontawesome-brands-discord:{ .lg .middle } **Discord**

    ---

    Real-time chat with the core team and other Cello developers. Get help, share projects, and stay up to date on announcements.

    [:octicons-arrow-right-24: Join Discord](https://discord.gg/cello)

</div>

---

## :material-shoe-print: How to Contribute

Contributing to Cello is straightforward. Follow these steps to go from zero to merged PR.

=== "Step 1 -- Fork & Clone"

    ```bash
    # Fork on GitHub, then clone your fork
    git clone https://github.com/<your-username>/cello.git
    cd cello

    # Create a virtual environment
    python -m venv .venv && source .venv/bin/activate

    # Install development dependencies
    pip install maturin pytest requests
    ```

=== "Step 2 -- Build & Test"

    ```bash
    # Build Rust extensions in development mode
    maturin develop

    # Run the test suite
    pytest tests/ -v

    # Run Rust checks
    cargo clippy --all-targets
    cargo fmt --check
    cargo test
    ```

=== "Step 3 -- Branch & Code"

    ```bash
    # Create a feature branch
    git checkout -b feature/my-awesome-feature

    # Make your changes, then verify
    cargo clippy --all-targets
    pytest tests/ -v
    ```

=== "Step 4 -- Submit PR"

    ```bash
    # Commit with a clear message
    git add -A
    git commit -m "feat: add awesome new feature"

    # Push and open a pull request
    git push origin feature/my-awesome-feature
    ```

    Then open a Pull Request on GitHub. The CI pipeline will run automatically.

!!! tip "First-time contributor?"

    Look for issues labeled **`good first issue`** -- these are specifically curated for newcomers:

    [:octicons-arrow-right-24: Good First Issues](https://github.com/jagadeesh32/cello/labels/good%20first%20issue){ .md-button }

---

## :material-hand-heart: Ways to Contribute

Not sure where to start? There are many ways to help beyond writing code.

| Contribution | Description | Getting Started |
|:-------------|:------------|:----------------|
| :material-bug: **Report Bugs** | Found something broken? File an issue with reproduction steps. | [Open a Bug Report](https://github.com/jagadeesh32/cello/issues/new?template=bug_report.md) |
| :material-lightbulb: **Request Features** | Have an idea for Cello? We would love to hear it. | [Open a Feature Request](https://github.com/jagadeesh32/cello/issues/new?template=feature_request.md) |
| :material-code-tags: **Contribute Code** | Fix bugs, add features, or optimize Rust performance. | [Contributing Guide](contributing.md) |
| :material-book-open-variant: **Improve Docs** | Fix typos, add examples, clarify explanations. | [Edit on GitHub](https://github.com/jagadeesh32/cello/tree/main/docs) |
| :material-translate: **Translate** | Help make the docs accessible in more languages. | [Discord #translations](https://discord.gg/cello) |
| :material-help-circle: **Help Others** | Answer questions on Discord or Stack Overflow. | [Discord](https://discord.gg/cello) |

---

## :material-github: Project Stats

<div class="grid cards" markdown>

-   :material-star:{ .lg .middle } **Stars**

    ---

    Show your support by starring the repository on GitHub.

    [:octicons-arrow-right-24: Star on GitHub](https://github.com/jagadeesh32/cello)

-   :material-source-fork:{ .lg .middle } **Forks**

    ---

    Fork the project to start contributing or experiment with your own changes.

    [:octicons-arrow-right-24: Fork on GitHub](https://github.com/jagadeesh32/cello/fork)

-   :material-account-multiple:{ .lg .middle } **Contributors**

    ---

    Join the growing list of developers who have contributed to Cello.

    [:octicons-arrow-right-24: View Contributors](https://github.com/jagadeesh32/cello/graphs/contributors)

-   :material-source-branch:{ .lg .middle } **Open Issues**

    ---

    Browse open issues and help close them. Every fix counts.

    [:octicons-arrow-right-24: View Issues](https://github.com/jagadeesh32/cello/issues)

</div>

---

## :material-chat: Community Channels

| Channel | Best For | Link |
|:--------|:---------|:-----|
| :fontawesome-brands-discord: **Discord** | Real-time help, announcements, casual chat | [Join Discord](https://discord.gg/cello) |
| :material-github: **GitHub Discussions** | Long-form questions, ideas, show-and-tell | [Discussions](https://github.com/jagadeesh32/cello/discussions) |
| :material-stack-overflow: **Stack Overflow** | Searchable Q&A (tag: `cello-framework`) | [Stack Overflow](https://stackoverflow.com/questions/tagged/cello-framework) |
| :fontawesome-brands-x-twitter: **X / Twitter** | News, release announcements | [@CelloFramework](https://twitter.com/CelloFramework) |
| :material-rss: **RSS Feed** | Automated release notifications | [Releases Feed](https://github.com/jagadeesh32/cello/releases.atom) |

---

## :material-heart: Acknowledgments

Cello is made possible by the work of these people and projects.

??? info "Core Contributors"

    - **Jagadeesh Katla** -- Creator and lead maintainer

??? info "Technologies We Build On"

    | Technology | Role in Cello |
    |:-----------|:--------------|
    | [Rust](https://www.rust-lang.org/) | Core runtime and hot-path engine |
    | [PyO3](https://pyo3.rs/) | Python-Rust FFI bindings |
    | [Tokio](https://tokio.rs/) | Async runtime |
    | [Hyper](https://hyper.rs/) | HTTP/1.1 and HTTP/2 server |
    | [simd-json](https://github.com/simd-lite/simd-json) | SIMD-accelerated JSON parsing |
    | [matchit](https://github.com/ibraheemdev/matchit) | Radix-tree routing |

??? info "Inspiration"

    Cello stands on the shoulders of:

    - [FastAPI](https://fastapi.tiangolo.com/) -- Developer experience and type hints
    - [Actix-web](https://actix.rs/) -- Rust performance patterns
    - [Flask](https://flask.palletsprojects.com/) -- Simplicity and blueprints
    - [Django](https://www.djangoproject.com/) -- Batteries-included philosophy

---

<div class="grid" markdown>

!!! example "Ready to dive in?"

    The best way to join the community is to start building. Pick an [example](../examples/index.md), read the [getting started guide](../getting-started/index.md), or jump straight into the [open issues](https://github.com/jagadeesh32/cello/issues).

    [:octicons-arrow-right-24: Get Started](../getting-started/index.md){ .md-button .md-button--primary }
    [:octicons-arrow-right-24: Browse Issues](https://github.com/jagadeesh32/cello/issues){ .md-button }

</div>
