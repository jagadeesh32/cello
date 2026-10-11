#!/usr/bin/env python3
"""
Real Database & Redis + ORM demo for Cello (v1.4.0).

Unlike the old mock demo, this talks to a **real** PostgreSQL database and a
**real** Redis server through Cello's native pools (deadpool-postgres + the
redis crate). It shows three layers:

  1. Raw async queries        -> request.database.fetch / fetchrow / execute
  2. Transactions             -> @transactional (auto commit / rollback)
  3. The built-in ORM         -> Model.objects.filter(...).order_by(...)
  4. Redis                    -> request.redis.get / set / incr
  5. ORM relations & grouping -> Q, aggregate/annotate, select_related,
                                 prefetch_related, bulk_create/bulk_update,
                                 db.fetch_json (raw fast path)

Prerequisites:
    - PostgreSQL running; a database you can connect to
    - Redis running on localhost:6379

Run with:
    export DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/cello_test
    python examples/database_orm_demo.py

Then:
    curl http://127.0.0.1:8000/users
    curl -X POST http://127.0.0.1:8000/users -d '{"name":"Alice","email":"alice@x.com"}'
    curl http://127.0.0.1:8000/users/1
    curl http://127.0.0.1:8000/orm/active
    curl -X POST http://127.0.0.1:8000/transfer -d '{"from":1,"to":2,"amount":100}'
    curl http://127.0.0.1:8000/cache/hits
    curl http://127.0.0.1:8000/posts            # select_related
    curl http://127.0.0.1:8000/authors          # prefetch_related + reverse accessor
    curl http://127.0.0.1:8000/posts/stats      # aggregate + values().annotate()
    curl "http://127.0.0.1:8000/posts/search?q=cello"   # Q objects
    curl -X POST http://127.0.0.1:8000/posts/bulk -d '{"author":1,"titles":["a","b"]}'
    curl http://127.0.0.1:8000/report           # db.fetch_json

Author: Jagadeesh Katla
"""

import json
import os

from cello import App, Response, DatabaseConfig, RedisConfig
from cello.database import transactional
from cello.orm import (
    Model, AutoField, CharField, BooleanField, IntegerField, ForeignKey, setup,
    Q, Count, Sum, Avg,
)

DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://postgres:postgres@127.0.0.1:5432/cello_test"
)
REDIS_URL = os.environ.get("REDIS_URL", "redis://127.0.0.1:6379")

app = App()
app.enable_database(DatabaseConfig(url=DATABASE_URL, pool_size=10))
app.enable_redis(RedisConfig(url=REDIS_URL))


# ── ORM model ─────────────────────────────────────────────────────────────────

class User(Model):
    id = AutoField()
    name = CharField(max_length=100)
    email = CharField(max_length=255, null=True)
    active = BooleanField(default=True)
    age = IntegerField(null=True)

    class Meta:
        table_name = "users"  # both the ORM and the raw handlers use this table


class Post(Model):
    id = AutoField()
    title = CharField(max_length=200)
    views = IntegerField(default=0)
    published = BooleanField(default=False)
    # Gives User a reverse accessor: `user.posts`.
    author = ForeignKey(User, related_name="posts")

    class Meta:
        table_name = "posts"


# ── Lifecycle: create schema + seed on startup ────────────────────────────────

@app.on_event("startup")
async def startup():
    # Bind the ORM to the live pool.
    setup(app.database)

    # Fresh schema for the demo (drop any leftovers so columns match the model).
    await Post.drop_table()
    await User.drop_table()
    await app.database.execute("DROP TABLE IF EXISTS accounts")

    # Raw DDL through the pool.
    await app.database.execute(
        "CREATE TABLE accounts (id SERIAL PRIMARY KEY, balance INT NOT NULL)"
    )
    await User.create_table()
    await Post.create_table()

    # Seed once (idempotent-ish for the demo).
    if await User.objects.count() == 0:
        await User.objects.create(name="Alice", email="alice@example.com", age=30)
        await User.objects.create(name="Bob", email="bob@example.com", age=25, active=False)
    if await Post.objects.count() == 0:
        alice = await User.objects.get(name="Alice")
        bob = await User.objects.get(name="Bob")
        # One multi-row INSERT ... RETURNING; ids are filled in.
        await Post.objects.bulk_create([
            Post(title="Hello Cello", views=120, published=True, author=alice),
            Post(title="Rust on the hot path", views=300, published=True, author=alice),
            Post(title="Draft notes", author=bob),
        ])
    count = await app.database.fetchval("SELECT count(*) FROM accounts")
    if count == 0:
        await app.database.execute(
            "INSERT INTO accounts (id, balance) VALUES (1, 1000), (2, 500)"
        )

    # Redis warm-up.
    await app.redis.set("app:status", "ready")
    print("startup complete: schema ready, redis warmed", flush=True)


@app.on_event("shutdown")
async def shutdown():
    await app.database.close()
    await app.redis.close()


# ── Raw query handlers (request.database) ─────────────────────────────────────

