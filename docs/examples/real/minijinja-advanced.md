---
title: MiniJinja Advanced Templates
description: Template inheritance, loop loop.index, globals, and standalone template engines in Cello
---

# :material-file-code-outline: MiniJinja Advanced Templates

This example demonstrates advanced templating concepts in Cello, including template inheritance (using a base layout), loop metadata (such as `loop.index`), rendering inline template strings, and running a standalone template engine instance.

## Features Demonstrated

- **Template Inheritance**: Reusing layouts by defining `{% extends "base.html" %}` and overridden blocks.
- **Loop Metadata**: Custom loop attributes like `loop.index` to count items during iteration.
- **Standalone Template Engine**: Creating and using a secondary `MiniJinjaEngine` instance for separate plain-text email generation without escaping.
- **Inline Rendering**: Dynamically compiling and rendering inline template strings with `app.render_string()`.

## Complete Source Code

```python
"""
MiniJinja Advanced Example — Cello v1.1.0

Demonstrates:
  - Template inheritance (base → child templates)
  - Loops, conditionals, filters
  - Globals (site-wide variables)
  - Standalone MiniJinjaEngine (outside of App)
  - Inline render_string()
  - JSON API endpoint alongside HTML endpoints

Run:
    python examples/minijinja_advanced.py
Then visit:
    http://localhost:8081/             — dashboard
    http://localhost:8081/users        — user list (HTML)
    http://localhost:8081/api/users    — same data as JSON
    http://localhost:8081/email/Alice  — plain-text email preview
"""

import json
import os
import tempfile

from cello import App, MiniJinjaEngine, Response

# ---------------------------------------------------------------------------
# Templates
# ---------------------------------------------------------------------------
TEMPLATE_DIR = tempfile.mkdtemp(prefix="cello_adv_tpl_")


def tpl(name: str, content: str):
    path = os.path.join(TEMPLATE_DIR, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content)


# Base layout — shared header/footer with named blocks
tpl("base.html", """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{% block title %}{{ site_name }}{% endblock %}</title>
  <style>
    body { font-family: sans-serif; max-width: 800px; margin: 2rem auto; }
    nav a { margin-right: 1rem; }
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #ccc; padding: 0.5rem; text-align: left; }
    .badge { background: #4caf50; color: white; border-radius: 4px; padding: 2px 6px; }
    .badge.inactive { background: #9e9e9e; }
  </style>
</head>
<body>
  <header>
    <strong>{{ site_name }}</strong> — v{{ version }}
    <nav>
      <a href="/">Dashboard</a>
      <a href="/users">Users</a>
    </nav>
    <hr>
  </header>
  <main>{% block content %}{% endblock %}</main>
  <footer><hr><small>© {{ year }} {{ site_name }}</small></footer>
</body>
</html>
""")

# Dashboard child template
tpl("dashboard.html", """{% extends "base.html" %}

{% block title %}Dashboard — {{ site_name }}{% endblock %}

{% block content %}
<h1>Dashboard</h1>
<p>Total users: <strong>{{ stats.total }}</strong></p>
<p>Active users: <strong>{{ stats.active }}</strong></p>
<p>Admins: <strong>{{ stats.admins }}</strong></p>

<h2>Recent activity</h2>
{% if activity %}
<ul>
  {% for event in activity %}
  <li>{{ event }}</li>
  {% endfor %}
</ul>
{% else %}
<p><em>No recent activity.</em></p>
{% endif %}
{% endblock %}
""")

# Users list child template
tpl("users.html", """{% extends "base.html" %}

{% block title %}Users — {{ site_name }}{% endblock %}

{% block content %}
<h1>Users ({{ users | length }})</h1>
<table>
  <tr><th>#</th><th>Name</th><th>Role</th><th>Status</th></tr>
  {% for user in users %}
  <tr>
    <td>{{ loop.index }}</td>
    <td>{{ user.name }}</td>
    <td>{{ user.role | title }}</td>
    <td>
      {% if user.active %}
      <span class="badge">Active</span>
      {% else %}
      <span class="badge inactive">Inactive</span>
      {% endif %}
    </td>
  </tr>
  {% endfor %}
</table>
{% endblock %}
""")

# Plain-text email template (not HTML — auto-escape won't apply)
tpl("emails/welcome.txt", """Hi {{ name }},

Welcome to {{ site_name }}! Your account has been created.

Login at: {{ login_url }}

Regards,
The {{ site_name }} Team
""")


# ---------------------------------------------------------------------------
# Standalone engine (used outside of App, e.g. for emails)
# ---------------------------------------------------------------------------
email_engine = MiniJinjaEngine(template_dir=TEMPLATE_DIR, auto_escape=False)

# ---------------------------------------------------------------------------
# App setup
# ---------------------------------------------------------------------------
app = App()
app.enable_templates(
    template_dir=TEMPLATE_DIR,
    auto_escape=True,
    globals={
        "site_name": "CelloAdmin",
        "version": "1.1.0",
        "year": 2026,
    },
)

# Sample data
USERS = [
    {"name": "Alice",   "role": "admin",  "active": True},
    {"name": "Bob",     "role": "editor", "active": True},
    {"name": "Charlie", "role": "viewer", "active": False},
    {"name": "Diana",   "role": "editor", "active": True},
]


@app.get("/")
def dashboard(request):
    stats = {
        "total":  len(USERS),
        "active": sum(1 for u in USERS if u["active"]),
        "admins": sum(1 for u in USERS if u["role"] == "admin"),
    }
    html = app.render("dashboard.html", {
        "stats": stats,
        "activity": [
            "Alice logged in",
            "Bob updated article #42",
            "Diana created new post",
        ],
    })
    return Response.html(html)


@app.get("/users")
def users_page(request):
    html = app.render("users.html", {"users": USERS})
    return Response.html(html)


@app.get("/api/users")
def users_api(request):
    # JSON endpoint — no template needed
    return {"users": USERS, "total": len(USERS)}


@app.get("/email/{name}")
def email_preview(request):
    """Preview a plain-text welcome email (rendered via standalone engine)."""
    name = request.params["name"]
    text = email_engine.render("emails/welcome.txt", {
        "name": name,
        "site_name": "CelloAdmin",
        "login_url": "https://example.com/login",
    })
    return Response.text(text)


@app.get("/snippet")
def snippet(request):
    """Render an inline template string — no file needed."""
    items = request.params.get("items", "a,b,c").split(",")
    html = app.render_string(
        "<ul>{% for x in items %}<li>{{ x | upper }}</li>{% endfor %}</ul>",
        {"items": items},
    )
    return Response.html(html)


if __name__ == "__main__":
    print(f"Templates dir: {TEMPLATE_DIR}")
    print("Listening on http://localhost:8081")
    app.run(port=8081)
```

## Running This Example

```bash
python examples/minijinja_advanced.py
# Test endpoints:
curl http://127.0.0.1:8081/
curl http://127.0.0.1:8081/users
curl http://127.0.0.1:8081/api/users
curl http://127.0.0.1:8081/email/Alice
curl http://127.0.0.1:8081/snippet?items=red,green,blue
```

## Key Concepts

- **Base Layouts & Extends**: The `{% extends "base.html" %}` tag lets child templates inherit structured wrappers and override only specific `{% block content %}` areas.
- **Dedicated Render Engines**: You can instantiate a separate `MiniJinjaEngine` with `auto_escape=False` for plain-text formatting (e.g. email generation).
- **String Rendering**: `app.render_string()` lets you compile templates on the fly from inline Python strings, avoiding disk reads for tiny templates or dynamic components.
