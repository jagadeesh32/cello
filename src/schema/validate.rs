//! Native request-body validation against a compiled [`ModelSpec`] (v1.4.0 Phase 03).
//!
//! `@app.post(..., body=SomeModel)` with a cello ORM model validates here,
//! straight off the request's raw body bytes, in **one pass**: the JSON is
//! streamed through a `serde` [`DeserializeSeed`] built from the spec, so each
//! value is type-checked, coerced and constraint-checked as it is parsed. There
//! is no intermediate JSON tree and no Python dict. Only after the whole body is
//! accepted is the model instance built (one GIL-held step).
//!
//! # Rules (frozen by `tests/test_native_validation.py`)
//!
//! * **Errors** are collected for every field, never just the first. Each item
//!   is exactly `{"loc": [key], "msg": ..., "type": ...}` using the same
//!   `type`/`msg` vocabulary as the Pydantic `body=` path. Input values are never
//!   echoed back (a rejected password must not be reflected in a 400). The
//!   response is `400 {"detail": [...]}`, the frozen `body=` status code.
//! * **Malformed JSON** (including invalid UTF-8, trailing data, or nesting
//!   deeper than serde_json's 128-level recursion limit) produces the single
//!   item `{"loc": ["body"], "msg": "Invalid JSON body", "type": "value_error.json"}`
//!   that `body=` has always returned.
//! * **Unknown keys are ignored** (the existing `body=` default for both Pydantic
//!   and plain classes). Their values are skipped without being materialised.
//! * **Read-only fields** (`AutoField`, any `read_only=True` field) are ignored
//!   on input exactly like unknown keys. This closes mass assignment: a client
//!   cannot set `id` and turn `save()` into an `UPDATE` of someone else's row.
//! * **Required** = accepts input, `null=False`, and no default (literal,
//!   callable or `auto_now_add`). Missing optional fields stay `None` on the
//!   instance, exactly what `Model(**data)` did, so `save()` applies defaults as
//!   before.
//! * **Duplicate keys**: the last occurrence wins (JSON object semantics as
//!   Python's `json` module applies them).
//! * **Coercion** mirrors Pydantic's lax mode for JSON input — see [`Coerce`].
//! * **U+0000** in a string field or anywhere in a `JSONField` value is
//!   rejected (`string_null_byte`): Postgres refuses it in text and jsonb.

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

use super::spec::{DefaultSpec, FieldSpec, ModelSpec, PgType};

/// How one field's JSON input is checked and converted.
///
/// | kind | accepts | notes |
/// |---|---|---|
/// | `Int32` / `Int64` | integers; floats with no fractional part; `true`/`false`; numeric strings (whitespace trimmed, `_` digit separators, `"4.0"`) | range-checked against the column (`INTEGER`/`SERIAL`/FK = 32-bit, `BIGINT` = 64-bit) |
/// | `Float` | numbers, booleans, numeric strings (`"1.5"`, `"inf"`, `"nan"`) | |
/// | `Bool` | booleans; `0`/`1`; strings `true/false/yes/no/on/off/t/f/y/n/1/0` (any case) | |
/// | `Str` | strings only (numbers are **not** stringified) | `max_length` counts characters, as Postgres `VARCHAR(n)` does |
/// | `DateTime` | ISO-8601 / RFC 3339 (`T` or space, optional fraction, `Z`/offset or naive = UTC), a bare date (midnight UTC), or a Unix timestamp (number or numeric string; > 2e10 is milliseconds) | normalised to the RFC 3339 string a `TIMESTAMPTZ` row decodes to |
/// | `Json` | any JSON value | `JSONField`; nested objects become Python dicts |
/// | `Any` | any JSON value, unchecked | custom `sql_type` columns |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coerce {
    Int32,
    Int64,
    Float,
    Bool,
    Str { max_length: Option<u32> },
    DateTime,
    Json,
    Any,
}

