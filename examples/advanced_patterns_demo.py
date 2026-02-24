#!/usr/bin/env python3
"""
Cello Framework v1.0.1 - Advanced Patterns Demo
================================================

Demonstrates the three major architectural patterns introduced in v1.0.1:

  1. Event Sourcing - Store all state changes as a sequence of events
  2. CQRS (Command Query Responsibility Segregation) - Separate read and write models
  3. Saga Pattern - Coordinate distributed transactions with compensation

Run with:
    python examples/advanced_patterns_demo.py

Then test with:
    curl http://127.0.0.1:8000/
    curl -X POST http://127.0.0.1:8000/orders -H "Content-Type: application/json" \
         -d '{"user_id": "user-1", "product": "Widget", "quantity": 3, "price": 29.99}'
    curl http://127.0.0.1:8000/orders/order-1
    curl http://127.0.0.1:8000/orders
    curl -X POST http://127.0.0.1:8000/orders/order-1/ship
    curl -X POST http://127.0.0.1:8000/orders/order-1/cancel
    curl http://127.0.0.1:8000/orders/order-1/events
    curl -X POST http://127.0.0.1:8000/saga/place-order \
         -H "Content-Type: application/json" \
         -d '{"user_id": "user-1", "product": "Gadget", "quantity": 1, "price": 99.99}'
    curl http://127.0.0.1:8000/saga/status
    curl http://127.0.0.1:8000/events/stats
    curl http://127.0.0.1:8000/cqrs/stats

Author: Jagadeesh Katla
"""

import asyncio
import uuid
from datetime import datetime

from cello import App, Response, EventSourcingConfig, CqrsConfig, SagaConfig

# ---------------------------------------------------------------------------
# Event Sourcing imports
# ---------------------------------------------------------------------------
from cello.eventsourcing import Event, Aggregate, event_handler, EventStore, Snapshot

# ---------------------------------------------------------------------------
# CQRS imports
# ---------------------------------------------------------------------------
from cello.cqrs import (
    Command,
    Query,
    CommandResult,
    QueryResult,
    command_handler,
    query_handler,
    CommandBus,
    QueryBus,
)

# ---------------------------------------------------------------------------
# Saga imports
# ---------------------------------------------------------------------------
from cello.saga import SagaStep, Saga, SagaExecution, SagaOrchestrator, SagaError

# =============================================================================
# Application Setup
# =============================================================================

app = App()

# Configure v1.0.1 features
app.enable_event_sourcing(EventSourcingConfig.memory())
app.enable_cqrs(CqrsConfig(
    enable_event_sync=True,
    command_timeout_ms=10000,
    query_timeout_ms=5000,
    max_retries=3,
))
app.enable_saga(SagaConfig(
    max_retries=3,
    retry_delay_ms=1000,
    timeout_ms=30000,
    enable_logging=True,
))

# Enable standard middleware
app.enable_cors()
app.enable_logging()

# =============================================================================
# In-Memory Stores (mock data layer for demo)
# =============================================================================

# Read model (projection) for CQRS queries
orders_read_model = {}

# Inventory tracking for saga demo
inventory = {
    "Widget": 100,
    "Gadget": 50,
    "Gizmo": 25,
}

# Payment tracking for saga demo
payments = {}

# Saga execution history
saga_history = []

# Event store instance (in-memory for demo)
event_store = None

# CQRS buses
command_bus = CommandBus()
query_bus = QueryBus()

# Saga orchestrator
orchestrator = SagaOrchestrator()


# =============================================================================
# Event Sourcing: Order Aggregate
# =============================================================================

