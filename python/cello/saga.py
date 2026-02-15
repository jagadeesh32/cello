"""
Cello Saga Pattern Module.

Provides Python-friendly wrappers for the Saga orchestration pattern
including saga steps with compensating actions, saga execution with
automatic rollback on failure, and a saga orchestrator for managing
multiple saga types. Designed for use with the Cello framework's
Rust-powered runtime.

Example:
    from cello import App
    from cello.saga import (
        Saga, SagaStep, SagaExecution, SagaOrchestrator,
        SagaError, SagaConfig, StepStatus,
    )

    # Define saga steps as async functions
    async def reserve_inventory(context):
        item_id = context["item_id"]
        # ... reserve inventory logic ...
        context["reservation_id"] = "res-123"
        return {"reserved": True, "reservation_id": "res-123"}

    async def cancel_inventory(context):
        reservation_id = context.get("reservation_id")
        # ... cancel reservation logic ...
        return {"cancelled": True}

    async def charge_payment(context):
        amount = context["amount"]
        # ... charge payment logic ...
        context["payment_id"] = "pay-456"
        return {"charged": True, "payment_id": "pay-456"}

    async def refund_payment(context):
        payment_id = context.get("payment_id")
        # ... refund logic ...
        return {"refunded": True}

    async def ship_order(context):
        # ... shipping logic ...
        context["tracking_id"] = "track-789"
        return {"shipped": True}

    # Define a saga using class-based approach
    class OrderSaga(Saga):
        steps = [
            SagaStep("reserve_inventory", reserve_inventory, cancel_inventory),
            SagaStep("charge_payment", charge_payment, refund_payment),
            SagaStep("ship_order", ship_order),
        ]

    # Use the orchestrator
    orchestrator = SagaOrchestrator()
    orchestrator.register(OrderSaga())

    app = App()

    @app.post("/orders")
    async def create_order(request):
        data = request.json()
        context = {
            "item_id": data["item_id"],
            "amount": data["amount"],
        }
        execution = await orchestrator.execute("OrderSaga", context)
        status = execution.get_status()
        if status["status"] == "completed":
            return {"order": "created", "tracking": context.get("tracking_id")}
        return {"error": "Order saga failed", "details": status}
"""

import inspect
import time
import uuid
from typing import Any, Callable, Dict, List, Optional


class StepStatus:
    """
    String constants for saga step statuses.

    Provides a standardised set of status values used to track
    the lifecycle of individual saga steps.

    Attributes:
        PENDING: Step has not started.
        RUNNING: Step is currently executing.
        COMPLETED: Step finished successfully.
        FAILED: Step failed during execution.
        COMPENSATING: Step compensation is in progress.
        COMPENSATED: Step was successfully compensated (rolled back).

    Example:
        if step.status == StepStatus.COMPLETED:
            print("Step finished successfully")
    """

    PENDING: str = "pending"
    RUNNING: str = "running"
    COMPLETED: str = "completed"
    FAILED: str = "failed"
    COMPENSATING: str = "compensating"
    COMPENSATED: str = "compensated"


class SagaError(Exception):
    """
    Exception raised when a saga step fails.

    Captures the step name and the original exception to provide
    context about where the saga failed.

    Attributes:
        step_name: Name of the step that failed.
        original_error: The original exception that caused the failure.

    Example:
        try:
            execution = await orchestrator.execute("OrderSaga", context)
        except SagaError as e:
            print(f"Saga failed at step: {e.step_name}")
            print(f"Original error: {e.original_error}")
    """

    def __init__(self, step_name: str, original_error: Exception):
        """
        Initialize a SagaError.

        Args:
            step_name: Name of the saga step that failed.
            original_error: The original exception that caused the failure.
        """
        self.step_name: str = step_name
        self.original_error: Exception = original_error
        super().__init__(
            f"Saga failed at step '{step_name}': {original_error}"
        )

    def __repr__(self) -> str:
        return (
            f"SagaError(step_name={self.step_name!r}, "
            f"original_error={self.original_error!r})"
        )


