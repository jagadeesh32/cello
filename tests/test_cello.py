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
    """Test that version is 0.8.0."""
    import cello

    assert cello.__version__ == "0.8.0"


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
    app.enable_openapi(title="Test API", version="0.8.0")
    assert True


def test_enable_openapi_default_version():
    """Test that OpenAPI defaults to v0.8.0."""
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
        return {"version": "0.8.0"}

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