@app.get("/users")
async def list_users(request):
    """Cache-through list: try Redis, fall back to Postgres."""
    cached = await request.redis.get("users:all")
    if cached:
        await request.redis.incr("cache:hits")
        return {"users": json.loads(cached), "source": "cache"}
    rows = await request.database.fetch("SELECT * FROM users ORDER BY id")
    await request.redis.set("users:all", json.dumps(rows), ex=30)
    await request.redis.incr("cache:misses")
    return {"users": rows, "source": "db"}


@app.post("/users")
async def create_user(request):
    data = request.json()
    new_id = await request.database.fetchval(
        "INSERT INTO users (name, email, age) VALUES ($1, $2, $3) RETURNING id",
        data["name"], data.get("email"), data.get("age"),
    )
    await request.redis.delete("users:all")  # invalidate cache
    return Response.json({"created": True, "id": new_id}, status=201)


@app.get("/users/{id}")
async def get_user(request):
    row = await request.database.fetchrow(
        "SELECT * FROM users WHERE id = $1", int(request.params["id"])
    )
    if row is None:
        return Response.json({"error": "not found"}, status=404)
    return {"user": row}


# ── ORM handlers (Model.objects) ──────────────────────────────────────────────

@app.get("/orm/active")
async def orm_active(request):
    # Return the queryset: Cello runs it after the handler returns and writes
    # the rows straight from Postgres into the response (no Python objects).
    return {"active_users": User.objects.filter(active=True).order_by("-age").limit(10)}


@app.get("/orm/search")
async def orm_search(request):
    q = request.get_query_param("q", "")
    return {"results": User.objects.filter(name__icontains=q).order_by("name")}


@app.get("/orm/adults")
async def orm_adults(request):
    # When the handler needs the objects, await the queryset (instances are
    # built natively; to_dict() is native too).
    users = await User.objects.filter(age__gte=18).order_by("name")
    return {"names": [u.name for u in users], "count": len(users)}


@app.post("/orm/rename")
@transactional
async def orm_rename(request, tx):
    data = request.json()
    await User.objects.filter(id=int(data["id"])).update(name=data["name"])
    # json() runs the query now, inside the transaction, on the same fast path.
    return {"user": await User.objects.filter(id=int(data["id"])).json()}


# ── ORM relations, Q objects, aggregates, bulk (v1.4.0) ───────────────────────

@app.get("/posts")
async def posts(request):
    # One query with a LEFT JOIN; post.author is a User instance.
    rows = await Post.objects.select_related("author").filter(published=True).order_by("-views")
    return {"posts": [{"title": p.title, "views": p.views, "author": p.author.name} for p in rows]}


@app.get("/authors")
async def authors(request):
    # Two queries in total: the users, then all their posts (WHERE author_id = ANY($1)).
    users = await User.objects.prefetch_related("posts").order_by("name")
    out = []
    for u in users:
        mine = await u.posts            # already loaded: no query
        out.append({"name": u.name, "posts": [p.title for p in mine]})
    return {"authors": out}


@app.get("/posts/stats")
async def post_stats(request):
    totals = await Post.objects.aggregate(n=Count(), views=Sum("views"), avg_views=Avg("views"))
    # avg of an INTEGER column is NUMERIC in Postgres: a decimal string.
    per_author = Post.objects.values("author").annotate(n=Count("id"), views=Sum("views")).order_by("-views")
    # The grouped queryset is returned as-is: written natively after the handler.
    return {"totals": totals, "per_author": per_author}


@app.get("/posts/search")
async def post_search(request):
    q = request.get_query_param("q", "")
    # OR across fields; ~Q keeps rows where the condition is false or unknown.
    return {"results": Post.objects.filter(
        Q(title__icontains=q) | Q(views__gte=1000),
        ~Q(published=False),
    ).order_by("id")}


@app.post("/posts/bulk")
async def post_bulk(request):
    data = request.json()
    author = await User.objects.get(id=int(data["author"]))
    created = await Post.objects.bulk_create(
        [Post(title=t, author=author) for t in data.get("titles", [])]
    )
    for p in created:
        p.published = True
    # One UPDATE ... FROM (VALUES ...) for all of them.
    n = await Post.objects.bulk_update(created, ["published"])
    return {"created": [p.id for p in created], "published": n}


@app.get("/report")
async def report(request):
    # Raw SQL, encoded to JSON natively (no Python object per row).
    return {"report": await request.database.fetch_json(
        "SELECT u.name, count(p.id) AS posts FROM users u "
        "LEFT JOIN posts p ON p.author_id = u.id GROUP BY u.name ORDER BY u.name"
    )}


# ── Transaction handler (@transactional) ──────────────────────────────────────

@app.post("/transfer")
@transactional
async def transfer(request, tx):
    data = request.json()
    amount = int(data["amount"])
    await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", amount, data["from"])
    await tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", amount, data["to"])
    # Raises here would roll the whole thing back automatically.
    return {"ok": True, "transferred": amount}


@app.get("/balances")
async def balances(request):
    return {"accounts": await request.database.fetch("SELECT id, balance FROM accounts ORDER BY id")}


@app.get("/cache/hits")
async def cache_hits(request):
    return {
        "hits": await request.redis.get("cache:hits") or 0,
        "misses": await request.redis.get("cache:misses") or 0,
    }


if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
