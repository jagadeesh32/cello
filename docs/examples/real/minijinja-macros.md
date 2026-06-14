---
title: MiniJinja UI Component Macros
description: Reusable UI component libraries, stat cards, alerts, badges, and custom buttons using MiniJinja Macros in Cello
---

# :material-puzzle-outline: MiniJinja UI Component Macros

This example demonstrates how to build and organize reusable UI components using Jinja2/MiniJinja macros. It covers defining macros with optional parameters, importing helper libraries, using call blocks, and structuring dashboard layouts entirely from components.

## Features Demonstrated

- **Reusable Component Blocks**: Defining dynamic layouts for alerts, badges, buttons, data tables, and cards.
- **Library Imports**: Importing macro libraries using `{% from "macros/ui.html" import ... %}`.
- **Call Containers**: Using `{% call card(...) %}` blocks to wrap child template fragments inside parent macro markups.
- **Dynamic Conditional Styling**: Formatting alternating table row striping using `{% if loop.index is odd %}`.

## Complete Source Code

```python
"""
MiniJinja Macros Example — Cello v1.1.0

Shows how to build reusable UI components using Jinja2 macros:
  - {% macro %} / {% call %} — define and use reusable snippets
  - {% import %} — import macros from a shared library template
  - alert(), badge(), card(), btn(), stat_card() components
  - A dashboard page that assembles everything from macros

Run:
    python examples/minijinja_macros.py
Then visit:
    http://localhost:8084/
    http://localhost:8084/components   ← macro showcase / component library
"""

import os
import tempfile

from cello import App, Response

TEMPLATE_DIR = tempfile.mkdtemp(prefix="cello_macros_")


def tpl(name, content):
    path = os.path.join(TEMPLATE_DIR, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content)


# ---------------------------------------------------------------------------
# Macro library — imported by other templates, not rendered directly
# ---------------------------------------------------------------------------
tpl("macros/ui.html", """\
{# ===== Alert ===== #}
{# variant: success | warning | error | info #}
{% macro alert(message, variant="info", dismissible=false) %}
{% set colors = {
  "success": "#e8f5e9;border-color:#66bb6a",
  "warning": "#fff8e1;border-color:#ffa726",
  "error":   "#ffebee;border-color:#ef5350",
  "info":    "#e3f2fd;border-color:#42a5f5"
} %}
<div role="alert" style="background:{{ colors[variant] | default('#e3f2fd;border-color:#42a5f5') }};
     border:1px solid;border-radius:4px;padding:0.75rem 1rem;margin:0.75rem 0;">
  {% if dismissible %}<span style="float:right;cursor:pointer" onclick="this.parentNode.remove()">✕</span>{% endif %}
  {{ message }}
</div>
{% endmacro %}

{# ===== Badge ===== #}
{% macro badge(text, color="#1a73e8") %}
<span style="display:inline-block;background:{{ color }};color:#fff;
     border-radius:3px;padding:1px 8px;font-size:0.78rem;
     font-family:monospace;">{{ text }}</span>
{% endmacro %}

{# ===== Button ===== #}
{% macro btn(label, href="#", variant="primary") %}
{% set styles = {
  "primary":   "background:#1a73e8;color:#fff",
  "secondary": "background:#757575;color:#fff",
  "danger":    "background:#d32f2f;color:#fff",
  "ghost":     "background:transparent;color:#1a73e8;border:1px solid #1a73e8"
} %}
<a href="{{ href }}"
   style="{{ styles[variant] | default(styles['primary']) }};
          padding:0.45rem 1.1rem;border-radius:4px;
          text-decoration:none;font-size:0.95rem;display:inline-block;">
  {{ label }}
 </a>
{% endmacro %}

{# ===== Card ===== #}
{% macro card(title, footer=none) %}
<div style="border:1px solid #e0e0e0;border-radius:6px;
            overflow:hidden;margin-bottom:1.25rem;">
  {% if title %}
  <div style="background:#f5f5f5;padding:0.6rem 1rem;
              font-weight:bold;border-bottom:1px solid #e0e0e0;">
    {{ title }}
  </div>
  {% endif %}
  <div style="padding:1rem;">{{ caller() }}</div>
  {% if footer %}
  <div style="background:#fafafa;padding:0.5rem 1rem;
              border-top:1px solid #e0e0e0;font-size:0.85rem;color:#666;">
    {{ footer }}
  </div>
  {% endif %}
</div>
{% endmacro %}

{# ===== Stat card ===== #}
{% macro stat_card(label, value, unit="", delta=none, delta_positive=true) %}
<div style="border:1px solid #e0e0e0;border-radius:6px;padding:1rem;
            text-align:center;background:#fff;">
  <div style="font-size:0.85rem;color:#888;margin-bottom:0.3rem;">{{ label }}</div>
  <div style="font-size:2rem;font-weight:bold;">{{ value }}<small style="font-size:1rem;color:#888;">{{ unit }}</small></div>
  {% if delta is not none %}
  <div style="font-size:0.82rem;color:{{ '#2e7d32' if delta_positive else '#c62828' }}">
    {{ '▲' if delta_positive else '▼' }} {{ delta }}
  </div>
  {% endif %}
</div>
{% endmacro %}

{# ===== Table ===== #}
{% macro data_table(headers, rows, empty_msg="No data.") %}
{% if rows %}
<table style="width:100%;border-collapse:collapse;font-size:0.92rem;">
  <thead>
    <tr>
      {% for h in headers %}
      <th style="border-bottom:2px solid #ccc;padding:0.5rem;text-align:left;">{{ h }}</th>
      {% endfor %}
    </tr>
  </thead>
  <tbody>
    {% for row in rows %}
    <tr style="{% if loop.index is odd %}background:#fafafa{% endif %}">
      {% for cell in row %}
      <td style="border-bottom:1px solid #eee;padding:0.45rem 0.5rem;">{{ cell }}</td>
      {% endfor %}
    </tr>
    {% endfor %}
  </tbody>
</table>
{% else %}
<p style="color:#888;">{{ empty_msg }}</p>
{% endif %}
{% endmacro %}
""")

# ---------------------------------------------------------------------------
# Page templates
# ---------------------------------------------------------------------------
tpl("base_macros.html", """\
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{% block title %}Cello Macros{% endblock %}</title>
  <style>
    body  { font-family: Arial, sans-serif; margin: 0; color: #222; }
    .wrap { max-width: 960px; margin: 0 auto; padding: 0 1.5rem; }
    nav   { background: #1a1a2e; padding: 0.8rem 0; }
    nav a { color: #a8d8ea; margin-right: 1.5rem; text-decoration: none; }
    h1    { margin: 1.5rem 0 0.5rem; }
    h2    { margin: 2rem 0 0.75rem; border-bottom: 1px solid #eee; padding-bottom: 0.4rem; }
    .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 1rem; }
    code  { background: #f5f5f5; padding: 2px 5px; border-radius: 3px; font-size: 0.88rem; }
  </style>
</head>
<body>
  <nav class="wrap">
    <a href="/">Dashboard</a>
    <a href="/components">Component Library</a>
  </nav>
  <div class="wrap">
    {% block content %}{% endblock %}
  </div>
</body>
</html>
""")

# Dashboard template
tpl("dashboard_macros.html", """\
{% extends "base_macros.html" %}
{% from "macros/ui.html" import alert, badge, btn, card, stat_card, data_table %}

{% block title %}Dashboard — Macro Demo{% endblock %}

{% block content %}
<h1>Dashboard</h1>

{# Inline alerts #}
{{ alert("Deployment to production succeeded.", variant="success") }}
{{ alert("Certificate expires in 14 days.", variant="warning", dismissible=true) }}

<h2>Stats</h2>
<div class="grid">
  {{ stat_card("Requests / s",   "12.4k", delta="8%",  delta_positive=true) }}
  {{ stat_card("Avg Latency",    "2.3",   unit="ms",   delta="0.4ms", delta_positive=false) }}
  {{ stat_card("Active Users",   "841") }}
  {{ stat_card("Error Rate",     "0.12",  unit="%",    delta="0.03%", delta_positive=false) }}
</div>

<h2>Recent Requests</h2>
{% call card(title="Last 5 requests", footer="Updated just now") %}
  {{ data_table(
    headers=["Method", "Path", "Status", "Latency"],
    rows=requests
  ) }}
{% endcall %}

<h2>Deployed Services</h2>
{% for svc in services %}
{% call card(title=svc.name) %}
  <p>{{ svc.description }}</p>
  <p>
    Status: {{ badge("healthy", "#2e7d32") if svc.healthy else badge("down", "#c62828") }}
    &nbsp; Version: {{ badge("v" + svc.version, "#555") }}
  </p>
  {{ btn("View Logs", href="/logs/" + svc.name, variant="ghost") }}
  {{ btn("Restart",   href="/restart/" + svc.name, variant="danger") }}
{% endcall %}
{% endfor %}
{% endblock %}
""")

tpl("components.html", """\
{% extends "base_macros.html" %}
{% from "macros/ui.html" import alert, badge, btn, card, stat_card, data_table %}

{% block title %}Component Library{% endblock %}

{% block content %}
<h1>Component Library</h1>
<p>All UI components are defined as Jinja2 macros in <code>macros/ui.html</code>.</p>

<h2>Alerts</h2>
{{ alert("This is an info alert.",    variant="info") }}
{{ alert("Operation succeeded!",      variant="success") }}
{{ alert("Disk usage above 80%.",     variant="warning",  dismissible=true) }}
{{ alert("Authentication failed.",    variant="error") }}

<h2>Badges</h2>
{{ badge("python") }}
{{ badge("rust",    "#b7410e") }}
{{ badge("v1.1.0",  "#555") }}
{{ badge("stable",  "#2e7d32") }}
{{ badge("beta",    "#f57c00") }}

<h2>Buttons</h2>
{{ btn("Primary",   href="#", variant="primary") }}
{{ btn("Secondary", href="#", variant="secondary") }}
{{ btn("Danger",    href="#", variant="danger") }}
{{ btn("Ghost",     href="#", variant="ghost") }}

<h2>Stat Cards</h2>
<div class="grid">
  {{ stat_card("Revenue",    "$48,200", delta="12%", delta_positive=true) }}
  {{ stat_card("Churn",      "3.2",     unit="%",    delta="0.5%", delta_positive=false) }}
  {{ stat_card("NPS Score",  "72") }}
</div>

<h2>Card with Table</h2>
{% call card(title="Sample Data Table", footer="3 rows shown") %}
  {{ data_table(
    headers=["ID", "Name", "Status"],
    rows=[
      ["1", "Alice",   "Active"],
      ["2", "Bob",     "Inactive"],
      ["3", "Charlie", "Active"],
    ]
  ) }}
{% endcall %}

<h2>Empty Table</h2>
{% call card(title="No Data") %}
  {{ data_table(headers=["Col A", "Col B"], rows=[], empty_msg="Nothing to display yet.") }}
{% endcall %}
{% endblock %}
""")

# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------
app = App()
app.enable_templates(template_dir=TEMPLATE_DIR, auto_escape=True)

SAMPLE_REQUESTS = [
    ["GET",  "/api/users",   "200", "1.2 ms"],
    ["POST", "/api/orders",  "201", "3.8 ms"],
    ["GET",  "/api/stats",   "200", "0.9 ms"],
    ["GET",  "/api/users/7", "404", "0.5 ms"],
    ["GET",  "/health",      "200", "0.1 ms"],
]

SERVICES = [
    {"name": "api-gateway",  "description": "Public HTTP gateway",       "version": "2.1.0", "healthy": True},
    {"name": "auth-service", "description": "JWT auth and sessions",     "version": "1.4.2", "healthy": True},
    {"name": "worker",       "description": "Background task processor", "version": "1.1.0", "healthy": False},
]


@app.get("/")
def dashboard(request):
    html = app.render("dashboard_macros.html", {
        "requests": SAMPLE_REQUESTS,
        "services": SERVICES,
    })
    return Response.html(html)


@app.get("/components")
def components(request):
    html = app.render("components.html", {})
    return Response.html(html)


if __name__ == "__main__":
    print(f"Templates: {TEMPLATE_DIR}")
    print("Listening on http://localhost:8084")
    print("  /             — dashboard using macros")
    print("  /components   — component library showcase")
    app.run(port=8084)
```

## Running This Example

```bash
python examples/minijinja_macros.py
# Test endpoints:
curl http://127.0.0.1:8084/
curl http://127.0.0.1:8084/components
```

## Key Concepts

- **Macros**: Declaring parametrized template chunks via `{% macro name(args) %}` to avoid duplicate design structures.
- **Call Blocks**: Injecting layout scopes into widgets by calling macros using `{% call card(...) %} content {% endcall %}`, which makes the nested content block accessible as `{{ caller() }}` inside the card macro.
- **Template modularization**: Reusing UI blocks by storing them in library files (e.g. `macros/ui.html`) and importing them as needed.
