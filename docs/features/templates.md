# Template Engine

_Available since v1.1.0_

Cello's template engine is powered by **MiniJinja** — a full Jinja2-compatible engine
written in Rust by Armin Ronacher (the original author of Jinja2). Because rendering
runs in the Rust extension, there is **zero Python overhead** on the render path.

---

## Quick start

### 1. Project layout

```
myproject/
├── app.py
└── templates/
    ├── base.html        ← shared layout
    ├── index.html       ← home page (extends base.html)
    ├── user.html        ← user detail page
    └── emails/
        └── welcome.txt  ← plain-text email (no auto-escape)
```

### 2. Attach the engine

Call `app.enable_templates()` **once** during application setup, before routes are
handled. Templates are loaded lazily from disk on first render.

```python
from cello import App, Response

app = App()

app.enable_templates(
    template_dir="templates",   # path to your templates directory
    auto_escape=True,           # XSS-safe HTML escaping (default: True)
    globals={                   # variables available in every template
        "site_name": "My App",
        "year": 2026,
    },
)
```

### 3. Render in a handler

```python
@app.get("/")
def home(request):
    html = app.render("index.html", {
        "title": "Welcome",
        "user":  {"name": "Alice", "role": "admin"},
        "items": ["Rust", "Python", "MiniJinja"],
    })
    return Response.html(html)
```

---

## `enable_templates()` reference

```python
engine = app.enable_templates(
    template_dir: str = "templates",
    auto_escape:  bool = True,
    globals:      dict | None = None,
) -> MiniJinjaEngine
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `template_dir` | `str` | `"templates"` | Directory containing template files |
| `auto_escape` | `bool` | `True` | HTML-escape `{{ }}` output in `.html`/`.htm`/`.xml` |
| `globals` | `dict` | `None` | Variables injected into every template |

Returns the `MiniJinjaEngine` instance.
Raises `RuntimeError` if called more than once on the same `App`.

---

## Rendering

### Render a file — `app.render()`

```python
html: str = app.render("page.html", {"key": "value"})
```

- `name` is relative to `template_dir` (e.g., `"index.html"` or `"emails/welcome.txt"`)
- `context` is a Python `dict`; pass `{}` or omit for no variables
- Raises `ValueError` if the template file is not found or contains a syntax error

### Render inline — `app.render_string()`

```python
text: str = app.render_string("Hello, {{ name }}!", {"name": "World"})
```

Useful for dynamic or short templates that don't need a file on disk.

---

## Template syntax

### Variables

```html
<p>{{ title }}</p>
<p>{{ user.name }}</p>        <!-- dict / object attribute -->
<p>{{ items[0] }}</p>         <!-- list index -->
<p>{{ user["email"] }}</p>    <!-- dict key -->
```

### Filters

Apply transformations with `|`:

```html
{{ name | upper }}            <!-- ALICE -->
{{ name | lower }}            <!-- alice -->
{{ name | title }}            <!-- Alice Smith -->
{{ name | trim }}             <!-- strip whitespace -->
{{ name | replace("a", "e") }}
{{ items | length }}          <!-- count -->
{{ items | first }}           <!-- first element -->
{{ items | last }}            <!-- last element -->
{{ items | join(", ") }}      <!-- join list -->
{{ score | round }}
{{ score | int }}
{{ data | tojson }}           <!-- serialize to JSON string -->
{{ html | safe }}             <!-- mark as safe — skip auto-escaping -->
{{ value | default("n/a") }}  <!-- fallback if undefined/empty -->
```

### If / elif / else

```html
{% if user.role == "admin" %}
  <span class="badge">Admin</span>
{% elif user.role == "editor" %}
  <span class="badge">Editor</span>
{% else %}
  <span class="badge">Viewer</span>
{% endif %}
```

### For loops

```html
<ul>
  {% for item in items %}
  <li>{{ item }}</li>
  {% else %}
  <li>Nothing here.</li>
  {% endfor %}
</ul>
```

**Loop variables** available inside `{% for %}`:

| Variable | Description |
|----------|-------------|
| `loop.index` | 1-based iteration counter |
| `loop.index0` | 0-based iteration counter |
| `loop.first` | `true` on first iteration |
| `loop.last` | `true` on last iteration |
| `loop.length` | total number of items |

```html
{% for post in posts %}
<article class="{{ 'highlight' if loop.first }}">
  <h2>{{ loop.index }}. {{ post.title }}</h2>
