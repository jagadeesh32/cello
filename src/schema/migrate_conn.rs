//! The database side of `cello migrate` (v1.4.0 Phase 05, **experimental**):
//! one dedicated Postgres session, catalog introspection, and the shadow-table
//! canonicalization the diff in [`super::migrate`] compares against.
//!
//! This is a cold-path CLI tool, so the session is deliberately simple: one
//! `tokio_postgres` connection on a private current-thread runtime, driven with
//! `block_on` while the GIL is released. A dedicated session (not the request
//! pool) matters for correctness: the advisory lock that serializes concurrent
//! `apply` runs is session-scoped, so lock, migrations and unlock must all run
//! on the same connection.
//!
//! Calling it from inside a running Tokio runtime (e.g. from a request
//! handler) raises instead of panicking.

use std::collections::HashMap;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use tokio::runtime::Runtime;
use tokio_postgres::Client;

use super::ident::quote_ident;
use super::migrate::{
    plan, Canonical, CanonicalTable, ConKind, LiveColumn, LiveConstraint, LiveTable, MigrateError,
};
use super::python::PyModelSpec;
use super::spec::{ModelSpec, OnDelete};
use crate::db::error::PgError;

impl From<MigrateError> for PyErr {
    fn from(e: MigrateError) -> PyErr {
        match e {
            MigrateError::Schema(s) => s.into(),
            other => PyValueError::new_err(other.to_string()),
        }
    }
}

const COLUMNS_SQL: &str = "\
SELECT c.relname::text, a.attname::text, format_type(a.atttypid, a.atttypmod)::text, \
       a.attnotnull, pg_get_expr(d.adbin, d.adrelid)::text, \
       a.attidentity::text <> '', a.attgenerated::text <> '' \
FROM pg_attribute a \
JOIN pg_class c ON c.oid = a.attrelid \
JOIN pg_namespace n ON n.oid = c.relnamespace \
LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
WHERE n.nspname = current_schema() AND c.relkind IN ('r', 'p') \
  AND c.relname::text = ANY($1::text[]) AND a.attnum > 0 AND NOT a.attisdropped \
ORDER BY c.relname, a.attnum";

const CONSTRAINTS_SQL: &str = "\
SELECT cl.relname::text, con.conname::text, con.contype::text, \
       ARRAY(SELECT a.attname::text FROM unnest(con.conkey) WITH ORDINALITY k(n, o) \
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.n ORDER BY k.o), \
       rc.relname::text, COALESCE(rn.nspname = current_schema(), true), \
       ARRAY(SELECT a.attname::text FROM unnest(con.confkey) WITH ORDINALITY k(n, o) \
             JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.n ORDER BY k.o), \
       con.confdeltype::text, con.confupdtype::text \
FROM pg_constraint con \
JOIN pg_class cl ON cl.oid = con.conrelid \
JOIN pg_namespace n ON n.oid = cl.relnamespace \
LEFT JOIN pg_class rc ON rc.oid = con.confrelid \
LEFT JOIN pg_namespace rn ON rn.oid = rc.relnamespace \
WHERE n.nspname = current_schema() AND cl.relname::text = ANY($1::text[]) \
  AND con.contype IN ('p', 'u', 'f') \
ORDER BY cl.relname, con.conname";

const SHADOW_SQL: &str = "\
SELECT a.attname::text, format_type(a.atttypid, a.atttypmod)::text, \
       pg_get_expr(d.adbin, d.adrelid)::text \
FROM pg_attribute a \
JOIN pg_class c ON c.oid = a.attrelid \
LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
WHERE c.relnamespace = pg_my_temp_schema() AND c.relname::text = $1 \
  AND a.attnum > 0 AND NOT a.attisdropped";

fn on_delete_from_code(code: &str) -> Option<OnDelete> {
    Some(match code {
        "a" => OnDelete::NoAction,
        "r" => OnDelete::Restrict,
        "c" => OnDelete::Cascade,
        "n" => OnDelete::SetNull,
        "d" => OnDelete::SetDefault,
        _ => return None,
    })
}

