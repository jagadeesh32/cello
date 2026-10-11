//! Python-facing consumers of the registry added in v1.4.0 Phase 03:
//!
//! * [`PyBodyValidator`] — `body=Model` validation off the raw request bytes
//!   (rules in [`super::validate`]).
//!
//! Serialization with visibility applied (`write_only` fields omitted) is
//! Phase 02's `ModelSpec::output_plan`: `write_only=True` clears
//! `FieldSpec.readable`, which every output path honours.
//! * `schema_openapi()` — the OpenAPI schema of a model (`src/openapi.rs`).

use std::sync::Arc;

use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyType};
use serde_json::Value;

use super::python::PyModelSpec;
use super::spec::ModelSpec;
use super::validate::{self, InputPlan, Outcome, Val};
use crate::request::Request;
use crate::response::Response;

/// Bodies larger than this are parsed with the GIL released.
const RELEASE_GIL_ABOVE: usize = 32 * 1024;

/// Validates a request body against one model and builds the instance.
///
/// Created once per route (at decoration time); calling it per request returns
/// either the unsaved model instance or a `Response` (400 `{"detail": [...]}`).
#[pyclass(name = "BodyValidator", module = "cello._cello", frozen)]
pub struct PyBodyValidator {
    spec: Arc<ModelSpec>,
    plan: InputPlan,
    cls: Py<PyType>,
    /// `Some` when the class uses `Model.__init__`: the instance is filled
    /// directly (`cls.__new__` + attribute sets), with each absent field set the
    /// way `Model.__init__` would (one entry per spec field). `None` calls
    /// `cls(**validated)` so a custom `__init__` still runs, as in 1.3.
    missing: Option<Vec<Missing>>,
    /// Interned attribute names, per spec field.
    names: Vec<Py<PyString>>,
    /// Interned column names for ForeignKey fields (`author_id`), per spec field.
    fk_columns: Vec<Option<Py<PyString>>>,
    /// Spec field index -> input slot (`None`: ignored on input).
    slot_of: Vec<Option<usize>>,
}

#[pymethods]
impl PyBodyValidator {
    #[new]
    /// `defaults` maps every field name to `(mode, value)` as `Model.__init__`
    /// applies it to an absent field: `"none"`, `"value"` (shared), `"call"`
    /// (`value()`) or `"copy"` (`copy.deepcopy(value)`). Pass `None` for a
    /// class with a custom `__init__`.
    #[pyo3(signature = (schema, cls, defaults=None))]
    fn new(
        py: Python<'_>,
        schema: &PyModelSpec,
        cls: &PyType,
        defaults: Option<&PyDict>,
    ) -> PyResult<Self> {
        let spec = Arc::clone(&schema.inner);
        let missing = match defaults {
            Some(d) => compile_missing(py, &spec, d)?,
            None => None,
        };
        let plan = InputPlan::compile(&spec);
        let mut slot_of = vec![None; spec.fields.len()];
        for (slot, input) in plan.inputs.iter().enumerate() {
            slot_of[input.field] = Some(slot);
        }
        let names = spec
            .fields
            .iter()
            .map(|f| PyString::intern(py, &f.name).into())
            .collect();
        let fk_columns = spec
            .fields
            .iter()
            .map(|f| {
                f.relation
                    .as_ref()
                    .map(|_| PyString::intern(py, &f.column).into())
            })
            .collect();
        Ok(Self {
            spec,
            plan,
            cls: cls.into(),
            missing,
            names,
            fk_columns,
            slot_of,
        })
    }

    /// Validate `request`'s body. Returns the model instance, or a `Response`.
    fn __call__(&self, py: Python<'_>, request: &PyAny) -> PyResult<PyObject> {
        let outcome = if let Ok(cell) = request.downcast::<PyCell<Request>>() {
            let req = cell.try_borrow()?;
            self.run(py, req.body_bytes())
        } else {
            // A stand-in request object (tests): use its `body()`.
            let body: Vec<u8> = request.call_method0(intern!(py, "body"))?.extract()?;
            self.run(py, &body)
        };
        match outcome {
            Outcome::Valid(values) => self.build(py, values),
            other => error_response(py, &self.plan, other),
        }
    }

