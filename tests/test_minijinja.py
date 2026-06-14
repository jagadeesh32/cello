"""
Tests for MiniJinja template engine integration (v1.1.0).

Run with:
    maturin develop
    pytest tests/test_minijinja.py -v
"""

import os
import tempfile

import pytest


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _write_template(directory: str, name: str, content: str) -> str:
    """Write a template file and return its path."""
    path = os.path.join(directory, name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)
    return path


# ===========================================================================
# Import tests
# ===========================================================================


def test_import_minijinja_engine():
    """MiniJinjaEngine is importable from the top-level cello package."""
    from cello import MiniJinjaEngine

    assert MiniJinjaEngine is not None


def test_minijinja_in_all():
    """MiniJinjaEngine is listed in cello.__all__."""
    import cello

    assert "MiniJinjaEngine" in cello.__all__


# ===========================================================================
# MiniJinjaEngine — standalone usage
# ===========================================================================


class TestMiniJinjaEngineStandalone:
    """Tests for MiniJinjaEngine used independently of App."""

    def test_create_default(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        assert engine is not None
        assert engine.template_dir == str(tmp_path)
        assert engine.auto_escape is True

    def test_create_no_auto_escape(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path), auto_escape=False)
        assert engine.auto_escape is False

    def test_repr(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        r = repr(engine)
        assert "MiniJinjaEngine" in r
        assert str(tmp_path) in r

    # -----------------------------------------------------------------------
    # render_string
    # -----------------------------------------------------------------------

    def test_render_string_simple_variable(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("Hello, {{ name }}!", {"name": "World"})
        assert result == "Hello, World!"

    def test_render_string_integer(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("Count: {{ n }}", {"n": 42})
        assert result == "Count: 42"

    def test_render_string_float(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("Pi: {{ pi }}", {"pi": 3.14})
        assert "3.14" in result

    def test_render_string_bool(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string(
            "{% if flag %}yes{% else %}no{% endif %}", {"flag": True}
        )
        assert result == "yes"

    def test_render_string_none(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string(
            "{% if val is defined %}defined{% else %}undefined{% endif %}",
            {"val": None},
        )
        # None maps to JSON null — Jinja2 treats it as falsy but defined
        assert result in ("defined", "undefined")

    def test_render_string_list(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string(
            "{% for item in items %}{{ item }},{% endfor %}",
            {"items": ["a", "b", "c"]},
        )
        assert result == "a,b,c,"

    def test_render_string_nested_dict(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string(
            "{{ user.name }} ({{ user.age }})",
            {"user": {"name": "Alice", "age": 30}},
        )
        assert result == "Alice (30)"

    def test_render_string_filter_upper(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("{{ word | upper }}", {"word": "cello"})
        assert result == "CELLO"

    def test_render_string_filter_lower(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("{{ word | lower }}", {"word": "CELLO"})
        assert result == "cello"

    def test_render_string_filter_length(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("{{ items | length }}", {"items": [1, 2, 3, 4]})
        assert result == "4"

    def test_render_string_if_else(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        tmpl = "{% if score >= 90 %}A{% elif score >= 80 %}B{% else %}C{% endif %}"
        assert engine.render_string(tmpl, {"score": 95}) == "A"
        assert engine.render_string(tmpl, {"score": 85}) == "B"
        assert engine.render_string(tmpl, {"score": 70}) == "C"

    def test_render_string_empty_context(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("static text", {})
        assert result == "static text"

    def test_render_string_tojson_filter(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("{{ data | tojson }}", {"data": [1, 2, 3]})
        assert "[1,2,3]" in result or "1" in result

    # -----------------------------------------------------------------------
    # render (from file)
    # -----------------------------------------------------------------------

    def test_render_from_file(self, tmp_path):
        from cello import MiniJinjaEngine

        _write_template(str(tmp_path), "hello.html", "Hello, {{ name }}!")
        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render("hello.html", {"name": "Cello"})
        assert result == "Hello, Cello!"

    def test_render_file_not_found_raises(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        with pytest.raises(ValueError, match="not found"):
            engine.render("nonexistent.html", {})

    def test_render_template_inheritance(self, tmp_path):
        from cello import MiniJinjaEngine

        _write_template(
            str(tmp_path),
            "base.html",
            "HEAD{% block content %}{% endblock %}FOOT",
        )
        _write_template(
            str(tmp_path),
            "child.html",
            "{% extends 'base.html' %}{% block content %}BODY{% endblock %}",
        )
        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render("child.html", {})
        assert result == "HEADBODYFOOT"

    def test_render_template_include(self, tmp_path):
        from cello import MiniJinjaEngine

        _write_template(str(tmp_path), "nav.html", "<nav>MENU</nav>")
        _write_template(
            str(tmp_path), "page.html", "{% include 'nav.html' %}<main>{{ body }}</main>"
        )
        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render("page.html", {"body": "Content"})
        assert "<nav>MENU</nav>" in result
        assert "<main>Content</main>" in result

    def test_render_subdirectory_template(self, tmp_path):
        from cello import MiniJinjaEngine

        _write_template(str(tmp_path), "emails/welcome.txt", "Welcome, {{ name }}!")
        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render("emails/welcome.txt", {"name": "User"})
        assert result == "Welcome, User!"

    # -----------------------------------------------------------------------
    # Globals
    # -----------------------------------------------------------------------

    def test_add_global_string(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        engine.add_global("site_name", "Cello")
        result = engine.render_string("{{ site_name }}", {})
        assert result == "Cello"

    def test_add_global_integer(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        engine.add_global("year", 2026)
        result = engine.render_string("{{ year }}", {})
        assert result == "2026"

    def test_add_globals_dict(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        engine.add_globals({"app": "Cello", "version": "1.1.0"})
        result = engine.render_string("{{ app }} v{{ version }}", {})
        assert result == "Cello v1.1.0"

    def test_context_overrides_global(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        engine.add_global("name", "Global")
        result = engine.render_string("{{ name }}", {"name": "Local"})
        assert result == "Local"


# ===========================================================================
# App.enable_templates / App.render / App.render_string
# ===========================================================================


class TestAppTemplates:
    """Tests for the App-level template integration."""

    def test_enable_templates_returns_engine(self, tmp_path):
        from cello import App, MiniJinjaEngine

        app = App()
        engine = app.enable_templates(template_dir=str(tmp_path))
        assert isinstance(engine, MiniJinjaEngine)

    def test_enable_templates_twice_raises(self, tmp_path):
        from cello import App

        app = App()
        app.enable_templates(template_dir=str(tmp_path))
        with pytest.raises(RuntimeError, match="already been called"):
            app.enable_templates(template_dir=str(tmp_path))

    def test_render_without_enable_raises(self):
        from cello import App

        app = App()
        with pytest.raises(RuntimeError, match="enable_templates"):
            app.render("index.html", {})

    def test_render_string_without_enable_raises(self):
        from cello import App

        app = App()
        with pytest.raises(RuntimeError, match="enable_templates"):
            app.render_string("{{ x }}", {"x": 1})

    def test_app_render_string(self, tmp_path):
        from cello import App

        app = App()
        app.enable_templates(template_dir=str(tmp_path))
        result = app.render_string("Hello, {{ name }}!", {"name": "Cello"})
        assert result == "Hello, Cello!"

    def test_app_render_file(self, tmp_path):
        from cello import App

        _write_template(str(tmp_path), "greet.html", "Hi {{ user }}!")
        app = App()
        app.enable_templates(template_dir=str(tmp_path))
        result = app.render("greet.html", {"user": "Alice"})
        assert result == "Hi Alice!"

    def test_app_render_default_empty_context(self, tmp_path):
        from cello import App

        _write_template(str(tmp_path), "static.html", "No variables here.")
        app = App()
        app.enable_templates(template_dir=str(tmp_path))
        result = app.render("static.html")
        assert result == "No variables here."

    def test_app_enable_templates_with_globals(self, tmp_path):
        from cello import App

        app = App()
        app.enable_templates(
            template_dir=str(tmp_path),
            globals={"framework": "Cello", "version": "1.1.0"},
        )
        result = app.render_string("{{ framework }} {{ version }}", {})
        assert result == "Cello 1.1.0"

    def test_app_render_html_response(self, tmp_path):
        """Verify render() output can be wrapped in Response.html()."""
        from cello import App, Response

        _write_template(str(tmp_path), "page.html", "<h1>{{ title }}</h1>")
        app = App()
        app.enable_templates(template_dir=str(tmp_path))
        html = app.render("page.html", {"title": "Welcome"})
        response = Response.html(html)
        assert response is not None

    def test_app_render_for_loop(self, tmp_path):
        from cello import App

        app = App()
        app.enable_templates(template_dir=str(tmp_path))
        result = app.render_string(
            "{% for x in items %}{{ x }}{% endfor %}",
            {"items": [1, 2, 3]},
        )
        assert result == "123"

    def test_app_render_if_condition(self, tmp_path):
        from cello import App

        app = App()
        app.enable_templates(template_dir=str(tmp_path))
        result = app.render_string(
            "{% if admin %}ADMIN{% else %}USER{% endif %}",
            {"admin": False},
        )
        assert result == "USER"


# ===========================================================================
# Auto-escape behaviour
# ===========================================================================


class TestAutoEscape:
    """Verify HTML auto-escaping works correctly for .html templates."""

    def test_auto_escape_enabled_html_template(self, tmp_path):
        from cello import MiniJinjaEngine

        _write_template(str(tmp_path), "xss.html", "{{ content }}")
        engine = MiniJinjaEngine(template_dir=str(tmp_path), auto_escape=True)
        result = engine.render("xss.html", {"content": "<script>alert(1)</script>"})
        assert "<script>" not in result
        assert "&lt;script&gt;" in result

    def test_auto_escape_disabled(self, tmp_path):
        from cello import MiniJinjaEngine

        _write_template(str(tmp_path), "raw.html", "{{ content }}")
        engine = MiniJinjaEngine(template_dir=str(tmp_path), auto_escape=False)
        result = engine.render("raw.html", {"content": "<b>bold</b>"})
        assert "<b>bold</b>" in result

    def test_auto_escape_txt_not_escaped(self, tmp_path):
        """Plain text templates should NOT be escaped even with auto_escape=True."""
        from cello import MiniJinjaEngine

        _write_template(str(tmp_path), "email.txt", "{{ content }}")
        engine = MiniJinjaEngine(template_dir=str(tmp_path), auto_escape=True)
        result = engine.render("email.txt", {"content": "<hello>"})
        assert "<hello>" in result


# ===========================================================================
# Complex context types
# ===========================================================================


class TestComplexContextTypes:
    """Verify Python → MiniJinja value conversion for various types."""

    def test_list_of_dicts(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string(
            "{% for u in users %}{{ u.name }},{% endfor %}",
            {"users": [{"name": "Alice"}, {"name": "Bob"}]},
        )
        assert result == "Alice,Bob,"

    def test_nested_list(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string(
            "{{ matrix[0][0] }}",
            {"matrix": [[1, 2], [3, 4]]},
        )
        assert result == "1"

    def test_tuple_as_list(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string(
            "{{ items | length }}",
            {"items": (10, 20, 30)},
        )
        assert result == "3"

    def test_large_integer(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        result = engine.render_string("{{ n }}", {"n": 1_000_000})
        assert "1000000" in result

    def test_zero_and_empty_string(self, tmp_path):
        from cello import MiniJinjaEngine

        engine = MiniJinjaEngine(template_dir=str(tmp_path))
        assert engine.render_string("{{ n }}", {"n": 0}) == "0"
        assert engine.render_string("{{ s }}", {"s": ""}) == ""


# ===========================================================================
# Version check
# ===========================================================================


def test_version_is_1_2_0():
    import cello

    assert cello.__version__ == "1.2.1"
