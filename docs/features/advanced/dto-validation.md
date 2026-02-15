---
title: DTOs & Validation
description: Data Transfer Objects and validation in Cello Framework
---

# DTOs & Validation

Cello provides a Data Transfer Object (DTO) system inspired by Litestar for controlling which fields are exposed in API inputs and outputs. Combined with optional Pydantic integration, you get full request validation with automatic error responses.

---

## Overview

DTOs solve a common problem: your internal data model has fields that should not be exposed to clients (like password hashes), and input data needs validation before reaching your business logic.

```
Client Request                 DTO Layer                    Internal Model
{                              Filter & Validate            {
  "username": "alice",    -->  - Remove read-only fields    "id": 1,
  "email": "a@b.com",         - Validate types             "username": "alice",
  "password": "secret"        - Check constraints           "email": "a@b.com",
}                              - Rename fields               "password_hash": "...",
                                                             "created_at": "...",
                                                           }
```

---

## DTOConfig

The `DTOConfig` class controls field filtering, renaming, and access rules:

```python
from cello._cello import DTOConfig

# Create DTO for user creation (exclude auto-generated fields)
create_config = (
    DTOConfig()
    .exclude(["id", "created_at"])
    .write_only(["password"])
)

# Create DTO for user response (exclude sensitive fields)
response_config = (
    DTOConfig()
    .exclude(["password"])
    .read_only(["id", "created_at"])
)

# Create DTO for user listing (include only specific fields)
list_config = (
    DTOConfig()
    .include(["id", "username", "is_active"])
)
```

### Configuration Methods

| Method | Description |
|--------|-------------|
| `.include(fields)` | Only include these fields (whitelist) |
| `.exclude(fields)` | Exclude these fields (blacklist) |
| `.read_only(fields)` | Fields that cannot be set in input (auto-generated) |
| `.write_only(fields)` | Fields excluded from output (e.g., passwords) |
| `.rename(field, alias)` | Rename a field in the output (e.g., `created_at` -> `createdAt`) |
| `.max_depth(n)` | Maximum nesting depth for nested DTOs |
| `.partial(bool)` | Allow partial updates (missing fields are ignored) |

---

## Field Filtering

### Include List (Whitelist)

When `include` is set, only the listed fields appear in the output:

```python
list_config = DTOConfig().include(["id", "username", "is_active"])

# Input:  {"id": 1, "username": "alice", "email": "a@b.com", "password": "..."}
# Output: {"id": 1, "username": "alice", "is_active": true}
```

### Exclude List (Blacklist)

When `exclude` is set, the listed fields are removed:

```python
response_config = DTOConfig().exclude(["password", "internal_notes"])

# Input:  {"id": 1, "username": "alice", "password": "hash", "internal_notes": "..."}
# Output: {"id": 1, "username": "alice"}
```

### Read-Only Fields

Read-only fields are rejected if present in input data:

```python
config = DTOConfig().read_only(["id", "created_at"])

# POST {"id": 5, "name": "alice"}
# -> Error: Field 'id' is read-only
```

### Write-Only Fields

Write-only fields are accepted in input but stripped from output:

```python
config = DTOConfig().write_only(["password"])

# Input:  {"username": "alice", "password": "secret"}  -> accepted
# Output: {"username": "alice"}                         -> password removed
```

---

## Field Renaming

Map internal field names to API-friendly names:

```python
config = (
    DTOConfig()
    .rename("created_at", "createdAt")
    .rename("is_active", "isActive")
    .rename("user_name", "userName")
)

# Internal: {"user_name": "alice", "created_at": "2026-01-01", "is_active": true}
# Output:   {"userName": "alice", "createdAt": "2026-01-01", "isActive": true}
```

---

## Pydantic Validation

Cello integrates with Pydantic for request body validation. When you type-hint a handler parameter with a Pydantic `BaseModel`, Cello automatically parses and validates the JSON body.

### Setup

```bash
pip install pydantic
```

### Basic Validation

```python
from pydantic import BaseModel, EmailStr, Field
from cello import App, Response

app = App()

class CreateUser(BaseModel):
    username: str = Field(min_length=3, max_length=50)
    email: str
    password: str = Field(min_length=8)
    age: int = Field(ge=0, le=150)

@app.post("/users")
def create_user(request, user: CreateUser):
    # 'user' is already validated
    return {
        "username": user.username,
        "email": user.email,
        "age": user.age,
    }
```

If validation fails, Cello returns a `422 Unprocessable Entity` response automatically:

