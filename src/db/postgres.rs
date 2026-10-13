//! Native async Postgres connection pool exposed to Python.
//!
//! Backed by `deadpool-postgres` + `tokio-postgres`. Query methods return Python
//! awaitables via `pyo3_async_runtimes::tokio::future_into_py`, so the GIL is released
//! for the entire duration of the database I/O (verified to resolve on Cello's
//! persistent asyncio loop — see `src/async_loop.rs` and `src/http_client.rs`).
//!
//! API (asyncpg-flavoured, `$1` positional params):
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

use std::str::FromStr;
use std::sync::Arc;

use deadpool_postgres::{Manager, ManagerConfig, Object, Pool, RecyclingMethod};
use pyo3::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use tokio::sync::Mutex;
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Statement};

use super::value::{SqlParam, py_params_to_sqlparams, row_to_pydict};

fn rt_err(msg: impl std::fmt::Display) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(msg.to_string())
}

/// Borrow a `Vec<SqlParam>` as the `&[&(dyn ToSql + Sync)]` slice tokio-postgres wants.
fn as_sql_refs(params: &[SqlParam]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|p| p as &(dyn ToSql + Sync)).collect()
}

/// Prepare `sql` through the connection's statement cache.
///
/// `client.execute("SQL", params)` parses, describes and binds on every call —
/// two round trips per query. Preparing once per distinct SQL text makes the
/// repeated case (every ORM call, every request) a single round trip. The cache
/// lives on the pooled connection, so it survives check-out/return with
/// `RecyclingMethod::Fast`; `tokio-postgres` bounds it and evicts on its own, so
/// SQL built from interpolated values cannot grow it without limit.
async fn prepare_cached(client: &Object, sql: &str) -> Result<Statement, tokio_postgres::Error> {
    client.prepare_cached(sql).await
}

/// Native Postgres connection pool.
#[pyclass(name = "Database")]
#[derive(Clone)]
pub struct PyDatabase {
    pool: Pool,
    #[pyo3(get)]
    dsn: String,
}

impl PyDatabase {
    /// Build a pool from a Postgres URL. Synchronous — connections are opened
    /// lazily on first use, so this is safe to call at configuration time.
    pub fn connect(dsn: &str, pool_size: usize) -> PyResult<Self> {
        let pg_config = tokio_postgres::Config::from_str(dsn)
            .map_err(|e| rt_err(format!("invalid Postgres URL: {e}")))?;
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
        let pool = Pool::builder(mgr)
            .max_size(pool_size.max(1))
            .build()
            .map_err(|e| rt_err(format!("failed to build pool: {e}")))?;
        Ok(Self {
            pool,
            dsn: dsn.to_string(),
        })
    }
}

