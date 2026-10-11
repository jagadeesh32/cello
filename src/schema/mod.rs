//! Schema registry: one compiled description per ORM model (v1.4.0 Phase 01).
//!
//! # Design
//!
//! A model is *declared* in Python (`class User(Model): ...` in
//! `python/cello/orm.py`) and *compiled* here, once, when the class statement
//! runs. The metaclass hands every field to [`python::schema_register`], which
//! builds a [`ModelSpec`] and stores it in a process-wide registry keyed by a
//! [`ModelId`]. After that the spec is immutable; every consumer reads it.
//!
//! ```text
//!   class User(Model)          ModelMeta.__new__            src/schema
//!   ─────────────────  ──────▶ _field_descriptor(f) × N ──▶ schema_register()
//!   (Python DX, stays)          (plain dicts, cold path)      │
//!                                                             ▼
//!                                         REGISTRY: DashMap<ModelId, Arc<ModelSpec>>
//!                                                             │
//!            ┌──────────────────────┬─────────────────────────┼───────────────────────┐
//!            ▼                      ▼                         ▼                       ▼
//!   identifier resolution     DDL (create/drop)     Phase 02: query compiler   Phase 03: validation,
//!   (lookup dict, Phase 01)   (Phase 01)            + row → JSON fast path     serialization, OpenAPI
//! ```
//!
//! ## What a spec holds
//!
//! * [`ModelSpec`]: class name, table (raw and pre-quoted), fields in the exact
//!   order the metaclass produces (own fields first, then inherited ones), the
//!   primary key, and a private identifier map used for resolution.
//! * [`FieldSpec`]: attribute name, column (`<name>_id` for a ForeignKey, or
//!   `db_column`) raw and pre-quoted, typed [`PgType`] (custom `sql_type` strings
//!   are kept verbatim as [`PgType::Custom`]), nullability, uniqueness,
//!   `max_length`, a typed [`DefaultSpec`], `auto` (database-generated),
//!   `auto_now_add`, read/write visibility (`write_only=True` clears
//!   `readable`, `read_only=True` clears `writable`; Phase 03), and a
//!   [`RelationSpec`] with the target table,
//!   target primary-key column, target [`ModelId`] and a closed [`OnDelete`].
//!
//! Specs are plain Rust data: `Send + Sync`, no Python objects, so Phase 02 can
//! read them from Tokio tasks without the GIL. [`PgType::wire_type`] gives the
//! `tokio_postgres` type a column decodes as, for the row pipeline.
//!
//! ## Identity and lifetime
//!
//! A [`ModelId`] comes from a process-wide counter and is never reused, unlike
//! a Python `id()`. The Python class keeps the owning `ModelSpec` handle on
//! `cls._meta.schema`; when the class is garbage collected (models defined
//! inside functions or tests) the handle's `Drop` removes the registry entry,
//! so redefining classes in a loop does not grow the registry. No registry lock
//! is held while Python code runs.
//!
//! ## Identifier resolution (security + speed)
//!
//! Query identifiers (`filter` / `exclude` / `order_by` / `update` keys) are
//! validated against `^[A-Za-z_][A-Za-z0-9_]*$` and resolved here, never in
//! Python string code. Because the per-query path must cost no more than the
//! 1.3.x Python dict lookup, the registry hands Python a dict at class creation
//! (`ModelSpec.lookup()`): every *valid* identifier that names a declared field
//! or column, mapped to its already-quoted column. A hit is one dict lookup.
//! Anything else falls through to [`ModelSpec::resolve`], which rejects
//! non-identifiers and quotes undeclared ones (deprecated; Python emits the
//! warning so it is attributed to the caller's line).
//!
//! ## DDL
//!
//! `CREATE TABLE` / `DROP TABLE` text, identifier quoting and type/default
//! rendering are produced here. Output is byte-for-byte what 1.3.x produced,
//! including pinned quirks (a dict default renders Python's `str()` of it).
//!
//! ## Dialects
//!
//! Only Postgres exists. Type and literal rendering are isolated in
//! [`PgType`] / [`Literal`] so a second dialect can be added beside them later.

pub mod ident;
pub mod migrate;
pub mod migrate_conn;
pub mod native;
pub mod python;
pub mod runtime;
pub mod spec;
pub mod validate;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;

pub use ident::{is_identifier, quote_ident};
pub use python::{PyFieldSpec, PyModelSpec};
pub use spec::{
    DefaultSpec, FieldSpec, Literal, ModelId, ModelSpec, OnDelete, OutputField, PgType,
    RelationSpec, RelationTarget, Resolved, SchemaError,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn registry() -> &'static DashMap<ModelId, Arc<ModelSpec>> {
    static REGISTRY: OnceLock<DashMap<ModelId, Arc<ModelSpec>>> = OnceLock::new();
    REGISTRY.get_or_init(DashMap::new)
}

/// Allocate a fresh, never-reused model id.
pub fn next_id() -> ModelId {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Store a compiled spec. Replaces any spec with the same id.
pub fn register(spec: ModelSpec) -> Arc<ModelSpec> {
    let spec = Arc::new(spec);
    registry().insert(spec.id, Arc::clone(&spec));
    spec
}

/// Look up a registered model.
pub fn get(id: ModelId) -> Option<Arc<ModelSpec>> {
    registry().get(&id).map(|e| Arc::clone(e.value()))
}

/// Remove a model (its Python class was garbage collected).
pub fn unregister(id: ModelId) {
    registry().remove(&id);
}

/// Number of live registered models.
pub fn len() -> usize {
    registry().len()
}

/// `true` when no models are registered.
pub fn is_empty() -> bool {
    registry().is_empty()
}
