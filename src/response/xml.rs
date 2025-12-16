//! XML serialization for Cello responses.
//!
//! Provides:
//! - XML serialization from Rust types
//! - Python dict/list to XML conversion
//! - Configurable XML output

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use serde::Serialize;
use std::io::Cursor;

// ============================================================================
// XML Configuration
// ============================================================================

/// XML output configuration.
#[derive(Clone, Debug)]
pub struct XmlConfig {
    /// Include XML declaration
    pub declaration: bool,
    /// XML version
    pub version: String,
    /// Encoding
    pub encoding: String,
    /// Root element name
    pub root_name: String,
    /// Indent output
    pub indent: Option<usize>,
    /// Attribute prefix (e.g., "@" for keys like "@id")
    pub attr_prefix: String,
    /// Text content key (e.g., "#text")
    pub text_key: String,
    /// CDATA content key (e.g., "#cdata")
    pub cdata_key: String,
}

impl XmlConfig {
    /// Create new XML config with defaults.
    pub fn new() -> Self {
        Self {
            declaration: true,
            version: "1.0".to_string(),
            encoding: "UTF-8".to_string(),
            root_name: "root".to_string(),
            indent: Some(2),
            attr_prefix: "@".to_string(),
            text_key: "#text".to_string(),
            cdata_key: "#cdata".to_string(),
        }
    }

    /// Disable XML declaration.
    pub fn no_declaration(mut self) -> Self {
        self.declaration = false;
        self
    }

    /// Set root element name.
    pub fn root(mut self, name: &str) -> Self {
        self.root_name = name.to_string();
        self
    }

    /// Disable indentation.
    pub fn no_indent(mut self) -> Self {
        self.indent = None;
        self
    }

    /// Set indent size.
    pub fn indent(mut self, spaces: usize) -> Self {
        self.indent = Some(spaces);
        self
    }

    /// Set attribute prefix.
    pub fn attr_prefix(mut self, prefix: &str) -> Self {
        self.attr_prefix = prefix.to_string();
        self
    }
}

impl Default for XmlConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// XML Serializer
// ============================================================================

/// XML serializer.
pub struct XmlSerializer {
    config: XmlConfig,
}

impl XmlSerializer {
    /// Create new serializer with default config.
    pub fn new() -> Self {
        Self {
            config: XmlConfig::default(),
        }
    }

    /// Create with custom config.
    pub fn with_config(config: XmlConfig) -> Self {
        Self { config }
    }

    /// Serialize a value to XML.
    pub fn serialize<T: Serialize>(&self, value: &T) -> Result<String, XmlError> {
        // Convert to JSON value first for unified handling
        let json = serde_json::to_value(value)
            .map_err(|e| XmlError::Serialization(e.to_string()))?;
        self.serialize_json(&json)
    }

    /// Serialize a JSON value to XML.
    pub fn serialize_json(&self, value: &serde_json::Value) -> Result<String, XmlError> {
        let mut writer = Writer::new(Cursor::new(Vec::new()));

        // Write declaration
        if self.config.declaration {
            writer
                .write_event(Event::Decl(BytesDecl::new(
                    &self.config.version,
                    Some(&self.config.encoding),
                    None,
                )))
                .map_err(|e| XmlError::Write(e.to_string()))?;

            if self.config.indent.is_some() {
                writer
                    .write_event(Event::Text(BytesText::new("\n")))
                    .map_err(|e| XmlError::Write(e.to_string()))?;
            }
        }

        // Write root element
        self.write_value(&mut writer, &self.config.root_name, value, 0)?;

        let result = writer.into_inner().into_inner();
        String::from_utf8(result).map_err(|e| XmlError::Encoding(e.to_string()))
    }

