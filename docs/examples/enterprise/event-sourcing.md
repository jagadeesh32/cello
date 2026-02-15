---
title: Event Sourcing
description: Event sourcing example with aggregates, event store, CQRS projections, and saga orchestration
---

# Event Sourcing Example

This example demonstrates event sourcing with Cello's v0.10.0 features. Instead of storing current state directly, all changes are recorded as immutable events. State is reconstructed by replaying events, and sagas coordinate multi-step workflows with automatic compensation.

---

## Architecture

```
┌──────────┐    Commands     ┌──────────────┐    Events     ┌──────────────┐
│  Client   │ ──────────────>│   Command    │ ────────────>│  Event Store │
│           │                │   Handlers   │               │  (append)    │
└──────────┘                └──────────────┘               └──────┬───────┘
                                                                  |
                            ┌──────────────┐               ┌──────▼───────┐
                            │   Read API    │<─────────────│  Projections │
                            │   (queries)   │  Materialized│  (read model)│
                            └──────────────┘    views      └──────────────┘

                            ┌──────────────┐
                            │    Saga      │  Orchestrates multi-step
                            │ Orchestrator │  workflows with compensation
                            └──────────────┘
```

- **Command Handlers** validate and produce events
- **Event Store** persists an immutable, append-only log
- **Projections** build read-optimized views from events
- **Saga Orchestrator** coordinates multi-step business processes

---

## Full Source Code

