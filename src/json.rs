//! SIMD-accelerated JSON handling.
//!
//! Uses simd-json for fast JSON parsing and serialization,
//! with serde_json as fallback.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

/// Parse JSON string to serde_json::Value using SIMD acceleration.
pub fn parse_json(input: &str) -> Result<serde_json::Value, String> {
    // simd-json requires mutable input, so we need to copy
    let mut input_bytes = input.as_bytes().to_vec();
    
    simd_json::serde::from_slice(&mut input_bytes)
        .map_err(|e| format!("JSON parse error: {}", e))
}

/// Parse JSON bytes to serde_json::Value using SIMD acceleration.
pub fn parse_json_bytes(input: &mut [u8]) -> Result<serde_json::Value, String> {
    simd_json::serde::from_slice(input)
        .map_err(|e| format!("JSON parse error: {}", e))
}

/// Serialize a serde_json::Value to JSON string.
pub fn serialize_json(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string(value)
        .map_err(|e| format!("JSON serialize error: {}", e))
}

/// Serialize a serde_json::Value to JSON bytes.
pub fn serialize_json_bytes(value: &serde_json::Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value)
        .map_err(|e| format!("JSON serialize error: {}", e))
}

/// Serialize a serde_json::Value to pretty JSON string.
pub fn serialize_json_pretty(value: &serde_json::Value) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| format!("JSON serialize error: {}", e))
}

/// Convert a Python object to serde_json::Value.
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
        let items: Result<Vec<serde_json::Value>, String> =
            list.iter().map(|item| python_to_json(py, item)).collect();
        return Ok(serde_json::Value::Array(items?));
    }

    // Handle dict
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
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
        let items: Result<Vec<serde_json::Value>, String> =
            tuple.iter().map(|item| python_to_json(py, item)).collect();
        return Ok(serde_json::Value::Array(items?));
    }

    Err(format!("Cannot convert Python object to JSON: {:?}", obj))
}

/// Convert a serde_json::Value to a Python object.
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