    /// Write a JSON value as XML.
    fn write_value<W: std::io::Write>(
        &self,
        writer: &mut Writer<W>,
        name: &str,
        value: &serde_json::Value,
        depth: usize,
    ) -> Result<(), XmlError> {
        match value {
            serde_json::Value::Null => {
                self.write_indent(writer, depth)?;
                let elem = BytesStart::new(name);
                writer
                    .write_event(Event::Empty(elem))
                    .map_err(|e| XmlError::Write(e.to_string()))?;
                self.write_newline(writer)?;
            }
            serde_json::Value::Bool(b) => {
                self.write_simple_element(writer, name, &b.to_string(), depth)?;
            }
            serde_json::Value::Number(n) => {
                self.write_simple_element(writer, name, &n.to_string(), depth)?;
            }
            serde_json::Value::String(s) => {
                self.write_simple_element(writer, name, s, depth)?;
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    self.write_value(writer, name, item, depth)?;
                }
            }
            serde_json::Value::Object(obj) => {
                self.write_object(writer, name, obj, depth)?;
            }
        }
        Ok(())
    }

    /// Write a simple element with text content.
    fn write_simple_element<W: std::io::Write>(
        &self,
        writer: &mut Writer<W>,
        name: &str,
        content: &str,
        depth: usize,
    ) -> Result<(), XmlError> {
        self.write_indent(writer, depth)?;

        let elem = BytesStart::new(name);
        writer
            .write_event(Event::Start(elem))
            .map_err(|e| XmlError::Write(e.to_string()))?;

        writer
            .write_event(Event::Text(BytesText::new(content)))
            .map_err(|e| XmlError::Write(e.to_string()))?;

        writer
            .write_event(Event::End(BytesEnd::new(name)))
            .map_err(|e| XmlError::Write(e.to_string()))?;

        self.write_newline(writer)?;
        Ok(())
    }

    /// Write an object element.
    fn write_object<W: std::io::Write>(
        &self,
        writer: &mut Writer<W>,
        name: &str,
        obj: &serde_json::Map<String, serde_json::Value>,
        depth: usize,
    ) -> Result<(), XmlError> {
        self.write_indent(writer, depth)?;

        // Collect attributes and children
        let mut attributes: Vec<(&str, &str)> = Vec::new();
        let mut children: Vec<(&str, &serde_json::Value)> = Vec::new();
        let mut text_content: Option<&str> = None;

        for (key, value) in obj {
            if key.starts_with(&self.config.attr_prefix) {
                let attr_name = &key[self.config.attr_prefix.len()..];
                if let serde_json::Value::String(s) = value {
                    attributes.push((attr_name, s));
                }
            } else if key == &self.config.text_key {
                if let serde_json::Value::String(s) = value {
                    text_content = Some(s);
                }
            } else {
                children.push((key.as_str(), value));
            }
        }

        // Create element with attributes
        let mut elem = BytesStart::new(name);
        for (attr_name, attr_value) in &attributes {
            elem.push_attribute((*attr_name, *attr_value));
        }

        if children.is_empty() && text_content.is_none() {
            // Empty element
            writer
                .write_event(Event::Empty(elem))
                .map_err(|e| XmlError::Write(e.to_string()))?;
        } else {
            writer
                .write_event(Event::Start(elem))
                .map_err(|e| XmlError::Write(e.to_string()))?;

            if let Some(text) = text_content {
                writer
                    .write_event(Event::Text(BytesText::new(text)))
                    .map_err(|e| XmlError::Write(e.to_string()))?;
            } else if !children.is_empty() {
                self.write_newline(writer)?;
                for (child_name, child_value) in children {
                    self.write_value(writer, child_name, child_value, depth + 1)?;
                }
                self.write_indent(writer, depth)?;
            }

            writer
                .write_event(Event::End(BytesEnd::new(name)))
                .map_err(|e| XmlError::Write(e.to_string()))?;
        }

        self.write_newline(writer)?;
        Ok(())
    }

    /// Write indentation.
    fn write_indent<W: std::io::Write>(
        &self,
        writer: &mut Writer<W>,
        depth: usize,
    ) -> Result<(), XmlError> {
        if let Some(indent_size) = self.config.indent {
            let indent = " ".repeat(depth * indent_size);
            writer
                .write_event(Event::Text(BytesText::new(&indent)))
                .map_err(|e| XmlError::Write(e.to_string()))?;
        }
        Ok(())
    }

    /// Write newline if indenting.
    fn write_newline<W: std::io::Write>(&self, writer: &mut Writer<W>) -> Result<(), XmlError> {
        if self.config.indent.is_some() {
            writer
                .write_event(Event::Text(BytesText::new("\n")))
                .map_err(|e| XmlError::Write(e.to_string()))?;
        }
        Ok(())
    }
}

impl Default for XmlSerializer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// XML Response Type
// ============================================================================

/// XML response wrapper.
pub struct XmlResponse {
    pub content: String,
    pub status: u16,
}

impl XmlResponse {
    /// Create new XML response.
    pub fn new(content: String) -> Self {
        Self {
            content,
            status: 200,
        }
    }

    /// Set status code.
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// Get content type header value.
    pub fn content_type() -> &'static str {
        "application/xml; charset=utf-8"
    }
}

// ============================================================================
// XML Error
// ============================================================================

