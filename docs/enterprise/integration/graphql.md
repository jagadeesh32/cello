---
title: GraphQL Integration
description: GraphQL support in Cello Framework - queries, mutations, subscriptions, and DataLoader
---

# GraphQL Integration

Cello provides first-class GraphQL support with decorator-based schema definition, DataLoader for N+1 prevention, and WebSocket-based subscriptions.

## Quick Start

```python
from cello import App, GraphQLConfig
from cello.graphql import Query, Mutation, Schema, DataLoader, GraphQL

app = App()

@Query
def users(info) -> list:
    return [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]

@Query
def user(info, id: int) -> dict:
    return {"id": id, "name": "Alice"}

@Mutation
def create_user(info, name: str, email: str) -> dict:
    return {"id": 3, "name": name, "email": email}

schema = Schema().query(users).query(user).mutation(create_user)

# Serve it at POST /graphql
app.enable_graphql(GraphQLConfig(path="/graphql"), schema=schema)

# Or execute directly
engine = schema.build()
result = await engine.execute('{ user(id: 1) { name } }')
# {"data": {"user": {"name": "Alice"}}}
```

## Decorators

### @Query

Marks a function as a GraphQL query resolver.

```python
from cello.graphql import Query

@Query
def books(info) -> list:
    return db.get_all_books()

@Query
def book(info, id: int) -> dict:
    return db.get_book(id)
```

### @Mutation

Marks a function as a GraphQL mutation resolver.

```python
from cello.graphql import Mutation

@Mutation
def create_book(info, title: str, author: str) -> dict:
    return db.create_book(title, author)
```

### @Subscription

Marks an async generator as a GraphQL subscription.

```python
from cello.graphql import Subscription

@Subscription
async def book_added(info):
    async for event in event_stream("book_added"):
        yield event
```

## DataLoader

Prevents N+1 query problems by batching and caching database calls.

```python
from cello.graphql import DataLoader

async def batch_load_authors(ids):
    return [db.get_author(id) for id in ids]

author_loader = DataLoader(batch_fn=batch_load_authors)

# Single load (cached after first call)
author = await author_loader.load(1)

# Batch load
authors = await author_loader.load_many([1, 2, 3])

# Clear cache
author_loader.clear(1)       # Clear single key
author_loader.clear()         # Clear all
```

## Schema Builder

Compose queries, mutations, and subscriptions into a schema using the fluent builder API.

```python
from cello.graphql import Schema

schema = (
    Schema()
    .query(users)
    .query(user)
    .mutation(create_user)
    .mutation(update_user)
    .subscription(user_created)
    .build()
)
```

### Class-Based Types

You can also register entire classes where each method becomes a resolver:

```python
class QueryType:
    def users(self, info) -> list:
        return db.get_all_users()

    def books(self, info) -> list:
        return db.get_all_books()

schema = Schema().query(QueryType).build()
```

## GraphQL Engine

Execute queries directly using the GraphQL engine.

```python
from cello.graphql import GraphQL

engine = GraphQL(max_depth=10, max_complexity=1000, introspection=False)
engine.add_query(users)
engine.add_mutation(create_user)

result = await engine.execute("{ users { id name } }")
# {"data": {"users": [{"id": 1, "name": "Alice"}, ...]}}

# Variables and operation names
result = await engine.execute(
    "mutation Create($name: String!) { createUser(name: $name, email: \"a@x.io\") { id } }",
    variables={"name": "Alice"},
    operation_name="Create",
)
```

How a document is executed:

- It is parsed by Cello's GraphQL parser. The operation type, `operationName`,
  variables (defaults and required checks), aliases, fragments and
  `@skip`/`@include` all come from the document.
- Only the root fields the operation selects run. A query never runs a
  mutation resolver.
- Each resolver is called as `resolver(info, **arguments)`. The arguments are
  the field's own, with variables substituted. `info` is a dict with
  `field_name`, `operation`, `variables`, `selection` and `context` (the HTTP
  request when served by `enable_graphql`).
- The result is trimmed to the sub-selection. Dicts are read by key, objects by
  attribute, and lists element by element.
- A root field `createUser` resolves to a resolver named `createUser` or
  `create_user`.
- `max_depth`, `max_complexity` (fields counted with fragments expanded) and
  `introspection` are checked before any resolver runs. A rejected document
  returns `{"data": None, "errors": [...]}`, which is HTTP 400 at the endpoint.

## Configuration

Serve a schema on your Cello app:

```python
from cello import App, GraphQLConfig

app = App()
app.enable_graphql(
    GraphQLConfig(path="/graphql", max_depth=10, max_complexity=1000),
    schema=schema,
)
```

| Option | Default | Description |
|--------|---------|-------------|
| `path` | `/graphql` | GraphQL endpoint path (`POST` executes) |
| `playground` | `False` | Serve the GraphQL Playground page on `GET` |
| `introspection` | `False` | Allow `__schema` / `__type` |
| `max_depth` | `10` | Deepest field nesting |
| `max_complexity` | `1000` | Most fields per operation |

Since 1.4.1 playground and introspection are off by default (both reveal the
schema to any caller); enable them for development. The endpoint runs behind
the normal middleware chain, so JWT/Basic/API-key auth and rate limiting apply
to it. Add the path to `skip_paths` only if it is meant to be public.
Subscriptions are not served over HTTP.

Without `schema=`, `enable_graphql` serves the Rust endpoint. It applies the
same parser and limits, but resolves only resolvers registered from Rust.

## API Reference

| Class | Description |
|-------|-------------|
| `Query` | Decorator for query resolvers |
| `Mutation` | Decorator for mutation resolvers |
| `Subscription` | Decorator for subscription resolvers |
| `Field` | GraphQL field definition with optional resolver |
| `DataLoader` | Batching and caching loader for N+1 prevention |
| `GraphQL` | Execution engine for running queries |
| `Schema` | Fluent builder for composing a schema |
