---
title: Real-time Dashboard
description: Real-time dashboard with SSE for live data updates and WebSocket for bidirectional communication
---

# Real-time Dashboard

This example builds a monitoring dashboard that uses Server-Sent Events (SSE) for streaming live system metrics and WebSocket for bidirectional operator commands.

---

## Architecture

```
┌──────────────┐         SSE (one-way)         ┌──────────────┐
│              │  ──── /api/metrics/stream ───> │              │
│  Cello       │                                │  Browser     │
│  Server      │  <─── /ws/control ──────────>  │  Dashboard   │
│              │       WebSocket (two-way)       │              │
└──────────────┘                                └──────────────┘
```

- **SSE** pushes metrics (CPU, memory, request counts) every second
- **WebSocket** lets operators send control commands (reset counters, change thresholds)

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
Real-time monitoring dashboard with SSE and WebSocket.
"""

from cello import App, Response, SseStream, SseEvent, StaticFilesConfig
import json
import time
import random
import threading

app = App()
app.enable_cors()
app.enable_logging()

# ===== Simulated Metrics =====

metrics = {
    "cpu_percent": 35.0,
    "memory_percent": 60.0,
    "requests_total": 0,
    "errors_total": 0,
    "active_connections": 0,
    "alert_threshold": 80.0,
}
metrics_lock = threading.Lock()

def simulate_metrics():
    """Background thread that updates metrics."""
    while True:
        with metrics_lock:
            metrics["cpu_percent"] = max(5, min(100, metrics["cpu_percent"] + random.uniform(-5, 5)))
            metrics["memory_percent"] = max(20, min(95, metrics["memory_percent"] + random.uniform(-2, 2)))
            metrics["requests_total"] += random.randint(10, 100)
            metrics["errors_total"] += random.randint(0, 3)
        time.sleep(1)

# Start the simulation in a background thread
thread = threading.Thread(target=simulate_metrics, daemon=True)
thread.start()


# ===== Dashboard Page =====

@app.get("/")
def dashboard(request):
    return Response.html("""
    <!DOCTYPE html>
    <html>
    <head>
        <title>Real-time Dashboard</title>
        <style>
            body { font-family: monospace; background: #1a1a2e; color: #eee; padding: 20px; }
            .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 15px; }
            .card { background: #16213e; padding: 20px; border-radius: 8px; }
            .card h3 { margin: 0 0 10px 0; color: #0f3460; }
            .value { font-size: 18px; font-weight: bold; }
            .ok { color: #00d2ff; }
            .warn { color: #e94560; }
            #log { background: #0f3460; padding: 10px; max-height: 200px; overflow-y: auto; margin-top: 20px; }
            .controls { margin-top: 20px; }
            button { background: #e94560; color: white; border: none; padding: 8px 16px; cursor: pointer; margin: 5px; }
        </style>
    </head>
    <body>
        <h1>System Dashboard</h1>
        <div class="grid">
            <div class="card"><h3>CPU</h3><div id="cpu" class="value ok">--</div></div>
            <div class="card"><h3>Memory</h3><div id="mem" class="value ok">--</div></div>
            <div class="card"><h3>Requests</h3><div id="req" class="value ok">--</div></div>
            <div class="card"><h3>Errors</h3><div id="err" class="value ok">--</div></div>
            <div class="card"><h3>Connections</h3><div id="conn" class="value ok">--</div></div>
            <div class="card"><h3>Threshold</h3><div id="threshold" class="value ok">--</div></div>
        </div>
        <div class="controls">
            <button onclick="sendCmd('reset_counters')">Reset Counters</button>
            <button onclick="sendCmd('set_threshold', 90)">Set Threshold: 90%</button>
            <button onclick="sendCmd('set_threshold', 70)">Set Threshold: 70%</button>
        </div>
        <div id="log"></div>
        <script>
            // SSE for metrics
            const sse = new EventSource("/api/metrics/stream");
            sse.addEventListener("metrics", (e) => {
                const d = JSON.parse(e.data);
                document.getElementById("cpu").textContent = d.cpu_percent.toFixed(1) + "%";
                document.getElementById("mem").textContent = d.memory_percent.toFixed(1) + "%";
                document.getElementById("req").textContent = d.requests_total;
                document.getElementById("err").textContent = d.errors_total;
                document.getElementById("conn").textContent = d.active_connections;
                document.getElementById("threshold").textContent = d.alert_threshold + "%";

                // Color coding
                document.getElementById("cpu").className = "value " + (d.cpu_percent > d.alert_threshold ? "warn" : "ok");
                document.getElementById("mem").className = "value " + (d.memory_percent > d.alert_threshold ? "warn" : "ok");
            });

            sse.addEventListener("alert", (e) => {
                addLog("ALERT: " + e.data);
            });

            // WebSocket for commands
            const ws = new WebSocket("ws://" + location.host + "/ws/control");
            ws.onmessage = (e) => {
                const data = JSON.parse(e.data);
                addLog(data.message);
            };

            function sendCmd(cmd, value) {
                ws.send(JSON.stringify({command: cmd, value: value}));
            }

            function addLog(msg) {
                const log = document.getElementById("log");
                const ts = new Date().toLocaleTimeString();
                log.innerHTML += "[" + ts + "] " + msg + "<br>";
                log.scrollTop = log.scrollHeight;
            }
        </script>
    </body>
    </html>
    """)


# ===== SSE Metrics Stream =====

@app.get("/api/metrics/stream")
def metrics_stream(request):
    stream = SseStream()

    # Send initial config
    stream.add(SseEvent(
        json.dumps({"interval_ms": 1000}),
        event="config",
        retry=5000,
    ))

    # Stream current metrics
    for i in range(60):
        with metrics_lock:
            snapshot = dict(metrics)

        stream.add(SseEvent(
            json.dumps(snapshot),
            event="metrics",
            id=str(i),
        ))

        # Check for alerts
        if snapshot["cpu_percent"] > snapshot["alert_threshold"]:
            stream.add(SseEvent(
                f"CPU usage at {snapshot['cpu_percent']:.1f}%",
                event="alert",
            ))

    return Response.sse(stream)


# ===== WebSocket Control Channel =====

@app.websocket("/ws/control")
def control_handler(ws):
    with metrics_lock:
        metrics["active_connections"] += 1

    ws.send_text(json.dumps({"message": "Control channel connected"}))

    while True:
        msg = ws.recv()
        if msg is None or msg.is_close():
            break

        try:
            data = json.loads(msg.text)
            command = data.get("command")

            if command == "reset_counters":
                with metrics_lock:
                    metrics["requests_total"] = 0
                    metrics["errors_total"] = 0
                ws.send_text(json.dumps({"message": "Counters reset"}))

            elif command == "set_threshold":
                value = float(data.get("value", 80))
                with metrics_lock:
                    metrics["alert_threshold"] = value
                ws.send_text(json.dumps({"message": f"Threshold set to {value}%"}))

            else:
                ws.send_text(json.dumps({"message": f"Unknown command: {command}"}))

        except Exception as e:
            ws.send_text(json.dumps({"message": f"Error: {str(e)}"}))

    with metrics_lock:
        metrics["active_connections"] -= 1


# ===== REST API for Snapshots =====

@app.get("/api/metrics")
def get_metrics(request):
    with metrics_lock:
        return dict(metrics)


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Running

```bash
python realtime_dashboard.py
```

Open `http://127.0.0.1:8000/` in your browser to see the live dashboard.

---

## Key Patterns

### SSE for Metrics

The `/api/metrics/stream` endpoint pushes metrics as named `metrics` events. The client uses `addEventListener("metrics", ...)` to handle them. Alert events are sent on a separate `alert` channel.

### WebSocket for Commands

The `/ws/control` endpoint accepts JSON commands from the browser. Operators can reset counters or change alert thresholds in real time.

### Thread-Safe Metrics

A `threading.Lock` protects the shared `metrics` dictionary, allowing the background simulation thread and request handlers to access it safely.

---

## Next Steps

- [WebSocket](../../features/realtime/websocket.md) - WebSocket documentation
- [SSE](../../features/realtime/sse.md) - Server-Sent Events documentation
- [Full-stack App](fullstack.md) - Combine with REST API and templates
