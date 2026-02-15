"""
Cello Event Sourcing Module.

Provides Python-friendly wrappers for event sourcing patterns including
events, aggregates, snapshots, and an in-memory event store. Designed
for use with the Cello framework's Rust-powered runtime.

Example:
    from cello import App
    from cello.eventsourcing import (
        Event, Aggregate, EventStore, Snapshot,
        EventSourcingConfig, event_handler,
    )

    # Define an aggregate with event handlers
    class OrderAggregate(Aggregate):

        @event_handler("OrderCreated")
        def on_order_created(self, event):
            self.state["status"] = "created"
            self.state["items"] = event.data.get("items", [])
            self.state["total"] = event.data.get("total", 0)

        @event_handler("OrderShipped")
        def on_order_shipped(self, event):
            self.state["status"] = "shipped"
            self.state["shipped_at"] = event.data.get("shipped_at")

    # Usage in application
    app = App()
    config = EventSourcingConfig.memory()

    @app.on_event("startup")
    async def setup():
        app.state.event_store = await EventStore.connect(config)

    @app.post("/orders")
    async def create_order(request):
        data = request.json()
        order = OrderAggregate()

        event = Event(
            event_type="OrderCreated",
            data={"items": data["items"], "total": data["total"]},
            aggregate_id=order.id,
        )
        order.apply(event)

        await app.state.event_store.append(order.id, order.uncommitted_events)
        order.clear_uncommitted()

        return {"order_id": order.id, "status": order.state["status"]}

    @app.get("/orders/{id}")
    async def get_order(request):
        order_id = request.params["id"]
        events = await app.state.event_store.get_events(order_id)
        order = OrderAggregate(aggregate_id=order_id)
        order.load_from_events(events)
        return {"order_id": order.id, "state": order.state}

    @app.on_event("shutdown")
    async def teardown():
        await app.state.event_store.close()
"""

import time
import uuid
from typing import Any, Callable, Dict, List, Optional


def event_handler(event_type: str) -> Callable:
    """
    Decorator to mark a method as a handler for a specific event type.

    When an event with the matching type is applied to an aggregate,
    the decorated method is automatically called to update state.

    Args:
        event_type: The event type string this handler processes.

    Returns:
        Decorator function for the event handler method.

    Example:
        class OrderAggregate(Aggregate):

            @event_handler("OrderCreated")
            def on_order_created(self, event):
                self.state["status"] = "created"
                self.state["items"] = event.data.get("items", [])

            @event_handler("OrderCancelled")
            def on_order_cancelled(self, event):
                self.state["status"] = "cancelled"
                self.state["cancelled_reason"] = event.data.get("reason")
    """
    def decorator(func: Callable) -> Callable:
        func._cello_event_handler = True
        func._cello_event_type = event_type
        return func
    return decorator


class Event:
    """
    Represents a domain event in the event sourcing system.

    Events are immutable records of something that happened in the domain.
    Each event has a unique ID, a type, associated data, and belongs to
    an aggregate identified by aggregate_id.

    Attributes:
        id: Unique event identifier (auto-generated UUID).
        event_type: String identifying the type of event.
        aggregate_id: ID of the aggregate this event belongs to.
        data: Dictionary of event payload data.
        metadata: Optional dictionary of additional metadata.
        version: Event version number (starts at 0).
        timestamp: Unix timestamp when the event was created.

    Example:
        event = Event(
            event_type="OrderCreated",
            data={"items": ["item1", "item2"], "total": 99.99},
            aggregate_id="order-123",
            metadata={"user_id": "user-456"},
        )
        print(event.json())
    """

    def __init__(
        self,
        event_type: str,
        data: Dict[str, Any],
        aggregate_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ):
        """
        Initialize a new Event.

        Args:
            event_type: String identifying the type of event.
            data: Dictionary of event payload data.
            aggregate_id: ID of the aggregate this event belongs to.
            metadata: Optional dictionary of additional metadata.
        """
        self.id: str = str(uuid.uuid4())
        self.event_type: str = event_type
        self.aggregate_id: Optional[str] = aggregate_id
        self.data: Dict[str, Any] = data
        self.metadata: Dict[str, Any] = metadata or {}
        self.version: int = 0
        self.timestamp: float = time.time()

    def json(self) -> Dict[str, Any]:
        """
        Serialize the event to a dictionary.

        Returns:
            Dictionary representation of the event suitable for JSON
            serialization or storage.

        Example:
            event = Event("UserCreated", {"name": "Alice"})
            payload = event.json()
            # {"id": "...", "event_type": "UserCreated", ...}
        """
        return {
            "id": self.id,
            "event_type": self.event_type,
            "aggregate_id": self.aggregate_id,
            "data": self.data,
            "metadata": self.metadata,
            "version": self.version,
            "timestamp": self.timestamp,
        }

    def __repr__(self) -> str:
        return (
            f"Event(id={self.id!r}, event_type={self.event_type!r}, "
            f"aggregate_id={self.aggregate_id!r}, version={self.version})"
        )