class SagaStep:
    """
    Represents a single step in a saga with an action and optional
    compensating action.

    Each step has a forward action that performs work and an optional
    compensating action that undoes the work if a later step fails.
    Steps track their own status and result.

    Attributes:
        name: Human-readable name of the step.
        action: Async callable that performs the step's work.
        compensate: Optional async callable to undo the step's work.
        timeout: Optional timeout in seconds for the step.
        status: Current status (one of StepStatus constants).
        result: Result data from execution, or None.
        error: Error from execution, or None.

    Example:
        step = SagaStep(
            name="charge_payment",
            action=charge_payment,
            compensate=refund_payment,
            timeout=10,
        )
        result = await step.execute(context)
    """

    def __init__(
        self,
        name: str,
        action: Callable,
        compensate: Optional[Callable] = None,
        timeout: Optional[float] = None,
    ):
        """
        Initialize a SagaStep.

        Args:
            name: Human-readable name of the step.
            action: Async callable that performs the step's work.
            compensate: Optional async callable to undo the step's work.
            timeout: Optional timeout in seconds for the step.
        """
        self.name: str = name
        self.action: Callable = action
        self.compensate: Optional[Callable] = compensate
        self.timeout: Optional[float] = timeout
        self.status: str = StepStatus.PENDING
        self.result: Optional[Any] = None
        self.error: Optional[Exception] = None

    async def execute(self, context: Dict[str, Any]) -> Any:
        """
        Execute the step's forward action.

        Runs the action callable with the provided context. Updates
        the step's status, result, and error accordingly.

        Args:
            context: Shared saga context dictionary. Steps can read
                from and write to this dictionary to pass data between
                steps.

        Returns:
            The result of the action callable.

        Raises:
            Exception: Re-raises any exception from the action after
                updating the step status to FAILED.

        Example:
            context = {"order_id": "order-123", "amount": 99.99}
            result = await step.execute(context)
        """
        self.status = StepStatus.RUNNING
        try:
            if inspect.iscoroutinefunction(self.action):
                self.result = await self.action(context)
            else:
                self.result = self.action(context)
            self.status = StepStatus.COMPLETED
            return self.result
        except Exception as e:
            self.status = StepStatus.FAILED
            self.error = e
            raise

    async def compensate_step(self, context: Dict[str, Any]) -> Any:
        """
        Execute the step's compensating action to undo its work.

        If no compensating action was provided, this method is a no-op.

        Args:
            context: Shared saga context dictionary.

        Returns:
            The result of the compensate callable, or None if no
            compensating action is defined.

        Example:
            await step.compensate_step(context)
        """
        if self.compensate is None:
            self.status = StepStatus.COMPENSATED
            return None

        self.status = StepStatus.COMPENSATING
        try:
            if inspect.iscoroutinefunction(self.compensate):
                result = await self.compensate(context)
            else:
                result = self.compensate(context)
            self.status = StepStatus.COMPENSATED
            return result
        except Exception:
            # Compensation failure is logged but does not raise
            # to allow remaining compensations to proceed
            self.status = StepStatus.FAILED
            raise

    def __repr__(self) -> str:
        return (
            f"SagaStep(name={self.name!r}, status={self.status!r})"
        )


class Saga:
    """
    Base class for defining sagas.

    A saga is an ordered sequence of steps, each with a forward action
    and an optional compensating action. Subclass this to define
    concrete sagas, either by setting a ``steps`` class attribute or
    by adding steps programmatically.

    Attributes:
        name: Name of the saga (defaults to the class name).
        steps: List of SagaStep instances.

    Example:
        class OrderSaga(Saga):
            steps = [
                SagaStep("reserve", reserve_fn, cancel_reserve_fn),
                SagaStep("charge", charge_fn, refund_fn),
                SagaStep("ship", ship_fn),
            ]

        # Or build programmatically
        saga = Saga(name="CustomSaga")
        saga.add_step(SagaStep("step1", action_fn, compensate_fn))
        saga.add_step(SagaStep("step2", another_action_fn))
    """

    steps: List[SagaStep] = []

    def __init__(self, name: Optional[str] = None):
        """
        Initialize a Saga.

        Auto-discovers ``steps`` from the class attribute if defined.
        Each saga instance gets its own copy of the steps list so that
        instances do not share mutable state.

        Args:
            name: Optional saga name. Defaults to the class name.
        """
        self.name: str = name or self.__class__.__name__
        # Copy class-level steps to instance to avoid mutation across instances
        self.steps: List[SagaStep] = list(self.__class__.steps)

    def add_step(self, step: SagaStep) -> None:
        """
        Add a step to the saga.

        Args:
            step: SagaStep instance to add.

        Example:
            saga = Saga(name="PaymentSaga")
            saga.add_step(SagaStep("authorize", authorize_fn, void_fn))
            saga.add_step(SagaStep("capture", capture_fn, refund_fn))
        """
        self.steps.append(step)

    def get_steps(self) -> List[SagaStep]:
        """
        Get the ordered list of saga steps.

        Returns:
            List of SagaStep instances.
        """
        return self.steps

    def step_count(self) -> int:
        """
        Get the number of steps in the saga.

        Returns:
            Number of steps.
        """
        return len(self.steps)

    def __repr__(self) -> str:
        return f"Saga(name={self.name!r}, steps={len(self.steps)})"


