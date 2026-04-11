# MiniJinja Template Engine

_Available since v1.1.0_

Cello ships with built-in support for **MiniJinja**, a Jinja2-compatible template engine
written in Rust by the original author of Jinja2 (Armin Ronacher).
Because the engine runs entirely in Rust it adds zero Python overhead on the render path.

---

## Installation

No extra packages required — MiniJinja is compiled into the `cello` Rust extension.

```
pip install cello-framework       # already includes MiniJinja
```

---

## Quick start

### 1. Create your templates directory

```
myproject/
├── app.py
└── templates/
    ├── base.html
    └── index.html
```

**`templates/base.html`**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{% block title %}{{ app_name }}{% endblock %}</title>
</head>
<body>
  {% block content %}{% endblock %}
</body>
</html>
```

**`templates/index.html`**

```html
{% extends "base.html" %}

{% block title %}{{ title }} – {{ app_name }}{% endblock %}

{% block content %}
<h1>{{ title }}</h1>

{% if items %}
<ul>
  {% for item in items %}
  <li>{{ item }}</li>
  {% endfor %}
</ul>
{% else %}
<p>No items yet.</p>
{% endif %}
{% endblock %}
```

### 2. Attach the engine to your app

```python
from cello import App, Response

app = App()

# Attach once during startup — optionally pass global variables
app.enable_templates(
    template_dir="templates",   # default
    auto_escape=True,           # default — escapes HTML in .html/.htm/.xml
    globals={
        "app_name": "My Site",
        "year": 2026,
    },
)

@app.get("/")
def home(request):
    html = app.render("index.html", {
        "title": "Welcome",
        "items": ["Cello", "MiniJinja", "Rust"],
    })
    return Response.html(html)

app.run(port=8000)
```

---

## API reference

### `App.enable_templates()`

```python
app.enable_templates(
    template_dir: str = "templates",
    auto_escape: bool = True,
    globals: dict | None = None,
) -> MiniJinjaEngine
```

Attach a MiniJinja engine to the application.
Must be called **once** before any `app.render()` call.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `template_dir` | `str` | `"templates"` | Directory containing template files |
| `auto_escape` | `bool` | `True` | HTML-escape `{{ }}` output in `.html`/`.htm`/`.xml` files |
| `globals` | `dict` | `None` | Variables available in every template |

Returns the configured `MiniJinjaEngine` instance.

---

### `App.render()`

```python
html: str = app.render(template_name: str, context: dict = {})
```

Render a template file and return the result as a string.

```python
@app.get("/users/{id}")
def user_page(request):
    html = app.render("user.html", {
        "user": {"id": request.params["id"], "name": "Alice"},
    })
    return Response.html(html)
```

---

### `App.render_string()`

```python
text: str = app.render_string(source: str, context: dict = {})
```

Render an inline template string — no file required.

```python
@app.get("/greet/{name}")
def greet(request):
    msg = app.render_string(
        "Hello, {{ name | title }}!",
        {"name": request.params["name"]},
    )
    return Response.text(msg)
```

---

### `MiniJinjaEngine` — standalone

For use outside of the `App` class (e.g. background tasks, CLI scripts):

```python
from cello import MiniJinjaEngine

engine = MiniJinjaEngine(template_dir="templates", auto_escape=True)

# Render a file
html = engine.render("email.html", {"user": "Bob", "code": "XYZ"})

# Render inline
text = engine.render_string("Hi {{ name }}!", {"name": "Carol"})

# Add globals
engine.add_global("site", "My App")
engine.add_globals({"year": 2026, "debug": False})
```

---

## Supported Jinja2 syntax

| Feature | Example |
|---------|---------|
| Variable | `{{ name }}` |
| Attribute access | `{{ user.email }}` |
| Filter | `{{ name \| upper }}` |
| If / elif / else | `{% if admin %}…{% endif %}` |
| For loop | `{% for item in items %}…{% endfor %}` |
| Loop variable | `{{ loop.index }}`, `{{ loop.first }}` |
| Template inheritance | `{% extends "base.html" %}` |
| Block | `{% block content %}…{% endblock %}` |
| Include | `{% include "nav.html" %}` |
| Macro | `{% macro btn(text) %}<button>{{ text }}</button>{% endmacro %}` |
| Set variable | `{% set x = 42 %}` |
| Comments | `{# this is a comment #}` |
| Raw block | `{% raw %}{{ not rendered }}{% endraw %}` |

---

## Built-in filters

A subset of the Jinja2 standard filter library is available:

| Filter | Description |
|--------|-------------|
| `upper` / `lower` | Case conversion |
| `title` | Title-case |
| `trim` | Strip whitespace |
| `replace(old, new)` | String replace |
| `length` | Count items |
| `first` / `last` | First/last element |
| `join(d)` | Join list with delimiter |
| `default(val)` | Fallback if undefined |
| `int` / `float` | Type cast |
| `abs` | Absolute value |
| `round` | Round number |
| `sort` | Sort list |
| `reverse` | Reverse list |
| `unique` | Deduplicate list |
| `items` | Dict key-value pairs |
| `tojson` | Serialize to JSON string |
| `urlencode` | URL-encode a string |

---

## Auto-escaping and XSS prevention

When `auto_escape=True` (the default), any value rendered with `{{ }}` in an
`.html`, `.htm`, or `.xml` template is HTML-escaped automatically:

```python
# User supplies malicious input
app.render_string("{{ content }}", {"content": "<script>alert(1)</script>"})
# → "&lt;script&gt;alert(1)&lt;/script&gt;"
```

To opt out for a specific value use the `safe` filter:

```jinja2
{{ trusted_html | safe }}
```

Plain text templates (`.txt`, `.csv`, etc.) are **never** auto-escaped.

---

## Context type mapping

Python values are converted to their Jinja2 equivalents transparently:

| Python type | Jinja2 type |
|-------------|-------------|
| `str` | string |
| `int` / `float` | number |
| `bool` | boolean |
| `None` | `null` |
| `list` / `tuple` | sequence |
| `dict` | object |
| object with `__dict__` | object (public attrs only) |

---

## Template inheritance example

**`templates/base.html`**
```html
<!DOCTYPE html>
<html>
<head><title>{% block title %}App{% endblock %}</title></head>
<body>
  <nav>{% block nav %}{% endblock %}</nav>
  <main>{% block content %}{% endblock %}</main>
  <footer>© {{ year }}</footer>
</body>
</html>
```

**`templates/dashboard.html`**
```html
{% extends "base.html" %}

{% block title %}Dashboard{% endblock %}

{% block nav %}<a href="/">Home</a>{% endblock %}

{% block content %}
<h1>Welcome, {{ user.name }}</h1>
<p>You have {{ notifications | length }} notifications.</p>
{% endblock %}
```

```python
@app.get("/dashboard")
def dashboard(request):
    html = app.render("dashboard.html", {
        "user": {"name": "Alice"},
        "notifications": ["msg1", "msg2"],
    })
    return Response.html(html)
```

---

## Using with Blueprints

Pass the `app` instance (or the engine directly) into your Blueprint handlers
via closure or dependency injection:

```python
from cello import App, Blueprint, Response

app = App()
app.enable_templates(template_dir="templates")

admin = Blueprint("/admin")

@admin.get("/users")
def admin_users(request):
    html = app.render("admin/users.html", {"users": []})
    return Response.html(html)

app.register_blueprint(admin)
```

---

## Version history

| Version | Change |
|---------|--------|
| **1.1.0** | MiniJinja integration added (`MiniJinjaEngine`, `App.enable_templates`, `App.render`, `App.render_string`) |
