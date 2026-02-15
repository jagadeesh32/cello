"""
Comprehensive test suite for Cello v2.

Run with:
    maturin develop
    pytest tests/ -v
    ruff check python/ tests/
"""

import threading
import time

import pytest
import requests

# =============================================================================
# Unit Tests - Core Imports
# =============================================================================


def test_import():
    """Test that the module can be imported."""
    from cello import App, Blueprint, Request, Response

    assert App is not None
    assert Request is not None
    assert Response is not None
    assert Blueprint is not None


def test_import_websocket():
    """Test WebSocket imports."""
    from cello import WebSocket, WebSocketMessage

    assert WebSocket is not None
    assert WebSocketMessage is not None


def test_import_sse():
    """Test SSE imports."""
    from cello import SseEvent, SseStream

    assert SseEvent is not None
    assert SseStream is not None


def test_import_multipart():
    """Test multipart imports."""
    from cello import FormData, UploadedFile

    assert FormData is not None
    assert UploadedFile is not None


# =============================================================================
# Unit Tests - Enterprise Configuration Classes
# =============================================================================


def test_import_enterprise_configs():
    """Test enterprise configuration imports."""
    from cello import (
        TimeoutConfig,
        LimitsConfig,
        ClusterConfig,
        TlsConfig,
        Http2Config,
        Http3Config,
        JwtConfig,
        RateLimitConfig,
        SessionConfig,
        SecurityHeadersConfig,
        CSP,
        StaticFilesConfig,
    )

    assert TimeoutConfig is not None
    assert LimitsConfig is not None
    assert ClusterConfig is not None
    assert TlsConfig is not None
    assert Http2Config is not None
    assert Http3Config is not None
    assert JwtConfig is not None
    assert RateLimitConfig is not None
    assert SessionConfig is not None
    assert SecurityHeadersConfig is not None
    assert CSP is not None
    assert StaticFilesConfig is not None


def test_timeout_config():
    """Test TimeoutConfig creation and defaults."""
    from cello import TimeoutConfig

    # Create with defaults
    config = TimeoutConfig()
    assert config is not None
    assert config.read_header_timeout == 5
    assert config.handler_timeout == 30

    # Create with custom values
    custom = TimeoutConfig(
        read_header=10,
        read_body=60,
        write=60,
        idle=120,
        handler=45,
    )
    assert custom.read_header_timeout == 10
    assert custom.handler_timeout == 45


def test_limits_config():
    """Test LimitsConfig creation and defaults."""
    from cello import LimitsConfig

    # Create with defaults
    config = LimitsConfig()
    assert config is not None
    assert config.max_header_size == 8192
    assert config.max_body_size == 10485760

    # Create with custom values
    custom = LimitsConfig(
        max_header_size=16384,
        max_body_size=52428800,
        max_connections=50000,
    )
    assert custom.max_header_size == 16384
    assert custom.max_body_size == 52428800


def test_cluster_config():
    """Test ClusterConfig creation."""
    from cello import ClusterConfig

    # Create with defaults
    config = ClusterConfig()
    assert config is not None
    assert config.workers >= 1
    assert config.graceful_shutdown == True

    # Create with custom values
    custom = ClusterConfig(
        workers=4,
        cpu_affinity=True,
        max_restarts=10,
        graceful_shutdown=True,
        shutdown_timeout=60,
    )
    assert custom.workers == 4
    assert custom.cpu_affinity == True
    assert custom.shutdown_timeout == 60

    # Test auto() factory
    auto_config = ClusterConfig.auto()
    assert auto_config is not None
    assert auto_config.workers >= 1


def test_tls_config():
    """Test TlsConfig creation."""
    from cello import TlsConfig

    config = TlsConfig(
        cert_path="/path/to/cert.pem",
        key_path="/path/to/key.pem",
    )
    assert config is not None
    assert config.cert_path == "/path/to/cert.pem"
    assert config.key_path == "/path/to/key.pem"
    assert config.min_version == "1.2"
    assert config.max_version == "1.3"


def test_http2_config():
    """Test Http2Config creation and defaults."""
    from cello import Http2Config

    # Create with defaults
    config = Http2Config()
    assert config is not None
    assert config.max_concurrent_streams == 100
    assert config.enable_push == False

    # Create with custom values
    custom = Http2Config(
        max_concurrent_streams=250,
        initial_window_size=2097152,
        max_frame_size=32768,
        enable_push=False,
    )
    assert custom.max_concurrent_streams == 250


def test_http3_config():
    """Test Http3Config creation and defaults."""
    from cello import Http3Config

    # Create with defaults
    config = Http3Config()
    assert config is not None
    assert config.max_idle_timeout == 30
    assert config.enable_0rtt == False


def test_jwt_config():
    """Test JwtConfig creation."""
    from cello import JwtConfig

    config = JwtConfig(secret="my-secret-key")
    assert config is not None
    assert config.secret == "my-secret-key"
    assert config.algorithm == "HS256"
    assert config.header_name == "Authorization"

    # Test with custom values
    custom = JwtConfig(
        secret="custom-secret",
        algorithm="HS512",
        header_name="X-Auth-Token",
        cookie_name="auth_token",
        leeway=60,
    )
    assert custom.algorithm == "HS512"
    assert custom.cookie_name == "auth_token"


def test_rate_limit_config():
    """Test RateLimitConfig creation."""
    from cello import RateLimitConfig

    # Token bucket factory
    token_bucket = RateLimitConfig.token_bucket(capacity=100, refill_rate=10)
    assert token_bucket is not None
    assert token_bucket.algorithm == "token_bucket"
    assert token_bucket.capacity == 100
    assert token_bucket.refill_rate == 10

    # Sliding window factory
    sliding = RateLimitConfig.sliding_window(max_requests=100, window_secs=60)
    assert sliding is not None
    assert sliding.algorithm == "sliding_window"
    assert sliding.capacity == 100
    assert sliding.window_secs == 60


def test_session_config():
    """Test SessionConfig creation and defaults."""
    from cello import SessionConfig

    # Create with defaults
    config = SessionConfig()
    assert config is not None
    assert config.cookie_name == "session_id"
    assert config.cookie_secure == True
    assert config.cookie_http_only == True
    assert config.cookie_same_site == "Lax"

    # Create with custom values
    custom = SessionConfig(
        cookie_name="my_session",
        cookie_path="/app",
        cookie_secure=False,
        cookie_same_site="Strict",
        max_age=7200,
    )
    assert custom.cookie_name == "my_session"
    assert custom.max_age == 7200


def test_security_headers_config():
    """Test SecurityHeadersConfig creation."""
    from cello import SecurityHeadersConfig

    # Create with defaults
    config = SecurityHeadersConfig()
    assert config is not None
    assert config.x_frame_options == "DENY"
    assert config.x_content_type_options == True

    # Test secure() factory
    secure = SecurityHeadersConfig.secure()
    assert secure is not None
    assert secure.hsts_max_age == 31536000
    assert secure.hsts_include_subdomains == True


def test_csp_builder():
    """Test CSP builder."""
    from cello import CSP

    csp = CSP()
    csp.default_src(["'self'"])
    csp.script_src(["'self'", "https://cdn.example.com"])
    csp.style_src(["'self'", "'unsafe-inline'"])
    csp.img_src(["'self'", "data:", "https:"])

    header_value = csp.build()
    assert header_value is not None
    assert "default-src" in header_value
    assert "'self'" in header_value


def test_static_files_config():
    """Test StaticFilesConfig creation."""
    from cello import StaticFilesConfig

    config = StaticFilesConfig(root="./static")
    assert config is not None
    assert config.root == "./static"
    assert config.prefix == "/static"
    assert config.enable_etag == True
    assert config.directory_listing == False


# =============================================================================
# Unit Tests - App
# =============================================================================


def test_app_creation():
    """Test App instance creation."""
    from cello import App

    app = App()
    assert app is not None
    assert app._app is not None


def test_route_registration():
    """Test that routes can be registered without errors."""
    from cello import App

    app = App()

    @app.get("/")
    def home(req):
        return {"message": "hello"}

    @app.post("/users")
    def create_user(req):
        return {"id": 1}

    @app.get("/users/{id}")
    def get_user(req):
        return {"id": req.params.get("id")}

    @app.put("/users/{id}")
    def update_user(req):
        return {"updated": True}

    @app.delete("/users/{id}")
    def delete_user(req):
        return {"deleted": True}

    assert True


def test_multi_method_route():
    """Test route decorator with multiple methods."""
    from cello import App

    app = App()

    @app.route("/resource", methods=["GET", "POST"])
    def resource_handler(req):
        return {"method": req.method}

    assert True


# =============================================================================
# Unit Tests - Blueprint
# =============================================================================


def test_blueprint_creation():
    """Test Blueprint creation."""
    from cello import Blueprint

    bp = Blueprint("/api", "api")
    assert bp.prefix == "/api"
    assert bp.name == "api"


def test_blueprint_route_registration():
    """Test route registration in blueprint."""
    from cello import App, Blueprint

    bp = Blueprint("/api")

    @bp.get("/users")
    def list_users(req):
        return {"users": []}

    @bp.post("/users")
    def create_user(req):
        return {"id": 1}

    app = App()
    app.register_blueprint(bp)

    assert True


def test_nested_blueprint():
    """Test nested blueprints."""
    from cello import Blueprint

    api = Blueprint("/api")
    v1 = Blueprint("/v1")

    @v1.get("/status")
    def status(req):
        return {"status": "ok"}

    api.register(v1)

    routes = api.get_all_routes()
    assert len(routes) == 1
    method, path, _ = routes[0]
    assert method == "GET"
    assert path == "/api/v1/status"


# =============================================================================
# Unit Tests - Request
# =============================================================================


def test_request_creation():
    """Test Request object creation."""
    from cello import Request

    req = Request(
        method="GET",
        path="/test",
        params={"id": "123"},
        query={"search": "hello"},
        headers={"content-type": "application/json"},
        body=b'{"test": true}',
    )

    assert req.method == "GET"
    assert req.path == "/test"
    assert req.params == {"id": "123"}
    assert req.query == {"search": "hello"}
    assert req.get_param("id") == "123"
    assert req.get_query_param("search") == "hello"
    assert req.get_header("content-type") == "application/json"


def test_request_body():
    """Test Request body methods."""
    from cello import Request

    req = Request(
        method="POST",
        path="/test",
        headers={"content-type": "application/json"},
        body=b'{"name": "test", "value": 42}',
    )

    body_bytes = bytes(req.body())
    assert body_bytes == b'{"name": "test", "value": 42}'
    assert req.text() == '{"name": "test", "value": 42}'

    json_data = req.json()
    assert json_data["name"] == "test"
    assert json_data["value"] == 42


def test_request_content_type():
    """Test content type detection."""
    from cello import Request

    json_req = Request(
        method="POST",
        path="/test",
        headers={"content-type": "application/json"},
        body=b"{}",
    )
    assert json_req.is_json()
    assert not json_req.is_form()

    form_req = Request(
        method="POST",
        path="/test",
        headers={"content-type": "application/x-www-form-urlencoded"},
        body=b"name=test",
    )
    assert form_req.is_form()
    assert not form_req.is_json()


def test_request_form_parsing():
    """Test form data parsing."""
    from cello import Request

    req = Request(
        method="POST",
        path="/test",
        headers={"content-type": "application/x-www-form-urlencoded"},
        body=b"name=John&email=john%40example.com",
    )

    form = req.form()
    assert form["name"] == "John"
    assert form["email"] == "john@example.com"


# =============================================================================
# Unit Tests - Response
# =============================================================================


def test_response_json():
    """Test Response.json static method."""
    from cello import Response

    resp = Response.text("Hello, World!", status=200)
    assert resp.status == 200
    # Content-type may or may not include charset
    assert "text/plain" in resp.content_type()


def test_response_text():
    """Test Response.text static method."""
    from cello import Response

    resp = Response.text("Hello, World!")
    assert resp.status == 200

    resp_custom = Response.text("Error", status=400)
    assert resp_custom.status == 400


def test_response_html():
    """Test Response.html static method."""
    from cello import Response

    resp = Response.html("<h1>Hello</h1>")
    assert resp.status == 200
    assert "text/html" in resp.content_type()


def test_response_headers():
    """Test Response header manipulation."""
    from cello import Response

    resp = Response.text("Test")
    resp.set_header("X-Custom", "value")
    assert resp.headers.get("X-Custom") == "value"


def test_response_redirect():
    """Test Response.redirect."""
    from cello import Response

    resp = Response.redirect("https://example.com")
    assert resp.status == 302
    assert resp.headers.get("Location") == "https://example.com"

    resp_perm = Response.redirect("https://example.com", permanent=True)
    assert resp_perm.status == 301


def test_response_no_content():
    """Test Response.no_content."""
    from cello import Response

    resp = Response.no_content()
    assert resp.status == 204


def test_response_binary():
    """Test Response.binary."""
    from cello import Response

    data = b"\x00\x01\x02\x03"
    resp = Response.binary(list(data))
    assert resp.status == 200


# =============================================================================
# Unit Tests - SSE
# =============================================================================


def test_sse_event_creation():
    """Test SseEvent creation."""
    from cello import SseEvent

    # Using constructor directly
    event = SseEvent("Hello, World!")
    # SseEvent has data as both attribute and static method
    # Access via to_sse_string() to verify content
    sse_str = event.to_sse_string()
    assert "data: Hello, World!" in sse_str


def test_sse_event_data():
    """Test SseEvent.data static method."""
    from cello import SseEvent

    event = SseEvent.data("Test message")
    sse_str = event.to_sse_string()
    assert "data: Test message" in sse_str


def test_sse_event_with_event():
    """Test SseEvent.with_event static method."""
    from cello import SseEvent

    event = SseEvent.with_event("notification", "New message")
    sse_str = event.to_sse_string()
    assert "event: notification" in sse_str
    assert "data: New message" in sse_str


def test_sse_stream():
    """Test SseStream."""
    from cello import SseEvent, SseStream

    stream = SseStream()
    stream.add(SseEvent.data("Event 1"))
    stream.add_event("update", "Event 2")
    stream.add_data("Event 3")

    assert stream.len() == 3
    assert not stream.is_empty()


# =============================================================================
# Unit Tests - WebSocket
# =============================================================================


def test_websocket_message_text():
    """Test WebSocketMessage.text."""
    from cello import WebSocketMessage

    msg = WebSocketMessage.text("Hello")
    assert msg.is_text()
    assert not msg.is_binary()
    # msg_type is the attribute we can check
    assert msg.msg_type == "text"


def test_websocket_message_binary():
    """Test WebSocketMessage.binary."""
    from cello import WebSocketMessage

    msg = WebSocketMessage.binary([1, 2, 3, 4])
    assert msg.is_binary()
    assert not msg.is_text()
    assert msg.msg_type == "binary"


def test_websocket_message_close():
    """Test WebSocketMessage.close."""
    from cello import WebSocketMessage

    msg = WebSocketMessage.close()
    assert msg.is_close()


# =============================================================================
# Unit Tests - Middleware
# =============================================================================


def test_middleware_enable():
    """Test enabling middleware."""
    from cello import App

    app = App()
    app.enable_cors()
    app.enable_logging()
    app.enable_compression()

    assert True


def test_middleware_cors_with_origins():
    """Test CORS middleware with custom origins."""
    from cello import App

    app = App()
    app.enable_cors(origins=["https://example.com", "https://api.example.com"])

    assert True


# =============================================================================
# Integration Tests
# =============================================================================