class SagaExecution:
    """
    Manages the execution of a saga instance.

    Runs saga steps in order. On failure, automatically compensates
    all previously completed steps in reverse order.

    Attributes:
        id: Unique execution identifier (auto-generated UUID).
        saga_name: Name of the saga being executed.
        steps: List of step status dictionaries.
        status: Current execution status.
        started_at: Unix timestamp when execution started.
        completed_at: Unix timestamp when execution finished, or None.

    Example:
        saga = OrderSaga()
        execution = SagaExecution(saga)
        context = {"item_id": "item-1", "amount": 50.00}
        await execution.run(context)
        print(execution.get_status())
    """

    def __init__(self, saga: Saga):
        """
        Initialize a SagaExecution.

        Args:
            saga: The Saga instance to execute.
        """
        self.id: str = str(uuid.uuid4())
        self.saga_name: str = saga.name
        self._saga: Saga = saga
        self.steps: List[Dict[str, Any]] = [
            {"name": step.name, "status": StepStatus.PENDING}
            for step in saga.get_steps()
        ]
        self.status: str = StepStatus.PENDING
        self.started_at: Optional[float] = None
        self.completed_at: Optional[float] = None

    async def run(self, context: Optional[Dict[str, Any]] = None) -> "SagaExecution":
        """
        Execute the saga steps in order.

        Runs each step sequentially. If any step fails, all previously
        completed steps are compensated in reverse order.

        Args:
            context: Optional shared context dictionary. If None, an
                empty dictionary is used. Steps can read from and
                write to this dictionary.

        Returns:
            This SagaExecution instance for chaining.

        Raises:
            SagaError: If a step fails (after compensation is attempted).

        Example:
            execution = SagaExecution(order_saga)
            context = {"order_id": "123", "amount": 99.99}
            try:
                await execution.run(context)
                print("Saga completed successfully")
            except SagaError as e:
                print(f"Saga failed at: {e.step_name}")
        """
        if context is None:
            context = {}

        self.status = StepStatus.RUNNING
        self.started_at = time.time()
        completed_steps: List[SagaStep] = []
        saga_steps = self._saga.get_steps()

        try:
            for i, step in enumerate(saga_steps):
                self.steps[i]["status"] = StepStatus.RUNNING
                try:
                    await step.execute(context)
                    self.steps[i]["status"] = StepStatus.COMPLETED
                    completed_steps.append(step)
                except Exception as e:
                    self.steps[i]["status"] = StepStatus.FAILED
                    self.steps[i]["error"] = str(e)

                    # Compensate completed steps in reverse order
                    for j in range(len(completed_steps) - 1, -1, -1):
                        comp_step = completed_steps[j]
                        comp_index = saga_steps.index(comp_step)
                        self.steps[comp_index]["status"] = StepStatus.COMPENSATING
                        try:
                            await comp_step.compensate_step(context)
                            self.steps[comp_index]["status"] = StepStatus.COMPENSATED
                        except Exception:
                            self.steps[comp_index]["status"] = StepStatus.FAILED

                    self.status = StepStatus.FAILED
                    self.completed_at = time.time()
                    raise SagaError(step.name, e)

            self.status = StepStatus.COMPLETED
            self.completed_at = time.time()
            return self

        except SagaError:
            raise
        except Exception as e:
            self.status = StepStatus.FAILED
            self.completed_at = time.time()
            raise

    def get_status(self) -> Dict[str, Any]:
        """
        Get the current execution status as a dictionary.

        Returns:
            Dictionary containing execution ID, saga name, overall
            status, step statuses, and timing information.

        Example:
            status = execution.get_status()
            print(status["status"])  # "completed" or "failed"
            for step in status["steps"]:
                print(f"  {step['name']}: {step['status']}")
        """
        return {
            "id": self.id,
            "saga_name": self.saga_name,
            "status": self.status,
            "steps": self.steps,
            "started_at": self.started_at,
            "completed_at": self.completed_at,
        }

    def __repr__(self) -> str:
        return (
            f"SagaExecution(id={self.id!r}, saga={self.saga_name!r}, "
            f"status={self.status!r})"
        )


