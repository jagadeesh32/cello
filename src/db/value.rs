//! Value conversion between Python objects and SQL / Redis wire types.
//!
//! This is the crux of the native data layer: Python is dynamically typed, but
//! Postgres parameters and results are strongly typed. We convert Python values
//! into a small [`SqlParam`] enum (which implements [`ToSql`] by dispatching on
//! the target parameter type) and convert result rows back into Python objects
//! by decoding each column's binary wire format based on its Postgres [`Type`].
//!
//! # Binding (`SqlParam::to_sql`)
//!
//! | Python value | Postgres parameter types it binds to |
//! |---|---|
//! | `None` | any (NULL) |
//! | `bool` | `bool`; `text`-like as `"true"` / `"false"` |
//! | `int` | `int2` / `int4` / `int8` / `oid` (range-checked), `float4` / `float8`, `numeric` (exact) |
//! | `float` | `float4` / `float8`, `numeric` (shortest round-trip digits) |
//! | `decimal.Decimal` | `numeric` (exact), `float4` / `float8` |
//! | `str` | `text`-like, `json`, `bytea`, `"char"`, enums and extension types (raw text); parsed for `numeric`, `uuid`, `date`, `time`, `timestamp`, `timestamptz` |
//! | `bytes` | `bytea`, `text`-like |
//! | `datetime` / `date` / `time` | `timestamptz`, `timestamp`, `date`, `time` |
//! | `uuid.UUID` | `uuid` |
//! | `list` / `tuple` | arrays (nested lists → multi-dimensional), `json` / `jsonb`, `text`-like (as JSON) |
//! | `dict` | `json` / `jsonb`, `text`-like (as JSON) |
//!
//! Anything else is a [`BindError`] naming the parameter and its Postgres type
//! (raised as `cello.ParameterTypeError` / `ParameterRangeError` /
//! `ParameterValueError`, all `DataError` → `DatabaseError` → `RuntimeError`).

use std::error::Error;

use bytes::{BufMut, BytesMut};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyList, PyLong, PyString, PyTuple};
use tokio_postgres::types::{to_sql_checked, FromSql, IsNull, Kind, ToSql, Type};
use tokio_postgres::Row;

use super::error::{decode_error, param_type_error, BindError, BindErrorKind};

type BoxErr = Box<dyn Error + Sync + Send>;

/// Microseconds per day.
const USECS_PER_DAY: i64 = 86_400_000_000;

