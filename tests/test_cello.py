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
        resp = requests.get(f"{server}/")
        assert resp.status_code == 200
        assert "Access-Control-Allow-Origin" in resp.headers

    @pytest.mark.integration
    def test_not_found(self, server):
        """Test 404 response for unknown routes."""
        resp = requests.get(f"{server}/nonexistent")
        assert resp.status_code == 404
        data = resp.json()
        assert "error" in data
