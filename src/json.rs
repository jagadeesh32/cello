//! SIMD-accelerated JSON handling.
//!
//! Uses simd-json for fast JSON parsing and serialization,
//! with serde_json as fallback.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

/// Parse JSON string to serde_json::Value using SIMD acceleration.
#[inline]
pub fn parse_json(input: &str) -> Result<serde_json::Value, String> {
    // simd-json requires mutable input, so we need to copy
    let mut input_bytes = input.as_bytes().to_vec();

    simd_json::serde::from_slice(&mut input_bytes).map_err(|e| format!("JSON parse error: {e}"))
}

/// Parse JSON bytes to serde_json::Value using SIMD acceleration.
#[inline]
pub fn parse_json_bytes(input: &mut [u8]) -> Result<serde_json::Value, String> {
    simd_json::serde::from_slice(input).map_err(|e| format!("JSON parse error: {e}"))
}

/// Serialize a serde_json::Value to JSON string.
#[inline]
pub fn serialize_json(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| format!("JSON serialize error: {e}"))
}

/// Serialize a serde_json::Value to JSON bytes.
#[inline]
pub fn serialize_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| format!("JSON serialize error: {e}"))
}

/// Serialize a serde_json::Value to pretty JSON string.
#[inline]
pub fn serialize_json_pretty(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| format!("JSON serialize error: {e}"))
}

/// Convert a Python object to serde_json::Value.
#[inline]
pub fn python_to_json(py: Python<'_>, obj: &PyAny) -> Result<serde_json::Value, String> {
    // Handle None
    if obj.is_none() {
        return Ok(serde_json::Value::Null);
    }

    // Handle bool (must come before int check since bool is subclass of int in Python)
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(serde_json::Value::Bool(b));
    }

    // Handle int
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(serde_json::Value::Number(i.into()));
    }

    // Handle float
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(serde_json::json!(f));
    }

    // Handle string
    if let Ok(s) = obj.extract::<String>() {
        return Ok(serde_json::Value::String(s));
    }

    // Handle list
    if let Ok(list) = obj.downcast::<PyList>() {
        // PERF: Pre-allocate vec with known capacity
        let mut items = Vec::with_capacity(list.len());
        for item in list.iter() {
            items.push(python_to_json(py, item)?);
        }
        return Ok(serde_json::Value::Array(items));
    }

    // Handle dict
    if let Ok(dict) = obj.downcast::<PyDict>() {
        // PERF: Pre-allocate map with known capacity
        let mut map = serde_json::Map::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            let key_str = key
                .extract::<String>()
                .map_err(|_| "Dict keys must be strings".to_string())?;
            let value_json = python_to_json(py, value)?;
            map.insert(key_str, value_json);
        }
        return Ok(serde_json::Value::Object(map));
    }

    // Handle tuple
    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        // PERF: Pre-allocate vec with known capacity
        let mut items = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            items.push(python_to_json(py, item)?);
        }
        return Ok(serde_json::Value::Array(items));
    }

    // Handle Response object - check by class name
    let class_name = obj.get_type().name().unwrap_or("");
    if class_name == "Response" {
        let mut response_obj = serde_json::Map::new();
        response_obj.insert(
            "__cello_response__".to_string(),
            serde_json::Value::Bool(true),
        );

        if let Ok(status) = obj.getattr("status") {
            if let Ok(s) = status.extract::<u16>() {
                response_obj.insert("status".to_string(), serde_json::Value::Number(s.into()));
            }
        }

        if let Ok(headers) = obj.getattr("headers") {
            if let Ok(dict) = headers.downcast::<PyDict>() {
                let mut headers_map = serde_json::Map::new();
                for (key, value) in dict.iter() {
                    if let (Ok(k), Ok(v)) = (key.extract::<String>(), value.extract::<String>()) {
                        headers_map.insert(k, serde_json::Value::String(v));
                    }
                }
                response_obj.insert(
                    "headers".to_string(),
                    serde_json::Value::Object(headers_map),
                );
            }
        }

        // Get body - use body() which is Python-accessible
        if let Ok(body_bytes) = obj.call_method0("body") {
            if let Ok(bytes) = body_bytes.extract::<Vec<u8>>() {
                if let Ok(body_str) = String::from_utf8(bytes) {
                    response_obj.insert("body".to_string(), serde_json::Value::String(body_str));
                }
            }
        }

        return Ok(serde_json::Value::Object(response_obj));
    }

    Err(format!("Cannot convert Python object to JSON: {obj:?}"))
}