```python
#!/usr/bin/env python3
"""
Event sourcing example with Cello.

Demonstrates:
- Append-only event store
- Aggregate state reconstruction
- CQRS read/write separation
- Saga orchestration with compensation
"""

from cello import App, Blueprint, Response, EventSourcingConfig, SagaConfig
import json
import time
import uuid

app = App()
app.enable_cors()
app.enable_logging()

# ===================================================================
# Event Store (in-memory)
# ===================================================================

event_store = {}       # aggregate_id -> [events]
snapshots = {}         # aggregate_id -> snapshot
projections = {        # Read-model views
    "accounts": {},    # account_id -> current state
    "transactions": [] # flat list for querying
}

def append_event(aggregate_id, event_type, data, expected_version=None):
    """Append an event to the store with optimistic concurrency."""
    events = event_store.setdefault(aggregate_id, [])

    current_version = len(events)
    if expected_version is not None and current_version != expected_version:
        raise ValueError(
            f"Concurrency conflict: expected version {expected_version}, "
            f"actual {current_version}"
        )

    event = {
        "id": f"evt-{uuid.uuid4().hex[:8]}",
        "aggregate_id": aggregate_id,
        "type": event_type,
        "data": data,
        "version": current_version + 1,
        "timestamp": time.time(),
    }
    events.append(event)

    # Update projections
    apply_projection(event)
    return event

def get_events(aggregate_id, from_version=0):
    """Retrieve events for an aggregate from a given version."""
    events = event_store.get(aggregate_id, [])
    return [e for e in events if e["version"] > from_version]

def rebuild_state(aggregate_id):
    """Reconstruct aggregate state by replaying all events."""
    events = event_store.get(aggregate_id, [])
    state = {"id": aggregate_id, "balance": 0, "status": "unknown", "version": 0}

    for event in events:
        state = apply_event_to_state(state, event)

    return state


# ===================================================================
# Event Application Logic
# ===================================================================

def apply_event_to_state(state, event):
    """Apply a single event to an aggregate state."""
    t = event["type"]
    d = event["data"]

    if t == "AccountOpened":
        state["owner"] = d["owner"]
        state["balance"] = d.get("initial_balance", 0)
        state["status"] = "active"
    elif t == "MoneyDeposited":
        state["balance"] += d["amount"]
    elif t == "MoneyWithdrawn":
        state["balance"] -= d["amount"]
    elif t == "AccountClosed":
        state["status"] = "closed"
    elif t == "TransferInitiated":
        state["pending_transfer"] = d["transfer_id"]
    elif t == "TransferCompleted":
        state.pop("pending_transfer", None)
    elif t == "TransferFailed":
        state.pop("pending_transfer", None)

    state["version"] = event["version"]
    return state

def apply_projection(event):
    """Update read-model projections from a new event."""
    agg_id = event["aggregate_id"]
    t = event["type"]
    d = event["data"]

    # Update accounts projection
    if t == "AccountOpened":
        projections["accounts"][agg_id] = {
            "id": agg_id,
            "owner": d["owner"],
            "balance": d.get("initial_balance", 0),
            "status": "active",
            "opened_at": event["timestamp"],
        }
    elif t == "MoneyDeposited":
        if agg_id in projections["accounts"]:
            projections["accounts"][agg_id]["balance"] += d["amount"]
    elif t == "MoneyWithdrawn":
        if agg_id in projections["accounts"]:
            projections["accounts"][agg_id]["balance"] -= d["amount"]
    elif t == "AccountClosed":
        if agg_id in projections["accounts"]:
            projections["accounts"][agg_id]["status"] = "closed"

    # Update transactions projection
    if t in ("MoneyDeposited", "MoneyWithdrawn"):
        projections["transactions"].append({
            "id": event["id"],
            "account_id": agg_id,
            "type": "deposit" if t == "MoneyDeposited" else "withdrawal",
            "amount": d["amount"],
            "timestamp": event["timestamp"],
        })


# ===================================================================
# Saga Orchestrator (Transfer between accounts)
# ===================================================================

sagas = {}  # saga_id -> saga state

def start_transfer_saga(from_account, to_account, amount):
    """Start a money transfer saga between two accounts."""
    saga_id = f"saga-{uuid.uuid4().hex[:8]}"
    saga = {
        "id": saga_id,
        "type": "MoneyTransfer",
        "status": "running",
        "from_account": from_account,
        "to_account": to_account,
        "amount": amount,
        "steps": [],
        "started_at": time.time(),
    }
    sagas[saga_id] = saga

    try:
        # Step 1: Withdraw from source
        from_state = rebuild_state(from_account)
        if from_state["balance"] < amount:
            raise ValueError(f"Insufficient balance: {from_state['balance']} < {amount}")

        append_event(from_account, "MoneyWithdrawn", {
            "amount": amount, "reason": f"Transfer {saga_id}"
        })
        saga["steps"].append({"name": "withdraw", "status": "completed"})

        # Step 2: Deposit to destination
        to_state = rebuild_state(to_account)
        if to_state.get("status") != "active":
            raise ValueError(f"Destination account {to_account} is not active")

        append_event(to_account, "MoneyDeposited", {
            "amount": amount, "reason": f"Transfer {saga_id}"
        })
        saga["steps"].append({"name": "deposit", "status": "completed"})

        # Step 3: Mark transfer complete on both sides
        append_event(from_account, "TransferCompleted", {"transfer_id": saga_id})
        append_event(to_account, "TransferCompleted", {"transfer_id": saga_id})

        saga["status"] = "completed"
        saga["completed_at"] = time.time()

    except Exception as e:
        # Compensation: reverse completed steps
        saga["error"] = str(e)
        saga["status"] = "compensating"

        for step in reversed(saga["steps"]):
            if step["status"] == "completed":
                if step["name"] == "withdraw":
                    # Compensate: re-deposit
                    append_event(from_account, "MoneyDeposited", {
                        "amount": amount, "reason": f"Compensation for {saga_id}"
                    })
                    step["status"] = "compensated"
                elif step["name"] == "deposit":
                    # Compensate: re-withdraw
                    append_event(to_account, "MoneyWithdrawn", {
                        "amount": amount, "reason": f"Compensation for {saga_id}"
                    })
                    step["status"] = "compensated"

        append_event(from_account, "TransferFailed", {"transfer_id": saga_id, "error": str(e)})
        saga["status"] = "compensated"
        saga["completed_at"] = time.time()

    return saga


# ===================================================================
# Command API (Write Side)
# ===================================================================

commands = Blueprint("/api/commands")

@commands.post("/accounts")
def open_account(request):
    """Open a new bank account."""
    data = request.json()
    account_id = f"acc-{uuid.uuid4().hex[:8]}"

    event = append_event(account_id, "AccountOpened", {
        "owner": data["owner"],
        "initial_balance": data.get("initial_balance", 0),
    })

    return Response.json({
        "account_id": account_id,
        "event": event,
    }, status=201)

@commands.post("/accounts/{id}/deposit")
def deposit(request):
    """Deposit money into an account."""
    account_id = request.params["id"]
    data = request.json()
    amount = data["amount"]

    if amount <= 0:
        return Response.json({"error": "Amount must be positive"}, status=400)

    state = rebuild_state(account_id)
    if state.get("status") != "active":
        return Response.json({"error": "Account is not active"}, status=400)

    event = append_event(account_id, "MoneyDeposited", {"amount": amount})
    return {"event": event, "new_balance": state["balance"] + amount}

@commands.post("/accounts/{id}/withdraw")
def withdraw(request):
    """Withdraw money from an account."""
    account_id = request.params["id"]
    data = request.json()
    amount = data["amount"]

    if amount <= 0:
        return Response.json({"error": "Amount must be positive"}, status=400)

    state = rebuild_state(account_id)
    if state.get("status") != "active":
        return Response.json({"error": "Account is not active"}, status=400)
    if state["balance"] < amount:
        return Response.json({"error": "Insufficient funds"}, status=400)

    event = append_event(account_id, "MoneyWithdrawn", {"amount": amount})
    return {"event": event, "new_balance": state["balance"] - amount}

@commands.post("/transfers")
def transfer(request):
    """Transfer money between accounts using a saga."""
    data = request.json()
    saga = start_transfer_saga(
        from_account=data["from_account"],
        to_account=data["to_account"],
        amount=data["amount"],
    )
    status_code = 200 if saga["status"] == "completed" else 409
    return Response.json(saga, status=status_code)

@commands.post("/accounts/{id}/close")
def close_account(request):
    """Close a bank account."""
    account_id = request.params["id"]
    state = rebuild_state(account_id)

    if state.get("status") != "active":
        return Response.json({"error": "Account is not active"}, status=400)
    if state["balance"] != 0:
        return Response.json({"error": "Account balance must be zero to close"}, status=400)

    event = append_event(account_id, "AccountClosed", {})
    return {"event": event}


# ===================================================================
# Query API (Read Side - CQRS)
# ===================================================================

queries = Blueprint("/api/queries")

@queries.get("/accounts")
def list_accounts(request):
    """List all accounts from the read projection."""
    accounts = list(projections["accounts"].values())
    return {"accounts": accounts, "count": len(accounts)}

@queries.get("/accounts/{id}")
def get_account(request):
    """Get account details from the read projection."""
    account = projections["accounts"].get(request.params["id"])
    if not account:
        return Response.json({"error": "Account not found"}, status=404)
    return account

@queries.get("/accounts/{id}/history")
def account_history(request):
    """Get the full event history for an account."""
    account_id = request.params["id"]
    events = event_store.get(account_id, [])
    if not events:
        return Response.json({"error": "Account not found"}, status=404)
    return {
        "account_id": account_id,
        "events": events,
        "version": len(events),
    }

@queries.get("/transactions")
def list_transactions(request):
    """List recent transactions across all accounts."""
    limit = int(request.query.get("limit", "50"))
    account_id = request.query.get("account_id")

    txns = projections["transactions"]
    if account_id:
        txns = [t for t in txns if t["account_id"] == account_id]

    return {"transactions": txns[-limit:], "total": len(txns)}

@queries.get("/sagas")
def list_sagas(request):
    """List all saga executions."""
    return {"sagas": list(sagas.values()), "count": len(sagas)}

@queries.get("/sagas/{id}")
def get_saga(request):
    """Get a saga execution by ID."""
    saga = sagas.get(request.params["id"])
    if not saga:
        return Response.json({"error": "Saga not found"}, status=404)
    return saga


# ===================================================================
# Stats & Health
# ===================================================================

@app.get("/")
def index(request):
    """Service discovery."""
    return {
        "service": "Event Sourcing Bank",
        "commands": "/api/commands",
        "queries": "/api/queries",
        "health": "/health",
    }

@app.get("/health")
def health(request):
    """Health check with event store statistics."""
    total_events = sum(len(evts) for evts in event_store.values())
    return {
        "status": "healthy",
        "aggregates": len(event_store),
        "total_events": total_events,
        "accounts": len(projections["accounts"]),
        "transactions": len(projections["transactions"]),
        "sagas": len(sagas),
    }


# ===================================================================
# Register and Run
# ===================================================================

app.register_blueprint(commands)
app.register_blueprint(queries)

if __name__ == "__main__":
    app.run(host="127.0.0.1", port=8000)
```

