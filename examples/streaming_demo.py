#!/usr/bin/env python3
"""
Streaming Responses Demo for Cello v0.4.0.

This example demonstrates streaming capabilities including:
- Server-Sent Events (SSE)
- Large response streaming
- Real-time data feeds

Run with:
    python examples/streaming_demo.py

Then test with:
    curl http://127.0.0.1:8000/
    curl http://127.0.0.1:8000/sse/events
    curl http://127.0.0.1:8000/sse/counter
"""

from cello import App, Response, SseEvent, SseStream

app = App()

# Enable middleware
app.enable_cors()
app.enable_logging()


# =============================================================================
# Routes
# =============================================================================


@app.get("/")
def home(request):
    """Home endpoint showing streaming features."""
    return {
        "message": "Cello Streaming Demo",
        "version": "0.4.0",
        "features": {
            "sse": "Server-Sent Events for real-time updates",
            "streaming": "Large response streaming",
        },
        "endpoints": [
            "GET  /                  - This overview",
            "GET  /sse/events        - SSE event stream",
            "GET  /sse/counter       - SSE counter stream",
            "GET  /sse/notifications - SSE notifications",
            "GET  /large-response    - Large streamed response",
        ],
        "usage": {
            "sse": "Use EventSource in JavaScript or curl to consume",
            "example": "const es = new EventSource('/sse/events')",
        },
    }


@app.get("/sse/events")
def sse_events(request):
    """
    Server-Sent Events stream.
    
    Demonstrates basic SSE functionality with different event types.
    
    JavaScript usage:
        const es = new EventSource('/sse/events');
        es.onmessage = (e) => console.log(e.data);
        es.addEventListener('update', (e) => console.log(e.data));
    
    Curl usage:
        curl -N http://127.0.0.1:8000/sse/events
    """
    stream = SseStream()
    
    # Add different types of events
    stream.add_data("Connected to SSE stream")
    stream.add_event("welcome", '{"message": "Welcome to Cello SSE!"}')
    stream.add_event("update", '{"status": "ready", "timestamp": "2024-12-16T00:00:00Z"}')
    stream.add_event("notification", '{"type": "info", "text": "Server is running"}')
    stream.add_data("Stream initialized successfully")
    
    return stream


@app.get("/sse/counter")
def sse_counter(request):
    """
    SSE counter stream.
    
    Demonstrates a simple counter that would increment in real-time.
    """
    stream = SseStream()
    
    # Simulate counter events
    for i in range(1, 6):
        stream.add_event("count", f'{{"value": {i}}}')
    
    stream.add_event("complete", '{"message": "Counter finished"}')
    
    return stream


@app.get("/sse/notifications")
def sse_notifications(request):
    """
    SSE notifications stream.
    
    Demonstrates notification-style events.
    """
    stream = SseStream()
    
    # Different notification types
    notifications = [
        {"type": "info", "title": "Welcome", "message": "You are now connected"},
        {"type": "success", "title": "Synced", "message": "Data synchronized successfully"},
        {"type": "warning", "title": "Attention", "message": "Rate limit approaching"},
        {"type": "info", "title": "Update", "message": "New version available"},
    ]
    
    for i, notif in enumerate(notifications):
        event = SseEvent.with_event("notification", str(notif).replace("'", '"'))
        stream.add(event)
    
    return stream


@app.get("/sse/stock-ticker")
def sse_stock_ticker(request):
    """
    SSE stock ticker simulation.
    
    Demonstrates real-time data feed pattern.
    """
    stream = SseStream()
    
    # Simulated stock data
    stocks = [
        {"symbol": "AAPL", "price": 150.25, "change": 2.5},
        {"symbol": "GOOGL", "price": 2800.50, "change": -15.0},
        {"symbol": "AMZN", "price": 3200.00, "change": 45.0},
        {"symbol": "MSFT", "price": 280.75, "change": 5.25},
        {"symbol": "TSLA", "price": 950.00, "change": -20.0},
    ]
    
    for stock in stocks:
        import json
        stream.add_event("ticker", json.dumps(stock))
    
    return stream


@app.get("/large-response")
def large_response(request):
    """
    Large response demonstration.
    
    This endpoint returns a larger JSON response.
    With compression enabled, it will be gzipped.
    """
    items = []
    for i in range(1000):
        items.append({
            "id": i,
            "name": f"Item {i}",
            "description": f"This is a detailed description for item number {i}. " * 3,
            "category": f"Category {i % 10}",
            "price": round(10.0 + (i * 0.5), 2),
            "in_stock": i % 2 == 0,
            "tags": [f"tag{i % 5}", f"tag{(i + 1) % 5}", f"tag{(i + 2) % 5}"],
        })
    
    return {
        "total": len(items),
        "items": items,
        "note": "This response will be compressed if Accept-Encoding: gzip is sent",
    }


# =============================================================================
# SSE Event Types Reference
# =============================================================================