class OrderAggregate(Aggregate):
    """
    Order aggregate that tracks state through events.

    The aggregate is the core domain object in Event Sourcing.
    All state changes go through events, making the full history
    of changes available for auditing, debugging, and replay.
    """

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.user_id = None
        self.product = None
        self.quantity = 0
        self.price = 0.0
        self.status = "new"
        self.tracking_number = None
        self.cancelled_reason = None

    @event_handler("OrderCreated")
    def on_order_created(self, event):
        """Handle OrderCreated event - initialize order state."""
        self.user_id = event.data.get("user_id")
        self.product = event.data.get("product")
        self.quantity = event.data.get("quantity", 1)
        self.price = event.data.get("price", 0.0)
        self.status = "created"

    @event_handler("OrderConfirmed")
    def on_order_confirmed(self, event):
        """Handle OrderConfirmed event - mark order as confirmed."""
        self.status = "confirmed"

    @event_handler("OrderShipped")
    def on_order_shipped(self, event):
        """Handle OrderShipped event - record shipping details."""
        self.status = "shipped"
        self.tracking_number = event.data.get("tracking_number")

    @event_handler("OrderDelivered")
    def on_order_delivered(self, event):
        """Handle OrderDelivered event - mark as delivered."""
        self.status = "delivered"

    @event_handler("OrderCancelled")
    def on_order_cancelled(self, event):
        """Handle OrderCancelled event - record cancellation."""
        self.status = "cancelled"
        self.cancelled_reason = event.data.get("reason", "No reason provided")

    def to_dict(self):
        """Serialize aggregate state to dict for API responses."""
        return {
            "aggregate_id": self.aggregate_id,
            "user_id": self.user_id,
            "product": self.product,
            "quantity": self.quantity,
            "price": self.price,
            "total": round(self.price * self.quantity, 2),
            "status": self.status,
            "tracking_number": self.tracking_number,
            "cancelled_reason": self.cancelled_reason,
            "version": self.version,
        }


# =============================================================================
# CQRS: Commands
# =============================================================================

class CreateOrderCommand(Command):
    """Command to create a new order."""
    pass


class ShipOrderCommand(Command):
    """Command to ship an existing order."""
    pass


class CancelOrderCommand(Command):
    """Command to cancel an existing order."""
    pass


# =============================================================================
# CQRS: Queries
# =============================================================================

class GetOrderQuery(Query):
    """Query to retrieve a single order by ID."""
    pass


class ListOrdersQuery(Query):
    """Query to list all orders, optionally filtered by status."""
    pass


# =============================================================================
# CQRS: Command Handlers
# =============================================================================

@command_handler(CreateOrderCommand)
async def handle_create_order(cmd):
    """
    Handle CreateOrderCommand.

    Creates a new OrderAggregate, applies the OrderCreated event,
    persists events to the event store, and updates the read model.
    """
    order_id = f"order-{uuid.uuid4().hex[:8]}"
    aggregate = OrderAggregate(aggregate_id=order_id)

    # Apply the creation event
    event = Event(
        "OrderCreated",
        {
            "user_id": cmd.user_id,
            "product": cmd.product,
            "quantity": cmd.quantity,
            "price": cmd.price,
        },
        aggregate_id=order_id,
        metadata={"source": "api", "timestamp": datetime.utcnow().isoformat()},
    )
    aggregate.apply(event)

    # Persist events to event store
    if event_store is not None:
        await event_store.append(order_id, aggregate.uncommitted_events)
    aggregate.clear_uncommitted()

    # Update read model (CQRS projection)
    orders_read_model[order_id] = aggregate.to_dict()

    return CommandResult.ok({
        "order_id": order_id,
        "status": "created",
        "product": cmd.product,
        "quantity": cmd.quantity,
        "total": round(cmd.price * cmd.quantity, 2),
    })


@command_handler(ShipOrderCommand)
async def handle_ship_order(cmd):
    """
    Handle ShipOrderCommand.

    Loads the aggregate from events, applies ShipOrder event,
    and persists the new event.
    """
    order_id = cmd.order_id

    if order_id not in orders_read_model:
        return CommandResult.fail(f"Order {order_id} not found")

    current = orders_read_model[order_id]
    if current["status"] == "cancelled":
        return CommandResult.rejected("Cannot ship a cancelled order")
    if current["status"] == "shipped":
        return CommandResult.rejected("Order already shipped")

    # Rebuild aggregate from event store
    aggregate = OrderAggregate(aggregate_id=order_id)
    if event_store is not None:
        events = await event_store.get_events(order_id)
        aggregate.load_from_events(events)

    # Apply shipping event
    tracking = f"TRK-{uuid.uuid4().hex[:10].upper()}"
    ship_event = Event(
        "OrderShipped",
        {"tracking_number": tracking},
        aggregate_id=order_id,
    )
    aggregate.apply(ship_event)

    # Persist and update read model
    if event_store is not None:
        await event_store.append(order_id, aggregate.uncommitted_events)
    aggregate.clear_uncommitted()
    orders_read_model[order_id] = aggregate.to_dict()

    return CommandResult.ok({
        "order_id": order_id,
        "status": "shipped",
        "tracking_number": tracking,
    })


