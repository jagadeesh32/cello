//! Schema diff for `cello migrate` (v1.4.0 Phase 05, **experimental**).
//!
//! Pure data in, ordered SQL operations out. No I/O happens here; the live
//! schema and the canonical form of the registry are gathered by
//! [`super::migrate_conn`] and handed in as plain values, so every decision
//! this module makes is unit-testable without a database.
//!
//! # Inputs
//!
//! * the registered [`ModelSpec`]s (the target state);
//! * for each target table, the **canonical** column type and default
//!   expression: the text Postgres itself produces (`format_type`,
//!   `pg_get_expr`) for the declared DDL, obtained from a rolled-back temporary
//!   table. Comparing Postgres' rendering with Postgres' rendering means no
//!   hand-written type alias table can drift from the server;
//! * the live tables ([`LiveTable`]), read from `pg_catalog`;
//! * the tables earlier migrations managed (for `DROP TABLE` detection).
//!
//! # Safety rules
//!
//! * Operations that can lose data (`DROP TABLE`, `DROP COLUMN`, type changes
//!   without a lossless assignment cast) are marked `destructive`; the caller
//!   decides whether they are emitted live or commented out.
//! * Renames are never guessed. A table that both loses and gains columns gets
//!   a warning that it may be a rename, and the drop stays destructive.
//! * Anything the diff does not model (multi-column constraints, primary-key
//!   changes, identity / generated columns, serial conversions) produces a
//!   warning and **no** SQL.
//! * Every identifier is quoted with [`quote_ident`]; constraint names come
//!   from the live catalog and are quoted the same way. Types and defaults are
//!   the developer's declared DDL (exactly what `create_table` emits) or
//!   Postgres' own `format_type` text.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use thiserror::Error;

use super::ident::quote_ident;
use super::spec::{FieldSpec, ModelSpec, OnDelete, SchemaError};

/// Errors that stop a plan from being produced.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MigrateError {
    #[error(transparent)]
    Schema(#[from] SchemaError),

    #[error("models {first} and {second} both use table {table:?}")]
    DuplicateTable {
        table: String,
        first: String,
        second: String,
    },

    #[error("model {model} maps two fields to column {column:?}")]
    DuplicateColumn { model: String, column: String },

    #[error("internal: no canonical type for {table}.{column}")]
    MissingCanonical { table: String, column: String },

    #[error("field {field:?} of {model}: ForeignKey target {table:?} declares no primary key")]
    TargetWithoutPk {
        model: String,
        field: String,
        table: String,
    },
}

// ── Live schema ───────────────────────────────────────────────────────────────

/// One column as `pg_catalog` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveColumn {
    pub name: String,
    /// `format_type(atttypid, atttypmod)`, e.g. `character varying(100)`.
    pub data_type: String,
    pub not_null: bool,
    /// `pg_get_expr(adbin, adrelid)` for a plain default.
    pub default: Option<String>,
    /// `GENERATED ... AS IDENTITY`.
    pub identity: bool,
    /// `GENERATED ALWAYS AS (...) STORED`.
    pub generated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConKind {
    Primary,
    Unique,
    Foreign,
}

/// One `PRIMARY KEY` / `UNIQUE` / `FOREIGN KEY` constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveConstraint {
    pub name: String,
    pub kind: ConKind,
    pub columns: Vec<String>,
    /// Referenced table (foreign keys).
    pub ref_table: Option<String>,
    /// The referenced table lives in the schema being migrated.
    pub ref_same_schema: bool,
    pub ref_columns: Vec<String>,
    /// `None` for an action this module does not model.
    pub on_delete: Option<OnDelete>,
    /// `ON UPDATE NO ACTION` (what the ORM always creates).
    pub on_update_no_action: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveTable {
    pub name: String,
    pub columns: Vec<LiveColumn>,
    pub constraints: Vec<LiveConstraint>,
}

impl LiveTable {
    pub fn column(&self, name: &str) -> Option<&LiveColumn> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Single-column constraints of `kind` on `column`.
    fn single<'a>(
        &'a self,
        kind: ConKind,
        column: &'a str,
    ) -> impl Iterator<Item = &'a LiveConstraint> + 'a {
        self.constraints
            .iter()
            .filter(move |c| c.kind == kind && c.columns.len() == 1 && c.columns[0] == column)
    }

    fn primary_key(&self) -> Option<&LiveConstraint> {
        self.constraints.iter().find(|c| c.kind == ConKind::Primary)
    }
}

/// Postgres' rendering of a declared column (from the shadow table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canonical {
    pub data_type: String,
    pub default: Option<String>,
}

/// Column name -> canonical form, for one target table.
pub type CanonicalTable = HashMap<String, Canonical>;

// ── Operations ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    CreateTable,
    DropTable,
    AddColumn,
    DropColumn,
    AlterType,
    SetNotNull,
    DropNotNull,
    SetDefault,
    DropDefault,
    AddUnique,
    DropUnique,
    AddForeignKey,
    DropForeignKey,
}