impl Coerce {
    pub fn for_field(f: &FieldSpec) -> Self {
        match &f.pg_type {
            Some(PgType::Serial | PgType::Integer) => Coerce::Int32,
            Some(PgType::BigInt) => Coerce::Int64,
            Some(PgType::DoublePrecision) => Coerce::Float,
            Some(PgType::Boolean) => Coerce::Bool,
            Some(PgType::Varchar(_) | PgType::Text) => Coerce::Str {
                max_length: f.max_length,
            },
            Some(PgType::Jsonb) => Coerce::Json,
            Some(PgType::Timestamptz) => Coerce::DateTime,
            Some(PgType::Custom(_)) | None => Coerce::Any,
        }
    }
}

/// `true` when a body may set this field (`AutoField` and `read_only=True`
/// fields are ignored on input).
pub fn accepts_input(f: &FieldSpec) -> bool {
    f.writable && !f.auto
}

/// `true` when a body must contain this field.
pub fn is_required(f: &FieldSpec) -> bool {
    accepts_input(f) && !f.nullable && !f.auto_now_add && matches!(f.default, DefaultSpec::None)
}

/// A validated value, ready to become a Python object.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Json(Value),
}

/// Why one field was rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrKind {
    Missing,
    IntType,
    IntParsing,
    IntFromFloat,
    LessThanEqual(i64),
    GreaterThanEqual(i64),
    FloatType,
    FloatParsing,
    BoolType,
    BoolParsing,
    StringType,
    StringTooLong(u32),
    StringNullByte,
    DatetimeType,
    DatetimeParsing,
    NotNull,
}

impl ErrKind {
    pub fn type_str(self) -> &'static str {
        match self {
            ErrKind::Missing => "missing",
            ErrKind::IntType => "int_type",
            ErrKind::IntParsing => "int_parsing",
            ErrKind::IntFromFloat => "int_from_float",
            ErrKind::LessThanEqual(_) => "less_than_equal",
            ErrKind::GreaterThanEqual(_) => "greater_than_equal",
            ErrKind::FloatType => "float_type",
            ErrKind::FloatParsing => "float_parsing",
            ErrKind::BoolType => "bool_type",
            ErrKind::BoolParsing => "bool_parsing",
            ErrKind::StringType => "string_type",
            ErrKind::StringTooLong(_) => "string_too_long",
            ErrKind::StringNullByte => "string_null_byte",
            ErrKind::DatetimeType => "datetime_type",
            ErrKind::DatetimeParsing => "datetime_parsing",
            ErrKind::NotNull => "value_error",
        }
    }

    pub fn msg(self) -> String {
        match self {
            ErrKind::Missing => "Field required".into(),
            ErrKind::IntType => "Input should be a valid integer".into(),
            ErrKind::IntParsing => {
                "Input should be a valid integer, unable to parse string as an integer".into()
            }
            ErrKind::IntFromFloat => {
                "Input should be a valid integer, got a number with a fractional part".into()
            }
            ErrKind::LessThanEqual(n) => format!("Input should be less than or equal to {n}"),
            ErrKind::GreaterThanEqual(n) => {
                format!("Input should be greater than or equal to {n}")
            }
            ErrKind::FloatType => "Input should be a valid number".into(),
            ErrKind::FloatParsing => {
                "Input should be a valid number, unable to parse string as a number".into()
            }
            ErrKind::BoolType => "Input should be a valid boolean".into(),
            ErrKind::BoolParsing => {
                "Input should be a valid boolean, unable to interpret input".into()
            }
            ErrKind::StringType => "Input should be a valid string".into(),
            ErrKind::StringTooLong(n) => format!(
                "String should have at most {n} character{}",
                if n == 1 { "" } else { "s" }
            ),
            ErrKind::StringNullByte => "String should not contain null characters (U+0000)".into(),
            ErrKind::DatetimeType => "Input should be a valid datetime".into(),
            ErrKind::DatetimeParsing => {
                "Input should be a valid datetime, unable to parse input".into()
            }
            ErrKind::NotNull => "Input should not be null".into(),
        }
    }

    fn type_error(c: Coerce) -> Self {
        match c {
            Coerce::Int32 | Coerce::Int64 => ErrKind::IntType,
            Coerce::Float => ErrKind::FloatType,
            Coerce::Bool => ErrKind::BoolType,
            Coerce::Str { .. } => ErrKind::StringType,
            Coerce::DateTime => ErrKind::DatetimeType,
            Coerce::Json | Coerce::Any => ErrKind::NotNull,
        }
    }
}

