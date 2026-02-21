#!/usr/bin/env python3
"""
API Protocols Demo for Cello v1.0.0.

This example demonstrates the API Protocol features:
  - GraphQL support with Query, Mutation, and Subscription decorators
  - gRPC service definitions with streaming support
  - Kafka messaging with consumers and producers
  - RabbitMQ integration

Run with:
    python examples/api_protocols_demo.py

Then test with:
    curl http://127.0.0.1:8000/
    curl http://127.0.0.1:8000/graphql/status
    curl http://127.0.0.1:8000/grpc/status
    curl http://127.0.0.1:8000/messaging/status
    curl http://127.0.0.1:8000/users
    curl -X POST http://127.0.0.1:8000/users -d '{"name": "Alice", "email": "alice@example.com"}'
    curl -X POST http://127.0.0.1:8000/orders -d '{"user_id": 1, "product": "Widget", "quantity": 3}'
    curl http://127.0.0.1:8000/orders

Author: Jagadeesh Katla
"""

from cello import App, Response

# GraphQL imports
from cello.graphql import Query, Mutation, Subscription, Schema, DataLoader

# gRPC imports
from cello.grpc import GrpcService, grpc_method, GrpcServer, GrpcRequest, GrpcResponse, GrpcConfig

# Messaging imports
from cello.messaging import (
    KafkaConfig,
    kafka_consumer,
    kafka_producer,
    Message,
    Producer,
    Consumer,
    MessageResult,
    RabbitMQConfig,
)

app = App()


# =============================================================================
# Protocol Configuration
# =============================================================================

# Configure gRPC server
grpc_config = GrpcConfig(
    port=50051,
    max_message_size=4 * 1024 * 1024,  # 4MB
    enable_reflection=True,
    enable_grpc_web=True,
)
app.enable_grpc(grpc_config)

# Configure Kafka messaging
kafka_config = KafkaConfig(
    bootstrap_servers="localhost:9092",
    group_id="cello-demo-group",
    auto_offset_reset="earliest",
    enable_auto_commit=True,
)
app.enable_messaging(kafka_config)

# Configure RabbitMQ messaging
rabbitmq_config = RabbitMQConfig(
    url="amqp://guest:guest@localhost:5672/",
    prefetch_count=10,
    exchange="cello_events",
    exchange_type="topic",
)
app.enable_rabbitmq(rabbitmq_config)

# Enable standard middleware
app.enable_cors()
app.enable_logging()


# =============================================================================
# In-memory mock data (simulates database)
# =============================================================================

mock_users = [
    {"id": 1, "name": "Alice", "email": "alice@example.com", "role": "admin"},
    {"id": 2, "name": "Bob", "email": "bob@example.com", "role": "user"},
    {"id": 3, "name": "Charlie", "email": "charlie@example.com", "role": "user"},
]

mock_orders = [
    {"id": 1, "user_id": 1, "product": "Laptop", "quantity": 1, "status": "shipped"},
    {"id": 2, "user_id": 2, "product": "Keyboard", "quantity": 2, "status": "pending"},
]

mock_messages = []

next_user_id = 4
next_order_id = 3


# =============================================================================
# GraphQL Schema Definition
# =============================================================================

# DataLoader for batching user lookups (prevents N+1 queries)
async def batch_load_users(user_ids):
    """Batch load users by IDs to prevent N+1 queries."""
    return [next((u for u in mock_users if u["id"] == uid), None) for uid in user_ids]


user_loader = DataLoader(batch_fn=batch_load_users)


@Query
def users(info) -> list:
    """Fetch all users."""
    return mock_users


@Query
def user(info, id: int) -> dict:
    """Fetch a single user by ID."""
    found = next((u for u in mock_users if u["id"] == id), None)
    if found is None:
        return {"error": "User not found"}
    return found


@Query
def orders(info, user_id: int = None) -> list:
    """Fetch orders, optionally filtered by user_id."""
    if user_id is not None:
        return [o for o in mock_orders if o["user_id"] == user_id]
    return mock_orders


@Mutation
def create_user(info, name: str, email: str, role: str = "user") -> dict:
    """Create a new user."""
    global next_user_id
    new_user = {"id": next_user_id, "name": name, "email": email, "role": role}
    mock_users.append(new_user)
    next_user_id += 1
    return new_user


