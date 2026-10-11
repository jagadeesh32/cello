//! The row pipeline (v1.4.0 Phase 02): Postgres rows straight to response
//! JSON, and straight to model instances.
//!
//! # Row → JSON (`rows_to_json`)
//!
//! Runs on the `cello-io` thread, **without the GIL**, right after the rows
//! arrive. Each column's raw wire bytes are decoded by
//! [`decode_cell`](super::value::decode_cell) — the same type dispatch the
//! Python path uses — and written as JSON into one pre-sized buffer. No Python
//! object exists for any row or value. The buffer is handed to Python as a
//! [`RawJson`], which the response serializer splices into the body verbatim.
//!
//! For a model, the object keys and the column behind each key come from the
//! compiled [`ModelSpec`] ([`ModelSpec::output_columns`]): exactly what
//! `Model.to_dict()` returns for an instance freshly loaded from the same row
//! (field names, `<name>_id` for a ForeignKey, `write_only` fields left out).
//! Without a model, the keys are the result's column names with `dict`
//! semantics (a repeated name keeps its first position and its last value),
//! matching `db.fetch()`.
//!
//! # Row → instances (`rows_to_models`)
//!
//! Runs on the loop thread (GIL held). Builds each instance with
//! `cls.__new__(cls)` and fills its `__dict__` from the row, exactly the
//! assignments `Model._from_row` makes, without building the intermediate
//! row dict.

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyString};
use tokio_postgres::{Column, Row};

use super::error::{ErrClass, PgError};
use super::value::{
    column_to_py, decode_cell, raw_column, unsupported_placeholder, write_cell_json, NotJson,
};
use crate::json::write_json_string;
use crate::schema::{ModelSpec, PyModelSpec};

/// Pre-serialized JSON produced natively (rows of a queryset, see
/// `QuerySet.json()`). The response serializer writes it verbatim wherever it
/// appears in a handler's return value.
///
/// It cannot be constructed from arbitrary text in Python, so it always holds
/// JSON that Cello itself encoded.
#[pyclass(name = "RawJSON", module = "cello._cello", frozen)]
pub struct RawJson {
    pub bytes: Vec<u8>,
}

#[pymethods]
impl RawJson {
    /// Serialize `obj` with the response serializer (same output as returning
    /// it from a handler).
    #[staticmethod]
    fn encode(py: Python<'_>, obj: &PyAny) -> PyResult<Self> {
        let bytes =
            crate::json::encode_value(py, obj).map_err(pyo3::exceptions::PyTypeError::new_err)?;
        Ok(Self { bytes })
    }

    fn __bytes__<'p>(&self, py: Python<'p>) -> &'p PyBytes {
        PyBytes::new(py, &self.bytes)
    }

    fn __str__(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    fn __len__(&self) -> usize {
        self.bytes.len()
    }

    fn __eq__(&self, other: &PyAny) -> bool {
        match other.downcast::<PyCell<RawJson>>() {
            Ok(o) => o.get().bytes == self.bytes,
            Err(_) => false,
        }
    }

    fn __repr__(&self) -> String {
        format!("<RawJSON {} bytes>", self.bytes.len())
    }
}

/// Why rows could not be written as JSON.
#[derive(Debug)]
pub enum RowJsonError {
    /// A malformed value from the server.
    Pg(Box<PgError>),
    /// A value with no JSON form (`bytea`): `TypeError`, as the serializer
    /// raises for `bytes`.
    NotJson(String),
}

impl RowJsonError {
    pub fn into_pyerr(self, py: Python<'_>) -> PyErr {
        match self {
            RowJsonError::Pg(e) => (*e).into_pyerr(py),
            RowJsonError::NotJson(m) => pyo3::exceptions::PyTypeError::new_err(m),
        }
    }
}

/// Index of the last column called `name` (a row dict keeps the last value of
/// a repeated column name).
fn last_index(columns: &[Column], name: &str) -> Option<usize> {
    columns.iter().rposition(|c| c.name() == name)
}