@command_handler(CancelOrderCommand)
async def handle_cancel_order(cmd):
    """
    Handle CancelOrderCommand.

    Loads the aggregate, verifies it can be cancelled,
    applies the cancellation event.
    """
    order_id = cmd.order_id

    if order_id not in orders_read_model:
        return CommandResult.fail(f"Order {order_id} not found")

    current = orders_read_model[order_id]
    if current["status"] in ("shipped", "delivered"):
        return CommandResult.rejected("Cannot cancel an order that has been shipped")
    if current["status"] == "cancelled":
        return CommandResult.rejected("Order is already cancelled")

    # Rebuild aggregate
    aggregate = OrderAggregate(aggregate_id=order_id)
    if event_store is not None:
        events = await event_store.get_events(order_id)
        aggregate.load_from_events(events)

    # Apply cancel event
    reason = getattr(cmd, "reason", "Cancelled by user")
    cancel_event = Event(
        "OrderCancelled",
        {"reason": reason},
        aggregate_id=order_id,
    )
    aggregate.apply(cancel_event)

    # Persist and update
    if event_store is not None:
        await event_store.append(order_id, aggregate.uncommitted_events)
    aggregate.clear_uncommitted()
    orders_read_model[order_id] = aggregate.to_dict()

    return CommandResult.ok({
        "order_id": order_id,
        "status": "cancelled",
        "reason": reason,
    })


# =============================================================================
# CQRS: Query Handlers
# =============================================================================

@query_handler(GetOrderQuery)
async def handle_get_order(q):
    """
    Handle GetOrderQuery.

    Returns order data from the read model (fast lookup).
    """
    order_id = q.order_id
    if order_id in orders_read_model:
        return QueryResult.ok(orders_read_model[order_id])
    return QueryResult.not_found()


@query_handler(ListOrdersQuery)
async def handle_list_orders(q):
    """
    Handle ListOrdersQuery.

    Returns all orders from the read model, optionally filtered by status.
    """
    status_filter = getattr(q, "status", None)
    orders = list(orders_read_model.values())

    if status_filter:
        orders = [o for o in orders if o["status"] == status_filter]

    return QueryResult.ok({
        "orders": orders,
        "total": len(orders),
    })


# =============================================================================
# Register CQRS handlers on buses
# =============================================================================

command_bus.register(CreateOrderCommand, handle_create_order)
command_bus.register(ShipOrderCommand, handle_ship_order)
command_bus.register(CancelOrderCommand, handle_cancel_order)

query_bus.register(GetOrderQuery, handle_get_order)
query_bus.register(ListOrdersQuery, handle_list_orders)


# =============================================================================
# Saga: Order Placement Saga
# =============================================================================

async def reserve_inventory(context):
    """
    Saga Step 1: Reserve inventory for the order.

    Checks availability and decrements stock.
    """
    product = context.get("product")
    quantity = context.get("quantity", 1)

    if product not in inventory:
        raise SagaError(
            step_name="reserve_inventory",
            original_error=ValueError(f"Product '{product}' not found"),
        )

    if inventory[product] < quantity:
        raise SagaError(
            step_name="reserve_inventory",
            original_error=ValueError(
                f"Insufficient stock for '{product}': "
                f"requested {quantity}, available {inventory[product]}"
            ),
        )

    inventory[product] -= quantity
    context["inventory_reserved"] = True
    context["reserved_quantity"] = quantity
    print(f"  [Saga] Reserved {quantity}x {product} (remaining: {inventory[product]})")
    return True


async def release_inventory(context):
    """
    Compensation for Step 1: Release reserved inventory.
    """
    product = context.get("product")
    quantity = context.get("reserved_quantity", 0)

    if product in inventory and quantity > 0:
        inventory[product] += quantity
        print(f"  [Saga] Released {quantity}x {product} (restored: {inventory[product]})")
    return True


async def process_payment(context):
    """
    Saga Step 2: Process payment for the order.

    Simulates payment processing with a mock payment gateway.
    """
    user_id = context.get("user_id")
    price = context.get("price", 0)
    quantity = context.get("quantity", 1)
    total = round(price * quantity, 2)

    # Simulate payment processing
    payment_id = f"pay-{uuid.uuid4().hex[:8]}"
    payments[payment_id] = {
        "id": payment_id,
        "user_id": user_id,
        "amount": total,
        "status": "completed",
        "timestamp": datetime.utcnow().isoformat(),
    }

    context["payment_id"] = payment_id
    context["payment_amount"] = total
    print(f"  [Saga] Payment {payment_id} processed: ${total}")
    return True


