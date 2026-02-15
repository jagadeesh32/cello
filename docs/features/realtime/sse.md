---
title: Server-Sent Events
description: Server-push streaming with SSE in Cello Framework
---

# Server-Sent Events (SSE)

Server-Sent Events provide a simple, one-way channel for pushing data from the server to connected clients over HTTP. Unlike WebSocket, SSE uses standard HTTP and is ideal for live feeds, notifications, and dashboards where only the server needs to send updates.

---

## Overview

```
Server                          Client (Browser)
  |                                |
  |  <-- GET /events (SSE)         |
  |                                |
  |  event: update                 |
  |  data: {"temp": 72}  -------> |  onmessage fires
  |                                |
  |  event: alert                  |
  |  data: "High temp"   -------> |  addEventListener("alert")
  |                                |
  |  (connection stays open)       |
```

---

## The `SseEvent` Class

Each event sent to the client is an `SseEvent` object.

### Constructor

```python
from cello import SseEvent

event = SseEvent(
    data="Hello, world!",      # Required: event data
    event="message",           # Optional: event type
    id="1",                    # Optional: event ID
    retry=3000,                # Optional: reconnect interval (ms)
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `data` | `str` | Required | The event payload (can be multi-line) |
| `event` | `str | None` | `None` | Named event type for client-side filtering |
| `id` | `str | None` | `None` | Unique event ID (used for resuming after reconnect) |
| `retry` | `int | None` | `None` | Client reconnection interval in milliseconds |

### Factory Methods

```python
# Simple data-only event
event = SseEvent.data("Hello!")

# Event with a named type
event = SseEvent.with_event("notification", "You have a new message")
```

### Wire Format

Each `SseEvent` is serialized to the SSE text protocol:

```python
event = SseEvent("sensor data", event="update", id="42", retry=5000)
print(event.to_sse_string())
```

Output:

```
id: 42
event: update
retry: 5000
data: sensor data

```

Multi-line data is automatically split into separate `data:` lines:

```python
event = SseEvent.data("Line 1\nLine 2\nLine 3")
```

```
data: Line 1
data: Line 2
data: Line 3

```

---

## The `SseStream` Class

`SseStream` collects multiple events for streaming to clients.

### Building a Stream

```python
from cello import SseStream, SseEvent

stream = SseStream()

# Add events
stream.add(SseEvent("First event"))
stream.add(SseEvent("Status update", event="status"))
stream.add(SseEvent("Alert!", event="alert", id="3"))

# Convenience methods
stream.add_data("Simple data event")
stream.add_event("notification", "New message received")

# Check stream state
print(stream.len())       # 5
print(stream.is_empty())  # False
```

---

## Streaming Events from a Handler

Return an SSE response from a route handler:

```python
from cello import App, Response, SseEvent
import json
import time

app = App()

@app.get("/events")
def event_stream(request):
    """Stream live events to connected clients."""
    stream = SseStream()

    # Add initial event
    stream.add(SseEvent(
        json.dumps({"status": "connected"}),
        event="init",
        retry=3000
    ))

    # Add data events
    for i in range(10):
        stream.add(SseEvent(
            json.dumps({"count": i, "timestamp": time.time()}),
            event="update",
            id=str(i)
        ))

    return Response.sse(stream)
```

The response is sent with these headers automatically:

```
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive
X-Accel-Buffering: no
```

---

## Event Types

Named event types let the client listen for specific categories of events:

### Server Side

```python
@app.get("/notifications")
def notifications(request):
    stream = SseStream()

    # Different event types
    stream.add(SseEvent("User logged in", event="auth"))
    stream.add(SseEvent("New order #1234", event="order"))
    stream.add(SseEvent("Server CPU at 85%", event="alert"))
    stream.add(SseEvent("General update", event="message"))

    return Response.sse(stream)
```

### Client Side (JavaScript)

```javascript
const source = new EventSource("/notifications");

// Listen to specific event types
source.addEventListener("auth", (e) => {
    console.log("Auth event:", e.data);
});

source.addEventListener("order", (e) => {
    console.log("Order event:", e.data);
});

source.addEventListener("alert", (e) => {
    console.log("Alert:", e.data);
});