/// A dynamically-typed SQL parameter converted from a Python object.
///
/// Encoding is chosen at bind time from the *target* Postgres type, so a Python
/// `int` binds correctly whether the parameter is `int2`, `int4`, `int8`,
/// `numeric` or a float.
#[derive(Debug, Clone)]
pub enum SqlParam {
    Null,
    Bool(bool),
    Int(i64),
    /// A Python `int` outside the `i64` range, as decimal text.
    BigInt(String),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Json(serde_json::Value),
    /// `decimal.Decimal`, as `str(value)`.
    Numeric(String),
    /// `datetime` / `date` / `time` object, as `value.isoformat()`.
    Temporal(TemporalKind, String),
    Uuid([u8; 16]),
    /// `list` / `tuple`: an array or a JSON array, depending on the target.
    List(Vec<SqlParam>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalKind {
    DateTime,
    Date,
    Time,
}

/// Python classes recognised on the slow path, imported once.
struct PyClasses {
    decimal: PyObject,
    datetime: PyObject,
    date: PyObject,
    time: PyObject,
    uuid: PyObject,
    json_dumps: PyObject,
}

static CLASSES: GILOnceCell<PyClasses> = GILOnceCell::new();

fn classes(py: Python<'_>) -> PyResult<&'static PyClasses> {
    CLASSES.get_or_try_init(py, || {
        let dt = py.import("datetime")?;
        Ok(PyClasses {
            decimal: py.import("decimal")?.getattr("Decimal")?.into(),
            datetime: dt.getattr("datetime")?.into(),
            date: dt.getattr("date")?.into(),
            time: dt.getattr("time")?.into(),
            uuid: py.import("uuid")?.getattr("UUID")?.into(),
            json_dumps: py.import("json")?.getattr("dumps")?.into(),
        })
    })
}

/// Convert a single Python object into a [`SqlParam`].
///
/// The common types (`None`, `bool`, `int`, `float`, `str`, `bytes`) are
/// checked first with exact-type tests; `bool` must come before `int` because
/// it is an `int` subclass.
pub fn py_to_sqlparam(obj: &PyAny) -> PyResult<SqlParam> {
    if obj.is_none() {
        return Ok(SqlParam::Null);
    }
    if let Ok(b) = obj.downcast::<PyBool>() {
        return Ok(SqlParam::Bool(b.is_true()));
    }
    if obj.downcast::<PyLong>().is_ok() {
        return Ok(match obj.extract::<i64>() {
            Ok(i) => SqlParam::Int(i),
            Err(_) => SqlParam::BigInt(obj.str()?.to_str()?.to_owned()),
        });
    }
    if let Ok(f) = obj.downcast::<PyFloat>() {
        return Ok(SqlParam::Float(f.value()));
    }
    if let Ok(s) = obj.downcast::<PyString>() {
        return Ok(SqlParam::Text(s.to_str()?.to_owned()));
    }
    if let Ok(b) = obj.downcast::<PyBytes>() {
        return Ok(SqlParam::Bytes(b.as_bytes().to_vec()));
    }
    py_to_sqlparam_slow(obj)
}

fn py_to_sqlparam_slow(obj: &PyAny) -> PyResult<SqlParam> {
    let py = obj.py();
    let c = classes(py)?;
    if obj.is_instance(c.decimal.as_ref(py))? {
        return Ok(SqlParam::Numeric(obj.str()?.to_str()?.to_owned()));
    }
    // `datetime` is a `date` subclass, so it is tested first.
    for (cls, kind) in [
        (&c.datetime, TemporalKind::DateTime),
        (&c.date, TemporalKind::Date),
        (&c.time, TemporalKind::Time),
    ] {
        if obj.is_instance(cls.as_ref(py))? {
            let iso: String = obj.call_method0("isoformat")?.extract()?;
            return Ok(SqlParam::Temporal(kind, iso));
        }
    }
    if obj.is_instance(c.uuid.as_ref(py))? {
        let bytes: &PyBytes = obj.getattr("bytes")?.downcast()?;
        let arr: [u8; 16] = bytes
            .as_bytes()
            .try_into()
            .map_err(|_| param_type_error(py, "uuid.UUID.bytes is not 16 bytes".into()))?;
        return Ok(SqlParam::Uuid(arr));
    }
    if let Ok(l) = obj.downcast::<PyList>() {
        return l
            .iter()
            .map(py_to_sqlparam)
            .collect::<PyResult<_>>()
            .map(SqlParam::List);
    }
    if let Ok(t) = obj.downcast::<PyTuple>() {
        return t
            .iter()
            .map(py_to_sqlparam)
            .collect::<PyResult<_>>()
            .map(SqlParam::List);
    }
    if !obj.is_instance_of::<PyDict>() {
        // int-like (`__index__`) and float-like (`__float__`) objects, e.g.
        // numpy scalars or `fractions.Fraction`.
        if let Ok(i) = obj.extract::<i64>() {
            return Ok(SqlParam::Int(i));
        }
        if let Ok(f) = obj.extract::<f64>() {
            return Ok(SqlParam::Float(f));
        }
    }
    // dict / anything JSON-serialisable → json/jsonb.
    let dumped: String = c.json_dumps.as_ref(py).call1((obj,))?.extract()?;
    let value: serde_json::Value = serde_json::from_str(&dumped).map_err(|e| {
        param_type_error(
            py,
            format!(
                "unsupported SQL parameter type ({}): {e}",
                obj.get_type().name().unwrap_or("?")
            ),
        )
    })?;
    Ok(SqlParam::Json(value))
}

/// Convert a Python sequence of parameters into `SqlParam`s.
pub fn py_params_to_sqlparams(params: &[&PyAny]) -> PyResult<Vec<SqlParam>> {
    params.iter().map(|p| py_to_sqlparam(p)).collect()
}

// ── Binding ──────────────────────────────────────────────────────────────────

fn bind_err(kind: BindErrorKind, msg: String) -> BoxErr {
    Box::new(BindError { kind, msg })
}

impl SqlParam {
    fn py_type_name(&self) -> &'static str {
        match self {
            SqlParam::Null => "None",
            SqlParam::Bool(_) => "bool",
            SqlParam::Int(_) | SqlParam::BigInt(_) => "int",
            SqlParam::Float(_) => "float",
            SqlParam::Text(_) => "str",
            SqlParam::Bytes(_) => "bytes",
            SqlParam::Json(_) => "dict",
            SqlParam::Numeric(_) => "Decimal",
            SqlParam::Temporal(TemporalKind::DateTime, _) => "datetime",
            SqlParam::Temporal(TemporalKind::Date, _) => "date",
            SqlParam::Temporal(TemporalKind::Time, _) => "time",
            SqlParam::Uuid(_) => "UUID",
            SqlParam::List(_) => "list",
        }
    }

    fn mismatch(&self, ty: &Type) -> BoxErr {
        bind_err(
            BindErrorKind::Type,
            format!(
                "expected a value for Postgres type {ty}, got Python {}",
                self.py_type_name()
            ),
        )
    }

    /// The JSON value for a `json` / `jsonb` / text target.
    fn to_json(&self) -> Result<serde_json::Value, BoxErr> {
        use serde_json::Value;
        Ok(match self {
            SqlParam::Null => Value::Null,
            SqlParam::Bool(b) => Value::Bool(*b),
            SqlParam::Int(i) => Value::from(*i),
            // Same as `json.dumps` + `serde_json::from_str` did in 1.3.0.
            SqlParam::BigInt(s) => serde_json::from_str(s)?,
            SqlParam::Float(f) => serde_json::Number::from_f64(*f)
                .map(Value::Number)
                .ok_or_else(|| {
                    bind_err(
                        BindErrorKind::Value,
                        format!("float {f} cannot be represented in JSON"),
                    )
                })?,
            SqlParam::Text(s) => Value::String(s.clone()),
            SqlParam::Json(v) => v.clone(),
            SqlParam::List(items) => Value::Array(
                items
                    .iter()
                    .map(SqlParam::to_json)
                    .collect::<Result<_, _>>()?,
            ),
            other => {
                return Err(bind_err(
                    BindErrorKind::Type,
                    format!("Python {} is not JSON serializable", other.py_type_name()),
                ))
            }
        })
    }
}

/// Builtin text types: a `bool` binds to these as `"true"` / `"false"`, and
/// JSON values as their text.
fn is_text_type(ty: &Type) -> bool {
    matches!(
        *ty,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN
    )
}

/// Types whose binary input format is the raw text bytes, so a Python `str`
/// is sent as-is: builtin text types, `json`, `xml`, `"char"`, `bytea`, enums,
/// and extension types (`citext`, …) — the 1.3.0 behaviour for those.
fn takes_raw_text(ty: &Type) -> bool {
    is_text_type(ty)
        || matches!(*ty, Type::JSON | Type::XML | Type::CHAR | Type::BYTEA)
        || matches!(ty.kind(), Kind::Enum(_))
        || (ty.oid() >= 16384 && matches!(ty.kind(), Kind::Simple))
}

impl ToSql for SqlParam {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, BoxErr> {
        if let Kind::Domain(base) = ty.kind() {
            return self.to_sql(base, out);
        }
        match self {
            SqlParam::Null => return Ok(IsNull::Yes),
            SqlParam::Bool(b) => match *ty {
                Type::BOOL => out.put_u8(*b as u8),
                _ if is_text_type(ty) => out.put_slice(if *b { b"true" } else { b"false" }),
                _ => return Err(self.mismatch(ty)),
            },
            SqlParam::Int(i) => bind_int(*i, ty, out).ok_or_else(|| self.mismatch(ty))??,
            SqlParam::BigInt(s) => match *ty {
                Type::NUMERIC => numeric_to_sql(s, out)?,
                Type::INT2 | Type::INT4 | Type::INT8 | Type::OID => {
                    return Err(out_of_range(s, ty))
                }
                Type::FLOAT4 | Type::FLOAT8 => {
                    let f: f64 = s.parse().map_err(|_| out_of_range(s, ty))?;
                    bind_float(f, ty, out).ok_or_else(|| out_of_range(s, ty))?;
                }
                _ => return Err(self.mismatch(ty)),
            },
            SqlParam::Float(f) => match *ty {
                Type::NUMERIC => numeric_to_sql(&float_text(*f), out)?,
                _ => bind_float(*f, ty, out).ok_or_else(|| self.mismatch(ty))?,
            },
            SqlParam::Numeric(s) => match *ty {
                Type::NUMERIC => numeric_to_sql(s, out)?,
                Type::FLOAT4 | Type::FLOAT8 => {
                    let f: f64 = s.parse().map_err(|_| invalid(s, ty))?;
                    bind_float(f, ty, out).ok_or_else(|| self.mismatch(ty))?;
                }
                _ => return Err(self.mismatch(ty)),
            },
            SqlParam::Text(s) => {
                if takes_raw_text(ty) {
                    out.put_slice(s.as_bytes());
                } else {
                    match *ty {
                        Type::NUMERIC => numeric_to_sql(s, out)?,
                        Type::UUID => {
                            let u = uuid::Uuid::parse_str(s.trim()).map_err(|_| invalid(s, ty))?;
                            out.put_slice(u.as_bytes());
                        }
                        Type::TIMESTAMPTZ | Type::TIMESTAMP | Type::DATE | Type::TIME => {
                            let p = parse_iso(s).ok_or_else(|| invalid(s, ty))?;
                            temporal_to_sql(&p, ty, out).ok_or_else(|| invalid(s, ty))?;
                        }
                        _ => {
                            return Err(bind_err(
                                BindErrorKind::Type,
                                format!(
                                    "expected a value for Postgres type {ty}, got Python str \
                                     (cast in SQL to parse text, e.g. $1::text::{ty})"
                                ),
                            ))
                        }
                    }
                }
            }
            SqlParam::Bytes(b) => match *ty {
                Type::BYTEA => out.put_slice(b),
                _ if is_text_type(ty) => out.put_slice(b),
                _ => return Err(self.mismatch(ty)),
            },
            SqlParam::Json(v) => match *ty {
                Type::JSON | Type::JSONB => {
                    v.to_sql(ty, out)?;
                }
                _ if is_text_type(ty) => {
                    v.to_sql(ty, out)?;
                }
                _ => return Err(self.mismatch(ty)),
            },
            SqlParam::Temporal(kind, iso) => {
                let ok = match kind {
                    TemporalKind::DateTime | TemporalKind::Date => {
                        matches!(*ty, Type::TIMESTAMPTZ | Type::TIMESTAMP | Type::DATE)
                    }
                    TemporalKind::Time => *ty == Type::TIME,
                };
                if !ok {
                    return Err(self.mismatch(ty));
                }
                let p = parse_iso(iso).ok_or_else(|| invalid(iso, ty))?;
                temporal_to_sql(&p, ty, out).ok_or_else(|| out_of_range(iso, ty))?;
            }
            SqlParam::Uuid(b) => match *ty {
                Type::UUID => out.put_slice(b),
                _ => return Err(self.mismatch(ty)),
            },
            SqlParam::List(items) => match ty.kind() {
                Kind::Array(elem) => array_to_sql(items, elem, out)?,
                _ if matches!(*ty, Type::JSON | Type::JSONB) || is_text_type(ty) => {
                    self.to_json()?.to_sql(ty, out)?;
                }
                _ => return Err(self.mismatch(ty)),
            },
        }
        Ok(IsNull::No)
    }

    // We dispatch on the target type inside `to_sql` and raise a `BindError`
    // naming the parameter for a genuine mismatch.
    fn accepts(_ty: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

fn out_of_range(v: &str, ty: &Type) -> BoxErr {
    bind_err(
        BindErrorKind::Range,
        format!("value {v} is out of range for Postgres type {ty}"),
    )
}

fn invalid(v: &str, ty: &Type) -> BoxErr {
    bind_err(
        BindErrorKind::Value,
        format!("invalid input for Postgres type {ty}: {v:?}"),
    )
}

/// `None` = type mismatch; `Some(Err)` = out of range.
fn bind_int(i: i64, ty: &Type, out: &mut BytesMut) -> Option<Result<(), BoxErr>> {
    let range = || Some(Err(out_of_range(&i.to_string(), ty)));
    match *ty {
        Type::INT2 => match i16::try_from(i) {
            Ok(v) => out.put_i16(v),
            Err(_) => return range(),
        },
        Type::INT4 => match i32::try_from(i) {
            Ok(v) => out.put_i32(v),
            Err(_) => return range(),
        },
        Type::INT8 => out.put_i64(i),
        Type::OID => match u32::try_from(i) {
            Ok(v) => out.put_u32(v),
            Err(_) => return range(),
        },
        Type::FLOAT4 => out.put_f32(i as f32),
        Type::FLOAT8 => out.put_f64(i as f64),
        Type::NUMERIC => return Some(numeric_to_sql(&i.to_string(), out)),
        _ => return None,
    }
    Some(Ok(()))
}

/// `None` = type mismatch.
fn bind_float(f: f64, ty: &Type, out: &mut BytesMut) -> Option<()> {
    match *ty {
        Type::FLOAT4 => out.put_f32(f as f32),
        Type::FLOAT8 => out.put_f64(f),
        _ => return None,
    }
    Some(())
}

/// Shortest round-trip decimal text of a float (`NaN` / `inf` / `-inf` for
/// the non-finite values, which `numeric_to_sql` understands).
fn float_text(f: f64) -> String {
    if f.is_nan() {
        "NaN".into()
    } else if f.is_infinite() {
        if f > 0.0 { "inf" } else { "-inf" }.into()
    } else {
        format!("{f}")
    }
}

/// Encode a list as a Postgres array. Nested lists become a multi-dimensional
/// array and must be rectangular.
fn array_to_sql(items: &[SqlParam], elem: &Type, out: &mut BytesMut) -> Result<(), BoxErr> {
    // Dimensions: follow the first element down while it is a list. For
    // `json[]` / `jsonb[]` a nested list is a JSON array element instead.
    let json_elems = matches!(*elem, Type::JSON | Type::JSONB);
    let mut dims: Vec<i32> = Vec::new();
    if !items.is_empty() {
        let mut level = items;
        loop {
            dims.push(i32::try_from(level.len()).map_err(|_| array_err("too many elements"))?);
            if json_elems {
                break;
            }
            match level.first() {
                Some(SqlParam::List(inner)) if !inner.is_empty() => level = inner,
                Some(SqlParam::List(_)) => {
                    return Err(array_err("nested empty lists cannot form an array"))
                }
                _ => break,
            }
        }
    }
    out.put_i32(dims.len() as i32);
    let has_nulls_at = out.len();
    out.put_i32(0);
    out.put_u32(elem.oid());
    for d in &dims {
        out.put_i32(*d);
        out.put_i32(1); // lower bound
    }
    let mut has_nulls = false;
    if !dims.is_empty() {
        write_array_level(items, &dims, elem, out, &mut has_nulls)?;
    }
    if has_nulls {
        out[has_nulls_at..has_nulls_at + 4].copy_from_slice(&1i32.to_be_bytes());
    }
    Ok(())
}

fn array_err(msg: &str) -> BoxErr {
    bind_err(
        BindErrorKind::Value,
        format!("invalid array parameter: {msg}"),
    )
}

fn write_array_level(
    items: &[SqlParam],
    dims: &[i32],
    elem: &Type,
    out: &mut BytesMut,
    has_nulls: &mut bool,
) -> Result<(), BoxErr> {
    if items.len() as i32 != dims[0] {
        return Err(array_err("nested lists must all have the same length"));
    }
    for item in items {
        if dims.len() > 1 {
            match item {
                SqlParam::List(inner) => {
                    write_array_level(inner, &dims[1..], elem, out, has_nulls)?
                }
                _ => return Err(array_err("nested lists must all have the same depth")),
            }
            continue;
        }
        if matches!(item, SqlParam::List(_)) && !matches!(*elem, Type::JSON | Type::JSONB) {
            return Err(array_err("nested lists must all have the same depth"));
        }
        let len_at = out.len();
        out.put_i32(0);
        match item.to_sql(elem, out)? {
            IsNull::Yes => {
                *has_nulls = true;
                out[len_at..len_at + 4].copy_from_slice(&(-1i32).to_be_bytes());
            }
            IsNull::No => {
                let n = i32::try_from(out.len() - len_at - 4)
                    .map_err(|_| array_err("element too large"))?;
                out[len_at..len_at + 4].copy_from_slice(&n.to_be_bytes());
            }
        }
    }
    Ok(())
}

// ── numeric ──────────────────────────────────────────────────────────────────

const NUMERIC_POS: u16 = 0x0000;
const NUMERIC_NEG: u16 = 0x4000;
const NUMERIC_NAN: u16 = 0xC000;
const NUMERIC_PINF: u16 = 0xD000;
const NUMERIC_NINF: u16 = 0xF000;

/// Encode decimal text (`-12.340`, `1E+3`, `1.5e-07`, `NaN`, `Infinity`,
/// `inf`) in Postgres' binary `numeric` format: base-10000 digit groups plus
/// weight, sign and display scale. The display scale is the number of digits
/// after the point, as `numeric_in` would compute it.
fn numeric_to_sql(s: &str, out: &mut BytesMut) -> Result<(), BoxErr> {
    let t = s.trim();
    let special = match t.to_ascii_lowercase().as_str() {
        "nan" => Some(NUMERIC_NAN),
        "inf" | "+inf" | "infinity" | "+infinity" => Some(NUMERIC_PINF),
        "-inf" | "-infinity" => Some(NUMERIC_NINF),
        _ => None,
    };
    if let Some(sign) = special {
        out.put_i16(0);
        out.put_i16(0);
        out.put_u16(sign);
        out.put_u16(0);
        return Ok(());
    }
    let ty = Type::NUMERIC;
    let bad = || invalid(s, &ty);
    let bytes = t.as_bytes();
    let mut i = 0;
    let mut neg = false;
    if let Some(&c) = bytes.first() {
        if c == b'-' || c == b'+' {
            neg = c == b'-';
            i = 1;
        }
    }
    let mut digits: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut frac_len: i64 = 0;
    let mut seen_point = false;
    let mut any_digit = false;
    while i < bytes.len() {
        match bytes[i] {
            c @ b'0'..=b'9' => {
                any_digit = true;
                // Skip leading zeros; they do not change the value.
                if !(digits.is_empty() && c == b'0') {
                    digits.push(c - b'0');
                }
                if seen_point {
                    frac_len += 1;
                }
            }
            b'.' if !seen_point => seen_point = true,
            b'e' | b'E' => break,
            _ => return Err(bad()),
        }
        i += 1;
    }
    if !any_digit {
        return Err(bad());
    }
    let mut exp: i64 = 0;
    if i < bytes.len() {
        // exponent
        let e = std::str::from_utf8(&bytes[i + 1..]).map_err(|_| bad())?;
        exp = e.parse::<i64>().map_err(|_| bad())?;
        if exp.abs() > 1_000_000 {
            return Err(out_of_range(s, &ty));
        }
    }
    // value = digits × 10^exp10
    let mut exp10 = exp - frac_len;
    let dscale = (-exp10).max(0);
    if dscale > 0x3FFF {
        return Err(out_of_range(s, &ty));
    }
    while digits.last() == Some(&0) {
        digits.pop();
        exp10 += 1;
    }
    if digits.is_empty() {
        out.put_i16(0);
        out.put_i16(0);
        out.put_u16(NUMERIC_POS);
        out.put_u16(dscale as u16);
        return Ok(());
    }
    let n = digits.len() as i64;
    let p_max = exp10 + n - 1;
    let weight = p_max.div_euclid(4);
    let low = exp10.div_euclid(4);
    let ngroups = weight - low + 1;
    if weight > i16::MAX as i64 || weight < i16::MIN as i64 || ngroups > i16::MAX as i64 {
        return Err(out_of_range(s, &ty));
    }
    let mut groups = vec![0i16; ngroups as usize];
    for (k, d) in digits.iter().enumerate() {
        let p = exp10 + (n - 1 - k as i64);
        let g = p.div_euclid(4);
        let pos = p - g * 4;
        groups[(weight - g) as usize] += *d as i16 * 10i16.pow(pos as u32);
    }
    out.reserve(8 + 2 * groups.len());
    out.put_i16(ngroups as i16);
    out.put_i16(weight as i16);
    out.put_u16(if neg { NUMERIC_NEG } else { NUMERIC_POS });
    out.put_u16(dscale as u16);
    for g in groups {
        out.put_i16(g);
    }
    Ok(())
}

/// Decode Postgres' binary `numeric` to its exact text form (what `numeric_out`
/// prints): `"12.3400"`, `"-0.5"`, `"0"`, `"NaN"`, `"Infinity"`, `"-Infinity"`.
fn numeric_from_sql(buf: &[u8]) -> Result<String, BoxErr> {
    if buf.len() < 8 {
        return Err("invalid numeric: too short".into());
    }
    let ndigits = i16::from_be_bytes([buf[0], buf[1]]);
    let weight = i16::from_be_bytes([buf[2], buf[3]]) as i32;
    let sign = u16::from_be_bytes([buf[4], buf[5]]);
    let dscale = u16::from_be_bytes([buf[6], buf[7]]) as i32;
    match sign {
        NUMERIC_NAN => return Ok("NaN".into()),
        NUMERIC_PINF => return Ok("Infinity".into()),
        NUMERIC_NINF => return Ok("-Infinity".into()),
        NUMERIC_POS | NUMERIC_NEG => {}
        _ => return Err("invalid numeric sign".into()),
    }
    if ndigits < 0 || buf.len() != 8 + 2 * ndigits as usize {
        return Err("invalid numeric length".into());
    }
    let nd = ndigits as i32;
    let digit = |d: i32| -> u16 {
        if d >= 0 && d < nd {
            let o = 8 + 2 * d as usize;
            i16::from_be_bytes([buf[o], buf[o + 1]]) as u16
        } else {
            0
        }
    };
    let int_groups = (weight + 1).max(1) as usize;
    let mut s = String::with_capacity(1 + int_groups * 4 + 1 + dscale.max(0) as usize + 3);
    if sign == NUMERIC_NEG {
        s.push('-');
    }
    if weight < 0 {
        s.push('0');
    } else {
        for d in 0..=weight {
            let g = digit(d);
            if g > 9999 {
                return Err("invalid numeric digit".into());
            }
            if d == 0 {
                // No leading zeros on the first group.
                let mut started = false;
                for div in [1000, 100, 10, 1] {
                    let c = (g / div) % 10;
                    if c != 0 || started || div == 1 {
                        started = true;
                        s.push((b'0' + c as u8) as char);
                    }
                }
            } else {
                push4(&mut s, g);
            }
        }
    }
    if dscale > 0 {
        s.push('.');
        let mut written = 0;
        let mut d = weight + 1;
        while written < dscale {
            let g = digit(d);
            let take = (dscale - written).min(4);
            let mut four = [0u8; 4];
            for (k, div) in [1000, 100, 10, 1].into_iter().enumerate() {
                four[k] = b'0' + ((g / div) % 10) as u8;
            }
            for &c in &four[..take as usize] {
                s.push(c as char);
            }
            written += take;
            d += 1;
        }
    }
    Ok(s)
}

fn push4(s: &mut String, g: u16) {
    for div in [1000, 100, 10, 1] {
        s.push((b'0' + ((g / div) % 10) as u8) as char);
    }
}

// ── temporal parsing (bind side) ─────────────────────────────────────────────

/// A parsed ISO-8601 / Postgres-style date, time or timestamp.
#[derive(Debug, PartialEq, Eq)]
enum Iso {
    Finite {
        /// Days since 2000-01-01.
        date: Option<i32>,
        /// Microseconds since midnight.
        time: Option<i64>,
        /// UTC offset in seconds east.
        offset: Option<i32>,
    },
    PosInf,
    NegInf,
}

fn digits_n(b: &[u8], at: usize, n: usize) -> Option<u32> {
    let s = b.get(at..at + n)?;
    let mut v = 0u32;
    for &c in s {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + (c - b'0') as u32;
    }
    Some(v)
}

/// Parse `YYYY-MM-DD`, `HH:MM[:SS[.ffffff]]`, or both joined by `T` / space,
/// with an optional `Z` / `±HH[:MM[:SS]]` offset. Also `infinity` /
/// `-infinity`.
fn parse_iso(s: &str) -> Option<Iso> {
    let s = s.trim();
    match s.to_ascii_lowercase().as_str() {
        "infinity" | "+infinity" => return Some(Iso::PosInf),
        "-infinity" => return Some(Iso::NegInf),
        _ => {}
    }
    let b = s.as_bytes();
    let mut i = 0;
    let mut date = None;
    // A date starts with 4+ digits followed by '-'.
    let year_len = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if year_len >= 4 && b.get(year_len) == Some(&b'-') {
        let year: i32 = s[..year_len].parse().ok()?;
        let month = digits_n(b, year_len + 1, 2)?;
        if b.get(year_len + 3) != Some(&b'-') {
            return None;
        }
        let day = digits_n(b, year_len + 4, 2)?;
        let d = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
        let epoch = chrono::NaiveDate::from_ymd_opt(2000, 1, 1)?;
        date = Some(i32::try_from((d - epoch).num_days()).ok()?);
        i = year_len + 6;
        if i < b.len() {
            if matches!(b[i], b'T' | b't' | b' ') {
                i += 1;
            } else {
                return None;
            }
        }
    }
    let mut time = None;
    let mut offset = None;
    if i < b.len() {
        let hh = digits_n(b, i, 2)? as i64;
        if b.get(i + 2) != Some(&b':') {
            return None;
        }
        let mm = digits_n(b, i + 3, 2)? as i64;
        i += 5;
        let mut ss = 0i64;
        let mut micros = 0i64;
        if b.get(i) == Some(&b':') {
            ss = digits_n(b, i + 1, 2)? as i64;
            i += 3;
            if b.get(i) == Some(&b'.') {
                i += 1;
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let frac = &s[start..i];
                if frac.is_empty() {
                    return None;
                }
                // Round to microseconds, as Postgres does.
                let mut digits: Vec<u8> = frac.bytes().map(|c| c - b'0').collect();
                digits.resize(digits.len().max(7), 0);
                let mut us = 0i64;
                for d in &digits[..6] {
                    us = us * 10 + *d as i64;
                }
                if digits[6] >= 5 {
                    us += 1;
                }
                micros = us;
            }
        }
        if mm > 59 || ss > 60 || hh > 24 || (hh == 24 && (mm, ss, micros) != (0, 0, 0)) {
            return None;
        }
        time = Some(((hh * 60 + mm) * 60 + ss) * 1_000_000 + micros);
        if b.get(i) == Some(&b' ') {
            i += 1;
        }
        if i < b.len() {
            match b[i] {
                b'Z' | b'z' if i + 1 == b.len() => offset = Some(0),
                c @ (b'+' | b'-') => {
                    let rest = &b[i + 1..];
                    let mut parts = [0i32; 3];
                    let mut k = 0;
                    let mut j = 0;
                    while j < rest.len() {
                        if k == 3 {
                            return None;
                        }
                        parts[k] = digits_n(rest, j, 2)? as i32;
                        k += 1;
                        j += 2;
                        if j < rest.len() && rest[j] == b':' {
                            j += 1;
                            if j == rest.len() {
                                return None;
                            }
                        }
                    }
                    if k == 0 || parts[0] > 15 || parts[1] > 59 || parts[2] > 59 {
                        return None;
                    }
                    let secs = parts[0] * 3600 + parts[1] * 60 + parts[2];
                    offset = Some(if c == b'-' { -secs } else { secs });
                }
                _ => return None,
            }
        }
    }
    if date.is_none() && time.is_none() {
        return None;
    }
    Some(Iso::Finite { date, time, offset })
}

/// Encode a parsed value for a `timestamptz` / `timestamp` / `date` / `time`
/// parameter. `None` when the shape does not fit the target (e.g. a bare time
/// for a date) or the value overflows.
///
/// * `timestamptz`: a value without an offset is taken as UTC.
/// * `timestamp`: an offset is ignored (the wall-clock time is kept), as
///   Postgres does when it parses text into `timestamp`.
/// * `date`: a time part is ignored, as Postgres does.
/// * `time`: a date part is not accepted; an offset is ignored.
fn temporal_to_sql(p: &Iso, ty: &Type, out: &mut BytesMut) -> Option<()> {
    let is_ts = matches!(*ty, Type::TIMESTAMPTZ | Type::TIMESTAMP);
    match p {
        Iso::PosInf if is_ts => out.put_i64(i64::MAX),
        Iso::NegInf if is_ts => out.put_i64(i64::MIN),
        Iso::PosInf if *ty == Type::DATE => out.put_i32(i32::MAX),
        Iso::NegInf if *ty == Type::DATE => out.put_i32(i32::MIN),
        Iso::Finite {
            date: Some(d),
            time,
            offset,
        } if is_ts => {
            let mut us = (*d as i64)
                .checked_mul(USECS_PER_DAY)?
                .checked_add(time.unwrap_or(0))?;
            if *ty == Type::TIMESTAMPTZ {
                us = us.checked_sub(offset.unwrap_or(0) as i64 * 1_000_000)?;
            }
            out.put_i64(us);
        }
        Iso::Finite { date: Some(d), .. } if *ty == Type::DATE => out.put_i32(*d),
        Iso::Finite {
            date: None,
            time: Some(t),
            ..
        } if *ty == Type::TIME => out.put_i64(*t),
        _ => return None,
    }
    Some(())
}

// ── Decoding ─────────────────────────────────────────────────────────────────

/// Borrow a column's raw wire bytes (binary format) for any type.
struct Raw<'a>(&'a [u8]);

impl<'a> FromSql<'a> for Raw<'a> {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, BoxErr> {
        Ok(Raw(raw))
    }
    fn accepts(_: &Type) -> bool {
        true
    }
}

fn be_i16(b: &[u8]) -> Result<i16, BoxErr> {
    Ok(i16::from_be_bytes(
        b.try_into().map_err(|_| "invalid int2 length")?,
    ))
}
fn be_i32(b: &[u8]) -> Result<i32, BoxErr> {
    Ok(i32::from_be_bytes(
        b.try_into().map_err(|_| "invalid 4-byte value length")?,
    ))
}
fn be_i64(b: &[u8]) -> Result<i64, BoxErr> {
    Ok(i64::from_be_bytes(
        b.try_into().map_err(|_| "invalid 8-byte value length")?,
    ))
}

/// One decoded column value, before it becomes a Python object or JSON text.
///
/// There is exactly one type dispatch ([`decode_cell`]) and two consumers:
/// [`cell_to_py`] (`fetch`, `fetchrow`, ORM hydration) and [`write_cell_json`]
/// (the row → JSON fast path, which runs on the I/O thread without the GIL).
/// Because both read the same `Cell`, a value can never have one shape in a
/// Python dict and another in a JSON response.
pub(crate) enum Cell<'a> {
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Borrowed from the wire buffer (text-like types, enums, infinities).
    Str(&'a str),
    /// Rendered text (timestamps, numeric, interval, inet, ...).
    Text(String),
    /// Lower-case hyphenated UUID.
    Uuid([u8; uuid::fmt::Hyphenated::LENGTH]),
    Bytes(&'a [u8]),
    Json(serde_json::Value),
    /// Dimension lengths (outermost first) and the elements in row-major
    /// order; `None` elements are SQL NULLs.
    Array(Vec<usize>, Vec<Option<Cell<'a>>>),
}

/// Decode one non-NULL value. `Ok(None)` = the type is not supported.
pub(crate) fn decode_cell<'a>(ty: &Type, buf: &'a [u8]) -> Result<Option<Cell<'a>>, BoxErr> {
    let cell = match *ty {
        Type::BOOL => match buf {
            [b] => Cell::Bool(*b != 0),
            _ => return Err("invalid bool length".into()),
        },
        Type::INT2 => Cell::Int(be_i16(buf)? as i64),
        Type::INT4 => Cell::Int(be_i32(buf)? as i64),
        Type::INT8 => Cell::Int(be_i64(buf)?),
        Type::OID => Cell::Int(be_i32(buf)? as u32 as i64),
        Type::FLOAT4 => Cell::Float(f32::from_bits(be_i32(buf)? as u32) as f64),
        Type::FLOAT8 => Cell::Float(f64::from_bits(be_i64(buf)? as u64)),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN => {
            Cell::Str(std::str::from_utf8(buf)?)
        }
        // The one-byte internal "char" type.
        Type::CHAR => match buf {
            [] | [0] => Cell::Str(""),
            [c] => Cell::Text((*c as char).to_string()),
            _ => return Err("invalid \"char\" length".into()),
        },
        Type::BYTEA => Cell::Bytes(buf),
        Type::JSON | Type::JSONB => Cell::Json(serde_json::Value::from_sql(ty, buf)?),
        Type::UUID => {
            let u = uuid::Uuid::from_slice(buf)?;
            let mut tmp = [0u8; uuid::fmt::Hyphenated::LENGTH];
            u.hyphenated().encode_lower(&mut tmp);
            Cell::Uuid(tmp)
        }
        Type::TIMESTAMP => match be_i64(buf)? {
            i64::MAX => Cell::Str("infinity"),
            i64::MIN => Cell::Str("-infinity"),
            _ => Cell::Text(
                chrono::NaiveDateTime::from_sql(ty, buf)?
                    .format("%Y-%m-%dT%H:%M:%S%.6f")
                    .to_string(),
            ),
        },
        Type::TIMESTAMPTZ => match be_i64(buf)? {
            i64::MAX => Cell::Str("infinity"),
            i64::MIN => Cell::Str("-infinity"),
            _ => Cell::Text(chrono::DateTime::<chrono::Utc>::from_sql(ty, buf)?.to_rfc3339()),
        },
        Type::DATE => match be_i32(buf)? {
            i32::MAX => Cell::Str("infinity"),
            i32::MIN => Cell::Str("-infinity"),
            _ => Cell::Text(chrono::NaiveDate::from_sql(ty, buf)?.to_string()),
        },
        Type::TIME => Cell::Text(chrono::NaiveTime::from_sql(ty, buf)?.to_string()),
        Type::NUMERIC => Cell::Text(numeric_from_sql(buf)?),
        Type::INTERVAL => Cell::Text(interval_text(buf)?),
        Type::TIMETZ => Cell::Text(timetz_text(buf)?),
        Type::MONEY => Cell::Text(money_text(be_i64(buf)?)),
        Type::INET | Type::CIDR => Cell::Text(inet_text(buf, *ty == Type::CIDR)?),
        Type::MACADDR | Type::MACADDR8 => {
            if buf.len() != if *ty == Type::MACADDR { 6 } else { 8 } {
                return Err("invalid macaddr length".into());
            }
            let parts: Vec<String> = buf.iter().map(|b| format!("{b:02x}")).collect();
            Cell::Text(parts.join(":"))
        }
        _ => match ty.kind() {
            Kind::Array(elem) => return decode_array_cell(elem, buf),
            Kind::Domain(base) => return decode_cell(base, buf),
            Kind::Enum(_) => Cell::Str(std::str::from_utf8(buf)?),
            // Extension text types (`citext`, `ltree`, …): same text decode as 1.3.0.
            _ if <&str as FromSql>::accepts(ty) => Cell::Str(<&str as FromSql>::from_sql(ty, buf)?),
            _ => return Ok(None),
        },
    };
    Ok(Some(cell))
}

/// Decode an array. Lower bounds are ignored. `Ok(None)` when the element
/// type is unsupported (the whole column then decodes to the placeholder).
fn decode_array_cell<'a>(elem: &Type, buf: &'a [u8]) -> Result<Option<Cell<'a>>, BoxErr> {
    let short = || -> BoxErr { "invalid array: truncated".into() };
    let ndim = be_i32(buf.get(0..4).ok_or_else(short)?)?;
    if !(0..=6).contains(&ndim) {
        return Err("invalid array: bad dimension count".into());
    }
    let mut pos = 12;
    let mut dims = Vec::with_capacity(ndim as usize);
    let mut total: usize = 1;
    for _ in 0..ndim {
        let len = be_i32(buf.get(pos..pos + 4).ok_or_else(short)?)?;
        if len < 0 {
            return Err("invalid array: negative dimension".into());
        }
        total = total.saturating_mul(len as usize);
        dims.push(len as usize);
        pos += 8;
    }
    if ndim == 0 {
        return Ok(Some(Cell::Array(vec![0], Vec::new())));
    }
    let mut values = Vec::with_capacity(total.min(1 << 20));
    for _ in 0..total {
        let len = be_i32(buf.get(pos..pos + 4).ok_or_else(short)?)?;
        pos += 4;
        if len < 0 {
            values.push(None);
            continue;
        }
        let end = pos + len as usize;
        let v = buf.get(pos..end).ok_or_else(short)?;
        pos = end;
        match decode_cell(elem, v)? {
            Some(c) => values.push(Some(c)),
            None => return Ok(None),
        }
    }
    Ok(Some(Cell::Array(dims, values)))
}