async def refund_payment(context):
    """
    Compensation for Step 2: Refund the payment.
    """
    payment_id = context.get("payment_id")

    if payment_id and payment_id in payments:
        payments[payment_id]["status"] = "refunded"
        amount = payments[payment_id]["amount"]
        print(f"  [Saga] Payment {payment_id} refunded: ${amount}")
    return True


async def create_order_via_saga(context):
    """
    Saga Step 3: Create the order through the command bus.

    This step uses CQRS to actually create the order after
    inventory and payment are confirmed.
    """
    result = await command_bus.dispatch(CreateOrderCommand(
        user_id=context.get("user_id"),
        product=context.get("product"),
        quantity=context.get("quantity", 1),
        price=context.get("price", 0),
    ))

    if not result.success:
        raise SagaError(
            step_name="create_order",
            original_error=RuntimeError(result.error or "Order creation failed"),
        )

    context["order_id"] = result.data["order_id"]
    print(f"  [Saga] Order {result.data['order_id']} created successfully")
    return True


async def cancel_order_compensation(context):
    """
    Compensation for Step 3: Cancel the created order.
    """
    order_id = context.get("order_id")

    if order_id:
        await command_bus.dispatch(CancelOrderCommand(
            order_id=order_id,
            reason="Saga compensation - rolling back",
        ))
        print(f"  [Saga] Order {order_id} cancelled (compensation)")
    return True


async def confirm_order_step(context):
    """
    Saga Step 4: Confirm the order (final step).

    Applies the OrderConfirmed event to finalize the order.
    """
    order_id = context.get("order_id")

    if order_id and order_id in orders_read_model:
        # Rebuild aggregate and apply confirmation
        aggregate = OrderAggregate(aggregate_id=order_id)
        if event_store is not None:
            events = await event_store.get_events(order_id)
            aggregate.load_from_events(events)

        confirm_event = Event(
            "OrderConfirmed",
            {"payment_id": context.get("payment_id")},
            aggregate_id=order_id,
        )
        aggregate.apply(confirm_event)

        if event_store is not None:
            await event_store.append(order_id, aggregate.uncommitted_events)
        aggregate.clear_uncommitted()
        orders_read_model[order_id] = aggregate.to_dict()

        print(f"  [Saga] Order {order_id} confirmed")
    return True


# Define the Order Placement Saga
class OrderPlacementSaga(Saga):
    """
    Saga that orchestrates the full order placement flow:
      1. Reserve inventory (compensate: release inventory)
      2. Process payment (compensate: refund payment)
      3. Create order (compensate: cancel order)
      4. Confirm order (no compensation needed - idempotent)
    """
    steps = [
        SagaStep("reserve_inventory", action=reserve_inventory, compensate=release_inventory),
        SagaStep("process_payment", action=process_payment, compensate=refund_payment),
        SagaStep("create_order", action=create_order_via_saga, compensate=cancel_order_compensation),
        SagaStep("confirm_order", action=confirm_order_step),
    ]


# Register saga with orchestrator
orchestrator.register(OrderPlacementSaga)


# =============================================================================
# Helper functions
# =============================================================================

def format_timestamp():
    """Return current UTC timestamp as ISO string."""
    return datetime.utcnow().isoformat() + "Z"


# =============================================================================
# REST API Endpoints
# =============================================================================

@app.get("/")
def home(request):
    """
    Home endpoint showing all v1.0.1 features and available routes.
    """
    return {
        "framework": "Cello",
        "version": "1.0.1",
        "features": {
            "event_sourcing": {
                "description": "Store all state changes as immutable events",
                "config": "EventSourcingConfig.memory()",
                "components": ["Event", "Aggregate", "EventStore", "Snapshot"],
            },
            "cqrs": {
                "description": "Separate command (write) and query (read) models",
                "config": "CqrsConfig(enable_event_sync=True)",
                "components": ["Command", "Query", "CommandBus", "QueryBus"],
            },
            "saga": {
                "description": "Distributed transaction coordination with compensation",
                "config": "SagaConfig(max_retries=3)",
                "components": ["SagaStep", "Saga", "SagaOrchestrator"],
            },
        },
        "endpoints": {
            "POST /orders": "Create a new order (via CQRS CommandBus)",
            "GET /orders": "List all orders (via CQRS QueryBus)",
            "GET /orders/{id}": "Get order by ID (via CQRS QueryBus)",
            "POST /orders/{id}/ship": "Ship an order (via CQRS CommandBus)",
            "POST /orders/{id}/cancel": "Cancel an order (via CQRS CommandBus)",
            "GET /orders/{id}/events": "Get event history for an order",
            "POST /saga/place-order": "Place order via Saga orchestration",
            "GET /saga/status": "View saga execution history",
            "GET /events/stats": "Event sourcing statistics",
            "GET /cqrs/stats": "CQRS bus statistics",
            "GET /inventory": "Current inventory levels",
        },
        "timestamp": format_timestamp(),
    }