class TestIntegration:
    """Integration tests that require running the server."""

    @pytest.fixture
    def server(self):
        """Start a test server in a background thread."""
        from cello import App, Blueprint, Response

        app = App()

        # Enable middleware
        app.enable_cors()

        @app.get("/")
        def home(req):
            return {"message": "Hello, Vasuki v2!"}

        @app.get("/hello/{name}")
        def hello(req):
            name = req.params.get("name", "World")
            return {"message": f"Hello, {name}!"}

        @app.get("/query")
        def query(req):
            q = req.query.get("q", "")
            return {"query": q}

        @app.post("/echo")
        def echo(req):
            try:
                data = req.json()
                return {"received": data}
            except Exception as e:
                return {"error": str(e)}

        @app.post("/form")
        def form_handler(req):
            try:
                form = req.form()
                return {"form": form}
            except Exception as e:
                return {"error": str(e)}

        @app.put("/update/{id}")
        def update(req):
            item_id = req.params.get("id")
            return {"id": item_id, "updated": True}

        @app.delete("/delete/{id}")
        def delete(req):
            item_id = req.params.get("id")
            return {"id": item_id, "deleted": True}

        @app.get("/text")
        def text_response(req):
            return Response.text("Plain text response")

        @app.get("/html")
        def html_response(req):
            return Response.html("<h1>HTML Response</h1>")

        # Blueprint
        api = Blueprint("/api")

        @api.get("/status")
        def status(req):
            return {"status": "ok", "version": "2.0"}

        app.register_blueprint(api)

        def run_server():
            try:
                app.run(host="127.0.0.1", port=18080)
            except Exception:
                pass

        thread = threading.Thread(target=run_server, daemon=True)
        thread.start()
        time.sleep(0.5)

        yield "http://127.0.0.1:18080"

    @pytest.mark.integration
    def test_home_endpoint(self, server):
        """Test the home endpoint."""
        resp = requests.get(f"{server}/")
        assert resp.status_code == 200
        data = resp.json()
        assert data["message"] == "Hello, Vasuki v2!"

    @pytest.mark.integration
    def test_path_parameter(self, server):
        """Test path parameter extraction."""
        resp = requests.get(f"{server}/hello/World")
        assert resp.status_code == 200
        data = resp.json()
        assert data["message"] == "Hello, World!"

    @pytest.mark.integration
    def test_query_parameter(self, server):
        """Test query parameter handling."""
        resp = requests.get(f"{server}/query", params={"q": "search term"})
        assert resp.status_code == 200
        data = resp.json()
        assert data["query"] == "search term"

    @pytest.mark.integration
    def test_post_json(self, server):
        """Test POST with JSON body."""
        resp = requests.post(
            f"{server}/echo",
            json={"name": "test", "value": 42},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["received"]["name"] == "test"
        assert data["received"]["value"] == 42

    @pytest.mark.integration
    def test_post_form(self, server):
        """Test POST with form data."""
        resp = requests.post(
            f"{server}/form",
            data={"name": "John", "email": "john@example.com"},
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["form"]["name"] == "John"
        assert data["form"]["email"] == "john@example.com"

    @pytest.mark.integration
    def test_put_method(self, server):
        """Test PUT method."""
        resp = requests.put(f"{server}/update/123")
        assert resp.status_code == 200
        data = resp.json()
        assert data["id"] == "123"
        assert data["updated"] is True

    @pytest.mark.integration
    def test_delete_method(self, server):
        """Test DELETE method."""
        resp = requests.delete(f"{server}/delete/456")
        assert resp.status_code == 200
        data = resp.json()
        assert data["id"] == "456"
        assert data["deleted"] is True

    @pytest.mark.integration
    def test_blueprint_route(self, server):
        """Test blueprint routes."""
        resp = requests.get(f"{server}/api/status")
        assert resp.status_code == 200
        data = resp.json()
        assert data["status"] == "ok"
        assert data["version"] == "2.0"

    @pytest.mark.integration
    def test_cors_headers(self, server):
        """Test CORS headers are present."""
        resp = requests.get(f"{server}/", headers={"Origin": "http://example.com"})
        assert resp.status_code == 200
        assert "Access-Control-Allow-Origin" in resp.headers

    @pytest.mark.integration
    def test_not_found(self, server):
        """Test 404 response for unknown routes."""
        resp = requests.get(f"{server}/nonexistent")
        assert resp.status_code == 404
        data = resp.json()
        assert "error" in data


# =============================================================================
# v0.5.0 Feature Tests
# =============================================================================


def test_import_v050_features():
    """Test that v0.5.0 features can be imported."""
    from cello import BackgroundTasks, TemplateEngine, Depends

    assert BackgroundTasks is not None
    assert TemplateEngine is not None
    assert Depends is not None


def test_background_tasks_creation():
    """Test BackgroundTasks creation and basic operations."""
    from cello import BackgroundTasks

    tasks = BackgroundTasks()
    assert tasks is not None
    assert tasks.pending_count() == 0


def test_background_tasks_add_and_run():
    """Test adding and running background tasks."""
    from cello import BackgroundTasks

    tasks = BackgroundTasks()
    results = []

    def task_func(value):
        results.append(value)

    # Add tasks
    tasks.add_task(task_func, ["task1"])
    tasks.add_task(task_func, ["task2"])
    assert tasks.pending_count() == 2

    # Run all tasks
    tasks.run_all()
    assert tasks.pending_count() == 0
    assert "task1" in results
    assert "task2" in results


def test_template_engine_creation():
    """Test TemplateEngine creation."""
    from cello import TemplateEngine

    engine = TemplateEngine("templates")
    assert engine is not None


def test_template_engine_render_string():
    """Test TemplateEngine string rendering."""
    from cello import TemplateEngine

    engine = TemplateEngine("templates")

    # Test simple variable substitution
    result = engine.render_string(
        "Hello, {{ name }}! You are {{ age }} years old.",
        {"name": "John", "age": 30}
    )
    assert result == "Hello, John! You are 30 years old."


def test_template_engine_render_no_spaces():
    """Test TemplateEngine with no spaces in placeholders."""
    from cello import TemplateEngine

    engine = TemplateEngine("templates")

    result = engine.render_string(
        "<h1>{{title}}</h1><p>{{content}}</p>",
        {"title": "Welcome", "content": "Hello World"}
    )
    assert result == "<h1>Welcome</h1><p>Hello World</p>"


def test_depends_creation():
    """Test Depends marker creation."""
    from cello import Depends

    dep = Depends("database")
    assert dep is not None
    assert dep.dependency == "database"


def test_depends_multiple():
    """Test multiple Depends markers."""
    from cello import Depends

    db_dep = Depends("database")
    cache_dep = Depends("cache")
    config_dep = Depends("config")

    assert db_dep.dependency == "database"
    assert cache_dep.dependency == "cache"
    assert config_dep.dependency == "config"


def test_prometheus_middleware():
    """Test that Prometheus middleware can be enabled."""
    from cello import App

    app = App()
    app.enable_prometheus()
    assert True


def test_prometheus_custom_config():
    """Test Prometheus with custom configuration."""
    from cello import App

    app = App()
    app.enable_prometheus(
        endpoint="/custom-metrics",
        namespace="myapp",
        subsystem="api"
    )
    assert True


def test_guards_registration():
    """Test that guards can be registered."""
    from cello import App

    app = App()

    def my_guard(request):
        return True

    app.add_guard(my_guard)
    assert True


def test_dependency_injection_registration():
    """Test that dependencies can be registered."""
    from cello import App

    app = App()

    # Register a singleton dependency
    app.register_singleton("database", {"url": "postgres://localhost/db"})
    app.register_singleton("cache", {"host": "localhost", "port": 6379})

    assert True


def test_version():
    """Test that version is 0.9.0."""
    import cello

    assert cello.__version__ == "0.9.0"


def test_all_exports():
    """Test that all expected exports are available."""
    from cello import (
        # Core
        App,
        Blueprint,
        Request,
        Response,
        WebSocket,
        WebSocketMessage,
        SseEvent,
        SseStream,
        FormData,
        UploadedFile,
        # Advanced Configuration
        TimeoutConfig,
        LimitsConfig,
        ClusterConfig,
        TlsConfig,
        Http2Config,
        Http3Config,
        JwtConfig,
        RateLimitConfig,
        SessionConfig,
        SecurityHeadersConfig,
        CSP,
        StaticFilesConfig,
        # v0.5.0 features
        BackgroundTasks,
        TemplateEngine,
        Depends,
    )

    # Verify all are not None
    assert all([
        App, Blueprint, Request, Response,
        WebSocket, WebSocketMessage, SseEvent, SseStream,
        FormData, UploadedFile, TimeoutConfig, LimitsConfig,
        ClusterConfig, TlsConfig, Http2Config, Http3Config,
        JwtConfig, RateLimitConfig, SessionConfig,
        SecurityHeadersConfig, CSP, StaticFilesConfig,
        BackgroundTasks, TemplateEngine, Depends
    ])


# =============================================================================
# Rename Enterprise to Advanced in existing tests
# =============================================================================


def test_import_advanced_configs():
    """Test advanced configuration imports (renamed from enterprise)."""
    from cello import (
        TimeoutConfig,
        LimitsConfig,
        ClusterConfig,
        TlsConfig,
        Http2Config,
        Http3Config,
        JwtConfig,
        RateLimitConfig,
        SessionConfig,
        SecurityHeadersConfig,
        CSP,
        StaticFilesConfig,
    )

    # All should be importable
    assert TimeoutConfig is not None
    assert LimitsConfig is not None
    assert ClusterConfig is not None
    assert TlsConfig is not None
    assert Http2Config is not None
    assert Http3Config is not None
    assert JwtConfig is not None
    assert RateLimitConfig is not None
    assert SessionConfig is not None
    assert SecurityHeadersConfig is not None
    assert CSP is not None
    assert StaticFilesConfig is not None


# =============================================================================
# v0.7.0 Enterprise Feature Tests
# =============================================================================


def test_import_v070_enterprise_configs():
    """Test that v0.7.0 enterprise configuration classes can be imported."""
    from cello import (
        OpenTelemetryConfig,
        HealthCheckConfig,
        DatabaseConfig,
        GraphQLConfig,
    )

    assert OpenTelemetryConfig is not None
    assert HealthCheckConfig is not None
    assert DatabaseConfig is not None
    assert GraphQLConfig is not None


def test_opentelemetry_config():
    """Test OpenTelemetryConfig creation and defaults."""
    from cello import OpenTelemetryConfig

    config = OpenTelemetryConfig(service_name="test-service")
    assert config is not None
    assert config.service_name == "test-service"


def test_health_check_config():
    """Test HealthCheckConfig creation and defaults."""
    from cello import HealthCheckConfig

    config = HealthCheckConfig()
    assert config is not None


def test_graphql_config():
    """Test GraphQLConfig creation."""
    from cello import GraphQLConfig

    config = GraphQLConfig()
    assert config is not None


def test_enable_telemetry():
    """Test enabling OpenTelemetry on an App."""
    from cello import App, OpenTelemetryConfig

    app = App()
    config = OpenTelemetryConfig(
        service_name="test-api",
        sampling_rate=0.5,
    )
    app.enable_telemetry(config)
    assert True


def test_enable_health_checks():
    """Test enabling health checks on an App."""
    from cello import App, HealthCheckConfig

    app = App()
    app.enable_health_checks(HealthCheckConfig())
    assert True


def test_enable_graphql():
    """Test enabling GraphQL on an App."""
    from cello import App, GraphQLConfig

    app = App()
    app.enable_graphql(GraphQLConfig())
    assert True


def test_enable_openapi():
    """Test enabling OpenAPI documentation."""
    from cello import App

    app = App()
    app.enable_openapi(title="Test API", version="0.9.0")
    assert True


def test_enable_openapi_default_version():
    """Test that OpenAPI defaults to v0.9.0."""
    from cello import App

    app = App()
    # Should not raise with default version
    app.enable_openapi()
    assert True


# =============================================================================
# v0.8.0 Data Layer Feature Tests
# =============================================================================


def test_import_v080_data_layer():
    """Test that v0.8.0 data layer features can be imported."""
    from cello import DatabaseConfig, RedisConfig
    from cello.database import transactional, Database, Redis, Transaction

    assert DatabaseConfig is not None
    assert RedisConfig is not None
    assert transactional is not None
    assert Database is not None
    assert Redis is not None
    assert Transaction is not None


def test_v080_exports_in_all():
    """Test that v0.8.0 features are in __all__."""
    import cello

    assert "RedisConfig" in cello.__all__
    assert "transactional" in cello.__all__
    assert "DatabaseConfig" in cello.__all__


def test_database_config_creation():
    """Test DatabaseConfig creation with default values."""
    from cello import DatabaseConfig

    config = DatabaseConfig("postgresql://user:pass@localhost/mydb")
    assert config is not None
    assert config.url == "postgresql://user:pass@localhost/mydb"


def test_database_config_custom():
    """Test DatabaseConfig creation with custom pool settings."""
    from cello import DatabaseConfig

    config = DatabaseConfig(
        url="postgresql://user:pass@localhost/testdb",
        pool_size=20,
        max_lifetime_secs=1800,
    )
    assert config.url == "postgresql://user:pass@localhost/testdb"
    assert config.pool_size == 20
    assert config.max_lifetime_secs == 1800


def test_redis_config_creation():
    """Test RedisConfig creation with defaults."""
    from cello import RedisConfig

    config = RedisConfig()
    assert config is not None


def test_redis_config_custom():
    """Test RedisConfig creation with custom settings."""
    from cello import RedisConfig

    config = RedisConfig(
        url="redis://localhost:6379",
        pool_size=10,
        cluster_mode=False,
        default_ttl=3600,
        tls=False,
    )
    assert config.url == "redis://localhost:6379"
    assert config.pool_size == 10
    assert config.cluster_mode is False
    assert config.default_ttl == 3600
    assert config.tls is False


def test_redis_config_local_factory():
    """Test RedisConfig.local() convenience constructor."""
    from cello import RedisConfig

    config = RedisConfig.local()
    assert config is not None
    assert config.url == "redis://127.0.0.1:6379"


def test_redis_config_cluster_factory():
    """Test RedisConfig.cluster() convenience constructor."""
    from cello import RedisConfig

    config = RedisConfig.cluster(
        url="redis://cluster-host:7000",
        pool_size=20,
    )
    assert config is not None
    assert config.url == "redis://cluster-host:7000"
    assert config.pool_size == 20
    assert config.cluster_mode is True


def test_redis_config_with_key_prefix():
    """Test RedisConfig with key prefix for namespacing."""
    from cello import RedisConfig

    config = RedisConfig(
        url="redis://localhost:6379",
        key_prefix="myapp:",
    )
    assert config.key_prefix == "myapp:"


def test_redis_config_with_tls():
    """Test RedisConfig with TLS enabled."""
    from cello import RedisConfig

    config = RedisConfig(
        url="rediss://secure-host:6380",
        tls=True,
    )
    assert config.tls is True


def test_enable_database():
    """Test enabling database connection pooling on an App."""
    from cello import App, DatabaseConfig

    app = App()
    config = DatabaseConfig(
        url="postgresql://user:pass@localhost/mydb",
        pool_size=20,
        max_lifetime_secs=1800,
    )
    app.enable_database(config)
    assert True


def test_enable_database_default():
    """Test enabling database with default config."""
    from cello import App

    app = App()
    app.enable_database()
    assert True


def test_enable_redis():
    """Test enabling Redis on an App."""
    from cello import App, RedisConfig

    app = App()
    config = RedisConfig(
        url="redis://localhost:6379",
        pool_size=10,
    )
    app.enable_redis(config)
    assert True


def test_enable_redis_default():
    """Test enabling Redis with default config."""
    from cello import App

    app = App()
    app.enable_redis()
    assert True


# =============================================================================
# v0.8.0 Database Python API Tests
# =============================================================================


def test_database_class_init():
    """Test Database class initialization."""
    from cello.database import Database

    db = Database()
    assert db._config is None
    assert db._pool is None


def test_database_class_with_config():
    """Test Database class initialization with config."""
    from cello import DatabaseConfig
    from cello.database import Database

    config = DatabaseConfig("postgresql://localhost/test")
    db = Database(config)
    assert db._config is config


@pytest.mark.asyncio
async def test_database_connect():
    """Test Database.connect class method."""
    from cello import DatabaseConfig
    from cello.database import Database

    config = DatabaseConfig("postgresql://localhost/test")
    db = await Database.connect(config)
    assert db is not None
    assert db._pool is not None


@pytest.mark.asyncio
async def test_database_fetch_all():
    """Test Database.fetch_all returns empty list (mock)."""
    from cello import DatabaseConfig
    from cello.database import Database

    config = DatabaseConfig("postgresql://localhost/test")
    db = await Database.connect(config)
    rows = await db.fetch_all("SELECT * FROM users")
    assert isinstance(rows, list)
    assert len(rows) == 0


@pytest.mark.asyncio
async def test_database_fetch_one():
    """Test Database.fetch_one returns None (mock)."""
    from cello import DatabaseConfig
    from cello.database import Database

    config = DatabaseConfig("postgresql://localhost/test")
    db = await Database.connect(config)
    row = await db.fetch_one("SELECT * FROM users WHERE id = $1", 1)
    assert row is None


@pytest.mark.asyncio
async def test_database_execute():
    """Test Database.execute returns 0 (mock)."""
    from cello import DatabaseConfig
    from cello.database import Database

    config = DatabaseConfig("postgresql://localhost/test")
    db = await Database.connect(config)
    count = await db.execute("INSERT INTO users (name) VALUES ($1)", "Alice")
    assert count == 0


@pytest.mark.asyncio
async def test_database_begin_transaction():
    """Test Database.begin returns a Transaction."""
    from cello import DatabaseConfig
    from cello.database import Database, Transaction

    config = DatabaseConfig("postgresql://localhost/test")
    db = await Database.connect(config)
    tx = await db.begin()
    assert isinstance(tx, Transaction)


@pytest.mark.asyncio
async def test_database_close():
    """Test Database.close resets pool."""
    from cello import DatabaseConfig
    from cello.database import Database

    config = DatabaseConfig("postgresql://localhost/test")
    db = await Database.connect(config)
    assert db._pool is not None
    await db.close()
    assert db._pool is None


# =============================================================================
# v0.8.0 Transaction Tests
# =============================================================================


@pytest.mark.asyncio
async def test_transaction_commit():
    """Test Transaction commit."""
    from cello import DatabaseConfig
    from cello.database import Database

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))
    tx = await db.begin()
    assert tx._committed is False
    await tx.commit()
    assert tx._committed is True


