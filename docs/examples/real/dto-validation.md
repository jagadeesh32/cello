---
title: DTO Validation with Pydantic
description: Shows how Cello integrates with Pydantic to automatically validate and parse incoming JSON request bodies using Data Transfer Objects (DTOs).
---

# :material-check-decagram: DTO Validation with Pydantic

Cello has first-class support for [Pydantic](https://docs.pydantic.dev/) models as handler parameters. When a Pydantic model is declared as a type-annotated argument, Cello automatically deserialises and validates the incoming JSON body before your handler is ever called. This example also gracefully handles environments where Pydantic is not installed.

## Features Demonstrated

- Declaring a `BaseModel` subclass as a Data Transfer Object (DTO)
- Using Pydantic's `EmailStr` for validated email fields
- Injecting a validated DTO directly into a route handler signature
- Graceful fallback when Pydantic is unavailable at runtime

## Complete Source Code

```python
from cello import App, Response

try:
    from pydantic import BaseModel, EmailStr
    HAS_PYDANTIC = True
except ImportError:
    HAS_PYDANTIC = False

app = App()

if HAS_PYDANTIC:
    class CreateUserDTO(BaseModel):
        username: str
        email: EmailStr
        age: int

    @app.post("/users")
    def create_user(request, user: CreateUserDTO):
        return {"status": "created", "user": user.dict()}
else:
    @app.post("/users")
    def create_user(request):
        return {"error": "Pydantic missing"}

if __name__ == "__main__":
    app.run(port=8080)
```

## Running This Example

```bash
python examples/dto_validation.py
# then test it:
curl -X POST http://127.0.0.1:8080/users \
  -H "Content-Type: application/json" \
  -d '{"username": "alice", "email": "alice@example.com", "age": 30}'
```

## Key Concepts

- **DTO injection** — Annotate a handler parameter with a Pydantic model type and Cello will parse, validate, and inject the model instance automatically.
- **Validation errors** — If the request body fails Pydantic validation (e.g., missing fields, wrong types, invalid email), Cello returns a `422 Unprocessable Entity` response with detailed error information before your code runs.
- **`EmailStr`** — A Pydantic field type that verifies the value is a syntactically valid email address, requiring the `email-validator` package.
- **Optional dependency pattern** — Wrapping the import in a `try/except` block and checking `HAS_PYDANTIC` lets the module load cleanly even without Pydantic installed, making it easy to ship an informative fallback response.
