---
title: Full-Stack Application
description: Full-stack app example with REST API, WebSocket, templates, and static files
---

# Full-Stack Application

This example builds a complete full-stack application combining a REST API, WebSocket chat, HTML templates, and static file serving -- all in a single Cello application.

---

## Project Structure

```
fullstack-app/
    app.py
    templates/
        index.html
        chat.html
    public/
        css/
            style.css
        js/
            chat.js
```

---

## Application Code

```python
#!/usr/bin/env python3
"""
Full-stack Cello application with REST API, WebSocket, templates, static files.
"""

from cello import App, Response, Blueprint, TemplateEngine, StaticFilesConfig
import json
import time

app = App()
engine = TemplateEngine("templates")

# ===== Middleware =====
app.enable_cors()
app.enable_logging()
app.enable_compression()
app.enable_static_files(StaticFilesConfig("/static", "./public"))

# ===== In-Memory Data =====
messages = []
users = {
    "1": {"id": "1", "name": "Alice", "role": "admin"},
    "2": {"id": "2", "name": "Bob", "role": "user"},
}

# ===== HTML Pages =====

@app.get("/")
def index(request):
    html = engine.render("index.html", {
        "title": "Cello Full-Stack App",
        "user_count": len(users),
        "message_count": len(messages),
    })
    return Response.html(html)

@app.get("/chat")
def chat_page(request):
    html = engine.render("chat.html", {
        "title": "Live Chat",
    })
    return Response.html(html)

# ===== REST API =====

api = Blueprint("/api")

@api.get("/users")
def list_users(request):
    return {"users": list(users.values())}

@api.get("/users/{id}")
def get_user(request):
    user_id = request.params["id"]
    user = users.get(user_id)
    if not user:
        return Response.json({"error": "Not found"}, status=404)
    return user

@api.post("/users")
def create_user(request):
    data = request.json()
    user_id = str(len(users) + 1)
    user = {"id": user_id, "name": data["name"], "role": data.get("role", "user")}
    users[user_id] = user
    return Response.json(user, status=201)

@api.get("/messages")
def list_messages(request):
    limit = int(request.query.get("limit", "50"))
    return {"messages": messages[-limit:], "total": len(messages)}

app.register_blueprint(api)

# ===== WebSocket Chat =====

clients = []

@app.websocket("/ws/chat")
def chat_handler(ws):
    clients.append(ws)
    ws.send_text(json.dumps({
        "type": "system",
        "text": "Connected to chat",
        "online": len(clients),
    }))

    while True:
        msg = ws.recv()
        if msg is None or msg.is_close():
            break

        data = json.loads(msg.text)
        chat_msg = {
            "type": "message",
            "user": data.get("user", "Anonymous"),
            "text": data.get("text", ""),
            "timestamp": time.time(),
        }
        messages.append(chat_msg)

        for client in clients:
            try:
                client.send_text(json.dumps(chat_msg))
            except Exception:
                pass

    clients.remove(ws)
    for client in clients:
        try:
            client.send_text(json.dumps({
                "type": "system",
                "text": "A user left",
                "online": len(clients),
            }))
        except Exception:
            pass

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Templates

### `templates/index.html`

```html
<!DOCTYPE html>
<html>
<head>
    <title>{{ title }}</title>
    <link rel="stylesheet" href="/static/css/style.css">
</head>
<body>
    <h1>{{ title }}</h1>
    <div class="stats">
        <p>Users: {{ user_count }}</p>
        <p>Messages: {{ message_count }}</p>
    </div>
    <nav>
        <a href="/chat">Live Chat</a>
        <a href="/api/users">API: Users</a>
        <a href="/api/messages">API: Messages</a>
    </nav>
</body>
</html>
```

### `templates/chat.html`

```html
<!DOCTYPE html>
<html>
<head>
    <title>{{ title }}</title>
    <link rel="stylesheet" href="/static/css/style.css">
</head>
<body>
    <h1>{{ title }}</h1>
    <div id="messages"></div>
    <form id="chat-form">
        <input id="username" placeholder="Your name" required>
        <input id="message" placeholder="Type a message..." required>
        <button type="submit">Send</button>
    </form>
    <script src="/static/js/chat.js"></script>
</body>
</html>
```

---

## Static Files

### `public/css/style.css`

```css
body { font-family: sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; }
nav a { margin-right: 15px; }
#messages { border: 1px solid #ccc; height: 400px; overflow-y: auto; padding: 10px; }
.msg { margin: 5px 0; }
.system { color: #888; font-style: italic; }
```

### `public/js/chat.js`

```javascript
const ws = new WebSocket(`ws://${location.host}/ws/chat`);
const messagesDiv = document.getElementById("messages");

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    const div = document.createElement("div");
    div.className = data.type === "system" ? "msg system" : "msg";
    div.textContent = data.type === "system"
        ? data.text
        : `${data.user}: ${data.text}`;
    messagesDiv.appendChild(div);
    messagesDiv.scrollTop = messagesDiv.scrollHeight;
};

document.getElementById("chat-form").onsubmit = (e) => {
    e.preventDefault();
    const user = document.getElementById("username").value;
    const text = document.getElementById("message").value;
    ws.send(JSON.stringify({ user, text }));
    document.getElementById("message").value = "";
};
```

---

## Running

```bash
python app.py
```

- Home page: `http://127.0.0.1:8000/`
- Chat page: `http://127.0.0.1:8000/chat`
- Users API: `http://127.0.0.1:8000/api/users`
- Messages API: `http://127.0.0.1:8000/api/messages`

---

## Next Steps

- [Microservices](microservices.md) - Split into separate services
- [Real-time Dashboard](realtime-dashboard.md) - SSE-powered dashboard
- [Templates](../../features/advanced/templates.md) - Template engine documentation