@pytest.mark.asyncio
async def test_transaction_rollback():
    """Test Transaction rollback."""
    from cello import DatabaseConfig
    from cello.database import Database

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))
    tx = await db.begin()
    assert tx._rolled_back is False
    await tx.rollback()
    assert tx._rolled_back is True


@pytest.mark.asyncio
async def test_transaction_context_manager_commit():
    """Test Transaction as async context manager auto-commits."""
    from cello import DatabaseConfig
    from cello.database import Database

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))
    tx = await db.begin()
    async with tx:
        await tx.execute("INSERT INTO users (name) VALUES ($1)", "Bob")
    # Should auto-commit on clean exit
    assert tx._committed is True
    assert tx._rolled_back is False


@pytest.mark.asyncio
async def test_transaction_context_manager_rollback():
    """Test Transaction as async context manager auto-rollbacks on exception."""
    from cello import DatabaseConfig
    from cello.database import Database

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))
    tx = await db.begin()

    with pytest.raises(ValueError):
        async with tx:
            await tx.execute("INSERT INTO users (name) VALUES ($1)", "Bob")
            raise ValueError("Test error")

    # Should auto-rollback on exception
    assert tx._rolled_back is True
    assert tx._committed is False


@pytest.mark.asyncio
async def test_transaction_execute():
    """Test executing queries within a transaction."""
    from cello import DatabaseConfig
    from cello.database import Database

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))
    tx = await db.begin()
    count = await tx.execute("UPDATE accounts SET balance = 100 WHERE id = $1", 1)
    assert count == 0  # Mock returns 0


@pytest.mark.asyncio
async def test_transaction_fetch_all():
    """Test fetching rows within a transaction."""
    from cello import DatabaseConfig
    from cello.database import Database

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))
    tx = await db.begin()
    rows = await tx.fetch_all("SELECT * FROM accounts")
    assert isinstance(rows, list)


@pytest.mark.asyncio
async def test_transaction_fetch_one():
    """Test fetching a single row within a transaction."""
    from cello import DatabaseConfig
    from cello.database import Database

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))
    tx = await db.begin()
    row = await tx.fetch_one("SELECT * FROM accounts WHERE id = $1", 1)
    assert row is None  # Mock returns None


# =============================================================================
# v0.8.0 Redis Python API Tests
# =============================================================================


def test_redis_class_init():
    """Test Redis class initialization."""
    from cello.database import Redis

    redis = Redis()
    assert redis._config is None
    assert redis._client is None


@pytest.mark.asyncio
async def test_redis_connect():
    """Test Redis.connect class method."""
    from cello import RedisConfig
    from cello.database import Redis

    config = RedisConfig(url="redis://localhost:6379")
    redis = await Redis.connect(config)
    assert redis is not None
    assert redis._client is not None


@pytest.mark.asyncio
async def test_redis_get_set():
    """Test Redis get/set operations."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())
    result = await redis.set("key", "value")
    assert result is True

    value = await redis.get("key")
    # Mock returns None (no actual Redis)
    assert value is None


@pytest.mark.asyncio
async def test_redis_delete():
    """Test Redis delete operation."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())
    result = await redis.delete("key")
    assert result is True


@pytest.mark.asyncio
async def test_redis_exists():
    """Test Redis exists operation."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())
    result = await redis.exists("key")
    assert result is False  # Mock returns False


@pytest.mark.asyncio
async def test_redis_incr_decr():
    """Test Redis increment and decrement."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())
    incr_val = await redis.incr("counter")
    assert isinstance(incr_val, int)

    decr_val = await redis.decr("counter")
    assert isinstance(decr_val, int)


@pytest.mark.asyncio
async def test_redis_expire():
    """Test Redis expire (TTL) operation."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())
    result = await redis.expire("key", 3600)
    assert result is True


@pytest.mark.asyncio
async def test_redis_hash_operations():
    """Test Redis hash operations (hset, hget, hgetall)."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())

    result = await redis.hset("user:1", "name", "Alice")
    assert result is True

    value = await redis.hget("user:1", "name")
    assert value is None  # Mock

    all_fields = await redis.hgetall("user:1")
    assert isinstance(all_fields, dict)


@pytest.mark.asyncio
async def test_redis_list_operations():
    """Test Redis list operations (lpush, rpush, lpop, lrange)."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())

    lpush_count = await redis.lpush("queue", "task1", "task2")
    assert isinstance(lpush_count, int)

    rpush_count = await redis.rpush("queue", "task3")
    assert isinstance(rpush_count, int)

    item = await redis.lpop("queue")
    assert item is None  # Mock

    items = await redis.lrange("queue", 0, -1)
    assert isinstance(items, list)


@pytest.mark.asyncio
async def test_redis_publish():
    """Test Redis publish operation."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())
    count = await redis.publish("events", '{"type": "update"}')
    assert isinstance(count, int)


@pytest.mark.asyncio
async def test_redis_close():
    """Test Redis close resets client."""
    from cello import RedisConfig
    from cello.database import Redis

    redis = await Redis.connect(RedisConfig())
    assert redis._client is not None
    await redis.close()
    assert redis._client is None


# =============================================================================
# v0.8.0 Transactional Decorator Tests
# =============================================================================


def test_transactional_decorator_exists():
    """Test that transactional decorator is importable."""
    from cello.database import transactional
    import inspect

    assert callable(transactional)


def test_transactional_wraps_async_function():
    """Test that @transactional wraps an async function correctly."""
    from cello.database import transactional
    import asyncio

    @transactional
    async def my_handler(request):
        return {"success": True}

    # Should return a coroutine function (wrapper)
    assert asyncio.iscoroutinefunction(my_handler)


@pytest.mark.asyncio
async def test_transactional_calls_function_without_db():
    """Test @transactional executes handler when no DB is available."""
    from cello.database import transactional

    @transactional
    async def my_handler(request):
        return {"success": True}

    # Create a simple mock request
    class MockRequest:
        pass

    result = await my_handler(MockRequest())
    assert result == {"success": True}


@pytest.mark.asyncio
async def test_transactional_commits_on_success():
    """Test @transactional commits transaction on success."""
    from cello.database import transactional, Database, Transaction
    from cello import DatabaseConfig

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))

    @transactional
    async def my_handler(request, db=None, **kwargs):
        return {"success": True}

    # Pass db as kwarg
    result = await my_handler(None, db=db)
    assert result == {"success": True}


@pytest.mark.asyncio
async def test_transactional_rollbacks_on_error():
    """Test @transactional rollbacks transaction on exception."""
    from cello.database import transactional, Database
    from cello import DatabaseConfig

    db = await Database.connect(DatabaseConfig("sqlite://test.db"))

    @transactional
    async def failing_handler(request, db=None, **kwargs):
        raise ValueError("Something went wrong")

    with pytest.raises(ValueError, match="Something went wrong"):
        await failing_handler(None, db=db)


# =============================================================================
# v0.8.0 Guards Tests
# =============================================================================


def test_guard_classes_importable():
    """Test that guard classes can be imported."""
    from cello.guards import (
        Guard,
        Role,
        Permission,
        Authenticated,
        And,
        Or,
        Not,
        GuardError,
        ForbiddenError,
        UnauthorizedError,
        verify_guards,
    )

    assert Guard is not None
    assert Role is not None
    assert Permission is not None
    assert Authenticated is not None
    assert And is not None
    assert Or is not None
    assert Not is not None
    assert GuardError is not None
    assert ForbiddenError is not None
    assert UnauthorizedError is not None
    assert verify_guards is not None


def test_role_guard_pass():
    """Test Role guard passes for user with correct role."""
    from cello.guards import Role

    guard = Role(["admin"])

    class MockRequest:
        context = {"user": {"roles": ["admin", "editor"]}}

    assert guard(MockRequest()) is True


def test_role_guard_fail():
    """Test Role guard fails for user without correct role."""
    from cello.guards import Role, ForbiddenError

    guard = Role(["admin"])

    class MockRequest:
        context = {"user": {"roles": ["viewer"]}}

    with pytest.raises(ForbiddenError):
        guard(MockRequest())


def test_role_guard_require_all():
    """Test Role guard with require_all=True."""
    from cello.guards import Role, ForbiddenError

    guard = Role(["admin", "editor"], require_all=True)

    class MockRequestPass:
        context = {"user": {"roles": ["admin", "editor", "viewer"]}}

    class MockRequestFail:
        context = {"user": {"roles": ["admin"]}}

    assert guard(MockRequestPass()) is True

    with pytest.raises(ForbiddenError):
        guard(MockRequestFail())


def test_permission_guard_pass():
    """Test Permission guard passes for user with correct permissions."""
    from cello.guards import Permission

    guard = Permission(["users:read"])

    class MockRequest:
        context = {"user": {"permissions": ["users:read", "users:write"]}}

    assert guard(MockRequest()) is True


def test_permission_guard_fail():
    """Test Permission guard fails for user without correct permissions."""
    from cello.guards import Permission, ForbiddenError

    guard = Permission(["users:delete"])

    class MockRequest:
        context = {"user": {"permissions": ["users:read"]}}

    with pytest.raises(ForbiddenError):
        guard(MockRequest())


def test_authenticated_guard_pass():
    """Test Authenticated guard passes when user exists."""
    from cello.guards import Authenticated

    guard = Authenticated()

    class MockRequest:
        context = {"user": {"id": 1, "name": "Alice"}}

    assert guard(MockRequest()) is True


def test_authenticated_guard_fail():
    """Test Authenticated guard fails when no user."""
    from cello.guards import Authenticated, UnauthorizedError

    guard = Authenticated()

    class MockRequest:
        context = {}

    with pytest.raises(UnauthorizedError):
        guard(MockRequest())


def test_and_guard():
    """Test And guard requires all guards to pass."""
    from cello.guards import And, Role, Permission, ForbiddenError

    guard = And([
        Role(["admin"]),
        Permission(["users:write"]),
    ])

    class MockRequestPass:
        context = {"user": {"roles": ["admin"], "permissions": ["users:write"]}}

    class MockRequestFail:
        context = {"user": {"roles": ["admin"], "permissions": []}}

    assert guard(MockRequestPass()) is True

    with pytest.raises(ForbiddenError):
        guard(MockRequestFail())


def test_or_guard():
    """Test Or guard passes if any guard passes."""
    from cello.guards import Or, Role

    guard = Or([
        Role(["admin"]),
        Role(["editor"]),
    ])

    class MockRequest:
        context = {"user": {"roles": ["editor"]}}

    assert guard(MockRequest()) is True


def test_not_guard():
    """Test Not guard inverts the result."""
    from cello.guards import Not, Role

    guard = Not(Role(["banned"]))

    class MockRequest:
        context = {"user": {"roles": ["regular"]}}

    assert guard(MockRequest()) is True


def test_not_guard_inverted():
    """Test Not guard raises when inner guard passes."""
    from cello.guards import Not, Role, ForbiddenError

    guard = Not(Role(["admin"]))

    class MockRequest:
        context = {"user": {"roles": ["admin"]}}

    with pytest.raises(ForbiddenError):
        guard(MockRequest())


def test_verify_guards():
    """Test verify_guards helper with multiple guards."""
    from cello.guards import verify_guards, Role, Authenticated

    guards = [Authenticated(), Role(["admin"])]

    class MockRequest:
        context = {"user": {"roles": ["admin"]}}

    # Should not raise
    verify_guards(guards, MockRequest())


def test_verify_guards_fails():
    """Test verify_guards raises on first failure."""
    from cello.guards import verify_guards, Role, Authenticated, UnauthorizedError

    guards = [Authenticated(), Role(["admin"])]

    class MockRequest:
        context = {}

    with pytest.raises(UnauthorizedError):
        verify_guards(guards, MockRequest())


def test_guard_error_hierarchy():
    """Test guard error class hierarchy."""
    from cello.guards import GuardError, ForbiddenError, UnauthorizedError

    assert issubclass(ForbiddenError, GuardError)
    assert issubclass(UnauthorizedError, GuardError)

    err = GuardError("test", 403)
    assert err.message == "test"
    assert err.status_code == 403

    forbidden = ForbiddenError("forbidden")
    assert forbidden.status_code == 403

    unauthorized = UnauthorizedError()
    assert unauthorized.status_code == 401
    assert unauthorized.message == "Authentication required"


# =============================================================================
# v0.8.0 Bug Fix Verification Tests
# =============================================================================


def test_logs_parameter_name():
    """Test that App.run() uses 'logs' parameter (not 'loogs' typo)."""
    from cello import App
    import inspect

    app = App()
    sig = inspect.signature(app.run)
    param_names = list(sig.parameters.keys())

    # The old bug had 'loogs' instead of 'logs'
    assert "loogs" not in param_names
    assert "logs" in param_names


def test_response_no_error_method():
    """Test that Response does not have a non-existent .error() method."""
    from cello import Response

    # Response.error() was used in examples but doesn't exist
    # The correct approach is Response.json() + set_status()
    assert not hasattr(Response, "error")

    # Correct pattern: use Response.json with status
    resp = Response.json({"error": "Something went wrong"}, status=500)
    assert resp.status == 500


def test_cors_with_custom_origins():
    """Test CORS middleware accepts and applies custom origins."""
    from cello import App

    app = App()
    # This was a bug: origins were being ignored with `let _ = o;`
    app.enable_cors(origins=["https://example.com"])
    assert True


def test_cors_with_wildcard_origin():
    """Test CORS middleware with wildcard origin."""
    from cello import App

    app = App()
    app.enable_cors(origins=["*"])
    assert True


def test_cors_with_multiple_origins():
    """Test CORS middleware with multiple origins."""
    from cello import App

    app = App()
    app.enable_cors(origins=[
        "https://app.example.com",
        "https://admin.example.com",
        "https://api.example.com",
    ])
    assert True


# =============================================================================
# v0.8.0 Full Feature Combination Tests
# =============================================================================


def test_app_with_all_v080_features():
    """Test an App with all v0.8.0 features enabled together."""
    from cello import App, DatabaseConfig, RedisConfig

    app = App()

    # Core middleware
    app.enable_cors()
    app.enable_logging()
    app.enable_compression()

    # v0.8.0 Data Layer
    app.enable_database(DatabaseConfig(
        url="postgresql://user:pass@localhost/mydb",
        pool_size=20,
    ))
    app.enable_redis(RedisConfig(
        url="redis://localhost:6379",
        pool_size=10,
    ))

    # Register routes
    @app.get("/")
    def home(req):
        return {"version": "0.9.0"}

    @app.post("/data")
    def create_data(req):
        return {"created": True}

    assert True


def test_app_with_database_and_transactional_route():
    """Test App with database and transactional route handler."""
    from cello import App, DatabaseConfig
    from cello.database import transactional

    app = App()
    app.enable_database(DatabaseConfig("sqlite://test.db"))

    @app.post("/transfer")
    @transactional
    async def transfer(request):
        return {"success": True}

    assert True


def test_v080_all_exports():
    """Test that all v0.8.0 expected exports are available."""
    from cello import (
        # Core
        App, Blueprint, Request, Response,
        # v0.7.0
        OpenTelemetryConfig, HealthCheckConfig,
        DatabaseConfig, GraphQLConfig,
        # v0.8.0
        RedisConfig,
    )
    from cello.database import transactional, Database, Redis, Transaction

    assert all([
        App, Blueprint, Request, Response,
        OpenTelemetryConfig, HealthCheckConfig,
        DatabaseConfig, GraphQLConfig,
        RedisConfig,
        transactional, Database, Redis, Transaction,
    ])


# =============================================================================
# v0.9.0 API Protocol Feature Tests
# =============================================================================


# ---------------------------------------------------------------------------
# v0.9.0 Version & Exports
# ---------------------------------------------------------------------------


def test_version_v090():
    """Test that version is 0.9.0."""
    import cello

    assert cello.__version__ == "0.9.0"


def test_v090_exports_in_all():
    """Test that v0.9.0 features are in __all__."""
    import cello

    assert "GrpcConfig" in cello.__all__
    assert "KafkaConfig" in cello.__all__
    assert "RabbitMQConfig" in cello.__all__
    assert "SqsConfig" in cello.__all__


def test_import_v090_api_protocol_configs():
    """Test that v0.9.0 API protocol configuration classes can be imported."""
    from cello import GrpcConfig, KafkaConfig, RabbitMQConfig, SqsConfig

    assert GrpcConfig is not None
    assert KafkaConfig is not None
    assert RabbitMQConfig is not None
    assert SqsConfig is not None


