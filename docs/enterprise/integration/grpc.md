---
title: gRPC Integration
description: gRPC support in Cello Framework - services, methods, streaming, and reflection
---

# gRPC Integration

Cello provides gRPC support with class-based service definitions, automatic method discovery, and Rust-powered serialization.

## Quick Start

```python
from cello import App, GrpcConfig
from cello.grpc import GrpcService, grpc_method, GrpcServer, GrpcRequest, GrpcResponse

app = App()
app.enable_grpc(GrpcConfig(port=50051, reflection=True))

class UserService(GrpcService):
    @grpc_method
    async def GetUser(self, request):
        return GrpcResponse.ok({"id": 1, "name": "Alice"})

    @grpc_method(stream=True)
    async def ListUsers(self, request):
        return GrpcResponse.ok({"users": [{"id": 1}, {"id": 2}]})

server = GrpcServer(host="localhost", port=50051)
server.register_service(UserService())
await server.start()
```

## GrpcService

Base class for defining gRPC services. Methods decorated with `@grpc_method` are automatically discovered.

```python
from cello.grpc import GrpcService, grpc_method

class OrderService(GrpcService):
    name = "OrderService"  # Optional, defaults to class name

    @grpc_method
    async def CreateOrder(self, request):
        return GrpcResponse.ok({"order_id": "123"})

    @grpc_method
    async def GetOrder(self, request):
        order_id = request.data.get("id")
        return GrpcResponse.ok({"id": order_id, "status": "pending"})

# Discover methods
service = OrderService()
print(service.get_methods())  # [{"name": "CreateOrder", ...}, {"name": "GetOrder", ...}]
```

## @grpc_method Decorator

Marks a method as a gRPC endpoint. Supports both unary and streaming modes.

```python
# Unary RPC
@grpc_method
async def GetUser(self, request):
    return GrpcResponse.ok({"id": 1})

# Streaming RPC
@grpc_method(stream=True)
async def ListUsers(self, request):
    return GrpcResponse.ok({"users": [...]})
```

## GrpcRequest and GrpcResponse

### GrpcRequest

```python
request = GrpcRequest(
    service="UserService",
    method="GetUser",
    data={"id": 1},
    metadata={"auth": "token123"}
)

print(request.service)   # "UserService"
print(request.method)    # "GetUser"
print(request.data)      # {"id": 1}
print(request.metadata)  # {"auth": "token123"}
```

### GrpcResponse

```python
# Success response
response = GrpcResponse.ok({"id": 1, "name": "Alice"})

# Error response
response = GrpcResponse.error(code=5, message="User not found")

# Custom response
response = GrpcResponse(
    data={"id": 1},
    status_code=0,
    message="OK",
    metadata={"request-id": "abc123"}
)
```

## GrpcServer

Hosts gRPC services and manages their lifecycle.

```python
from cello.grpc import GrpcServer

server = GrpcServer(host="localhost", port=50051)
server.register_service(UserService())
server.register_service(OrderService())

# Start/stop
await server.start()
await server.stop()

# Query registered services
print(server.get_services())  # {"UserService": <UserService>, "OrderService": <OrderService>}
```

## GrpcChannel (Client)

Connect to gRPC services as a client.

```python
from cello.grpc import GrpcChannel, GrpcRequest

channel = await GrpcChannel.connect("localhost:50051")

response = await channel.call(
    service="UserService",
    method="GetUser",
    request=GrpcRequest(data={"id": 1})
)

print(response.data)  # {"id": 1, "name": "Alice"}
await channel.close()
```

## GrpcError

Standard gRPC status codes for error handling.

```python
from cello.grpc import GrpcError

# Status code constants
GrpcError.OK              # 0
GrpcError.CANCELLED       # 1
GrpcError.UNKNOWN         # 2
GrpcError.INVALID_ARGUMENT # 3
GrpcError.NOT_FOUND       # 5
GrpcError.PERMISSION_DENIED # 7
GrpcError.INTERNAL        # 13
GrpcError.UNAVAILABLE     # 14
GrpcError.UNAUTHENTICATED # 16

# Raise errors
raise GrpcError(code=GrpcError.NOT_FOUND, message="User not found")
```

## Configuration

```python
from cello import App, GrpcConfig

app = App()
app.enable_grpc(GrpcConfig(
    port=50051,
    max_message_size=4_194_304,   # 4MB
    reflection=True,
    enable_web=True,
    keepalive_secs=60,
    concurrency_limit=100
))
```

| Option | Default | Description |
|--------|---------|-------------|
| `port` | `50051` | gRPC server port |
| `max_message_size` | `4MB` | Max message size in bytes |
| `reflection` | `False` | Enable gRPC reflection service |
| `enable_web` | `False` | Enable gRPC-Web for browser clients |
| `keepalive_secs` | `60` | Keepalive interval |
| `concurrency_limit` | `100` | Max concurrent streams |

## API Reference

| Class | Description |
|-------|-------------|
| `GrpcService` | Base class for gRPC service definitions |
| `grpc_method` | Decorator to mark methods as gRPC endpoints |
| `GrpcRequest` | Request wrapper with service, method, data, metadata |
| `GrpcResponse` | Response wrapper with data, status_code, message |
| `GrpcServer` | Server for hosting gRPC services |
| `GrpcChannel` | Client for calling gRPC services |
| `GrpcError` | Exception class with standard gRPC status codes |
| `GrpcConfig` | Rust-backed configuration class |