class Aggregate:
    """
    Base class for event-sourced aggregates.

    An aggregate is the fundamental building block of event sourcing.
    It maintains state by applying events and tracks uncommitted events
    that need to be persisted.

    Subclasses should define event handler methods decorated with
    @event_handler to process specific event types. Alternatively,
    methods named ``_handle_<event_type>`` are discovered automatically.

    Attributes:
        id: Unique aggregate identifier (auto-generated UUID).
        version: Current version of the aggregate.
        state: Dictionary holding the aggregate's current state.
        uncommitted_events: List of events not yet persisted.

    Example:
        class AccountAggregate(Aggregate):

            @event_handler("AccountOpened")
            def on_account_opened(self, event):
                self.state["balance"] = event.data["initial_balance"]
                self.state["owner"] = event.data["owner"]

            @event_handler("MoneyDeposited")
            def on_money_deposited(self, event):
                self.state["balance"] += event.data["amount"]

            @event_handler("MoneyWithdrawn")
            def on_money_withdrawn(self, event):
                self.state["balance"] -= event.data["amount"]

        account = AccountAggregate()
        event = Event("AccountOpened", {"initial_balance": 1000, "owner": "Alice"})
        account.apply(event)
        print(account.state)  # {"balance": 1000, "owner": "Alice"}
    """

    def __init__(self, aggregate_id: Optional[str] = None):
        """
        Initialize a new Aggregate.

        Args:
            aggregate_id: Optional aggregate ID. If None, a UUID is generated.
        """
        self.id: str = aggregate_id or str(uuid.uuid4())
        self.version: int = 0
        self.state: Dict[str, Any] = {}
        self.uncommitted_events: List[Event] = []

        # Build event handler registry from decorated methods
        self._event_handlers: Dict[str, Callable] = {}
        for attr_name in dir(self):
            try:
                attr = getattr(self, attr_name)
            except AttributeError:
                continue
            if callable(attr) and getattr(attr, "_cello_event_handler", False):
                event_type = getattr(attr, "_cello_event_type", None)
                if event_type:
                    self._event_handlers[event_type] = attr

    def apply(self, event: Event) -> None:
        """
        Apply an event to this aggregate.

        Looks for a handler in the following order:
        1. A method decorated with @event_handler for the event type.
        2. A method named ``_handle_<event_type>`` on the aggregate.

        If a handler is found, it is called with the event. The event
        is then appended to the uncommitted events list, and the
        aggregate version is incremented.

        Args:
            event: The event to apply.

        Example:
            order = OrderAggregate()
            event = Event("OrderCreated", {"total": 42.00})
            order.apply(event)
            assert len(order.uncommitted_events) == 1
        """
        # Set event version and aggregate_id
        event.version = self.version + 1
        if event.aggregate_id is None:
            event.aggregate_id = self.id

        # Look for decorated handler first
        handler = self._event_handlers.get(event.event_type)

        # Fall back to _handle_<event_type> convention
        if handler is None:
            handler_name = f"_handle_{event.event_type}"
            handler = getattr(self, handler_name, None)

        if handler is not None:
            handler(event)

        self.uncommitted_events.append(event)
        self.version = event.version

    def load_from_events(self, events: List[Event]) -> None:
        """
        Rebuild aggregate state by replaying a list of events.

        This method replays each event in order without adding them
        to the uncommitted events list. It is used to reconstitute
        an aggregate from the event store.

        Args:
            events: Ordered list of events to replay.

        Example:
            events = await event_store.get_events("order-123")
            order = OrderAggregate(aggregate_id="order-123")
            order.load_from_events(events)
            print(order.state)
        """
        self.state = {}
        self.version = 0

        for event in events:
            # Look for decorated handler first
            handler = self._event_handlers.get(event.event_type)

            # Fall back to _handle_<event_type> convention
            if handler is None:
                handler_name = f"_handle_{event.event_type}"
                handler = getattr(self, handler_name, None)

            if handler is not None:
                handler(event)

            self.version = event.version

    def clear_uncommitted(self) -> None:
        """
        Clear the list of uncommitted events.

        Call this after successfully persisting events to the event store.

        Example:
            await event_store.append(aggregate.id, aggregate.uncommitted_events)
            aggregate.clear_uncommitted()
        """
        self.uncommitted_events = []

    def get_version(self) -> int:
        """
        Get the current version of the aggregate.

        Returns:
            The current version number.
        """
        return self.version

    def __repr__(self) -> str:
        return (
            f"{self.__class__.__name__}(id={self.id!r}, "
            f"version={self.version}, state_keys={list(self.state.keys())})"
        )