@Mutation
def create_order(info, user_id: int, product: str, quantity: int = 1) -> dict:
    """Create a new order for a user."""
    global next_order_id
    user = next((u for u in mock_users if u["id"] == user_id), None)
    if user is None:
        return {"error": "User not found"}
    new_order = {
        "id": next_order_id,
        "user_id": user_id,
        "product": product,
        "quantity": quantity,
        "status": "pending",
    }
    mock_orders.append(new_order)
    next_order_id += 1
    return new_order


@Subscription
async def order_updates(info, user_id: int):
    """Subscribe to order status changes for a specific user."""
    # In production, this would yield events from a message broker
    yield {"order_id": 1, "status": "shipped", "user_id": user_id}


# Build the GraphQL schema
schema = Schema(
    queries=[users, user, orders],
    mutations=[create_user, create_order],
    subscriptions=[order_updates],
)

# Mount GraphQL endpoint
app.mount("/graphql", schema)


# =============================================================================
# gRPC Service Definitions
# =============================================================================


class UserService(GrpcService):
    """gRPC service for user operations."""

    service_name = "cello.demo.UserService"

    @grpc_method
    async def GetUser(self, request: GrpcRequest) -> GrpcResponse:
        """Fetch a user by ID."""
        user_id = request.get("id")
        user = next((u for u in mock_users if u["id"] == user_id), None)
        if user is None:
            return GrpcResponse(error="User not found", code=5)  # NOT_FOUND
        return GrpcResponse(data=user)

    @grpc_method
    async def ListUsers(self, request: GrpcRequest) -> GrpcResponse:
        """List all users with optional role filter."""
        role = request.get("role")
        if role:
            filtered = [u for u in mock_users if u["role"] == role]
            return GrpcResponse(data={"users": filtered, "count": len(filtered)})
        return GrpcResponse(data={"users": mock_users, "count": len(mock_users)})

    @grpc_method
    async def CreateUser(self, request: GrpcRequest) -> GrpcResponse:
        """Create a new user via gRPC."""
        global next_user_id
        new_user = {
            "id": next_user_id,
            "name": request.get("name", "Unknown"),
            "email": request.get("email", ""),
            "role": request.get("role", "user"),
        }
        mock_users.append(new_user)
        next_user_id += 1
        return GrpcResponse(data=new_user)


class OrderService(GrpcService):
    """gRPC service for order operations."""

    service_name = "cello.demo.OrderService"

    @grpc_method
    async def GetOrder(self, request: GrpcRequest) -> GrpcResponse:
        """Fetch an order by ID."""
        order_id = request.get("id")
        order = next((o for o in mock_orders if o["id"] == order_id), None)
        if order is None:
            return GrpcResponse(error="Order not found", code=5)
        return GrpcResponse(data=order)

    @grpc_method
    async def ListOrders(self, request: GrpcRequest) -> GrpcResponse:
        """List all orders."""
        return GrpcResponse(data={"orders": mock_orders, "count": len(mock_orders)})


# Register gRPC services
app.add_grpc_service(UserService())
app.add_grpc_service(OrderService())


# =============================================================================
# Kafka Messaging - Consumers
# =============================================================================


@kafka_consumer(topic="user-events", group="cello-demo-group")
async def handle_user_event(message: Message):
    """Process user-related events from Kafka."""
    event = message.json()
    mock_messages.append({
        "source": "kafka",
        "topic": "user-events",
        "event": event,
        "offset": message.offset,
    })
    print(f"[Kafka] Received user event: {event.get('type', 'unknown')}")
    return MessageResult.ACK


@kafka_consumer(topic="order-events", group="cello-demo-group")
async def handle_order_event(message: Message):
    """Process order-related events from Kafka."""
    event = message.json()
    mock_messages.append({
        "source": "kafka",
        "topic": "order-events",
        "event": event,
        "offset": message.offset,
    })

    # Simulate order processing logic
    order_id = event.get("order_id")
    if event.get("type") == "order_created":
        print(f"[Kafka] Processing new order: {order_id}")
    elif event.get("type") == "order_shipped":
        print(f"[Kafka] Order shipped: {order_id}")
        # Update order status in mock data
        order = next((o for o in mock_orders if o["id"] == order_id), None)
        if order:
            order["status"] = "shipped"

    return MessageResult.ACK


# =============================================================================
# Kafka Messaging - Producers
# =============================================================================

# Create a producer instance for publishing messages
order_producer = Producer(topic="order-events", config=kafka_config)
user_producer = Producer(topic="user-events", config=kafka_config)


# =============================================================================
# REST API Routes
# =============================================================================


