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
    """Test that version is 0.5.0."""
    import cello

    assert cello.__version__ == "0.5.0"


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