/// Build the Python object for a decoded value.
pub(crate) fn cell_to_py(py: Python<'_>, cell: Cell<'_>) -> PyResult<PyObject> {
    Ok(match cell {
        Cell::Bool(b) => b.into_py(py),
        Cell::Int(i) => i.into_py(py),
        Cell::Float(f) => f.into_py(py),
        Cell::Str(s) => py_str(py, s)?,
        Cell::Text(s) => py_str(py, &s)?,
        // Hex digits and hyphens only: always valid UTF-8.
        Cell::Uuid(tmp) => py_str(py, std::str::from_utf8(&tmp).unwrap_or_default())?,
        Cell::Bytes(b) => PyBytes::new(py, b).into_py(py),
        Cell::Json(v) => json_to_py(py, &v)?,
        Cell::Array(dims, values) => {
            let mut it = values.into_iter();
            nest(py, &dims, &mut it)?
        }
    })
}

fn nest<'a>(
    py: Python<'_>,
    dims: &[usize],
    it: &mut std::vec::IntoIter<Option<Cell<'a>>>,
) -> PyResult<PyObject> {
    let list = PyList::empty(py);
    for _ in 0..dims[0] {
        let item = if dims.len() == 1 {
            match it.next().flatten() {
                Some(c) => cell_to_py(py, c)?,
                None => py.None(),
            }
        } else {
            nest(py, &dims[1..], it)?
        };
        list.append(item)?;
    }
    Ok(list.into_py(py))
}

