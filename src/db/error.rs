//! Database exception hierarchy and the mapping from driver / server errors.
//!
//! ```text
//! RuntimeError
//! └── DatabaseError                 sqlstate / detail / constraint / table / column
//!     ├── IntegrityError            SQLSTATE class 23
//!     │   ├── UniqueViolation       23505
//!     │   ├── NotNullViolation      23502
//!     │   ├── ForeignKeyViolation   23503
//!     │   └── CheckViolation        23514
//!     ├── DataError                 SQLSTATE class 22, and parameters Cello cannot bind
//!     │   ├── ParameterTypeError    (also a TypeError)     wrong Python type for $n
//!     │   ├── ParameterRangeError   (also an OverflowError) value out of range for $n
//!     │   └── ParameterValueError   (also a ValueError)    unparsable value for $n
//!     ├── OperationalError          connection / pool failures, classes 08 40 53 57 58
//!     └── ProgrammingError          SQLSTATE class 42, wrong parameter count
//! ```
//!
//! `DatabaseError` subclasses `RuntimeError`, so code written against 1.3.0
//! (`except RuntimeError`) keeps working.
//!
//! Errors are carried across the I/O thread as plain Rust data ([`PgError`]) and
//! turned into Python exceptions on the event-loop thread, where the GIL is
//! already held (see `crate::async_bridge`).

use std::error::Error as StdError;
use std::fmt;

use pyo3::exceptions::{PyOverflowError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyTuple, PyType};

pyo3::create_exception!(
    cello,
    DatabaseError,
    PyRuntimeError,
    "Base class for database errors. Subclasses RuntimeError."
);
pyo3::create_exception!(
    cello,
    IntegrityError,
    DatabaseError,
    "A constraint was violated (SQLSTATE class 23)."
);
pyo3::create_exception!(
    cello,
    UniqueViolation,
    IntegrityError,
    "Unique constraint violated (23505)."
);
pyo3::create_exception!(
    cello,
    NotNullViolation,
    IntegrityError,
    "NOT NULL constraint violated (23502)."
);
pyo3::create_exception!(
    cello,
    ForeignKeyViolation,
    IntegrityError,
    "Foreign key constraint violated (23503)."
);
pyo3::create_exception!(
    cello,
    CheckViolation,
    IntegrityError,
    "CHECK constraint violated (23514)."
);
pyo3::create_exception!(
    cello,
    DataError,
    DatabaseError,
    "Invalid data (SQLSTATE class 22), or a parameter that cannot be bound."
);
pyo3::create_exception!(
    cello,
    OperationalError,
    DatabaseError,
    "Connection, pool or server-availability failure."
);
pyo3::create_exception!(
    cello,
    ProgrammingError,
    DatabaseError,
    "Invalid SQL or schema reference (SQLSTATE class 42), or wrong parameter count."
);

/// `DataError` subclasses that also derive from a builtin, created once with
/// `type(name, (DataError, builtin), ns)` because `create_exception!` takes a
/// single base.
static PARAM_TYPES: GILOnceCell<[Py<PyType>; 3]> = GILOnceCell::new();