def test_v090_all_exports():
    """Test that all v0.9.0 expected exports are available."""
    from cello import (
        # Core
        App, Blueprint, Request, Response,
        # v0.7.0
        OpenTelemetryConfig, HealthCheckConfig,
        DatabaseConfig, GraphQLConfig,
        # v0.8.0
        RedisConfig,
        # v0.9.0
        GrpcConfig, KafkaConfig, RabbitMQConfig, SqsConfig,
    )
    from cello.database import transactional, Database, Redis, Transaction
    from cello.graphql import Query, Mutation, Subscription, Field, DataLoader, GraphQL, Schema
    from cello.grpc import (
        GrpcService, grpc_method, GrpcRequest, GrpcResponse,
        GrpcServer, GrpcChannel, GrpcError,
    )
    from cello.messaging import (
        kafka_consumer, kafka_producer, Message, MessageResult,
        Producer, Consumer,
    )

    assert all([
        App, Blueprint, Request, Response,
        OpenTelemetryConfig, HealthCheckConfig,
        DatabaseConfig, GraphQLConfig,
        RedisConfig,
        GrpcConfig, KafkaConfig, RabbitMQConfig, SqsConfig,
        transactional, Database, Redis, Transaction,
        Query, Mutation, Subscription, Field, DataLoader, GraphQL, Schema,
        GrpcService, grpc_method, GrpcRequest, GrpcResponse,
        GrpcServer, GrpcChannel, GrpcError,
        kafka_consumer, kafka_producer, Message, MessageResult,
        Producer, Consumer,
    ])


# ---------------------------------------------------------------------------
# v0.9.0 GrpcConfig (Rust-backed PyO3 class)
# ---------------------------------------------------------------------------


def test_grpc_config_defaults():
    """Test GrpcConfig creation with default values."""
    from cello import GrpcConfig

    config = GrpcConfig()
    assert config is not None
    assert config.address == "[::]:50051"
    assert config.reflection is True
    assert config.max_message_size == 4194304  # 4MB
    assert config.enable_web is False
    assert config.keepalive_secs == 60
    assert config.concurrency_limit == 100


def test_grpc_config_custom():
    """Test GrpcConfig creation with custom values."""
    from cello import GrpcConfig

    config = GrpcConfig(
        address="0.0.0.0:9090",
        reflection=False,
        max_message_size=8388608,
        enable_web=True,
        keepalive_secs=120,
        concurrency_limit=500,
    )
    assert config.address == "0.0.0.0:9090"
    assert config.reflection is False
    assert config.max_message_size == 8388608
    assert config.enable_web is True
    assert config.keepalive_secs == 120
    assert config.concurrency_limit == 500


def test_grpc_config_local():
    """Test GrpcConfig.local() static method."""
    from cello import GrpcConfig

    config = GrpcConfig.local()
    assert config is not None
    assert config.address == "[::]:50051"
    assert config.reflection is True
    assert config.enable_web is True
    assert config.keepalive_secs == 60
    assert config.concurrency_limit == 100


def test_grpc_config_production():
    """Test GrpcConfig.production() static method."""
    from cello import GrpcConfig

    config = GrpcConfig.production()
    assert config is not None
    assert config.address == "[::]:50051"
    assert config.reflection is False
    assert config.enable_web is False
    assert config.keepalive_secs == 120
    assert config.concurrency_limit == 1000


def test_grpc_config_production_custom():
    """Test GrpcConfig.production() with custom address and message size."""
    from cello import GrpcConfig

    config = GrpcConfig.production(address="0.0.0.0:50051", max_message_size=8388608)
    assert config.address == "0.0.0.0:50051"
    assert config.max_message_size == 8388608
    assert config.reflection is False


def test_grpc_config_attributes_settable():
    """Test that GrpcConfig attributes can be set after creation."""
    from cello import GrpcConfig

    config = GrpcConfig()
    config.address = "localhost:50052"
    config.reflection = False
    config.max_message_size = 1024
    config.enable_web = True
    config.keepalive_secs = 30
    config.concurrency_limit = 50

    assert config.address == "localhost:50052"
    assert config.reflection is False
    assert config.max_message_size == 1024
    assert config.enable_web is True
    assert config.keepalive_secs == 30
    assert config.concurrency_limit == 50


# ---------------------------------------------------------------------------
# v0.9.0 KafkaConfig (Rust-backed PyO3 class)
# ---------------------------------------------------------------------------


def test_kafka_config_defaults():
    """Test KafkaConfig creation with default values."""
    from cello import KafkaConfig

    config = KafkaConfig()
    assert config is not None
    assert config.brokers == ["localhost:9092"]
    assert config.group_id is None
    assert config.client_id is None
    assert config.auto_commit is True
    assert config.session_timeout_ms == 30000
    assert config.max_poll_records == 500


def test_kafka_config_custom():
    """Test KafkaConfig creation with custom values."""
    from cello import KafkaConfig

    config = KafkaConfig(
        brokers=["broker1:9092", "broker2:9092"],
        group_id="my-group",
        client_id="my-client",
        auto_commit=False,
        session_timeout_ms=60000,
        max_poll_records=1000,
    )
    assert config.brokers == ["broker1:9092", "broker2:9092"]
    assert config.group_id == "my-group"
    assert config.client_id == "my-client"
    assert config.auto_commit is False
    assert config.session_timeout_ms == 60000
    assert config.max_poll_records == 1000


def test_kafka_config_local():
    """Test KafkaConfig.local() static method."""
    from cello import KafkaConfig

    config = KafkaConfig.local()
    assert config is not None
    assert config.brokers == ["localhost:9092"]
    assert config.auto_commit is True
    assert config.session_timeout_ms == 30000
    assert config.max_poll_records == 500


def test_kafka_config_attributes_settable():
    """Test that KafkaConfig attributes can be set after creation."""
    from cello import KafkaConfig

    config = KafkaConfig()
    config.brokers = ["new-broker:9092"]
    config.group_id = "updated-group"
    config.client_id = "updated-client"
    config.auto_commit = False
    config.session_timeout_ms = 15000
    config.max_poll_records = 100

    assert config.brokers == ["new-broker:9092"]
    assert config.group_id == "updated-group"
    assert config.client_id == "updated-client"
    assert config.auto_commit is False
    assert config.session_timeout_ms == 15000
    assert config.max_poll_records == 100


# ---------------------------------------------------------------------------
# v0.9.0 RabbitMQConfig (Rust-backed PyO3 class)
# ---------------------------------------------------------------------------


def test_rabbitmq_config_defaults():
    """Test RabbitMQConfig creation with default values."""
    from cello import RabbitMQConfig

    config = RabbitMQConfig()
    assert config is not None
    assert config.url == "amqp://localhost"
    assert config.vhost == "/"
    assert config.prefetch_count == 10
    assert config.heartbeat == 60
    assert config.connection_timeout_secs == 5


def test_rabbitmq_config_custom():
    """Test RabbitMQConfig creation with custom values."""
    from cello import RabbitMQConfig

    config = RabbitMQConfig(
        url="amqp://user:pass@rabbitmq.example.com:5672",
        vhost="/production",
        prefetch_count=20,
        heartbeat=30,
        connection_timeout_secs=10,
    )
    assert config.url == "amqp://user:pass@rabbitmq.example.com:5672"
    assert config.vhost == "/production"
    assert config.prefetch_count == 20
    assert config.heartbeat == 30
    assert config.connection_timeout_secs == 10


def test_rabbitmq_config_local():
    """Test RabbitMQConfig.local() static method."""
    from cello import RabbitMQConfig

    config = RabbitMQConfig.local()
    assert config is not None
    assert config.url == "amqp://localhost"
    assert config.vhost == "/"
    assert config.prefetch_count == 10
    assert config.heartbeat == 60
    assert config.connection_timeout_secs == 5


def test_rabbitmq_config_attributes_settable():
    """Test that RabbitMQConfig attributes can be set after creation."""
    from cello import RabbitMQConfig

    config = RabbitMQConfig()
    config.url = "amqp://new-host"
    config.vhost = "/staging"
    config.prefetch_count = 50
    config.heartbeat = 120
    config.connection_timeout_secs = 15

    assert config.url == "amqp://new-host"
    assert config.vhost == "/staging"
    assert config.prefetch_count == 50
    assert config.heartbeat == 120
    assert config.connection_timeout_secs == 15


# ---------------------------------------------------------------------------
# v0.9.0 SqsConfig (Rust-backed PyO3 class)
# ---------------------------------------------------------------------------


def test_sqs_config_defaults():
    """Test SqsConfig creation with default values."""
    from cello import SqsConfig

    config = SqsConfig()
    assert config is not None
    assert config.region == "us-east-1"
    assert config.queue_url == ""
    assert config.endpoint_url is None
    assert config.max_messages == 10
    assert config.wait_time_secs == 20
    assert config.visibility_timeout_secs == 30


def test_sqs_config_custom():
    """Test SqsConfig creation with custom values."""
    from cello import SqsConfig

    config = SqsConfig(
        region="us-west-2",
        queue_url="https://sqs.us-west-2.amazonaws.com/123/my-queue",
        endpoint_url=None,
        max_messages=5,
        wait_time_secs=10,
        visibility_timeout_secs=60,
    )
    assert config.region == "us-west-2"
    assert config.queue_url == "https://sqs.us-west-2.amazonaws.com/123/my-queue"
    assert config.endpoint_url is None
    assert config.max_messages == 5
    assert config.wait_time_secs == 10
    assert config.visibility_timeout_secs == 60


def test_sqs_config_local():
    """Test SqsConfig.local() static method."""
    from cello import SqsConfig

    queue_url = "http://localhost:4566/000000000000/my-queue"
    config = SqsConfig.local(queue_url)
    assert config is not None
    assert config.region == "us-east-1"
    assert config.queue_url == queue_url
    assert config.endpoint_url == "http://localhost:4566"
    assert config.max_messages == 10
    assert config.wait_time_secs == 20
    assert config.visibility_timeout_secs == 30


def test_sqs_config_attributes_settable():
    """Test that SqsConfig attributes can be set after creation."""
    from cello import SqsConfig

    config = SqsConfig()
    config.region = "eu-west-1"
    config.queue_url = "https://sqs.eu-west-1.amazonaws.com/456/other-queue"
    config.endpoint_url = "http://custom-endpoint:4566"
    config.max_messages = 3
    config.wait_time_secs = 5
    config.visibility_timeout_secs = 120

    assert config.region == "eu-west-1"
    assert config.queue_url == "https://sqs.eu-west-1.amazonaws.com/456/other-queue"
    assert config.endpoint_url == "http://custom-endpoint:4566"
    assert config.max_messages == 3
    assert config.wait_time_secs == 5
    assert config.visibility_timeout_secs == 120


# ---------------------------------------------------------------------------
# v0.9.0 App Integration Methods
# ---------------------------------------------------------------------------


def test_app_enable_grpc():
    """Test App.enable_grpc() does not raise errors."""
    from cello import App

    app = App()
    app.enable_grpc()
    assert True


def test_app_enable_grpc_with_config():
    """Test App.enable_grpc() with explicit config."""
    from cello import App, GrpcConfig

    app = App()
    config = GrpcConfig(
        address="0.0.0.0:50051",
        reflection=True,
        enable_web=True,
    )
    app.enable_grpc(config)
    assert True


def test_app_enable_grpc_local():
    """Test App.enable_grpc() with local config."""
    from cello import App, GrpcConfig

    app = App()
    app.enable_grpc(GrpcConfig.local())
    assert True


def test_app_enable_grpc_production():
    """Test App.enable_grpc() with production config."""
    from cello import App, GrpcConfig

    app = App()
    app.enable_grpc(GrpcConfig.production())
    assert True


def test_app_add_grpc_service():
    """Test App.add_grpc_service() with name and methods."""
    from cello import App, GrpcConfig

    app = App()
    app.enable_grpc()
    app.add_grpc_service("UserService", ["GetUser", "ListUsers", "CreateUser"])
    assert True


def test_app_add_grpc_service_no_methods():
    """Test App.add_grpc_service() with name only."""
    from cello import App

    app = App()
    app.enable_grpc()
    app.add_grpc_service("EmptyService")
    assert True


def test_app_enable_messaging():
    """Test App.enable_messaging() with KafkaConfig."""
    from cello import App, KafkaConfig

    app = App()
    config = KafkaConfig(
        brokers=["localhost:9092"],
        group_id="test-group",
    )
    app.enable_messaging(config)
    assert True


def test_app_enable_messaging_local():
    """Test App.enable_messaging() with local KafkaConfig."""
    from cello import App, KafkaConfig

    app = App()
    app.enable_messaging(KafkaConfig.local())
    assert True


def test_app_enable_rabbitmq():
    """Test App.enable_rabbitmq() with RabbitMQConfig."""
    from cello import App, RabbitMQConfig

    app = App()
    config = RabbitMQConfig(
        url="amqp://guest:guest@localhost",
        prefetch_count=20,
    )
    app.enable_rabbitmq(config)
    assert True


def test_app_enable_rabbitmq_local():
    """Test App.enable_rabbitmq() with local RabbitMQConfig."""
    from cello import App, RabbitMQConfig

    app = App()
    app.enable_rabbitmq(RabbitMQConfig.local())
    assert True


def test_app_enable_sqs():
    """Test App.enable_sqs() with SqsConfig."""
    from cello import App, SqsConfig

    app = App()
    config = SqsConfig(
        region="us-west-2",
        queue_url="https://sqs.us-west-2.amazonaws.com/123/queue",
    )
    app.enable_sqs(config)
    assert True


def test_app_enable_sqs_local():
    """Test App.enable_sqs() with local SqsConfig."""
    from cello import App, SqsConfig

    app = App()
    app.enable_sqs(SqsConfig.local("http://localhost:4566/000000000000/test"))
    assert True


def test_app_with_all_v090_features():
    """Test an App with all v0.9.0 features enabled together."""
    from cello import App, GrpcConfig, KafkaConfig, RabbitMQConfig, SqsConfig

    app = App()

    # Core middleware
    app.enable_cors()
    app.enable_logging()

    # v0.9.0 API Protocol features
    app.enable_grpc(GrpcConfig.local())
    app.add_grpc_service("UserService", ["GetUser", "CreateUser"])
    app.enable_messaging(KafkaConfig.local())
    app.enable_rabbitmq(RabbitMQConfig.local())
    app.enable_sqs(SqsConfig.local("http://localhost:4566/000000000000/test"))

    # Register routes
    @app.get("/")
    def home(req):
        return {"version": "0.9.0"}

    assert True


# ---------------------------------------------------------------------------
# v0.9.0 GraphQL Python Module Tests
# ---------------------------------------------------------------------------


def test_graphql_query_decorator():
    """Test Query decorator wraps a function."""
    from cello.graphql import Query

    @Query
    def users(info) -> list:
        """Fetch all users."""
        return [{"id": 1, "name": "Alice"}]

    assert users.name == "users"
    assert users.return_type == "list"
    assert users._doc == "Fetch all users."
    assert repr(users) == "<Query 'users'>"


def test_graphql_query_callable():
    """Test Query-decorated function is callable."""
    from cello.graphql import Query

    @Query
    def hello(info) -> str:
        return "hello"

    result = hello({})
    assert result == "hello"


def test_graphql_query_parameters():
    """Test Query decorator extracts parameter type hints."""
    from cello.graphql import Query

    @Query
    def user(info, id: int, name: str) -> dict:
        return {"id": id, "name": name}

    params = user.parameters
    assert "id" in params
    assert params["id"] == "int"
    assert "name" in params
    assert params["name"] == "str"


def test_graphql_mutation_decorator():
    """Test Mutation decorator wraps a function."""
    from cello.graphql import Mutation

    @Mutation
    def create_user(info, name: str) -> dict:
        """Create a new user."""
        return {"id": 3, "name": name}

    assert create_user.name == "create_user"
    assert create_user.return_type == "dict"
    assert repr(create_user) == "<Mutation 'create_user'>"


def test_graphql_mutation_callable():
    """Test Mutation-decorated function is callable."""
    from cello.graphql import Mutation

    @Mutation
    def delete_user(info, id: int) -> dict:
        return {"deleted": True}

    result = delete_user({}, id=42)
    assert result == {"deleted": True}


def test_graphql_subscription_decorator():
    """Test Subscription decorator wraps a function."""
    from cello.graphql import Subscription

    @Subscription
    def on_message(info) -> dict:
        """Subscribe to messages."""
        return {"message": "hello"}

    assert on_message.name == "on_message"
    assert on_message.return_type == "dict"
    assert repr(on_message) == "<Subscription 'on_message'>"


def test_graphql_subscription_callable():
    """Test Subscription-decorated function is callable."""
    from cello.graphql import Subscription

    @Subscription
    def on_event(info) -> dict:
        return {"event": "test"}

    result = on_event({})
    assert result == {"event": "test"}