/// Models with more JSON keys than this use a hash map for key lookup.
const LINEAR_KEYS: usize = 32;

/// One input slot (a field that accepts input).
#[derive(Debug, Clone)]
pub struct InputField {
    /// Index into `ModelSpec.fields`.
    pub field: usize,
    pub coerce: Coerce,
    pub nullable: bool,
    pub required: bool,
}

/// The per-model decision table, compiled once from the spec.
#[derive(Debug, Clone)]
pub struct InputPlan {
    pub inputs: Vec<InputField>,
    /// JSON key -> (index into `inputs`, key is a ForeignKey column). A field's
    /// name, plus its column for a ForeignKey (`author` and `author_id` both set
    /// the same slot). Scanned linearly (length first) for typical models;
    /// `by_key` takes over for wide ones.
    keys: Vec<(Box<str>, usize, bool)>,
    by_key: Option<HashMap<Box<str>, (usize, bool)>>,
    /// Model class name, for the `model_type` message.
    pub model_name: String,
}

impl InputPlan {
    pub fn compile(spec: &ModelSpec) -> Self {
        let mut inputs = Vec::new();
        let mut keys: Vec<(Box<str>, usize, bool)> = Vec::new();
        for (i, f) in spec.fields.iter().enumerate() {
            if !accepts_input(f) {
                continue;
            }
            let slot = inputs.len();
            inputs.push(InputField {
                field: i,
                coerce: Coerce::for_field(f),
                nullable: f.nullable,
                required: is_required(f),
            });
            keys.retain(|(k, _, _)| **k != *f.name);
            keys.push((f.name.clone().into_boxed_str(), slot, false));
            if f.relation.is_some() && !keys.iter().any(|(k, _, _)| **k == *f.column) {
                keys.push((f.column.clone().into_boxed_str(), slot, true));
            }
        }
        let by_key = (keys.len() > LINEAR_KEYS)
            .then(|| keys.iter().map(|(k, s, c)| (k.clone(), (*s, *c))).collect());
        Self {
            inputs,
            keys,
            by_key,
            model_name: spec.name.clone(),
        }
    }

    /// The input slot a JSON key sets, and whether the key was a FK column.
    pub fn slot(&self, key: &str) -> Option<(usize, bool)> {
        if let Some(map) = &self.by_key {
            return map.get(key).copied();
        }
        self.keys
            .iter()
            .find(|(k, _, _)| k.len() == key.len() && **k == *key)
            .map(|&(_, s, c)| (s, c))
    }
}

/// One field-level validation error.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldError {
    pub loc: String,
    pub kind: ErrKind,
}

/// Outcome of validating a body.
#[derive(Debug)]
pub enum Outcome {
    /// One entry per `InputPlan.inputs`: `None` when the key was absent.
    Valid(Vec<Option<Val>>),
    Invalid(Vec<FieldError>),
    /// Not parseable as JSON.
    BadJson,
    /// Valid JSON, but not an object.
    NotObject,
}