/// XML serialization error.
#[derive(Debug, Clone)]
pub enum XmlError {
    Serialization(String),
    Write(String),
    Encoding(String),
    Python(String),
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XmlError::Serialization(e) => write!(f, "XML serialization error: {}", e),
            XmlError::Write(e) => write!(f, "XML write error: {}", e),
            XmlError::Encoding(e) => write!(f, "XML encoding error: {}", e),
            XmlError::Python(e) => write!(f, "Python conversion error: {}", e),
        }
    }
}

impl std::error::Error for XmlError {}

// ============================================================================
// Python Integration
// ============================================================================

/// Convert Python object to XML string.
pub fn python_to_xml(py: Python<'_>, obj: &PyAny, root_name: &str) -> Result<String, String> {
    let json_value = python_to_json_value(py, obj)?;
    let config = XmlConfig::new().root(root_name);
    let serializer = XmlSerializer::with_config(config);
    serializer
        .serialize_json(&json_value)
        .map_err(|e| e.to_string())
}

/// Convert Python object to JSON value.
fn python_to_json_value(py: Python<'_>, obj: &PyAny) -> Result<serde_json::Value, String> {
    if obj.is_none() {
        return Ok(serde_json::Value::Null);
    }

    if let Ok(b) = obj.downcast::<PyBool>() {
        return Ok(serde_json::Value::Bool(b.is_true()));
    }

    if let Ok(i) = obj.downcast::<PyInt>() {
        if let Ok(n) = i.extract::<i64>() {
            return Ok(serde_json::Value::Number(n.into()));
        }
    }

    if let Ok(f) = obj.downcast::<PyFloat>() {
        if let Ok(n) = f.extract::<f64>() {
            if let Some(num) = serde_json::Number::from_f64(n) {
                return Ok(serde_json::Value::Number(num));
            }
        }
    }

    if let Ok(s) = obj.downcast::<PyString>() {
        return Ok(serde_json::Value::String(s.to_string()));
    }

    if let Ok(list) = obj.downcast::<PyList>() {
        let mut arr = Vec::new();
        for item in list.iter() {
            arr.push(python_to_json_value(py, item)?);
        }
        return Ok(serde_json::Value::Array(arr));
    }

    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key_str = key
                .extract::<String>()
                .map_err(|_| "Dict keys must be strings")?;
            map.insert(key_str, python_to_json_value(py, value)?);
        }
        return Ok(serde_json::Value::Object(map));
    }

    // Try to convert using Python's __dict__
    if let Ok(dict) = obj.getattr("__dict__") {
        return python_to_json_value(py, dict);
    }

    Err(format!("Cannot convert Python object to XML: {:?}", obj.get_type().name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_serializer_simple() {
        let serializer = XmlSerializer::with_config(XmlConfig::new().root("item").no_indent());

        let json = serde_json::json!({
            "name": "Test",
            "value": 42
        });

        let xml = serializer.serialize_json(&json).unwrap();
        assert!(xml.contains("<item>"));
        assert!(xml.contains("<name>Test</name>"));
        assert!(xml.contains("<value>42</value>"));
        assert!(xml.contains("</item>"));
    }

    #[test]
    fn test_xml_serializer_array() {
        let serializer = XmlSerializer::with_config(XmlConfig::new().root("items").no_indent());

        let json = serde_json::json!({
            "item": [1, 2, 3]
        });

        let xml = serializer.serialize_json(&json).unwrap();
        assert!(xml.contains("<item>1</item>"));
        assert!(xml.contains("<item>2</item>"));
        assert!(xml.contains("<item>3</item>"));
    }

    #[test]
    fn test_xml_serializer_attributes() {
        let serializer = XmlSerializer::with_config(XmlConfig::new().root("root").no_indent());

        let json = serde_json::json!({
            "item": {
                "@id": "123",
                "@type": "test",
                "value": "content"
            }
        });

        let xml = serializer.serialize_json(&json).unwrap();
        assert!(xml.contains("id=\"123\""));
        assert!(xml.contains("type=\"test\""));
    }

    #[test]
    fn test_xml_config() {
        let config = XmlConfig::new()
            .root("data")
            .no_declaration()
            .indent(4);

        assert_eq!(config.root_name, "data");
        assert!(!config.declaration);
        assert_eq!(config.indent, Some(4));
    }

    #[test]
    fn test_xml_declaration() {
        let serializer = XmlSerializer::with_config(XmlConfig::new().root("test"));
        let xml = serializer.serialize_json(&serde_json::json!({})).unwrap();
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("version=\"1.0\""));
        assert!(xml.contains("encoding=\"UTF-8\""));
    }
}