@app.get("/")
def home(request):
    """Root endpoint with feature overview."""
    return {
        "message": "Cello v1.0.0 - API Protocols Demo",
        "features": {
            "graphql": "Schema-first GraphQL with Query, Mutation, Subscription",
            "grpc": "gRPC services with reflection and gRPC-Web support",
            "kafka": "Kafka consumer/producer with message processing",
            "rabbitmq": "RabbitMQ integration with topic exchanges",
        },
        "endpoints": {
            "rest": [
                "GET  /                   - This overview",
                "GET  /users              - List all users",
                "POST /users              - Create a user (produces Kafka event)",
                "GET  /orders             - List all orders",
                "POST /orders             - Create an order (produces Kafka event)",
                "GET  /messages           - View received messages",
            ],
            "graphql": [
                "POST /graphql            - GraphQL queries and mutations",
                "WS   /graphql            - GraphQL subscriptions (WebSocket)",
                "GET  /graphql/status     - GraphQL schema status",
            ],
            "grpc": [
                "gRPC :50051              - UserService.GetUser",
                "gRPC :50051              - UserService.ListUsers",
                "gRPC :50051              - UserService.CreateUser",
                "gRPC :50051              - OrderService.GetOrder",
                "gRPC :50051              - OrderService.ListOrders",
                "GET  /grpc/status        - gRPC server status",
            ],
            "messaging": [
                "Kafka consumer           - user-events topic",
                "Kafka consumer           - order-events topic",
                "GET  /messaging/status   - Messaging status",
            ],
        },
    }


@app.get("/users")
def list_users(request):
    """List all users."""
    return {
        "users": mock_users,
        "count": len(mock_users),
        "note": "Also available via GraphQL: query { users { id name email } }",
    }


@app.post("/users")
@kafka_producer(topic="user-events")
async def create_user_rest(request):
    """Create a new user and publish a Kafka event."""
    global next_user_id
    try:
        data = request.json()
        new_user = {
            "id": next_user_id,
            "name": data.get("name", "Anonymous"),
            "email": data.get("email", ""),
            "role": data.get("role", "user"),
        }
        mock_users.append(new_user)
        next_user_id += 1

        # The @kafka_producer decorator automatically publishes the return
        # value as a message to the configured topic
        return Response.json(
            {
                "user": new_user,
                "created": True,
                "event_published": {
                    "topic": "user-events",
                    "type": "user_created",
                },
            },
            status=201,
        )
    except Exception as e:
        return Response.json({"error": str(e)}, status=400)


@app.get("/orders")
def list_orders(request):
    """List all orders."""
    return {
        "orders": mock_orders,
        "count": len(mock_orders),
        "note": "Also available via GraphQL: query { orders { id product status } }",
    }


@app.post("/orders")
@kafka_producer(topic="order-events")
async def create_order_rest(request):
    """Create a new order and publish a Kafka event."""
    global next_order_id
    try:
        data = request.json()
        user_id = int(data.get("user_id", 0))
        user = next((u for u in mock_users if u["id"] == user_id), None)

        if user is None:
            return Response.json({"error": "User not found"}, status=404)

        new_order = {
            "id": next_order_id,
            "user_id": user_id,
            "product": data.get("product", ""),
            "quantity": int(data.get("quantity", 1)),
            "status": "pending",
        }
        mock_orders.append(new_order)
        next_order_id += 1

        return Response.json(
            {
                "order": new_order,
                "created": True,
                "event_published": {
                    "topic": "order-events",
                    "type": "order_created",
                },
            },
            status=201,
        )
    except (ValueError, TypeError) as e:
        return Response.json({"error": f"Invalid input: {e}"}, status=400)


@app.get("/messages")
def list_messages(request):
    """View received Kafka/RabbitMQ messages."""
    return {
        "messages": mock_messages,
        "count": len(mock_messages),
        "note": "Messages received by Kafka consumers appear here",
    }


# =============================================================================
# Protocol Status Endpoints
# =============================================================================


@app.get("/graphql/status")
def graphql_status(request):
    """GraphQL schema and endpoint status."""
    return {
        "graphql": {
            "status": "active",
            "endpoint": "/graphql",
            "schema": {
                "queries": ["users", "user(id)", "orders(user_id)"],
                "mutations": ["createUser(name, email, role)", "createOrder(user_id, product, quantity)"],
                "subscriptions": ["orderUpdates(user_id)"],
            },
            "features": {
                "data_loader": "Enabled - batches user lookups to prevent N+1",
                "subscriptions": "WebSocket-based real-time updates",
                "introspection": "Enabled",
            },
            "example_query": 'query { users { id name email } }',
            "example_mutation": 'mutation { createUser(name: "Dave", email: "dave@example.com") { id name } }',
        }
    }