</article>
{% endfor %}
```

### Set variable

```html
{% set greeting = "Hello, " ~ user.name ~ "!" %}
<p>{{ greeting }}</p>
```

### Comments

```html
{# This comment is stripped from the output #}
```

### Raw block

Escape Jinja2 syntax so it is output literally:

```html
{% raw %}
  Handlebars: {{ this.is.not.jinja }}
{% endraw %}
```

---

## Template inheritance

Template inheritance is the most powerful reuse mechanism. A **base template** defines
the page skeleton with named `{% block %}` regions. **Child templates** extend the base
and fill in those regions.

### Base template — `templates/base.html`

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{% block title %}{{ site_name }}{% endblock %}</title>
  {% block head %}{% endblock %}
</head>
<body>

  <header>
    <a href="/">{{ site_name }}</a>
    <nav>{% block nav %}
      <a href="/">Home</a>
      <a href="/about">About</a>
    {% endblock %}</nav>
  </header>

  <main>
    {% block content %}{% endblock %}
  </main>

  <footer>
    {% block footer %}
    <p>© {{ year }} {{ site_name }}</p>
    {% endblock %}
  </footer>

</body>
</html>
```

### Child template — `templates/index.html`

```html
{% extends "base.html" %}

{# Override the <title> #}
{% block title %}Home — {{ site_name }}{% endblock %}

{# Add a CSS link just for this page #}
{% block head %}
<link rel="stylesheet" href="/static/home.css">
{% endblock %}

{# Main content — this is the primary block to override #}
{% block content %}
<h1>Welcome, {{ user.name }}!</h1>

<ul>
  {% for item in items %}
  <li>{{ item }}</li>
  {% endfor %}
</ul>
{% endblock %}

{# Footer override — call super() to keep the parent content #}
{% block footer %}
{{ super() }}
<p><small>Version {{ cello_version }}</small></p>
{% endblock %}
```

### Render the child

```python
html = app.render("index.html", {
    "user":  {"name": "Alice"},
    "items": ["Apples", "Bananas"],
})
```

### Block rules

- A child can override **any** block defined in the parent (or any ancestor in the chain)
- Call `{{ super() }}` inside a block to include the parent's content plus your additions
- Blocks not overridden in the child keep their parent default
- Nesting depth is unlimited — a child can itself be a base for another template

### Multi-level inheritance

```
base.html          ← outermost skeleton (HTML, head, header, footer)
  └── layout.html  ← adds sidebar, breadcrumbs, flash messages
        └── user_profile.html  ← fills in the actual page content
```

`layout.html`:
```html
{% extends "base.html" %}

{% block content %}
<div class="layout">
  <aside>{% block sidebar %}{% endblock %}</aside>
  <section>{% block page_content %}{% endblock %}</section>
</div>
{% endblock %}
```

`user_profile.html`:
```html
{% extends "layout.html" %}

{% block title %}{{ user.name }} — Profile{% endblock %}

{% block sidebar %}
<img src="{{ user.avatar_url }}" alt="{{ user.name }}">
<p>Member since {{ user.joined }}</p>
{% endblock %}

{% block page_content %}
<h1>{{ user.name }}</h1>
<p>{{ user.bio }}</p>
{% endblock %}
```

---

## Includes

Use `{% include %}` to embed a partial template:

```html
{# Include a shared navigation snippet #}
{% include "partials/nav.html" %}

{# Include with a fallback if the file doesn't exist #}
{% include "partials/banner.html" ignore missing %}
```

`templates/partials/nav.html`:
```html
<nav>
  <a href="/">Home</a>
  <a href="/posts">Blog</a>
  <a href="/contact">Contact</a>
</nav>
```

Includes share the current template's context — all variables in scope are available
inside the included file.

---

## Macros

Macros are reusable template snippets with parameters, similar to functions.

### Define and use in the same file

```html
{% macro alert(message, variant="info") %}
<div class="alert alert-{{ variant }}">{{ message }}</div>
{% endmacro %}

{{ alert("File saved!", variant="success") }}
{{ alert("Low disk space.", variant="warning") }}
```

### Define in a shared library and import

`templates/macros/ui.html`:
```html
{% macro badge(text, color="blue") %}
<span class="badge" style="background:{{ color }}">{{ text }}</span>
{% endmacro %}

{% macro card(title) %}
<div class="card">
  <div class="card-header">{{ title }}</div>
  <div class="card-body">{{ caller() }}</div>
</div>
{% endmacro %}
```

Import and use in another template:
```html
{% from "macros/ui.html" import badge, card %}

{{ badge("Admin", color="red") }}
{{ badge("v1.1.0") }}

{# card uses {% call %} to pass a body block #}
{% call card(title="User Details") %}
  <p>Name: Alice</p>
  <p>Role: Admin</p>
{% endcall %}
```

---

## Global variables

Global variables are injected into **every** template rendered by the engine.
Per-render context takes precedence over globals on name collision.

### Set at startup via `enable_templates()`

```python
app.enable_templates(
    template_dir="templates",
    globals={
        "site_name":     "My App",
        "year":          2026,
        "support_email": "help@example.com",
    },
)
```

### Add later via the engine instance

```python
engine = app.enable_templates(template_dir="templates")
engine.add_global("debug_mode", False)
engine.add_globals({"cdn_url": "https://cdn.example.com", "version": "1.1.0"})
```

### Access in templates

```html
<footer>© {{ year }} {{ site_name }}</footer>
<a href="mailto:{{ support_email }}">Contact support</a>
```

---

## Auto-escaping and XSS prevention

When `auto_escape=True` (the default), any `{{ }}` output in `.html`, `.htm`, and
`.xml` templates is HTML-escaped automatically:

| User input | Rendered output |
|-----------|-----------------|
| `<script>alert(1)</script>` | `&lt;script&gt;alert(1)&lt;/script&gt;` |
| `" onclick="bad()` | `&quot; onclick=&quot;bad()` |
| `&amp;` | `&amp;amp;` |

To output **trusted** HTML, use the `safe` filter:

```html
{# Only do this with content you control — never with user input #}
{{ article.body_html | safe }}
```

Plain-text templates (`.txt`, `.csv`, `.md`, etc.) are **never** auto-escaped,
even when `auto_escape=True`.

---

## Standalone engine

Use `MiniJinjaEngine` directly — without `App` — for background tasks, CLI scripts,
or rendering emails:

```python
from cello import MiniJinjaEngine

# HTML emails (auto-escape on)
html_engine = MiniJinjaEngine(template_dir="templates/emails/html", auto_escape=True)
html_engine.add_globals({"company": "Cello Corp", "year": 2026})

html = html_engine.render("welcome.html", {"name": "Alice", "confirm_url": "..."})

# Plain-text emails (auto-escape off)
text_engine = MiniJinjaEngine(template_dir="templates/emails/text", auto_escape=False)
text = text_engine.render("welcome.txt", {"name": "Alice", "confirm_url": "..."})
```

---

## Python type conversion

Python values are converted automatically via `serde_json` as an intermediary:

| Python type | Template access |
|-------------|-----------------|
| `str` | `{{ name }}` |
| `int`, `float` | `{{ count }}`, `{{ price }}` |
| `bool` | `{% if active %}` |
| `None` | treated as undefined/null |
| `list` | `{% for x in items %}` |
| `tuple` | `{% for x in items %}` (treated as list) |
| `dict` | `{{ user.name }}`, `{{ user["email"] }}` |
| object with `__dict__` | `{{ obj.attr }}` (private `_` attrs excluded) |
| anything else | converted to string via `str()` |

---

## Directory structure best practices

```
templates/
├── base.html              ← main HTML skeleton
├── layouts/
│   ├── app.html           ← authenticated layout (sidebar, user menu)
│   └── marketing.html     ← public pages layout
├── pages/
│   ├── index.html         ← home
│   ├── about.html
│   └── contact.html
├── components/            ← includes and macro libraries
│   ├── nav.html
│   ├── footer.html
│   └── macros/
│       ├── forms.html
│       └── ui.html
└── emails/
    ├── html/
    │   ├── base_email.html
    │   ├── welcome.html
    │   └── invoice.html
    └── text/
        ├── welcome.txt
        └── invoice.txt
```

---

## Working with Blueprints

The `App` instance (and its `render()` method) is available anywhere you have a
reference to it. Pass it to Blueprint handlers via closure:

```python
from cello import App, Blueprint, Response

app = App()
app.enable_templates(template_dir="templates")

admin = Blueprint("/admin")

@admin.get("/dashboard")
def dashboard(request):
    html = app.render("admin/dashboard.html", {"stats": get_stats()})
    return Response.html(html)

app.register_blueprint(admin)
```

---

## Error handling with templates

Render custom error pages from exception handlers:

```python
@app.get("/post/{id}")
def post_detail(request):
    post = db.get(int(request.params["id"]))
    if not post:
        html = app.render("errors/404.html", {"path": request.path})
        return Response.html(html, status=404)
    return Response.html(app.render("post.html", {"post": post}))
```

`templates/errors/404.html`:
```html
{% extends "base.html" %}
{% block title %}404 Not Found{% endblock %}
{% block content %}
<h1>Page Not Found</h1>
<p>No page at <code>{{ path }}</code>.</p>
<a href="/">← Go home</a>
{% endblock %}
```

---

## Examples

Six runnable examples are included:

| Example | Description |
|---------|-------------|
| [`examples/minijinja_basic.py`](https://github.com/jagadeesh32/cello/blob/main/examples/minijinja_basic.py) | Getting started: variables, filters, loops |
| [`examples/minijinja_advanced.py`](https://github.com/jagadeesh32/cello/blob/main/examples/minijinja_advanced.py) | Inheritance, globals, standalone engine |
| [`examples/minijinja_blog.py`](https://github.com/jagadeesh32/cello/blob/main/examples/minijinja_blog.py) | Multi-page blog with pagination and 404 |
| [`examples/minijinja_forms.py`](https://github.com/jagadeesh32/cello/blob/main/examples/minijinja_forms.py) | Form validation with sticky values |
| [`examples/minijinja_macros.py`](https://github.com/jagadeesh32/cello/blob/main/examples/minijinja_macros.py) | Reusable UI component library |
| [`examples/minijinja_emails.py`](https://github.com/jagadeesh32/cello/blob/main/examples/minijinja_emails.py) | HTML + plain-text email templates |

---

## Version history

| Version | Change |
|---------|--------|
| **v1.1.0** | MiniJinja integration: `MiniJinjaEngine`, `App.enable_templates()`, `App.render()`, `App.render_string()`, globals, auto-escaping |