/// Parse + validate `body` against `plan` in one pass. Never panics on any
/// input; pure Rust, so callers may run it without the GIL.
pub fn validate(plan: &InputPlan, spec: &ModelSpec, body: &[u8]) -> Outcome {
    let mut de = serde_json::Deserializer::from_slice(body);
    let parsed = ModelSeed { plan }.deserialize(&mut de);
    let slots = match parsed.and_then(|r| de.end().map(|_| r)) {
        Ok(Some(slots)) => slots,
        Ok(None) => return Outcome::NotObject,
        Err(_) => return Outcome::BadJson,
    };
    let mut errors = Vec::new();
    let mut values = Vec::with_capacity(slots.len());
    for (input, slot) in plan.inputs.iter().zip(slots) {
        match slot {
            Some((Ok(v), _)) => values.push(Some(v)),
            Some((Err(kind), via_column)) => {
                let f = &spec.fields[input.field];
                let loc = if via_column { &f.column } else { &f.name };
                errors.push(FieldError {
                    loc: loc.clone(),
                    kind,
                });
                values.push(None);
            }
            None => {
                if input.required {
                    errors.push(FieldError {
                        loc: spec.fields[input.field].output_key().to_string(),
                        kind: ErrKind::Missing,
                    });
                }
                values.push(None);
            }
        }
    }
    if errors.is_empty() {
        Outcome::Valid(values)
    } else {
        Outcome::Invalid(errors)
    }
}

/// A field's outcome, and whether it was set through its FK column key.
type Slot = Option<(Result<Val, ErrKind>, bool)>;

struct ModelSeed<'a> {
    plan: &'a InputPlan,
}

impl<'de> DeserializeSeed<'de> for ModelSeed<'_> {
    /// `None` when the body is valid JSON but not an object.
    type Value = Option<Vec<Slot>>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ModelSeed<'_> {
    type Value = Option<Vec<Slot>>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut slots: Vec<Slot> = vec![None; self.plan.inputs.len()];
        while let Some(found) = map.next_key_seed(KeySeed { plan: self.plan })? {
            match found {
                Some((idx, via_column)) => {
                    let input = &self.plan.inputs[idx];
                    let v = map.next_value_seed(FieldSeed {
                        coerce: input.coerce,
                        nullable: input.nullable,
                    })?;
                    slots[idx] = Some((v, via_column));
                }
                None => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Some(slots))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(None)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
}

/// Resolves an object key to an input slot without allocating for unknown keys.
struct KeySeed<'a> {
    plan: &'a InputPlan,
}

impl<'de> DeserializeSeed<'de> for KeySeed<'_> {
    type Value = Option<(usize, bool)>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeySeed<'_> {
    type Value = Option<(usize, bool)>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a string key")
    }

    fn visit_str<E: de::Error>(self, key: &str) -> Result<Self::Value, E> {
        Ok(self.plan.slot(key))
    }
}

/// Validates and coerces one field value. Always consumes the whole value, so a
/// rejected field never aborts the parse and later fields are still checked.
struct FieldSeed {
    coerce: Coerce,
    nullable: bool,
}

impl<'de> DeserializeSeed<'de> for FieldSeed {
    type Value = Result<Val, ErrKind>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

const I32_MIN: i64 = i32::MIN as i64;
const I32_MAX: i64 = i32::MAX as i64;

impl FieldSeed {
    fn int(&self, i: i128) -> Result<Val, ErrKind> {
        let (lo, hi) = match self.coerce {
            Coerce::Int32 => (I32_MIN, I32_MAX),
            _ => (i64::MIN, i64::MAX),
        };
        if i > hi as i128 {
            Err(ErrKind::LessThanEqual(hi))
        } else if i < lo as i128 {
            Err(ErrKind::GreaterThanEqual(lo))
        } else {
            Ok(Val::Int(i as i64))
        }
    }

    fn int_from_f64(&self, f: f64) -> Result<Val, ErrKind> {
        if !f.is_finite() {
            return Err(ErrKind::IntParsing);
        }
        if f.fract() != 0.0 {
            return Err(ErrKind::IntFromFloat);
        }
        // Saturating cast; anything this large is out of range anyway.
        self.int(f.clamp(-1e30, 1e30) as i128)
    }

