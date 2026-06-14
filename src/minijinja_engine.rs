//! MiniJinja Template Engine for Cello Framework
//!
//! Provides full Jinja2-compatible template rendering via the `minijinja` Rust crate.
//! Attach as optional middleware with `app.enable_templates()`.
//!
//! ## Supported syntax
//! - `{{ variable }}` — variable interpolation
//! - `{% if %} / {% elif %} / {% else %} / {% endif %}` — conditionals
//! - `{% for item in list %} / {% endfor %}` — loops
//! - `{% block %} / {% extends %}` — template inheritance
//! - `{% include %}` — template includes
//! - `{% macro %} / {% call %}` — macros
//! - `{{ value | filter }}` — built-in filters (upper, lower, trim, length, …)
//! - Auto HTML-escaping for `.html` / `.htm` / `.xml` templates

use minijinja::Environment;
use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};
use std::sync::Arc;

// ============================================================================
// Core Engine
// ============================================================================

struct EngineInner {
    env: Environment<'static>,
}

/// MiniJinja-powered Jinja2-compatible template engine for Cello.
///
/// Attach to your application with `app.enable_templates()` or create
/// a standalone instance for direct use.
///
/// # Example — via App
///
/// ```python
/// from cello import App
///
/// app = App()
/// app.enable_templates(template_dir="templates", auto_escape=True)
///
/// @app.get("/")
/// def home(request):
///     html = app.render("index.html", {"title": "Home", "items": [1, 2, 3]})
///     from cello import Response
///     return Response.html(html)
/// ```
///
/// # Example — standalone
///
/// ```python
/// from cello import MiniJinjaEngine
///
/// engine = MiniJinjaEngine(template_dir="templates")
/// html = engine.render("index.html", {"name": "World"})
/// ```
#[pyclass(name = "MiniJinjaEngine")]
pub struct PyMiniJinjaEngine {
    inner: Arc<RwLock<EngineInner>>,
    template_dir: String,
    auto_escape: bool,
}

#[pymethods]
impl PyMiniJinjaEngine {
    /// Create a new MiniJinja engine.
    ///
    /// Args:
    ///     template_dir: Directory containing template files (default: ``"templates"``).
    ///     auto_escape: Enable HTML auto-escaping for ``.html``/``.htm``/``.xml``
    ///         templates (default: ``True``).
    #[new]
    #[pyo3(signature = (template_dir = "templates", auto_escape = true))]
    pub fn new(template_dir: &str, auto_escape: bool) -> PyResult<Self> {
        let mut env = Environment::new();

        let dir = template_dir.to_string();
        env.set_loader(minijinja::path_loader(&dir));

        if auto_escape {
            env.set_auto_escape_callback(|name: &str| -> minijinja::AutoEscape {
                if name.ends_with(".html") || name.ends_with(".htm") || name.ends_with(".xml") {
                    minijinja::AutoEscape::Html
                } else {
                    minijinja::AutoEscape::None
                }
            });
        } else {
            // minijinja enables HTML escaping for .html/.htm/.xml by default;
            // explicitly override that when the caller opts out.
            env.set_auto_escape_callback(|_name: &str| -> minijinja::AutoEscape {
                minijinja::AutoEscape::None
            });
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(EngineInner { env })),
            template_dir: dir,
            auto_escape,
        })
    }

    /// Render a template file with context variables.
    ///
    /// Args:
    ///     name: Template filename relative to ``template_dir``
    ///         (e.g. ``"index.html"`` or ``"emails/welcome.txt"``).
    ///     context: Dictionary of variables available in the template.
    ///
    /// Returns:
    ///     Rendered string.
    ///
    /// Raises:
    ///     ValueError: Template not found or Jinja2 syntax/runtime error.
    ///
    /// Example:
    ///     ```python
    ///     html = engine.render("page.html", {
    ///         "title": "Hello",
    ///         "user": {"name": "Alice", "age": 30},
    ///         "items": ["a", "b", "c"],
    ///     })
    ///     ```
    pub fn render(&self, name: &str, context: &PyDict, py: Python<'_>) -> PyResult<String> {
        let ctx = pydict_to_json(context, py)?;
        let inner = self.inner.read();
        let tmpl = inner.env.get_template(name).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "MiniJinja: template '{}' not found: {}",
                name, e
            ))
        })?;
        tmpl.render(&ctx).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "MiniJinja: render error in '{}': {}",
                name, e
            ))
        })
    }

    /// Render an inline template string.
    ///
    /// Useful for one-off templates or testing without a file on disk.
    ///
    /// Args:
    ///     source: Jinja2 template source string.
    ///     context: Dictionary of variables.
    ///
    /// Returns:
    ///     Rendered string.
    ///
    /// Raises:
    ///     ValueError: Jinja2 syntax or runtime error.
    ///
    /// Example:
    ///     ```python
    ///     result = engine.render_string(
    ///         "Hello, {{ name }}! You have {{ count }} messages.",
    ///         {"name": "Bob", "count": 5},
    ///     )
    ///     # → "Hello, Bob! You have 5 messages."
    ///     ```
    pub fn render_string(&self, source: &str, context: &PyDict, py: Python<'_>) -> PyResult<String> {
        let ctx = pydict_to_json(context, py)?;
        let inner = self.inner.read();
        inner.env.render_str(source, &ctx).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("MiniJinja: render error: {}", e))
        })
    }

    /// Add a global variable available in every template rendered by this engine.
    ///
    /// Globals are merged with per-render context, with per-render values taking
    /// precedence on name collision.
    ///
    /// Args:
    ///     name: Variable name.
    ///     value: Python value (``str``, ``int``, ``float``, ``bool``,
    ///         ``None``, ``list``, ``dict``).
    ///
    /// Example:
    ///     ```python
    ///     engine.add_global("app_name", "My App")
    ///     engine.add_global("version", "1.1.0")
    ///     ```
    pub fn add_global(&self, name: &str, value: PyObject, py: Python<'_>) -> PyResult<()> {
        let json_val = pyobj_to_json(value.as_ref(py))?;
        let mj_val = minijinja::Value::from_serialize(&json_val);
        self.inner.write().env.add_global(name.to_string(), mj_val);
        Ok(())
    }

    /// Add multiple globals at once from a dictionary.
    ///
    /// Args:
    ///     globals: Dictionary of name → value pairs.
    ///
    /// Example:
    ///     ```python
    ///     engine.add_globals({
    ///         "app_name": "My App",
    ///         "debug": False,
    ///         "year": 2026,
    ///     })
    ///     ```
    pub fn add_globals(&self, globals: &PyDict, _py: Python<'_>) -> PyResult<()> {
        let mut inner = self.inner.write();
        for (k, v) in globals.iter() {
            let key: String = k.extract()?;
            let json_val = pyobj_to_json(v)?;
            let mj_val = minijinja::Value::from_serialize(&json_val);
            inner.env.add_global(key, mj_val);
        }
        Ok(())
    }

    /// Directory from which templates are loaded.
    #[getter]
    pub fn template_dir(&self) -> &str {
        &self.template_dir
    }

    /// Whether HTML auto-escaping is enabled.
    #[getter]
    pub fn auto_escape(&self) -> bool {
        self.auto_escape
    }

    pub fn __repr__(&self) -> String {
        format!(
            "MiniJinjaEngine(template_dir='{}', auto_escape={})",
            self.template_dir, self.auto_escape
        )
    }
}