/// A decoded value that has no JSON form.
#[derive(Debug)]
pub(crate) struct NotJson(pub &'static str);

/// Write a decoded value as JSON, byte-for-byte what the response serializer
/// (`src/json.rs`) writes for the Python object [`cell_to_py`] builds.
pub(crate) fn write_cell_json(cell: &Cell<'_>, out: &mut Vec<u8>) -> Result<(), NotJson> {
    use crate::json::{write_json_f64, write_json_i64, write_json_string};
    match cell {
        Cell::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Cell::Int(i) => write_json_i64(*i, out),
        Cell::Float(f) => write_json_f64(*f, out),
        Cell::Str(s) => write_json_string(s, out),
        Cell::Text(s) => write_json_string(s, out),
        Cell::Uuid(tmp) => write_json_string(std::str::from_utf8(tmp).unwrap_or_default(), out),
        // `bytes` is not JSON-serializable on the Python path either.
        Cell::Bytes(_) => return Err(NotJson("bytes")),
        Cell::Json(v) => write_serde_json(v, out),
        Cell::Array(dims, values) => {
            let mut it = values.iter();
            write_array_json(dims, &mut it, out)?;
        }
    }
    Ok(())
}

fn write_array_json<'c, 'a: 'c>(
    dims: &[usize],
    it: &mut std::slice::Iter<'c, Option<Cell<'a>>>,
    out: &mut Vec<u8>,
) -> Result<(), NotJson> {
    out.push(b'[');
    for i in 0..dims[0] {
        if i > 0 {
            out.push(b',');
        }
        if dims.len() == 1 {
            match it.next() {
                Some(Some(c)) => write_cell_json(c, out)?,
                _ => out.extend_from_slice(b"null"),
            }
        } else {
            write_array_json(&dims[1..], it, out)?;
        }
    }
    out.push(b']');
    Ok(())
}