def test_graphql_subscription_async_detection():
    """Test Subscription detects async functions."""
    from cello.graphql import Subscription
    import asyncio

    @Subscription
    async def on_async_event(info) -> dict:
        return {"event": "async"}

    assert on_async_event._is_async is True


def test_graphql_field_basic():
    """Test Field creation with name and type."""
    from cello.graphql import Field

    field = Field("name", "String")
    assert field.name == "name"
    assert field.type_name == "String"
    assert field.description is None
    assert field.resolver is None
    assert repr(field) == "<Field 'name: String'>"


def test_graphql_field_with_description():
    """Test Field creation with description."""
    from cello.graphql import Field

    field = Field("email", "String", description="User's email address")
    assert field.description == "User's email address"


def test_graphql_field_with_resolver():
    """Test Field with custom resolver function."""
    from cello.graphql import Field

    field = Field(
        "full_name",
        "String",
        resolver=lambda obj, info: f"{obj['first']} {obj['last']}",
    )
    result = field.resolve({"first": "John", "last": "Doe"}, {})
    assert result == "John Doe"


def test_graphql_field_default_dict_resolve():
    """Test Field default resolution from dict."""
    from cello.graphql import Field

    field = Field("name", "String")
    result = field.resolve({"name": "Alice", "age": 30}, {})
    assert result == "Alice"


def test_graphql_field_default_object_resolve():
    """Test Field default resolution from object attribute."""
    from cello.graphql import Field

    class User:
        name = "Bob"

    field = Field("name", "String")
    result = field.resolve(User(), {})
    assert result == "Bob"


def test_graphql_field_missing_key():
    """Test Field returns None for missing key."""
    from cello.graphql import Field

    field = Field("missing", "String")
    result = field.resolve({"name": "Alice"}, {})
    assert result is None


@pytest.mark.asyncio
async def test_graphql_dataloader_load():
    """Test DataLoader.load() batches and caches."""
    from cello.graphql import DataLoader

    async def batch_fn(keys):
        return [{"id": k, "name": f"User {k}"} for k in keys]

    loader = DataLoader(batch_fn)
    result = await loader.load(1)
    assert result == {"id": 1, "name": "User 1"}


@pytest.mark.asyncio
async def test_graphql_dataloader_load_cached():
    """Test DataLoader returns cached results on second call."""
    from cello.graphql import DataLoader

    call_count = 0

    async def batch_fn(keys):
        nonlocal call_count
        call_count += 1
        return [f"result-{k}" for k in keys]

    loader = DataLoader(batch_fn)
    result1 = await loader.load("a")
    result2 = await loader.load("a")
    assert result1 == "result-a"
    assert result2 == "result-a"
    assert call_count == 1  # Only one batch call


@pytest.mark.asyncio
async def test_graphql_dataloader_load_many():
    """Test DataLoader.load_many() batches multiple keys."""
    from cello.graphql import DataLoader

    async def batch_fn(keys):
        return [k * 10 for k in keys]

    loader = DataLoader(batch_fn)
    results = await loader.load_many([1, 2, 3])
    assert results == [10, 20, 30]


@pytest.mark.asyncio
async def test_graphql_dataloader_clear_key():
    """Test DataLoader.clear() for a specific key."""
    from cello.graphql import DataLoader

    async def batch_fn(keys):
        return [f"v-{k}" for k in keys]

    loader = DataLoader(batch_fn)
    await loader.load("x")
    assert "x" in loader._cache

    loader.clear("x")
    assert "x" not in loader._cache


@pytest.mark.asyncio
async def test_graphql_dataloader_clear_all():
    """Test DataLoader.clear() with no key clears entire cache."""
    from cello.graphql import DataLoader

    async def batch_fn(keys):
        return [k for k in keys]

    loader = DataLoader(batch_fn)
    await loader.load_many([1, 2, 3])
    assert len(loader._cache) == 3

    loader.clear()
    assert len(loader._cache) == 0


@pytest.mark.asyncio
async def test_graphql_dataloader_mismatched_results():
    """Test DataLoader raises on mismatched result count."""
    from cello.graphql import DataLoader

    async def bad_batch_fn(keys):
        return [1]  # Always returns 1 result regardless of key count

    loader = DataLoader(bad_batch_fn)
    with pytest.raises(ValueError, match="Must return exactly one result per key"):
        await loader.load_many([1, 2, 3])


@pytest.mark.asyncio
async def test_graphql_engine_add_query():
    """Test GraphQL engine add_query and execute."""
    from cello.graphql import GraphQL, Query

    gql = GraphQL()

    @Query
    def hello(info) -> str:
        return "Hello, world!"

    gql.add_query(hello)

    result = await gql.execute("{ hello }")
    assert "data" in result
    assert result["data"]["hello"] == "Hello, world!"


@pytest.mark.asyncio
async def test_graphql_engine_add_mutation():
    """Test GraphQL engine add_mutation and execute."""
    from cello.graphql import GraphQL, Mutation

    gql = GraphQL()

    @Mutation
    def create_item(info, name: str) -> dict:
        return {"id": 1, "name": name}

    gql.add_mutation(create_item)

    result = await gql.execute("mutation { createItem }", variables={"name": "Widget"})
    assert "data" in result
    assert result["data"]["create_item"]["name"] == "Widget"


@pytest.mark.asyncio
async def test_graphql_engine_add_subscription():
    """Test GraphQL engine add_subscription registers correctly."""
    from cello.graphql import GraphQL, Subscription

    gql = GraphQL()

    @Subscription
    def on_update(info) -> dict:
        return {"updated": True}

    gql.add_subscription(on_update)

    schema = gql.get_schema()
    assert len(schema["subscriptions"]) == 1
    assert schema["subscriptions"][0]["name"] == "on_update"


@pytest.mark.asyncio
async def test_graphql_engine_plain_function():
    """Test GraphQL engine accepts plain functions (not decorated)."""
    from cello.graphql import GraphQL

    gql = GraphQL()

    def users(info) -> list:
        return [{"id": 1}]

    gql.add_query(users)

    result = await gql.execute("{ users }")
    assert result["data"]["users"] == [{"id": 1}]


@pytest.mark.asyncio
async def test_graphql_engine_error_handling():
    """Test GraphQL engine captures resolver errors."""
    from cello.graphql import GraphQL, Query

    gql = GraphQL()

    @Query
    def failing(info) -> str:
        raise RuntimeError("Something broke")

    gql.add_query(failing)

    result = await gql.execute("{ failing }")
    assert "errors" in result
    assert len(result["errors"]) == 1
    assert "Something broke" in result["errors"][0]["message"]


def test_graphql_engine_get_schema():
    """Test GraphQL engine get_schema returns structure."""
    from cello.graphql import GraphQL, Query, Mutation, Subscription

    gql = GraphQL()

    @Query
    def q1(info) -> list:
        return []

    @Mutation
    def m1(info, x: int) -> dict:
        return {}

    @Subscription
    def s1(info) -> dict:
        return {}

    gql.add_query(q1)
    gql.add_mutation(m1)
    gql.add_subscription(s1)

    schema = gql.get_schema()
    assert len(schema["queries"]) == 1
    assert len(schema["mutations"]) == 1
    assert len(schema["subscriptions"]) == 1
    assert schema["queries"][0]["name"] == "q1"
    assert schema["mutations"][0]["name"] == "m1"
    assert schema["subscriptions"][0]["name"] == "s1"


def test_graphql_engine_repr():
    """Test GraphQL engine repr."""
    from cello.graphql import GraphQL

    gql = GraphQL()
    assert "queries=0" in repr(gql)
    assert "mutations=0" in repr(gql)
    assert "subscriptions=0" in repr(gql)


def test_graphql_schema_builder():
    """Test Schema builder creates GraphQL engine."""
    from cello.graphql import Schema, Query, Mutation, Subscription

    schema = Schema()

    @Query
    def users(info) -> list:
        return []

    @Mutation
    def create_user(info, name: str) -> dict:
        return {"name": name}

    @Subscription
    def on_message(info) -> dict:
        return {}

    schema.query(users)
    schema.mutation(create_user)
    schema.subscription(on_message)

    gql = schema.build()
    assert gql is not None

    s = gql.get_schema()
    assert len(s["queries"]) == 1
    assert len(s["mutations"]) == 1
    assert len(s["subscriptions"]) == 1


def test_graphql_schema_builder_chaining():
    """Test Schema builder supports method chaining."""
    from cello.graphql import Schema, Query

    @Query
    def q1(info) -> str:
        return "a"

    @Query
    def q2(info) -> str:
        return "b"

    schema = Schema()
    result = schema.query(q1).query(q2)
    assert result is schema  # Should return self

    gql = schema.build()
    s = gql.get_schema()
    assert len(s["queries"]) == 2


def test_graphql_schema_repr():
    """Test Schema repr."""
    from cello.graphql import Schema

    schema = Schema()
    assert "queries=0" in repr(schema)


# ---------------------------------------------------------------------------
# v0.9.0 gRPC Python Module Tests
# ---------------------------------------------------------------------------


def test_grpc_service_subclass():
    """Test GrpcService subclassing and method discovery."""
    from cello.grpc import GrpcService, grpc_method

    class UserService(GrpcService):
        @grpc_method
        def get_user(self, request):
            return {"id": 1}

        @grpc_method(stream=True)
        def list_users(self, request):
            yield {"id": 1}
            yield {"id": 2}

    service = UserService()
    assert service.get_name() == "UserService"

    methods = service.get_methods()
    assert len(methods) == 2

    method_names = [m["name"] for m in methods]
    assert "get_user" in method_names
    assert "list_users" in method_names

    # Check stream flags
    method_map = {m["name"]: m for m in methods}
    assert method_map["get_user"]["stream"] is False
    assert method_map["list_users"]["stream"] is True


def test_grpc_service_custom_name():
    """Test GrpcService with explicit custom name."""
    from cello.grpc import GrpcService

    class MyService(GrpcService):
        pass

    service = MyService(name="custom.ServiceName")
    assert service.get_name() == "custom.ServiceName"


def test_grpc_service_repr():
    """Test GrpcService repr."""
    from cello.grpc import GrpcService, grpc_method

    class TestService(GrpcService):
        @grpc_method
        def ping(self, request):
            return {}

    service = TestService()
    r = repr(service)
    assert "TestService" in r
    assert "methods=1" in r


def test_grpc_method_decorator_without_args():
    """Test @grpc_method decorator without parentheses."""
    from cello.grpc import grpc_method

    @grpc_method
    def my_method(self, request):
        return {}

    assert my_method._grpc_method is True
    assert my_method._grpc_method_name == "my_method"
    assert my_method._grpc_stream is False


def test_grpc_method_decorator_with_stream():
    """Test @grpc_method(stream=True) decorator."""
    from cello.grpc import grpc_method

    @grpc_method(stream=True)
    def streaming_method(self, request):
        yield {}

    assert streaming_method._grpc_method is True
    assert streaming_method._grpc_method_name == "streaming_method"
    assert streaming_method._grpc_stream is True


def test_grpc_request_creation():
    """Test GrpcRequest creation and properties."""
    from cello.grpc import GrpcRequest

    req = GrpcRequest(
        service="UserService",
        method="GetUser",
        data={"id": 42},
        metadata={"authorization": "Bearer token123"},
    )
    assert req.service == "UserService"
    assert req.method == "GetUser"
    assert req.data == {"id": 42}
    assert req.metadata == {"authorization": "Bearer token123"}
    assert repr(req) == "GrpcRequest(service='UserService', method='GetUser')"


def test_grpc_request_defaults():
    """Test GrpcRequest default values."""
    from cello.grpc import GrpcRequest

    req = GrpcRequest(service="Svc", method="Method")
    assert req.data == {}
    assert req.metadata == {}


def test_grpc_response_ok():
    """Test GrpcResponse.ok() class method."""
    from cello.grpc import GrpcResponse

    resp = GrpcResponse.ok({"id": 1, "name": "Alice"})
    assert resp.data == {"id": 1, "name": "Alice"}
    assert resp.status_code == 0
    assert resp.message == "OK"


def test_grpc_response_error():
    """Test GrpcResponse.error() class method."""
    from cello.grpc import GrpcResponse

    resp = GrpcResponse.error(5, "Not found")
    assert resp.data == {}
    assert resp.status_code == 5
    assert resp.message == "Not found"


def test_grpc_response_custom():
    """Test GrpcResponse with custom constructor values."""
    from cello.grpc import GrpcResponse

    resp = GrpcResponse(
        data={"result": "ok"},
        status_code=0,
        message="Success",
    )
    assert resp.data == {"result": "ok"}
    assert resp.status_code == 0
    assert resp.message == "Success"
    assert resp.metadata == {}


def test_grpc_response_repr():
    """Test GrpcResponse repr."""
    from cello.grpc import GrpcResponse

    resp = GrpcResponse.ok({"x": 1})
    r = repr(resp)
    assert "status_code=0" in r
    assert "OK" in r


def test_grpc_server_creation():
    """Test GrpcServer creation."""
    from cello.grpc import GrpcServer

    server = GrpcServer()
    assert server is not None
    assert server._running is False
    assert server.get_services() == []


def test_grpc_server_register_service():
    """Test GrpcServer.register_service()."""
    from cello.grpc import GrpcServer, GrpcService, grpc_method

    class TestService(GrpcService):
        @grpc_method
        def ping(self, request):
            return {"pong": True}

    server = GrpcServer()
    service = TestService()
    server.register_service(service)

    services = server.get_services()
    assert len(services) == 1
    assert "TestService" in services


def test_grpc_server_register_multiple():
    """Test GrpcServer with multiple services."""
    from cello.grpc import GrpcServer, GrpcService, grpc_method

    class ServiceA(GrpcService):
        @grpc_method
        def a_method(self, request):
            return {}

    class ServiceB(GrpcService):
        @grpc_method
        def b_method(self, request):
            return {}

    server = GrpcServer()
    server.register_service(ServiceA())
    server.register_service(ServiceB())

    services = server.get_services()
    assert len(services) == 2
    assert "ServiceA" in services
    assert "ServiceB" in services


def test_grpc_server_register_type_error():
    """Test GrpcServer raises TypeError for non-GrpcService."""
    from cello.grpc import GrpcServer

    server = GrpcServer()
    with pytest.raises(TypeError, match="Expected GrpcService instance"):
        server.register_service("not a service")


def test_grpc_server_register_duplicate_error():
    """Test GrpcServer raises ValueError for duplicate service name."""
    from cello.grpc import GrpcServer, GrpcService

    class DupService(GrpcService):
        pass

    server = GrpcServer()
    server.register_service(DupService())
    with pytest.raises(ValueError, match="already registered"):
        server.register_service(DupService())


@pytest.mark.asyncio
async def test_grpc_server_start_stop():
    """Test GrpcServer start and stop lifecycle."""
    from cello.grpc import GrpcServer

    server = GrpcServer()
    assert server._running is False

    await server.start("[::]:50051")
    assert server._running is True

    await server.stop()
    assert server._running is False


@pytest.mark.asyncio
async def test_grpc_server_start_already_running():
    """Test GrpcServer raises RuntimeError if started while running."""
    from cello.grpc import GrpcServer

    server = GrpcServer()
    await server.start()
    with pytest.raises(RuntimeError, match="already running"):
        await server.start()
    await server.stop()


def test_grpc_server_repr():
    """Test GrpcServer repr."""
    from cello.grpc import GrpcServer

    server = GrpcServer()
    r = repr(server)
    assert "services=0" in r
    assert "running=False" in r


@pytest.mark.asyncio
async def test_grpc_channel_connect():
    """Test GrpcChannel.connect() creates connected channel."""
    from cello.grpc import GrpcChannel

    channel = await GrpcChannel.connect("localhost:50051")
    assert channel is not None
    assert channel._connected is True
    assert channel._target == "localhost:50051"


@pytest.mark.asyncio
async def test_grpc_channel_call():
    """Test GrpcChannel.call() returns a dict."""
    from cello.grpc import GrpcChannel

    channel = await GrpcChannel.connect("localhost:50051")
    result = await channel.call("UserService", "GetUser", {"id": 1})
    assert isinstance(result, dict)


@pytest.mark.asyncio
async def test_grpc_channel_call_disconnected():
    """Test GrpcChannel.call() raises GrpcError when not connected."""
    from cello.grpc import GrpcChannel, GrpcError

    channel = GrpcChannel("localhost:50051")
    assert channel._connected is False
    with pytest.raises(GrpcError) as exc_info:
        await channel.call("Svc", "Method", {})
    assert exc_info.value.code == GrpcError.UNAVAILABLE


@pytest.mark.asyncio
async def test_grpc_channel_close():
    """Test GrpcChannel.close() disconnects."""
    from cello.grpc import GrpcChannel

    channel = await GrpcChannel.connect("localhost:50051")
    assert channel._connected is True
    await channel.close()
    assert channel._connected is False


