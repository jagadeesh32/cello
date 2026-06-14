---
title: MiniJinja Form Handling & Validation
description: HTML form handling, sticky input values, validation errors, and success redirects using MiniJinja in Cello
---

# :material-form-select: MiniJinja Form Handling & Validation

This example demonstrates how to render HTML forms, process form submissions (`application/x-www-form-urlencoded`), validate form inputs, return custom validation error states, and implement sticky forms (preserving valid inputs on failure) using Cello and MiniJinja.

## Features Demonstrated

- **Sticky Form Values**: Automatically preserving user input (excluding passwords) when rendering validation error messages.
- **Dynamic Field Styling**: Conditional CSS class assignment (e.g. `class="{{ 'error' if errors.username }}"`) for visual error indicators.
- **Form Parsing**: Reading URL-encoded request body binaries into a Python dictionary.
- **Custom Client Warnings**: Setting HTTP status `422 Unprocessable Entity` when form validation fails.

## Complete Source Code

```python
"""
MiniJinja Forms Example — Cello v1.1.0

Shows how to:
  - Render an HTML form from a template
  - Re-render the same form with inline validation errors
  - Preserve user-entered values on error ("sticky forms")
  - Show a success page on valid submission
  - Use the {{ field_class(name) }} macro pattern for DRY error styling

Run:
    python examples/minijinja_forms.py
Then visit:
    http://localhost:8083/register    ← GET renders the blank form
    POST /register                    ← validates and redirects or re-renders
    http://localhost:8083/contact     ← a simpler single-field contact form
"""

import os
import re
import tempfile

from cello import App, Response

TEMPLATE_DIR = tempfile.mkdtemp(prefix="cello_forms_")


def tpl(name, content):
    path = os.path.join(TEMPLATE_DIR, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content)


# ---------------------------------------------------------------------------
# Templates
# ---------------------------------------------------------------------------

tpl("base.html", """\
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{% block title %}Forms Demo{% endblock %}</title>
  <style>
    body  { font-family: Arial, sans-serif; max-width: 540px;
            margin: 3rem auto; padding: 0 1rem; color: #222; }
    h1    { margin-bottom: 1.5rem; }
    label { display: block; margin-top: 1rem; font-weight: bold; }
    input, textarea, select {
      width: 100%; padding: 0.45rem 0.6rem; margin-top: 0.3rem;
      border: 1px solid #ccc; border-radius: 4px; font-size: 1rem;
    }
    input.error, textarea.error { border-color: #d32f2f; background: #fff8f8; }
    .err  { color: #d32f2f; font-size: 0.85rem; margin-top: 0.25rem; }
    .hint { color: #888; font-size: 0.82rem; margin-top: 0.2rem; }
    button { margin-top: 1.5rem; padding: 0.55rem 1.5rem; background: #1a73e8;
             color: #fff; border: none; border-radius: 4px; cursor: pointer;
             font-size: 1rem; }
    button:hover { background: #1558b0; }
    .success { background: #e8f5e9; border: 1px solid #66bb6a; padding: 1rem;
               border-radius: 4px; }
    nav a { margin-right: 1rem; color: #1a73e8; }
  </style>
</head>
<body>
  <nav><a href="/register">Register</a><a href="/contact">Contact</a></nav>
  {% block content %}{% endblock %}
</body>
</html>
""")

# Registration form — demonstrates multi-field validation + sticky values
tpl("register.html", """\
{% extends "base.html" %}
{% block title %}Register{% endblock %}

{% block content %}
<h1>Create Account</h1>

{# Show a global error banner if any errors exist #}
{% if errors %}
<p class="err" style="font-size:1rem;">
  Please fix the {{ errors | length }} error(s) below.
</p>
{% endif %}

<form method="post" action="/register" novalidate>

  <label for="username">Username</label>
  <input id="username" name="username" type="text"
         value="{{ values.username }}"
         class="{{ 'error' if errors.username }}">
  {% if errors.username %}<p class="err">{{ errors.username }}</p>{% endif %}
  <p class="hint">3–20 characters, letters and numbers only.</p>

  <label for="email">Email</label>
  <input id="email" name="email" type="email"
         value="{{ values.email }}"
         class="{{ 'error' if errors.email }}">
  {% if errors.email %}<p class="err">{{ errors.email }}</p>{% endif %}

  <label for="password">Password</label>
  <input id="password" name="password" type="password"
         class="{{ 'error' if errors.password }}">
  {% if errors.password %}<p class="err">{{ errors.password }}</p>{% endif %}
  <p class="hint">At least 8 characters.</p>

  <label for="role">Role</label>
  <select id="role" name="role">
    {% for option in ["viewer", "editor", "admin"] %}
    <option value="{{ option }}"
      {% if values.role == option %}selected{% endif %}>
      {{ option | title }}
    </option>
    {% endfor %}
  </select>

  <button type="submit">Register</button>
</form>
{% endblock %}
""")

# Success page
tpl("register_success.html", """\
{% extends "base.html" %}
{% block title %}Welcome, {{ username }}!{% endblock %}

{% block content %}
<h1>Registration Complete</h1>
<div class="success">
  <p>Welcome, <strong>{{ username }}</strong>!</p>
  <p>Account created with role: <strong>{{ role | title }}</strong>.</p>
  <p>A confirmation email will be sent to <strong>{{ email }}</strong>.</p>
</div>
<p style="margin-top:1rem;"><a href="/register">← Register another</a></p>
{% endblock %}
""")

# Simple contact form
tpl("contact.html", """\
{% extends "base.html" %}
{% block title %}Contact Us{% endblock %}

{% block content %}
<h1>Contact Us</h1>

{% if sent %}
<div class="success">
  <strong>Message sent!</strong>
  <p>Thanks, {{ values.name }}. We'll get back to you at {{ values.email }}.</p>
</div>
{% else %}

{% if errors %}
<p class="err" style="font-size:1rem;">Please fix the errors below.</p>
{% endif %}

<form method="post" action="/contact" novalidate>
  <label for="name">Your Name</label>
  <input id="name" name="name" type="text" value="{{ values.name }}"
         class="{{ 'error' if errors.name }}">
  {% if errors.name %}<p class="err">{{ errors.name }}</p>{% endif %}

  <label for="email">Your Email</label>
  <input id="email" name="email" type="email" value="{{ values.email }}"
         class="{{ 'error' if errors.email }}">
  {% if errors.email %}<p class="err">{{ errors.email }}</p>{% endif %}

  <label for="message">Message</label>
  <textarea id="message" name="message" rows="5"
            class="{{ 'error' if errors.message }}">{{ values.message }}</textarea>
  {% if errors.message %}<p class="err">{{ errors.message }}</p>{% endif %}

  <button type="submit">Send Message</button>
</form>
{% endif %}
{% endblock %}
""")

# ---------------------------------------------------------------------------
# Validation helpers
# ---------------------------------------------------------------------------


def _parse_form(body: bytes) -> dict:
    """Parse application/x-www-form-urlencoded body into a dict."""
    from urllib.parse import parse_qs
    parsed = parse_qs(body.decode("utf-8", errors="replace"), keep_blank_values=True)
    return {k: v[0] for k, v in parsed.items()}


def _validate_register(data: dict) -> dict:
    errors = {}
    username = data.get("username", "").strip()
    email    = data.get("email", "").strip()
    password = data.get("password", "")
    if not username:
        errors["username"] = "Username is required."
    elif not re.fullmatch(r"[A-Za-z0-9]{3,20}", username):
        errors["username"] = "3–20 characters, letters and numbers only."
    if not email:
        errors["email"] = "Email is required."
    elif not re.fullmatch(r"[^@]+@[^@]+\.[^@]+", email):
        errors["email"] = "Enter a valid email address."
    if not password:
        errors["password"] = "Password is required."
    elif len(password) < 8:
        errors["password"] = "Password must be at least 8 characters."
    return errors


def _validate_contact(data: dict) -> dict:
    errors = {}
    if not data.get("name", "").strip():
        errors["name"] = "Name is required."
    email = data.get("email", "").strip()
    if not email:
        errors["email"] = "Email is required."
    elif not re.fullmatch(r"[^@]+@[^@]+\.[^@]+", email):
        errors["email"] = "Enter a valid email address."
    if len(data.get("message", "").strip()) < 10:
        errors["message"] = "Message must be at least 10 characters."
    return errors


# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------
app = App()
app.enable_templates(template_dir=TEMPLATE_DIR, auto_escape=True)


@app.get("/register")
def register_form(request):
    html = app.render("register.html", {"values": {}, "errors": {}})
    return Response.html(html)


@app.post("/register")
def register_submit(request):
    data   = _parse_form(request.body())
    errors = _validate_register(data)
    if errors:
        # Re-render with errors and sticky values (password intentionally cleared)
        html = app.render("register.html", {
            "values": {k: v for k, v in data.items() if k != "password"},
            "errors": errors,
        })
        return Response.html(html, status=422)
    # Valid — show success
    html = app.render("register_success.html", {
        "username": data["username"],
        "email":    data["email"],
        "role":     data.get("role", "viewer"),
    })
    return Response.html(html)


@app.get("/contact")
def contact_form(request):
    html = app.render("contact.html", {"values": {}, "errors": {}, "sent": False})
    return Response.html(html)


@app.post("/contact")
def contact_submit(request):
    data   = _parse_form(request.body())
    errors = _validate_contact(data)
    if errors:
        html = app.render("contact.html", {
            "values": data,
            "errors": errors,
            "sent":   False,
        })
        return Response.html(html, status=422)
    # Pretend we sent the email
    html = app.render("contact.html", {
        "values": data,
        "errors": {},
        "sent":   True,
    })
    return Response.html(html)


if __name__ == "__main__":
    print(f"Templates: {TEMPLATE_DIR}")
    print("Listening on http://localhost:8083")
    print("  GET  /register  — blank form")
    print("  POST /register  — submit")
    print("  GET  /contact   — contact form")
    app.run(port=8083)
```

## Running This Example

```bash
python examples/minijinja_forms.py
# Test registration form submit (with invalid password under 8 chars, returns 422):
curl -X POST -d "username=testuser&email=test@example.com&password=abc" http://127.0.0.1:8083/register
# Test registration success:
curl -X POST -d "username=testuser&email=test@example.com&password=abc12345" http://127.0.0.1:8083/register
```

## Key Concepts

- **Sticky Forms**: Retaining user-typed input on validation failures by passing `values` back into the render context, while security targets (like `password`) are intentionally omitted.
- **HTTP status 422**: Using `Response.html(html, status=422)` to inform the HTTP client that validation has failed while still rendering the form interface.
- **Input Sanitization**: Processing the raw binary `request.body()` via standard library query parsers to construct clean Python dictionaries.
