---
title: File Storage (S3-Compatible)
description: Upload and stream files to S3-compatible storage
---

# :material-cloud-upload: File Storage (S3-Compatible)

Handling binary uploads efficiently requires streaming data directly to object storage rather than buffering it in memory. This example demonstrates multipart `multipart/form-data` uploads to any S3-compatible bucket (AWS S3, MinIO, Cloudflare R2, Backblaze B2), pre-signed URL generation for direct client-side downloads, and chunked streaming downloads proxied through the Cello server.

## Complete Example

```python
import io
import os
import mimetypes
import hashlib
import uuid
from datetime import datetime, timezone
from typing import AsyncIterator, Optional

import aioboto3
from botocore.exceptions import ClientError

import cello
from cello import Request, Response, on_startup, on_shutdown

# ---------------------------------------------------------------------------
# S3 configuration — override via environment variables
# ---------------------------------------------------------------------------

S3_ENDPOINT   = os.getenv("S3_ENDPOINT",   "http://localhost:9000")   # MinIO default
S3_REGION     = os.getenv("S3_REGION",     "us-east-1")
S3_BUCKET     = os.getenv("S3_BUCKET",     "cello-files")
S3_ACCESS_KEY = os.getenv("S3_ACCESS_KEY", "minioadmin")
S3_SECRET_KEY = os.getenv("S3_SECRET_KEY", "minioadmin")

# Maximum accepted upload size (50 MB)
MAX_UPLOAD_BYTES = 50 * 1024 * 1024

# How long pre-signed URLs remain valid (seconds)
PRESIGN_TTL = 3600   # 1 hour

# ---------------------------------------------------------------------------
# aioboto3 session — shared across all requests
# ---------------------------------------------------------------------------

_session: Optional[aioboto3.Session] = None


@on_startup
async def init_s3():
    """Create the aioboto3 session and ensure the bucket exists."""
    global _session
    _session = aioboto3.Session(
        aws_access_key_id=S3_ACCESS_KEY,
        aws_secret_access_key=S3_SECRET_KEY,
        region_name=S3_REGION,
    )
    # Create the bucket if it does not exist (useful for local MinIO)
    async with _session.client("s3", endpoint_url=S3_ENDPOINT) as s3:
        try:
            await s3.head_bucket(Bucket=S3_BUCKET)
            print(f"[s3] Bucket {S3_BUCKET!r} already exists")
        except ClientError as exc:
            code = exc.response["Error"]["Code"]
            if code in ("404", "NoSuchBucket"):
                await s3.create_bucket(Bucket=S3_BUCKET)
                print(f"[s3] Created bucket {S3_BUCKET!r}")
            else:
                raise


@on_shutdown
async def close_s3():
    """Nothing to close — aioboto3 manages connection pools internally."""
    print("[s3] Shutting down")


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _object_key(filename: str, prefix: str = "uploads") -> str:
    """
    Build a unique, collision-resistant S3 object key.

    Format: ``uploads/2026/06/14/<uuid>-<original-filename>``
    """
    today = datetime.now(tz=timezone.utc)
    date_path = today.strftime("%Y/%m/%d")
    uid = uuid.uuid4().hex[:8]
    safe_name = filename.replace(" ", "_")
    return f"{prefix}/{date_path}/{uid}-{safe_name}"


def _guess_content_type(filename: str) -> str:
    ct, _ = mimetypes.guess_type(filename)
    return ct or "application/octet-stream"


async def _compute_md5(data: bytes) -> str:
    return hashlib.md5(data).hexdigest()


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------

app = cello.App()


# ---------------------------------------------------------------------------
# 1. Multipart upload
# ---------------------------------------------------------------------------

@app.route("/files", methods=["POST"])
async def upload_file(req: Request) -> Response:
    """
    Upload a file via ``multipart/form-data``.

    Form fields:
    - ``file``    (required) — the binary file part
    - ``prefix``  (optional) — custom S3 key prefix, default ``uploads``

    Returns the object metadata including the S3 key and a pre-signed URL
    valid for one hour.

    **Size limit**: 50 MB.  Requests exceeding this are rejected with 413.
    """
    form = await req.form()
    file_part = form.get("file")
    if file_part is None:
        return Response.json({"error": "Missing 'file' field"}, status=400)

    filename    = file_part.filename or "unnamed"
    prefix      = form.get("prefix", "uploads")
    data: bytes = await file_part.read()

    if len(data) > MAX_UPLOAD_BYTES:
        return Response.json(
            {"error": f"File too large (max {MAX_UPLOAD_BYTES // 1024 // 1024} MB)"},
            status=413,
        )

    content_type = file_part.content_type or _guess_content_type(filename)
    key          = _object_key(filename, prefix=prefix)
    md5          = await _compute_md5(data)

    async with _session.client("s3", endpoint_url=S3_ENDPOINT) as s3:
        await s3.put_object(
            Bucket=S3_BUCKET,
            Key=key,
            Body=data,
            ContentType=content_type,
            Metadata={
                "original-filename": filename,
                "md5":               md5,
                "uploaded-by":       req.headers.get("X-User-Id", "anonymous"),
            },
        )

        # Generate a pre-signed download URL valid for PRESIGN_TTL seconds
        presigned_url = await s3.generate_presigned_url(
            "get_object",
            Params={"Bucket": S3_BUCKET, "Key": key},
            ExpiresIn=PRESIGN_TTL,
        )

    return Response.json(
        {
            "key":           key,
            "filename":      filename,
            "size_bytes":    len(data),
            "content_type":  content_type,
            "md5":           md5,
            "download_url":  presigned_url,
            "expires_in":    PRESIGN_TTL,
        },
        status=201,
    )


# ---------------------------------------------------------------------------
# 2. Pre-signed URL generation (for an existing object)
# ---------------------------------------------------------------------------

@app.route("/files/presign", methods=["POST"])
async def generate_presign(req: Request) -> Response:
    """
    Generate a fresh pre-signed download URL for an existing S3 object.

    Body (JSON)::

        {
            "key":     "uploads/2026/06/14/abc123-report.pdf",
            "expires": 7200
        }

    The ``expires`` field is optional (default: ``PRESIGN_TTL`` = 3 600 s).
    This endpoint is useful when the original URL has expired.
    """
    body       = await req.json()
    key        = body.get("key")
    expires_in = int(body.get("expires", PRESIGN_TTL))

    if not key:
        return Response.json({"error": "Missing 'key'"}, status=400)

    expires_in = max(60, min(expires_in, 86_400))   # clamp 1 min – 24 h

    async with _session.client("s3", endpoint_url=S3_ENDPOINT) as s3:
        # Verify the object actually exists before signing
        try:
            head = await s3.head_object(Bucket=S3_BUCKET, Key=key)
        except ClientError as exc:
            if exc.response["Error"]["Code"] == "404":
                return Response.json({"error": "Object not found"}, status=404)
            raise

        url = await s3.generate_presigned_url(
            "get_object",
            Params={"Bucket": S3_BUCKET, "Key": key},
            ExpiresIn=expires_in,
        )

    return Response.json(
        {
            "key":          key,
            "url":          url,
            "expires_in":   expires_in,
            "content_type": head.get("ContentType"),
            "size_bytes":   head.get("ContentLength"),
        }
    )


# ---------------------------------------------------------------------------
# 3. Streaming download (proxied through Cello)
# ---------------------------------------------------------------------------

@app.route("/files/download", methods=["GET"])
async def stream_download(req: Request) -> Response:
    """
    Stream an S3 object back to the client without buffering it in memory.

    Query params:
    - ``key``   (required) — the S3 object key
    - ``inline`` (optional) — if ``true``, set ``Content-Disposition: inline``

    This is useful when you need to add auth checks or logging before
    serving a file rather than using a pre-signed URL.
    """
    key    = req.query_params.get("key")
    inline = req.query_params.get("inline", "false").lower() == "true"

    if not key:
        return Response.json({"error": "Missing query param: key"}, status=400)

    async with _session.client("s3", endpoint_url=S3_ENDPOINT) as s3:
        try:
            obj = await s3.get_object(Bucket=S3_BUCKET, Key=key)
        except ClientError as exc:
            if exc.response["Error"]["Code"] in ("404", "NoSuchKey"):
                return Response.json({"error": "Object not found"}, status=404)
            raise

        content_type   = obj["ContentType"]
        content_length = obj["ContentLength"]
        filename       = obj.get("Metadata", {}).get("original-filename", key.split("/")[-1])
        disposition    = "inline" if inline else f'attachment; filename="{filename}"'

        # Stream the body in 64 KB chunks so we never load the whole file
        CHUNK_SIZE = 64 * 1024

        async def body_generator() -> AsyncIterator[bytes]:
            stream = obj["Body"]
            async with stream:
                while chunk := await stream.read(CHUNK_SIZE):
                    yield chunk

        return Response.stream(
            body_generator(),
            content_type=content_type,
            headers={
                "Content-Length":      str(content_length),
                "Content-Disposition": disposition,
                "Cache-Control":       "private, max-age=3600",
            },
        )


# ---------------------------------------------------------------------------
# 4. List objects
# ---------------------------------------------------------------------------

@app.route("/files", methods=["GET"])
async def list_files(req: Request) -> Response:
    """
    List objects in the bucket under an optional prefix.

    Query params:
    - ``prefix``  (optional) — key prefix to filter (default: ``uploads/``)
    - ``limit``   (optional) — max results, 1–1000 (default: 100)
    """
    prefix = req.query_params.get("prefix", "uploads/")
    limit  = min(int(req.query_params.get("limit", 100)), 1000)

    async with _session.client("s3", endpoint_url=S3_ENDPOINT) as s3:
        response = await s3.list_objects_v2(
            Bucket=S3_BUCKET,
            Prefix=prefix,
            MaxKeys=limit,
        )

    objects = [
        {
            "key":          obj["Key"],
            "size_bytes":   obj["Size"],
            "last_modified": obj["LastModified"].isoformat(),
            "etag":         obj["ETag"].strip('"'),
        }
        for obj in response.get("Contents", [])
    ]

    return Response.json(
        {
            "objects":     objects,
            "count":       len(objects),
            "prefix":      prefix,
            "truncated":   response.get("IsTruncated", False),
        }
    )


# ---------------------------------------------------------------------------
# 5. Delete object
# ---------------------------------------------------------------------------

@app.route("/files", methods=["DELETE"])
async def delete_file(req: Request) -> Response:
    """
    Delete an S3 object by key.

    Body (JSON)::

        {"key": "uploads/2026/06/14/abc123-report.pdf"}
    """
    body = await req.json()
    key  = body.get("key")
    if not key:
        return Response.json({"error": "Missing 'key'"}, status=400)

    async with _session.client("s3", endpoint_url=S3_ENDPOINT) as s3:
        await s3.delete_object(Bucket=S3_BUCKET, Key=key)

    return Response.json({"deleted": key})


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **Multipart upload endpoint** — `POST /files` accepts `multipart/form-data`, reads the binary part into memory, validates the size limit (50 MB), and uploads it to S3 with `put_object`; the original filename and MD5 are stored as S3 object metadata.
- **Pre-signed URL generation** — `POST /files/presign` uses `generate_presigned_url` to create time-limited direct download URLs, offloading bandwidth from the Cello server to S3 for subsequent requests; `expires_in` is clamped between 60 s and 24 h.
- **Streaming download response** — `GET /files/download` uses `Response.stream()` with an async generator that reads the S3 `StreamingBody` in 64 KB chunks, so even a 1 GB file never fully materialises in Python memory.
- **Object key strategy** — keys are namespaced by date (`uploads/YYYY/MM/DD/`) and prefixed with a short UUID to prevent filename collisions while keeping objects browsable in the S3 console.
- **`aioboto3` session** — a single `aioboto3.Session` is created at startup and reused across requests; each route opens a short-lived async context manager (`async with _session.client(…)`) that returns a pooled connection.
- **S3-compatible backends** — all calls use standard AWS SDK semantics; point `S3_ENDPOINT` at a MinIO, Cloudflare R2, or Backblaze B2 endpoint with no code changes.
- **Content-Disposition header** — the `?inline=true` query param switches between `attachment` (forces browser download) and `inline` (renders in browser for images/PDFs).

## Running This Example

```bash
# Start a local MinIO instance (S3-compatible)
docker run -d -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  quay.io/minio/minio server /data --console-address ":9001"

# Install dependencies
pip install cello aioboto3

# Run the server
python examples/advanced/file-storage.py
```

Upload a file and use the returned pre-signed URL:

```bash
# Upload a file
RESP=$(curl -s -X POST http://localhost:8000/files \
  -F "file=@/path/to/document.pdf" \
  -F "prefix=reports")

echo $RESP | python -m json.tool

# Extract the key
KEY=$(echo $RESP | python -c "import sys,json; print(json.load(sys.stdin)['key'])")

# Download via the Cello proxy (streams in chunks)
curl -OJ "http://localhost:8000/files/download?key=$KEY"

# Or generate a fresh pre-signed URL valid for 2 hours
curl -s -X POST http://localhost:8000/files/presign \
  -H "Content-Type: application/json" \
  -d "{\"key\": \"$KEY\", \"expires\": 7200}" | python -m json.tool

# List all uploaded files
curl -s "http://localhost:8000/files?prefix=reports/" | python -m json.tool
```

Open the MinIO console to browse your bucket visually:

```
http://localhost:9001  (user: minioadmin / password: minioadmin)
```