/// Read the live definition of `tables` (those that exist) in `current_schema()`.
async fn introspect(
    client: &Client,
    tables: &[String],
) -> Result<HashMap<String, LiveTable>, tokio_postgres::Error> {
    let mut out: HashMap<String, LiveTable> = HashMap::new();
    for row in client.query(COLUMNS_SQL, &[&tables]).await? {
        let table: String = row.get(0);
        out.entry(table.clone())
            .or_insert_with(|| LiveTable {
                name: table,
                ..Default::default()
            })
            .columns
            .push(LiveColumn {
                name: row.get(1),
                data_type: row.get(2),
                not_null: row.get(3),
                default: row.get(4),
                identity: row.get(5),
                generated: row.get(6),
            });
    }
    for row in client.query(CONSTRAINTS_SQL, &[&tables]).await? {
        let table: String = row.get(0);
        let Some(t) = out.get_mut(&table) else {
            continue;
        };
        let kind = match row.get::<_, String>(2).as_str() {
            "p" => ConKind::Primary,
            "u" => ConKind::Unique,
            _ => ConKind::Foreign,
        };
        let foreign = kind == ConKind::Foreign;
        t.constraints.push(LiveConstraint {
            name: row.get(1),
            kind,
            columns: row.get(3),
            ref_table: row.get(4),
            ref_same_schema: row.get(5),
            ref_columns: row.get(6),
            on_delete: if foreign {
                on_delete_from_code(&row.get::<_, String>(7))
            } else {
                None
            },
            on_update_no_action: !foreign || row.get::<_, String>(8) == "a",
        });
    }
    Ok(out)
}

/// Postgres' own rendering of every declared column: each model is created as
/// a temporary table (types and defaults only) inside a transaction that is
/// always rolled back, and `format_type` / `pg_get_expr` are read back.
///
/// Statements go through the extended protocol, so a declared type or default
/// can never smuggle a second statement in.
async fn canonicalize(
    client: &Client,
    specs: &[&ModelSpec],
) -> Result<HashMap<String, CanonicalTable>, PgError> {
    client
        .batch_execute("BEGIN")
        .await
        .map_err(|e| PgError::from_pg(&e))?;
    let result = async {
        let mut out = HashMap::new();
        for (i, spec) in specs.iter().enumerate() {
            let name = format!("cello_shadow_{i}");
            let mut cols = Vec::with_capacity(spec.fields.len());
            for f in &spec.fields {
                let ty = f.pg_type.as_ref().map(|t| t.ddl()).unwrap_or_default();
                let mut col = format!("{} {}", f.quoted_column, ty);
                if let Some(d) = f.default.to_sql() {
                    col.push_str(" DEFAULT ");
                    col.push_str(&d);
                }
                cols.push(col);
            }
            let ddl = format!(
                "CREATE TEMP TABLE {} ({})",
                quote_ident(&name),
                cols.join(", ")
            );
            client.execute(ddl.as_str(), &[]).await.map_err(|e| {
                let mut err = PgError::from_pg(&e);
                err.msg = format!(
                    "model {} ({}): declared column types/defaults are invalid: {}",
                    spec.name, spec.quoted_table, err.msg
                );
                err
            })?;
            let mut table = CanonicalTable::new();
            for row in client
                .query(SHADOW_SQL, &[&name])
                .await
                .map_err(|e| PgError::from_pg(&e))?
            {
                table.insert(
                    row.get(0),
                    Canonical {
                        data_type: row.get(1),
                        default: row.get(2),
                    },
                );
            }
            out.insert(spec.table.clone(), table);
        }
        Ok::<_, PgError>(out)
    }
    .await;
    // Always discard the shadow tables, also after an error.
    let rollback = client.batch_execute("ROLLBACK").await;
    let out = result?;
    rollback.map_err(|e| PgError::from_pg(&e))?;
    Ok(out)
}

