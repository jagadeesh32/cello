---
title: "Tutorial: Build a Chat App"
description: Step-by-step guide to building a real-time chat application with Cello WebSockets
---

# Tutorial: Build a Chat App

In this tutorial you will build a real-time chat application using Cello's WebSocket support. You will learn how to create WebSocket handlers, broadcast messages to all connected clients, track connections, handle disconnects, and serve a simple HTML client.

---

## Prerequisites

- Python 3.12 or later
- Cello installed (`pip install cello-framework`)

---

## Step 1: Project Setup

```bash
mkdir chat-app && cd chat-app
python -m venv .venv
source .venv/bin/activate
pip install cello-framework
touch app.py
```

---

## Step 2: Initialize the App and Client Store

```python
from cello import App, Response

app = App()

# Track connected WebSocket clients
clients = {}  # ws_id -> {"ws": ws, "username": str}
```

The `clients` dictionary maps each WebSocket connection to metadata about the user.

---

## Step 3: Create the WebSocket Handler

```python
import json

@app.websocket("/ws/chat")
def chat_handler(ws):
    """Handle a single WebSocket connection."""
    ws_id = id(ws)
    username = f"User-{ws_id % 10000}"

    # Register the client
    clients[ws_id] = {"ws": ws, "username": username}
    broadcast({"type": "system", "message": f"{username} joined the chat"})

    try:
        while True:
            msg = ws.recv()
            if msg is None:
                break  # Client disconnected

            # Parse the incoming message
            try:
                data = json.loads(msg.text)
            except (json.JSONDecodeError, AttributeError):
                data = {"message": msg.text}

            # Handle special commands
            if data.get("type") == "set_name":
                old_name = username
                username = data["name"]
                clients[ws_id]["username"] = username
                broadcast({
                    "type": "system",
                    "message": f"{old_name} is now known as {username}",
                })
                continue

            # Broadcast the chat message
            broadcast({
                "type": "chat",
                "username": username,
                "message": data.get("message", ""),
            })
    finally:
        # Clean up on disconnect
        del clients[ws_id]
        broadcast({"type": "system", "message": f"{username} left the chat"})
```

!!! note
    `ws.recv()` blocks until a message arrives or the client disconnects (returns `None`). The `finally` block guarantees cleanup even on unexpected errors.

---

## Step 4: Broadcast Messages

```python
def broadcast(payload: dict):
    """Send a JSON message to every connected client."""
    text = json.dumps(payload)
    disconnected = []

    for ws_id, client in clients.items():
        try:
            client["ws"].send_text(text)
        except Exception:
            disconnected.append(ws_id)

    # Remove any clients that failed to receive
    for ws_id in disconnected:
        clients.pop(ws_id, None)
```

---

## Step 5: Serve the HTML Client

Instead of a separate frontend project, serve a self-contained HTML page directly from a Cello route.

```python
@app.get("/")
def index(request):
    """Serve the chat client."""
    return Response.html(CHAT_HTML)
```

Define the HTML template as a module-level string.

```python
CHAT_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Cello Chat</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: system-ui, sans-serif; display: flex;
           flex-direction: column; height: 100vh; }
    #messages { flex: 1; overflow-y: auto; padding: 1rem; }
    .msg { margin-bottom: 0.5rem; }
    .msg.system { color: #888; font-style: italic; }
    .msg .user { font-weight: bold; }
    #form { display: flex; padding: 0.5rem; border-top: 1px solid #ccc; }
    #form input { flex: 1; padding: 0.5rem; font-size: 1rem; }
    #form button { padding: 0.5rem 1rem; font-size: 1rem; }
  </style>
</head>
<body>
  <div id="messages"></div>
  <form id="form">
    <input id="input" autocomplete="off" placeholder="Type a message..." />
    <button type="submit">Send</button>
  </form>
  <script>
    const messages = document.getElementById('messages');
    const form = document.getElementById('form');
    const input = document.getElementById('input');

    const ws = new WebSocket(`ws://${location.host}/ws/chat`);

    ws.onmessage = (event) => {
      const data = JSON.parse(event.data);
      const div = document.createElement('div');
      div.classList.add('msg');
      if (data.type === 'system') {
        div.classList.add('system');
        div.textContent = data.message;
      } else {
        div.innerHTML = `<span class="user">${data.username}:</span> ${data.message}`;
      }
      messages.appendChild(div);
      messages.scrollTop = messages.scrollHeight;
    };

    form.addEventListener('submit', (e) => {
      e.preventDefault();
      const text = input.value.trim();
      if (!text) return;

      if (text.startsWith('/name ')) {
        ws.send(JSON.stringify({ type: 'set_name', name: text.slice(6) }));
      } else {
        ws.send(JSON.stringify({ type: 'chat', message: text }));
      }
      input.value = '';
    });
  </script>
</body>
</html>"""
```

---

## Step 6: Add an Online Users Endpoint

Provide a REST endpoint that returns the list of currently connected users.

```python
@app.get("/users")
def online_users(request):
    """Return currently connected usernames."""
    return {
        "users": [c["username"] for c in clients.values()],
        "count": len(clients),
    }
```

---

## Step 7: Run the Application

```python
if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

Start the server and open **http://127.0.0.1:8000** in two or more browser tabs.

```bash
python app.py
```

---

## Testing the Chat

1. Open two browser tabs at `http://127.0.0.1:8000`.
2. Type a message in one tab and press **Send**. It appears in both tabs.
3. Type `/name Alice` to change your display name.
4. Close one tab -- the other receives a "left the chat" system message.
5. Call the REST endpoint to see who is online:

```bash
curl http://127.0.0.1:8000/users
```

---

## Handling Disconnect Edge Cases

Cello's WebSocket layer runs in Rust, so the TCP connection is monitored at the OS level. When a client disconnects (even abruptly), `ws.recv()` returns `None` and the `finally` block fires. For additional resilience you can implement a heartbeat.

```python
import time
import threading

def heartbeat_loop():
    """Periodically ping all clients to detect stale connections."""
    while True:
        time.sleep(30)
        stale = []
        for ws_id, client in list(clients.items()):
            try:
                client["ws"].send_text('{"type":"ping"}')
            except Exception:
                stale.append(ws_id)
        for ws_id in stale:
            clients.pop(ws_id, None)

threading.Thread(target=heartbeat_loop, daemon=True).start()
```

---

## Next Steps

- Add [JWT authentication](auth-system.md) so users must log in before chatting.
- Use [Blueprints](../../reference/api/blueprint.md) to separate chat routes from other parts of your application.
- Explore [SSE](../../reference/api/response.md) for one-way server-to-client streaming.