    fn number(&self, n: Number) -> Result<Val, ErrKind> {
        match self.coerce {
            Coerce::Int32 | Coerce::Int64 => match n {
                Number::I(i) => self.int(i as i128),
                Number::U(u) => self.int(u as i128),
                Number::F(f) => self.int_from_f64(f),
            },
            Coerce::Float => Ok(Val::Float(n.as_f64())),
            Coerce::Bool => {
                let x = n.as_f64();
                if x == 0.0 {
                    Ok(Val::Bool(false))
                } else if x == 1.0 {
                    Ok(Val::Bool(true))
                } else {
                    Err(ErrKind::BoolParsing)
                }
            }
            Coerce::Str { .. } => Err(ErrKind::StringType),
            Coerce::DateTime => from_timestamp(n.as_f64()).map(Val::Str),
            Coerce::Json | Coerce::Any => Ok(Val::Json(n.to_value())),
        }
    }
}

impl FieldSeed {
    /// A `JSONField` value; `Any` (custom column types) is passed through.
    fn json(&self, v: Value) -> Result<Val, ErrKind> {
        match self.coerce {
            Coerce::Json => no_nul(v),
            _ => Ok(Val::Json(v)),
        }
    }
}

/// Reject U+0000 anywhere in a JSON value's strings or keys (jsonb refuses it).
fn no_nul(v: Value) -> Result<Val, ErrKind> {
    fn has(v: &Value) -> bool {
        match v {
            Value::String(s) => s.as_bytes().contains(&0),
            Value::Array(a) => a.iter().any(has),
            Value::Object(m) => m.iter().any(|(k, x)| k.as_bytes().contains(&0) || has(x)),
            _ => false,
        }
    }
    if has(&v) {
        Err(ErrKind::StringNullByte)
    } else {
        Ok(Val::Json(v))
    }
}

#[derive(Clone, Copy)]
enum Number {
    I(i64),
    U(u64),
    F(f64),
}

impl Number {
    fn as_f64(self) -> f64 {
        match self {
            Number::I(i) => i as f64,
            Number::U(u) => u as f64,
            Number::F(f) => f,
        }
    }

    fn to_value(self) -> Value {
        match self {
            Number::I(i) => Value::from(i),
            Number::U(u) => Value::from(u),
            Number::F(f) => serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        }
    }
}

impl<'de> Visitor<'de> for FieldSeed {
    type Value = Result<Val, ErrKind>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(if self.nullable {
            Ok(Val::Null)
        } else {
            Err(ErrKind::type_error(self.coerce))
        })
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.visit_unit()
    }

    fn visit_bool<E: de::Error>(self, b: bool) -> Result<Self::Value, E> {
        Ok(match self.coerce {
            Coerce::Int32 | Coerce::Int64 => Ok(Val::Int(b as i64)),
            Coerce::Float => Ok(Val::Float(if b { 1.0 } else { 0.0 })),
            Coerce::Bool => Ok(Val::Bool(b)),
            Coerce::Str { .. } => Err(ErrKind::StringType),
            Coerce::DateTime => Err(ErrKind::DatetimeType),
            Coerce::Json | Coerce::Any => Ok(Val::Json(Value::Bool(b))),
        })
    }

    fn visit_i64<E: de::Error>(self, i: i64) -> Result<Self::Value, E> {
        Ok(self.number(Number::I(i)))
    }

    fn visit_u64<E: de::Error>(self, u: u64) -> Result<Self::Value, E> {
        Ok(self.number(Number::U(u)))
    }

    fn visit_f64<E: de::Error>(self, f: f64) -> Result<Self::Value, E> {
        Ok(self.number(Number::F(f)))
    }

    fn visit_borrowed_str<E: de::Error>(self, s: &'de str) -> Result<Self::Value, E> {
        self.visit_str(s)
    }

