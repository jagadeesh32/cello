//! Native async Postgres connection pool exposed to Python.
//!
//! Backed by `deadpool-postgres` + `tokio-postgres`. Query methods return Python
//! awaitables via `crate::async_bridge`: the I/O runs on the `cello-io` thread
//! without the GIL, and rows are converted to Python objects on the event-loop
//! thread when the awaitable resolves.
//!
//! API (`$1` positional params):
//! ```python
//! rows = await request.database.fetch("SELECT * FROM users WHERE active = $1", True)
//! row  = await request.database.fetchrow("SELECT * FROM users WHERE id = $1", 1)
//! val  = await request.database.fetchval("SELECT count(*) FROM users")
//! n    = await request.database.execute("INSERT INTO users(name) VALUES($1)", "Alice")
//!
//! async with request.database.transaction() as tx:
//!     await tx.execute("UPDATE accounts SET balance = balance - $1 WHERE id = $2", 100, 1)
//!     await tx.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", 100, 2)
//! ```

use std::future::Future;
use std::sync::Arc;

use deadpool_postgres::{ClientWrapper, Manager, ManagerConfig, Object, Pool, RecyclingMethod};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use tokio::sync::Mutex;
use tokio_postgres::error::SqlState;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Row, Statement};

use super::error::{ErrClass, PgError};
use super::rows::{
    rows_to_json, rows_to_model_graph, rows_to_models, spec_of, JoinTarget, RawJson, RowJsonError,
};
use super::tls::{self, SslMode, TlsOptions};
use super::value::{
    first_column_to_py, py_params_to_sqlparams, row_to_pydict, rows_to_pylist, SqlParam,
};
use crate::async_bridge as bridge;
use crate::schema::{ModelSpec, PyModelSpec};

fn rt_err(msg: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(msg.to_string())
}

/// Classify a driver / server error (see `super::error`).
fn pg_err(e: tokio_postgres::Error) -> PgError {
    PgError::from_pg(&e)
}

fn not_active() -> PgError {
    PgError::new(
        ErrClass::Database,
        "transaction is not active (use `async with`)",
    )
}

/// Run `fut` on the I/O thread; on the loop thread, convert its value with
/// `conv` or raise its error as the mapped `cello.DatabaseError` subclass.
fn spawn<'py, F, T, C>(py: Python<'py>, fut: F, conv: C) -> PyResult<&'py PyAny>
where
    F: Future<Output = Result<T, PgError>> + Send + 'static,
    T: Send + 'static,
    C: FnOnce(Python<'_>, T) -> PyResult<PyObject> + Send + 'static,
{
    bridge::future_into_py_with(py, async move {
        let r = fut.await;
        Ok(move |py: Python<'_>| match r {
            Ok(v) => conv(py, v),
            Err(e) => Err(e.into_pyerr(py)),
        })
    })
}

fn int_to_py(py: Python<'_>, n: u64) -> PyResult<PyObject> {
    Ok((n as i64).into_py(py))
}

fn rows_to_py(py: Python<'_>, rows: Vec<Row>) -> PyResult<PyObject> {
    rows_to_pylist(py, &rows)
}

fn row_opt_to_py(py: Python<'_>, row: Option<Row>) -> PyResult<PyObject> {
    match row {
        Some(r) => row_to_pydict(py, &r),
        None => Ok(py.None()),
    }
}

fn val_opt_to_py(py: Python<'_>, row: Option<Row>) -> PyResult<PyObject> {
    match row {
        // First column (same semantics as looking it up in the row dict).
        Some(r) if !r.is_empty() => first_column_to_py(py, &r),
        _ => Ok(py.None()),
    }
}

/// `RawJSON` for rows already encoded on the I/O thread.
fn raw_json_to_py(py: Python<'_>, r: Result<Vec<u8>, RowJsonError>) -> PyResult<PyObject> {
    match r {
        Ok(bytes) => Ok(Py::new(py, RawJson { bytes })?.into_py(py)),
        Err(e) => Err(e.into_pyerr(py)),
    }
}

/// Encode fetched rows as JSON on the I/O thread (no GIL; see `super::rows`).
fn encode_rows(rows: Vec<Row>, spec: Option<&ModelSpec>) -> Result<Vec<u8>, RowJsonError> {
    rows_to_json(&rows, spec)
}

