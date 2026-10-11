//! Python bindings: compile field descriptors from the ORM metaclass into specs
//! and expose read-only handles (`cello._cello.ModelSpec` / `FieldSpec`).
//!
//! Everything in here runs at class-definition time or on a resolution miss;
//! the per-query hit path is the dict returned by `ModelSpec.lookup()`.

use std::sync::Arc;

use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyBool, PyDict, PyFloat, PyList, PyLong, PyString};

use super::ident::quote_ident;
use super::spec::{
    DefaultSpec, FieldSpec, Literal, ModelId, ModelSpec, OnDelete, PgType, RelationSpec,
    RelationTarget, Resolved, SchemaError,
};

impl From<SchemaError> for PyErr {
    fn from(e: SchemaError) -> PyErr {
        match e {
            // 1.3.0 raised AttributeError (`'str' object has no attribute '_meta'`).
            SchemaError::UnresolvedRelation { .. } => PyAttributeError::new_err(e.to_string()),
            _ => PyValueError::new_err(e.to_string()),
        }
    }
}

/// A descriptor value, treating a missing key and `None` alike.
fn item<'p>(d: &'p PyDict, key: &str) -> PyResult<Option<&'p PyAny>> {
    Ok(d.get_item(key)?.filter(|v| !v.is_none()))
}

fn req_str(d: &PyDict, key: &str) -> PyResult<String> {
    match item(d, key)? {
        Some(v) => v.extract(),
        None => Err(PyValueError::new_err(format!(
            "schema field descriptor is missing {key:?}"
        ))),
    }
}

fn flag(d: &PyDict, key: &str) -> PyResult<bool> {
    match item(d, key)? {
        Some(v) => v.is_true(),
        None => Ok(false),
    }
}

/// Classify a Python default exactly as 1.3.0's `_sql_literal` did: bool first,
/// then int/float (Python `str()` text), everything else `str()`-ed and quoted.
fn compile_default(d: &PyDict) -> PyResult<DefaultSpec> {
    if flag(d, "default_now")? {
        return Ok(DefaultSpec::Now);
    }
    let Some(v) = item(d, "default")? else {
        return Ok(DefaultSpec::None);
    };
    if v.is_callable() {
        return Ok(DefaultSpec::Callable);
    }
    let text = || -> PyResult<String> { Ok(v.str()?.to_str()?.to_owned()) };
    let lit = if v.is_instance_of::<PyBool>() {
        Literal::Bool(v.is_true()?)
    } else if v.is_instance_of::<PyLong>() {
        Literal::Int(text()?)
    } else if v.is_instance_of::<PyFloat>() {
        Literal::Float(text()?)
    } else if v.is_instance_of::<PyString>() {
        Literal::Text(text()?)
    } else {
        Literal::Other(text()?)
    };
    Ok(DefaultSpec::Literal(lit))
}

fn compile_relation(d: &PyDict) -> PyResult<Option<RelationSpec>> {
    let Some(fk) = item(d, "fk")? else {
        return Ok(None);
    };
    let fk: &PyDict = fk.downcast()?;
    let on_delete = OnDelete::parse(&req_str(fk, "on_delete")?)?;
    let target = match item(fk, "table")? {
        Some(table) => Some(RelationTarget {
            model: item(fk, "model_id")?.map(|v| v.extract()).transpose()?,
            table: table.extract()?,
            pk_column: item(fk, "pk_column")?.map(|v| v.extract()).transpose()?,
        }),
        None => None,
    };
    Ok(Some(RelationSpec { target, on_delete }))
}

