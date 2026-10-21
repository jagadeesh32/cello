"""
Cello middleware configuration classes.

Re-exports config objects and provides Python-level middleware wrappers
that can be passed to ``app.use()``.
"""

from __future__ import annotations
from typing import Callable, Dict, List, Optional, Union

# Re-export config classes that live on the Rust side
from cello._cello import (
    JwtConfig,
    RateLimitConfig,
    SessionConfig,
    SecurityHeadersConfig,
    CSP,
)


class JwtAuth:
    """JWT authentication middleware wrapper.

    Pass to ``app.use()`` to enable JWT auth for the whole application.

    Example::

        from cello import App, JwtConfig
        from cello.middleware import JwtAuth

        app = App()
        jwt_config = JwtConfig(secret="your-secret-key-minimum-32-bytes", algorithm="HS256")
        app.use(JwtAuth(jwt_config, skip_paths=["/health", "/login"]))
    """

    def __init__(
        self,
        config: JwtConfig,
        skip_paths: Optional[List[str]] = None,
    ) -> None:
        self.config = config
        self.skip_paths = skip_paths or []


class BasicAuth:
    """HTTP Basic authentication middleware wrapper.

    Pass to ``app.use()`` to enable Basic auth.

    Args:
        credentials: Dict of ``{username: password}`` pairs.
        realm: WWW-Authenticate realm string (default: ``"Restricted"``).

    Example::

        from cello.middleware import BasicAuth

        app.use(BasicAuth(credentials={"admin": "s3cr3t"}, realm="My API"))
    """

    def __init__(
        self,
        credentials: Dict[str, str],
        realm: str = "Restricted",
    ) -> None:
        self.credentials = credentials
        self.realm = realm


class ApiKeyAuth:
    """API Key authentication middleware wrapper.

    Pass to ``app.use()`` to validate API keys.

    Args:
        keys: Dict of ``{api_key: client_name}`` pairs.
        header: Header name to read the key from (default: ``"X-API-Key"``).

    Example::

        from cello.middleware import ApiKeyAuth

        app.use(ApiKeyAuth(keys={"key-abc": "service-a", "key-xyz": "service-b"}))
    """

    def __init__(
        self,
        keys: Dict[str, str],
        header: str = "X-API-Key",
    ) -> None:
        self.keys = keys
        self.header = header


class CsrfConfig:
    """CSRF protection configuration.

    Pass to ``app.use()`` (or ``app.enable_csrf()``) to enable CSRF middleware.

    The server sets a ``__Host-csrf`` cookie (readable by JavaScript).
    State-changing requests must echo the token in the ``X-CSRF-Token`` header
    or in a ``_csrf`` form field (urlencoded or multipart body; the query string
    is not accepted).

    The ``__Host-`` prefix makes the browser accept the cookie only from this
    exact host over HTTPS (``Secure``, ``Path=/``, no ``Domain``), so a sibling
    subdomain cannot plant a token. With ``cookie_secure=False`` or a path other
    than ``/`` the prefix is dropped (the cookie is then ``csrf``).

    When sessions are enabled, the token is stored in the session as soon as a
    session exists (including the request that creates it, e.g. login), and
    from then on the submitted token must match the session's copy.

    Example::

        from cello.middleware import CsrfConfig

        app.use(CsrfConfig())
    """

    def __init__(
        self,
        cookie_name: str = "__Host-csrf",
        header_name: str = "X-CSRF-Token",
        safe_methods: Optional[List[str]] = None,
        allowed_origins: Optional[List[str]] = None,
    ) -> None:
        self.cookie_name = cookie_name
        self.header_name = header_name
        self.safe_methods = safe_methods or ["GET", "HEAD", "OPTIONS"]
        # Explicit full-origin allow-list for cross-origin CSRF validation. When
        # empty, requests are validated as same-origin against the Host header.
        self.allowed_origins = allowed_origins or []


class AdaptiveRateLimitConfig:
    """Convenience alias for ``RateLimitConfig.adaptive()``.

    Example::

        from cello.middleware import AdaptiveRateLimitConfig

        config = AdaptiveRateLimitConfig(capacity=100, refill_rate=10)
        app.enable_rate_limit(config.to_rate_limit_config())
    """

    def __init__(
        self,
        capacity: int = 100,
        refill_rate: int = 10,
        min_capacity: Optional[int] = None,
        error_threshold: float = 0.10,
    ) -> None:
        self.capacity = capacity
        self.refill_rate = refill_rate
        self.min_capacity = min_capacity
        self.error_threshold = error_threshold

    def to_rate_limit_config(self) -> RateLimitConfig:
        """Convert to ``RateLimitConfig`` for use with ``app.enable_rate_limit()``."""
        return RateLimitConfig.adaptive(
            capacity=self.capacity,
            refill_rate=self.refill_rate,
            min_capacity=self.min_capacity or self.capacity // 2,
            error_threshold=self.error_threshold,
        )


__all__ = [
    "JwtConfig",
    "RateLimitConfig",
    "SessionConfig",
    "SecurityHeadersConfig",
    "CSP",
    "JwtAuth",
    "BasicAuth",
    "ApiKeyAuth",
    "CsrfConfig",
    "AdaptiveRateLimitConfig",
]
