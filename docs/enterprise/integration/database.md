---
title: Database Integration
description: Database connection pooling and Redis integration in Cello Framework
---

# Database Integration

Cello v0.8.0 introduced async database connection pooling, Redis integration, and automatic transaction management.

## Database Connection Pooling

### Configuration

```python
from cello import App, DatabaseConfig

app = App()

app.enable_database(DatabaseConfig(
    url="postgresql://user:pass@localhost:5432/mydb",
    pool_size=20,
    max_lifetime_secs=1800,
    idle_timeout_secs=600,
    connect_timeout_secs=5
))

# Local development
app.enable_database(DatabaseConfig.local())  # sqlite://localhost/cello_dev
```

| Option | Default | Description |
|--------|---------|-------------|
| `url` | `sqlite://localhost/cello` | Database connection URL |
| `pool_size` | `10` | Maximum pool connections |
| `max_lifetime_secs` | `1800` | Max connection lifetime |
| `idle_timeout_secs` | `300` | Idle connection timeout |
| `connect_timeout_secs` | `5` | Connection timeout |

### Database Class

```python
from cello.database import Database

db = Database(url="postgresql://localhost/mydb")
await db.connect()

# Queries
rows = await db.fetch_all("SELECT * FROM users")
user = await db.fetch_one("SELECT * FROM users WHERE id = $1", [1])
await db.execute("INSERT INTO users (name) VALUES ($1)", ["Alice"])

# Transactions
tx = await db.begin_transaction()
await tx.execute("UPDATE accounts SET balance = balance - 100 WHERE id = $1", [1])
await tx.execute("UPDATE accounts SET balance = balance + 100 WHERE id = $1", [2])
await tx.commit()

await db.close()
```

### Transaction Decorator

```python
from cello.database import transactional

@app.post("/transfer")
@transactional
async def transfer(request):
    data = request.json()
    # Automatically wrapped in a transaction
    # Commits on success, rolls back on exception
    return {"success": True}
```

## Redis Integration

### Configuration

```python
from cello import App, RedisConfig

app = App()

app.enable_redis(RedisConfig(
    url="redis://localhost:6379",
    pool_size=10,
    key_prefix="myapp:",
    default_ttl_secs=300
))

# Local development
app.enable_redis(RedisConfig.local())  # redis://localhost:6379

# Cluster mode
config = RedisConfig.cluster(nodes=["redis1:6379", "redis2:6379", "redis3:6379"])
```

### Redis Class

```python
from cello.database import Redis

redis = Redis(url="redis://localhost:6379")
await redis.connect()

# Key-value operations
await redis.set("key", "value")
value = await redis.get("key")
await redis.delete("key")
exists = await redis.exists("key")

# Atomic counters
await redis.incr("counter")
await redis.decr("counter")

# Expiration
await redis.expire("key", 300)

# Hash operations
await redis.hset("user:1", "name", "Alice")
name = await redis.hget("user:1", "name")
all_fields = await redis.hgetall("user:1")

# List operations
await redis.lpush("queue", "item1")
await redis.rpush("queue", "item2")
item = await redis.lpop("queue")

# Pub/Sub
await redis.publish("channel", "message")

await redis.close()
```

## API Reference

| Class | Description |
|-------|-------------|
| `DatabaseConfig` | Database connection pool configuration (Rust-backed) |
| `RedisConfig` | Redis connection configuration (Rust-backed) |
| `Database` | Async database client with pooling |
| `Redis` | Async Redis client with pool |
| `Transaction` | Database transaction wrapper |
| `transactional` | Decorator for automatic transaction management |
