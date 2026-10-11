//! SIMD-accelerated JSON handling.
//!
//! Uses simd-json for fast JSON parsing and serialization,
//! with serde_json as fallback.

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyList, PyLong, PyString, PyTuple};

use crate::db::RawJson;
use crate::schema::runtime::{classify, OrmKind};
use crate::schema::PyModelSpec;

/// A queryset found inside a handler's return value. The serializer leaves a
/// hole at byte `at` of the body; `src/handler.rs` runs the query and splices
/// the rows' JSON in (see `QuerySet.json()`).
pub struct Hole {
    pub at: usize,
    pub queryset: PyObject,
}

/// Holes in one serialized body, in body order.
pub type Holes = Vec<Hole>;

const QUERYSET_HERE: &str = "a QuerySet can only be serialized as (part of) a handler's return \
     value, where Cello runs it after the handler returns; here, use `await qs.json()` \
     (pre-serialized rows) or `await qs` (model instances)";

/// Parse JSON string to serde_json::Value.
/// Uses SIMD acceleration on x86_64 and aarch64 (NEON), falls back to serde_json
/// on other architectures for maximum cross-platform compatibility.
#[inline]
pub fn parse_json(input: &str) -> Result<serde_json::Value, String> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
    {
        // simd-json requires mutable input, so we need to copy
        let mut input_bytes = input.as_bytes().to_vec();
        simd_json::serde::from_slice(&mut input_bytes).map_err(|e| format!("JSON parse error: {e}"))
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
    {
        serde_json::from_str(input).map_err(|e| format!("JSON parse error: {e}"))
    }
}

