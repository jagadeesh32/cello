#!/usr/bin/env python3
"""
Advanced ORM tour for Cello v1.4.0.

A focused walk through the 1.4.0 ORM on a real PostgreSQL database:

  * Q objects            -> OR / AND / NOT in filter() and exclude()
  * Aggregates           -> aggregate(), values().annotate() (GROUP BY)
  * Relations            -> select_related, prefetch_related, reverse accessor
  * Bulk writes          -> bulk_create / bulk_update
  * Native row pipeline  -> return {"books": qs}, await qs.json(),
                            db.fetch_json(sql)
  * Ambient transactions -> ORM calls inside `async with db.transaction()`
                            commit or roll back together

The demo creates its own tables (``ex_authors`` and ``ex_books``) at startup
(dropping any leftovers first). Remove them afterwards with ``--cleanup``.

Run:
    export DATABASE_URL=postgresql://postgres:postgres@127.0.0.1:5432/cello_test
    python examples/orm_advanced_demo.py --port 8000

Try:
    curl "localhost:8000/books/search?q=rust"
    curl localhost:8000/books/not-drafts
    curl localhost:8000/stats
    curl localhost:8000/stats/by-author
    curl localhost:8000/books/with-author
    curl localhost:8000/authors
    curl localhost:8000/authors/1/books
    curl -X POST localhost:8000/books/bulk \
         -d '{"author": 1, "titles": ["Bulk one", "Bulk two"]}'
    curl localhost:8000/books/popular
    curl localhost:8000/report
    curl -X POST localhost:8000/transfer-views \
         -d '{"from": 1, "to": 2, "views": 50, "fail": true}'
"""

import argparse
import os

from cello import App, DatabaseConfig, Response
from cello.orm import (
    Model, AutoField, CharField, IntegerField, BooleanField, ForeignKey,
    Q, Count, Sum, Avg, Max, setup,
)

DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://postgres:postgres@127.0.0.1:5432/cello_test"
)

app = App()
app.enable_database(DatabaseConfig(url=DATABASE_URL, pool_size=5))


# -- Models -------------------------------------------------------------------

class Author(Model):
    id = AutoField()
    name = CharField(max_length=100)

    class Meta:
        table_name = "ex_authors"


class Book(Model):
    id = AutoField()
    title = CharField(max_length=200)
    views = IntegerField(default=0)
    draft = BooleanField(default=False)
    # Installs the reverse accessor `author.books`.
    author = ForeignKey(Author, related_name="books")

    class Meta:
        table_name = "ex_books"


# -- Schema + seed data -------------------------------------------------------

@app.on_event("startup")
async def startup():
    setup(app.database)
    await Book.drop_table()
    await Author.drop_table()
    await Author.create_table()
    await Book.create_table()

    ada = await Author.objects.create(name="Ada")
    lin = await Author.objects.create(name="Lin")
    await Book.objects.bulk_create([
        Book(title="Rust on the hot path", views=300, author=ada),
        Book(title="Rust for Python people", views=1200, author=ada),
        Book(title="Cooking with SIMD", views=80, author=lin),
        Book(title="Draft: async notes", views=5, draft=True, author=lin),
        Book(title="Scaling Postgres", views=640, author=lin),
    ])
    print("startup complete: ex_authors / ex_books seeded", flush=True)


async def cleanup():
    """Drop the demo tables (also done at the start of every run)."""
    setup(app.database)
    await Book.drop_table()
    await Author.drop_table()


@app.get("/")
def index(request):
    return {"demo": "orm_advanced", "see": "the docstring for curl examples"}


# -- Q objects ----------------------------------------------------------------

@app.get("/books/search")
async def search(request):
    q = request.get_query_param("q", "")
    # (title contains q OR views >= 1000) AND NOT draft.
    # The queryset is returned as-is: rows are written to JSON natively.
    return {"books": Book.objects.filter(
        Q(title__icontains=q) | Q(views__gte=1000),
        ~Q(draft=True),
    ).order_by("id")}


@app.get("/books/not-drafts")
async def not_drafts(request):
    # exclude() is filter(~Q(...)); both are NULL-safe.
    return {"books": Book.objects.exclude(draft=True).order_by("-views").limit(3)}


# -- Aggregates and GROUP BY --------------------------------------------------