impl OpKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OpKind::CreateTable => "create_table",
            OpKind::DropTable => "drop_table",
            OpKind::AddColumn => "add_column",
            OpKind::DropColumn => "drop_column",
            OpKind::AlterType => "alter_type",
            OpKind::SetNotNull => "set_not_null",
            OpKind::DropNotNull => "drop_not_null",
            OpKind::SetDefault => "set_default",
            OpKind::DropDefault => "drop_default",
            OpKind::AddUnique => "add_unique",
            OpKind::DropUnique => "drop_unique",
            OpKind::AddForeignKey => "add_foreign_key",
            OpKind::DropForeignKey => "drop_foreign_key",
        }
    }
}

/// One SQL statement of a migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub kind: OpKind,
    pub table: String,
    pub column: Option<String>,
    /// A single statement, no trailing `;`, no newlines.
    pub sql: String,
    /// May lose data: emitted only with `--allow-destructive`.
    pub destructive: bool,
    /// A human note for the reviewer (why it may fail, why it is flagged).
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub ops: Vec<Op>,
    pub warnings: Vec<String>,
}

// ── Type-change classification ────────────────────────────────────────────────

/// How a column type change converts existing values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastSafety {
    /// Lossless and cannot fail (`integer` -> `bigint`, `varchar(n)` -> `text`).
    Safe,
    /// Lossless when it succeeds; the assignment cast raises (aborting the
    /// migration's transaction) instead of truncating (`bigint` -> `integer`,
    /// `text` -> `varchar(n)`).
    Checked,
    /// No lossless assignment cast: needs `USING` and review. Destructive.
    Flagged,
}

fn int_rank(t: &str) -> Option<u8> {
    match t {
        "smallint" => Some(1),
        "integer" => Some(2),
        "bigint" => Some(3),
        _ => None,
    }
}

/// `Some(None)` for `text` / unbounded `character varying`, `Some(Some(n))` for
/// `character varying(n)`, `None` for anything else.
fn string_bound(t: &str) -> Option<Option<u32>> {
    match t {
        "text" | "character varying" => Some(None),
        _ => t
            .strip_prefix("character varying(")
            .and_then(|r| r.strip_suffix(')'))
            .and_then(|n| n.parse().ok())
            .map(Some),
    }
}

/// Classify `old` -> `new` (both `format_type` text).
pub fn classify_type_change(old: &str, new: &str) -> CastSafety {
    if old == new {
        return CastSafety::Safe;
    }
    if let (Some(a), Some(b)) = (int_rank(old), int_rank(new)) {
        return if b > a {
            CastSafety::Safe
        } else {
            CastSafety::Checked
        };
    }
    if matches!(old, "smallint" | "integer") && matches!(new, "double precision" | "numeric") {
        return CastSafety::Safe;
    }
    if old == "bigint" && new == "numeric" {
        return CastSafety::Safe;
    }
    match (string_bound(old), string_bound(new)) {
        // string -> string: widening is safe, narrowing raises on long values.
        (Some(_), Some(None)) => CastSafety::Safe,
        (Some(Some(a)), Some(Some(b))) if b >= a => CastSafety::Safe,
        (Some(_), Some(Some(_))) => CastSafety::Checked,
        // anything -> string: the assignment I/O cast always exists; it changes
        // the representation, and a bounded target raises when too long.
        (None, Some(_)) => CastSafety::Checked,
        _ => CastSafety::Flagged,
    }
}

// ── Planning ──────────────────────────────────────────────────────────────────

fn is_sequence_default(d: Option<&str>) -> bool {
    d.is_some_and(|d| d.starts_with("nextval("))
}

/// The field's declared type DDL (`create_table` would fail without one).
fn declared_type(spec: &ModelSpec, f: &FieldSpec) -> Result<String, MigrateError> {
    f.pg_type
        .as_ref()
        .map(|t| t.ddl().into_owned())
        .ok_or_else(|| {
            SchemaError::UnknownType {
                model: spec.name.clone(),
                field: f.name.clone(),
            }
            .into()
        })
}

/// `(table, pk_column, on_delete)` the field references, if it is a ForeignKey.
fn fk_target<'a>(
    spec: &ModelSpec,
    f: &'a FieldSpec,
) -> Result<Option<(&'a str, &'a str, OnDelete)>, MigrateError> {
    let Some(rel) = &f.relation else {
        return Ok(None);
    };
    let target = rel
        .target
        .as_ref()
        .ok_or_else(|| SchemaError::UnresolvedRelation {
            model: spec.name.clone(),
            field: f.name.clone(),
        })?;
    let pk = target
        .pk_column
        .as_deref()
        .ok_or_else(|| MigrateError::TargetWithoutPk {
            model: spec.name.clone(),
            field: f.name.clone(),
            table: target.table.clone(),
        })?;
    Ok(Some((target.table.as_str(), pk, rel.on_delete)))
}