/// JSON for a `json` / `jsonb` value, as [`json_to_py`] followed by the
/// response serializer would write it: object keys in the map's order,
/// integers exactly (`i64`, then `u64`), every other number as an `f64`.
fn write_serde_json(v: &serde_json::Value, out: &mut Vec<u8>) {
    use crate::json::{write_json_f64, write_json_i64, write_json_string};
    use serde_json::Value;
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                write_json_i64(i, out);
            } else if let Some(u) = n.as_u64() {
                use std::io::Write;
                let _ = write!(out, "{u}");
            } else {
                write_json_f64(n.as_f64().unwrap_or(0.0), out);
            }
        }
        Value::String(s) => write_json_string(s, out),
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_serde_json(item, out);
            }
            out.push(b']');
        }
        Value::Object(map) => {
            out.push(b'{');
            for (i, (k, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_json_string(k, out);
                out.push(b':');
                write_serde_json(item, out);
            }
            out.push(b'}');
        }
    }
}

/// `HH:MM:SS[.ffffff]` with trailing fractional zeros trimmed, as Postgres
/// prints times. `t` may exceed 24h (intervals).
fn push_hms(s: &mut String, t: i64) {
    use std::fmt::Write;
    let neg = t < 0;
    let t = t.unsigned_abs();
    let us = t % 1_000_000;
    let secs = t / 1_000_000;
    let (h, m, sec) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if neg {
        s.push('-');
    }
    let _ = write!(s, "{h:02}:{m:02}:{sec:02}");
    if us != 0 {
        let frac = format!("{us:06}");
        s.push('.');
        s.push_str(frac.trim_end_matches('0'));
    }
}