@app.get("/stats")
async def stats(request):
    # aggregate() runs now and returns a dict.
    # sum(integer) is a bigint (int); avg(integer) is NUMERIC (decimal string).
    return await Book.objects.aggregate(
        books=Count(), total_views=Sum("views"),
        avg_views=Avg("views"), top=Max("views"),
    )


@app.get("/stats/by-author")
async def stats_by_author(request):
    # values(...).annotate(...) is GROUP BY; returned, so it runs natively.
    return {"by_author": (
        Book.objects.values("author")
        .annotate(books=Count("id"), views=Sum("views"))
        .order_by("-views")
    )}


# -- Relations ----------------------------------------------------------------

@app.get("/books/with-author")
async def books_with_author(request):
    # One query: the books LEFT JOINed to their author.
    rows = await Book.objects.select_related("author").order_by("id")
    return {"books": [{"title": b.title, "author": b.author.name} for b in rows]}


@app.get("/authors")
async def authors(request):
    # Two queries in total: the authors, then every author's books in one
    # `WHERE author_id = ANY($1)`. Awaiting the accessor then runs no query.
    people = await Author.objects.prefetch_related("books").order_by("name")
    out = []
    for a in people:
        mine = await a.books
        out.append({"name": a.name, "books": [b.title for b in mine]})
    return {"authors": out}


@app.get("/authors/{id}/books")
async def author_books(request):
    author = await Author.objects.get(id=int(request.params["id"]))
    # The reverse accessor is a normal queryset, so it chains and can be returned.
    return {"author": author.name, "books": author.books.filter(draft=False).order_by("title")}


# -- Bulk writes --------------------------------------------------------------

@app.post("/books/bulk")
async def bulk(request):
    data = request.json()
    author = await Author.objects.get(id=int(data["author"]))
    # One multi-row INSERT ... RETURNING; the ids are filled in.
    created = await Book.objects.bulk_create(
        [Book(title=t, author=author, draft=True) for t in data["titles"]]
    )
    for book in created:
        book.draft = False
        book.views = 10
    # One UPDATE ... FROM (VALUES ...) for the whole list.
    updated = await Book.objects.bulk_update(created, ["draft", "views"])
    return Response.json({"created": [b.id for b in created], "updated": updated}, status=201)


# -- The three ways to get rows out as JSON -----------------------------------

@app.get("/books/popular")
async def popular(request):
    # `await qs.json()` runs the query now (inside the handler, so inside any
    # transaction the handler holds) and returns pre-serialized JSON.
    return {"popular": await Book.objects.filter(views__gte=100).order_by("-views").json()}


@app.get("/report")
async def report(request):
    # Raw SQL, encoded to JSON without creating a Python object per row.
    return {"report": await request.database.fetch_json(
        "SELECT a.name, count(b.id) AS books FROM ex_authors a "
        "LEFT JOIN ex_books b ON b.author_id = a.id "
        "GROUP BY a.name ORDER BY a.name"
    )}


# -- Ambient transactions -----------------------------------------------------

@app.post("/transfer-views")
async def transfer_views(request):
    data = request.json()
    try:
        async with request.database.transaction():
            # No using=... needed: ORM calls inside the block run on the
            # transaction and commit or roll back with it.
            src = await Book.objects.get(id=int(data["from"]))
            dst = await Book.objects.get(id=int(data["to"]))
            await Book.objects.filter(id=src.id).update(views=src.views - data["views"])
            await Book.objects.filter(id=dst.id).update(views=dst.views + data["views"])
            if data.get("fail"):
                raise RuntimeError("simulated failure after both updates")
    except RuntimeError as exc:
        # Both updates were rolled back.
        after = await Book.objects.filter(id__in=[src.id, dst.id]).order_by("id").values("id", "views")
        return Response.json({"rolled_back": True, "error": str(exc), "views": after}, status=409)
    after = await Book.objects.filter(id__in=[src.id, dst.id]).order_by("id").values("id", "views")
    return {"rolled_back": False, "views": after}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    parser.add_argument("--port", type=int, default=8000)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--cleanup", action="store_true",
                        help="drop the demo tables and exit")
    args = parser.parse_args()
    if args.cleanup:
        import asyncio
        asyncio.run(cleanup())
        print("dropped ex_books and ex_authors")
        raise SystemExit(0)
    app.run(host=args.host, port=args.port)
