"""
MiniJinja Basic Example — Cello v1.1.0

Demonstrates the simplest way to use MiniJinja templates in a Cello app.

Run:
    python examples/minijinja_basic.py
Then visit:
    http://localhost:8080/
    http://localhost:8080/hello/Alice
    http://localhost:8080/items
"""

import os
import tempfile

from cello import App, Response

# ---------------------------------------------------------------------------
# Create a temp template directory for this self-contained example.
# In a real project you'd have a "templates/" folder alongside your app.
# ---------------------------------------------------------------------------
TEMPLATE_DIR = tempfile.mkdtemp(prefix="cello_tpl_")

# Write example templates
with open(os.path.join(TEMPLATE_DIR, "index.html"), "w") as f:
    f.write("""<!DOCTYPE html>
<html>
<head><title>{{ title }}</title></head>
<body>
  <h1>{{ title }}</h1>
  <p>Welcome to <strong>{{ app_name }}</strong>!</p>
  <nav>
    <a href="/hello/World">Greeting page</a> |
    <a href="/items">Items list</a>
  </nav>
</body>
</html>
""")

with open(os.path.join(TEMPLATE_DIR, "hello.html"), "w") as f:
    f.write("""<!DOCTYPE html>
<html>
<head><title>Hello {{ name }}</title></head>
<body>
  <h1>Hello, {{ name | title }}!</h1>
  <p>You are visitor #{{ count }}.</p>
  <a href="/">← Back</a>
</body>
</html>
""")

with open(os.path.join(TEMPLATE_DIR, "items.html"), "w") as f:
    f.write("""<!DOCTYPE html>
<html>
<head><title>Items</title></head>
<body>
  <h1>Items ({{ items | length }} total)</h1>
  {% if items %}
  <ul>
    {% for item in items %}
    <li>{{ loop.index }}. {{ item.name }} — ${{ item.price }}</li>
    {% endfor %}
  </ul>
  {% else %}
  <p>No items available.</p>
  {% endif %}
  <a href="/">← Back</a>
</body>
</html>
""")

# ---------------------------------------------------------------------------
# App setup
# ---------------------------------------------------------------------------
app = App()

# Attach MiniJinja — call once, before registering routes
app.enable_templates(
    template_dir=TEMPLATE_DIR,
    auto_escape=True,                   # HTML-escape output (XSS safe)
    globals={"app_name": "Cello Demo"}, # available in every template
)

_visitor_count = 0


@app.get("/")
def home(request):
    html = app.render("index.html", {"title": "Home"})
    return Response.html(html)


@app.get("/hello/{name}")
def greet(request):
    global _visitor_count
    _visitor_count += 1
    html = app.render("hello.html", {
        "name": request.params["name"],
        "count": _visitor_count,
    })
    return Response.html(html)


@app.get("/items")
def items(request):
    data = [
        {"name": "Apple",  "price": 0.99},
        {"name": "Banana", "price": 0.49},
        {"name": "Cherry", "price": 2.99},
    ]
    html = app.render("items.html", {"items": data})
    return Response.html(html)


if __name__ == "__main__":
    print(f"Templates dir: {TEMPLATE_DIR}")
    print("Listening on http://localhost:8080")
    app.run(port=8080)