```json
{
    "detail": [
        {
            "loc": ["password"],
            "msg": "String should have at least 8 characters",
            "type": "string_too_short"
        }
    ]
}
```

### Type Coercion

Pydantic handles type coercion automatically:

```python
class SearchParams(BaseModel):
    query: str
    limit: int = 10        # String "10" is coerced to int
    offset: int = 0
    active: bool = True    # String "true" is coerced to bool
```

---

## Validation Errors

### Automatic Error Responses

When Pydantic validation fails, Cello responds with structured error details:

```python
class Product(BaseModel):
    name: str = Field(min_length=1)
    price: float = Field(gt=0)
    quantity: int = Field(ge=0)

@app.post("/products")
def create_product(request, product: Product):
    return {"created": product.name}
```

Request with invalid data:

```bash
curl -X POST /products -d '{"name": "", "price": -5, "quantity": -1}'
```

Response:

```json
{
    "detail": [
        {"loc": ["name"], "msg": "String should have at least 1 character", "type": "string_too_short"},
        {"loc": ["price"], "msg": "Input should be greater than 0", "type": "greater_than"},
        {"loc": ["quantity"], "msg": "Input should be greater than or equal to 0", "type": "greater_than_equal"}
    ]
}
```

### Custom Validators

Use Pydantic's `field_validator` for custom validation logic:

```python
from pydantic import BaseModel, field_validator

class CreateUser(BaseModel):
    username: str
    email: str
    password: str

    @field_validator("email")
    @classmethod
    def validate_email(cls, v):
        if "@" not in v:
            raise ValueError("Invalid email format")
        return v.lower()

    @field_validator("password")
    @classmethod
    def validate_password(cls, v):
        if len(v) < 8:
            raise ValueError("Password must be at least 8 characters")
        if not any(c.isupper() for c in v):
            raise ValueError("Password must contain an uppercase letter")
        return v
```

---

## Nested DTOs

DTOs can contain nested models for complex data structures:

```python
from pydantic import BaseModel
from typing import List, Optional

class Address(BaseModel):
    street: str
    city: str
    country: str
    zip_code: str

class CreateUser(BaseModel):
    username: str
    email: str
    address: Optional[Address] = None
    tags: List[str] = []

@app.post("/users")
def create_user(request, user: CreateUser):
    return {
        "username": user.username,
        "city": user.address.city if user.address else None,
    }
```

The `DTOConfig.max_depth()` setting limits how deeply nested DTOs are processed, preventing excessive nesting:

```python
config = DTOConfig().max_depth(3)

# Nesting beyond 3 levels raises DTOError.DepthExceeded
```

---

## DTO Factory Patterns

Use factory methods to create reusable configurations for different operations:

```python
class UserDTOs:
    """Centralized DTO configurations for the User model."""

    @staticmethod
    def for_create():
        return DTOConfig().exclude(["id", "created_at"]).write_only(["password"])

    @staticmethod
    def for_update():
        return DTOConfig().exclude(["id", "created_at"]).partial(True)

    @staticmethod
    def for_response():
        return DTOConfig().exclude(["password"])

    @staticmethod
    def for_list():
        return DTOConfig().include(["id", "username", "is_active"])
```

---

## Complete Example

```python
from cello import App, Response
from pydantic import BaseModel, Field, field_validator
from typing import Optional

app = App()

class CreateUser(BaseModel):
    username: str = Field(min_length=3, max_length=50)
    email: str
    password: str = Field(min_length=8)
    bio: Optional[str] = None

    @field_validator("email")
    @classmethod
    def validate_email(cls, v):
        if "@" not in v:
            raise ValueError("Invalid email address")
        return v.lower()

class UpdateUser(BaseModel):
    username: Optional[str] = Field(None, min_length=3, max_length=50)
    email: Optional[str] = None
    bio: Optional[str] = None

@app.post("/users")
def create_user(request, user: CreateUser):
    # user is validated -- password present, email valid
    new_user = db.create(user.model_dump())
    # Exclude password from response
    return {k: v for k, v in new_user.items() if k != "password"}

@app.patch("/users/{id}")
def update_user(request, updates: UpdateUser):
    user_id = request.params["id"]
    # Only non-None fields are applied
    changes = updates.model_dump(exclude_none=True)
    db.update(user_id, changes)
    return {"updated": True, "fields": list(changes.keys())}

if __name__ == "__main__":
    app.run()
```

---

## Next Steps

- [File Uploads](file-uploads.md) - Validate uploaded file metadata
- [Dependency Injection](dependency-injection.md) - Inject validated services
- [Error Handling](../security/overview.md) - Customize validation error responses
