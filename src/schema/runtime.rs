//! The Python ORM classes the native serializer recognises (Phase 02).
//!
//! `python/cello/orm.py` calls [`orm_bind`] once at import with its `Model`
//! base class, `QuerySet` class, the base `Model.to_dict` function and the
//! coroutine factory that runs returned querysets. The response serializer
//! (`src/json.rs`) then knows, without importing anything per request:
//!
//! * a **model instance** serializes as its `to_dict()` (written straight to
//!   the response buffer when `to_dict` is not overridden);
//! * a **queryset** returned from a handler is a hole in the body, filled with
//!   the rows' JSON once the query has run (`src/handler.rs`).
//!
//! Re-binding (a reloaded `cello.orm`) replaces the previous classes.

use parking_lot::RwLock;
use pyo3::prelude::*;
use pyo3::types::PyType;

struct OrmClasses {
    model: Py<PyType>,
    queryset: Py<PyType>,
    to_dict: PyObject,
    run_querysets: PyObject,
}

static ORM: RwLock<Option<OrmClasses>> = RwLock::new(None);

/// Register the ORM's classes with the native serializer. Called by
/// `cello.orm` at import.
#[pyfunction]
pub fn orm_bind(
    model: &PyType,
    queryset: &PyType,
    to_dict: PyObject,
    run_querysets: PyObject,
) -> PyResult<()> {
    *ORM.write() = Some(OrmClasses {
        model: model.into(),
        queryset: queryset.into(),
        to_dict,
        run_querysets,
    });
    Ok(())
}

/// What an object is, as far as the serializer cares.
pub enum OrmKind {
    /// A `Model` instance whose class keeps the base `to_dict`.
    Model,
    /// A `Model` instance whose class overrides `to_dict`.
    ModelCustom,
    QuerySet,
    Other,
}

/// Classify `obj`. Only called for objects that are not `str`, `dict`,
/// `list`, `bool`, `int` or `float`, so the common path never pays for it.
pub fn classify(py: Python<'_>, obj: &PyAny) -> PyResult<OrmKind> {
    // Take references out of the lock: `isinstance` can run Python code.
    let Some((model, queryset, base_to_dict)) = ORM.read().as_ref().map(|o| {
        (
            o.model.clone_ref(py),
            o.queryset.clone_ref(py),
            o.to_dict.clone_ref(py),
        )
    }) else {
        return Ok(OrmKind::Other);
    };
    if obj.is_instance(model.as_ref(py))? {
        let to_dict = obj.get_type().getattr(pyo3::intern!(py, "to_dict"))?;
        return Ok(if to_dict.is(base_to_dict.as_ref(py)) {
            OrmKind::Model
        } else {
            OrmKind::ModelCustom
        });
    }
    if obj.is_instance(queryset.as_ref(py))? {
        return Ok(OrmKind::QuerySet);
    }
    Ok(OrmKind::Other)
}

/// `isinstance(obj, Model)`.
pub fn is_model(py: Python<'_>, obj: &PyAny) -> PyResult<bool> {
    let model = ORM.read().as_ref().map(|o| o.model.clone_ref(py));
    match model {
        Some(m) => obj.is_instance(m.as_ref(py)),
        None => Ok(false),
    }
}

/// The awaitable that runs `querysets` and yields one `RawJSON` per queryset
/// (`cello.orm._run_querysets`).
pub fn run_querysets<'p>(py: Python<'p>, querysets: Vec<PyObject>) -> PyResult<&'p PyAny> {
    let f = match ORM.read().as_ref() {
        Some(orm) => orm.run_querysets.clone_ref(py),
        None => {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "cello.orm is not imported",
            ))
        }
    };
    f.into_ref(py).call1((querysets,))
}