/// `interval` in Postgres' default (`IntervalStyle = postgres`) output format:
/// `1 year 2 mons 3 days 04:05:06.5`, `-1 days +02:00:00`, `00:00:00`.
fn interval_text(buf: &[u8]) -> Result<String, BoxErr> {
    use std::fmt::Write;
    if buf.len() != 16 {
        return Err("invalid interval length".into());
    }
    let time = be_i64(&buf[0..8])?;
    let days = be_i32(&buf[8..12])?;
    let months = be_i32(&buf[12..16])?;
    if (time, days, months) == (i64::MAX, i32::MAX, i32::MAX) {
        return Ok("infinity".into());
    }
    if (time, days, months) == (i64::MIN, i32::MIN, i32::MIN) {
        return Ok("-infinity".into());
    }
    let mut s = String::new();
    let mut is_zero = true;
    let mut is_before = false;
    let mut part = |s: &mut String, v: i64, unit: &str| {
        if v == 0 {
            return;
        }
        let _ = write!(
            s,
            "{}{}{} {}{}",
            if is_zero { "" } else { " " },
            if is_before && v > 0 { "+" } else { "" },
            v,
            unit,
            if v != 1 { "s" } else { "" }
        );
        is_before = v < 0;
        is_zero = false;
    };
    part(&mut s, (months / 12) as i64, "year");
    part(&mut s, (months % 12) as i64, "mon");
    part(&mut s, days as i64, "day");
    if is_zero || time != 0 {
        if !is_zero {
            s.push(' ');
        }
        if time >= 0 && is_before {
            s.push('+');
        }
        push_hms(&mut s, time);
    }
    Ok(s)
}