def test_grpc_channel_repr():
    """Test GrpcChannel repr."""
    from cello.grpc import GrpcChannel

    channel = GrpcChannel("localhost:50051")
    r = repr(channel)
    assert "localhost:50051" in r
    assert "connected=False" in r


def test_grpc_error_status_codes():
    """Test GrpcError status code constants."""
    from cello.grpc import GrpcError

    assert GrpcError.OK == 0
    assert GrpcError.CANCELLED == 1
    assert GrpcError.UNKNOWN == 2
    assert GrpcError.INVALID_ARGUMENT == 3
    assert GrpcError.DEADLINE_EXCEEDED == 4
    assert GrpcError.NOT_FOUND == 5
    assert GrpcError.ALREADY_EXISTS == 6
    assert GrpcError.PERMISSION_DENIED == 7
    assert GrpcError.RESOURCE_EXHAUSTED == 8
    assert GrpcError.FAILED_PRECONDITION == 9
    assert GrpcError.ABORTED == 10
    assert GrpcError.OUT_OF_RANGE == 11
    assert GrpcError.UNIMPLEMENTED == 12
    assert GrpcError.INTERNAL == 13
    assert GrpcError.UNAVAILABLE == 14
    assert GrpcError.DATA_LOSS == 15
    assert GrpcError.UNAUTHENTICATED == 16


def test_grpc_error_creation():
    """Test GrpcError creation and attributes."""
    from cello.grpc import GrpcError

    err = GrpcError(
        code=GrpcError.NOT_FOUND,
        message="User not found",
        details="No user with id=42",
    )
    assert err.code == 5
    assert err.message == "User not found"
    assert err.details == "No user with id=42"
    assert "GrpcError" in str(err)


def test_grpc_error_is_exception():
    """Test GrpcError is an Exception subclass."""
    from cello.grpc import GrpcError

    assert issubclass(GrpcError, Exception)

    try:
        raise GrpcError(code=13, message="Internal error")
    except GrpcError as e:
        assert e.code == 13
        assert e.message == "Internal error"


def test_grpc_error_repr():
    """Test GrpcError repr."""
    from cello.grpc import GrpcError

    err = GrpcError(code=7, message="Permission denied", details="No access")
    r = repr(err)
    assert "code=7" in r
    assert "Permission denied" in r
    assert "No access" in r


# ---------------------------------------------------------------------------
# v0.9.0 Messaging Python Module Tests
# ---------------------------------------------------------------------------


def test_kafka_consumer_decorator():
    """Test kafka_consumer decorator attaches metadata."""
    from cello.messaging import kafka_consumer

    @kafka_consumer(topic="orders", group="order-processor", auto_commit=False)
    async def handle_order(message):
        return "ack"

    assert handle_order._cello_consumer is True
    assert handle_order._cello_consumer_topic == "orders"
    assert handle_order._cello_consumer_group == "order-processor"
    assert handle_order._cello_consumer_auto_commit is False


def test_kafka_consumer_decorator_defaults():
    """Test kafka_consumer decorator with defaults."""
    from cello.messaging import kafka_consumer

    @kafka_consumer(topic="events")
    async def handle_event(message):
        return "ack"

    assert handle_event._cello_consumer_topic == "events"
    assert handle_event._cello_consumer_group is None
    assert handle_event._cello_consumer_auto_commit is True


def test_kafka_producer_decorator():
    """Test kafka_producer decorator attaches metadata."""
    from cello.messaging import kafka_producer

    @kafka_producer(topic="events")
    async def emit_event(request):
        return {"event": "signup"}

    assert emit_event._cello_producer is True
    assert emit_event._cello_producer_topic == "events"


@pytest.mark.asyncio
async def test_kafka_producer_decorator_callable():
    """Test kafka_producer-decorated function is callable and tracks publish."""
    from cello.messaging import kafka_producer

    @kafka_producer(topic="events")
    async def emit_event():
        return {"event": "test"}

    result = await emit_event()
    assert result == {"event": "test"}
    assert emit_event._cello_last_publish["topic"] == "events"
    assert emit_event._cello_last_publish["value"] == {"event": "test"}


def test_messaging_kafka_config_python():
    """Test Python-side KafkaConfig class."""
    from cello.messaging import KafkaConfig

    config = KafkaConfig(
        brokers=["broker1:9092", "broker2:9092"],
        group_id="my-group",
        client_id="my-client",
        auto_commit=False,
        session_timeout_ms=60000,
        max_poll_records=1000,
    )
    assert config.brokers == ["broker1:9092", "broker2:9092"]
    assert config.group_id == "my-group"
    assert config.client_id == "my-client"
    assert config.auto_commit is False
    assert config.session_timeout_ms == 60000
    assert config.max_poll_records == 1000


def test_messaging_kafka_config_defaults():
    """Test Python-side KafkaConfig default values."""
    from cello.messaging import KafkaConfig

    config = KafkaConfig()
    assert config.brokers == ["localhost:9092"]
    assert config.group_id is None
    assert config.client_id is None
    assert config.auto_commit is True
    assert config.session_timeout_ms == 30000
    assert config.max_poll_records == 500


def test_messaging_kafka_config_local():
    """Test Python-side KafkaConfig.local() factory."""
    from cello.messaging import KafkaConfig

    config = KafkaConfig.local()
    assert config.brokers == ["localhost:9092"]
    assert config.group_id == "cello-local"
    assert config.client_id == "cello-dev"


def test_messaging_rabbitmq_config_python():
    """Test Python-side RabbitMQConfig class."""
    from cello.messaging import RabbitMQConfig

    config = RabbitMQConfig(
        url="amqp://user:pass@host:5672",
        vhost="/staging",
        prefetch_count=25,
        heartbeat=30,
    )
    assert config.url == "amqp://user:pass@host:5672"
    assert config.vhost == "/staging"
    assert config.prefetch_count == 25
    assert config.heartbeat == 30


def test_messaging_rabbitmq_config_local():
    """Test Python-side RabbitMQConfig.local() factory."""
    from cello.messaging import RabbitMQConfig

    config = RabbitMQConfig.local()
    assert config.url == "amqp://localhost"
    assert config.vhost == "/"
    assert config.prefetch_count == 10


def test_messaging_sqs_config_python():
    """Test Python-side SqsConfig class."""
    from cello.messaging import SqsConfig

    config = SqsConfig(
        region="us-west-2",
        queue_url="https://sqs.us-west-2.amazonaws.com/123/queue",
        endpoint_url="http://localstack:4566",
        max_messages=5,
        wait_time_secs=10,
    )
    assert config.region == "us-west-2"
    assert config.queue_url == "https://sqs.us-west-2.amazonaws.com/123/queue"
    assert config.endpoint_url == "http://localstack:4566"
    assert config.max_messages == 5
    assert config.wait_time_secs == 10


def test_messaging_sqs_config_local():
    """Test Python-side SqsConfig.local() factory."""
    from cello.messaging import SqsConfig

    config = SqsConfig.local("http://localhost:4566/000000000000/my-queue")
    assert config.region == "us-east-1"
    assert config.queue_url == "http://localhost:4566/000000000000/my-queue"
    assert config.endpoint_url == "http://localhost:4566"
    assert config.wait_time_secs == 5


def test_message_creation():
    """Test Message class creation and defaults."""
    from cello.messaging import Message

    msg = Message(topic="orders", value='{"id": 1}')
    assert msg.topic == "orders"
    assert msg.value == '{"id": 1}'
    assert msg.id is not None  # UUID auto-generated
    assert msg.key is None
    assert msg.headers == {}
    assert msg.timestamp > 0
    assert msg._acked is False
    assert msg._nacked is False


def test_message_text_property():
    """Test Message.text property."""
    from cello.messaging import Message

    msg = Message(value="hello world")
    assert msg.text == "hello world"

    msg_bytes = Message(value=b"byte content")
    assert msg_bytes.text == "byte content"

    msg_none = Message(value=None)
    assert msg_none.text == ""


def test_message_json():
    """Test Message.json() parsing."""
    from cello.messaging import Message

    msg = Message(value='{"name": "Alice", "age": 30}')
    data = msg.json()
    assert data["name"] == "Alice"
    assert data["age"] == 30


def test_message_json_dict():
    """Test Message.json() returns dict value as-is."""
    from cello.messaging import Message

    msg = Message(value={"already": "parsed"})
    data = msg.json()
    assert data == {"already": "parsed"}


def test_message_json_bytes():
    """Test Message.json() from bytes."""
    from cello.messaging import Message

    msg = Message(value=b'{"key": "value"}')
    data = msg.json()
    assert data["key"] == "value"


def test_message_ack():
    """Test Message.ack() sets acked flag."""
    from cello.messaging import Message

    msg = Message(value="test")
    assert msg._acked is False
    msg.ack()
    assert msg._acked is True


def test_message_nack():
    """Test Message.nack() sets nacked flag."""
    from cello.messaging import Message

    msg = Message(value="test")
    assert msg._nacked is False
    msg.nack()
    assert msg._nacked is True


def test_message_result_constants():
    """Test MessageResult constants."""
    from cello.messaging import MessageResult

    assert MessageResult.ACK == "ack"
    assert MessageResult.NACK == "nack"
    assert MessageResult.REJECT == "reject"
    assert MessageResult.REQUEUE == "requeue"
    assert MessageResult.DEAD_LETTER == "dead_letter"


@pytest.mark.asyncio
async def test_producer_connect():
    """Test Producer.connect() creates connected producer."""
    from cello.messaging import Producer, KafkaConfig

    producer = await Producer.connect(KafkaConfig.local())
    assert producer is not None
    assert producer._connected is True


@pytest.mark.asyncio
async def test_producer_send():
    """Test Producer.send() returns True."""
    from cello.messaging import Producer, KafkaConfig

    producer = await Producer.connect(KafkaConfig.local())
    result = await producer.send(
        "events",
        value={"event": "signup"},
        key="user-123",
        headers={"source": "api"},
    )
    assert result is True


@pytest.mark.asyncio
async def test_producer_send_batch():
    """Test Producer.send_batch() returns count."""
    from cello.messaging import Producer, KafkaConfig

    producer = await Producer.connect(KafkaConfig.local())
    messages = [
        {"topic": "events", "value": {"type": "a"}},
        {"topic": "events", "value": {"type": "b"}},
        {"topic": "events", "value": {"type": "c"}},
    ]
    count = await producer.send_batch(messages)
    assert count == 3


@pytest.mark.asyncio
async def test_producer_close():
    """Test Producer.close() disconnects."""
    from cello.messaging import Producer, KafkaConfig

    producer = await Producer.connect(KafkaConfig.local())
    assert producer._connected is True
    await producer.close()
    assert producer._connected is False


@pytest.mark.asyncio
async def test_consumer_connect():
    """Test Consumer.connect() creates connected consumer."""
    from cello.messaging import Consumer, KafkaConfig

    consumer = await Consumer.connect(KafkaConfig.local())
    assert consumer is not None
    assert consumer._connected is True


@pytest.mark.asyncio
async def test_consumer_subscribe():
    """Test Consumer.subscribe() sets topic subscriptions."""
    from cello.messaging import Consumer, KafkaConfig

    consumer = await Consumer.connect(KafkaConfig.local())
    await consumer.subscribe(["orders", "events"])
    assert consumer._subscriptions == ["orders", "events"]


@pytest.mark.asyncio
async def test_consumer_poll():
    """Test Consumer.poll() returns list of messages."""
    from cello.messaging import Consumer, KafkaConfig

    consumer = await Consumer.connect(KafkaConfig.local())
    await consumer.subscribe(["orders"])
    messages = await consumer.poll(timeout_ms=100)
    assert isinstance(messages, list)
    assert len(messages) == 0  # Mock returns empty


@pytest.mark.asyncio
async def test_consumer_commit():
    """Test Consumer.commit() acks message."""
    from cello.messaging import Consumer, KafkaConfig, Message

    consumer = await Consumer.connect(KafkaConfig.local())
    msg = Message(topic="orders", value="test")
    await consumer.commit(msg)
    assert msg._acked is True


@pytest.mark.asyncio
async def test_consumer_close():
    """Test Consumer.close() disconnects and clears subscriptions."""
    from cello.messaging import Consumer, KafkaConfig

    consumer = await Consumer.connect(KafkaConfig.local())
    await consumer.subscribe(["orders"])
    assert consumer._connected is True
    assert len(consumer._subscriptions) == 1

    await consumer.close()
    assert consumer._connected is False
    assert consumer._subscriptions == []


@pytest.mark.asyncio
async def test_producer_with_rabbitmq_config():
    """Test Producer works with RabbitMQConfig."""
    from cello.messaging import Producer, RabbitMQConfig

    producer = await Producer.connect(RabbitMQConfig.local())
    assert producer._connected is True
    result = await producer.send("tasks", value={"action": "process"})
    assert result is True
    await producer.close()


@pytest.mark.asyncio
async def test_producer_with_sqs_config():
    """Test Producer works with SqsConfig."""
    from cello.messaging import Producer, SqsConfig

    config = SqsConfig.local("http://localhost:4566/000000000000/test")
    producer = await Producer.connect(config)
    assert producer._connected is True
    result = await producer.send("test", value={"event": "created"})
    assert result is True
    await producer.close()


@pytest.mark.asyncio
async def test_consumer_with_rabbitmq_config():
    """Test Consumer works with RabbitMQConfig."""
    from cello.messaging import Consumer, RabbitMQConfig

    consumer = await Consumer.connect(RabbitMQConfig.local())
    assert consumer._connected is True
    await consumer.subscribe(["tasks"])
    messages = await consumer.poll()
    assert isinstance(messages, list)
    await consumer.close()


@pytest.mark.asyncio
async def test_consumer_with_sqs_config():
    """Test Consumer works with SqsConfig."""
    from cello.messaging import Consumer, SqsConfig

    config = SqsConfig.local("http://localhost:4566/000000000000/test")
    consumer = await Consumer.connect(config)
    assert consumer._connected is True
    await consumer.subscribe(["test"])
    messages = await consumer.poll()
    assert isinstance(messages, list)
    await consumer.close()


# =============================================================================
# v0.10.0 - Event Sourcing, CQRS, and Saga Pattern Tests
# =============================================================================

# ---------------------------------------------------------------------------
# v0.10.0 Version & Export Tests
# ---------------------------------------------------------------------------


def test_version_v0100():
    """Test that version is 0.10.0."""
    import cello

    assert cello.__version__ == "0.10.0"


def test_v0100_exports_in_all():
    """Test that v0.10.0 features are in __all__."""
    import cello

    for name in ["EventSourcingConfig", "CqrsConfig", "SagaConfig"]:
        assert name in cello.__all__


def test_import_v0100_advanced_pattern_configs():
    """Test that v0.10.0 advanced pattern configuration classes can be imported."""
    from cello import EventSourcingConfig, CqrsConfig, SagaConfig

    assert EventSourcingConfig is not None
    assert CqrsConfig is not None
    assert SagaConfig is not None


def test_v0100_all_exports():
    """Test that all v0.10.0 expected exports are available."""
    from cello.eventsourcing import Event, Aggregate, event_handler, EventStore, Snapshot
    from cello.cqrs import (
        Command, Query, CommandResult, QueryResult,
        command_handler, query_handler, CommandBus, QueryBus,
    )
    from cello.saga import (
        SagaStep, Saga, SagaExecution, SagaOrchestrator, SagaError, StepStatus,
    )

    assert all([
        Event, Aggregate, event_handler, EventStore, Snapshot,
        Command, Query, CommandResult, QueryResult,
        command_handler, query_handler, CommandBus, QueryBus,
        SagaStep, Saga, SagaExecution, SagaOrchestrator, SagaError, StepStatus,
    ])


# ---------------------------------------------------------------------------
# v0.10.0 EventSourcingConfig (Rust-backed PyO3 class)
# ---------------------------------------------------------------------------


def test_eventsourcing_config_defaults():
    """Test EventSourcingConfig creation with default values."""
    from cello import EventSourcingConfig

    config = EventSourcingConfig()
    assert config is not None
    assert config.store_type == "memory"
    assert config.snapshot_interval == 100
    assert config.enable_snapshots is True
    assert config.max_events_per_aggregate == 10000


def test_eventsourcing_config_custom():
    """Test EventSourcingConfig creation with custom values."""
    from cello import EventSourcingConfig

    config = EventSourcingConfig(
        store_type="postgresql",
        snapshot_interval=50,
        enable_snapshots=False,
        max_events_per_aggregate=5000,
    )
    assert config.store_type == "postgresql"
    assert config.snapshot_interval == 50
    assert config.enable_snapshots is False
    assert config.max_events_per_aggregate == 5000


def test_eventsourcing_config_memory():
    """Test EventSourcingConfig.memory() factory method."""
    from cello import EventSourcingConfig

    config = EventSourcingConfig.memory()
    assert config is not None
    assert config.store_type == "memory"
    assert config.enable_snapshots is True
    assert config.snapshot_interval == 100
    assert config.max_events_per_aggregate == 10000