/// The model class and its compiled spec, for `_fetch_models`.
fn model_target(cls: &PyAny) -> PyResult<(PyObject, Py<PyModelSpec>)> {
    let py = cls.py();
    let spec = cls
        .getattr(pyo3::intern!(py, "_meta"))?
        .getattr(pyo3::intern!(py, "schema"))?
        .downcast::<PyCell<PyModelSpec>>()?;
    Ok((cls.into(), spec.into()))
}

/// Build model instances on the loop thread.
fn models_to_py(
    target: (PyObject, Py<PyModelSpec>),
) -> impl FnOnce(Python<'_>, Vec<Row>) -> PyResult<PyObject> + Send + 'static {
    move |py, rows| {
        let (cls, spec) = target;
        rows_to_models(py, cls.as_ref(py), spec.as_ref(py).get(), &rows)
    }
}

/// The models of a `select_related` query: `plan` is a list of
/// `(cls, parent_index, attr)` tuples, the base model first with
/// `parent_index`/`attr` `None` (see `rows::rows_to_model_graph`).
fn join_targets(plan: &PyAny) -> PyResult<Vec<JoinTarget>> {
    let mut out = Vec::new();
    for item in plan.iter()? {
        let (cls, parent, attr): (&PyAny, Option<usize>, Option<&pyo3::types::PyString>) =
            item?.extract()?;
        let (cls, spec) = model_target(cls)?;
        let parent = match (parent, attr) {
            (Some(p), Some(a)) if p < out.len() => Some((p, a.into())),
            (None, None) if out.is_empty() => None,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "invalid select_related plan entry",
                ))
            }
        };
        out.push(JoinTarget { cls, spec, parent });
    }
    if out.is_empty() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "empty select_related plan",
        ));
    }
    Ok(out)
}

/// Build the joined model graph on the loop thread.
fn graph_to_py(
    targets: Vec<JoinTarget>,
) -> impl FnOnce(Python<'_>, Vec<Row>) -> PyResult<PyObject> + Send + 'static {
    move |py, rows| rows_to_model_graph(py, &targets, &rows)
}

/// The first row of a query, or `None`; any further rows are discarded
/// (the connection drains them).
async fn query_first(
    client: &tokio_postgres::Client,
    stmt: &Statement,
    params: &[&(dyn ToSql + Sync)],
) -> Result<Option<Row>, tokio_postgres::Error> {
    use futures_util::TryStreamExt;
    let stream = client.query_raw(stmt, params.iter().copied()).await?;
    futures_util::pin_mut!(stream);
    stream.try_next().await
}

/// Borrow a `Vec<SqlParam>` as the `&[&(dyn ToSql + Sync)]` slice tokio-postgres wants.
fn as_sql_refs(params: &[SqlParam]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|p| p as &(dyn ToSql + Sync)).collect()
}

/// Upper bound on cached statements per connection. Code that builds many
/// distinct SQL strings (e.g. literals interpolated into raw SQL) would otherwise
/// grow the cache — and the server-side prepared statements behind it — without
/// limit. When the cap is reached the connection's cache is cleared; dropping the
/// last handle to a `Statement` closes it on the server.
const STMT_CACHE_CAP: usize = 512;

/// Prepare `sql` through the connection's statement cache.
///
/// The first call on a connection does the Parse/Describe round trip; every
/// later call with the same SQL text reuses the server-side statement, so a
/// query costs one round trip instead of two.
async fn prepare(client: &ClientWrapper, sql: &str) -> Result<Statement, tokio_postgres::Error> {
    if client.statement_cache.size() >= STMT_CACHE_CAP {
        client.statement_cache.clear();
    }
    client.prepare_cached(sql).await
}

/// True when a cached statement can no longer be used on the server:
///
/// * the schema under it changed (`ALTER TABLE ... ADD COLUMN` behind a
///   `SELECT *`, a table dropped and recreated with different columns) —
///   Postgres raises `0A000` from `RevalidateCachedQuery` ("cached plan must not
///   change result type"); the routine name is checked as well as the message so
///   this also matches when the server's `lc_messages` is not English;
/// * the server-side statement was deallocated behind the cache's back
///   (`DISCARD ALL` / `DEALLOCATE ALL` run through the same connection) — `26000`.
fn is_stale_statement(e: &tokio_postgres::Error) -> bool {
    let Some(db) = e.as_db_error() else {
        return false;
    };
    if *db.code() == SqlState::INVALID_SQL_STATEMENT_NAME {
        return true;
    }
    *db.code() == SqlState::FEATURE_NOT_SUPPORTED
        && (db.routine() == Some("RevalidateCachedQuery")
            || db
                .message()
                .starts_with("cached plan must not change result type"))
}