---

## Testing with curl

### Open Accounts

```bash
# Open Alice's account with initial balance
curl -X POST http://127.0.0.1:8000/api/commands/accounts \
  -H "Content-Type: application/json" \
  -d '{"owner": "Alice", "initial_balance": 1000}'

# Open Bob's account
curl -X POST http://127.0.0.1:8000/api/commands/accounts \
  -H "Content-Type: application/json" \
  -d '{"owner": "Bob", "initial_balance": 500}'
```

### Deposit and Withdraw

```bash
# Deposit (replace acc-XXXXXXXX with actual account ID)
curl -X POST http://127.0.0.1:8000/api/commands/accounts/acc-XXXXXXXX/deposit \
  -H "Content-Type: application/json" \
  -d '{"amount": 250}'

# Withdraw
curl -X POST http://127.0.0.1:8000/api/commands/accounts/acc-XXXXXXXX/withdraw \
  -H "Content-Type: application/json" \
  -d '{"amount": 100}'
```

### Transfer (Saga)

```bash
curl -X POST http://127.0.0.1:8000/api/commands/transfers \
  -H "Content-Type: application/json" \
  -d '{"from_account": "acc-AAAAAAAA", "to_account": "acc-BBBBBBBB", "amount": 200}'
```

### Query the Read Model