fn live_to_py(py: Python<'_>, live: &HashMap<String, LiveTable>) -> PyResult<PyObject> {
    let out = PyDict::new(py);
    for (name, t) in live {
        let cols = PyList::empty(py);
        for c in &t.columns {
            let d = PyDict::new(py);
            d.set_item("name", &c.name)?;
            d.set_item("type", &c.data_type)?;
            d.set_item("not_null", c.not_null)?;
            d.set_item("default", &c.default)?;
            d.set_item("identity", c.identity)?;
            d.set_item("generated", c.generated)?;
            cols.append(d)?;
        }
        let cons = PyList::empty(py);
        for c in &t.constraints {
            let d = PyDict::new(py);
            d.set_item("name", &c.name)?;
            d.set_item(
                "kind",
                match c.kind {
                    ConKind::Primary => "primary",
                    ConKind::Unique => "unique",
                    ConKind::Foreign => "foreign",
                },
            )?;
            d.set_item("columns", &c.columns)?;
            d.set_item("ref_table", &c.ref_table)?;
            d.set_item("ref_columns", &c.ref_columns)?;
            d.set_item("on_delete", c.on_delete.map(|o| o.as_sql()))?;
            cons.append(d)?;
        }
        let td = PyDict::new(py);
        td.set_item("columns", cols)?;
        td.set_item("constraints", cons)?;
        out.set_item(name, td)?;
    }
    Ok(out.into())
}

/// A dedicated Postgres session for `cello migrate`.
#[pyclass(name = "MigrationConnection", module = "cello._cello")]
pub struct PyMigrationConnection {
    rt: Runtime,
    client: Option<Client>,
    conn_task: Option<tokio::task::JoinHandle<()>>,
}

impl PyMigrationConnection {
    fn client(&self) -> PyResult<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("migration connection is closed"))
    }

    /// Run `fut` to completion on the private runtime, GIL released.
    /// (`PgError` is large, but this is a cold, once-per-command path.)
    #[allow(clippy::result_large_err)]
    fn run<T: Send, F>(&self, py: Python<'_>, fut: F) -> Result<T, PgError>
    where
        F: std::future::Future<Output = Result<T, PgError>> + Send,
    {
        py.allow_threads(|| self.rt.block_on(fut))
    }
}

fn guard_runtime() -> PyResult<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(PyRuntimeError::new_err(
            "cello migrate runs synchronously and cannot be used from inside a running \
             server or async task; run it as a separate command",
        ));
    }
    Ok(())
}

