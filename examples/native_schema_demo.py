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