/// Run `$op` against a cached statement for `$sql`. If the statement went stale
/// after DDL, every connection's cache is cleared (the DDL affects them all) and
/// the operation is retried once with a freshly prepared statement.
///
/// Only for autocommit use: inside an explicit transaction the failed statement
/// has already aborted the transaction, so a retry cannot succeed.
macro_rules! with_cached_stmt {
    ($pool:expr, $client:expr, $sql:expr, |$stmt:ident| $op:expr) => {{
        let $stmt = prepare(&$client, $sql).await.map_err(pg_err)?;
        match $op.await {
            Err(e) if is_stale_statement(&e) => {
                $pool.manager().statement_caches.clear();
                let $stmt = prepare(&$client, $sql).await.map_err(pg_err)?;
                $op.await.map_err(pg_err)
            }
            other => other.map_err(pg_err),
        }
    }};
}

/// Run `$op` against a cached statement inside a held transaction connection.
/// A stale statement here aborts the transaction, so it cannot be retried; the
/// caches are cleared so the *next* attempt (a new transaction) succeeds, and
/// the error is surfaced to the caller.
macro_rules! with_cached_stmt_tx {
    ($pool:expr, $client:expr, $sql:expr, |$stmt:ident| $op:expr) => {{
        let $stmt = prepare($client, $sql).await.map_err(pg_err)?;
        match $op.await {
            Err(e) if is_stale_statement(&e) => {
                $pool.manager().statement_caches.clear();
                let mut err = PgError::from_pg(&e);
                err.msg = format!(
                    "{}; the schema changed under a cached prepared statement, which \
                     aborted this transaction. Statement caches have been cleared — \
                     retry the transaction.",
                    err.msg
                );
                Err(err)
            }
            other => other.map_err(pg_err),
        }
    }};
}

/// Native Postgres connection pool.
#[pyclass(name = "Database")]
#[derive(Clone)]
pub struct PyDatabase {
    pool: Pool,
    #[pyo3(get)]
    dsn: String,
    ssl_mode: SslMode,
}

impl PyDatabase {
    /// The effective `sslmode` this pool connects with.
    pub fn ssl_mode(&self) -> SslMode {
        self.ssl_mode
    }

    /// Build a pool from a Postgres URL. Synchronous — connections are opened
    /// lazily on first use, so this is safe to call at configuration time.
    ///
    /// TLS follows libpq's `sslmode` (DSN keys, overridden by `tls_opts`; see
    /// `super::tls`). Certificate files are loaded here, so a bad path or PEM
    /// raises `ValueError` from `enable_database()` instead of failing later.
    pub fn connect(dsn: &str, pool_size: usize, tls_opts: TlsOptions) -> PyResult<Self> {
        let tls::PgTls {
            config: pg_config,
            connector,
            mode,
        } = tls::configure(dsn, tls_opts).map_err(pyo3::exceptions::PyValueError::new_err)?;
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, connector, mgr_config);
        let pool = Pool::builder(mgr)
            .max_size(pool_size.max(1))
            .build()
            .map_err(|e| rt_err(format!("failed to build pool: {e}")))?;
        Ok(Self {
            pool,
            dsn: dsn.to_string(),
            ssl_mode: mode,
        })
    }
}