#[pymethods]
impl PyMigrationConnection {
    #[new]
    fn new(py: Python<'_>, dsn: &str) -> PyResult<Self> {
        guard_runtime()?;
        // Same libpq `sslmode` handling as the request pool (`db::tls`).
        let crate::db::tls::PgTls {
            config, connector, ..
        } = crate::db::tls::configure(dsn, Default::default()).map_err(PyValueError::new_err)?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| PyRuntimeError::new_err(format!("runtime: {e}")))?;
        let (client, task) = py
            .allow_threads(|| {
                rt.block_on(async {
                    let (client, conn) = config.connect(connector).await?;
                    let task = tokio::spawn(async move {
                        let _ = conn.await;
                    });
                    Ok::<_, tokio_postgres::Error>((client, task))
                })
            })
            .map_err(|e| PgError::from_pg(&e).into_pyerr(py))?;
        Ok(Self {
            rt,
            client: Some(client),
            conn_task: Some(task),
        })
    }

    /// Run a SQL script (one or more statements, simple-query protocol).
    fn execute(&self, py: Python<'_>, sql: String) -> PyResult<()> {
        guard_runtime()?;
        let client = self.client()?;
        self.run(py, async {
            client
                .batch_execute(&sql)
                .await
                .map_err(|e| PgError::from_pg(&e))
        })
        .map_err(|e| e.into_pyerr(py))
    }

    /// Run one statement with text parameters; every result column must be
    /// `text` (cast in SQL). Returns rows as lists of `str | None`.
    #[pyo3(signature = (sql, params = Vec::new()))]
    fn query(
        &self,
        py: Python<'_>,
        sql: String,
        params: Vec<Option<String>>,
    ) -> PyResult<Vec<Vec<Option<String>>>> {
        guard_runtime()?;
        let client = self.client()?;
        self.run(py, async {
            let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
                .iter()
                .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();
            let rows = client
                .query(sql.as_str(), &refs)
                .await
                .map_err(|e| PgError::from_pg(&e))?;
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let mut r = Vec::with_capacity(row.len());
                for i in 0..row.len() {
                    r.push(
                        row.try_get::<_, Option<String>>(i)
                            .map_err(|e| PgError::from_pg(&e))?,
                    );
                }
                out.push(r);
            }
            Ok(out)
        })
        .map_err(|e| e.into_pyerr(py))
    }

    /// The live definition of `tables` (those that exist), as plain dicts.
    fn introspect(&self, py: Python<'_>, tables: Vec<String>) -> PyResult<PyObject> {
        guard_runtime()?;
        let client = self.client()?;
        let live = self
            .run(py, async {
                introspect(client, &tables)
                    .await
                    .map_err(|e| PgError::from_pg(&e))
            })
            .map_err(|e| e.into_pyerr(py))?;
        live_to_py(py, &live)
    }

    /// Diff `specs` against the live schema.
    ///
    /// Returns `(ops, warnings)`; each op is a dict with `kind`, `table`,
    /// `column`, `sql`, `destructive` and `note`.
    fn plan(
        &self,
        py: Python<'_>,
        specs: Vec<PyRef<'_, PyModelSpec>>,
        managed_before: Vec<String>,
    ) -> PyResult<(PyObject, Vec<String>)> {
        guard_runtime()?;
        let client = self.client()?;
        let owned: Vec<std::sync::Arc<ModelSpec>> = specs
            .iter()
            .map(|s| std::sync::Arc::clone(&s.inner))
            .collect();
        let refs: Vec<&ModelSpec> = owned.iter().map(|s| s.as_ref()).collect();
        let mut tables: Vec<String> = managed_before.clone();
        for s in &refs {
            tables.push(s.table.clone());
            for f in &s.fields {
                if let Some(t) = f.relation.as_ref().and_then(|r| r.target.as_ref()) {
                    tables.push(t.table.clone());
                }
            }
        }
        tables.sort();
        tables.dedup();
        let (canonical, live) = self
            .run(py, async {
                let live = introspect(client, &tables)
                    .await
                    .map_err(|e| PgError::from_pg(&e))?;
                let existing: Vec<&ModelSpec> = refs
                    .iter()
                    .copied()
                    .filter(|s| live.contains_key(&s.table))
                    .collect();
                let canonical = canonicalize(client, &existing).await?;
                Ok((canonical, live))
            })
            .map_err(|e| e.into_pyerr(py))?;
        let p = plan(&refs, &canonical, &live, &managed_before)?;
        let ops = PyList::empty(py);
        for o in &p.ops {
            let d = PyDict::new(py);
            d.set_item("kind", o.kind.as_str())?;
            d.set_item("table", &o.table)?;
            d.set_item("column", &o.column)?;
            d.set_item("sql", &o.sql)?;
            d.set_item("destructive", o.destructive)?;
            d.set_item("note", &o.note)?;
            ops.append(d)?;
        }
        Ok((ops.into(), p.warnings))
    }

    /// Close the session (releases any session-level advisory lock).
    fn close(&mut self, py: Python<'_>) {
        // Dropping the client asks the connection task to send Terminate; drive
        // it to completion so the server ends the session (and its locks) now.
        self.client = None;
        if let Some(task) = self.conn_task.take() {
            if tokio::runtime::Handle::try_current().is_err() {
                py.allow_threads(|| {
                    self.rt.block_on(async {
                        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
                    })
                });
            }
        }
    }

    #[getter]
    fn closed(&self) -> bool {
        self.client.is_none()
    }
}