@app.post("/orders")
async def create_order(request):
    """
    Create a new order using the CQRS command bus.

    Expects JSON body:
        {"user_id": "user-1", "product": "Widget", "quantity": 3, "price": 29.99}
    """
    try:
        body = request.json()
    except Exception:
        return Response.json(
            {"error": "Invalid JSON body"},
            status=400,
        )

    user_id = body.get("user_id", "anonymous")
    product = body.get("product", "Unknown")
    quantity = body.get("quantity", 1)
    price = body.get("price", 0.0)

    result = await command_bus.dispatch(CreateOrderCommand(
        user_id=user_id,
        product=product,
        quantity=quantity,
        price=price,
    ))

    if result.success:
        return Response.json(
            {"message": "Order created", "data": result.data},
            status=201,
        )
    else:
        return Response.json(
            {"error": result.error},
            status=400,
        )


@app.get("/orders")
async def list_orders(request):
    """
    List all orders using the CQRS query bus.
    """
    result = await query_bus.execute(ListOrdersQuery())

    if result.found:
        return result.data
    else:
        return {"orders": [], "total": 0}


@app.get("/orders/{id}")
async def get_order(request):
    """
    Get a single order by ID using the CQRS query bus.
    """
    order_id = request.params["id"]
    result = await query_bus.execute(GetOrderQuery(order_id=order_id))

    if result.found:
        return result.data
    else:
        return Response.json(
            {"error": f"Order {order_id} not found"},
            status=404,
        )


@app.post("/orders/{id}/ship")
async def ship_order(request):
    """
    Ship an order using the CQRS command bus.
    """
    order_id = request.params["id"]
    result = await command_bus.dispatch(ShipOrderCommand(order_id=order_id))

    if result.success:
        return {"message": "Order shipped", "data": result.data}
    else:
        status = 400
        if hasattr(result, "status") and result.status == "rejected":
            status = 409
        return Response.json(
            {"error": result.error},
            status=status,
        )


@app.post("/orders/{id}/cancel")
async def cancel_order(request):
    """
    Cancel an order using the CQRS command bus.
    """
    order_id = request.params["id"]

    try:
        body = request.json()
        reason = body.get("reason", "Cancelled by user")
    except Exception:
        reason = "Cancelled by user"

    result = await command_bus.dispatch(CancelOrderCommand(
        order_id=order_id,
        reason=reason,
    ))

    if result.success:
        return {"message": "Order cancelled", "data": result.data}
    else:
        return Response.json(
            {"error": result.error},
            status=400,
        )


@app.get("/orders/{id}/events")
async def get_order_events(request):
    """
    Get the full event history for an order from the event store.

    This demonstrates Event Sourcing's ability to replay
    the complete history of state changes.
    """
    order_id = request.params["id"]

    if event_store is None:
        return Response.json({"error": "Event store not initialized"}, status=503)

    events = await event_store.get_events(order_id)

    if not events:
        return Response.json(
            {"error": f"No events found for {order_id}"},
            status=404,
        )

    return {
        "aggregate_id": order_id,
        "event_count": len(events),
        "events": [e.to_dict() for e in events],
    }


