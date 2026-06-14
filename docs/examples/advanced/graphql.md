---
title: GraphQL API
description: Add a GraphQL endpoint alongside REST routes
---

# :material-graph: GraphQL API

GraphQL and REST are not mutually exclusive. This example shows how to mount a fully featured GraphQL endpoint at `/graphql` inside a Cello app while keeping all existing REST routes. It covers schema definition, query and mutation resolvers, and a DataLoader to eliminate the classic N+1 query problem when fetching nested relations.

## Complete Example

```python
import asyncio
from collections import defaultdict
from typing import Any, Optional

import strawberry
from strawberry.asgi import GraphQL as StrawberryASGI
from strawberry.dataloader import DataLoader

import cello
from cello import Request, Response, route

# ---------------------------------------------------------------------------
# Simulated data layer (replace with real DB queries)
# ---------------------------------------------------------------------------

AUTHORS_DB: dict[int, dict] = {
    1: {"id": 1, "name": "Ada Lovelace",   "email": "ada@example.com"},
    2: {"id": 2, "name": "Grace Hopper",   "email": "grace@example.com"},
    3: {"id": 3, "name": "Linus Torvalds", "email": "linus@example.com"},
}

POSTS_DB: dict[int, dict] = {
    1: {"id": 1, "title": "Notes on the Analytical Engine", "body": "...", "author_id": 1, "published": True},
    2: {"id": 2, "title": "Compiling the Future",           "body": "...", "author_id": 2, "published": True},
    3: {"id": 3, "title": "Open Source Philosophy",         "body": "...", "author_id": 3, "published": True},
    4: {"id": 4, "title": "Draft: Loops in Ada",            "body": "...", "author_id": 1, "published": False},
}

_next_post_id = 5


async def db_get_authors_by_ids(ids: list[int]) -> list[Optional[dict]]:
    """Batch-load authors in a single 'query' (DataLoader batch function)."""
    await asyncio.sleep(0.01)   # simulate I/O
    return [AUTHORS_DB.get(aid) for aid in ids]


async def db_get_posts_by_author(author_id: int) -> list[dict]:
    return [p for p in POSTS_DB.values() if p["author_id"] == author_id]


# ---------------------------------------------------------------------------
# DataLoader — eliminates N+1 author fetches when resolving post.author
# ---------------------------------------------------------------------------

def build_author_loader() -> DataLoader:
    """
    Returns a fresh DataLoader per request so each request has its own
    per-request cache (prevents cross-request data leakage).
    """
    return DataLoader(load_fn=db_get_authors_by_ids)


# ---------------------------------------------------------------------------
# Strawberry GraphQL schema
# ---------------------------------------------------------------------------

@strawberry.type
class Author:
    id:    int
    name:  str
    email: str

    @strawberry.field
    async def posts(self, info: strawberry.types.Info) -> list["Post"]:
        """Resolve the posts written by this author."""
        raw_posts = await db_get_posts_by_author(self.id)
        return [
            Post(
                id=p["id"],
                title=p["title"],
                body=p["body"],
                published=p["published"],
                author_id=p["author_id"],
            )
            for p in raw_posts
        ]


@strawberry.type
class Post:
    id:        int
    title:     str
    body:      str
    published: bool
    author_id: strawberry.Private[int]   # internal; not exposed in schema

    @strawberry.field
    async def author(self, info: strawberry.types.Info) -> Optional[Author]:
        """
        Resolve the author for this post.

        Uses the per-request DataLoader so that resolving 100 posts that
        belong to 3 authors fires exactly 3 DB queries, not 100.
        """
        loader: DataLoader = info.context["author_loader"]
        raw = await loader.load(self.author_id)
        if raw is None:
            return None
        return Author(id=raw["id"], name=raw["name"], email=raw["email"])


@strawberry.type
class Query:

    @strawberry.field
    async def posts(
        self,
        info: strawberry.types.Info,
        published_only: bool = False,
    ) -> list[Post]:
        """Return all posts, optionally filtered to published ones."""
        items = POSTS_DB.values()
        if published_only:
            items = [p for p in items if p["published"]]
        return [
            Post(
                id=p["id"],
                title=p["title"],
                body=p["body"],
                published=p["published"],
                author_id=p["author_id"],
            )
            for p in items
        ]

    @strawberry.field
    async def post(self, info: strawberry.types.Info, id: int) -> Optional[Post]:
        """Fetch a single post by ID."""
        p = POSTS_DB.get(id)
        if p is None:
            return None
        return Post(
            id=p["id"],
            title=p["title"],
            body=p["body"],
            published=p["published"],
            author_id=p["author_id"],
        )

    @strawberry.field
    async def authors(self, info: strawberry.types.Info) -> list[Author]:
        """Return all authors."""
        return [
            Author(id=a["id"], name=a["name"], email=a["email"])
            for a in AUTHORS_DB.values()
        ]

    @strawberry.field
    async def author(self, info: strawberry.types.Info, id: int) -> Optional[Author]:
        """Fetch a single author by ID."""
        a = AUTHORS_DB.get(id)
        if a is None:
            return None
        return Author(id=a["id"], name=a["name"], email=a["email"])


# ---------------------------------------------------------------------------
# Mutations
# ---------------------------------------------------------------------------

@strawberry.input
class CreatePostInput:
    title:     str
    body:      str
    author_id: int
    published: bool = False


@strawberry.input
class UpdatePostInput:
    id:        int
    title:     Optional[str] = strawberry.UNSET
    body:      Optional[str] = strawberry.UNSET
    published: Optional[bool] = strawberry.UNSET


@strawberry.type
class MutationError:
    message: str


# Union return type for create/update mutations
CreatePostResult = strawberry.union("CreatePostResult", [Post, MutationError])
UpdatePostResult = strawberry.union("UpdatePostResult", [Post, MutationError])


@strawberry.type
class Mutation:

    @strawberry.mutation
    async def create_post(
        self, info: strawberry.types.Info, input: CreatePostInput
    ) -> CreatePostResult:  # type: ignore[valid-type]
        """Create a new post and return it."""
        global _next_post_id

        if input.author_id not in AUTHORS_DB:
            return MutationError(message=f"Author {input.author_id} not found")

        post_dict = {
            "id":        _next_post_id,
            "title":     input.title,
            "body":      input.body,
            "published": input.published,
            "author_id": input.author_id,
        }
        POSTS_DB[_next_post_id] = post_dict
        _next_post_id += 1

        return Post(
            id=post_dict["id"],
            title=post_dict["title"],
            body=post_dict["body"],
            published=post_dict["published"],
            author_id=post_dict["author_id"],
        )

    @strawberry.mutation
    async def update_post(
        self, info: strawberry.types.Info, input: UpdatePostInput
    ) -> UpdatePostResult:  # type: ignore[valid-type]
        """Partially update an existing post."""
        post = POSTS_DB.get(input.id)
        if post is None:
            return MutationError(message=f"Post {input.id} not found")

        if input.title is not strawberry.UNSET:
            post["title"] = input.title
        if input.body is not strawberry.UNSET:
            post["body"] = input.body
        if input.published is not strawberry.UNSET:
            post["published"] = input.published

        return Post(
            id=post["id"],
            title=post["title"],
            body=post["body"],
            published=post["published"],
            author_id=post["author_id"],
        )

    @strawberry.mutation
    async def delete_post(self, info: strawberry.types.Info, id: int) -> bool:
        """Delete a post. Returns ``true`` if it existed."""
        return POSTS_DB.pop(id, None) is not None


# ---------------------------------------------------------------------------
# Build the schema and ASGI sub-app
# ---------------------------------------------------------------------------

schema = strawberry.Schema(query=Query, mutation=Mutation)


async def graphql_context(request) -> dict[str, Any]:
    """
    Build a fresh per-request context dict.

    Strawberry passes this dict to every resolver via ``info.context``.
    By creating the DataLoader here we guarantee per-request batching
    without any cross-request cache pollution.
    """
    return {"author_loader": build_author_loader(), "request": request}


graphql_app = StrawberryASGI(schema, context_getter=graphql_context)


# ---------------------------------------------------------------------------
# Cello app — REST routes + mounted GraphQL
# ---------------------------------------------------------------------------

app = cello.App()


# Mount the Strawberry ASGI app at /graphql
# Both GET (GraphiQL playground) and POST (API calls) are handled here.
app.mount("/graphql", graphql_app)


# Regular REST routes co-exist alongside GraphQL
@app.route("/health", methods=["GET"])
async def health(req: Request) -> Response:
    return Response.json({"status": "ok", "graphql": "/graphql"})


@app.route("/schema", methods=["GET"])
async def introspect_schema(req: Request) -> Response:
    """Return the SDL representation of the GraphQL schema."""
    return Response.text(str(schema), content_type="text/plain")


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
```

