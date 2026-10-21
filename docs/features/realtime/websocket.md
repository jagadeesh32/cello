---
title: WebSocket
description: Real-time bidirectional communication with WebSocket in Cello Framework
---

# WebSocket

Cello provides WebSocket support through `tokio-tungstenite`, enabling real-time bidirectional communication between clients and your server. WebSocket handlers are registered with the `@app.websocket()` decorator and receive a `WebSocket` connection object.

```mermaid
flowchart TD
    C(["Browser / Client"]) -->|"1. HTTP GET\nUpgrade: websocket"| S["Cello Server\ntokio-tungstenite 🦀"]
    S -->|"2. 101 Switching Protocols"| C
    S --> H["@app.websocket handler\n🐍 Python"]
    H -->|"ws.send_text / send_binary"| C
    C -->|"ws.recv()"| H
    H -->|"msg is None\nor is_close()"| CLOSE(["Connection closed\ncleanup runs"])
    C -->|"Close frame"| CLOSE

    style C     fill:#1565C0,color:#fff,stroke:none
    style S     fill:#E65100,color:#fff,stroke:none
    style H     fill:#6A1B9A,color:#fff,stroke:none
    style CLOSE fill:#5F6368,color:#fff,stroke:none
```

---

## Quick Start

```python
from cello import App

app = App()

@app.websocket("/ws")
def echo(ws):
    ws.send_text("Connected!")
    while True:
        msg = ws.recv()
        if msg is None or msg.is_close():
            break
        ws.send_text(f"Echo: {msg.content}")

if __name__ == "__main__":
    app.run()
```

Test with any WebSocket client:

```javascript
const ws = new WebSocket("ws://localhost:8000/ws");
ws.onmessage = (e) => console.log(e.data);
ws.onopen = () => ws.send("Hello!");
// Output: "Connected!"
// Output: "Echo: Hello!"
```

---

## The `@app.websocket()` Decorator

Register a WebSocket endpoint at a given path:

```python
@app.websocket("/ws/chat")
def chat_handler(ws):
    # ws is a WebSocket connection object
    pass
```

The handler function receives a single `ws` argument -- the active WebSocket connection. The handler runs for the duration of the connection; when the function returns, queued messages are flushed and the connection is closed.

- A `def` handler runs on the blocking thread pool; `ws.recv()` blocks that thread without holding the GIL. Each open connection occupies one pool thread, so size the pool with `app.set_threadpool(...)`.
- An `async def` handler runs on the application's event loop and holds no thread while it waits. It must use the awaitable methods (`await ws.arecv()`, `await ws.asend_text(...)`, see below). The blocking `recv`/`send*` raise `RuntimeError` there instead of stalling every coroutine on the loop.
- The sync middleware chain (JWT/Basic/API-key auth, rate limits, CORS) and `app.add_guard` guards run on the upgrade request; a rejection is returned as a normal HTTP response instead of `101`.
- **Origin check.** Browsers send `Origin` on every handshake and attach the site's cookies, so a page on another site could otherwise open a socket as the logged-in user. A handshake with an `Origin` must be same-origin (its authority equals `Host`) or listed in `allowed_origins`; anything else gets `403`. Clients that send no `Origin` (scripts, services) are unaffected.

  ```python
  @app.websocket("/ws", allowed_origins=["https://app.example.com"])
  def handler(ws): ...
  ```

  `allowed_origins=["*"]` accepts any origin. Use it only when the socket does not rely on cookies.
- WebSocket paths are matched exactly (no path parameters).
- Messages larger than 16 MiB are rejected. Messages waiting for the handler are capped at 4 MiB per socket: when the handler falls behind, the server stops reading the socket and TCP pushes back on the client. Outgoing sends wait when 64 frames are queued for a slow client.

---

## The `WebSocket` Object

### Sending Messages

| Method | Description |
|--------|-------------|
| `ws.send_text(text)` | Send a UTF-8 text message |
| `ws.send_binary(data)` | Send binary data as `bytes` |
| `ws.send(message)` | Send a `WebSocketMessage` object |
| `ws.close()` | Close the connection gracefully |

Sending on a closed connection raises `ConnectionError`.

```python
@app.websocket("/ws")
def handler(ws):
    # Send text
    ws.send_text("Hello, client!")

    # Send binary
    ws.send_binary(b"\x00\x01\x02\x03")

    # Send a message object
    msg = WebSocketMessage.text("Structured message")
    ws.send(msg)

    # Close the connection
    ws.close()
```

### Receiving Messages

Call `ws.recv(timeout=None)` to wait for the next message from the client. It returns a `WebSocketMessage`, or `None` once the client has closed the connection (or when `timeout` seconds pass). Pings are answered automatically:

```python
@app.websocket("/ws")
def handler(ws):
    while True:
        msg = ws.recv()
        if msg is None:
            break  # Client disconnected
        if msg.is_close():
            break  # Client sent close frame
        if msg.is_text():
            print(f"Text: {msg.content}")
        elif msg.is_binary():
            print(f"Binary: {len(msg.data)} bytes")
```

### Async handlers

An `async def` handler uses the awaitable counterparts:

| Method | Description |
|--------|-------------|
| `await ws.arecv(timeout=None)` | Next message, or `None` once the client closes (or on timeout) |
| `await ws.asend_text(text)` | Send a text message |
| `await ws.asend_binary(data)` | Send binary data |
| `await ws.asend(message)` | Send a `WebSocketMessage` |
| `ws.close()` | Close (never blocks) |

```python
@app.websocket("/ws")
async def echo(ws):
    while (msg := await ws.arecv()) is not None:
        await ws.asend_text(f"Echo: {msg.content}")
```

