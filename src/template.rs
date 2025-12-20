//! Template Engine for Cello Framework
//!
//! This module provides Django/Jinja2-style template rendering support.

use pyo3::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;

// ============================================================================
// Template Types
// ============================================================================

/// Template configuration.
#[derive(Debug, Clone)]
pub struct TemplateConfig {
    /// Template directory path.
    pub template_dir: PathBuf,
    /// Auto-reload templates (for development).
    pub auto_reload: bool,
    /// Default content type.
    pub content_type: String,
    /// Template extension (e.g., ".html").
    pub extension: String,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            template_dir: PathBuf::from("templates"),
            auto_reload: true,
            content_type: "text/html; charset=utf-8".to_string(),
            extension: ".html".to_string(),
        }
    }
}

/// Template context for rendering.
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    data: HashMap<String, serde_json::Value>,
}

impl TemplateContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a value into the context.
    pub fn insert<T: serde::Serialize>(&mut self, key: &str, value: T) -> &mut Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), json_value);
        }
        self
    }

    /// Get a value from the context.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    /// Convert context to JSON object.
    pub fn to_json_object(&self) -> serde_json::Value {
        serde_json::Value::Object(
            self.data
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }
}

// ============================================================================
// Simple Template Engine (Jinja2-like)
// ============================================================================

/// Simple template engine with basic variable substitution.
/// For production use, integrate with Tera or another full template engine.
pub struct TemplateEngine {
    config: TemplateConfig,
    cache: Arc<RwLock<HashMap<String, String>>>,
}

impl TemplateEngine {
    /// Create a new template engine.
    pub fn new(config: TemplateConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with default configuration.
    pub fn with_dir(template_dir: &str) -> Self {
        Self::new(TemplateConfig {
            template_dir: PathBuf::from(template_dir),
            ..Default::default()
        })
    }

    /// Load a template from file.
    pub fn load_template(&self, name: &str) -> Result<String, String> {
        // Check cache first
        if !self.config.auto_reload {
            if let Some(cached) = self.cache.read().get(name) {
                return Ok(cached.clone());
            }
        }

        // Build file path
        let mut path = self.config.template_dir.clone();
        path.push(name);
        if !name.ends_with(&self.config.extension) {
            path.set_extension(&self.config.extension.trim_start_matches('.'));
        }

        // Read file
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to load template '{}': {}", name, e))?;

        // Cache if not auto-reload
        if !self.config.auto_reload {
            self.cache.write().insert(name.to_string(), content.clone());
        }

        Ok(content)
    }

    /// Render a template with context.
    pub fn render(&self, name: &str, context: &TemplateContext) -> Result<String, String> {
        let template = self.load_template(name)?;
        self.render_string(&template, context)
    }

    /// Render a template string with context.
    pub fn render_string(&self, template: &str, context: &TemplateContext) -> Result<String, String> {
        let mut result = template.to_string();

        // Simple variable substitution: {{ variable }}
        for (key, value) in &context.data {
            let pattern = format!("{{{{ {} }}}}", key);
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => "".to_string(),
                _ => serde_json::to_string(value).unwrap_or_default(),
            };
            result = result.replace(&pattern, &replacement);
        }

        // Also handle {{ variable }} without spaces
        for (key, value) in &context.data {
            let pattern = format!("{{{{{}}}}}", key);
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => "".to_string(),
                _ => serde_json::to_string(value).unwrap_or_default(),
            };
            result = result.replace(&pattern, &replacement);
        }

        Ok(result)
    }

    /// Clear the template cache.
    pub fn clear_cache(&self) {
        self.cache.write().clear();
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new(TemplateConfig::default())
    }
}

// ============================================================================
// Python Integration
// ============================================================================

/// Python-exposed template engine.
#[pyclass]
pub struct PyTemplateEngine {
    inner: TemplateEngine,
}

#[pymethods]
impl PyTemplateEngine {
    #[new]
    #[pyo3(signature = (template_dir="templates"))]
    pub fn new(template_dir: &str) -> Self {
        Self {
            inner: TemplateEngine::with_dir(template_dir),
        }
    }

    /// Render a template with context.
    pub fn render(&self, name: &str, context: HashMap<String, PyObject>, py: Python<'_>) -> PyResult<String> {
        let mut ctx = TemplateContext::new();
        
        for (key, value) in context {
            if let Ok(s) = value.extract::<String>(py) {
                ctx.insert(&key, s);
            } else if let Ok(n) = value.extract::<i64>(py) {
                ctx.insert(&key, n);
            } else if let Ok(f) = value.extract::<f64>(py) {
                ctx.insert(&key, f);
            } else if let Ok(b) = value.extract::<bool>(py) {
                ctx.insert(&key, b);
            } else {
                // Try to convert to string
                ctx.insert(&key, value.to_string());
            }
        }

        self.inner
            .render(name, &ctx)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }

    /// Render a template string with context.
    pub fn render_string(&self, template: &str, context: HashMap<String, PyObject>, py: Python<'_>) -> PyResult<String> {
        let mut ctx = TemplateContext::new();
        
        for (key, value) in context {
            if let Ok(s) = value.extract::<String>(py) {
                ctx.insert(&key, s);
            } else if let Ok(n) = value.extract::<i64>(py) {
                ctx.insert(&key, n);
            } else if let Ok(f) = value.extract::<f64>(py) {
                ctx.insert(&key, f);
            } else if let Ok(b) = value.extract::<bool>(py) {
                ctx.insert(&key, b);
            }
        }

        self.inner
            .render_string(template, &ctx)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e))
    }

    /// Clear template cache.
    pub fn clear_cache(&self) {
        self.inner.clear_cache();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_context() {
        let mut ctx = TemplateContext::new();
        ctx.insert("name", "John");
        ctx.insert("age", 30);
        ctx.insert("active", true);

        assert!(ctx.get("name").is_some());
        assert!(ctx.get("age").is_some());
    }

    #[test]
    fn test_render_string() {
        let engine = TemplateEngine::default();
        let mut ctx = TemplateContext::new();
        ctx.insert("name", "World");
        ctx.insert("count", 42);

        let result = engine.render_string("Hello, {{ name }}! Count: {{ count }}", &ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, World! Count: 42");
    }

    #[test]
    fn test_render_string_no_spaces() {
        let engine = TemplateEngine::default();
        let mut ctx = TemplateContext::new();
        ctx.insert("title", "Test");

        let result = engine.render_string("<h1>{{title}}</h1>", &ctx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "<h1>Test</h1>");
    }
}