## Key Concepts

- **Schema definition** — Strawberry uses Python type annotations and `@strawberry.type` / `@strawberry.field` decorators to generate the SDL schema from plain Python classes, keeping schema and resolvers co-located.
- **Query resolvers** — `Query.posts`, `Query.post`, `Query.authors`, and `Query.author` map 1-to-1 to GraphQL query fields; each receives an `info` object carrying the per-request context.
- **Mutation resolvers** — `Mutation.create_post` and `Mutation.update_post` use typed `@strawberry.input` classes for argument validation and return union types (`Post | MutationError`) so clients can handle errors without relying on HTTP status codes.
- **DataLoader for N+1** — `build_author_loader()` is instantiated once per request inside `graphql_context`; when 50 `Post` resolvers each call `loader.load(author_id)`, Strawberry batches all 50 loads into a single `db_get_authors_by_ids` call before the event loop tick ends.
- **Per-request context** — `context_getter` runs before every request and returns a fresh dict; this is where you inject DB sessions, auth principals, and DataLoaders that must not be shared across requests.
- **Mounting at `/graphql`** — `app.mount("/graphql", graphql_app)` delegates all requests under that prefix to the Strawberry ASGI sub-application; the interactive GraphiQL IDE is served automatically on `GET /graphql`.
- **Co-existing with REST** — all existing `@app.route` handlers continue to work normally; GraphQL is just another mounted sub-app.

## Running This Example

```bash
# Install dependencies
pip install cello strawberry-graphql

# Run the server
python examples/advanced/graphql.py
```

Open the interactive GraphiQL IDE in your browser:

```
http://localhost:8000/graphql
```

Or send queries via `curl`:

```bash
# Fetch all published posts with their author names (N+1 safe)
curl -s -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ posts(publishedOnly: true) { id title author { name } } }"
  }' | python -m json.tool

# Create a new post
curl -s -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { createPost(input: { title: \"Hello GraphQL\", body: \"Content here\", authorId: 1, published: true }) { ... on Post { id title } ... on MutationError { message } } }"
  }' | python -m json.tool
```