@app.get("/sse/reference")
def sse_reference(request):
    """SSE implementation reference."""
    return {
        "sse_format": {
            "data_only": "data: message\\n\\n",
            "with_event": "event: eventname\\ndata: message\\n\\n",
            "with_id": "id: 123\\ndata: message\\n\\n",
            "with_retry": "retry: 3000\\ndata: message\\n\\n",
            "multiline": "data: line1\\ndata: line2\\n\\n",
        },
        "python_api": {
            "SseStream": "Container for SSE events",
            "SseEvent": "Single SSE event",
            "SseEvent.data(msg)": "Create data-only event",
            "SseEvent.with_event(name, msg)": "Create named event",
            "stream.add(event)": "Add event to stream",
            "stream.add_data(msg)": "Add data-only event",
            "stream.add_event(name, msg)": "Add named event",
        },
        "javascript_client": {
            "connect": "const es = new EventSource('/sse/events')",
            "on_message": "es.onmessage = (e) => console.log(e.data)",
            "on_event": "es.addEventListener('update', (e) => {...})",
            "close": "es.close()",
        },
    }


# =============================================================================
# HTML Demo Page
# =============================================================================


@app.get("/demo")
def demo_page(request):
    """HTML page demonstrating SSE consumption."""
    html = """
    <!DOCTYPE html>
    <html>
    <head>
        <title>Cello SSE Demo</title>
        <style>
            body { font-family: -apple-system, sans-serif; padding: 20px; max-width: 800px; margin: 0 auto; }
            h1 { color: #333; }
            .events { background: #f5f5f5; padding: 15px; border-radius: 8px; margin: 20px 0; }
            .event { background: white; padding: 10px; margin: 5px 0; border-radius: 4px; border-left: 4px solid #2196F3; }
            .event.notification { border-left-color: #4CAF50; }
            .event.ticker { border-left-color: #FF9800; }
            button { background: #2196F3; color: white; border: none; padding: 10px 20px; border-radius: 4px; cursor: pointer; margin: 5px; }
            button:hover { background: #1976D2; }
            button.stop { background: #f44336; }
            button.stop:hover { background: #d32f2f; }
            .status { padding: 10px; border-radius: 4px; margin: 10px 0; }
            .status.connected { background: #e8f5e9; color: #2e7d32; }
            .status.disconnected { background: #ffebee; color: #c62828; }
        </style>
    </head>
    <body>
        <h1>🎵 Cello SSE Demo</h1>
        
        <div id="status" class="status disconnected">Disconnected</div>
        
        <div>
            <button onclick="connectEvents()">Connect to Events</button>
            <button onclick="connectTicker()">Connect to Ticker</button>
            <button onclick="disconnect()" class="stop">Disconnect</button>
            <button onclick="clearEvents()">Clear</button>
        </div>
        
        <div class="events" id="events">
            <p>Events will appear here...</p>
        </div>
        
        <script>
            let eventSource = null;
            
            function updateStatus(connected) {
                const status = document.getElementById('status');
                status.className = 'status ' + (connected ? 'connected' : 'disconnected');
                status.textContent = connected ? 'Connected' : 'Disconnected';
            }
            
            function addEvent(type, data) {
                const events = document.getElementById('events');
                const div = document.createElement('div');
                div.className = 'event ' + type;
                div.innerHTML = '<strong>' + type + ':</strong> ' + data;
                events.insertBefore(div, events.firstChild);
            }
            
            function connectEvents() {
                disconnect();
                eventSource = new EventSource('/sse/events');
                
                eventSource.onopen = () => updateStatus(true);
                eventSource.onerror = () => updateStatus(false);
                
                eventSource.onmessage = (e) => addEvent('message', e.data);
                eventSource.addEventListener('welcome', (e) => addEvent('welcome', e.data));
                eventSource.addEventListener('update', (e) => addEvent('update', e.data));
                eventSource.addEventListener('notification', (e) => addEvent('notification', e.data));
            }
            
            function connectTicker() {
                disconnect();
                eventSource = new EventSource('/sse/stock-ticker');
                
                eventSource.onopen = () => updateStatus(true);
                eventSource.onerror = () => updateStatus(false);
                
                eventSource.addEventListener('ticker', (e) => {
                    const data = JSON.parse(e.data);
                    const change = data.change >= 0 ? '+' + data.change : data.change;
                    addEvent('ticker', data.symbol + ': $' + data.price + ' (' + change + ')');
                });
            }
            
            function disconnect() {
                if (eventSource) {
                    eventSource.close();
                    eventSource = null;
                }
                updateStatus(false);
            }
            
            function clearEvents() {
                document.getElementById('events').innerHTML = '<p>Events will appear here...</p>';
            }
        </script>
    </body>
    </html>
    """
    return Response.html(html)


if __name__ == "__main__":
    print("📡 Cello Streaming Demo")
    print()
    print("   Try these endpoints:")
    print("   - GET  http://127.0.0.1:8000/              - Overview")
    print("   - GET  http://127.0.0.1:8000/demo          - HTML demo page")
    print("   - GET  http://127.0.0.1:8000/sse/events    - SSE events")
    print("   - GET  http://127.0.0.1:8000/sse/counter   - SSE counter")
    print("   - GET  http://127.0.0.1:8000/large-response - Large response")
    print()
    print("   Open http://127.0.0.1:8000/demo in a browser for interactive demo")
    print()
    app.run(host="127.0.0.1", port=8000)