class Snapshot:
    """
    Represents a snapshot of aggregate state at a specific version.

    Snapshots are used to optimize event replay by capturing the full
    state at a point in time. Instead of replaying all events from the
    beginning, the aggregate can be restored from a snapshot and only
    replay events that occurred after the snapshot version.

    Attributes:
        aggregate_id: ID of the aggregate this snapshot belongs to.
        version: Aggregate version at the time of the snapshot.
        state: Dictionary of the aggregate state.
        timestamp: Unix timestamp when the snapshot was created.

    Example:
        snapshot = Snapshot(
            aggregate_id="order-123",
            version=50,
            state={"status": "active", "total": 199.99},
        )
        await event_store.save_snapshot(snapshot)
    """

    def __init__(
        self,
        aggregate_id: str,
        version: int,
        state: Dict[str, Any],
    ):
        """
        Initialize a new Snapshot.

        Args:
            aggregate_id: ID of the aggregate this snapshot belongs to.
            version: Aggregate version at the time of the snapshot.
            state: Dictionary of the aggregate state.
        """
        self.aggregate_id: str = aggregate_id
        self.version: int = version
        self.state: Dict[str, Any] = state
        self.timestamp: float = time.time()

    def __repr__(self) -> str:
        return (
            f"Snapshot(aggregate_id={self.aggregate_id!r}, "
            f"version={self.version}, state_keys={list(self.state.keys())})"
        )


class EventStore:
    """
    Event store for persisting and retrieving domain events.

    Provides an async interface for appending events, retrieving event
    streams, and managing snapshots. The default implementation uses
    in-memory storage suitable for development and testing.

    In production, the Rust runtime provides optimized storage backends
    (PostgreSQL, etc.) that implement the same interface.

    Attributes:
        config: EventSourcingConfig used to create this store.
        connected: Whether the store is currently connected.

    Example:
        config = EventSourcingConfig.memory()
        store = await EventStore.connect(config)

        # Append events
        events = [
            Event("OrderCreated", {"total": 99.99}, aggregate_id="order-1"),
            Event("OrderShipped", {"carrier": "UPS"}, aggregate_id="order-1"),
        ]
        await store.append("order-1", events)

        # Retrieve events
        history = await store.get_events("order-1")
        print(len(history))  # 2

        # Snapshots
        snapshot = Snapshot("order-1", 2, {"status": "shipped"})
        await store.save_snapshot(snapshot)
        loaded = await store.get_snapshot("order-1")

        await store.close()
    """

    def __init__(self, config: Optional["EventSourcingConfig"] = None):
        """
        Initialize the EventStore.

        Prefer using the ``connect`` classmethod for async initialization.

        Args:
            config: Optional EventSourcingConfig. Defaults to in-memory storage.
        """
        self.config: "EventSourcingConfig" = config or EventSourcingConfig()
        self.connected: bool = False

        # Internal dict-based storage for testing/development
        self._events: Dict[str, List[Event]] = {}
        self._snapshots: Dict[str, Snapshot] = {}

    @classmethod
    async def connect(cls, config: Optional["EventSourcingConfig"] = None) -> "EventStore":
        """
        Create and connect an EventStore instance.

        Factory classmethod for async initialization of the event store.

        Args:
            config: Optional EventSourcingConfig. Defaults to in-memory storage.

        Returns:
            A connected EventStore instance ready for use.

        Example:
            config = EventSourcingConfig.memory()
            store = await EventStore.connect(config)
        """
        store = cls(config)
        store.connected = True
        return store

    async def append(self, aggregate_id: str, events: List[Event]) -> None:
        """
        Append events to the event stream for an aggregate.

        Events are added in order and assigned sequential version numbers
        within the aggregate's stream.

        Args:
            aggregate_id: ID of the aggregate owning these events.
            events: List of Event objects to append.

        Raises:
            RuntimeError: If the store is not connected.

        Example:
            event = Event("ItemAdded", {"item": "Widget"}, aggregate_id="cart-1")
            await store.append("cart-1", [event])
        """
        if not self.connected:
            raise RuntimeError("EventStore is not connected. Call connect() first.")

        if aggregate_id not in self._events:
            self._events[aggregate_id] = []

        current_version = len(self._events[aggregate_id])
        for event in events:
            current_version += 1
            event.version = current_version
            event.aggregate_id = aggregate_id
            self._events[aggregate_id].append(event)

        # Auto-snapshot if configured
        if (
            self.config.enable_snapshots
            and self.config.snapshot_interval > 0
            and current_version % self.config.snapshot_interval == 0
        ):
            # Store a marker; the caller is responsible for computing state
            pass

    async def get_events(
        self, aggregate_id: str, since_version: int = 0
    ) -> List[Event]:
        """
        Retrieve events for an aggregate, optionally from a specific version.

        Args:
            aggregate_id: ID of the aggregate to retrieve events for.
            since_version: Only return events after this version (default: 0).

        Returns:
            Ordered list of Event objects.

        Example:
            # Get all events
            events = await store.get_events("order-1")

            # Get events since version 5
            new_events = await store.get_events("order-1", since_version=5)
        """
        if not self.connected:
            raise RuntimeError("EventStore is not connected. Call connect() first.")

        all_events = self._events.get(aggregate_id, [])
        return [e for e in all_events if e.version > since_version]

    async def save_snapshot(self, snapshot: Snapshot) -> None:
        """
        Save a snapshot of aggregate state.

        Only the latest snapshot per aggregate is retained.

        Args:
            snapshot: Snapshot instance to save.

        Example:
            snapshot = Snapshot("order-1", 100, {"status": "completed"})
            await store.save_snapshot(snapshot)
        """
        if not self.connected:
            raise RuntimeError("EventStore is not connected. Call connect() first.")

        self._snapshots[snapshot.aggregate_id] = snapshot

    async def get_snapshot(self, aggregate_id: str) -> Optional[Snapshot]:
        """
        Retrieve the latest snapshot for an aggregate.

        Args:
            aggregate_id: ID of the aggregate.

        Returns:
            The latest Snapshot, or None if no snapshot exists.

        Example:
            snapshot = await store.get_snapshot("order-1")
            if snapshot:
                aggregate.state = snapshot.state
                aggregate.version = snapshot.version
        """
        if not self.connected:
            raise RuntimeError("EventStore is not connected. Call connect() first.")

        return self._snapshots.get(aggregate_id)

    async def close(self) -> None:
        """
        Close the event store connection.

        After calling close, all subsequent operations will raise
        RuntimeError until connect is called again.

        Example:
            await store.close()
        """
        self.connected = False

    def __repr__(self) -> str:
        aggregate_count = len(self._events)
        total_events = sum(len(v) for v in self._events.values())
        return (
            f"EventStore(store_type={self.config.store_type!r}, "
            f"connected={self.connected}, aggregates={aggregate_count}, "
            f"total_events={total_events})"
        )