/// `"key":` for every output key, with the column each one reads.
struct JsonPlan {
    keys: Vec<Vec<u8>>,
    cols: Vec<Option<usize>>,
}

impl JsonPlan {
    fn new(spec: Option<&ModelSpec>, columns: &[Column]) -> Self {
        let pairs: Vec<(String, Option<usize>)> = match spec {
            Some(spec) => spec
                .output_columns()
                .into_iter()
                .map(|(key, col)| {
                    let idx = last_index(columns, &col);
                    (key, idx)
                })
                .collect(),
            None => {
                let mut out: Vec<(String, Option<usize>)> = Vec::with_capacity(columns.len());
                for c in columns {
                    if !out.iter().any(|(k, _)| k == c.name()) {
                        out.push((c.name().to_string(), last_index(columns, c.name())));
                    }
                }
                out
            }
        };
        let mut keys = Vec::with_capacity(pairs.len());
        let mut cols = Vec::with_capacity(pairs.len());
        for (i, (key, col)) in pairs.into_iter().enumerate() {
            let mut k = Vec::with_capacity(key.len() + 4);
            if i > 0 {
                k.push(b',');
            }
            write_json_string(&key, &mut k);
            k.push(b':');
            keys.push(k);
            cols.push(col);
        }
        Self { keys, cols }
    }
}

fn decode_err(msg: impl std::fmt::Display) -> RowJsonError {
    RowJsonError::Pg(Box::new(PgError::new(
        ErrClass::Data,
        format!("cannot decode column: {msg}"),
    )))
}

fn write_row(row: &Row, plan: &JsonPlan, out: &mut Vec<u8>) -> Result<(), RowJsonError> {
    let columns = row.columns();
    out.push(b'{');
    for (key, col) in plan.keys.iter().zip(&plan.cols) {
        out.extend_from_slice(key);
        let Some(idx) = *col else {
            out.extend_from_slice(b"null");
            continue;
        };
        let ty = columns[idx].type_();
        match raw_column(row, idx).map_err(decode_err)? {
            None => out.extend_from_slice(b"null"),
            Some(buf) => match decode_cell(ty, buf).map_err(decode_err)? {
                Some(cell) => write_cell_json(&cell, out).map_err(|NotJson(what)| {
                    RowJsonError::NotJson(format!(
                        "column {:?} ({ty}) decodes to {what}, which is not JSON serializable",
                        columns[idx].name()
                    ))
                })?,
                None => write_json_string(&unsupported_placeholder(ty), out),
            },
        }
    }
    out.push(b'}');
    Ok(())
}

/// Write `rows` as a JSON array of objects (see the module docs). Pure Rust:
/// safe to run on the I/O thread.
pub fn rows_to_json(rows: &[Row], spec: Option<&ModelSpec>) -> Result<Vec<u8>, RowJsonError> {
    let Some(first) = rows.first() else {
        return Ok(b"[]".to_vec());
    };
    let plan = JsonPlan::new(spec, first.columns());
    let mut out = Vec::with_capacity(256);
    out.push(b'[');
    write_row(first, &plan, &mut out)?;
    // Size the buffer once from the first row instead of growing it.
    out.reserve((out.len() + 8) * (rows.len() - 1) + 1);
    for row in &rows[1..] {
        out.push(b',');
        write_row(row, &plan, &mut out)?;
    }
    out.push(b']');
    Ok(out)
}

/// Column name that separates the models of a joined row (`select_related`):
/// `SELECT "_c0".*, NULL AS "__cello_join__", "_c1".*, ...`.
pub const JOIN_SEPARATOR: &str = "__cello_join__";

/// Index of the last column called `name` within `columns[start..end]`.
fn last_index_in(columns: &[Column], start: usize, end: usize, name: &str) -> Option<usize> {
    columns[start..end]
        .iter()
        .rposition(|c| c.name() == name)
        .map(|i| i + start)
}