/// Build a [`FieldSpec`] from the dict `orm._field_descriptor()` produces.
fn compile_field(d: &PyDict) -> PyResult<FieldSpec> {
    let name = req_str(d, "name")?;
    let column = req_str(d, "column")?;
    let pg_type = item(d, "sql_type")?
        .map(|v| v.extract::<String>())
        .transpose()?
        .map(|s| PgType::parse(&s));
    let max_length = match item(d, "max_length")? {
        Some(v) => v.extract::<u32>().ok(),
        None => None,
    }
    .or(match pg_type {
        Some(PgType::Varchar(n)) => Some(n),
        _ => None,
    });
    let write_only = flag(d, "write_only")?;
    let read_only = flag(d, "read_only")?;
    if write_only && read_only {
        return Err(PyValueError::new_err(format!(
            "field {name:?} cannot be both write_only and read_only"
        )));
    }
    Ok(FieldSpec {
        quoted_column: quote_ident(&column),
        name,
        column,
        pg_type,
        primary_key: flag(d, "primary_key")?,
        nullable: flag(d, "null")?,
        unique: flag(d, "unique")?,
        max_length,
        default: compile_default(d)?,
        auto: flag(d, "auto")?,
        auto_now_add: flag(d, "auto_now_add")?,
        readable: !write_only,
        writable: !read_only,
        relation: compile_relation(d)?,
    })
}

/// Compile and register a model. Called once per class by `ModelMeta`.
///
/// `fields` is a list of descriptor dicts in declaration order.
#[pyfunction]
pub fn schema_register(name: String, table: String, fields: &PyList) -> PyResult<PyModelSpec> {
    let mut specs = Vec::with_capacity(fields.len());
    for d in fields.iter() {
        specs.push(compile_field(d.downcast()?)?);
    }
    // All Python work is done before the registry is touched.
    let spec = super::register(ModelSpec::new(super::next_id(), name, table, specs));
    Ok(PyModelSpec::new(spec, true))
}

/// Compile a single field without registering anything (`Field.column_ddl()`).
#[pyfunction]
pub fn schema_compile_field(descriptor: &PyDict) -> PyResult<PyFieldSpec> {
    Ok(PyFieldSpec {
        inner: compile_field(descriptor)?,
        model: "<unbound>".to_string(),
    })
}

/// Validate and quote a query-supplied identifier that is not a model field
/// (an aggregate / annotation alias). Returns the quoted identifier, or `None`
/// when `name` is not a string matching `^[A-Za-z_][A-Za-z0-9_]*$`.
#[pyfunction]
pub fn schema_ident(name: &PyAny) -> Option<String> {
    let s = name.downcast::<PyString>().ok()?.to_str().ok()?;
    super::ident::is_identifier(s).then(|| quote_ident(s))
}

/// A non-owning handle to a registered model, or `None`.
#[pyfunction]
pub fn schema_get(id: ModelId) -> Option<PyModelSpec> {
    super::get(id).map(|inner| PyModelSpec::new(inner, false))
}

/// Number of live registered models.
#[pyfunction]
pub fn schema_count() -> usize {
    super::len()
}

/// Read-only handle to a compiled model. The handle created by
/// `schema_register` owns the registry entry and removes it when dropped.
#[pyclass(name = "ModelSpec", module = "cello._cello", frozen)]
pub struct PyModelSpec {
    pub inner: Arc<ModelSpec>,
    owner: bool,
    /// `to_dict()` as Python strings, built on first use (Phase 02).
    out: GILOnceCell<Vec<OutEntry>>,
    /// `_from_row` assignments as `(attribute, column)`, built on first use.
    hydrate: GILOnceCell<Vec<(Py<PyString>, String)>>,
}

/// One `to_dict()` key with what is needed to compute its value from an
/// instance (see [`ModelSpec::output_plan`]).
pub(crate) struct OutEntry {
    pub key: Py<PyString>,
    /// `"key":`, JSON-escaped, for the response serializer.
    pub key_json: Vec<u8>,
    /// Attribute read with `getattr(obj, attr, None)`.
    pub attr: Py<PyString>,
    /// For a ForeignKey: the field name, read when `attr` is `None` (an
    /// assigned, unsaved related instance contributes its primary key).
    pub fk_name: Option<Py<PyString>>,
}