fn param_types(py: Python<'_>) -> &'static [Py<PyType>; 3] {
    PARAM_TYPES
        .get_or_try_init(py, || -> PyResult<_> {
            let make = |name: &str, builtin: &PyType, doc: &str| -> PyResult<Py<PyType>> {
                let ns = PyDict::new(py);
                ns.set_item("__module__", "cello")?;
                ns.set_item("__doc__", doc)?;
                let bases = PyTuple::new(py, [py.get_type::<DataError>(), builtin]);
                let t = py.get_type::<PyType>().call1((name, bases, ns))?;
                Ok(t.downcast::<PyType>()?.into())
            };
            Ok([
                make(
                    "ParameterTypeError",
                    py.get_type::<PyTypeError>(),
                    "A query parameter has a Python type that cannot be bound to the \
                     parameter's Postgres type. Also a TypeError.",
                )?,
                make(
                    "ParameterRangeError",
                    py.get_type::<PyOverflowError>(),
                    "A query parameter is out of range for the parameter's Postgres \
                     type. Also an OverflowError.",
                )?,
                make(
                    "ParameterValueError",
                    py.get_type::<PyValueError>(),
                    "A query parameter's value cannot be parsed as the parameter's \
                     Postgres type. Also a ValueError.",
                )?,
            ])
        })
        .expect("creating the cello parameter exception classes cannot fail")
}

const ATTRS: [&str; 5] = ["sqlstate", "detail", "constraint", "table", "column"];

/// Register the hierarchy on the extension module.
pub fn register(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    let base = py.get_type::<DatabaseError>();
    for a in ATTRS {
        base.setattr(a, py.None())?;
    }
    m.add("DatabaseError", base)?;
    m.add("IntegrityError", py.get_type::<IntegrityError>())?;
    m.add("UniqueViolation", py.get_type::<UniqueViolation>())?;
    m.add("NotNullViolation", py.get_type::<NotNullViolation>())?;
    m.add("ForeignKeyViolation", py.get_type::<ForeignKeyViolation>())?;
    m.add("CheckViolation", py.get_type::<CheckViolation>())?;
    m.add("DataError", py.get_type::<DataError>())?;
    m.add("OperationalError", py.get_type::<OperationalError>())?;
    m.add("ProgrammingError", py.get_type::<ProgrammingError>())?;
    let [t, r, v] = param_types(py);
    m.add("ParameterTypeError", t.as_ref(py))?;
    m.add("ParameterRangeError", r.as_ref(py))?;
    m.add("ParameterValueError", v.as_ref(py))?;
    Ok(())
}

// ── Bind errors (raised from `ToSql`) ────────────────────────────────────────

/// What went wrong binding one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindErrorKind {
    /// The Python type cannot be bound to the Postgres type.
    Type,
    /// The value does not fit the Postgres type.
    Range,
    /// The value cannot be parsed as the Postgres type.
    Value,
}

/// Error returned by `SqlParam::to_sql`; tokio-postgres wraps it with the
/// parameter index, and [`PgError::from_pg`] unwraps it again.
#[derive(Debug)]
pub struct BindError {
    pub kind: BindErrorKind,
    pub msg: String,
}

impl fmt::Display for BindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl StdError for BindError {}

// ── Rust-side error carried to the loop thread ───────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrClass {
    Database,
    Integrity,
    Unique,
    NotNull,
    ForeignKey,
    Check,
    Data,
    Operational,
    Programming,
    ParamType,
    ParamRange,
    ParamValue,
}

/// A database error as plain data (`Send`, no GIL needed to build it).
#[derive(Debug)]
pub struct PgError {
    pub class: ErrClass,
    pub msg: String,
    pub sqlstate: Option<String>,
    pub detail: Option<String>,
    pub constraint: Option<String>,
    pub table: Option<String>,
    pub column: Option<String>,
}

fn class_for_sqlstate(code: &str) -> ErrClass {
    match code {
        "23505" => ErrClass::Unique,
        "23502" => ErrClass::NotNull,
        "23503" => ErrClass::ForeignKey,
        "23514" => ErrClass::Check,
        _ => match code.get(..2) {
            Some("23") => ErrClass::Integrity,
            Some("22") => ErrClass::Data,
            Some("42") => ErrClass::Programming,
            Some("08" | "40" | "53" | "57" | "58") => ErrClass::Operational,
            _ => ErrClass::Database,
        },
    }
}

impl PgError {
    pub fn new(class: ErrClass, msg: impl Into<String>) -> Self {
        Self {
            class,
            msg: msg.into(),
            sqlstate: None,
            detail: None,
            constraint: None,
            table: None,
            column: None,
        }
    }

    /// Connection / pool failure.
    pub fn operational(msg: impl fmt::Display) -> Self {
        Self::new(ErrClass::Operational, msg.to_string())
    }

    /// Failure to check a connection out of the pool. The whole error chain
    /// is included: the driver's own message is only its error kind ("error
    /// performing TLS handshake", "db error"), and the useful part — e.g.
    /// "invalid peer certificate: NotValidForName" or the server's pg_hba
    /// rejection — is in the sources.
    pub fn pool(e: impl StdError) -> Self {
        Self::new(ErrClass::Operational, with_causes(&e))
    }