/// Builds instances of one model class from (a segment of) a row: the
/// `Model._from_row` assignments, written straight into `__dict__`.
struct Hydrator<'py> {
    new: &'py PyAny,
    args: &'py pyo3::types::PyTuple,
    /// (attribute, column index) per assignment.
    assign: Vec<(&'py PyString, Option<usize>)>,
}

impl<'py> Hydrator<'py> {
    fn new(
        py: Python<'py>,
        cls: &'py PyAny,
        spec: &'py PyModelSpec,
        columns: &[Column],
        start: usize,
        end: usize,
    ) -> PyResult<Self> {
        let assign = spec
            .hydrate_plan(py)
            .iter()
            .map(|(attr, col)| (attr.as_ref(py), last_index_in(columns, start, end, col)))
            .collect();
        Ok(Self {
            new: cls.getattr(pyo3::intern!(py, "__new__"))?,
            args: pyo3::types::PyTuple::new(py, [cls]),
            assign,
        })
    }

    /// One instance and its `__dict__`. `values` caches decoded columns for
    /// this row: a column assigned to two attributes (a ForeignKey's name and
    /// id) is decoded once and shared, as `row.get(col)` would share it.
    fn build(
        &self,
        py: Python<'py>,
        row: &Row,
        columns: &[Column],
        values: &mut [Option<PyObject>],
    ) -> PyResult<(PyObject, PyObject)> {
        use pyo3::ffi;
        let none = py.None();
        // SAFETY: every pointer passed is a live object; each call returns a
        // new reference (owned by a `PyObject`) or NULL with an exception set,
        // and `PyObject_SetItem` does not steal references.
        unsafe {
            let obj =
                ffi::PyObject_Call(self.new.as_ptr(), self.args.as_ptr(), std::ptr::null_mut());
            if obj.is_null() {
                return Err(PyErr::fetch(py));
            }
            let obj = PyObject::from_owned_ptr(py, obj);
            let d = ffi::PyObject_GetAttr(obj.as_ptr(), pyo3::intern!(py, "__dict__").as_ptr());
            if d.is_null() {
                return Err(PyErr::fetch(py));
            }
            let d = PyObject::from_owned_ptr(py, d);
            for (attr, col) in &self.assign {
                let value = match *col {
                    None => none.as_ptr(),
                    Some(idx) => {
                        if values[idx].is_none() {
                            values[idx] = Some(column_to_py(py, row, idx, columns[idx].type_())?);
                        }
                        values[idx].as_ref().map_or(none.as_ptr(), |v| v.as_ptr())
                    }
                };
                if ffi::PyObject_SetItem(d.as_ptr(), attr.as_ptr(), value) != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
            Ok((obj, d))
        }
    }
}

/// Build model instances from `rows` the way `Model._from_row` does:
/// `obj = cls.__new__(cls)`, then `obj.__dict__[attr] = row[column]` for each
/// assignment of the model's hydrate plan (`None` for a missing column).
pub fn rows_to_models(
    py: Python<'_>,
    cls: &PyAny,
    spec: &PyModelSpec,
    rows: &[Row],
) -> PyResult<PyObject> {
    let Some(first) = rows.first() else {
        return Ok(PyList::empty(py).into_py(py));
    };
    let columns = first.columns();
    let hydrator = Hydrator::new(py, cls, spec, columns, 0, columns.len())?;
    let mut values: Vec<Option<PyObject>> = vec![None; columns.len()];
    let out = PyList::empty(py);
    for row in rows {
        values.iter_mut().for_each(|v| *v = None);
        let (obj, _) = hydrator.build(py, row, columns, &mut values)?;
        out.append(obj)?;
    }
    Ok(out.into_py(py))
}

/// One model of a `select_related` join: its class, and where its instance
/// goes (`parent.__dict__[attr]`; the base model has no parent).
pub struct JoinTarget {
    pub cls: PyObject,
    pub spec: Py<PyModelSpec>,
    pub parent: Option<(usize, Py<PyString>)>,
}

/// Build base-model instances from joined rows (`select_related`).
///
/// The row holds one column segment per target, separated by
/// [`JOIN_SEPARATOR`] columns, in target order. Each segment is hydrated with
/// its own model's plan (so equal column names in different tables never mix).
/// A related instance is attached to its parent's `__dict__` only when its
/// primary-key column is non-NULL; otherwise (NULL foreign key, or a key with
/// no matching row) the parent keeps the id it was loaded with. Returns the
/// list of base instances.
pub fn rows_to_model_graph(
    py: Python<'_>,
    targets: &[JoinTarget],
    rows: &[Row],
) -> PyResult<PyObject> {
    let Some(first) = rows.first() else {
        return Ok(PyList::empty(py).into_py(py));
    };
    let columns = first.columns();
    let seps: Vec<usize> = columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.name() == JOIN_SEPARATOR)
        .map(|(i, _)| i)
        .collect();
    if seps.len() + 1 != targets.len() {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "select_related: the joined result has {} {JOIN_SEPARATOR:?} columns, expected {} \
             (does a table declare a column with that name?)",
            seps.len(),
            targets.len().saturating_sub(1)
        )));
    }
    let mut bounds = Vec::with_capacity(targets.len());
    let mut start = 0;
    for &s in &seps {
        bounds.push((start, s));
        start = s + 1;
    }
    bounds.push((start, columns.len()));

    let mut hydrators = Vec::with_capacity(targets.len());
    let mut pks = Vec::with_capacity(targets.len());
    for (t, &(s, e)) in targets.iter().zip(&bounds) {
        let spec = t.spec.as_ref(py).get();
        hydrators.push(Hydrator::new(py, t.cls.as_ref(py), spec, columns, s, e)?);
        let pk = spec
            .inner
            .pk_field()
            .and_then(|f| last_index_in(columns, s, e, &f.column));
        pks.push(pk);
    }

    let mut values: Vec<Option<PyObject>> = vec![None; columns.len()];
    let mut dicts: Vec<Option<PyObject>> = vec![None; targets.len()];
    let out = PyList::empty(py);
    for row in rows {
        values.iter_mut().for_each(|v| *v = None);
        for (k, t) in targets.iter().enumerate() {
            dicts[k] = None;
            let parent = match &t.parent {
                None => None,
                Some((p, attr)) => match &dicts[*p] {
                    // The parent row is absent: so is this one.
                    None => continue,
                    Some(d) => {
                        let present = match pks[k] {
                            Some(idx) => raw_column(row, idx).map_err(|e| {
                                PgError::new(ErrClass::Data, format!("cannot decode column: {e}"))
                                    .into_pyerr(py)
                            })?,
                            None => None,
                        };
                        if present.is_none() {
                            continue;
                        }
                        Some((d.clone_ref(py), attr))
                    }
                },
            };
            let (obj, d) = hydrators[k].build(py, row, columns, &mut values)?;
            match parent {
                None => out.append(obj.as_ref(py))?,
                Some((pd, attr)) => pd.as_ref(py).set_item(attr.as_ref(py), obj.as_ref(py))?,
            }
            dicts[k] = Some(d);
        }
    }
    Ok(out.into_py(py))
}

/// The compiled spec behind a `ModelSpec` handle, or `None`.
pub fn spec_of(spec: Option<&PyAny>) -> PyResult<Option<Arc<ModelSpec>>> {
    match spec {
        None => Ok(None),
        Some(s) if s.is_none() => Ok(None),
        Some(s) => Ok(Some(Arc::clone(
            &s.downcast::<PyCell<PyModelSpec>>()?.get().inner,
        ))),
    }
}