#[pymethods]
impl PyDatabase {
    /// The effective libpq `sslmode` (after defaults, the `require` +
    /// `sslrootcert` upgrade to `verify-ca`, and Unix-socket hosts).
    #[getter]
    #[pyo3(name = "sslmode")]
    fn py_sslmode(&self) -> &'static str {
        self.ssl_mode.as_str()
    }

    /// Execute a statement, returning the number of rows affected.
    #[pyo3(signature = (sql, *params))]
    fn execute<'py>(&self, py: Python<'py>, sql: String, params: &PyTuple) -> PyResult<&'py PyAny> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt!(pool, client, &sql, |stmt| client.execute(&stmt, &refs))
            },
            int_to_py,
        )
    }

    /// Run a query and return every row as a list of dicts.
    #[pyo3(signature = (sql, *params))]
    fn fetch<'py>(&self, py: Python<'py>, sql: String, params: &PyTuple) -> PyResult<&'py PyAny> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt!(pool, client, &sql, |stmt| client.query(&stmt, &refs))
            },
            rows_to_py,
        )
    }

    /// Run a query and return the first row as a dict (or `None`). Further
    /// rows are discarded.
    #[pyo3(signature = (sql, *params))]
    fn fetchrow<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt!(pool, client, &sql, |stmt| query_first(
                    &client, &stmt, &refs
                ))
            },
            row_opt_to_py,
        )
    }

    /// Run a query and return the first column of the first row (or `None`).
    /// Further rows are discarded.
    #[pyo3(signature = (sql, *params))]
    fn fetchval<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt!(pool, client, &sql, |stmt| query_first(
                    &client, &stmt, &refs
                ))
            },
            val_opt_to_py,
        )
    }

    /// Run a query and return its rows as a JSON array (`RawJSON`), encoded
    /// on the I/O thread without creating a Python object per row. With a
    /// `ModelSpec`, each object has that model's `to_dict()` keys; with
    /// `None`, the column names. Used by `QuerySet.json()`.
    #[pyo3(signature = (spec, sql, *params))]
    fn _fetch_json<'py>(
        &self,
        py: Python<'py>,
        spec: &PyAny,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let spec = spec_of(Some(spec))?;
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                let refs = as_sql_refs(&sql_params);
                let rows =
                    with_cached_stmt!(pool, client, &sql, |stmt| client.query(&stmt, &refs))?;
                Ok(encode_rows(rows, spec.as_deref()))
            },
            raw_json_to_py,
        )
    }

    /// Run a query and build an instance of model class `cls` per row
    /// (`Model._from_row` semantics, without the row dicts). Used by
    /// `QuerySet.all()`.
    #[pyo3(signature = (cls, sql, *params))]
    fn _fetch_models<'py>(
        &self,
        py: Python<'py>,
        cls: &PyAny,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let target = model_target(cls)?;
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt!(pool, client, &sql, |stmt| client.query(&stmt, &refs))
            },
            models_to_py(target),
        )
    }

    /// Run a query and return its rows as one JSON array (`cello.RawJSON`),
    /// encoded on the I/O thread straight from the Postgres wire format: no
    /// Python object per row or value. Each row is an object keyed by column
    /// name, with exactly the keys and values `fetch()` would give (a repeated
    /// column name keeps its first position and its last value). Return the
    /// result from a handler (alone or inside a dict / list) and it is written
    /// into the response verbatim.
    #[pyo3(signature = (sql, *params))]
    fn fetch_json<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        self._fetch_json(py, py.None().into_ref(py), sql, params)
    }

    /// Run a `select_related` join and build the base instances with their
    /// related instances attached (see `rows::rows_to_model_graph`). Used by
    /// `QuerySet.all()`.
    #[pyo3(signature = (plan, sql, *params))]
    fn _fetch_related<'py>(
        &self,
        py: Python<'py>,
        plan: &PyAny,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let targets = join_targets(plan)?;
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt!(pool, client, &sql, |stmt| client.query(&stmt, &refs))
            },
            graph_to_py(targets),
        )
    }

    /// Start a transaction. Use as an async context manager::
    ///
    ///     async with db.transaction() as tx:
    ///         await tx.execute(...)
    fn transaction(&self) -> PyTransaction {
        PyTransaction {
            pool: self.pool.clone(),
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Verify connectivity by acquiring a connection and running `SELECT 1`.
    fn ping<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                client.execute("SELECT 1", &[]).await.map_err(pg_err)?;
                Ok(())
            },
            |py, ()| Ok(true.into_py(py)),
        )
    }

    /// Close the pool, dropping all idle connections.
    fn close<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let pool = self.pool.clone();
        bridge::future_into_py(py, async move {
            pool.close();
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        let status = self.pool.status();
        format!(
            "<Database size={} available={} max_size={}>",
            status.size, status.available, status.max_size
        )
    }
}

/// A database transaction bound to a single pooled connection.
///
/// `BEGIN`/`COMMIT`/`ROLLBACK` are issued explicitly on a connection checked out
/// for the lifetime of the transaction. This sidesteps the `Transaction<'a>`
/// borrow lifetime, which cannot cross the Python await boundary.
#[pyclass(name = "Transaction")]
#[derive(Clone)]
pub struct PyTransaction {
    pool: Pool,
    conn: Arc<Mutex<Option<Object>>>,
}