    /// Validate raw bytes (benchmarks and tests). Same return contract.
    fn validate_bytes(&self, py: Python<'_>, body: &[u8]) -> PyResult<PyObject> {
        match self.run(py, body) {
            Outcome::Valid(values) => self.build(py, values),
            other => error_response(py, &self.plan, other),
        }
    }

    #[getter]
    fn model(&self, py: Python<'_>) -> Py<PyType> {
        self.cls.clone_ref(py)
    }
}

impl PyBodyValidator {
    fn run(&self, py: Python<'_>, body: &[u8]) -> Outcome {
        if body.len() > RELEASE_GIL_ABOVE {
            py.allow_threads(|| validate::validate(&self.plan, &self.spec, body))
        } else {
            validate::validate(&self.plan, &self.spec, body)
        }
    }

    fn build(&self, py: Python<'_>, mut values: Vec<Option<Val>>) -> PyResult<PyObject> {
        let cls = self.cls.as_ref(py);
        if let Some(missing) = &self.missing {
            // The attributes `Model.__init__` sets: every field, and a
            // ForeignKey's `<name>_id` column alongside its name.
            let obj = cls.call_method1(intern!(py, "__new__"), (cls,))?;
            let set = |name: &Py<PyString>, v: &PyObject| -> PyResult<()> {
                // SAFETY: GIL held; all three pointers are live owned references.
                let rc = unsafe { ffi::PyObject_SetAttr(obj.as_ptr(), name.as_ptr(), v.as_ptr()) };
                if rc == -1 {
                    Err(PyErr::fetch(py))
                } else {
                    Ok(())
                }
            };
            for (i, name) in self.names.iter().enumerate() {
                let v = match self.slot_of[i].and_then(|s| values[s].take()) {
                    Some(v) => to_py(py, v),
                    None => missing[i].value(py)?,
                };
                if let Some(col) = &self.fk_columns[i] {
                    set(col, &v)?;
                }
                set(name, &v)?;
            }
            return Ok(obj.into());
        }
        let kwargs = PyDict::new(py);
        for (i, name) in self.names.iter().enumerate() {
            if let Some(v) = self.slot_of[i].and_then(|s| values[s].take()) {
                kwargs.set_item(name.as_ref(py), to_py(py, v))?;
            }
        }
        Ok(cls.call((), Some(kwargs))?.into())
    }
}

/// `400 {"detail": [{"loc": [...], "msg": ..., "type": ...}, ...]}`, written
/// straight to bytes.
/// How an absent field is filled (mirrors `Model.__init__`).
enum Missing {
    None,
    Value(PyObject),
    Call(PyObject),
    Copy(PyObject, PyObject),
}

impl Missing {
    fn value(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(match self {
            Missing::None => py.None(),
            Missing::Value(v) => v.clone_ref(py),
            Missing::Call(f) => f.call0(py)?,
            Missing::Copy(deepcopy, v) => deepcopy.call1(py, (v.clone_ref(py),))?,
        })
    }
}

/// One [`Missing`] per spec field, or `None` if `defaults` does not cover them
/// all (the validator then calls the class instead).
fn compile_missing(
    py: Python<'_>,
    spec: &ModelSpec,
    defaults: &PyDict,
) -> PyResult<Option<Vec<Missing>>> {
    let deepcopy: PyObject = py.import("copy")?.getattr("deepcopy")?.into();
    let mut out = Vec::with_capacity(spec.fields.len());
    for f in &spec.fields {
        let Some(entry) = defaults.get_item(f.name.as_str())? else {
            return Ok(None);
        };
        let (mode, value): (String, PyObject) = entry.extract()?;
        out.push(match mode.as_str() {
            "none" => Missing::None,
            "value" => Missing::Value(value),
            "call" => Missing::Call(value),
            "copy" => Missing::Copy(deepcopy.clone_ref(py), value),
            _ => return Ok(None),
        });
    }
    Ok(Some(out))
}