#[pymethods]
impl PyDatabase {
    /// Execute a statement, returning the number of rows affected.
    #[pyo3(signature = (sql, *params))]
    fn execute<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = pool.get().await.map_err(rt_err)?;
            let stmt = prepare_cached(&client, &sql).await.map_err(rt_err)?;
            let refs = as_sql_refs(&sql_params);
            let n = client.execute(&stmt, &refs).await.map_err(rt_err)?;
            Ok::<i64, PyErr>(n as i64)
        })
    }

    /// Run a query and return every row as a list of dicts.
    #[pyo3(signature = (sql, *params))]
    fn fetch<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = pool.get().await.map_err(rt_err)?;
            let stmt = prepare_cached(&client, &sql).await.map_err(rt_err)?;
            let refs = as_sql_refs(&sql_params);
            let rows = client.query(&stmt, &refs).await.map_err(rt_err)?;
            Python::attach(|py| {
                let out = pyo3::types::PyList::empty(py);
                for row in &rows {
                    out.append(row_to_pydict(py, row)?)?;
                }
                out.into_py_any(py)
            })
        })
    }

    /// Run a query and return the first row as a dict (or `None`).
    #[pyo3(signature = (sql, *params))]
    fn fetchrow<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = pool.get().await.map_err(rt_err)?;
            let stmt = prepare_cached(&client, &sql).await.map_err(rt_err)?;
            let refs = as_sql_refs(&sql_params);
            let row = client.query_opt(&stmt, &refs).await.map_err(rt_err)?;
            Python::attach(|py| match row {
                Some(r) => row_to_pydict(py, &r),
                None => Ok(py.None()),
            })
        })
    }

    /// Run a query and return the first column of the first row (or `None`).
    #[pyo3(signature = (sql, *params))]
    fn fetchval<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let pool = self.pool.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = pool.get().await.map_err(rt_err)?;
            let stmt = prepare_cached(&client, &sql).await.map_err(rt_err)?;
            let refs = as_sql_refs(&sql_params);
            let row = client.query_opt(&stmt, &refs).await.map_err(rt_err)?;
            Python::attach(|py| match row {
                Some(r) if !r.is_empty() => {
                    let dict = row_to_pydict(py, &r)?;
                    let dict = dict.bind(py).cast::<pyo3::types::PyDict>()?;
                    // First column by position.
                    let name = r.columns()[0].name();
                    Ok(dict
                        .get_item(name)?
                        .map(|v| v.unbind())
                        .unwrap_or_else(|| py.None()))
                }
                _ => Ok(py.None()),
            })
        })
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
    fn ping<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = self.pool.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = pool.get().await.map_err(rt_err)?;
            client.execute("SELECT 1", &[]).await.map_err(rt_err)?;
            Ok(true)
        })
    }

    /// Close the pool, dropping all idle connections.
    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = self.pool.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            pool.close();
            Ok(Python::attach(|py| py.None()))
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
    fn __aenter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let pool = self.pool.clone();
        let slot = self.conn.clone();
        let this = self.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let client = pool.get().await.map_err(rt_err)?;
            client.batch_execute("BEGIN").await.map_err(rt_err)?;
            *slot.lock().await = Some(client);
            Ok(Python::attach(|py| this.into_py_any(py))?)
        })
    }

    #[pyo3(signature = (exc_type, _exc_val, _exc_tb))]
    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        exc_type: Py<PyAny>,
        _exc_val: Py<PyAny>,
        _exc_tb: Py<PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let slot = self.conn.clone();
        let errored = !exc_type.is_none(py);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            if let Some(client) = slot.lock().await.take() {
                let stmt = if errored { "ROLLBACK" } else { "COMMIT" };
                client.batch_execute(stmt).await.map_err(rt_err)?;
                // client drops here → returned to the pool.
            }
            // Do not suppress the exception.
            Ok(false)
        })
    }

    /// Execute a statement inside the transaction; returns rows affected.
    #[pyo3(signature = (sql, *params))]
    fn execute<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = slot.lock().await;
            let client = guard
                .as_ref()
                .ok_or_else(|| rt_err("transaction is not active (use `async with`)"))?;
            let stmt = prepare_cached(client, &sql).await.map_err(rt_err)?;
            let refs = as_sql_refs(&sql_params);
            let n = client.execute(&stmt, &refs).await.map_err(rt_err)?;
            Ok::<i64, PyErr>(n as i64)
        })
    }

    /// Fetch rows inside the transaction.
    #[pyo3(signature = (sql, *params))]
    fn fetch<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = slot.lock().await;
            let client = guard
                .as_ref()
                .ok_or_else(|| rt_err("transaction is not active (use `async with`)"))?;
            let stmt = prepare_cached(client, &sql).await.map_err(rt_err)?;
            let refs = as_sql_refs(&sql_params);
            let rows = client.query(&stmt, &refs).await.map_err(rt_err)?;
            Python::attach(|py| {
                let out = pyo3::types::PyList::empty(py);
                for row in &rows {
                    out.append(row_to_pydict(py, row)?)?;
                }
                out.into_py_any(py)
            })
        })
    }

    /// Fetch a single row inside the transaction.
    #[pyo3(signature = (sql, *params))]
    fn fetchrow<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: &Bound<'py, PyTuple>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let sql_params = py_params_to_sqlparams(&params.iter().collect::<Vec<_>>())?;
        let slot = self.conn.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let guard = slot.lock().await;
            let client = guard
                .as_ref()
                .ok_or_else(|| rt_err("transaction is not active (use `async with`)"))?;
            let stmt = prepare_cached(client, &sql).await.map_err(rt_err)?;
            let refs = as_sql_refs(&sql_params);
            let row = client.query_opt(&stmt, &refs).await.map_err(rt_err)?;
            Python::attach(|py| match row {
                Some(r) => row_to_pydict(py, &r),
                None => Ok(py.None()),
            })
        })
    }
}