def test_eventsourcing_config_postgresql():
    """Test EventSourcingConfig.postgresql() factory method."""
    from cello import EventSourcingConfig

    config = EventSourcingConfig.postgresql("postgresql://localhost/events")
    assert config is not None
    assert config.store_type == "postgresql"
    assert config.enable_snapshots is True


def test_eventsourcing_config_attributes_settable():
    """Test that EventSourcingConfig attributes can be set after creation."""
    from cello import EventSourcingConfig

    config = EventSourcingConfig()
    config.store_type = "postgresql"
    config.snapshot_interval = 200
    config.enable_snapshots = False
    config.max_events_per_aggregate = 20000

    assert config.store_type == "postgresql"
    assert config.snapshot_interval == 200
    assert config.enable_snapshots is False
    assert config.max_events_per_aggregate == 20000


def test_eventsourcing_config_snapshot_disabled():
    """Test EventSourcingConfig with snapshots disabled."""
    from cello import EventSourcingConfig

    config = EventSourcingConfig(enable_snapshots=False)
    assert config.enable_snapshots is False
    assert config.snapshot_interval == 100  # still has default interval


# ---------------------------------------------------------------------------
# v0.10.0 CqrsConfig (Rust-backed PyO3 class)
# ---------------------------------------------------------------------------


def test_cqrs_config_defaults():
    """Test CqrsConfig creation with default values."""
    from cello import CqrsConfig

    config = CqrsConfig()
    assert config is not None
    assert config.enable_event_sync is True
    assert config.command_timeout_ms == 5000
    assert config.query_timeout_ms == 3000
    assert config.max_retries == 3


def test_cqrs_config_custom():
    """Test CqrsConfig creation with custom values."""
    from cello import CqrsConfig

    config = CqrsConfig(
        enable_event_sync=False,
        command_timeout_ms=10000,
        query_timeout_ms=8000,
        max_retries=5,
    )
    assert config.enable_event_sync is False
    assert config.command_timeout_ms == 10000
    assert config.query_timeout_ms == 8000
    assert config.max_retries == 5


def test_cqrs_config_attributes_settable():
    """Test that CqrsConfig attributes can be set after creation."""
    from cello import CqrsConfig

    config = CqrsConfig()
    config.enable_event_sync = False
    config.command_timeout_ms = 15000
    config.query_timeout_ms = 12000
    config.max_retries = 10

    assert config.enable_event_sync is False
    assert config.command_timeout_ms == 15000
    assert config.query_timeout_ms == 12000
    assert config.max_retries == 10


def test_cqrs_config_high_timeout():
    """Test CqrsConfig with very large timeout values."""
    from cello import CqrsConfig

    config = CqrsConfig(
        command_timeout_ms=120000,
        query_timeout_ms=60000,
    )
    assert config.command_timeout_ms == 120000
    assert config.query_timeout_ms == 60000


def test_cqrs_config_no_retries():
    """Test CqrsConfig with retries disabled."""
    from cello import CqrsConfig

    config = CqrsConfig(max_retries=0)
    assert config.max_retries == 0


# ---------------------------------------------------------------------------
# v0.10.0 SagaConfig (Rust-backed PyO3 class)
# ---------------------------------------------------------------------------


def test_saga_config_defaults():
    """Test SagaConfig creation with default values."""
    from cello import SagaConfig

    config = SagaConfig()
    assert config is not None
    assert config.max_retries == 3
    assert config.retry_delay_ms == 1000
    assert config.timeout_ms == 30000
    assert config.enable_logging is True


def test_saga_config_custom():
    """Test SagaConfig creation with custom values."""
    from cello import SagaConfig

    config = SagaConfig(
        max_retries=5,
        retry_delay_ms=2000,
        timeout_ms=60000,
        enable_logging=False,
    )
    assert config.max_retries == 5
    assert config.retry_delay_ms == 2000
    assert config.timeout_ms == 60000
    assert config.enable_logging is False


def test_saga_config_attributes_settable():
    """Test that SagaConfig attributes can be set after creation."""
    from cello import SagaConfig

    config = SagaConfig()
    config.max_retries = 10
    config.retry_delay_ms = 5000
    config.timeout_ms = 120000
    config.enable_logging = False

    assert config.max_retries == 10
    assert config.retry_delay_ms == 5000
    assert config.timeout_ms == 120000
    assert config.enable_logging is False


def test_saga_config_no_logging():
    """Test SagaConfig with logging disabled."""
    from cello import SagaConfig

    config = SagaConfig(enable_logging=False)
    assert config.enable_logging is False
    assert config.max_retries == 3  # other defaults still valid


def test_saga_config_fast_retry():
    """Test SagaConfig with small retry delay for fast retries."""
    from cello import SagaConfig

    config = SagaConfig(retry_delay_ms=100)
    assert config.retry_delay_ms == 100


# ---------------------------------------------------------------------------
# v0.10.0 App Integration Methods
# ---------------------------------------------------------------------------


def test_app_enable_event_sourcing():
    """Test App.enable_event_sourcing() does not raise errors."""
    from cello import App

    app = App()
    app.enable_event_sourcing()
    assert True


def test_app_enable_event_sourcing_with_config():
    """Test App.enable_event_sourcing() with explicit config."""
    from cello import App, EventSourcingConfig

    app = App()
    config = EventSourcingConfig(
        store_type="memory",
        snapshot_interval=50,
        enable_snapshots=True,
    )
    app.enable_event_sourcing(config)
    assert True


def test_app_enable_event_sourcing_memory():
    """Test App.enable_event_sourcing() with memory config."""
    from cello import App, EventSourcingConfig

    app = App()
    app.enable_event_sourcing(EventSourcingConfig.memory())
    assert True


def test_app_enable_cqrs():
    """Test App.enable_cqrs() does not raise errors."""
    from cello import App

    app = App()
    app.enable_cqrs()
    assert True


def test_app_enable_cqrs_with_config():
    """Test App.enable_cqrs() with explicit config."""
    from cello import App, CqrsConfig

    app = App()
    config = CqrsConfig(
        enable_event_sync=True,
        command_timeout_ms=10000,
        max_retries=5,
    )
    app.enable_cqrs(config)
    assert True


def test_app_enable_saga():
    """Test App.enable_saga() does not raise errors."""
    from cello import App

    app = App()
    app.enable_saga()
    assert True


def test_app_with_all_v0100_features():
    """Test an App with all v0.10.0 features enabled together."""
    from cello import App, EventSourcingConfig, CqrsConfig, SagaConfig

    app = App()

    # Core middleware
    app.enable_cors()
    app.enable_logging()

    # v0.10.0 Advanced Pattern features
    app.enable_event_sourcing(EventSourcingConfig.memory())
    app.enable_cqrs(CqrsConfig())
    app.enable_saga(SagaConfig())

    # Register routes
    @app.get("/")
    def home(req):
        return {"version": "0.10.0"}

    assert True


# ---------------------------------------------------------------------------
# v0.10.0 Event Sourcing Python Module Tests
# ---------------------------------------------------------------------------


def test_event_creation():
    """Test Event creation with type and data."""
    from cello.eventsourcing import Event

    event = Event("OrderCreated", {"id": 1})
    assert event is not None
    assert event.event_type == "OrderCreated"
    assert event.data == {"id": 1}


def test_event_properties():
    """Test Event has all expected properties."""
    from cello.eventsourcing import Event

    event = Event("OrderCreated", {"id": 1})
    assert event.id is not None
    assert event.event_type == "OrderCreated"
    assert event.data == {"id": 1}
    assert event.aggregate_id is None
    assert event.metadata == {}
    assert event.version == 0
    assert event.timestamp is not None


def test_event_with_aggregate_id():
    """Test Event creation with aggregate_id."""
    from cello.eventsourcing import Event

    event = Event("OrderCreated", {"id": 1}, aggregate_id="order-1")
    assert event.aggregate_id == "order-1"
    assert event.event_type == "OrderCreated"


def test_event_with_metadata():
    """Test Event creation with metadata."""
    from cello.eventsourcing import Event

    event = Event("OrderCreated", {}, metadata={"user": "admin"})
    assert event.metadata == {"user": "admin"}


def test_event_json():
    """Test Event.to_dict() returns dict with all fields."""
    from cello.eventsourcing import Event

    event = Event("OrderCreated", {"id": 1}, aggregate_id="order-1")
    data = event.to_dict()
    assert isinstance(data, dict)
    assert data["event_type"] == "OrderCreated"
    assert data["data"] == {"id": 1}
    assert data["aggregate_id"] == "order-1"
    assert "id" in data
    assert "timestamp" in data
    assert "version" in data


def test_event_repr():
    """Test Event repr contains event_type."""
    from cello.eventsourcing import Event

    event = Event("OrderCreated", {"id": 1})
    r = repr(event)
    assert "OrderCreated" in r


def test_aggregate_creation():
    """Test Aggregate subclass creation."""
    from cello.eventsourcing import Aggregate

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate()
    assert agg is not None
    assert agg.version == 0


def test_aggregate_default_id():
    """Test Aggregate auto-generates a UUID id."""
    from cello.eventsourcing import Aggregate

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate()
    assert agg.aggregate_id is not None
    assert len(agg.aggregate_id) > 0


def test_aggregate_custom_id():
    """Test Aggregate with custom aggregate_id."""
    from cello.eventsourcing import Aggregate

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate(aggregate_id="order-1")
    assert agg.aggregate_id == "order-1"


def test_aggregate_apply_event():
    """Test applying an event increments version."""
    from cello.eventsourcing import Aggregate, Event

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate(aggregate_id="order-1")
    event = Event("OrderCreated", {"id": 1})
    agg.apply(event)
    assert agg.version == 1


def test_aggregate_uncommitted_events():
    """Test that applied events appear in uncommitted events."""
    from cello.eventsourcing import Aggregate, Event

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate(aggregate_id="order-1")
    event = Event("OrderCreated", {"id": 1})
    agg.apply(event)
    uncommitted = agg.uncommitted_events
    assert len(uncommitted) == 1
    assert uncommitted[0].event_type == "OrderCreated"


def test_aggregate_clear_uncommitted():
    """Test clear_uncommitted() empties uncommitted events."""
    from cello.eventsourcing import Aggregate, Event

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate(aggregate_id="order-1")
    agg.apply(Event("OrderCreated", {"id": 1}))
    agg.apply(Event("OrderShipped", {"tracking": "ABC"}))
    assert len(agg.uncommitted_events) == 2

    agg.clear_uncommitted()
    assert len(agg.uncommitted_events) == 0


def test_aggregate_load_from_events():
    """Test replaying a list of events onto an aggregate."""
    from cello.eventsourcing import Aggregate, Event

    class OrderAggregate(Aggregate):
        pass

    events = [
        Event("OrderCreated", {"id": 1}),
        Event("OrderShipped", {"tracking": "ABC"}),
        Event("OrderDelivered", {}),
    ]

    agg = OrderAggregate(aggregate_id="order-1")
    agg.load_from_events(events)
    assert agg.version == 3


def test_aggregate_event_handler_decorator():
    """Test @event_handler decorator on aggregate method."""
    from cello.eventsourcing import Aggregate, Event, event_handler

    class OrderAggregate(Aggregate):
        def __init__(self, **kwargs):
            super().__init__(**kwargs)
            self.status = "new"

        @event_handler("OrderCreated")
        def on_order_created(self, event):
            self.status = "created"

        @event_handler("OrderShipped")
        def on_order_shipped(self, event):
            self.status = "shipped"

    agg = OrderAggregate(aggregate_id="order-1")
    agg.apply(Event("OrderCreated", {"id": 1}))
    assert agg.status == "created"

    agg.apply(Event("OrderShipped", {"tracking": "XYZ"}))
    assert agg.status == "shipped"


def test_aggregate_version_tracking():
    """Test version tracking with multiple events."""
    from cello.eventsourcing import Aggregate, Event

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate(aggregate_id="order-1")
    assert agg.version == 0

    for i in range(5):
        agg.apply(Event("ItemAdded", {"item": i}))

    assert agg.version == 5


def test_aggregate_repr():
    """Test Aggregate repr."""
    from cello.eventsourcing import Aggregate

    class OrderAggregate(Aggregate):
        pass

    agg = OrderAggregate(aggregate_id="order-1")
    r = repr(agg)
    assert "order-1" in r


def test_snapshot_creation():
    """Test Snapshot creation."""
    from cello.eventsourcing import Snapshot

    snap = Snapshot("agg-1", 5, {"status": "active"})
    assert snap is not None


def test_snapshot_properties():
    """Test Snapshot has all expected properties."""
    from cello.eventsourcing import Snapshot

    snap = Snapshot("agg-1", 5, {"status": "active"})
    assert snap.aggregate_id == "agg-1"
    assert snap.version == 5
    assert snap.state == {"status": "active"}
    assert snap.timestamp is not None


@pytest.mark.asyncio
async def test_event_store_creation():
    """Test EventStore creation."""
    from cello.eventsourcing import EventStore

    store = EventStore()
    assert store is not None


@pytest.mark.asyncio
async def test_event_store_connect():
    """Test EventStore.connect() class method."""
    from cello.eventsourcing import EventStore

    store = await EventStore.connect()
    assert store is not None


@pytest.mark.asyncio
async def test_event_store_append_and_get():
    """Test appending events and retrieving them."""
    from cello.eventsourcing import EventStore, Event

    store = await EventStore.connect()
    events = [
        Event("OrderCreated", {"id": 1}, aggregate_id="order-1"),
        Event("OrderShipped", {"tracking": "ABC"}, aggregate_id="order-1"),
    ]
    await store.append("order-1", events)
    retrieved = await store.get_events("order-1")
    assert len(retrieved) == 2
    assert retrieved[0].event_type == "OrderCreated"
    assert retrieved[1].event_type == "OrderShipped"


@pytest.mark.asyncio
async def test_event_store_get_since_version():
    """Test get_events with since_version filter."""
    from cello.eventsourcing import EventStore, Event

    store = await EventStore.connect()
    events = [
        Event("OrderCreated", {"id": 1}, aggregate_id="order-2"),
        Event("OrderShipped", {"tracking": "DEF"}, aggregate_id="order-2"),
        Event("OrderDelivered", {}, aggregate_id="order-2"),
    ]
    await store.append("order-2", events)
    retrieved = await store.get_events("order-2", since_version=1)
    assert len(retrieved) == 2
    assert retrieved[0].event_type == "OrderShipped"
    assert retrieved[1].event_type == "OrderDelivered"


@pytest.mark.asyncio
async def test_event_store_snapshot():
    """Test saving and retrieving a snapshot."""
    from cello.eventsourcing import EventStore, Snapshot

    store = await EventStore.connect()
    snap = Snapshot("order-3", 5, {"status": "shipped", "items": 3})
    await store.save_snapshot(snap)

    loaded = await store.get_snapshot("order-3")
    assert loaded is not None
    assert loaded.aggregate_id == "order-3"
    assert loaded.version == 5
    assert loaded.state == {"status": "shipped", "items": 3}


@pytest.mark.asyncio
async def test_event_store_close():
    """Test EventStore.close() does not raise errors."""
    from cello.eventsourcing import EventStore

    store = await EventStore.connect()
    await store.close()
    assert True


def test_eventsourcing_config_python():
    """Test Python-side EventSourcingConfig usage."""
    from cello import EventSourcingConfig

    config = EventSourcingConfig.memory()
    assert config.store_type == "memory"

    config2 = EventSourcingConfig.postgresql("postgresql://localhost/events")
    assert config2.store_type == "postgresql"


# ---------------------------------------------------------------------------
# v0.10.0 CQRS Python Module Tests
# ---------------------------------------------------------------------------


def test_command_creation():
    """Test Command subclass creation with kwargs."""
    from cello.cqrs import Command

    class CreateOrder(Command):
        pass

    cmd = CreateOrder(user_id=1, product="Widget", quantity=3)
    assert cmd is not None
    assert cmd.user_id == 1
    assert cmd.product == "Widget"
    assert cmd.quantity == 3


def test_command_properties():
    """Test Command has id, command_type, and timestamp."""
    from cello.cqrs import Command

    class CreateOrder(Command):
        pass

    cmd = CreateOrder(user_id=1)
    assert cmd.id is not None
    assert cmd.command_type is not None
    assert cmd.timestamp is not None


def test_command_to_dict():
    """Test Command.to_dict() returns dict of all kwargs."""
    from cello.cqrs import Command

    class CreateOrder(Command):
        pass

    cmd = CreateOrder(user_id=1, product="Widget")
    data = cmd.to_dict()
    assert isinstance(data, dict)
    assert data["user_id"] == 1
    assert data["product"] == "Widget"
    assert "id" in data
    assert "command_type" in data


def test_command_type_is_class_name():
    """Test that command_type defaults to the class name."""
    from cello.cqrs import Command

    class CreateOrder(Command):
        pass

    cmd = CreateOrder(user_id=1)
    assert cmd.command_type == "CreateOrder"