/// PERF: Convert a Python object directly to JSON bytes, skipping intermediate serde_json::Value.
/// Returns Ok(Some(bytes)) for normal dicts/lists/primitives,
/// Returns Ok(None) for Response objects (caller must fall back to python_to_json).
#[inline]
pub fn python_to_json_bytes_direct(py: Python<'_>, obj: &PyAny) -> Result<Option<Vec<u8>>, String> {
    // PERF: Check for dict/list FIRST (common case) before the expensive class name check.
    // Most handlers return dicts, so fast-path that.
    if obj.downcast::<PyDict>().is_ok() || obj.downcast::<PyList>().is_ok() {
        let mut buf = Vec::with_capacity(128);
        write_json_value(py, obj, &mut buf)?;
        return Ok(Some(buf));
    }

    // Check primitives
    if obj.is_none()
        || obj.extract::<bool>().is_ok()
        || obj.extract::<i64>().is_ok()
        || obj.extract::<f64>().is_ok()
        || obj.extract::<String>().is_ok()
    {
        let mut buf = Vec::with_capacity(64);
        write_json_value(py, obj, &mut buf)?;
        return Ok(Some(buf));
    }

    // Not a simple type - likely a Response object, fall back to Value path
    Ok(None)
}

/// Write a Python object as JSON directly to a byte buffer.
fn write_json_value(py: Python<'_>, obj: &PyAny, buf: &mut Vec<u8>) -> Result<(), String> {
    use std::io::Write;

    // Handle None
    if obj.is_none() {
        buf.extend_from_slice(b"null");
        return Ok(());
    }

    // Handle bool (must come before int check since bool is subclass of int in Python)
    if let Ok(b) = obj.extract::<bool>() {
        buf.extend_from_slice(if b { b"true" } else { b"false" });
        return Ok(());
    }

    // Handle int
    if let Ok(i) = obj.extract::<i64>() {
        write!(buf, "{i}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Handle float
    if let Ok(f) = obj.extract::<f64>() {
        if f.is_finite() {
            write!(buf, "{f}").map_err(|e| e.to_string())?;
        } else {
            buf.extend_from_slice(b"null");
        }
        return Ok(());
    }

    // Handle string - need to JSON-escape
    if let Ok(s) = obj.extract::<String>() {
        write_json_string(&s, buf);
        return Ok(());
    }

    // Handle list
    if let Ok(list) = obj.downcast::<PyList>() {
        buf.push(b'[');
        for (i, item) in list.iter().enumerate() {
            if i > 0 {
                buf.push(b',');
            }
            write_json_value(py, item, buf)?;
        }
        buf.push(b']');
        return Ok(());
    }

    // Handle dict
    if let Ok(dict) = obj.downcast::<PyDict>() {
        buf.push(b'{');
        let mut first = true;
        for (key, value) in dict.iter() {
            let key_str = key
                .extract::<String>()
                .map_err(|_| "Dict keys must be strings".to_string())?;
            if !first {
                buf.push(b',');
            }
            first = false;
            write_json_string(&key_str, buf);
            buf.push(b':');
            write_json_value(py, value, buf)?;
        }
        buf.push(b'}');
        return Ok(());
    }

    // Handle tuple
    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        buf.push(b'[');
        for (i, item) in tuple.iter().enumerate() {
            if i > 0 {
                buf.push(b',');
            }
            write_json_value(py, item, buf)?;
        }
        buf.push(b']');
        return Ok(());
    }

    Err(format!("Cannot convert Python object to JSON: {obj:?}"))
}

/// Write a JSON-escaped string to the buffer.
#[inline]
fn write_json_string(s: &str, buf: &mut Vec<u8>) {
    buf.push(b'"');
    for byte in s.bytes() {
        match byte {
            b'"' => buf.extend_from_slice(b"\\\""),
            b'\\' => buf.extend_from_slice(b"\\\\"),
            b'\n' => buf.extend_from_slice(b"\\n"),
            b'\r' => buf.extend_from_slice(b"\\r"),
            b'\t' => buf.extend_from_slice(b"\\t"),
            b if b < 0x20 => {
                // Control characters: \u00XX
                buf.extend_from_slice(b"\\u00");
                let high = b >> 4;
                let low = b & 0x0f;
                buf.push(if high < 10 {
                    b'0' + high
                } else {
                    b'a' + high - 10
                });
                buf.push(if low < 10 {
                    b'0' + low
                } else {
                    b'a' + low - 10
                });
            }
            _ => buf.push(byte),
        }
    }
    buf.push(b'"');
}

/// Convert a serde_json::Value to a Python object.
#[inline]
pub fn json_to_python(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(b.into_py(py)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => Ok(s.into_py(py)),
        serde_json::Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_to_python(py, item)?)?;
            }
            Ok(list.into_py(py))
        }
        serde_json::Value::Object(obj) => {
            let dict = PyDict::new(py);
            for (key, val) in obj {
                dict.set_item(key, json_to_python(py, val)?)?;
            }
            Ok(dict.into_py(py))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json() {
        let result = parse_json(r#"{"name": "test", "value": 42}"#);
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["name"], "test");
        assert_eq!(value["value"], 42);
    }

    #[test]
    fn test_parse_json_array() {
        let result = parse_json(r#"[1, 2, 3, "four"]"#);
        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.is_array());
        assert_eq!(value[0], 1);
        assert_eq!(value[3], "four");
    }

    #[test]
    fn test_serialize_json() {
        let value = serde_json::json!({
            "message": "hello",
            "count": 10
        });
        let result = serialize_json(&value);
        assert!(result.is_ok());
        let json_str = result.unwrap();
        assert!(json_str.contains("hello"));
        assert!(json_str.contains("10"));
    }
}
