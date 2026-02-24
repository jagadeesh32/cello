import inspect
from functools import wraps
from typing import get_type_hints, Any
from cello._cello import Response

try:
    from pydantic import BaseModel, ValidationError
    HAS_PYDANTIC = True
except ImportError:
    HAS_PYDANTIC = False

def _validate_pydantic_params(pydantic_params, request, kwargs):
    """Shared validation logic for sync and async handlers.

    Returns:
        (kwargs, errors) tuple. If errors is non-empty, return 422 response.
    """
    json_body = None
    errors = []
    for name, model in pydantic_params.items():
        if name in kwargs:
            continue

        # Parse JSON once
        if json_body is None:
            try:
                json_body = request.json()
            except (ValueError, TypeError, UnicodeDecodeError, RuntimeError):
                errors.append({"loc": ["body"], "msg": "Invalid JSON body", "type": "value_error.json"})
                break

        try:
            instance = model.model_validate(json_body)
            kwargs[name] = instance
        except ValidationError as e:
            for err in e.errors():
                errors.append(err)
        except Exception as e:
            errors.append({"loc": [name], "msg": str(e), "type": "unknown"})

    return kwargs, errors


def wrap_handler_with_validation(handler):
    """
    Wrap a handler with Pydantic validation if type hints are present.
    Supports both sync and async handlers.
    """
    if not HAS_PYDANTIC:
        return handler

    try:
        # get_type_hints is more reliable than signature.parameters for resolved types
        type_hints = get_type_hints(handler)
        sig = inspect.signature(handler)
    except (TypeError, ValueError, NameError):
        # If we can't inspect (e.g. built-in or unresolvable type hints), just return
        return handler

    # Identify Pydantic params
    pydantic_params = {}

    for name, param in sig.parameters.items():
        if name in type_hints:
            annotation = type_hints[name]
            if isinstance(annotation, type) and issubclass(annotation, BaseModel):
                pydantic_params[name] = annotation

    if not pydantic_params:
        return handler

    # Create async wrapper for async handlers
    if inspect.iscoroutinefunction(handler):
        @wraps(handler)
        async def async_wrapper(request, *args, **kwargs):
            kwargs, errors = _validate_pydantic_params(pydantic_params, request, kwargs)
            if errors:
                return Response.json({"detail": errors}, status=422)
            return await handler(request, *args, **kwargs)
        return async_wrapper

    # Sync wrapper for sync handlers
    @wraps(handler)
    def wrapper(request, *args, **kwargs):
        kwargs, errors = _validate_pydantic_params(pydantic_params, request, kwargs)
        if errors:
            return Response.json({"detail": errors}, status=422)
        return handler(request, *args, **kwargs)

    return wrapper