fn add_fk_sql(table: &str, column: &str, ref_table: &str, ref_col: &str, on: OnDelete) -> String {
    format!(
        "ALTER TABLE {} ADD FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {}",
        quote_ident(table),
        quote_ident(column),
        quote_ident(ref_table),
        quote_ident(ref_col),
        on.as_sql()
    )
}

fn drop_constraint_sql(table: &str, name: &str) -> String {
    format!(
        "ALTER TABLE {} DROP CONSTRAINT {}",
        quote_ident(table),
        quote_ident(name)
    )
}

fn alter_column(table: &str, column: &str, action: &str) -> String {
    format!(
        "ALTER TABLE {} ALTER COLUMN {} {}",
        quote_ident(table),
        quote_ident(column),
        action
    )
}

/// `CREATE TABLE` for `spec`, with the ForeignKeys at `deferred` field indices
/// left out (added later with `ALTER TABLE ... ADD FOREIGN KEY`). With nothing
/// deferred this is exactly `create_table_sql(false)`.
fn create_sql(spec: &ModelSpec, deferred: &BTreeSet<usize>) -> Result<String, MigrateError> {
    if deferred.is_empty() {
        return Ok(spec.create_table_sql(false)?);
    }
    let cols = spec
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if deferred.contains(&i) {
                let mut plain = f.clone();
                plain.relation = None;
                plain.column_ddl(&spec.name)
            } else {
                f.column_ddl(&spec.name)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!(
        "CREATE TABLE {} ({})",
        spec.quoted_table,
        cols.join(", ")
    ))
}

/// Builder that keeps the statement groups in their execution order.
#[derive(Default)]
struct Buckets {
    drop_fk: Vec<Op>,
    drop_unique: Vec<Op>,
    create: Vec<Op>,
    alter: Vec<Op>,
    add_column: Vec<Op>,
    add_unique: Vec<Op>,
    add_fk: Vec<Op>,
    drop_column: Vec<Op>,
    drop_table: Vec<Op>,
}