The handler runs on the same loop as `async def` HTTP handlers. Loop-bound objects
created at startup (database pools, `asyncio.Lock`) are therefore usable from it.

### Connection State

| Property | Type | Description |
|----------|------|-------------|
| `ws.connected` | `bool` | Whether the connection is active |
| `ws.request` | `Request \| None` | The upgrade request: headers, `query_params`, and context set by middleware (e.g. JWT claims) |

---

## The `WebSocketMessage` Class

Messages are represented by `WebSocketMessage` objects with the following interface:

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `msg_type` | `str` | Message type: `"text"`, `"binary"`, `"ping"`, `"pong"`, or `"close"` |
| `content` | `str \| bytes \| None` | Payload: `str` for text, `bytes` for binary |
| `data` | `bytes \| None` | Binary content (for binary messages) |

!!! note
    `WebSocketMessage.text` is the factory method below, so read a text payload
    with `msg.content`, not `msg.text`.

### Factory Methods

```python
from cello import WebSocketMessage

# Create a text message
msg = WebSocketMessage.text("Hello")

# Create a binary message
msg = WebSocketMessage.binary(b"\x00\x01\x02")

# Create a ping message
msg = WebSocketMessage.ping()

# Create a close message
msg = WebSocketMessage.close()
```

### Type Checking

```python
msg = ws.recv()

if msg.is_text():
    print(f"Text: {msg.content}")
elif msg.is_binary():
    print(f"Binary: {len(msg.data)} bytes")
elif msg.is_close():
    print("Client closing connection")
```

---

## Connection Lifecycle

The WebSocket connection follows this lifecycle:

```
1. Client sends HTTP upgrade request
2. Server accepts upgrade (handled by Rust)
3. Handler function is called with active WebSocket
4. Handler sends/receives messages in a loop
5. Handler returns OR client disconnects
6. Connection is closed
```

```python
@app.websocket("/ws/lifecycle")
def lifecycle_demo(ws):
    # Phase: Connected
    ws.send_text("Welcome!")

    # Phase: Message loop
    while True:
        msg = ws.recv()
        if msg is None or msg.is_close():
            break
        ws.send_text(f"Got: {msg.content}")

    # Phase: Cleanup (connection will close when handler returns)
    print("Client disconnected")
```

---

## Sending JSON

Use `send_text` with `json.dumps` to send structured data:

```python
import json

@app.websocket("/ws/data")
def data_stream(ws):
    ws.send_text(json.dumps({"type": "connected", "status": "ok"}))

    while True:
        msg = ws.recv()
        if msg is None or msg.is_close():
            break

        try:
            data = json.loads(msg.content)
            response = process_command(data)
            ws.send_text(json.dumps(response))
        except json.JSONDecodeError:
            ws.send_text(json.dumps({"error": "Invalid JSON"}))
```

---

## Chat Room Example

A complete chat application with multiple connected clients:

```python
from cello import App
import json

app = App()

# Store connected clients (in production, use a proper data structure)
clients = []

@app.websocket("/ws/chat")
def chat(ws):
    # Register client
    clients.append(ws)
    ws.send_text(json.dumps({
        "type": "system",
        "message": "Welcome to the chat!",
        "users_online": len(clients),
    }))

    # Broadcast join notification
    broadcast(json.dumps({
        "type": "system",
        "message": "A new user joined",
        "users_online": len(clients),
    }), exclude=ws)

    # Message loop
    while True:
        msg = ws.recv()
        if msg is None or msg.is_close():
            break

        # Broadcast the message to all clients
        broadcast(json.dumps({
            "type": "message",
            "text": msg.content,
        }))

    # Cleanup on disconnect
    clients.remove(ws)
    broadcast(json.dumps({
        "type": "system",
        "message": "A user left",
        "users_online": len(clients),
    }))

def broadcast(message, exclude=None):
    """Send a message to all connected clients."""
    for client in clients:
        if client is not exclude:
            try:
                client.send_text(message)
            except Exception:
                pass  # Client may have disconnected

if __name__ == "__main__":
    app.run()
```

---

## Per-Connection Parameters

WebSocket paths are matched exactly, so pass per-connection values in the query
string and read them from the upgrade request:

```python
@app.websocket("/ws/rooms")
def room_handler(ws):
    room_id = ws.request.query_params.get("room", "lobby")
    ws.send_text(f"Joined room: {room_id}")

    while (msg := ws.recv()) is not None:
        ws.send_text(f"[{room_id}] {msg.content}")
```

---

## Client-Side Example

### JavaScript (Browser)

```javascript
const ws = new WebSocket("ws://localhost:8000/ws/chat");

ws.onopen = () => {
    console.log("Connected");
    ws.send("Hello from browser!");
};

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    console.log("Received:", data);
};

ws.onclose = () => {
    console.log("Disconnected");
};

ws.onerror = (error) => {
    console.error("WebSocket error:", error);
};
```

### Python Client

```python
import asyncio
import websockets

async def client():
    async with websockets.connect("ws://localhost:8000/ws") as ws:
        await ws.send("Hello from Python!")
        response = await ws.recv()
        print(f"Received: {response}")

asyncio.run(client())
```

---

## Performance

Frames are parsed and written in Rust (`tokio-tungstenite`) on the server's I/O
thread; only `recv()`/`send_*()` cross into Python. A `def` handler holds one
blocking-pool thread for the life of its connection. An `async def` handler holds
none, so prefer `async def` for many long-lived sockets.

---

## Next Steps

- [Server-Sent Events](sse.md) - One-way server push for simpler use cases
- [Routing](../core/routing.md) - Path parameters for HTTP routes
- [Real-time Dashboard Example](../../examples/advanced/realtime-dashboard.md) - Full example with live data