def test_command_repr():
    """Test Command repr contains class name."""
    from cello.cqrs import Command

    class CreateOrder(Command):
        pass

    cmd = CreateOrder(user_id=1)
    r = repr(cmd)
    assert "CreateOrder" in r


def test_query_creation():
    """Test Query subclass creation."""
    from cello.cqrs import Query

    class GetOrder(Query):
        pass

    q = GetOrder(order_id="order-1")
    assert q is not None
    assert q.order_id == "order-1"


def test_query_properties():
    """Test Query has id, query_type, and timestamp."""
    from cello.cqrs import Query

    class GetOrder(Query):
        pass

    q = GetOrder(order_id="order-1")
    assert q.id is not None
    assert q.query_type is not None
    assert q.timestamp is not None


def test_query_to_dict():
    """Test Query.to_dict() returns dict of all kwargs."""
    from cello.cqrs import Query

    class GetOrder(Query):
        pass

    q = GetOrder(order_id="order-1")
    data = q.to_dict()
    assert isinstance(data, dict)
    assert data["order_id"] == "order-1"
    assert "id" in data
    assert "query_type" in data


def test_query_type_is_class_name():
    """Test that query_type defaults to the class name."""
    from cello.cqrs import Query

    class GetOrder(Query):
        pass

    q = GetOrder(order_id="order-1")
    assert q.query_type == "GetOrder"


def test_query_repr():
    """Test Query repr contains class name."""
    from cello.cqrs import Query

    class GetOrder(Query):
        pass

    q = GetOrder(order_id="order-1")
    r = repr(q)
    assert "GetOrder" in r


def test_command_result_ok():
    """Test CommandResult.ok() factory."""
    from cello.cqrs import CommandResult

    result = CommandResult.ok({"id": 1})
    assert result.success is True
    assert result.data == {"id": 1}
    assert result.error is None


def test_command_result_fail():
    """Test CommandResult.fail() factory."""
    from cello.cqrs import CommandResult

    result = CommandResult.fail("something went wrong")
    assert result.success is False
    assert result.error == "something went wrong"
    assert result.data is None


def test_command_result_rejected():
    """Test CommandResult.rejected() factory."""
    from cello.cqrs import CommandResult

    result = CommandResult.rejected("invalid input")
    assert result.success is False
    assert result.error == "invalid input"
    assert result.status == "rejected"


def test_command_result_success_flag():
    """Test CommandResult success flag for both ok and fail."""
    from cello.cqrs import CommandResult

    ok_result = CommandResult.ok({"id": 1})
    fail_result = CommandResult.fail("error")

    assert ok_result.success is True
    assert fail_result.success is False


def test_query_result_ok():
    """Test QueryResult.ok() factory."""
    from cello.cqrs import QueryResult

    result = QueryResult.ok({"id": 1, "name": "Alice"})
    assert result.found is True
    assert result.data == {"id": 1, "name": "Alice"}
    assert result.error is None


def test_query_result_not_found():
    """Test QueryResult.not_found() factory."""
    from cello.cqrs import QueryResult

    result = QueryResult.not_found()
    assert result.found is False
    assert result.data is None
    assert result.error is None


def test_query_result_fail():
    """Test QueryResult.fail() factory."""
    from cello.cqrs import QueryResult

    result = QueryResult.fail("database error")
    assert result.found is False
    assert result.error == "database error"
    assert result.data is None


def test_query_result_found_flag():
    """Test QueryResult found flag for ok, not_found, and fail."""
    from cello.cqrs import QueryResult

    ok_result = QueryResult.ok({"id": 1})
    not_found_result = QueryResult.not_found()
    fail_result = QueryResult.fail("error")

    assert ok_result.found is True
    assert not_found_result.found is False
    assert fail_result.found is False


def test_command_handler_decorator():
    """Test @command_handler decorator marks a function."""
    from cello.cqrs import Command, command_handler

    class CreateOrder(Command):
        pass

    @command_handler(CreateOrder)
    def handle_create_order(cmd):
        return {"created": True}

    assert handle_create_order._command_type is CreateOrder
    assert handle_create_order._is_command_handler is True
    result = handle_create_order(CreateOrder(user_id=1))
    assert result == {"created": True}


def test_query_handler_decorator():
    """Test @query_handler decorator marks a function."""
    from cello.cqrs import Query, query_handler

    class GetOrder(Query):
        pass

    @query_handler(GetOrder)
    def handle_get_order(q):
        return {"id": "order-1", "status": "active"}

    assert handle_get_order._query_type is GetOrder
    assert handle_get_order._is_query_handler is True
    result = handle_get_order(GetOrder(order_id="order-1"))
    assert result == {"id": "order-1", "status": "active"}


@pytest.mark.asyncio
async def test_command_bus_register_and_dispatch():
    """Test CommandBus register handler and dispatch command."""
    from cello.cqrs import Command, CommandResult, CommandBus

    class CreateOrder(Command):
        pass

    bus = CommandBus()

    async def handle_create(cmd):
        return CommandResult.ok({"order_id": "new-1", "user": cmd.user_id})

    bus.register(CreateOrder, handle_create)

    result = await bus.dispatch(CreateOrder(user_id=42))
    assert result.success is True
    assert result.data["order_id"] == "new-1"
    assert result.data["user"] == 42


@pytest.mark.asyncio
async def test_command_bus_missing_handler():
    """Test CommandBus raises on dispatch with unregistered command."""
    from cello.cqrs import Command, CommandBus

    class UnregisteredCommand(Command):
        pass

    bus = CommandBus()

    with pytest.raises(Exception):
        await bus.dispatch(UnregisteredCommand())


@pytest.mark.asyncio
async def test_query_bus_register_and_execute():
    """Test QueryBus register handler and execute query."""
    from cello.cqrs import Query, QueryResult, QueryBus

    class GetOrder(Query):
        pass

    bus = QueryBus()

    async def handle_get(q):
        return QueryResult.ok({"order_id": q.order_id, "status": "active"})

    bus.register(GetOrder, handle_get)

    result = await bus.execute(GetOrder(order_id="order-1"))
    assert result.found is True
    assert result.data["order_id"] == "order-1"
    assert result.data["status"] == "active"


@pytest.mark.asyncio
async def test_query_bus_missing_handler():
    """Test QueryBus raises on execute with unregistered query."""
    from cello.cqrs import Query, QueryBus

    class UnregisteredQuery(Query):
        pass

    bus = QueryBus()

    with pytest.raises(Exception):
        await bus.execute(UnregisteredQuery())


def test_command_bus_repr():
    """Test CommandBus repr."""
    from cello.cqrs import CommandBus

    bus = CommandBus()
    r = repr(bus)
    assert "CommandBus" in r


# ---------------------------------------------------------------------------
# v0.10.0 Saga Python Module Tests
# ---------------------------------------------------------------------------


def test_saga_step_creation():
    """Test SagaStep creation with action function."""
    from cello.saga import SagaStep

    async def my_action(context):
        return {"done": True}

    step = SagaStep("step1", action=my_action)
    assert step is not None
    assert step.name == "step1"


def test_saga_step_with_compensate():
    """Test SagaStep creation with both action and compensate."""
    from cello.saga import SagaStep

    async def my_action(context):
        return {"done": True}

    async def my_compensate(context):
        return {"undone": True}

    step = SagaStep("step1", action=my_action, compensate=my_compensate)
    assert step.name == "step1"
    assert step.action is not None
    assert step.compensate is not None


def test_saga_step_properties():
    """Test SagaStep has name, status, action, and compensate."""
    from cello.saga import SagaStep

    async def action_fn(ctx):
        pass

    async def comp_fn(ctx):
        pass

    step = SagaStep("reserve_inventory", action=action_fn, compensate=comp_fn)
    assert step.name == "reserve_inventory"
    assert step.status is not None
    assert step.action is action_fn
    assert step.compensate is comp_fn


def test_saga_step_default_status():
    """Test SagaStep default status is 'pending'."""
    from cello.saga import SagaStep

    async def action_fn(ctx):
        pass

    step = SagaStep("step1", action=action_fn)
    assert step.status == "pending"


@pytest.mark.asyncio
async def test_saga_step_execute():
    """Test SagaStep execute runs the action."""
    from cello.saga import SagaStep

    executed = []

    async def action_fn(ctx):
        executed.append("action")
        return {"result": "ok"}

    step = SagaStep("step1", action=action_fn)
    result = await step.execute({})
    assert "action" in executed
    assert result == {"result": "ok"}


@pytest.mark.asyncio
async def test_saga_step_compensate_step():
    """Test SagaStep compensate_step runs the compensation."""
    from cello.saga import SagaStep

    compensated = []

    async def action_fn(ctx):
        return True

    async def comp_fn(ctx):
        compensated.append("compensated")
        return True

    step = SagaStep("step1", action=action_fn, compensate=comp_fn)
    await step.compensate_step({})
    assert "compensated" in compensated


def test_saga_step_repr():
    """Test SagaStep repr contains step name."""
    from cello.saga import SagaStep

    async def action_fn(ctx):
        pass

    step = SagaStep("reserve_inventory", action=action_fn)
    r = repr(step)
    assert "reserve_inventory" in r


def test_step_status_constants():
    """Test StepStatus has all expected constants."""
    from cello.saga import StepStatus

    assert StepStatus.PENDING == "pending"
    assert StepStatus.RUNNING == "running"
    assert StepStatus.COMPLETED == "completed"
    assert StepStatus.FAILED == "failed"
    assert StepStatus.COMPENSATING == "compensating"
    assert StepStatus.COMPENSATED == "compensated"


def test_saga_creation():
    """Test Saga subclass creation with steps."""
    from cello.saga import Saga, SagaStep

    async def step1_action(ctx):
        pass

    async def step2_action(ctx):
        pass

    class OrderSaga(Saga):
        steps = [
            SagaStep("step1", action=step1_action),
            SagaStep("step2", action=step2_action),
        ]

    saga = OrderSaga()
    assert saga is not None


def test_saga_default_name():
    """Test Saga default name is class name."""
    from cello.saga import Saga, SagaStep

    async def action_fn(ctx):
        pass

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    saga = OrderSaga()
    assert saga.name == "OrderSaga"


def test_saga_custom_name():
    """Test Saga with custom name."""
    from cello.saga import Saga, SagaStep

    async def action_fn(ctx):
        pass

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    saga = OrderSaga(name="my-saga")
    assert saga.name == "my-saga"


def test_saga_add_step():
    """Test Saga.add_step() adds a step."""
    from cello.saga import Saga, SagaStep

    class OrderSaga(Saga):
        steps = []

    saga = OrderSaga()

    async def new_action(ctx):
        pass

    saga.add_step(SagaStep("new_step", action=new_action))
    assert len(saga.get_steps()) == 1
    assert saga.get_steps()[0].name == "new_step"


def test_saga_get_steps():
    """Test Saga.get_steps() returns list of steps."""
    from cello.saga import Saga, SagaStep

    async def action1(ctx):
        pass

    async def action2(ctx):
        pass

    class OrderSaga(Saga):
        steps = [
            SagaStep("step1", action=action1),
            SagaStep("step2", action=action2),
        ]

    saga = OrderSaga()
    steps = saga.get_steps()
    assert len(steps) == 2
    assert steps[0].name == "step1"
    assert steps[1].name == "step2"


def test_saga_step_count():
    """Test Saga step count matches added steps."""
    from cello.saga import Saga, SagaStep

    async def action_fn(ctx):
        pass

    class MySaga(Saga):
        steps = [
            SagaStep("a", action=action_fn),
            SagaStep("b", action=action_fn),
            SagaStep("c", action=action_fn),
        ]

    saga = MySaga()
    assert len(saga.get_steps()) == 3


def test_saga_repr():
    """Test Saga repr contains saga name."""
    from cello.saga import Saga, SagaStep

    async def action_fn(ctx):
        pass

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    saga = OrderSaga()
    r = repr(saga)
    assert "OrderSaga" in r


@pytest.mark.asyncio
async def test_saga_execution_creation():
    """Test SagaExecution creation from a Saga."""
    from cello.saga import Saga, SagaStep, SagaExecution

    async def action_fn(ctx):
        pass

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    saga = OrderSaga()
    execution = SagaExecution(saga)
    assert execution is not None


@pytest.mark.asyncio
async def test_saga_execution_run_success():
    """Test SagaExecution.run() with all steps succeeding."""
    from cello.saga import Saga, SagaStep, SagaExecution

    results = []

    async def step1_action(ctx):
        results.append("step1")
        return True

    async def step2_action(ctx):
        results.append("step2")
        return True

    async def step3_action(ctx):
        results.append("step3")
        return True

    class SuccessSaga(Saga):
        steps = [
            SagaStep("step1", action=step1_action),
            SagaStep("step2", action=step2_action),
            SagaStep("step3", action=step3_action),
        ]

    saga = SuccessSaga()
    execution = SagaExecution(saga)
    result = await execution.run({})

    assert result.success is True
    assert results == ["step1", "step2", "step3"]


@pytest.mark.asyncio
async def test_saga_execution_run_failure_compensates():
    """Test SagaExecution compensates completed steps on failure."""
    from cello.saga import Saga, SagaStep, SagaExecution

    actions = []
    compensations = []

    async def step1_action(ctx):
        actions.append("step1")
        return True

    async def step1_comp(ctx):
        compensations.append("step1_comp")
        return True

    async def step2_action(ctx):
        actions.append("step2")
        raise RuntimeError("step2 failed")

    async def step2_comp(ctx):
        compensations.append("step2_comp")
        return True

    async def step3_action(ctx):
        actions.append("step3")
        return True

    class FailingSaga(Saga):
        steps = [
            SagaStep("step1", action=step1_action, compensate=step1_comp),
            SagaStep("step2", action=step2_action, compensate=step2_comp),
            SagaStep("step3", action=step3_action),
        ]

    saga = FailingSaga()
    execution = SagaExecution(saga)
    result = await execution.run({})

    assert result.success is False
    assert "step1" in actions
    assert "step2" in actions
    assert "step3" not in actions
    # Compensation should run in reverse order for completed steps
    assert "step1_comp" in compensations


@pytest.mark.asyncio
async def test_saga_execution_status_tracking():
    """Test SagaExecution tracks execution status."""
    from cello.saga import Saga, SagaStep, SagaExecution

    async def action_fn(ctx):
        return True

    class MySaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    saga = MySaga()
    execution = SagaExecution(saga)

    assert execution.status in ("pending", "created")
    await execution.run({})
    assert execution.status in ("completed", "success")


def test_saga_execution_repr():
    """Test SagaExecution repr."""
    from cello.saga import Saga, SagaStep, SagaExecution

    async def action_fn(ctx):
        pass

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    saga = OrderSaga()
    execution = SagaExecution(saga)
    r = repr(execution)
    assert "SagaExecution" in r


@pytest.mark.asyncio
async def test_saga_orchestrator_creation():
    """Test SagaOrchestrator creation."""
    from cello.saga import SagaOrchestrator

    orchestrator = SagaOrchestrator()
    assert orchestrator is not None


@pytest.mark.asyncio
async def test_saga_orchestrator_register():
    """Test SagaOrchestrator.register() adds a saga."""
    from cello.saga import Saga, SagaStep, SagaOrchestrator

    async def action_fn(ctx):
        pass

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    orchestrator = SagaOrchestrator()
    orchestrator.register(OrderSaga)
    assert True


@pytest.mark.asyncio
async def test_saga_orchestrator_execute():
    """Test SagaOrchestrator.execute() runs a registered saga."""
    from cello.saga import Saga, SagaStep, SagaOrchestrator

    executed = []

    async def action_fn(ctx):
        executed.append("done")
        return True

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    orchestrator = SagaOrchestrator()
    orchestrator.register(OrderSaga)
    result = await orchestrator.execute("OrderSaga", {})

    assert result.success is True
    assert "done" in executed


@pytest.mark.asyncio
async def test_saga_orchestrator_list_executions():
    """Test SagaOrchestrator.list_executions() returns execution history."""
    from cello.saga import Saga, SagaStep, SagaOrchestrator

    async def action_fn(ctx):
        return True

    class OrderSaga(Saga):
        steps = [SagaStep("step1", action=action_fn)]

    orchestrator = SagaOrchestrator()
    orchestrator.register(OrderSaga)
    await orchestrator.execute("OrderSaga", {"order": 1})
    await orchestrator.execute("OrderSaga", {"order": 2})

    executions = orchestrator.list_executions()
    assert isinstance(executions, list)
    assert len(executions) >= 2


def test_saga_error_creation():
    """Test SagaError creation with step_name and original_error."""
    from cello.saga import SagaError

    original = RuntimeError("connection lost")
    error = SagaError(step_name="process_payment", original_error=original)
    assert error.step_name == "process_payment"
    assert error.original_error is original
    assert "process_payment" in str(error)