#[pymethods]
impl PyTransaction {
    fn __aenter__<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let pool = self.pool.clone();
        let slot = self.conn.clone();
        let this = self.clone();
        spawn(
            py,
            async move {
                let client = pool.get().await.map_err(PgError::pool)?;
                client.batch_execute("BEGIN").await.map_err(pg_err)?;
                *slot.lock().await = Some(client);
                Ok(this)
            },
            |py, this| Ok(this.into_py(py)),
        )
    }

    #[pyo3(signature = (exc_type, _exc_val, _exc_tb))]
    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        exc_type: PyObject,
        _exc_val: PyObject,
        _exc_tb: PyObject,
    ) -> PyResult<&'py PyAny> {
        let slot = self.conn.clone();
        let errored = !exc_type.is_none(py);
        spawn(
            py,
            async move {
                if let Some(client) = slot.lock().await.take() {
                    let stmt = if errored { "ROLLBACK" } else { "COMMIT" };
                    client.batch_execute(stmt).await.map_err(pg_err)?;
                    // client drops here → returned to the pool.
                }
                Ok(())
            },
            // Do not suppress the exception.
            |py, ()| Ok(false.into_py(py)),
        )
    }

    /// Execute a statement inside the transaction; returns rows affected.
    #[pyo3(signature = (sql, *params))]
    fn execute<'py>(&self, py: Python<'py>, sql: String, params: &PyTuple) -> PyResult<&'py PyAny> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let guard = slot.lock().await;
                let client = guard.as_ref().ok_or_else(not_active)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt_tx!(pool, client, &sql, |stmt| client.execute(&stmt, &refs))
            },
            int_to_py,
        )
    }

    /// Fetch rows inside the transaction.
    #[pyo3(signature = (sql, *params))]
    fn fetch<'py>(&self, py: Python<'py>, sql: String, params: &PyTuple) -> PyResult<&'py PyAny> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let guard = slot.lock().await;
                let client = guard.as_ref().ok_or_else(not_active)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt_tx!(pool, client, &sql, |stmt| client.query(&stmt, &refs))
            },
            rows_to_py,
        )
    }

    /// Fetch the first row inside the transaction (further rows discarded).
    #[pyo3(signature = (sql, *params))]
    fn fetchrow<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let guard = slot.lock().await;
                let client = guard.as_ref().ok_or_else(not_active)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt_tx!(pool, client, &sql, |stmt| query_first(client, &stmt, &refs))
            },
            row_opt_to_py,
        )
    }

    /// `Database._fetch_json` inside the transaction.
    #[pyo3(signature = (spec, sql, *params))]
    fn _fetch_json<'py>(
        &self,
        py: Python<'py>,
        spec: &PyAny,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let spec = spec_of(Some(spec))?;
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let guard = slot.lock().await;
                let client = guard.as_ref().ok_or_else(not_active)?;
                let refs = as_sql_refs(&sql_params);
                let rows =
                    with_cached_stmt_tx!(pool, client, &sql, |stmt| client.query(&stmt, &refs))?;
                drop(guard);
                Ok(encode_rows(rows, spec.as_deref()))
            },
            raw_json_to_py,
        )
    }

    /// `Database.fetch_json` inside the transaction.
    #[pyo3(signature = (sql, *params))]
    fn fetch_json<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        self._fetch_json(py, py.None().into_ref(py), sql, params)
    }

    /// `Database._fetch_related` inside the transaction.
    #[pyo3(signature = (plan, sql, *params))]
    fn _fetch_related<'py>(
        &self,
        py: Python<'py>,
        plan: &PyAny,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let targets = join_targets(plan)?;
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let guard = slot.lock().await;
                let client = guard.as_ref().ok_or_else(not_active)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt_tx!(pool, client, &sql, |stmt| client.query(&stmt, &refs))
            },
            graph_to_py(targets),
        )
    }

    /// `Database._fetch_models` inside the transaction.
    #[pyo3(signature = (cls, sql, *params))]
    fn _fetch_models<'py>(
        &self,
        py: Python<'py>,
        cls: &PyAny,
        sql: String,
        params: &PyTuple,
    ) -> PyResult<&'py PyAny> {
        let target = model_target(cls)?;
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        let pool = self.pool.clone();
        spawn(
            py,
            async move {
                let guard = slot.lock().await;
                let client = guard.as_ref().ok_or_else(not_active)?;
                let refs = as_sql_refs(&sql_params);
                with_cached_stmt_tx!(pool, client, &sql, |stmt| client.query(&stmt, &refs))
            },
            models_to_py(target),
        )
    }
}