/// `timetz` as Postgres prints it: `13:14:15+02`, `13:14:15.25-05:30`.
fn timetz_text(buf: &[u8]) -> Result<String, BoxErr> {
    use std::fmt::Write;
    if buf.len() != 12 {
        return Err("invalid timetz length".into());
    }
    let time = be_i64(&buf[0..8])?;
    // Seconds WEST of UTC.
    let zone = be_i32(&buf[8..12])?;
    let mut s = String::with_capacity(24);
    push_hms(&mut s, time);
    let abs = zone.unsigned_abs();
    let (h, m, sec) = (abs / 3600, (abs / 60) % 60, abs % 60);
    s.push(if zone <= 0 { '+' } else { '-' });
    if sec != 0 {
        let _ = write!(s, "{h:02}:{m:02}:{sec:02}");
    } else if m != 0 {
        let _ = write!(s, "{h:02}:{m:02}");
    } else {
        let _ = write!(s, "{h:02}");
    }
    Ok(s)
}

/// `money` as a plain decimal with two fractional digits (`"12.50"`,
/// `"-0.05"`). Postgres' own text form is locale-formatted (`$12.50`).
fn money_text(cents: i64) -> String {
    let abs = cents.unsigned_abs();
    format!(
        "{}{}.{:02}",
        if cents < 0 { "-" } else { "" },
        abs / 100,
        abs % 100
    )
}

/// `inet` / `cidr` text: `10.0.0.1`, `10.0.0.0/8`, `::1`, `2001:db8::/32`.
fn inet_text(buf: &[u8], cidr: bool) -> Result<String, BoxErr> {
    if buf.len() < 4 {
        return Err("invalid inet".into());
    }
    let (family, bits, nb) = (buf[0], buf[1], buf[3] as usize);
    let addr = &buf[4..];
    if addr.len() != nb {
        return Err("invalid inet length".into());
    }
    let (ip, max): (std::net::IpAddr, u8) = match (family, nb) {
        (2, 4) => (<[u8; 4]>::try_from(addr)?.into(), 32),
        (3, 16) => (<[u8; 16]>::try_from(addr)?.into(), 128),
        _ => return Err("invalid inet family".into()),
    };
    Ok(if cidr || bits != max {
        format!("{ip}/{bits}")
    } else {
        ip.to_string()
    })
}

