---
title: Form Handling
description: Form handling example with multipart uploads and form data in Cello Framework
---

# Form Handling Example

This example demonstrates handling HTML forms, multipart file uploads, and URL-encoded form data in a Cello application.

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
Form handling example - File uploads and form data.

Run: python forms.py
Test: http://127.0.0.1:8000/
"""

from cello import App, Response
import os
import uuid

app = App()
app.enable_logging()

UPLOAD_DIR = "./uploads"
os.makedirs(UPLOAD_DIR, exist_ok=True)

# ===== HTML Pages =====

@app.get("/")
def index(request):
    """Serve the main page with forms."""
    return Response.html("""
    <!DOCTYPE html>
    <html>
    <head><title>Cello Forms Example</title></head>
    <body>
        <h1>Form Examples</h1>

        <h2>1. URL-Encoded Form (Login)</h2>
        <form action="/login" method="POST">
            <input name="username" placeholder="Username" required>
            <input name="password" type="password" placeholder="Password" required>
            <button type="submit">Login</button>
        </form>

        <h2>2. Single File Upload</h2>
        <form action="/upload" method="POST" enctype="multipart/form-data">
            <input name="description" placeholder="Description">
            <input name="file" type="file" required>
            <button type="submit">Upload</button>
        </form>

        <h2>3. Multiple File Upload</h2>
        <form action="/upload/multiple" method="POST" enctype="multipart/form-data">
            <input name="album" placeholder="Album name">
            <input name="photos" type="file" multiple required>
            <button type="submit">Upload Photos</button>
        </form>

        <h2>4. Contact Form</h2>
        <form action="/contact" method="POST" enctype="multipart/form-data">
            <input name="name" placeholder="Your name" required>
            <input name="email" placeholder="Email" required>
            <textarea name="message" placeholder="Message"></textarea>
            <input name="attachment" type="file">
            <button type="submit">Send</button>
        </form>
    </body>
    </html>
    """)


# ===== URL-Encoded Form =====

@app.post("/login")
def login(request):
    """Handle URL-encoded login form."""
    form = request.form()
    username = form.get("username")
    password = form.get("password")

    if not username or not password:
        return Response.json({"error": "Username and password required"}, status=400)

    # In production, verify credentials against a database
    return {
        "status": "authenticated",
        "username": username,
        "token": f"tok_{uuid.uuid4().hex[:16]}",
    }


# ===== Single File Upload =====

@app.post("/upload")
def upload_single(request):
    """Handle single file upload with metadata."""
    form = request.form()

    description = form.get_or("description", "No description")
    file = form.get_file("file")

    if not file:
        return Response.json({"error": "No file provided"}, status=400)

    # Validate file type
    allowed_types = [
        "image/jpeg", "image/png", "image/gif",
        "application/pdf", "text/plain",
    ]
    if file.content_type not in allowed_types:
        return Response.json(
            {"error": f"File type '{file.content_type}' not allowed"},
            status=415,
        )

    # Save with a unique name
    ext = file.extension() or "bin"
    filename = f"{uuid.uuid4().hex}.{ext}"
    filepath = os.path.join(UPLOAD_DIR, filename)
    file.save(filepath)

    return Response.json({
        "uploaded": True,
        "description": description,
        "file": {
            "original_name": file.filename,
            "saved_as": filename,
            "content_type": file.content_type,
            "size_bytes": file.size(),
        },
    }, status=201)


# ===== Multiple File Upload =====

@app.post("/upload/multiple")
def upload_multiple(request):
    """Handle multiple file uploads."""
    form = request.form()
    album = form.get_or("album", "default")
    photos = form.get_files("photos")

    if not photos:
        return Response.json({"error": "No photos provided"}, status=400)

    # Create album directory
    album_dir = os.path.join(UPLOAD_DIR, album)
    os.makedirs(album_dir, exist_ok=True)

    results = []
    for photo in photos:
        ext = photo.extension() or "bin"
        filename = f"{uuid.uuid4().hex[:8]}.{ext}"
        photo.save(os.path.join(album_dir, filename))
        results.append({
            "original_name": photo.filename,
            "saved_as": filename,
            "size_bytes": photo.size(),
        })

    return Response.json({
        "album": album,
        "uploaded_count": len(results),
        "files": results,
    }, status=201)


# ===== Mixed Form (Text + Files) =====

@app.post("/contact")
def contact(request):
    """Handle contact form with optional attachment."""
    form = request.form()

    name = form.get("name")
    email = form.get("email")
    message = form.get_or("message", "")
    attachment = form.get_file("attachment")

    if not name or not email:
        return Response.json({"error": "Name and email required"}, status=400)

    result = {
        "received": True,
        "from": {"name": name, "email": email},
        "message_length": len(message),
        "has_attachment": attachment is not None,
    }

    if attachment:
        ext = attachment.extension() or "bin"
        filename = f"attachment_{uuid.uuid4().hex[:8]}.{ext}"
        attachment.save(os.path.join(UPLOAD_DIR, filename))
        result["attachment"] = {
            "original_name": attachment.filename,
            "saved_as": filename,
            "size_bytes": attachment.size(),
        }

    return result


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Testing with curl

### URL-Encoded Form

```bash
curl -X POST http://127.0.0.1:8000/login \
  -d "username=alice&password=secret"
```

### Single File Upload

```bash
curl -X POST http://127.0.0.1:8000/upload \
  -F "description=My document" \
  -F "file=@document.pdf"
```

### Multiple Files

```bash
curl -X POST http://127.0.0.1:8000/upload/multiple \
  -F "album=vacation" \
  -F "photos=@photo1.jpg" \
  -F "photos=@photo2.jpg" \
  -F "photos=@photo3.jpg"
```

### Contact Form with Attachment

```bash
curl -X POST http://127.0.0.1:8000/contact \
  -F "name=Alice" \
  -F "email=alice@example.com" \
  -F "message=Hello!" \
  -F "attachment=@resume.pdf"
```

---

## Key Concepts

| Concept | Method | Description |
|---------|--------|-------------|
| Text fields | `form.get("name")` | Get a URL-encoded or multipart text field |
| Default values | `form.get_or("name", "default")` | Get with fallback |
| Single file | `form.get_file("field")` | Get one uploaded file |
| Multiple files | `form.get_files("field")` | Get all files for a field |
| File content | `file.read()` / `file.read_text()` | Read bytes or text |
| Save to disk | `file.save("path")` | Write file to filesystem |
| File metadata | `file.filename`, `file.content_type`, `file.size()` | Inspect the upload |

---

## Next Steps

- [REST API](rest-api.md) - Build a JSON API
- [File Uploads](../../features/advanced/file-uploads.md) - Full file upload documentation
- [Full-stack App](../advanced/fullstack.md) - Combine forms with templates