impl Buckets {
    fn into_ops(self) -> Vec<Op> {
        [
            self.drop_fk,
            self.drop_unique,
            self.create,
            self.alter,
            self.add_column,
            self.add_unique,
            self.add_fk,
            self.drop_column,
            self.drop_table,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

fn op(kind: OpKind, table: &str, column: Option<&str>, sql: String) -> Op {
    Op {
        kind,
        table: table.to_string(),
        column: column.map(str::to_string),
        sql,
        destructive: false,
        note: None,
    }
}

impl Op {
    fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Diff the registry against the live schema.
///
/// `specs` are the target models; `canonical` holds, per target table, the
/// shadow-table rendering of every column; `live` holds every live table that
/// is a target, previously managed, or referenced by a ForeignKey;
/// `managed_before` lists the tables earlier migrations owned.
pub fn plan(
    specs: &[&ModelSpec],
    canonical: &HashMap<String, CanonicalTable>,
    live: &HashMap<String, LiveTable>,
    managed_before: &[String],
) -> Result<Plan, MigrateError> {
    // Deterministic order, duplicate tables / columns rejected.
    let mut by_table: BTreeMap<&str, &ModelSpec> = BTreeMap::new();
    for spec in specs {
        if let Some(prev) = by_table.insert(spec.table.as_str(), spec) {
            return Err(MigrateError::DuplicateTable {
                table: spec.table.clone(),
                first: prev.name.clone(),
                second: spec.name.clone(),
            });
        }
        let mut seen = HashSet::new();
        for f in &spec.fields {
            if !seen.insert(f.column.as_str()) {
                return Err(MigrateError::DuplicateColumn {
                    model: spec.name.clone(),
                    column: f.column.clone(),
                });
            }
            declared_type(spec, f)?;
            fk_target(spec, f)?;
        }
    }

    let mut b = Buckets::default();
    let mut warnings = Vec::new();

    // Referenced tables that neither exist nor will be created.
    for spec in by_table.values() {
        for f in &spec.fields {
            if let Some((t, _, _)) = fk_target(spec, f)? {
                if !by_table.contains_key(t) && !live.contains_key(t) {
                    warnings.push(format!(
                        "{}.{} references table {:?}, which does not exist and is not one of \
                         the migrated models; applying will fail",
                        spec.table, f.column, t
                    ));
                }
            }
        }
    }

    // ── New tables, ordered so ForeignKey targets come first ─────────────────
    let new_tables: Vec<&ModelSpec> = by_table
        .values()
        .copied()
        .filter(|s| !live.contains_key(&s.table))
        .collect();
    let new_names: HashSet<&str> = new_tables.iter().map(|s| s.table.as_str()).collect();
    let mut created: HashSet<&str> = HashSet::new();
    let mut remaining = new_tables;
    while !remaining.is_empty() {
        // Pending dependencies of a table: other new tables not yet created.
        let deps = |s: &ModelSpec| -> Result<Vec<usize>, MigrateError> {
            let mut out = Vec::new();
            for (i, f) in s.fields.iter().enumerate() {
                if let Some((t, _, _)) = fk_target(s, f)? {
                    if t != s.table && new_names.contains(t) && !created.contains(t) {
                        out.push(i);
                    }
                }
            }
            Ok(out)
        };
        let mut pick = None;
        for (idx, s) in remaining.iter().enumerate() {
            if deps(s)?.is_empty() {
                pick = Some((idx, BTreeSet::new()));
                break;
            }
        }
        // A cycle: create the first table without its pending ForeignKeys.
        let (idx, deferred) = match pick {
            Some(p) => p,
            None => (0, deps(remaining[0])?.into_iter().collect()),
        };
        let spec = remaining.remove(idx);
        b.create.push(op(
            OpKind::CreateTable,
            &spec.table,
            None,
            create_sql(spec, &deferred)?,
        ));
        for &i in &deferred {
            let f = &spec.fields[i];
            let (t, pk, on) = fk_target(spec, f)?.expect("deferred fields are ForeignKeys");
            b.add_fk.push(
                op(
                    OpKind::AddForeignKey,
                    &spec.table,
                    Some(&f.column),
                    add_fk_sql(&spec.table, &f.column, t, pk, on),
                )
                .note("deferred: the tables reference each other"),
            );
        }
        created.insert(spec.table.as_str());
    }

    // ── Existing tables ───────────────────────────────────────────────────────
    for spec in by_table.values() {
        let Some(lt) = live.get(&spec.table) else {
            continue;
        };
        let canon = canonical.get(&spec.table);
        diff_table(spec, lt, canon, &mut b, &mut warnings)?;
    }

    // ── Tables no longer declared ─────────────────────────────────────────────
    let dropped: Vec<&LiveTable> = managed_before
        .iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|t| !by_table.contains_key(t.as_str()))
        .filter_map(|t| live.get(t))
        .collect();
    plan_drops(&dropped, &mut b);

    Ok(Plan {
        ops: b.into_ops(),
        warnings,
    })
}

/// `DROP TABLE` for tables no model declares any more: tables referenced by
/// another dropped table go last; a reference cycle is dropped in one
/// statement. No `CASCADE`: a table some unmanaged table still references makes
/// the migration fail instead of silently dropping that constraint.
fn plan_drops(dropped: &[&LiveTable], b: &mut Buckets) {
    let names: HashSet<&str> = dropped.iter().map(|t| t.name.as_str()).collect();
    let mut left: Vec<&LiveTable> = dropped.to_vec();
    let note = "drops the table and all of its rows";
    while !left.is_empty() {
        // A table is safe to drop when no other remaining table references it.
        let referenced = |name: &str, left: &[&LiveTable]| {
            left.iter().any(|o| {
                o.name != name
                    && o.constraints.iter().any(|c| {
                        c.kind == ConKind::Foreign
                            && c.ref_same_schema
                            && c.ref_table.as_deref() == Some(name)
                    })
            })
        };
        match left.iter().position(|t| !referenced(&t.name, &left)) {
            Some(i) => {
                let t = left.remove(i);
                b.drop_table.push(
                    op(
                        OpKind::DropTable,
                        &t.name,
                        None,
                        format!("DROP TABLE {}", quote_ident(&t.name)),
                    )
                    .destructive()
                    .note(note),
                );
            }
            None => {
                let list: Vec<String> = left.iter().map(|t| quote_ident(&t.name)).collect();
                let tables: Vec<&str> = left.iter().map(|t| t.name.as_str()).collect();
                debug_assert!(tables.iter().all(|t| names.contains(t)));
                b.drop_table.push(
                    op(
                        OpKind::DropTable,
                        &tables.join(", "),
                        None,
                        format!("DROP TABLE {}", list.join(", ")),
                    )
                    .destructive()
                    .note(note),
                );
                left.clear();
            }
        }
    }
}

fn diff_table(
    spec: &ModelSpec,
    lt: &LiveTable,
    canon: Option<&CanonicalTable>,
    b: &mut Buckets,
    warnings: &mut Vec<String>,
) -> Result<(), MigrateError> {
    let table = spec.table.as_str();
    let mut added = Vec::new();

    // Primary key: reported, never changed.
    let model_pk: Vec<String> = spec
        .pk_field()
        .map(|f| f.column.clone())
        .into_iter()
        .collect();
    let live_pk: Vec<String> = lt
        .primary_key()
        .map(|c| c.columns.clone())
        .unwrap_or_default();
    if model_pk != live_pk {
        warnings.push(format!(
            "{table}: primary key differs (live {live_pk:?}, model {model_pk:?}); primary keys \
             are not changed automatically"
        ));
    }
    for c in &lt.constraints {
        if matches!(c.kind, ConKind::Unique | ConKind::Foreign) && c.columns.len() != 1 {
            warnings.push(format!(
                "{table}: multi-column constraint {:?} is not managed and was left unchanged",
                c.name
            ));
        }
    }

    for f in &spec.fields {
        let col = f.column.as_str();
        let Some(lc) = lt.column(col) else {
            let ddl = f.column_ddl(&spec.name)?;
            let mut o = op(
                OpKind::AddColumn,
                table,
                Some(col),
                format!("ALTER TABLE {} ADD COLUMN {}", spec.quoted_table, ddl),
            );
            if f.primary_key {
                o = o.note("adds a primary key; fails if the table already has one");
            } else if !f.nullable && f.default.to_sql().is_none() && !f.auto {
                o = o.note("NOT NULL without a default: fails if the table already has rows");
            }
            b.add_column.push(o);
            added.push(col.to_string());
            continue;
        };
        if lc.generated {
            warnings.push(format!(
                "{table}.{col}: live column is GENERATED; not managed, left unchanged"
            ));
            continue;
        }
        let want =
            canon
                .and_then(|c| c.get(col))
                .ok_or_else(|| MigrateError::MissingCanonical {
                    table: table.to_string(),
                    column: col.to_string(),
                })?;

        let want_auto = is_sequence_default(want.default.as_deref());
        let live_auto = lc.identity || is_sequence_default(lc.default.as_deref());
        let manage_default = want_auto == live_auto && !want_auto;
        if want_auto && !live_auto {
            warnings.push(format!(
                "{table}.{col}: the model declares an auto-increment column but the live column \
                 has no sequence; converting is not automated, left unchanged"
            ));
        } else if live_auto && !want_auto {
            warnings.push(format!(
                "{table}.{col}: the live column has a sequence/identity default the model does \
                 not declare; left unchanged"
            ));
        }

        // Type.
        let type_change = lc.data_type != want.data_type;
        if type_change {
            let safety = classify_type_change(&lc.data_type, &want.data_type);
            let type_flagged = safety == CastSafety::Flagged;
            let qc = &f.quoted_column;
            // A plain default may not cast to the new type: drop it first and
            // set the declared one afterwards.
            if manage_default && lc.default.is_some() {
                let mut d = op(
                    OpKind::DropDefault,
                    table,
                    Some(col),
                    alter_column(table, col, "DROP DEFAULT"),
                )
                .note("cleared before the type change");
                if type_flagged {
                    d = d.destructive();
                }
                b.alter.push(d);
            }
            let (action, note) = match safety {
                CastSafety::Safe => (
                    format!("TYPE {}", want.data_type),
                    format!("{} -> {}: lossless", lc.data_type, want.data_type),
                ),
                CastSafety::Checked => (
                    format!("TYPE {}", want.data_type),
                    format!(
                        "{} -> {}: fails (and rolls back) if an existing value does not fit",
                        lc.data_type, want.data_type
                    ),
                ),
                CastSafety::Flagged => (
                    format!("TYPE {} USING {}::{}", want.data_type, qc, want.data_type),
                    format!(
                        "{} -> {}: no lossless conversion; review the USING cast",
                        lc.data_type, want.data_type
                    ),
                ),
            };
            let mut t = op(
                OpKind::AlterType,
                table,
                Some(col),
                alter_column(table, col, &action),
            )
            .note(note);
            if type_flagged {
                t = t.destructive();
            }
            b.alter.push(t);
            if manage_default {
                if let Some(d) = f.default.to_sql() {
                    let mut s = op(
                        OpKind::SetDefault,
                        table,
                        Some(col),
                        alter_column(table, col, &format!("SET DEFAULT {d}")),
                    );
                    if type_flagged {
                        s = s.destructive();
                    }
                    b.alter.push(s);
                }
            }
        } else if manage_default && want.default != lc.default {
            b.alter.push(match f.default.to_sql() {
                Some(d) => op(
                    OpKind::SetDefault,
                    table,
                    Some(col),
                    alter_column(table, col, &format!("SET DEFAULT {d}")),
                ),
                None => op(
                    OpKind::DropDefault,
                    table,
                    Some(col),
                    alter_column(table, col, "DROP DEFAULT"),
                ),
            });
        }

        // Nullability (a primary key is NOT NULL by definition).
        if !f.primary_key && !live_pk.iter().any(|p| p == col) {
            let want_not_null = !f.nullable;
            if want_not_null && !lc.not_null {
                b.alter.push(
                    op(
                        OpKind::SetNotNull,
                        table,
                        Some(col),
                        alter_column(table, col, "SET NOT NULL"),
                    )
                    .note("fails (and rolls back) if existing rows hold NULL"),
                );
            } else if !want_not_null && lc.not_null {
                b.alter.push(op(
                    OpKind::DropNotNull,
                    table,
                    Some(col),
                    alter_column(table, col, "DROP NOT NULL"),
                ));
            }
        }

        // Single-column UNIQUE.
        let want_unique = f.unique && !f.primary_key;
        let live_unique: Vec<&LiveConstraint> = lt.single(ConKind::Unique, col).collect();
        if want_unique && live_unique.is_empty() {
            b.add_unique.push(
                op(
                    OpKind::AddUnique,
                    table,
                    Some(col),
                    format!(
                        "ALTER TABLE {} ADD UNIQUE ({})",
                        spec.quoted_table, f.quoted_column
                    ),
                )
                .note("fails (and rolls back) if existing rows hold duplicates"),
            );
        } else if !want_unique {
            for c in live_unique {
                b.drop_unique.push(op(
                    OpKind::DropUnique,
                    table,
                    Some(col),
                    drop_constraint_sql(table, &c.name),
                ));
            }
        }

        // Single-column FOREIGN KEY.
        let want_fk = fk_target(spec, f)?;
        let live_fks: Vec<&LiveConstraint> = lt.single(ConKind::Foreign, col).collect();
        let matches = |c: &LiveConstraint| match want_fk {
            Some((t, pk, on)) => {
                c.ref_same_schema
                    && c.ref_table.as_deref() == Some(t)
                    && c.ref_columns.len() == 1
                    && c.ref_columns[0] == pk
                    && c.on_delete == Some(on)
                    && c.on_update_no_action
            }
            None => false,
        };
        let keep = live_fks.iter().position(|c| matches(c));
        for (i, c) in live_fks.iter().enumerate() {
            if Some(i) != keep {
                b.drop_fk.push(op(
                    OpKind::DropForeignKey,
                    table,
                    Some(col),
                    drop_constraint_sql(table, &c.name),
                ));
            }
        }
        if let (Some((t, pk, on)), None) = (want_fk, keep) {
            b.add_fk.push(
                op(
                    OpKind::AddForeignKey,
                    table,
                    Some(col),
                    add_fk_sql(table, col, t, pk, on),
                )
                .note("fails (and rolls back) if existing rows reference missing keys"),
            );
        }
    }

    // Live columns no field declares.
    let declared: HashSet<&str> = spec.fields.iter().map(|f| f.column.as_str()).collect();
    let mut removed = Vec::new();
    for lc in &lt.columns {
        if declared.contains(lc.name.as_str()) {
            continue;
        }
        b.drop_column.push(
            op(
                OpKind::DropColumn,
                table,
                Some(&lc.name),
                format!(
                    "ALTER TABLE {} DROP COLUMN {}",
                    spec.quoted_table,
                    quote_ident(&lc.name)
                ),
            )
            .destructive()
            .note("drops the column and its data"),
        );
        removed.push(lc.name.clone());
    }
    if !added.is_empty() && !removed.is_empty() {
        warnings.push(format!(
            "{table}: columns {removed:?} are dropped and {added:?} added. Renames are NOT \
             detected: if this is a rename, replace the DROP/ADD pair with ALTER TABLE ... \
             RENAME COLUMN, or the data in the dropped column is lost"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::spec::{DefaultSpec, Literal, PgType, RelationSpec, RelationTarget};

    fn field(name: &str, ty: &str) -> FieldSpec {
        FieldSpec {
            name: name.into(),
            column: name.into(),
            quoted_column: quote_ident(name),
            pg_type: Some(PgType::parse(ty)),
            primary_key: false,
            nullable: true,
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

    fn pk() -> FieldSpec {
        let mut f = field("id", "SERIAL");
        f.primary_key = true;
        f.nullable = false;
        f.auto = true;
        f
    }

    fn fk(name: &str, table: &str) -> FieldSpec {
        let mut f = field(name, "INTEGER");
        f.column = format!("{name}_id");
        f.quoted_column = quote_ident(&f.column);
        f.relation = Some(RelationSpec {
            target: Some(RelationTarget {
                model: None,
                table: table.into(),
                pk_column: Some("id".into()),
            }),
            on_delete: OnDelete::Cascade,
        });
        f
    }

    fn model(table: &str, fields: Vec<FieldSpec>) -> ModelSpec {
        ModelSpec::new(1, table.to_uppercase(), table.into(), fields)
    }

    fn canon(spec: &ModelSpec, types: &[(&str, &str, Option<&str>)]) -> CanonicalTable {
        assert_eq!(spec.fields.len(), types.len());
        types
            .iter()
            .map(|(c, t, d)| {
                (
                    c.to_string(),
                    Canonical {
                        data_type: t.to_string(),
                        default: d.map(str::to_string),
                    },
                )
            })
            .collect()
    }

    fn live_col(name: &str, ty: &str, not_null: bool, default: Option<&str>) -> LiveColumn {
        LiveColumn {
            name: name.into(),
            data_type: ty.into(),
            not_null,
            default: default.map(str::to_string),
            identity: false,
            generated: false,
        }
    }

    fn pk_con(table: &str) -> LiveConstraint {
        LiveConstraint {
            name: format!("{table}_pkey"),
            kind: ConKind::Primary,
            columns: vec!["id".into()],
            ref_table: None,
            ref_same_schema: true,
            ref_columns: vec![],
            on_delete: None,
            on_update_no_action: true,
        }
    }

    const SEQ: &str = "nextval('t_id_seq'::regclass)";

    #[test]
    fn classify() {
        use CastSafety::*;
        assert_eq!(classify_type_change("integer", "bigint"), Safe);
        assert_eq!(classify_type_change("bigint", "integer"), Checked);
        assert_eq!(classify_type_change("character varying(10)", "text"), Safe);
        assert_eq!(
            classify_type_change("character varying(10)", "character varying(20)"),
            Safe
        );
        assert_eq!(
            classify_type_change("character varying(20)", "character varying(10)"),
            Checked
        );
        assert_eq!(
            classify_type_change("text", "character varying(5)"),
            Checked
        );
        assert_eq!(classify_type_change("integer", "text"), Checked);
        assert_eq!(classify_type_change("text", "integer"), Flagged);
        assert_eq!(classify_type_change("double precision", "integer"), Flagged);
        assert_eq!(classify_type_change("bigint", "double precision"), Flagged);
        assert_eq!(classify_type_change("text", "jsonb"), Flagged);
        assert_eq!(classify_type_change("integer", "double precision"), Safe);
    }

    #[test]
    fn creates_in_dependency_order() {
        let book = model("book", vec![pk(), fk("author", "author")]);
        let author = model("author", vec![pk(), field("name", "TEXT")]);
        let p = plan(&[&book, &author], &HashMap::new(), &HashMap::new(), &[]).unwrap();
        let tables: Vec<_> = p.ops.iter().map(|o| o.table.as_str()).collect();
        assert_eq!(tables, ["author", "book"]);
        assert_eq!(p.ops[1].sql, book.create_table_sql(false).unwrap());
        assert!(p.ops.iter().all(|o| !o.destructive));
    }

    #[test]
    fn cycles_defer_foreign_keys() {
        let a = model("a", vec![pk(), fk("b", "b")]);
        let b = model("b", vec![pk(), fk("a", "a")]);
        let p = plan(&[&a, &b], &HashMap::new(), &HashMap::new(), &[]).unwrap();
        let kinds: Vec<_> = p.ops.iter().map(|o| o.kind).collect();
        assert_eq!(
            kinds,
            [
                OpKind::CreateTable,
                OpKind::CreateTable,
                OpKind::AddForeignKey
            ]
        );
        assert!(!p.ops[0].sql.contains("REFERENCES"));
        assert!(p.ops[1].sql.contains("REFERENCES \"a\""));
        assert_eq!(
            p.ops[2].sql,
            "ALTER TABLE \"a\" ADD FOREIGN KEY (\"b_id\") REFERENCES \"b\"(\"id\") ON DELETE CASCADE"
        );
    }

    #[test]
    fn column_changes() {
        let mut n = field("n", "BIGINT");
        n.nullable = false;
        let mut flag = field("flag", "BOOLEAN");
        flag.default = DefaultSpec::Literal(Literal::Bool(true));
        let mut code = field("code", "VARCHAR(5)");
        code.unique = true;
        let spec = model("t", vec![pk(), n, flag, code, field("new", "TEXT")]);
        let c = canon(
            &spec,
            &[
                ("id", "integer", Some(SEQ)),
                ("n", "bigint", None),
                ("flag", "boolean", Some("true")),
                ("code", "character varying(5)", None),
                ("new", "text", None),
            ],
        );
        let live = LiveTable {
            name: "t".into(),
            columns: vec![
                live_col("id", "integer", true, Some(SEQ)),
                live_col("n", "integer", false, None),
                live_col("flag", "boolean", false, None),
                live_col("code", "character varying(5)", false, None),
                live_col("old", "text", false, None),
            ],
            constraints: vec![pk_con("t")],
        };
        let p = plan(
            &[&spec],
            &HashMap::from([("t".into(), c)]),
            &HashMap::from([("t".into(), live)]),
            &["t".into()],
        )
        .unwrap();
        let sql: Vec<_> = p.ops.iter().map(|o| o.sql.as_str()).collect();
        assert_eq!(
            sql,
            [
                "ALTER TABLE \"t\" ALTER COLUMN \"n\" TYPE bigint",
                "ALTER TABLE \"t\" ALTER COLUMN \"n\" SET NOT NULL",
                "ALTER TABLE \"t\" ALTER COLUMN \"flag\" SET DEFAULT TRUE",
                "ALTER TABLE \"t\" ADD COLUMN \"new\" TEXT",
                "ALTER TABLE \"t\" ADD UNIQUE (\"code\")",
                "ALTER TABLE \"t\" DROP COLUMN \"old\"",
            ]
        );
        assert_eq!(
            p.ops.iter().filter(|o| o.destructive).count(),
            1,
            "only the drop"
        );
        assert!(p.warnings.iter().any(|w| w.contains("Renames are NOT")));
    }

    #[test]
    fn flagged_type_change_gates_its_default() {
        let mut v = field("v", "INTEGER");
        v.default = DefaultSpec::Literal(Literal::Int("0".into()));
        let spec = model("t", vec![pk(), v]);
        let c = canon(
            &spec,
            &[("id", "integer", Some(SEQ)), ("v", "integer", Some("0"))],
        );
        let live = LiveTable {
            name: "t".into(),
            columns: vec![
                live_col("id", "integer", true, Some(SEQ)),
                live_col("v", "text", false, Some("'0'::text")),
            ],
            constraints: vec![pk_con("t")],
        };
        let p = plan(
            &[&spec],
            &HashMap::from([("t".into(), c)]),
            &HashMap::from([("t".into(), live)]),
            &[],
        )
        .unwrap();
        let kinds: Vec<_> = p.ops.iter().map(|o| (o.kind, o.destructive)).collect();
        assert_eq!(
            kinds,
            [
                (OpKind::DropDefault, true),
                (OpKind::AlterType, true),
                (OpKind::SetDefault, true),
            ]
        );
        assert!(p.ops[1].sql.ends_with("TYPE integer USING \"v\"::integer"));
    }

    #[test]
    fn foreign_key_and_unique_drift() {
        let spec = model("book", vec![pk(), fk("author", "author")]);
        let c = canon(
            &spec,
            &[("id", "integer", Some(SEQ)), ("author_id", "integer", None)],
        );
        let live = LiveTable {
            name: "book".into(),
            columns: vec![
                live_col("id", "integer", true, Some(SEQ)),
                live_col("author_id", "integer", false, None),
            ],
            constraints: vec![
                pk_con("book"),
                LiveConstraint {
                    name: "book_author_id_fkey".into(),
                    kind: ConKind::Foreign,
                    columns: vec!["author_id".into()],
                    ref_table: Some("author".into()),
                    ref_same_schema: true,
                    ref_columns: vec!["id".into()],
                    on_delete: Some(OnDelete::Restrict),
                    on_update_no_action: true,
                },
                LiveConstraint {
                    name: "book_author_id_key".into(),
                    kind: ConKind::Unique,
                    columns: vec!["author_id".into()],
                    ref_table: None,
                    ref_same_schema: true,
                    ref_columns: vec![],
                    on_delete: None,
                    on_update_no_action: true,
                },
            ],
        };
        let author = LiveTable {
            name: "author".into(),
            ..Default::default()
        };
        let p = plan(
            &[&spec],
            &HashMap::from([("book".into(), c)]),
            &HashMap::from([("book".into(), live), ("author".into(), author)]),
            &[],
        )
        .unwrap();
        let sql: Vec<_> = p.ops.iter().map(|o| o.sql.as_str()).collect();
        assert_eq!(
            sql,
            [
                "ALTER TABLE \"book\" DROP CONSTRAINT \"book_author_id_fkey\"",
                "ALTER TABLE \"book\" DROP CONSTRAINT \"book_author_id_key\"",
                "ALTER TABLE \"book\" ADD FOREIGN KEY (\"author_id\") REFERENCES \"author\"(\"id\") ON DELETE CASCADE",
            ]
        );
    }

    #[test]
    fn dropped_tables_are_destructive_and_ordered() {
        let parent = LiveTable {
            name: "parent".into(),
            ..Default::default()
        };
        let child = LiveTable {
            name: "child".into(),
            columns: vec![],
            constraints: vec![LiveConstraint {
                name: "child_p_fkey".into(),
                kind: ConKind::Foreign,
                columns: vec!["p".into()],
                ref_table: Some("parent".into()),
                ref_same_schema: true,
                ref_columns: vec!["id".into()],
                on_delete: Some(OnDelete::Cascade),
                on_update_no_action: true,
            }],
        };
        let p = plan(
            &[],
            &HashMap::new(),
            &HashMap::from([("parent".into(), parent), ("child".into(), child)]),
            &["parent".into(), "child".into(), "gone".into()],
        )
        .unwrap();
        let sql: Vec<_> = p
            .ops
            .iter()
            .map(|o| (o.sql.as_str(), o.destructive))
            .collect();
        assert_eq!(
            sql,
            [
                ("DROP TABLE \"child\"", true),
                ("DROP TABLE \"parent\"", true)
            ]
        );
    }

    #[test]
    fn identifiers_are_quoted() {
        let mut odd = field("x", "TEXT");
        odd.column = "we\"ird".into();
        odd.quoted_column = quote_ident(&odd.column);
        let spec = model("t", vec![pk()]);
        let spec2 = model("t\"x", vec![pk(), odd]);
        let p = plan(&[&spec, &spec2], &HashMap::new(), &HashMap::new(), &[]).unwrap();
        assert!(p.ops[1].sql.contains("\"t\"\"x\""));
        assert!(p.ops[1].sql.contains("\"we\"\"ird\""));
    }

    #[test]
    fn duplicates_rejected() {
        let a = model("t", vec![pk()]);
        let b = model("t", vec![pk()]);
        assert!(matches!(
            plan(&[&a, &b], &HashMap::new(), &HashMap::new(), &[]),
            Err(MigrateError::DuplicateTable { .. })
        ));
    }
}