/// Borrow column `idx` of `row` as raw wire bytes (`None` for SQL NULL).
#[inline]
pub(crate) fn raw_column(row: &Row, idx: usize) -> Result<Option<&[u8]>, tokio_postgres::Error> {
    Ok(row.try_get::<_, Option<Raw<'_>>>(idx)?.map(|Raw(b)| b))
}

/// The value an unsupported (non-NULL) column decodes to.
pub(crate) fn unsupported_placeholder(ty: &Type) -> String {
    format!("<unsupported column type: {ty}>")
}

/// Decode column `idx` of `row`.
#[inline]
pub(crate) fn column_to_py(py: Python<'_>, row: &Row, idx: usize, ty: &Type) -> PyResult<PyObject> {
    let raw = raw_column(row, idx).map_err(|e| decode_error(py, e))?;
    match raw {
        None => Ok(py.None()),
        Some(buf) => match decode_cell(ty, buf) {
            Ok(Some(c)) => cell_to_py(py, c),
            Ok(None) => Ok(unsupported_placeholder(ty).into_py(py)),
            Err(e) => Err(decode_error(py, e)),
        },
    }
}

/// Column names of a result as Python strings, built once per result set.
fn column_names(py: Python<'_>, row: &Row) -> Vec<Py<PyString>> {
    row.columns()
        .iter()
        .map(|c| PyString::new(py, c.name()).into())
        .collect()
}

fn row_to_pydict_named(py: Python<'_>, row: &Row, names: &[Py<PyString>]) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for (idx, (col, name)) in row.columns().iter().zip(names).enumerate() {
        let value = column_to_py(py, row, idx, col.type_())?;
        dict.set_item(name.as_ref(py), value)?;
    }
    Ok(dict.into_py(py))
}

/// Convert a full Postgres [`Row`] into a Python `dict` keyed by column name.
pub fn row_to_pydict(py: Python<'_>, row: &Row) -> PyResult<PyObject> {
    let names = column_names(py, row);
    row_to_pydict_named(py, row, &names)
}

/// Convert a result set into a list of dicts (column-name strings shared).
pub fn rows_to_pylist(py: Python<'_>, rows: &[Row]) -> PyResult<PyObject> {
    let Some(first) = rows.first() else {
        return Ok(PyList::empty(py).into_py(py));
    };
    let names = column_names(py, first);
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_pydict_named(py, row, &names)?);
    }
    Ok(PyList::new(py, out).into_py(py))
}

/// Decode the value `fetchval` returns without building the whole row dict.
///
/// Matches `row_to_pydict(row)[first column's name]`: when a later column has
/// the same name as the first, the dict keeps the later value, so that one is
/// decoded (pinned by `test_raw_duplicate_column_names_collapse`).
pub fn first_column_to_py(py: Python<'_>, row: &Row) -> PyResult<PyObject> {
    let cols = row.columns();
    let name = cols[0].name();
    let idx = cols.iter().rposition(|c| c.name() == name).unwrap_or(0);
    column_to_py(py, row, idx, cols[idx].type_())
}

/// Recursively convert a `serde_json::Value` into a native Python object.
pub fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    use pyo3::ffi;
    use serde_json::Value;
    let obj = match value {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py(py)
            } else if let Some(u) = n.as_u64() {
                u.into_py(py)
            } else {
                n.as_f64().unwrap_or(0.0).into_py(py)
            }
        }
        Value::String(s) => py_str(py, s)?,
        // PERF: built through the C API with owned references. A jsonb column
        // is the most expensive value to decode; going through the GIL pool
        // for every element cost about a third of it.
        // SAFETY: `PyList_New(n)` / `PyDict_New` return a new reference or
        // NULL with an exception set; `PyList_SetItem` steals the item (a
        // list dropped before every slot is filled holds NULLs, which its
        // deallocator skips); `PyDict_SetItem` does not steal.
        Value::Array(arr) => unsafe {
            let list = ffi::PyList_New(arr.len() as ffi::Py_ssize_t);
            if list.is_null() {
                return Err(PyErr::fetch(py));
            }
            let list = PyObject::from_owned_ptr(py, list);
            for (i, item) in arr.iter().enumerate() {
                let v = json_to_py(py, item)?;
                if ffi::PyList_SetItem(list.as_ptr(), i as ffi::Py_ssize_t, v.into_ptr()) != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
            list
        },
        Value::Object(map) => unsafe {
            let dict = ffi::PyDict_New();
            if dict.is_null() {
                return Err(PyErr::fetch(py));
            }
            let dict = PyObject::from_owned_ptr(py, dict);
            for (k, v) in map {
                let key = py_str(py, k)?;
                let v = json_to_py(py, v)?;
                if ffi::PyDict_SetItem(dict.as_ptr(), key.as_ptr(), v.as_ptr()) != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
            dict
        },
    };
    Ok(obj)
}

/// A Python `str` as an owned reference (no GIL-pool entry).
#[inline]
pub(crate) fn py_str(py: Python<'_>, s: &str) -> PyResult<PyObject> {
    // SAFETY: `s` is valid UTF-8 of the given length; the call returns a new
    // reference or NULL with an exception set.
    unsafe {
        let p = pyo3::ffi::PyUnicode_FromStringAndSize(
            s.as_ptr().cast(),
            s.len() as pyo3::ffi::Py_ssize_t,
        );
        if p.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(PyObject::from_owned_ptr(py, p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(s: &str) -> Vec<u8> {
        let mut b = BytesMut::new();
        numeric_to_sql(s, &mut b).unwrap();
        b.to_vec()
    }

    #[test]
    fn numeric_round_trip() {
        for (input, out) in [
            ("0", "0"),
            ("-0", "0"),
            ("0.00", "0.00"),
            ("12.3400", "12.3400"),
            ("-0.5", "-0.5"),
            ("1E+3", "1000"),
            ("1.5e-07", "0.00000015"),
            ("10000", "10000"),
            ("99999999.0001", "99999999.0001"),
            (
                "123456789012345678901234567890",
                "123456789012345678901234567890",
            ),
            ("0.000000000000000000001", "0.000000000000000000001"),
            ("NaN", "NaN"),
            ("Infinity", "Infinity"),
            ("-inf", "-Infinity"),
        ] {
            assert_eq!(numeric_from_sql(&enc(input)).unwrap(), out, "{input}");
        }
    }

    #[test]
    fn iso_parsing() {
        assert_eq!(
            parse_iso("2000-01-02T00:00:01.5+01:00"),
            Some(Iso::Finite {
                date: Some(1),
                time: Some(1_500_000),
                offset: Some(3600)
            })
        );
        assert!(parse_iso("2024-02-30").is_none());
        assert!(parse_iso("nonsense").is_none());
        assert_eq!(parse_iso("-infinity"), Some(Iso::NegInf));
    }
}
