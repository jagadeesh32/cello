# 🚀 Ultra-Fast Rust-Powered Python Async Web Framework

**Goal:** Build a Rust-first web framework with Python syntax that is
**faster than Robyn** and approaches **C-level performance** on the hot
path.

**Framwork Name:** Cello

------------------------------------------------------------------------

## 1️⃣ Core Vision

-   **Python = Developer Experience (DX)**
-   **Rust = Runtime & Execution Engine**
-   **Async-first**
-   **Zero-copy data flow**
-   **Minimal Python involvement per request**

> Python should behave like a **DSL**, not a runtime.

------------------------------------------------------------------------

## 2️⃣ Non-Negotiable Performance Rules

### ❌ Must Never Happen

-   Python handles sockets or HTTP parsing
-   `asyncio` drives I/O
-   Python middleware in request hot path
-   JSON serialization in Python
-   Dynamic routing lookups per request

### ✅ Must Always Happen

-   Rust owns:
    -   TCP accept loop
    -   HTTP parsing
    -   Routing
    -   Middleware
    -   Serialization
-   Python only:
    -   Registers routes
    -   Provides handler function pointers
    -   Returns minimal data structures

------------------------------------------------------------------------

## 3️⃣ High-Level Architecture

    Client
      │
      ▼
    ┌──────────────────────────┐
    │ Rust TCP / HTTP Engine   │
    │ - epoll / io_uring       │
    │ - HTTP parsing           │
    │ - Routing (radix tree)   │
    │ - Middleware             │
    └──────────┬───────────────┘
               ▼
    ┌──────────────────────────┐
    │ Rust ↔ Python ABI Layer  │
    │ - pyo3 + abi3            │
    │ - GIL minimized          │
    │ - Handler caching        │
    └──────────┬───────────────┘
               ▼
    ┌──────────────────────────┐
    │ Python User Handler      │
    │ - Pure business logic    │
    │ - Returns struct / dict  │
    └──────────┬───────────────┘
               ▼
    ┌──────────────────────────┐
    │ Rust Response Builder    │
    │ - SIMD JSON              │
    │ - Zero-copy write        │
    └──────────────────────────┘

------------------------------------------------------------------------

## 4️⃣ Technology Stack

### 🔩 Rust Side

  Component   Choice
  ----------- ------------------
  Runtime     tokio
  HTTP        hyper / custom
  JSON        simd-json
  Routing     Radix tree
  FFI         pyo3 + abi3
  Memory      Arena allocators
  Syscalls    io_uring

------------------------------------------------------------------------

## 5️⃣ Agent-Driven Development Model

### 🤖 Agent 1: Architecture Guardian

-   Enforce Rust-first execution
-   Define ABI boundaries
-   Prevent Python hot-path leaks

### 🤖 Agent 2: Rust Core Engine Agent

-   TCP accept loop
-   HTTP parsing
-   Routing
-   Middleware
-   Response writer

### 🤖 Agent 3: Python ABI / FFI Agent

-   PyCapsule registry
-   Handler caching
-   GIL control

### 🤖 Agent 4: Python DX Agent

``` python
from ultrarust import App

app = App()

@app.get("/hello")
def hello(req):
    return {"msg": "hello"}
```

### 🤖 Agent 5: Benchmark Agent

-   wrk
-   bombardier
-   latency p50/p99

------------------------------------------------------------------------

## 6️⃣ Why This Beats Robyn

  Feature           Robyn    This
  ----------------- -------- -----------
  Python hot path   Yes      No
  Routing           Python   Rust
  JSON              Python   SIMD Rust
  io_uring          No       Yes

------------------------------------------------------------------------