// Default handler for unnamed events
source.onmessage = (e) => {
    console.log("Message:", e.data);
};
```

---

## Event IDs and Resuming

Event IDs enable automatic resume after a disconnection. When the client reconnects, the browser sends a `Last-Event-ID` header so the server can resume from where it left off:

```python
@app.get("/feed")
def feed(request):
    # Check if client is resuming
    last_id = request.get_header("Last-Event-ID")
    start_from = int(last_id) + 1 if last_id else 0

    stream = SseStream()
    events = get_events_from(start_from)

    for event in events:
        stream.add(SseEvent(
            json.dumps(event["data"]),
            event=event["type"],
            id=str(event["id"])
        ))

    return Response.sse(stream)
```

---

## Retry Configuration

The `retry` field tells the client how long to wait (in milliseconds) before attempting to reconnect after a disconnection:

```python
# Tell client to retry after 5 seconds if disconnected
stream.add(SseEvent("init", retry=5000))
```

If not specified, browsers typically default to 3 seconds.

---

## Practical Examples

### Live Dashboard

```python
import json
import time
import random

@app.get("/dashboard/stream")
def dashboard_stream(request):
    stream = SseStream()

    # Send initial configuration
    stream.add(SseEvent(
        json.dumps({"interval": 1000}),
        event="config",
        retry=5000
    ))

    # Stream system metrics
    for i in range(60):
        metrics = {
            "cpu": random.uniform(20, 90),
            "memory": random.uniform(40, 80),
            "requests_per_sec": random.randint(100, 500),
            "timestamp": time.time(),
        }
        stream.add(SseEvent(
            json.dumps(metrics),
            event="metrics",
            id=str(i)
        ))

    return Response.sse(stream)
```

### News Feed

```python
@app.get("/news/stream")
def news_stream(request):
    stream = SseStream()
    last_id = request.get_header("Last-Event-ID")

    articles = fetch_new_articles(since_id=last_id)
    for article in articles:
        stream.add(SseEvent(
            json.dumps({
                "title": article["title"],
                "summary": article["summary"],
                "url": article["url"],
            }),
            event="article",
            id=str(article["id"])
        ))

    return Response.sse(stream)
```

### Deployment Progress

```python
@app.get("/deploy/{deploy_id}/progress")
def deploy_progress(request):
    deploy_id = request.params["deploy_id"]
    stream = SseStream()

    steps = get_deploy_steps(deploy_id)
    for step in steps:
        stream.add(SseEvent(
            json.dumps({
                "step": step["name"],
                "status": step["status"],
                "progress": step["progress"],
            }),
            event="progress",
            id=str(step["id"])
        ))

    # Send completion event
    stream.add(SseEvent(
        json.dumps({"status": "complete"}),
        event="done"
    ))

    return Response.sse(stream)
```

---

## Client-Side Integration

### JavaScript (Browser)

```javascript
const source = new EventSource("/events");

source.onmessage = (event) => {
    const data = JSON.parse(event.data);
    updateUI(data);
};

source.onerror = (error) => {
    console.error("SSE error:", error);
    // Browser will automatically reconnect
};

// Close the connection when done
source.close();
```

### With Authentication

`EventSource` does not support custom headers. Use query parameters or cookies for authentication:

```python
@app.get("/events")
def authenticated_stream(request):
    token = request.query.get("token")
    if not verify_token(token):
        return Response.json({"error": "Unauthorized"}, status=401)

    stream = SseStream()
    # ... add events
    return Response.sse(stream)
```

```javascript
const source = new EventSource("/events?token=your-jwt-token");
```

---

## SSE vs WebSocket

| Feature | SSE | WebSocket |
|---------|-----|-----------|
| Direction | Server -> Client only | Bidirectional |
| Protocol | Standard HTTP | WebSocket protocol |
| Auto-reconnect | Built-in | Manual |
| Binary data | No (text only) | Yes |
| Browser support | All modern browsers | All modern browsers |
| Best for | Notifications, feeds, dashboards | Chat, games, collaboration |

---

## Next Steps

- [WebSocket](websocket.md) - Bidirectional real-time communication
- [Real-time Dashboard Example](../../examples/advanced/realtime-dashboard.md) - Complete SSE dashboard example
- [Routing](../core/routing.md) - Path parameters for SSE endpoints