// ============================================================================
// Value conversion: Python → serde_json::Value → minijinja
//
// Using serde_json as an intermediary is the cleanest approach:
//   • serde_json::Value implements serde::Serialize
//   • minijinja can render any serde::Serialize value
//   • All Python primitive types map naturally to JSON types
// ============================================================================

/// Convert a Python `dict` to `serde_json::Value::Object`.
pub fn pydict_to_json(dict: &PyDict, _py: Python<'_>) -> PyResult<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        map.insert(key, pyobj_to_json(v)?);
    }
    Ok(serde_json::Value::Object(map))
}

/// Recursively convert any Python object to `serde_json::Value`.
pub fn pyobj_to_json(val: &PyAny) -> PyResult<serde_json::Value> {
    // None
    if val.is_none() {
        return Ok(serde_json::Value::Null);
    }
    // bool must be checked before int (bool is a subclass of int in Python)
    if val.is_instance_of::<PyBool>() {
        let b: bool = val.extract()?;
        return Ok(serde_json::Value::Bool(b));
    }
    // int
    if val.is_instance_of::<PyInt>() {
        let i: i64 = val.extract()?;
        return Ok(serde_json::json!(i));
    }
    // float
    if val.is_instance_of::<PyFloat>() {
        let f: f64 = val.extract()?;
        return Ok(serde_json::json!(f));
    }
    // str
    if val.is_instance_of::<PyString>() {
        let s: String = val.extract()?;
        return Ok(serde_json::Value::String(s));
    }
    // list / tuple → JSON array
    if val.is_instance_of::<PyList>() {
        let list = val.downcast::<PyList>()?;
        let arr: Vec<serde_json::Value> = list
            .iter()
            .map(pyobj_to_json)
            .collect::<PyResult<_>>()?;
        return Ok(serde_json::Value::Array(arr));
    }
    if val.is_instance_of::<PyTuple>() {
        let tup = val.downcast::<PyTuple>()?;
        let arr: Vec<serde_json::Value> = tup
            .iter()
            .map(pyobj_to_json)
            .collect::<PyResult<_>>()?;
        return Ok(serde_json::Value::Array(arr));
    }
    // dict → JSON object
    if val.is_instance_of::<PyDict>() {
        let dict = val.downcast::<PyDict>()?;
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract().unwrap_or_else(|_| k.str().unwrap().to_string());
            map.insert(key, pyobj_to_json(v)?);
        }
        return Ok(serde_json::Value::Object(map));
    }
    // Dataclass / object with __dict__ → recurse into dict
    if let Ok(obj_dict) = val.getattr("__dict__") {
        if let Ok(d) = obj_dict.downcast::<PyDict>() {
            let mut map = serde_json::Map::new();
            for (k, v) in d.iter() {
                let key: String = k.extract().unwrap_or_else(|_| k.str().unwrap().to_string());
                // Skip private/dunder attributes
                if key.starts_with('_') {
                    continue;
                }
                map.insert(key, pyobj_to_json(v)?);
            }
            return Ok(serde_json::Value::Object(map));
        }
    }
    // Fallback: stringify
    Ok(serde_json::Value::String(
        val.str()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "<unprintable>".to_string()),
    ))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Build a temporary directory with a test template file.
    fn setup_templates(content: &str) -> (TempDir, String) {
        let dir = TempDir::new().expect("tmpdir");
        let path = dir.path().join("test.html");
        fs::write(&path, content).expect("write template");
        (dir, path.to_string_lossy().to_string())
    }

    #[test]
    fn test_render_string_simple() {
        // We can test render_str logic directly via the minijinja Environment.
        let mut env = Environment::new();
        env.add_template("hello", "Hello, {{ name }}!").unwrap();
        let result = env
            .render_str("Hello, {{ name }}!", serde_json::json!({ "name": "World" }))
            .unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_render_if_block() {
        let env = Environment::new();
        let tmpl = "{% if active %}yes{% else %}no{% endif %}";
        let result = env
            .render_str(tmpl, serde_json::json!({ "active": true }))
            .unwrap();
        assert_eq!(result, "yes");
    }

    #[test]
    fn test_render_for_loop() {
        let env = Environment::new();
        let tmpl = "{% for i in items %}{{ i }},{% endfor %}";
        let result = env
            .render_str(tmpl, serde_json::json!({ "items": [1, 2, 3] }))
            .unwrap();
        assert_eq!(result, "1,2,3,");
    }

    #[test]
    fn test_render_filters() {
        let env = Environment::new();
        let result = env
            .render_str("{{ name | upper }}", serde_json::json!({ "name": "cello" }))
            .unwrap();
        assert_eq!(result, "CELLO");
    }

    #[test]
    fn test_render_nested_dict() {
        let env = Environment::new();
        let ctx = serde_json::json!({
            "user": { "name": "Alice", "age": 30 }
        });
        let result = env
            .render_str("{{ user.name }} is {{ user.age }}", ctx)
            .unwrap();
        assert_eq!(result, "Alice is 30");
    }

    #[test]
    fn test_global_variable() {
        let mut env = Environment::new();
        env.add_global("app_name", minijinja::Value::from("Cello"));
        let result = env
            .render_str("App: {{ app_name }}", serde_json::json!({}))
            .unwrap();
        assert_eq!(result, "App: Cello");
    }

    #[test]
    fn test_render_from_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("greet.html"), "Hi, {{ name }}!").unwrap();
        let mut env = Environment::new();
        env.set_loader(minijinja::path_loader(dir.path()));
        let tmpl = env.get_template("greet.html").unwrap();
        let result = tmpl
            .render(serde_json::json!({ "name": "Cello" }))
            .unwrap();
        assert_eq!(result, "Hi, Cello!");
    }

    #[test]
    fn test_template_inheritance() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("base.html"),
            "HEADER{% block content %}{% endblock %}FOOTER",
        )
        .unwrap();
        fs::write(
            dir.path().join("child.html"),
            "{% extends 'base.html' %}{% block content %}BODY{% endblock %}",
        )
        .unwrap();
        let mut env = Environment::new();
        env.set_loader(minijinja::path_loader(dir.path()));
        let tmpl = env.get_template("child.html").unwrap();
        let result = tmpl.render(serde_json::json!({})).unwrap();
        assert_eq!(result, "HEADERBODYFOOTER");
    }

    #[test]
    fn test_json_filter() {
        let env = Environment::new();
        let result = env
            .render_str(
                "{{ data | tojson }}",
                serde_json::json!({ "data": [1, 2, 3] }),
            )
            .unwrap();
        assert!(result.contains("1"));
        assert!(result.contains("2"));
        assert!(result.contains("3"));
    }
}