@app.get("/grpc/status")
def grpc_status(request):
    """gRPC server and service status."""
    return {
        "grpc": {
            "status": "active",
            "port": grpc_config.port,
            "max_message_size": grpc_config.max_message_size,
            "reflection_enabled": grpc_config.enable_reflection,
            "grpc_web_enabled": grpc_config.enable_grpc_web,
            "services": {
                "cello.demo.UserService": {
                    "methods": ["GetUser", "ListUsers", "CreateUser"],
                },
                "cello.demo.OrderService": {
                    "methods": ["GetOrder", "ListOrders"],
                },
            },
            "note": "Use grpcurl or a gRPC client to interact with services",
        }
    }


@app.get("/messaging/status")
def messaging_status(request):
    """Kafka and RabbitMQ messaging status."""
    return {
        "messaging": {
            "kafka": {
                "status": "active",
                "bootstrap_servers": kafka_config.bootstrap_servers,
                "group_id": kafka_config.group_id,
                "consumers": [
                    {"topic": "user-events", "handler": "handle_user_event"},
                    {"topic": "order-events", "handler": "handle_order_event"},
                ],
                "producers": [
                    {"topic": "user-events", "trigger": "POST /users"},
                    {"topic": "order-events", "trigger": "POST /orders"},
                ],
            },
            "rabbitmq": {
                "status": "configured",
                "exchange": rabbitmq_config.exchange,
                "exchange_type": rabbitmq_config.exchange_type,
                "prefetch_count": rabbitmq_config.prefetch_count,
            },
            "messages_received": len(mock_messages),
        }
    }


# =============================================================================
# Configuration Reference
# =============================================================================


@app.get("/config")
def show_config(request):
    """Show available configuration options for API Protocol features."""
    return {
        "GraphQL": {
            "Schema": "Build from @Query, @Mutation, @Subscription decorated functions",
            "DataLoader": "Batch loading to prevent N+1 queries",
            "Subscriptions": "Real-time updates via WebSocket transport",
            "Introspection": "Schema introspection for tooling (GraphiQL, Apollo)",
        },
        "GrpcConfig": {
            "port": "gRPC server port (default: 50051)",
            "max_message_size": "Maximum message size in bytes (default: 4MB)",
            "enable_reflection": "Enable gRPC reflection service (default: True)",
            "enable_grpc_web": "Enable gRPC-Web for browser clients (default: False)",
        },
        "KafkaConfig": {
            "bootstrap_servers": "Kafka broker addresses (comma-separated)",
            "group_id": "Consumer group ID",
            "auto_offset_reset": "Where to start reading: earliest or latest",
            "enable_auto_commit": "Auto-commit offsets (default: True)",
            "security_protocol": "PLAINTEXT, SSL, SASL_PLAINTEXT, SASL_SSL",
        },
        "RabbitMQConfig": {
            "url": "AMQP connection URL",
            "prefetch_count": "Messages to prefetch per consumer (default: 10)",
            "exchange": "Exchange name for publishing",
            "exchange_type": "Exchange type: direct, topic, fanout, headers",
        },
    }


if __name__ == "__main__":
    print("Cello v1.0.0 - API Protocols Demo")
    print()
    print("  REST endpoints:")
    print("  - GET  /                   - Feature overview")
    print("  - GET  /users              - List all users")
    print("  - POST /users              - Create a user")
    print("  - GET  /orders             - List all orders")
    print("  - POST /orders             - Create an order")
    print("  - GET  /messages           - View received messages")
    print()
    print("  Protocol status:")
    print("  - GET  /graphql/status     - GraphQL schema status")
    print("  - GET  /grpc/status        - gRPC server status")
    print("  - GET  /messaging/status   - Messaging status")
    print("  - GET  /config             - Configuration reference")
    print()
    print("  GraphQL:")
    print("  - POST /graphql            - GraphQL endpoint")
    print("  - WS   /graphql            - Subscriptions (WebSocket)")
    print()
    print("  gRPC:")
    print("  - gRPC :50051              - UserService, OrderService")
    print()
    app.run(host="127.0.0.1", port=8000)
