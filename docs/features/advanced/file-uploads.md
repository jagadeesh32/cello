---
title: File Uploads
description: Multipart form handling and file uploads in Cello Framework
---

# File Uploads

Cello handles multipart form data and file uploads through its Rust-powered `multer` integration. Files are parsed efficiently in Rust and exposed to Python as `UploadedFile` objects with methods for reading, saving, and inspecting uploaded content.

---

## Basic File Upload

### Single File

```python
from cello import App

app = App()

@app.post("/upload")
def upload_file(request):
    form = request.form()
    file = form.get_file("document")

    if file is None:
        return {"error": "No file uploaded"}, 400

    return {
        "filename": file.filename,
        "content_type": file.content_type,
        "size": file.size(),
    }
```

### HTML Form

```html
<form action="/upload" method="POST" enctype="multipart/form-data">
    <input type="file" name="document">
    <button type="submit">Upload</button>
</form>
```

---

## The `UploadedFile` Class

Each uploaded file is represented as an `UploadedFile` object with the following interface:

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `filename` | `str` | Original filename from the client |
| `content_type` | `str` | MIME type (e.g., `"image/png"`) |
| `temp_path` | `str | None` | Temporary file path, if saved to disk |

### Methods

| Method | Return Type | Description |
|--------|-------------|-------------|
| `read()` | `bytes` | Get the file content as raw bytes |
| `read_text()` | `str` | Get the file content as UTF-8 text |
| `size()` | `int` | Get the file size in bytes |
| `save(path)` | `None` | Save the file to the specified filesystem path |
| `extension()` | `str | None` | Get the file extension (e.g., `"png"`) |

### Example

```python
@app.post("/upload")
def upload(request):
    form = request.form()
    file = form.get_file("avatar")

    if file is None:
        return {"error": "No file provided"}, 400

    # Inspect the file
    print(f"Name: {file.filename}")        # "photo.jpg"
    print(f"Type: {file.content_type}")    # "image/jpeg"
    print(f"Size: {file.size()} bytes")    # 245760
    print(f"Ext:  {file.extension()}")     # "jpg"

    # Read content
    raw_bytes = file.read()
    text_content = file.read_text()  # Only for text files

    # Save to disk
    file.save(f"./uploads/{file.filename}")

    return {"saved": file.filename, "size": file.size()}
```

---

## The `FormData` Class

Multipart forms can contain both text fields and file fields. The `FormData` class provides access to both:

### Text Fields

```python
@app.post("/profile")
def update_profile(request):
    form = request.form()

    # Get text fields
    name = form.get("name")                   # Returns str or None
    bio = form.get_or("bio", "No bio set")    # Returns str with default

    # Check field existence
    if form.has_field("email"):
        email = form.get("email")

    # List all field names
    field_names = form.field_names()   # ["name", "bio", "email"]
    field_count = form.field_count()   # 3

    return {"name": name, "bio": bio}
```

### File Fields

```python
@app.post("/documents")
def upload_documents(request):
    form = request.form()

    # Get a single file
    file = form.get_file("document")

    # Check file existence
    if form.has_file("attachment"):
        attachment = form.get_file("attachment")

    # List all file field names
    file_names = form.file_names()   # ["document", "attachment"]
    file_count = form.file_count()   # 2

    return {"files_received": file_count}
```

---

## Multiple File Uploads

### Multiple Files in One Field

When a form field accepts multiple files (e.g., `<input type="file" multiple>`), use `get_files()` to retrieve all of them:

```python
@app.post("/gallery")
def upload_gallery(request):
    form = request.form()

    # Get all files from the "photos" field
    photos = form.get_files("photos")

    results = []
    for photo in photos:
        photo.save(f"./uploads/gallery/{photo.filename}")
        results.append({
            "filename": photo.filename,
            "size": photo.size(),
        })

    return {"uploaded": len(results), "files": results}
```

HTML form:

```html
<form action="/gallery" method="POST" enctype="multipart/form-data">
    <input type="file" name="photos" multiple>
    <button type="submit">Upload Photos</button>
</form>
```

### Multiple File Fields

```python
@app.post("/application")
def submit_application(request):
    form = request.form()

    # Text fields
    name = form.get("name")
    email = form.get("email")

    # Different file fields
    resume = form.get_file("resume")
    cover_letter = form.get_file("cover_letter")
    portfolio = form.get_files("portfolio")  # Multiple files

    if resume:
        resume.save(f"./uploads/resumes/{resume.filename}")

    return {
        "applicant": name,
        "resume": resume.filename if resume else None,
        "portfolio_count": len(portfolio),
    }
```

---

## File Size Limits

### Default Limits

Cello enforces default size limits to prevent abuse:

| Limit | Default |
|-------|---------|
| Maximum file size | 10 MB |
| Maximum total form size | 50 MB |

### Body Limit Middleware

Use the body limit middleware to customize request size limits:

```python
from cello import App

app = App()

# Set maximum request body to 25 MB
app.enable_body_limit(25 * 1024 * 1024)
```

Requests exceeding the limit receive a `413 Payload Too Large` response before the body is fully read.

### Validating File Size in Handlers

You can also validate file size within your handler for more granular control:

```python
MAX_AVATAR_SIZE = 2 * 1024 * 1024  # 2 MB

@app.post("/avatar")
def upload_avatar(request):
    form = request.form()
    file = form.get_file("avatar")

    if file is None:
        return Response.json({"error": "No file"}, status=400)

    if file.size() > MAX_AVATAR_SIZE:
        return Response.json(
            {"error": f"File too large. Max {MAX_AVATAR_SIZE // (1024*1024)} MB"},
            status=413,
        )

    # Validate content type
    allowed_types = ["image/jpeg", "image/png", "image/webp"]
    if file.content_type not in allowed_types:
        return Response.json(
            {"error": f"Invalid file type: {file.content_type}"},
            status=415,
        )

    file.save(f"./uploads/avatars/{file.filename}")
    return {"uploaded": file.filename}
```

---

## URL-Encoded Forms

For non-file forms (`application/x-www-form-urlencoded`), Cello parses the body automatically:

```python
@app.post("/login")
def login(request):
    form = request.form()
    username = form.get("username")
    password = form.get("password")

    if authenticate(username, password):
        return {"token": generate_token(username)}
    return Response.json({"error": "Invalid credentials"}, status=401)
```

HTML form:

```html
<form action="/login" method="POST">
    <input type="text" name="username">
    <input type="password" name="password">
    <button type="submit">Login</button>
</form>
```

---

## Practical Example: Image Upload Service

```python
from cello import App, Response
import os
import uuid

app = App()
app.enable_body_limit(20 * 1024 * 1024)  # 20 MB max

UPLOAD_DIR = "./uploads"
os.makedirs(UPLOAD_DIR, exist_ok=True)

ALLOWED_TYPES = {"image/jpeg", "image/png", "image/gif", "image/webp"}

@app.post("/images")
def upload_image(request):
    form = request.form()
    file = form.get_file("image")

    if not file:
        return Response.json({"error": "No image provided"}, status=400)

    if file.content_type not in ALLOWED_TYPES:
        return Response.json({"error": "Unsupported image type"}, status=415)

    # Generate unique filename
    ext = file.extension() or "bin"
    unique_name = f"{uuid.uuid4().hex}.{ext}"
    save_path = os.path.join(UPLOAD_DIR, unique_name)

    file.save(save_path)

    return {
        "id": unique_name,
        "original_name": file.filename,
        "size": file.size(),
        "content_type": file.content_type,
        "url": f"/uploads/{unique_name}",
    }

if __name__ == "__main__":
    app.run()
```

---

## Next Steps

- [Static Files](static-files.md) - Serve uploaded files back to clients
- [DTOs & Validation](dto-validation.md) - Validate form field values
- [Body Limit Middleware](../middleware/overview.md) - Configure request size limits