```bash
# List all accounts
curl http://127.0.0.1:8000/api/queries/accounts

# Account event history
curl http://127.0.0.1:8000/api/queries/accounts/acc-XXXXXXXX/history

# Recent transactions
curl http://127.0.0.1:8000/api/queries/transactions?limit=10

# Saga executions
curl http://127.0.0.1:8000/api/queries/sagas
```

---

## Key Patterns

### Append-Only Event Store

Every state change is recorded as an immutable event. The event store never updates or deletes entries. This provides a complete audit trail and enables time-travel debugging by replaying events to any point in history.

### Aggregate State Reconstruction

The `rebuild_state()` function replays all events for an aggregate to compute the current state. In production, use snapshots (via `EventSourcingConfig`) to avoid replaying thousands of events on every request.

### CQRS (Command Query Responsibility Segregation)

The write side (`/api/commands`) validates business rules and appends events. The read side (`/api/queries`) returns pre-computed projections optimized for query patterns. This separation allows each side to scale independently.

### Saga with Compensation

The transfer saga coordinates a multi-step process: withdraw from the source account, then deposit to the destination. If any step fails, previously completed steps are compensated (reversed) automatically. This ensures data consistency without distributed transactions.

### Optimistic Concurrency

The `append_event()` function accepts an `expected_version` parameter. If another request has modified the aggregate since it was last read, a concurrency conflict is raised, preventing lost updates.

---

## Next Steps

- [API Gateway](api-gateway.md) - Add auth and rate limiting
- [Multi-Tenant SaaS](multi-tenant.md) - Tenant isolation patterns
- [Microservices](../advanced/microservices.md) - Deploy services independently