class EventSourcingConfig:
    """
    Configuration for the event sourcing subsystem.

    Controls the storage backend, snapshot behaviour, and event
    retention settings.

    Attributes:
        store_type: Storage backend type ("memory" or "postgresql").
        snapshot_interval: Number of events between automatic snapshots.
        enable_snapshots: Whether to enable snapshot support.
        max_events: Maximum number of events to retain per aggregate.

    Example:
        # In-memory for development
        config = EventSourcingConfig.memory()

        # PostgreSQL for production
        config = EventSourcingConfig.postgresql("postgresql://user:pass@localhost/events")
    """

    def __init__(
        self,
        store_type: str = "memory",
        snapshot_interval: int = 100,
        enable_snapshots: bool = True,
        max_events: int = 10000,
    ):
        """
        Initialize EventSourcingConfig.

        Args:
            store_type: Storage backend ("memory" or "postgresql").
            snapshot_interval: Events between automatic snapshots (default: 100).
            enable_snapshots: Enable snapshot support (default: True).
            max_events: Maximum events per aggregate (default: 10000).
        """
        self.store_type: str = store_type
        self.snapshot_interval: int = snapshot_interval
        self.enable_snapshots: bool = enable_snapshots
        self.max_events: int = max_events
        self._connection_url: Optional[str] = None

    @classmethod
    def memory(cls) -> "EventSourcingConfig":
        """
        Create an in-memory EventSourcingConfig for development and testing.

        Returns:
            EventSourcingConfig with memory storage backend.

        Example:
            config = EventSourcingConfig.memory()
            store = await EventStore.connect(config)
        """
        return cls(
            store_type="memory",
            snapshot_interval=100,
            enable_snapshots=True,
            max_events=10000,
        )

    @classmethod
    def postgresql(cls, url: str) -> "EventSourcingConfig":
        """
        Create an EventSourcingConfig backed by PostgreSQL.

        Args:
            url: PostgreSQL connection URL.

        Returns:
            EventSourcingConfig with PostgreSQL storage backend.

        Example:
            config = EventSourcingConfig.postgresql(
                "postgresql://user:pass@localhost/events"
            )
        """
        config = cls(
            store_type="postgresql",
            snapshot_interval=100,
            enable_snapshots=True,
            max_events=10000,
        )
        config._connection_url = url
        return config

    def __repr__(self) -> str:
        return (
            f"EventSourcingConfig(store_type={self.store_type!r}, "
            f"snapshot_interval={self.snapshot_interval}, "
            f"enable_snapshots={self.enable_snapshots}, "
            f"max_events={self.max_events})"
        )
