---
title: Streaming & Server-Sent Events
description: Use SseStream and SseEvent to push real-time events from a Cello server to browser clients over a persistent HTTP connection.
---

# :material-broadcast: Streaming & Server-Sent Events

Server-Sent Events (SSE) let a server push a sequence of messages to a client over a single long-lived HTTP connection — no WebSocket handshake required. Cello provides `SseStream` and `SseEvent` to build these streams declaratively: you add named events to a stream object and return it from a route handler, and Cello handles the `text/event-stream` content type, chunked transfer encoding, and correct newline framing automatically.

## Features Demonstrated

- `SseStream` — collects an ordered sequence of SSE frames to be flushed as a streaming response
- `SseEvent` — named event objects carrying arbitrary string data
- `stream.add_data(text)` — appends a bare `data:` frame (no event name)
- `stream.add_event(name, json_string)` — appends a named event with a JSON payload
- Multiple independent SSE endpoints on the same app (`/sse/events`, `/sse/counter`, `/sse/stock-ticker`)
- Large non-streaming JSON response (`/large-response`) for comparison benchmarking
- `app.enable_cors()` and `app.enable_logging()` working alongside streaming routes

## Complete Source Code

```python
#!/usr/bin/env python3
"""
Streaming Responses Demo for Cello v1.0.1.
Run with: python examples/streaming_demo.py
Test:
    curl -N http://127.0.0.1:8000/sse/events
    curl -N http://127.0.0.1:8000/sse/counter
    Open http://127.0.0.1:8000/demo in browser
"""

from cello import App, Response, SseEvent, SseStream
import json

app = App()
app.enable_cors()
app.enable_logging()

@app.get("/")
def home(request):
    return {"message": "Cello Streaming Demo", "endpoints": ["/sse/events", "/sse/counter", "/sse/stock-ticker", "/large-response", "/demo"]}

@app.get("/sse/events")
def sse_events(request):
    stream = SseStream()
    stream.add_data("Connected to SSE stream")
    stream.add_event("welcome", '{"message": "Welcome to Cello SSE!"}')
    stream.add_event("update", '{"status": "ready"}')
    stream.add_event("notification", '{"type": "info", "text": "Server is running"}')
    return stream

@app.get("/sse/counter")
def sse_counter(request):
    stream = SseStream()
    for i in range(1, 6):
        stream.add_event("count", f'{{"value": {i}}}')
    stream.add_event("complete", '{"message": "Counter finished"}')
    return stream

@app.get("/sse/stock-ticker")
def sse_stock_ticker(request):
    stream = SseStream()
    stocks = [
        {"symbol": "AAPL", "price": 150.25, "change": 2.5},
        {"symbol": "GOOGL", "price": 2800.50, "change": -15.0},
        {"symbol": "MSFT", "price": 280.75, "change": 5.25},
    ]
    for stock in stocks:
        stream.add_event("ticker", json.dumps(stock))
    return stream

@app.get("/large-response")
def large_response(request):
    items = [{"id": i, "name": f"Item {i}", "description": f"Desc for item {i}" * 3,
              "price": round(10.0 + i * 0.5, 2)} for i in range(1000)]
    return {"total": len(items), "items": items}

if __name__ == "__main__":
    print("Open http://127.0.0.1:8000/demo for interactive SSE demo")
    app.run(host="127.0.0.1", port=8000)
```

## Running This Example

```bash
python examples/streaming_demo.py
# Test:
curl http://127.0.0.1:8000/
```

## Key Concepts

- **`curl -N`** — the `-N` flag disables curl's internal buffering so SSE frames are printed to the terminal as they arrive, rather than being held until the connection closes.
- **`add_data` vs `add_event`** — `add_data` emits an anonymous `data:` frame that browsers receive as a `message` event; `add_event` emits a named `event:` line followed by `data:`, which lets client-side `EventSource` listeners filter by event type.
- **Returning `SseStream` from a handler** — Cello detects the `SseStream` return type and automatically sets `Content-Type: text/event-stream`, disables response buffering, and streams each frame as it is added, so the pattern is identical to returning a plain dict for JSON responses.
- **`json.dumps(stock)` in the stock ticker** — SSE data payloads are plain strings; serialising dicts with `json.dumps` before passing them to `add_event` keeps the handler explicit about what is sent over the wire and avoids hidden serialisation surprises.
- **SSE vs WebSockets** — SSE is unidirectional (server → client) and works over plain HTTP/1.1 with automatic reconnection built into the browser `EventSource` API, making it a lighter-weight choice than WebSockets for push notifications, live feeds, and progress reporting.