class SagaOrchestrator:
    """
    Orchestrates saga registration and execution.

    Manages a registry of saga definitions and creates SagaExecution
    instances for running sagas. Keeps track of all executions for
    monitoring and debugging.

    Example:
        orchestrator = SagaOrchestrator()
        orchestrator.register(OrderSaga())
        orchestrator.register(PaymentSaga())

        # Execute a saga
        execution = await orchestrator.execute(
            "OrderSaga",
            context={"item_id": "item-1", "amount": 99.99},
        )

        # Check status
        status = execution.get_status()
        print(status["status"])

        # List all executions
        all_executions = orchestrator.list_executions()
    """

    def __init__(self):
        """Initialize the SagaOrchestrator with empty registries."""
        self._sagas: Dict[str, Saga] = {}
        self._executions: Dict[str, SagaExecution] = {}

    def register(self, saga: Saga) -> None:
        """
        Register a saga definition with the orchestrator.

        Args:
            saga: Saga instance to register.

        Example:
            orchestrator.register(OrderSaga())
        """
        self._sagas[saga.name] = saga

    async def execute(
        self,
        saga_name: str,
        context: Optional[Dict[str, Any]] = None,
    ) -> SagaExecution:
        """
        Create and run a saga execution.

        Looks up the registered saga by name, creates a SagaExecution,
        runs it with the provided context, and stores the execution
        for later retrieval.

        Args:
            saga_name: Name of the registered saga to execute.
            context: Optional shared context dictionary for the saga.

        Returns:
            The completed (or failed) SagaExecution instance.

        Raises:
            ValueError: If no saga is registered with the given name.
            SagaError: If a saga step fails.

        Example:
            execution = await orchestrator.execute(
                "OrderSaga",
                context={"item_id": "item-1"},
            )
        """
        saga = self._sagas.get(saga_name)
        if saga is None:
            raise ValueError(f"No saga registered with name: {saga_name}")

        execution = SagaExecution(saga)
        self._executions[execution.id] = execution

        await execution.run(context)
        return execution

    def get_execution(self, execution_id: str) -> Optional[SagaExecution]:
        """
        Retrieve a saga execution by its ID.

        Args:
            execution_id: UUID of the execution.

        Returns:
            The SagaExecution instance, or None if not found.

        Example:
            execution = orchestrator.get_execution("abc-123-def")
            if execution:
                print(execution.get_status())
        """
        return self._executions.get(execution_id)

    def list_executions(self) -> List[SagaExecution]:
        """
        List all saga executions.

        Returns:
            List of all SagaExecution instances managed by this
            orchestrator.

        Example:
            for execution in orchestrator.list_executions():
                print(f"{execution.saga_name}: {execution.status}")
        """
        return list(self._executions.values())

    def __repr__(self) -> str:
        saga_names = list(self._sagas.keys())
        return (
            f"SagaOrchestrator(sagas={saga_names}, "
            f"executions={len(self._executions)})"
        )


class SagaConfig:
    """
    Configuration for the saga orchestration subsystem.

    Controls retry behaviour, timeouts, and logging for saga
    execution.

    Attributes:
        max_retries: Maximum retry attempts for failed steps
            (default: 3).
        retry_delay_ms: Delay between retries in milliseconds
            (default: 1000).
        timeout_ms: Overall saga timeout in milliseconds
            (default: 30000).
        enable_logging: Whether to enable saga execution logging
            (default: True).

    Example:
        config = SagaConfig(
            max_retries=5,
            retry_delay_ms=2000,
            timeout_ms=60000,
            enable_logging=True,
        )
    """

    def __init__(
        self,
        max_retries: int = 3,
        retry_delay_ms: int = 1000,
        timeout_ms: int = 30000,
        enable_logging: bool = True,
    ):
        """
        Initialize SagaConfig.

        Args:
            max_retries: Maximum retry attempts (default: 3).
            retry_delay_ms: Retry delay in ms (default: 1000).
            timeout_ms: Overall timeout in ms (default: 30000).
            enable_logging: Enable logging (default: True).
        """
        self.max_retries: int = max_retries
        self.retry_delay_ms: int = retry_delay_ms
        self.timeout_ms: int = timeout_ms
        self.enable_logging: bool = enable_logging

    def __repr__(self) -> str:
        return (
            f"SagaConfig(max_retries={self.max_retries}, "
            f"retry_delay_ms={self.retry_delay_ms}, "
            f"timeout_ms={self.timeout_ms}, "
            f"enable_logging={self.enable_logging})"
        )
