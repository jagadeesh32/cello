---
title: Native Schema (body=Model)
description: One model declaration drives request validation, serialization and OpenAPI in Cello 1.4.0, with no database needed.
---

# :material-shield-check: Native Schema

In Cello 1.4.0 a single `Model` declaration is compiled once into a Rust schema registry. The same declaration validates the request body (`body=User`), serializes the response, and produces the OpenAPI `requestBody` schema. This example needs no database: the handler returns the validated, unsaved instance.

## Features Demonstrated

- `@app.post(path, body=Model)` validates the JSON body in Rust before the handler runs and injects a real model instance
- Type coercion (`"36"` becomes `36`) and every error reported in one `400 {"detail": [...]}` response, without echoing input values
- `AutoField` and `read_only=True` fields ignored on input (no mass assignment)
- `write_only=True` fields (here `password`) accepted on input and omitted from output
- OpenAPI request schema generated from the same declaration (`readOnly`, `writeOnly`, `maxLength`, `required`)

## Complete Source Code

```python
"""Native schema demo (v1.4.0): one model declaration drives validation,
serialization and OpenAPI.

    python examples/native_schema_demo.py

    # valid: "age" is coerced, "id" is ignored (read-only), password accepted
    curl -s -X POST localhost:8000/users -H 'Content-Type: application/json' \
         -d '{"id": 99, "email": "ada@example.com", "password": "s3cret", "age": "36"}'

    # invalid: every error is reported, input values are not echoed
    curl -s -X POST localhost:8000/users -H 'Content-Type: application/json' \
         -d '{"email": 5, "age": 1.5}'

    curl -s localhost:8000/openapi.json      # requestBody from the same schema

No database is needed: the handler returns the validated (unsaved) instance.
Call ``await user.save(using=request.database)`` to insert it.
"""

from cello import App
from cello.orm import AutoField, BooleanField, CharField, DateTimeField, IntegerField, JSONField, Model


class User(Model):
    id = AutoField()                                     # read-only on input
    email = CharField(max_length=255)
    password = CharField(max_length=128, write_only=True)  # never serialized
    age = IntegerField()
    active = BooleanField(default=True)
    joined = DateTimeField(null=True)                    # ISO-8601 accepted
    prefs = JSONField(null=True)
    slug = CharField(max_length=64, null=True, read_only=True)


app = App()
app.enable_openapi(title="Native schema demo")


@app.post("/users", body=User)
def create_user(request, user: User):
    # `user` is a real User instance, validated in Rust off the raw body.
    return user  # serialized from the schema: `password` is omitted


@app.get("/")
def index(request):
    return {"try": "POST /users, GET /openapi.json, GET /docs"}


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

## Running This Example

```bash
python examples/native_schema_demo.py      # listens on 127.0.0.1:8000
```

### Try it

A valid body. `age` is coerced from a string, `id` is ignored because it is read-only, and the response has no `password`:

```bash
curl -s -i -X POST localhost:8000/users -H 'Content-Type: application/json' \
     -d '{"id": 99, "email": "ada@example.com", "password": "s3cret", "age": "36"}'
```

```text
HTTP/1.1 200 OK
content-type: application/json

{"id":null,"email":"ada@example.com","age":36,"active":true,"joined":null,"prefs":null,"slug":null}
```

An invalid body. Every problem is listed, and the offending values are not echoed back:

```bash
curl -s -X POST localhost:8000/users -H 'Content-Type: application/json' \
     -d '{"email": 5, "age": 1.5}'
```

```json
{"detail":[{"loc":["email"],"msg":"Input should be a valid string","type":"string_type"},{"loc":["password"],"msg":"Field required","type":"missing"},{"loc":["age"],"msg":"Input should be a valid integer, got a number with a fractional part","type":"int_from_float"}]}
```

The OpenAPI document describes the same model (`GET /openapi.json`, excerpt of `components.schemas.User`):

```json
{"properties":{"id":{"format":"int32","readOnly":true,"type":"integer"},"email":{"maxLength":255,"type":"string"},"password":{"maxLength":128,"type":"string","writeOnly":true},"age":{"format":"int32","type":"integer"},"active":{"default":true,"type":"boolean"},"joined":{"format":"date-time","nullable":true,"type":"string"},"prefs":{"nullable":true},"slug":{"maxLength":64,"nullable":true,"readOnly":true,"type":"string"}},"required":["email","password","age"],"title":"User","type":"object"}
```

## Key Concepts

- **One declaration, three jobs** -- the model is compiled into the Rust schema registry once; validation, serialization and OpenAPI all read from it, so they cannot drift apart.
- **`body=Model`** -- the body is parsed and validated off the raw request bytes in Rust. The handler receives a `User` instance, not a dict.
- **Visibility flags** -- `read_only` fields are ignored when sent by the client; `write_only` fields are accepted but never serialized.
- **Unsaved instance** -- the handler does not touch a database. To insert the row, call `await user.save(using=request.database)`.
- See also [DTO validation](../../features/advanced/dto-validation.md) and the [data layer guide](../../data-layer.md).