@app.post("/saga/place-order")
async def saga_place_order(request):
    """
    Place an order using the Saga orchestrator.

    This coordinates inventory reservation, payment processing,
    order creation, and confirmation as a distributed transaction.
    If any step fails, completed steps are compensated in reverse.

    Expects JSON body:
        {"user_id": "user-1", "product": "Widget", "quantity": 2, "price": 29.99}
    """
    try:
        body = request.json()
    except Exception:
        return Response.json(
            {"error": "Invalid JSON body"},
            status=400,
        )

    context = {
        "user_id": body.get("user_id", "anonymous"),
        "product": body.get("product", "Unknown"),
        "quantity": body.get("quantity", 1),
        "price": body.get("price", 0.0),
    }

    print(f"\n[Saga] Starting OrderPlacementSaga for {context['product']}...")

    try:
        result = await orchestrator.execute("OrderPlacementSaga", context)

        saga_record = {
            "saga": "OrderPlacementSaga",
            "success": result.success,
            "context": {
                "user_id": context.get("user_id"),
                "product": context.get("product"),
                "quantity": context.get("quantity"),
                "order_id": context.get("order_id"),
                "payment_id": context.get("payment_id"),
            },
            "timestamp": format_timestamp(),
        }
        saga_history.append(saga_record)

        if result.success:
            print(f"[Saga] OrderPlacementSaga completed successfully!\n")
            return Response.json({
                "message": "Order placed successfully via saga",
                "order_id": context.get("order_id"),
                "payment_id": context.get("payment_id"),
                "product": context.get("product"),
                "quantity": context.get("quantity"),
                "total": round(context.get("price", 0) * context.get("quantity", 1), 2),
            }, status=201)
        else:
            print(f"[Saga] OrderPlacementSaga failed, compensation executed.\n")
            return Response.json({
                "error": "Order placement failed",
                "message": "All completed steps have been compensated",
            }, status=400)

    except SagaError as e:
        saga_history.append({
            "saga": "OrderPlacementSaga",
            "success": False,
            "error": str(e),
            "step": e.step_name,
            "timestamp": format_timestamp(),
        })
        return Response.json({
            "error": f"Saga failed at step '{e.step_name}'",
            "detail": str(e.original_error),
        }, status=400)


@app.get("/saga/status")
def saga_status(request):
    """
    View saga execution history.
    """
    executions = orchestrator.list_executions()

    return {
        "registered_sagas": ["OrderPlacementSaga"],
        "total_executions": len(saga_history),
        "history": saga_history[-20:],  # Last 20 executions
    }


@app.get("/events/stats")
async def event_stats(request):
    """
    Event sourcing statistics.

    Shows aggregate counts and event distribution.
    """
    total_events = 0
    aggregates = {}

    for order_id in orders_read_model:
        if event_store is not None:
            events = await event_store.get_events(order_id)
            count = len(events)
            total_events += count
            aggregates[order_id] = {
                "event_count": count,
                "status": orders_read_model[order_id]["status"],
            }

    return {
        "store_type": "memory",
        "total_aggregates": len(orders_read_model),
        "total_events": total_events,
        "aggregates": aggregates,
    }


@app.get("/cqrs/stats")
def cqrs_stats(request):
    """
    CQRS bus statistics.

    Shows registered commands and queries.
    """
    return {
        "command_bus": {
            "registered_commands": [
                "CreateOrderCommand",
                "ShipOrderCommand",
                "CancelOrderCommand",
            ],
            "description": "Handles write operations through commands",
        },
        "query_bus": {
            "registered_queries": [
                "GetOrderQuery",
                "ListOrdersQuery",
            ],
            "description": "Handles read operations through queries",
        },
        "read_model_size": len(orders_read_model),
    }


@app.get("/inventory")
def get_inventory(request):
    """
    Get current inventory levels.
    """
    return {
        "inventory": inventory,
        "total_products": len(inventory),
    }


# =============================================================================
# Lifecycle Hooks
# =============================================================================

@app.on_event("startup")
async def on_startup():
    """Initialize event store on application startup."""
    global event_store
    event_store = await EventStore.connect()
    print("Event store initialized (in-memory)")
    print("CQRS command and query buses ready")
    print("Saga orchestrator ready with OrderPlacementSaga")


@app.on_event("shutdown")
async def on_shutdown():
    """Clean up resources on application shutdown."""
    if event_store is not None:
        await event_store.close()
    print("Event store closed")


# =============================================================================
# Main entry point
# =============================================================================

if __name__ == "__main__":
    print("""
    ================================================================
    Cello Framework v1.0.1 - Advanced Patterns Demo
    ================================================================

    Features demonstrated:
      - Event Sourcing (Order aggregate with event replay)
      - CQRS (Separate CommandBus and QueryBus)
      - Saga Pattern (OrderPlacementSaga with compensation)

    Endpoints:
      GET  /                     - Feature overview
      POST /orders               - Create order (CQRS command)
      GET  /orders               - List orders (CQRS query)
      GET  /orders/{id}          - Get order (CQRS query)
      POST /orders/{id}/ship     - Ship order (CQRS command)
      POST /orders/{id}/cancel   - Cancel order (CQRS command)
      GET  /orders/{id}/events   - Event history (Event Sourcing)
      POST /saga/place-order     - Full saga orchestration
      GET  /saga/status          - Saga execution history
      GET  /events/stats         - Event store statistics
      GET  /cqrs/stats           - CQRS bus statistics
      GET  /inventory            - Current inventory levels

    ================================================================
    """)

    app.run(host="127.0.0.1", port=8000)
