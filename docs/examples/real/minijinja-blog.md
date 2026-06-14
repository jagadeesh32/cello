---
title: MiniJinja Multi-page Blog
description: A complete multi-page blog with pagination, tag filtering, sidebar widgets, and custom 404 handler page in Cello
---

# :material-post-outline: MiniJinja Multi-page Blog

This example showcases a realistic, full-featured multi-page blog application using Cello and MiniJinja. It implements template layouts, pagination, filtering by tags, custom global template contexts (e.g. for sidebar widgets), and template-driven 404 page error handling.

## Features Demonstrated

- **Layout Structure**: Hierarchical templates with `base.html` providing layout frameworks.
- **Pagination Logic**: Pagination helpers using query parameters to slice in-memory mock datasets.
- **Sidebar Integration**: Computing dynamic values once (e.g. tag count and recent posts) and adding them to the global templating context.
- **Error Page Templates**: Rendering user-friendly error views using custom functions and setting a `404` status code.

## Complete Source Code

```python
"""
MiniJinja Blog Example — Cello v1.1.0

A realistic multi-page blog with:
  - Template inheritance  (base → layout → page)
  - Listing page with pagination
  - Post detail page
  - 404 error page via error handler
  - Sidebar with recent posts (from a shared global helper)

Run:
    python examples/minijinja_blog.py
Then visit:
    http://localhost:8082/
    http://localhost:8082/posts
    http://localhost:8082/posts/1
    http://localhost:8082/posts/99      ← triggers 404 template
    http://localhost:8082/tag/python
"""

import os
import tempfile
from datetime import date

from cello import App, Response

# ---------------------------------------------------------------------------
# In-memory "database"
# ---------------------------------------------------------------------------
POSTS = [
    {
        "id": 1,
        "slug": "hello-cello",
        "title": "Hello, Cello!",
        "summary": "Introducing the ultra-fast Rust-powered Python web framework.",
        "body": "Cello is built on a Rust core with PyO3 bindings...",
        "author": "Jagadeesh",
        "date": "2026-01-10",
        "tags": ["cello", "rust", "python"],
        "reading_time": 3,
    },
    {
        "id": 2,
        "slug": "minijinja-templates",
        "title": "Jinja2 Templates in Rust with MiniJinja",
        "summary": "Full Jinja2 syntax at Rust speed — now built into Cello v1.1.0.",
        "body": "MiniJinja is written by Armin Ronacher, the author of Jinja2...",
        "author": "Jagadeesh",
        "date": "2026-03-15",
        "tags": ["cello", "templates", "minijinja"],
        "reading_time": 5,
    },
    {
        "id": 3,
        "slug": "async-handlers",
        "title": "Writing Async Handlers",
        "summary": "How to use async/await in Cello route handlers for I/O-bound work.",
        "body": "Cello supports both sync and async handlers transparently...",
        "author": "Jagadeesh",
        "date": "2026-04-01",
        "tags": ["cello", "python", "async"],
        "reading_time": 4,
    },
    {
        "id": 4,
        "slug": "rbac-guards",
        "title": "Role-Based Access Control with Guards",
        "summary": "Protect routes with composable RBAC guards in just one line.",
        "body": "Cello's guard system lets you attach role and permission checks...",
        "author": "Jagadeesh",
        "date": "2026-04-08",
        "tags": ["cello", "security", "rbac"],
        "reading_time": 6,
    },
]

# ---------------------------------------------------------------------------
# Templates
# ---------------------------------------------------------------------------
TEMPLATE_DIR = tempfile.mkdtemp(prefix="cello_blog_")


def tpl(name, content):
    path = os.path.join(TEMPLATE_DIR, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(content)


# base.html — outermost HTML skeleton
tpl("base.html", """\
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{% block title %}{{ site_name }}{% endblock %}</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    body  { font-family: Georgia, serif; margin: 0; color: #222; }
    a     { color: #1a73e8; text-decoration: none; }
    a:hover { text-decoration: underline; }
    .wrap { max-width: 900px; margin: 0 auto; padding: 0 1.5rem; }
    header { background: #1a1a2e; color: #fff; padding: 1rem 0; }
    header a { color: #a8d8ea; }
    header h1 { margin: 0; font-size: 1.6rem; }
    header p  { margin: 0; font-size: 0.85rem; opacity: 0.75; }
    .layout   { display: grid; grid-template-columns: 1fr 280px; gap: 2rem;
                padding: 2rem 0; }
    main      { min-width: 0; }
    aside     { border-left: 1px solid #eee; padding-left: 1.5rem; }
    footer    { border-top: 1px solid #eee; text-align: center;
                padding: 1.5rem 0; font-size: 0.8rem; color: #888; }
    .tag { display: inline-block; background: #e8f0fe; color: #1a73e8;
           border-radius: 3px; padding: 1px 7px; font-size: 0.78rem;
           margin: 2px; font-family: monospace; }
  </style>
</head>
<body>
  <header>
    <div class="wrap">
      <h1><a href="/">{{ site_name }}</a></h1>
      <p>{{ tagline }}</p>
    </div>
  </header>

  <div class="wrap layout">
    <main>{% block content %}{% endblock %}</main>
    <aside>{% block sidebar %}
      <h3>Recent Posts</h3>
      <ul>
        {% for p in recent_posts %}
        <li><a href="/posts/{{ p.id }}">{{ p.title }}</a></li>
        {% endfor %}
      </ul>
      <h3>Tags</h3>
      {% for tag in all_tags %}
      <a class="tag" href="/tag/{{ tag }}">#{{ tag }}</a>
      {% endfor %}
    {% endblock %}</aside>
  </div>

  <footer>
    <div class="wrap">© {{ year }} {{ site_name }} · Built with Cello {{ cello_version }}</div>
  </footer>
</body>
</html>
""")

# index.html — home / post listing
tpl("index.html", """\
{% extends "base.html" %}
{% block title %}{{ site_name }} — Blog{% endblock %}

{% block content %}
<h2>Latest Posts</h2>

{% for post in posts %}
<article style="margin-bottom:2rem; padding-bottom:1.5rem; border-bottom:1px solid #eee;">
  <h3 style="margin-bottom:0.25rem;">
    <a href="/posts/{{ post.id }}">{{ post.title }}</a>
  </h3>
  <small style="color:#888;">
    By {{ post.author }} · {{ post.date }} · {{ post.reading_time }} min read
  </small>
  <p>{{ post.summary }}</p>
  {% for tag in post.tags %}
  <a class="tag" href="/tag/{{ tag }}">#{{ tag }}</a>
  {% endfor %}
</article>
{% else %}
<p>No posts yet.</p>
{% endfor %}

{# Pagination #}
{% if total_pages > 1 %}
<nav style="margin-top:1rem;">
  {% if page > 1 %}
  <a href="/posts?page={{ page - 1 }}">&laquo; Prev</a> &nbsp;
  {% endif %}
  Page {{ page }} of {{ total_pages }}
  {% if page < total_pages %}
  &nbsp; <a href="/posts?page={{ page + 1 }}">Next &raquo;</a>
  {% endif %}
</nav>
{% endif %}
{% endblock %}
""")

# post.html — single post detail
tpl("post.html", """\
{% extends "base.html" %}
{% block title %}{{ post.title }} — {{ site_name }}{% endblock %}

{% block content %}
<article>
  <h2>{{ post.title }}</h2>
  <p style="color:#888; font-size:0.9rem;">
    By {{ post.author }} · {{ post.date }} · {{ post.reading_time }} min read
  </p>
  {% for tag in post.tags %}
  <a class="tag" href="/tag/{{ tag }}">#{{ tag }}</a>
  {% endfor %}
  <hr>
  <p style="line-height:1.8;">{{ post.body }}</p>
</article>

<p style="margin-top:2rem;"><a href="/posts">&larr; All Posts</a></p>
{% endblock %}
""")

# tag.html — posts filtered by tag
tpl("tag.html", """\
{% extends "base.html" %}
{% block title %}#{{ tag }} — {{ site_name }}{% endblock %}

{% block content %}
<h2>Posts tagged <span class="tag">#{{ tag }}</span></h2>

{% if posts %}
{% for post in posts %}
<article style="margin-bottom:1.5rem;">
  <h3><a href="/posts/{{ post.id }}">{{ post.title }}</a></h3>
  <small style="color:#888;">{{ post.date }}</small>
  <p>{{ post.summary }}</p>
</article>
{% endfor %}
{% else %}
<p>No posts with this tag.</p>
{% endif %}

<p><a href="/posts">&larr; All Posts</a></p>
{% endblock %}
""")

# 404.html
tpl("404.html", """\
{% extends "base.html" %}
{% block title %}404 Not Found — {{ site_name }}{% endblock %}

{% block content %}
<div style="text-align:center; padding:3rem 0;">
  <h1 style="font-size:5rem; margin:0; color:#ccc;">404</h1>
  <h2>Page Not Found</h2>
  <p>{{ message }}</p>
  <a href="/posts">← Back to all posts</a>
</div>
{% endblock %}
""")

# ---------------------------------------------------------------------------
# App
# ---------------------------------------------------------------------------
app = App()

# Collect sidebar data once
ALL_TAGS = sorted({tag for post in POSTS for tag in post["tags"]})
RECENT = POSTS[-3:][::-1]  # latest 3

app.enable_templates(
    template_dir=TEMPLATE_DIR,
    auto_escape=True,
    globals={
        "site_name":      "The Cello Blog",
        "tagline":        "Ultra-fast web development with Rust + Python",
        "cello_version":  "1.1.0",
        "year":           2026,
        "recent_posts":   RECENT,
        "all_tags":       ALL_TAGS,
    },
)

PAGE_SIZE = 2


@app.get("/")
def home(request):
    html = app.render("index.html", {
        "posts":       POSTS[:PAGE_SIZE],
        "page":        1,
        "total_pages": -(-len(POSTS) // PAGE_SIZE),  # ceiling division
    })
    return Response.html(html)


@app.get("/posts")
def post_list(request):
    try:
        page = int(request.params.get("page", "1"))
    except ValueError:
        page = 1
    page = max(1, page)
    total_pages = -(-len(POSTS) // PAGE_SIZE)
    page = min(page, total_pages)
    start = (page - 1) * PAGE_SIZE
    html = app.render("index.html", {
        "posts":       POSTS[start: start + PAGE_SIZE],
        "page":        page,
        "total_pages": total_pages,
    })
    return Response.html(html)


@app.get("/posts/{id}")
def post_detail(request):
    try:
        post_id = int(request.params["id"])
    except ValueError:
        return _not_found("Invalid post ID.")
    post = next((p for p in POSTS if p["id"] == post_id), None)
    if not post:
        return _not_found(f"Post #{post_id} does not exist.")
    html = app.render("post.html", {"post": post})
    return Response.html(html)


@app.get("/tag/{tag}")
def tag_filter(request):
    tag = request.params["tag"]
    matched = [p for p in POSTS if tag in p["tags"]]
    html = app.render("tag.html", {"tag": tag, "posts": matched})
    return Response.html(html)


def _not_found(message: str):
    html = app.render("404.html", {"message": message})
    return Response.html(html, status=404)


if __name__ == "__main__":
    print(f"Templates: {TEMPLATE_DIR}")
    print("Listening on http://localhost:8082")
    app.run(port=8082)
```

## Running This Example

```bash
python examples/minijinja_blog.py
# Test endpoints:
curl http://127.0.0.1:8082/
curl http://127.0.0.1:8082/posts?page=2
curl http://127.0.0.1:8082/posts/1
curl http://127.0.0.1:8082/posts/99
curl http://127.0.0.1:8082/tag/rust
```

## Key Concepts

- **Pagination Arithmetic**: Implementing clean index slices by combining integer ceiling division and maximum/minimum clamps on parsed parameters.
- **Dynamic 404 Handler**: Defining localized fallback rendering functions that present template error UI pages while returning the appropriate HTTP status codes.
- **Nested Layout Inheritance**: Using intermediate layout templates extending base frameworks to construct standard formats for site sidebars and primary columns.