/// `getattr(obj, name, None)` as an owned reference (no GIL-pool entry: this
/// runs once per field per row).
pub(crate) fn attr_or_none(py: Python<'_>, obj: &PyAny, name: &PyString) -> PyResult<PyObject> {
    // SAFETY: both pointers are valid, live objects; `PyObject_GetAttr`
    // returns a new reference or NULL with an exception set.
    unsafe {
        let p = pyo3::ffi::PyObject_GetAttr(obj.as_ptr(), name.as_ptr());
        if p.is_null() {
            let e = PyErr::fetch(py);
            if e.is_instance_of::<PyAttributeError>(py) {
                return Ok(py.None());
            }
            return Err(e);
        }
        Ok(PyObject::from_owned_ptr(py, p))
    }
}

/// `orm._pk_of(obj)`: the instance's primary-key value, or `None`.
fn pk_of(py: Python<'_>, obj: &PyAny) -> PyResult<PyObject> {
    let pk_name = obj
        .getattr(pyo3::intern!(py, "_meta"))?
        .getattr(pyo3::intern!(py, "pk_name"))?;
    if pk_name.is_none() {
        return Ok(py.None());
    }
    attr_or_none(py, obj, pk_name.downcast()?)
}

impl PyModelSpec {
    fn new(inner: Arc<ModelSpec>, owner: bool) -> Self {
        Self {
            inner,
            owner,
            out: GILOnceCell::new(),
            hydrate: GILOnceCell::new(),
        }
    }

    /// The `to_dict()` plan (keys deduplicated with dict semantics,
    /// `write_only` fields left out).
    pub(crate) fn out_plan(&self, py: Python<'_>) -> &[OutEntry] {
        self.out.get_or_init(py, || {
            self.inner
                .output_plan()
                .into_iter()
                .map(|o| {
                    let f = &self.inner.fields[o.field];
                    let mut key_json = Vec::with_capacity(o.key.len() + 3);
                    crate::json::write_json_string(&o.key, &mut key_json);
                    key_json.push(b':');
                    OutEntry {
                        key: PyString::intern(py, &o.key).into(),
                        key_json,
                        attr: PyString::intern(py, f.output_attr()).into(),
                        fk_name: f
                            .relation
                            .as_ref()
                            .map(|_| PyString::intern(py, &f.name).into()),
                    }
                })
                .collect()
        })
    }

    /// `_from_row`'s assignments: `(attribute, column)` in order.
    pub(crate) fn hydrate_plan(&self, py: Python<'_>) -> &[(Py<PyString>, String)] {
        self.hydrate.get_or_init(py, || {
            self.inner
                .hydrate_plan()
                .into_iter()
                .map(|(a, c)| (PyString::intern(py, a).into(), c.to_string()))
                .collect()
        })
    }

    /// The value `to_dict()` gives `entry` for `obj`.
    pub(crate) fn out_value(&self, obj: &PyAny, entry: &OutEntry) -> PyResult<PyObject> {
        let py = obj.py();
        let v = attr_or_none(py, obj, entry.attr.as_ref(py))?;
        if let (true, Some(name)) = (v.is_none(py), &entry.fk_name) {
            let related = attr_or_none(py, obj, name.as_ref(py))?;
            let related = related.as_ref(py);
            if super::runtime::is_model(py, related)? {
                return pk_of(py, related);
            }
        }
        Ok(v)
    }
}

impl Drop for PyModelSpec {
    fn drop(&mut self) {
        if self.owner {
            super::unregister(self.inner.id);
        }
    }
}

impl PyModelSpec {
    fn field_handle(&self, f: &FieldSpec) -> PyFieldSpec {
        PyFieldSpec {
            inner: f.clone(),
            model: self.inner.name.clone(),
        }
    }
}

