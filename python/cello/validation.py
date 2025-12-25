import inspect
from functools import wraps
from typing import get_type_hints, Any
from cello._cello import Response

try:
    from pydantic import BaseModel, ValidationError
    HAS_PYDANTIC = True
except ImportError:
    HAS_PYDANTIC = False

def wrap_handler_with_validation(handler):
    """
    Wrap a handler with Pydantic validation if type hints are present.
    """
    if not HAS_PYDANTIC:
        return handler

    try:
        # get_type_hints is more reliable than signature.parameters for resolved types
        type_hints = get_type_hints(handler)
        sig = inspect.signature(handler)
    except Exception:
        # If we can't inspect (e.g. built-in), just return
        return handler

    # Identify Pydantic params
    # mapped to: (param_name, PydanticModel)
    pydantic_params = {}
    
    for name, param in sig.parameters.items():
        if name in type_hints:
            annotation = type_hints[name]
            if isinstance(annotation, type) and issubclass(annotation, BaseModel):
                pydantic_params[name] = annotation

    if not pydantic_params:
        return handler

    @wraps(handler)
    def wrapper(request, *args, **kwargs):
        # request is always first arg in Cello handlers
        
        # Only parse JSON if we have pydantic params to validate against
        json_body = None
        
        # Validate each pydantic param
        errors = []
        for name, model in pydantic_params.items():
            # If param is already in kwargs (e.g. path param or dependency), skip
            # But here we assume Pydantic models come from Body.
            # Compatibility with path params? 
            # If path param is Pydantic model? Cello path params are strings in request.params
            # We assume Pydantic models map to Request Body.
            
            if name in kwargs: 
                continue

            # Parse JSON once
            if json_body is None:
                try:
                    # request.json() is exposed by Rust bindings
                    json_body = request.json()
                except Exception:
                    # Invalid JSON or empty
                    errors.append({"loc": ["body"], "msg": "Invalid JSON body", "type": "value_error.json"})
                    break

            try:
                # model_validate works on dict
                instance = model.model_validate(json_body)
                kwargs[name] = instance
            except ValidationError as e:
                # Add errors
                for err in e.errors():
                    # Prefix location with body/param name?
                    # err["loc"] is tuple.
                    errors.append(err)
            except Exception as e:
                errors.append({"loc": [name], "msg": str(e), "type": "unknown"})

        if errors:
            return Response.json({"detail": errors}, status=422)

        return handler(request, *args, **kwargs)

    return wrapper