/// Parse JSON bytes to serde_json::Value.
/// Uses SIMD acceleration where available, serde_json fallback otherwise.
#[inline]
pub fn parse_json_bytes(input: &mut [u8]) -> Result<serde_json::Value, String> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]
    {
        simd_json::serde::from_slice(input).map_err(|e| format!("JSON parse error: {e}"))
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
    {
        serde_json::from_slice(input).map_err(|e| format!("JSON parse error: {e}"))
    }
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

    // PERF: concrete-type fast paths first (see `write_json_value`).
    if let Ok(s) = obj.downcast::<PyString>() {
        return Ok(serde_json::Value::String(
            s.to_str().map_err(|e| e.to_string())?.to_owned(),
        ));
    }
    if let Ok(b) = obj.downcast::<PyBool>() {
        return Ok(serde_json::Value::Bool(b.is_true()));
    }
    if obj.downcast::<PyLong>().is_ok() {
        if let Ok(i) = obj.extract::<i64>() {
            return Ok(serde_json::Value::Number(i.into()));
        }
    } else if let Ok(f) = obj.downcast::<PyFloat>() {
        return Ok(serde_json::json!(f.value()));
    }

    // Handle bool (must come before int check since bool is subclass of int in Python)
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(serde_json::Value::Bool(b));
    }

    // Handle int. Try i64 first, then u64 for large unsigned values (e.g. 64-bit IDs
    // and values in (i64::MAX, u64::MAX]) so they are not silently downgraded to f64
    // and corrupted. Integers beyond u64 still fall through to the float path.
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(serde_json::Value::Number(i.into()));
    }
    if let Ok(u) = obj.extract::<u64>() {
        return Ok(serde_json::Value::Number(u.into()));
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

    // Native ORM values (1.4.0 Phase 02).
    if let Ok(raw) = obj.downcast::<PyCell<RawJson>>() {
        return serde_json::from_slice(&raw.get().bytes).map_err(|e| e.to_string());
    }
    match classify(py, obj).map_err(|e| e.to_string())? {
        OrmKind::Model | OrmKind::ModelCustom => {
            let d = obj.call_method0("to_dict").map_err(|e| e.to_string())?;
            return python_to_json(py, d);
        }
        OrmKind::QuerySet => return Err(QUERYSET_HERE.to_string()),
        OrmKind::Other => {}
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
/// Returns Ok(Some((bytes, holes))) for dicts/lists/primitives and native ORM
/// values (a returned queryset leaves a [`Hole`]); Ok(None) for Response
/// objects (caller must fall back to python_to_json).
#[inline]
pub fn python_to_json_bytes_direct(
    py: Python<'_>,
    obj: &PyAny,
) -> Result<Option<(Vec<u8>, Holes)>, String> {
    let mut holes = Holes::new();
    // PERF: Check for dict/list FIRST (common case) before the expensive class name check.
    // Most handlers return dicts, so fast-path that.
    if obj.downcast::<PyDict>().is_ok() || obj.downcast::<PyList>().is_ok() {
        let mut buf = Vec::with_capacity(128);
        write_json_value(py, obj, &mut buf, &mut holes)?;
        return Ok(Some((buf, holes)));
    }

    // A queryset, model instance or RawJSON returned on its own.
    let mut buf = Vec::new();
    if write_orm(py, obj, &mut buf, &mut holes)? {
        return Ok(Some((buf, holes)));
    }

    // Check primitives
    if obj.is_none()
        || obj.extract::<bool>().is_ok()
        || obj.extract::<i64>().is_ok()
        || obj.extract::<f64>().is_ok()
        || obj.extract::<String>().is_ok()
    {
        let mut buf = Vec::with_capacity(64);
        write_json_value(py, obj, &mut buf, &mut holes)?;
        return Ok(Some((buf, holes)));
    }

    // Not a simple type - likely a Response object, fall back to Value path
    Ok(None)
}

/// Serialize `obj` exactly as a handler's return value is serialized, for
/// values that must be complete now (no querysets).
pub fn encode_value(py: Python<'_>, obj: &PyAny) -> Result<Vec<u8>, String> {
    let mut holes = Holes::new();
    let mut buf = Vec::with_capacity(128);
    write_json_value(py, obj, &mut buf, &mut holes)?;
    if !holes.is_empty() {
        return Err(QUERYSET_HERE.to_string());
    }
    Ok(buf)
}

/// Write a native ORM value: `RawJSON` verbatim, a model instance as its
/// `to_dict()`, a queryset as a [`Hole`]. `Ok(false)` for anything else.
fn write_orm(
    py: Python<'_>,
    obj: &PyAny,
    buf: &mut Vec<u8>,
    holes: &mut Holes,
) -> Result<bool, String> {
    if let Ok(raw) = obj.downcast::<PyCell<RawJson>>() {
        buf.extend_from_slice(&raw.get().bytes);
        return Ok(true);
    }
    match classify(py, obj).map_err(|e| e.to_string())? {
        OrmKind::Model => write_model(py, obj, buf, holes)?,
        OrmKind::ModelCustom => {
            let d = obj.call_method0("to_dict").map_err(|e| e.to_string())?;
            write_json_value(py, d, buf, holes)?;
        }
        OrmKind::QuerySet => holes.push(Hole {
            at: buf.len(),
            queryset: obj.into(),
        }),
        OrmKind::Other => return Ok(false),
    }
    Ok(true)
}

/// A model instance as the JSON of its `to_dict()`, written directly (no
/// intermediate dict). Keys and values come from the same plan as the native
/// `to_dict()` (`PyModelSpec::out_plan`).
fn write_model(
    py: Python<'_>,
    obj: &PyAny,
    buf: &mut Vec<u8>,
    holes: &mut Holes,
) -> Result<(), String> {
    let err = |e: PyErr| e.to_string();
    let cell = obj
        .getattr(pyo3::intern!(py, "_meta"))
        .and_then(|m| m.getattr(pyo3::intern!(py, "schema")))
        .map_err(err)?
        .downcast::<PyCell<PyModelSpec>>()
        .map_err(|e| e.to_string())?;
    let spec = cell.get();
    buf.push(b'{');
    for (i, entry) in spec.out_plan(py).iter().enumerate() {
        if i > 0 {
            buf.push(b',');
        }
        buf.extend_from_slice(&entry.key_json);
        let value = spec.out_value(obj, entry).map_err(err)?;
        write_json_value(py, value.as_ref(py), buf, holes)?;
    }
    buf.push(b'}');
    Ok(())
}

/// Write a Python object as JSON directly to a byte buffer.
fn write_json_value(
    py: Python<'_>,
    obj: &PyAny,
    buf: &mut Vec<u8>,
    holes: &mut Holes,
) -> Result<(), String> {
    use std::io::Write;

    // Handle None
    if obj.is_none() {
        buf.extend_from_slice(b"null");
        return Ok(());
    }

    // PERF: dispatch on the concrete type first. The extract chain below raises
    // (and discards) a Python exception for every failed attempt, so a string
    // used to pay for four exceptions before it was written. The chain stays as
    // the fallback for subclasses and foreign numeric types (`__index__`/`__float__`).
    if let Ok(s) = obj.downcast::<PyString>() {
        write_json_string(s.to_str().map_err(|e| e.to_string())?, buf);
        return Ok(());
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        return write_json_dict(py, dict, buf, holes);
    }
    if let Ok(list) = obj.downcast::<PyList>() {
        return write_json_seq(py, list.iter(), buf, holes);
    }
    if let Ok(b) = obj.downcast::<PyBool>() {
        buf.extend_from_slice(if b.is_true() { b"true" } else { b"false" });
        return Ok(());
    }
    if obj.downcast::<PyLong>().is_ok() {
        if let Ok(i) = obj.extract::<i64>() {
            write!(buf, "{i}").map_err(|e| e.to_string())?;
            return Ok(());
        }
    } else if let Ok(f) = obj.downcast::<PyFloat>() {
        write_json_f64(f.value(), buf);
        return Ok(());
    }

    // Native ORM values: RawJSON, model instances, querysets (Phase 02).
    if write_orm(py, obj, buf, holes)? {
        return Ok(());
    }

    // Handle bool (must come before int check since bool is subclass of int in Python)
    if let Ok(b) = obj.extract::<bool>() {
        buf.extend_from_slice(if b { b"true" } else { b"false" });
        return Ok(());
    }

    // Handle int. Try i64, then u64 for large unsigned values, so 64-bit IDs and
    // values in (i64::MAX, u64::MAX] are written exactly instead of being coerced to
    // an imprecise float. Integers beyond u64 still fall through to the float path.
    if let Ok(i) = obj.extract::<i64>() {
        write!(buf, "{i}").map_err(|e| e.to_string())?;
        return Ok(());
    }
    if let Ok(u) = obj.extract::<u64>() {
        write!(buf, "{u}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Handle float
    if let Ok(f) = obj.extract::<f64>() {
        write_json_f64(f, buf);
        return Ok(());
    }

    // Handle tuple
    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        return write_json_seq(py, tuple.iter(), buf, holes);
    }

    Err(format!("Cannot convert Python object to JSON: {obj:?}"))
}

/// A float as JSON: Rust's shortest round-trip `Display` (never an exponent),
/// and `null` for NaN / infinity.
#[inline]
pub(crate) fn write_json_f64(f: f64, buf: &mut Vec<u8>) {
    use std::io::Write;
    if f.is_finite() {
        // Writing to a Vec cannot fail.
        let _ = write!(buf, "{f}");
    } else {
        buf.extend_from_slice(b"null");
    }
}

/// An integer as JSON.
#[inline]
pub(crate) fn write_json_i64(i: i64, buf: &mut Vec<u8>) {
    use std::io::Write;
    let _ = write!(buf, "{i}");
}

fn write_json_seq<'py>(
    py: Python<'py>,
    items: impl Iterator<Item = &'py PyAny>,
    buf: &mut Vec<u8>,
    holes: &mut Holes,
) -> Result<(), String> {
    buf.push(b'[');
    for (i, item) in items.enumerate() {
        if i > 0 {
            buf.push(b',');
        }
        write_json_value(py, item, buf, holes)?;
    }
    buf.push(b']');
    Ok(())
}

fn write_json_dict(
    py: Python<'_>,
    dict: &PyDict,
    buf: &mut Vec<u8>,
    holes: &mut Holes,
) -> Result<(), String> {
    buf.push(b'{');
    let mut first = true;
    for (key, value) in dict.iter() {
        let key_str = key
            .downcast::<PyString>()
            .ok()
            .and_then(|k| k.to_str().ok())
            .ok_or_else(|| "Dict keys must be strings".to_string())?;
        if !first {
            buf.push(b',');
        }
        first = false;
        write_json_string(key_str, buf);
        buf.push(b':');
        write_json_value(py, value, buf, holes)?;
    }
    buf.push(b'}');
    Ok(())
}

/// Write a JSON-escaped string to the buffer.
#[inline]
pub(crate) fn write_json_string(s: &str, buf: &mut Vec<u8>) {
    // PERF: copy runs of bytes that need no escaping in one go.
    let bytes = s.as_bytes();
    buf.reserve(bytes.len() + 2);
    buf.push(b'"');
    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte >= 0x20 && byte != b'"' && byte != b'\\' {
            continue;
        }
        buf.extend_from_slice(&bytes[start..i]);
        start = i + 1;
        match byte {
            b'"' => buf.extend_from_slice(b"\\\""),
            b'\\' => buf.extend_from_slice(b"\\\\"),
            b'\n' => buf.extend_from_slice(b"\\n"),
            b'\r' => buf.extend_from_slice(b"\\r"),
            b'\t' => buf.extend_from_slice(b"\\t"),
            b => {
                // Control characters: \u00XX
                const HEX: &[u8; 16] = b"0123456789abcdef";
                buf.extend_from_slice(b"\\u00");
                buf.push(HEX[(b >> 4) as usize]);
                buf.push(HEX[(b & 0x0f) as usize]);
            }
        }
    }
    buf.extend_from_slice(&bytes[start..]);
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