#[pymethods]
impl PyModelSpec {
    #[getter]
    fn id(&self) -> ModelId {
        self.inner.id
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn table(&self) -> &str {
        &self.inner.table
    }

    #[getter]
    fn quoted_table(&self) -> &str {
        &self.inner.quoted_table
    }

    #[getter]
    fn pk_name(&self) -> Option<&str> {
        self.inner.pk_field().map(|f| f.name.as_str())
    }

    #[getter]
    fn pk_column(&self) -> Option<&str> {
        self.inner.pk_field().map(|f| f.column.as_str())
    }

    #[getter]
    fn quoted_pk(&self) -> Option<&str> {
        self.inner.pk_field().map(|f| f.quoted_column.as_str())
    }

    #[getter]
    fn fields(&self) -> Vec<PyFieldSpec> {
        self.inner
            .fields
            .iter()
            .map(|f| self.field_handle(f))
            .collect()
    }

    /// `False` when a declaration error was deferred (a ForeignKey target that
    /// is not a model, or a `sql_type` that raised): DDL will fail.
    #[getter]
    fn complete(&self) -> bool {
        self.inner
            .fields
            .iter()
            .all(|f| f.pg_type.is_some() && f.relation.as_ref().is_none_or(|r| r.target.is_some()))
    }

    #[getter]
    fn owner(&self) -> bool {
        self.owner
    }

    fn field(&self, name: &str) -> Option<PyFieldSpec> {
        self.inner.field(name).map(|f| self.field_handle(f))
    }

    /// Resolve a query identifier.
    ///
    /// Returns `(quoted_column, declared)`, or `None` when `name` is not a
    /// valid identifier (or not a string) and must be rejected.
    fn resolve(&self, name: &PyAny) -> Option<(String, bool)> {
        let s = name.downcast::<PyString>().ok()?.to_str().ok()?;
        match self.inner.resolve(s) {
            Resolved::Declared(f) => Some((f.quoted_column.clone(), true)),
            Resolved::Undeclared(q) => Some((q, false)),
            Resolved::Invalid => None,
        }
    }

    /// `{identifier: quoted_column}` for every valid identifier that names a
    /// declared field or column. Built once; the ORM keeps it on `_meta`.
    fn lookup<'p>(&self, py: Python<'p>) -> PyResult<&'p PyDict> {
        let d = PyDict::new(py);
        for (k, q) in self.inner.identifiers() {
            d.set_item(k, q)?;
        }
        Ok(d)
    }

    /// `{field_name: quoted_column}` for every field, in declaration order.
    fn quoted_columns<'p>(&self, py: Python<'p>) -> PyResult<&'p PyDict> {
        let d = PyDict::new(py);
        for f in &self.inner.fields {
            d.set_item(&f.name, &f.quoted_column)?;
        }
        Ok(d)
    }

    /// `Model.to_dict()`: one key per readable field (the ForeignKey column for
    /// a ForeignKey), values read with `getattr`. Native since 1.4.0 Phase 02.
    fn to_dict(&self, py: Python<'_>, obj: &PyAny) -> PyResult<PyObject> {
        // SAFETY: `PyDict_New` returns a new reference or NULL with an
        // exception set; `PyDict_SetItem` does not steal its arguments.
        unsafe {
            let d = pyo3::ffi::PyDict_New();
            if d.is_null() {
                return Err(PyErr::fetch(py));
            }
            let d = PyObject::from_owned_ptr(py, d);
            for e in self.out_plan(py) {
                let v = self.out_value(obj, e)?;
                if pyo3::ffi::PyDict_SetItem(d.as_ptr(), e.key.as_ptr(), v.as_ptr()) != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
            Ok(d)
        }
    }

    #[pyo3(signature = (if_not_exists = true))]
    fn create_table_sql(&self, if_not_exists: bool) -> PyResult<String> {
        Ok(self.inner.create_table_sql(if_not_exists)?)
    }

    #[pyo3(signature = (if_exists = true))]
    fn drop_table_sql(&self, if_exists: bool) -> String {
        self.inner.drop_table_sql(if_exists)
    }

    fn __len__(&self) -> usize {
        self.inner.fields.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "<ModelSpec {} table={} fields={}>",
            self.inner.name,
            self.inner.quoted_table,
            self.inner.fields.len()
        )
    }
}