    fn visit_string<E: de::Error>(self, s: String) -> Result<Self::Value, E> {
        Ok(match self.coerce {
            Coerce::Str { max_length } => check_len(&s, max_length).map(|_| Val::Str(s)),
            Coerce::Json => no_nul(Value::String(s)),
            Coerce::Any => Ok(Val::Json(Value::String(s))),
            _ => return self.visit_str(&s),
        })
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
        Ok(match self.coerce {
            Coerce::Int32 | Coerce::Int64 => match parse_int_str(s) {
                Some(i) => self.int(i),
                None => Err(ErrKind::IntParsing),
            },
            Coerce::Float => s
                .trim()
                .parse::<f64>()
                .map(Val::Float)
                .map_err(|_| ErrKind::FloatParsing),
            Coerce::Bool => parse_bool_str(s).map(Val::Bool).ok_or(ErrKind::BoolParsing),
            Coerce::Str { max_length } => check_len(s, max_length).map(|_| Val::Str(s.to_owned())),
            Coerce::DateTime => parse_datetime(s).map(Val::Str),
            Coerce::Json => no_nul(Value::String(s.to_owned())),
            Coerce::Any => Ok(Val::Json(Value::String(s.to_owned()))),
        })
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        match self.coerce {
            Coerce::Json | Coerce::Any => {
                let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(64));
                while let Some(v) = seq.next_element::<Value>()? {
                    items.push(v);
                }
                Ok(self.json(Value::Array(items)))
            }
            c => {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Err(ErrKind::type_error(c)))
            }
        }
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        match self.coerce {
            Coerce::Json | Coerce::Any => {
                let mut obj = serde_json::Map::new();
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    obj.insert(k, v);
                }
                Ok(self.json(Value::Object(obj)))
            }
            c => {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Err(ErrKind::type_error(c)))
            }
        }
    }
}

/// Postgres rejects U+0000 in `text`/`varchar` and in `jsonb` strings, so it is
/// a 400 here rather than a database error after the handler ran.
fn check_len(s: &str, max_length: Option<u32>) -> Result<(), ErrKind> {
    if s.as_bytes().contains(&0) {
        return Err(ErrKind::StringNullByte);
    }
    match max_length {
        // Byte length bounds character count, so most strings skip the count.
        Some(max) if s.len() > max as usize && s.chars().count() > max as usize => {
            Err(ErrKind::StringTooLong(max))
        }
        _ => Ok(()),
    }
}

