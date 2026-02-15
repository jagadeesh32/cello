---
title: Templates
description: Jinja2-compatible template rendering in Cello Framework
---

# Templates

Cello includes a built-in template engine that supports Jinja2-compatible variable substitution syntax. Templates are rendered in Rust for performance, with a Python API for convenience.

---

## Setup

### Template Directory

Create a `templates/` directory in your project root:

```
myproject/
    app.py
    templates/
        index.html
        users/
            profile.html
            list.html
```

### Creating the Template Engine

```python
from cello import App, TemplateEngine

app = App()

# Default: looks for templates in ./templates/
engine = TemplateEngine()

# Or specify a custom directory
engine = TemplateEngine("path/to/templates")
```

---

## The `TemplateEngine` Class

### Constructor

```python
TemplateEngine(template_dir="templates")
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `template_dir` | `str` | `"templates"` | Path to the directory containing template files |

### Configuration Defaults

The engine uses these defaults internally:

| Setting | Default | Description |
|---------|---------|-------------|
| `auto_reload` | `True` | Re-read templates from disk on every render (development mode) |
| `content_type` | `"text/html; charset=utf-8"` | Default response content type |
| `extension` | `".html"` | Default file extension appended when not specified |

---

## Rendering Templates

### `render(name, context)`

Load a template file and render it with the given context variables:

```python
@app.get("/")
def home(request):
    html = engine.render("index.html", {
        "title": "Welcome",
        "username": "Alice",
    })
    return Response.html(html)
```

The template file `templates/index.html`:

```html
<!DOCTYPE html>
<html>
<head>
    <title>{{ title }}</title>
</head>
<body>
    <h1>Hello, {{ username }}!</h1>
</body>
</html>
```

Output:

```html
<!DOCTYPE html>
<html>
<head>
    <title>Welcome</title>
</head>
<body>
    <h1>Hello, Alice!</h1>
</body>
</html>
```

### `render_string(template, context)`

Render a template string directly without loading from a file:

```python
@app.get("/greeting/{name}")
def greet(request):
    name = request.params["name"]
    html = engine.render_string(
        "<h1>Hello, {{ name }}!</h1><p>Your ID is {{ id }}</p>",
        {"name": name, "id": 42}
    )
    return Response.html(html)
```

This is useful for small templates, email bodies, or dynamically constructed content.

---

## Template Variables

### Supported Types

The context dictionary accepts several Python types, which are converted automatically:

| Python Type | Template Output |
|-------------|-----------------|
| `str` | Rendered as-is |
| `int` | Converted to string |
| `float` | Converted to string |
| `bool` | `"true"` or `"false"` |
| `None` | Empty string |
| `dict` / `list` | JSON representation |

### Variable Syntax

Variables use the double-brace syntax, with or without spaces:

```html
<!-- Both forms work -->
{{ variable }}
{{variable}}
```

### Example with Multiple Types

```python
html = engine.render_string("""
<div>
    <p>Name: {{ name }}</p>
    <p>Age: {{ age }}</p>
    <p>Score: {{ score }}</p>
    <p>Active: {{ active }}</p>
</div>
""", {
    "name": "Bob",
    "age": 30,
    "score": 95.5,
    "active": True,
})
```

Output:

```html
<div>
    <p>Name: Bob</p>
    <p>Age: 30</p>
    <p>Score: 95.5</p>
    <p>Active: true</p>
</div>
```

---

## Template Caching

When `auto_reload` is disabled (production mode), templates are cached in memory after the first load. Subsequent calls to `render()` with the same template name skip disk I/O entirely.

### Clearing the Cache

```python
# Clear all cached templates
engine.clear_cache()
```

!!! tip "Development vs Production"
    In development, `auto_reload=True` (the default) re-reads templates on every render so you see changes immediately. In production, disable auto-reload to cache templates and eliminate file I/O on every request.

---

## Subdirectories

Organize templates into subdirectories and reference them with relative paths:

```
templates/
    base.html
    pages/
        home.html
        about.html
    emails/
        welcome.html
```

```python
# Renders templates/pages/home.html
html = engine.render("pages/home.html", {"title": "Home"})

# Renders templates/emails/welcome.html
email_body = engine.render("emails/welcome.html", {"user": "Alice"})
```

---

## Integration with Responses

### HTML Response

```python
from cello import Response

@app.get("/dashboard")
def dashboard(request):
    html = engine.render("dashboard.html", {
        "user": "Admin",
        "stats": {"users": 150, "orders": 42},
    })
    return Response.html(html)
```

### Email Template Rendering

Combine templates with [background tasks](background-tasks.md) for sending emails:

```python
from cello import BackgroundTasks

def send_email(to: str, subject: str, body: str):
    # Your email sending logic here
    print(f"Sending to {to}: {subject}")

@app.post("/users")
def create_user(request):
    data = request.json()
    user = db.create_user(data)

    # Render email template
    email_html = engine.render("emails/welcome.html", {
        "name": data["name"],
        "login_url": "https://example.com/login",
    })

    # Send email after response
    tasks = BackgroundTasks()
    tasks.add_task(send_email, [data["email"], "Welcome!", email_html])

    return {"created": True, "id": user["id"]}
```

---

## Error Handling

Template errors are raised as Python `ValueError` exceptions:

| Error | Cause |
|-------|-------|
| Template not found | The file does not exist in the template directory |
| Read error | The file cannot be read (permissions, encoding) |

```python
try:
    html = engine.render("nonexistent.html", {})
except ValueError as e:
    print(f"Template error: {e}")
    # "Failed to load template 'nonexistent.html': No such file or directory"
```

---

## Complete Example

```python
from cello import App, Response, TemplateEngine

app = App()
engine = TemplateEngine("templates")

@app.get("/")
def index(request):
    return Response.html(engine.render("index.html", {
        "title": "My App",
        "year": 2026,
    }))

@app.get("/users/{id}")
def user_profile(request):
    user_id = request.params["id"]
    user = db.get_user(user_id)
    return Response.html(engine.render("users/profile.html", {
        "user_name": user["name"],
        "user_email": user["email"],
        "member_since": user["created_at"],
    }))

if __name__ == "__main__":
    app.run()
```

---

## Next Steps

- [Static Files](static-files.md) - Serve CSS, JavaScript, and images alongside templates
- [Background Tasks](background-tasks.md) - Render and send email templates asynchronously
- [Responses](../core/responses.md) - Learn about all response types