/// Read-only view of one compiled field.
#[pyclass(name = "FieldSpec", module = "cello._cello", frozen)]
pub struct PyFieldSpec {
    pub inner: FieldSpec,
    model: String,
}

#[pymethods]
impl PyFieldSpec {
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    #[getter]
    fn column(&self) -> &str {
        &self.inner.column
    }

    #[getter]
    fn quoted_column(&self) -> &str {
        &self.inner.quoted_column
    }

    /// The column type as written in DDL (`"VARCHAR(100)"`), or `None`.
    #[getter]
    fn pg_type(&self) -> Option<String> {
        self.inner.pg_type.as_ref().map(|t| t.ddl().into_owned())
    }

    /// Short type tag (`"varchar"`, `"custom"`, ...), or `None`.
    #[getter]
    fn kind(&self) -> Option<&'static str> {
        self.inner.pg_type.as_ref().map(|t| t.kind())
    }

    #[getter]
    fn primary_key(&self) -> bool {
        self.inner.primary_key
    }

    #[getter]
    fn null(&self) -> bool {
        self.inner.nullable
    }

    #[getter]
    fn unique(&self) -> bool {
        self.inner.unique
    }

    #[getter]
    fn max_length(&self) -> Option<u32> {
        self.inner.max_length
    }

    #[getter]
    fn auto(&self) -> bool {
        self.inner.auto
    }

    #[getter]
    fn auto_now_add(&self) -> bool {
        self.inner.auto_now_add
    }

    #[getter]
    fn readable(&self) -> bool {
        self.inner.readable
    }

    #[getter]
    fn writable(&self) -> bool {
        self.inner.writable
    }

    #[getter]
    fn write_only(&self) -> bool {
        self.inner.write_only()
    }

    /// `"none"`, `"now"`, `"callable"`, `"bool"`, `"int"`, `"float"`,
    /// `"text"` or `"other"`.
    #[getter]
    fn default_kind(&self) -> &'static str {
        match &self.inner.default {
            DefaultSpec::None => "none",
            DefaultSpec::Now => "now",
            DefaultSpec::Callable => "callable",
            DefaultSpec::Literal(Literal::Bool(_)) => "bool",
            DefaultSpec::Literal(Literal::Int(_)) => "int",
            DefaultSpec::Literal(Literal::Float(_)) => "float",
            DefaultSpec::Literal(Literal::Text(_)) => "text",
            DefaultSpec::Literal(Literal::Other(_)) => "other",
        }
    }

    /// The `DEFAULT` clause body, or `None`.
    #[getter]
    fn default_sql(&self) -> Option<String> {
        self.inner.default.to_sql()
    }

    /// `(table, pk_column, on_delete, model_id)` for a ForeignKey, else `None`.
    /// `table` is `None` when the target was not a model class.
    #[allow(clippy::type_complexity)]
    #[getter]
    fn references(
        &self,
    ) -> Option<(
        Option<String>,
        Option<String>,
        &'static str,
        Option<ModelId>,
    )> {
        self.inner.relation.as_ref().map(|r| match &r.target {
            Some(t) => (
                Some(t.table.clone()),
                t.pk_column.clone(),
                r.on_delete.as_sql(),
                t.model,
            ),
            None => (None, None, r.on_delete.as_sql(), None),
        })
    }

    fn column_ddl(&self) -> PyResult<String> {
        Ok(self.inner.column_ddl(&self.model)?)
    }

    fn __repr__(&self) -> String {
        format!(
            "<FieldSpec {}.{} column={} type={}>",
            self.model,
            self.inner.name,
            self.inner.quoted_column,
            self.inner.pg_type.as_ref().map_or("?".into(), |t| t.ddl())
        )
    }
}