/// Integer from a string: surrounding whitespace, a sign, `_` between digits
/// and a zero fractional part (`"4.0"`) are accepted; exponents are not.
fn parse_int_str(s: &str) -> Option<i128> {
    let t = s.trim();
    let (neg, body) = match t.as_bytes().first()? {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let (int_part, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    if let Some(f) = frac {
        if !f.bytes().all(|b| b == b'0') {
            return None;
        }
    }
    let bytes = int_part.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() || !bytes[bytes.len() - 1].is_ascii_digit() {
        return None;
    }
    let mut v: i128 = 0;
    let mut prev_us = false;
    for &b in bytes {
        match b {
            b'0'..=b'9' => {
                v = v.checked_mul(10)?.checked_add((b - b'0') as i128)?;
                // Anything beyond i64 is out of range for every integer column.
                if v > u64::MAX as i128 {
                    v = u64::MAX as i128 + 1;
                }
                prev_us = false;
            }
            b'_' if !prev_us => prev_us = true,
            _ => return None,
        }
    }
    Some(if neg { -v } else { v })
}

fn parse_bool_str(s: &str) -> Option<bool> {
    if s.len() > 5 {
        return None;
    }
    match s.to_ascii_lowercase().as_str() {
        "1" | "on" | "t" | "true" | "y" | "yes" => Some(true),
        "0" | "off" | "f" | "false" | "n" | "no" => Some(false),
        _ => None,
    }
}

/// Unix timestamp → RFC 3339 (seconds; values above 2e10 are milliseconds).
fn from_timestamp(ts: f64) -> Result<String, ErrKind> {
    if !ts.is_finite() {
        return Err(ErrKind::DatetimeParsing);
    }
    let secs = if ts.abs() > 2e10 { ts / 1000.0 } else { ts };
    if secs.abs() > 1e13 {
        return Err(ErrKind::DatetimeParsing);
    }
    let whole = secs.floor();
    let nanos = ((secs - whole) * 1e9).round().min(999_999_999.0) as u32;
    DateTime::<Utc>::from_timestamp(whole as i64, nanos)
        .map(|d| d.to_rfc3339())
        .ok_or(ErrKind::DatetimeParsing)
}

/// ISO-8601 / RFC 3339 datetime, bare date, or numeric timestamp string, as
/// the RFC 3339 text a `TIMESTAMPTZ` column decodes to. Naive values are UTC.
pub fn parse_datetime(s: &str) -> Result<String, ErrKind> {
    let t = s.trim();
    if t.len() > 64 {
        return Err(ErrKind::DatetimeParsing);
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(t) {
        return Ok(d.with_timezone(&Utc).to_rfc3339());
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%.f%:z",
        "%Y-%m-%dT%H:%M:%S%.f%z",
        "%Y-%m-%d %H:%M:%S%.f%z",
        "%Y-%m-%dT%H:%M%:z",
        "%Y-%m-%d %H:%M%:z",
    ] {
        if let Ok(d) = DateTime::parse_from_str(t, fmt) {
            return Ok(d.with_timezone(&Utc).to_rfc3339());
        }
    }
    let naive = t.strip_suffix(['Z', 'z']).unwrap_or(t);
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(naive, fmt) {
            return Ok(d.and_utc().to_rfc3339());
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        if let Some(dt) = d.and_hms_opt(0, 0, 0) {
            return Ok(dt.and_utc().to_rfc3339());
        }
    }
    if !t.is_empty()
        && t.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+'))
    {
        if let Ok(f) = t.parse::<f64>() {
            return from_timestamp(f);
        }
    }
    Err(ErrKind::DatetimeParsing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ident::quote_ident;
    use crate::schema::spec::{Literal, OnDelete, RelationSpec};

    fn field(name: &str, ty: &str) -> FieldSpec {
        FieldSpec {
            name: name.into(),
            column: name.into(),
            quoted_column: quote_ident(name),
            pg_type: Some(PgType::parse(ty)),
            primary_key: false,
            nullable: false,
            unique: false,
            max_length: None,
            default: DefaultSpec::None,
            auto: false,
            auto_now_add: false,
            readable: true,
            writable: true,
            relation: None,
        }
    }

    fn spec() -> ModelSpec {
        let mut id = field("id", "SERIAL");
        id.auto = true;
        id.primary_key = true;
        let mut name = field("name", "VARCHAR(3)");
        name.max_length = Some(3);
        let mut active = field("active", "BOOLEAN");
        active.default = DefaultSpec::Literal(Literal::Bool(true));
        let mut note = field("note", "TEXT");
        note.nullable = true;
        let mut author = field("author", "INTEGER");
        author.column = "author_id".into();
        author.relation = Some(RelationSpec {
            target: None,
            on_delete: OnDelete::Cascade,
        });
        let fields = vec![
            id,
            name,
            field("n", "INTEGER"),
            field("big", "BIGINT"),
            field("x", "DOUBLE PRECISION"),
            active,
            note,
            field("at", "TIMESTAMPTZ"),
            field("data", "JSONB"),
            author,
        ];
        ModelSpec::new(1, "M".into(), "m".into(), fields)
    }

    fn run(body: &str) -> Outcome {
        let s = spec();
        validate(&InputPlan::compile(&s), &s, body.as_bytes())
    }

    fn errors(body: &str) -> Vec<(String, &'static str)> {
        match run(body) {
            Outcome::Invalid(e) => e.into_iter().map(|e| (e.loc, e.kind.type_str())).collect(),
            other => panic!("expected errors, got {other:?}"),
        }
    }

    const OK: &str = r#"{"name":"abc","n":"42","big":5,"x":"1.5","at":"2024-01-02","data":{"a":[1]},"author_id":3}"#;

    #[test]
    fn accepts_and_coerces() {
        let Outcome::Valid(v) = run(OK) else { panic!() };
        // inputs: name, n, big, x, active, note, at, data, author (id is read-only)
        assert_eq!(v[0], Some(Val::Str("abc".into())));
        assert_eq!(v[1], Some(Val::Int(42)));
        assert_eq!(v[3], Some(Val::Float(1.5)));
        assert_eq!(v[4], None);
        assert_eq!(v[6], Some(Val::Str("2024-01-02T00:00:00+00:00".into())));
        assert_eq!(v[8], Some(Val::Int(3)));
    }

    #[test]
    fn collects_every_error() {
        let e = errors(
            r#"{"name":"abcd","n":1.5,"x":"no","active":"maybe","at":"x","author":true,"zzz":[[[]]]}"#,
        );
        assert_eq!(
            e,
            vec![
                ("name".into(), "string_too_long"),
                ("n".into(), "int_from_float"),
                ("big".into(), "missing"),
                ("x".into(), "float_parsing"),
                ("active".into(), "bool_parsing"),
                ("at".into(), "datetime_parsing"),
                ("data".into(), "missing"),
            ]
        );
    }

    #[test]
    fn ranges_and_nulls() {
        let e = errors(
            r#"{"name":null,"n":2147483648,"big":-99999999999999999999,"x":1,"at":0,"data":null,"author":1}"#,
        );
        assert_eq!(
            e,
            vec![
                ("name".into(), "string_type"),
                ("n".into(), "less_than_equal"),
                ("big".into(), "greater_than_equal"),
                ("data".into(), "value_error"),
            ]
        );
    }

    #[test]
    fn read_only_and_unknown_ignored() {
        let body = OK.replacen('{', r#"{"id":999,"unknown":{"deep":[1,2]},"#, 1);
        assert!(matches!(run(&body), Outcome::Valid(_)));
    }

    #[test]
    fn malformed() {
        for b in [
            "",
            "{",
            "[1]",
            "nul",
            "{\"name\":\"a\"} x",
            "\"s\"",
            "{\"a\":1,}",
        ] {
            let o = run(b);
            assert!(
                matches!(o, Outcome::BadJson | Outcome::NotObject),
                "{b}: {o:?}"
            );
        }
        assert!(matches!(run("[1,2]"), Outcome::NotObject));
        let deep = format!("{{\"data\":{}{}}}", "[".repeat(5000), "]".repeat(5000));
        assert!(matches!(run(&deep), Outcome::BadJson));
    }

    #[test]
    fn null_bytes_rejected() {
        let e = errors(r#"{"name":"a\u0000","data":{"k":["\u0000"]}}"#);
        assert!(e.contains(&("name".into(), "string_null_byte")));
        assert!(e.contains(&("data".into(), "string_null_byte")));
    }

    #[test]
    fn int_strings() {
        assert_eq!(parse_int_str(" 42 "), Some(42));
        assert_eq!(parse_int_str("+4_2"), Some(42));
        assert_eq!(parse_int_str("4.00"), Some(4));
        assert_eq!(parse_int_str("4.5"), None);
        assert_eq!(parse_int_str("1e3"), None);
        assert_eq!(parse_int_str("4__2"), None);
        assert_eq!(parse_int_str("_4"), None);
        assert_eq!(parse_int_str("-"), None);
        assert!(parse_int_str(&"9".repeat(400)).unwrap() > u64::MAX as i128);
    }

    #[test]
    fn datetimes() {
        let utc = "2024-01-02T03:04:05+00:00";
        for s in [
            "2024-01-02T03:04:05Z",
            "2024-01-02 03:04:05",
            "2024-01-02T03:04:05",
            "2024-01-02T08:34:05+05:30",
            "2024-01-02T08:34:05+0530",
            "1704164645",
        ] {
            assert_eq!(parse_datetime(s).as_deref(), Ok(utc), "{s}");
        }
        assert!(parse_datetime("2024-13-01").is_err());
        assert!(parse_datetime("x").is_err());
        assert_eq!(
            from_timestamp(1704164645000.0).as_deref(),
            Ok(utc),
            "milliseconds"
        );
        assert!(from_timestamp(1e300).is_err());
    }
}