fn error_response(py: Python<'_>, plan: &InputPlan, outcome: Outcome) -> PyResult<PyObject> {
    fn item(buf: &mut Vec<u8>, loc: &str, msg: &str, ty: &str) {
        if buf.len() > 11 {
            buf.push(b',');
        }
        buf.extend_from_slice(b"{\"loc\":[");
        // Strings always serialize.
        let _ = serde_json::to_writer(&mut *buf, loc);
        buf.extend_from_slice(b"],\"msg\":");
        let _ = serde_json::to_writer(&mut *buf, msg);
        buf.extend_from_slice(b",\"type\":");
        let _ = serde_json::to_writer(&mut *buf, ty);
        buf.push(b'}');
    }
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(b"{\"detail\":[");
    match outcome {
        Outcome::BadJson => item(&mut buf, "body", "Invalid JSON body", "value_error.json"),
        Outcome::NotObject => item(
            &mut buf,
            "body",
            &format!(
                "Input should be a valid dictionary or instance of {}",
                plan.model_name
            ),
            "model_type",
        ),
        Outcome::Invalid(errors) => {
            for e in errors {
                item(&mut buf, &e.loc, &e.kind.msg(), e.kind.type_str());
            }
        }
        Outcome::Valid(_) => unreachable!("valid bodies build an instance"),
    }
    buf.extend_from_slice(b"]}");
    let resp = Response::from_json_bytes(buf, 400);
    Ok(Py::new(py, resp)?.into_py(py))
}

fn to_py(py: Python<'_>, v: Val) -> PyObject {
    match v {
        Val::Null => py.None(),
        Val::Int(i) => i.into_py(py),
        Val::Float(f) => f.into_py(py),
        Val::Bool(b) => b.into_py(py),
        Val::Str(s) => s.into_py(py),
        Val::Json(j) => json_to_py(py, &j),
    }
}

/// JSON → Python. Depth is bounded by the parser's recursion limit.
fn json_to_py(py: Python<'_>, v: &Value) -> PyObject {
    match v {
        Value::Null => py.None(),
        Value::Bool(b) => b.into_py(py),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py(py)
            } else if let Some(u) = n.as_u64() {
                u.into_py(py)
            } else {
                n.as_f64().unwrap_or(f64::NAN).into_py(py)
            }
        }
        Value::String(s) => s.into_py(py),
        Value::Array(a) => PyList::new(py, a.iter().map(|x| json_to_py(py, x))).into_py(py),
        Value::Object(m) => {
            let d = PyDict::new(py);
            for (k, x) in m {
                // Setting a str key on a fresh dict cannot fail.
                let _ = d.set_item(k, json_to_py(py, x));
            }
            d.into_py(py)
        }
    }
}

// ── OpenAPI ──────────────────────────────────────────────────────────────────

/// The OpenAPI 3.0 schema object for a model, as a Python dict.
#[pyfunction]
pub fn schema_openapi(py: Python<'_>, schema: &PyModelSpec) -> PyResult<PyObject> {
    let doc = crate::openapi::model_schema(&schema.inner);
    let out = json_to_py(py, &doc);
    // serde_json maps are sorted; list properties in declaration order instead.
    if let (Some(props), Ok(d)) = (doc["properties"].as_object(), out.downcast::<PyDict>(py)) {
        let ordered = PyDict::new(py);
        for f in &schema.inner.fields {
            let key = f.output_key();
            if let Some(p) = props.get(key) {
                ordered.set_item(key, json_to_py(py, p))?;
            }
        }
        d.set_item("properties", ordered)?;
    }
    Ok(out)
}

/// Register this module's classes and functions.
pub fn register(m: &PyModule) -> PyResult<()> {
    m.add_class::<PyBodyValidator>()?;
    m.add_function(wrap_pyfunction!(schema_openapi, m)?)?;
    Ok(())
}