    /// Classify a tokio-postgres error.
    pub fn from_pg(e: &tokio_postgres::Error) -> Self {
        if let Some(db) = e.as_db_error() {
            let code = db.code().code();
            return Self {
                class: class_for_sqlstate(code),
                // tokio-postgres renders a server error as just "db error";
                // include the server's message (and any DETAIL/HINT).
                msg: format!("db error: {db}"),
                sqlstate: Some(code.to_string()),
                detail: db.detail().map(str::to_string),
                constraint: db.constraint().map(str::to_string),
                table: db.table().map(str::to_string),
                column: db.column().map(str::to_string),
            };
        }
        let text = e.to_string();
        if let Some(bind) = e.source().and_then(|s| s.downcast_ref::<BindError>()) {
            // "error serializing parameter N" (0-based) → "$N+1".
            let msg = match text
                .strip_prefix("error serializing parameter ")
                .and_then(|n| n.trim().parse::<usize>().ok())
            {
                Some(i) => format!("parameter ${}: {}", i + 1, bind.msg),
                None => format!("{text}: {}", bind.msg),
            };
            let class = match bind.kind {
                BindErrorKind::Type => ErrClass::ParamType,
                BindErrorKind::Range => ErrClass::ParamRange,
                BindErrorKind::Value => ErrClass::ParamValue,
            };
            return Self::new(class, msg);
        }
        if e.is_closed()
            || e.source()
                .is_some_and(|s| s.downcast_ref::<std::io::Error>().is_some())
        {
            // Include the cause (e.g. the TLS / certificate failure).
            return Self::new(ErrClass::Operational, with_causes(e));
        }
        if text.starts_with("expected ") && text.contains(" parameters but got ") {
            return Self::new(ErrClass::Programming, text);
        }
        Self::new(ErrClass::Database, text)
    }

    /// Build the Python exception (loop thread, GIL held).
    pub fn into_pyerr(self, py: Python<'_>) -> PyErr {
        let cls: &PyType = match self.class {
            ErrClass::Database => py.get_type::<DatabaseError>(),
            ErrClass::Integrity => py.get_type::<IntegrityError>(),
            ErrClass::Unique => py.get_type::<UniqueViolation>(),
            ErrClass::NotNull => py.get_type::<NotNullViolation>(),
            ErrClass::ForeignKey => py.get_type::<ForeignKeyViolation>(),
            ErrClass::Check => py.get_type::<CheckViolation>(),
            ErrClass::Data => py.get_type::<DataError>(),
            ErrClass::Operational => py.get_type::<OperationalError>(),
            ErrClass::Programming => py.get_type::<ProgrammingError>(),
            ErrClass::ParamType => param_types(py)[0].as_ref(py),
            ErrClass::ParamRange => param_types(py)[1].as_ref(py),
            ErrClass::ParamValue => param_types(py)[2].as_ref(py),
        };
        let build = || -> PyResult<PyErr> {
            let inst = cls.call1((self.msg.as_str(),))?;
            let vals = [
                &self.sqlstate,
                &self.detail,
                &self.constraint,
                &self.table,
                &self.column,
            ];
            for (name, v) in ATTRS.iter().zip(vals) {
                if let Some(v) = v {
                    inst.setattr(*name, v.as_str())?;
                }
            }
            Ok(PyErr::from_value(inst))
        };
        build().unwrap_or_else(|e| e)
    }
}

/// Raise a parameter error synchronously (Python → Rust conversion, GIL held).
pub fn param_type_error(py: Python<'_>, msg: String) -> PyErr {
    PgError::new(ErrClass::ParamType, msg).into_pyerr(py)
}

/// A malformed value received from the server (decode side).
pub fn decode_error(py: Python<'_>, msg: impl fmt::Display) -> PyErr {
    PgError::new(ErrClass::Data, format!("cannot decode column: {msg}")).into_pyerr(py)
}

/// `e` followed by each distinct message in its `source()` chain.
fn with_causes(e: &dyn StdError) -> String {
    let mut msg = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        let part = c.to_string();
        if !part.is_empty() && !msg.contains(&part) {
            msg.push_str(": ");
            msg.push_str(&part);
        }
        cur = c.source();
    }
    msg
}
