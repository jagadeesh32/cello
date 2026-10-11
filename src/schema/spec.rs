//! The compiled model description: [`ModelSpec`] and [`FieldSpec`].
//!
//! Everything here is plain Rust data (`Send + Sync`, no Python objects), built
//! once when a model class is defined and read-only afterwards.

use std::borrow::Cow;
use std::collections::HashMap;

use thiserror::Error;
use tokio_postgres::types::Type;

use super::ident::{is_identifier, quote_ident, quote_literal};

/// Stable identity of a registered model (never reused within a process).
pub type ModelId = u64;

/// Errors raised while compiling or rendering a schema.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    #[error("ForeignKey on_delete={0:?} is not valid; use one of CASCADE, NO ACTION, RESTRICT, SET DEFAULT, SET NULL")]
    InvalidOnDelete(String),

    #[error("field {field:?} of {model}: ForeignKey target is not a model, so its table cannot be referenced")]
    UnresolvedRelation { model: String, field: String },

    #[error("field {field:?} of {model}: column type could not be determined at class definition")]
    UnknownType { model: String, field: String },
}

/// Postgres column type of a field.
///
/// Built-in field types map to a typed variant; a custom `sql_type` string is
/// kept verbatim as [`PgType::Custom`]. Parsing only accepts a variant when it
/// renders back to the identical string, so DDL output never changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgType {
    Serial,
    Integer,
    BigInt,
    DoublePrecision,
    Boolean,
    Varchar(u32),
    Text,
    Jsonb,
    Timestamptz,
    /// A declared type this module does not model; rendered verbatim.
    Custom(String),
}

impl PgType {
    pub fn parse(ddl: &str) -> Self {
        let parsed = match ddl {
            "SERIAL" => Some(PgType::Serial),
            "INTEGER" => Some(PgType::Integer),
            "BIGINT" => Some(PgType::BigInt),
            "DOUBLE PRECISION" => Some(PgType::DoublePrecision),
            "BOOLEAN" => Some(PgType::Boolean),
            "TEXT" => Some(PgType::Text),
            "JSONB" => Some(PgType::Jsonb),
            "TIMESTAMPTZ" => Some(PgType::Timestamptz),
            _ => ddl
                .strip_prefix("VARCHAR(")
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|n| n.parse::<u32>().ok())
                .map(PgType::Varchar),
        };
        match parsed {
            Some(t) if t.ddl() == ddl => t,
            _ => PgType::Custom(ddl.to_string()),
        }
    }

    /// The type as written in `CREATE TABLE`.
    pub fn ddl(&self) -> Cow<'_, str> {
        Cow::Borrowed(match self {
            PgType::Serial => "SERIAL",
            PgType::Integer => "INTEGER",
            PgType::BigInt => "BIGINT",
            PgType::DoublePrecision => "DOUBLE PRECISION",
            PgType::Boolean => "BOOLEAN",
            PgType::Text => "TEXT",
            PgType::Jsonb => "JSONB",
            PgType::Timestamptz => "TIMESTAMPTZ",
            PgType::Varchar(n) => return Cow::Owned(format!("VARCHAR({n})")),
            PgType::Custom(s) => s.as_str(),
        })
    }

    /// Short lowercase tag (`"varchar"`, `"custom"`, ...).
    pub fn kind(&self) -> &'static str {
        match self {
            PgType::Serial => "serial",
            PgType::Integer => "integer",
            PgType::BigInt => "bigint",
            PgType::DoublePrecision => "double precision",
            PgType::Boolean => "boolean",
            PgType::Varchar(_) => "varchar",
            PgType::Text => "text",
            PgType::Jsonb => "jsonb",
            PgType::Timestamptz => "timestamptz",
            PgType::Custom(_) => "custom",
        }
    }

    /// The type a column of this field has on the wire, when known. `SERIAL`
    /// is `INT4` storage. `None` for custom types: decoders must fall back to
    /// the type the server reports (the 1.3.0 `pg_value_to_py` behaviour).
    pub fn wire_type(&self) -> Option<Type> {
        Some(match self {
            PgType::Serial | PgType::Integer => Type::INT4,
            PgType::BigInt => Type::INT8,
            PgType::DoublePrecision => Type::FLOAT8,
            PgType::Boolean => Type::BOOL,
            PgType::Varchar(_) => Type::VARCHAR,
            PgType::Text => Type::TEXT,
            PgType::Jsonb => Type::JSONB,
            PgType::Timestamptz => Type::TIMESTAMPTZ,
            PgType::Custom(_) => return None,
        })
    }
}

/// A literal default value, captured from the Python declaration.
///
/// Numbers and "other" values keep Python's `str()` text, because that text is
/// what 1.3.0 put into the DDL and `information_schema` output must not change.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Bool(bool),
    Int(String),
    Float(String),
    Text(String),
    /// Any other value (dict, list, Decimal, ...) as Python's `str()` of it,
    /// which is what the DDL renders (quoted). A structured value for
    /// validation/serialization defaults is Phase 03's to add.
    Other(String),
}

impl Literal {
    /// SQL text for `DEFAULT <literal>`.
    pub fn to_sql(&self) -> String {
        match self {
            Literal::Bool(true) => "TRUE".to_string(),
            Literal::Bool(false) => "FALSE".to_string(),
            // Verbatim only when the text is a plain numeric token (what int/float
            // `str()` produces, including inf/nan). A subclass with a custom
            // `__str__` is quoted instead, so it can never splice SQL.
            Literal::Int(t) | Literal::Float(t) if is_numeric_token(t) => t.clone(),
            Literal::Int(t) | Literal::Float(t) | Literal::Text(t) => quote_literal(t),
            Literal::Other(text) => quote_literal(text),
        }
    }
}

fn is_numeric_token(t: &str) -> bool {
    matches!(t, "inf" | "-inf" | "nan")
        || (!t.is_empty()
            && t.bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E')))
}

/// A field's default.
#[derive(Debug, Clone, PartialEq)]
pub enum DefaultSpec {
    None,
    /// `DEFAULT now()` (`DateTimeField(auto_now_add=True)`).
    Now,
    /// A Python callable. It produces no DDL default (1.3.0 never invokes it).
    Callable,
    Literal(Literal),
}

impl DefaultSpec {
    pub fn to_sql(&self) -> Option<String> {
        match self {
            DefaultSpec::Now => Some("now()".to_string()),
            DefaultSpec::Literal(l) => Some(l.to_sql()),
            DefaultSpec::None | DefaultSpec::Callable => None,
        }
    }
}

/// `ON DELETE` action of a foreign key. A closed set, so it is never spliced
/// into SQL from a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnDelete {
    Cascade,
    Restrict,
    SetNull,
    SetDefault,
    NoAction,
}

impl OnDelete {
    /// Parse an action, case-insensitively and with runs of whitespace collapsed.
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let norm = s
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_uppercase();
        Ok(match norm.as_str() {
            "CASCADE" => OnDelete::Cascade,
            "RESTRICT" => OnDelete::Restrict,
            "SET NULL" => OnDelete::SetNull,
            "SET DEFAULT" => OnDelete::SetDefault,
            "NO ACTION" => OnDelete::NoAction,
            _ => return Err(SchemaError::InvalidOnDelete(s.to_string())),
        })
    }

    pub fn as_sql(self) -> &'static str {
        match self {
            OnDelete::Cascade => "CASCADE",
            OnDelete::Restrict => "RESTRICT",
            OnDelete::SetNull => "SET NULL",
            OnDelete::SetDefault => "SET DEFAULT",
            OnDelete::NoAction => "NO ACTION",
        }
    }
}

/// The model a foreign key points at, captured at class definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationTarget {
    /// Registry id of the target, when it is a registered model.
    pub model: Option<ModelId>,
    pub table: String,
    /// `None` when the target model declares no primary key.
    pub pk_column: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationSpec {
    /// `None` when `to` was not a model class (DDL then fails; queries work).
    pub target: Option<RelationTarget>,
    pub on_delete: OnDelete,
}

/// One declared field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSpec {
    /// Attribute name on the model (`author`).
    pub name: String,
    /// Column name (`author_id`, or `db_column`).
    pub column: String,
    /// `column`, quoted for SQL. Computed once.
    pub quoted_column: String,
    /// `None` only when the declared type raised at class definition; the
    /// declaration error is reported when DDL is requested.
    pub pg_type: Option<PgType>,
    pub primary_key: bool,
    pub nullable: bool,
    pub unique: bool,
    pub max_length: Option<u32>,
    pub default: DefaultSpec,
    /// Value generated by the database (`AutoField`); omitted from INSERT.
    pub auto: bool,
    pub auto_now_add: bool,
    /// Visibility: may this field appear in output (serialization)?
    pub readable: bool,
    /// Visibility: may this field be set from request input (`body=`
    /// validation)? `false` for `read_only=True`. Input visibility only: the
    /// ORM still writes the column; `auto` alone decides INSERT omission.
    pub writable: bool,
    pub relation: Option<RelationSpec>,
}

impl FieldSpec {
    /// Accepted on input, never emitted on output (`write_only=True`).
    pub fn write_only(&self) -> bool {
        self.writable && !self.readable
    }

    /// The `to_dict()` key: the column for a ForeignKey (`author_id`), else
    /// the field name.
    pub fn output_key(&self) -> &str {
        if self.relation.is_some() {
            &self.column
        } else {
            &self.name
        }
    }

    /// The attribute `to_dict()` reads: the id column for a ForeignKey, else
    /// the field name.
    pub fn output_attr(&self) -> &str {
        self.output_key()
    }

    /// The column definition used in `CREATE TABLE`.
    pub fn column_ddl(&self, model: &str) -> Result<String, SchemaError> {
        let ty = self
            .pg_type
            .as_ref()
            .ok_or_else(|| SchemaError::UnknownType {
                model: model.to_string(),
                field: self.name.clone(),
            })?;
        let mut out = String::with_capacity(self.quoted_column.len() + 48);
        out.push_str(&self.quoted_column);
        out.push(' ');
        out.push_str(&ty.ddl());
        if self.primary_key {
            out.push_str(" PRIMARY KEY");
        }
        if self.unique && !self.primary_key {
            out.push_str(" UNIQUE");
        }
        if !self.nullable && !self.primary_key {
            out.push_str(" NOT NULL");
        }
        if let Some(lit) = self.default.to_sql() {
            out.push_str(" DEFAULT ");
            out.push_str(&lit);
        }
        if let Some(rel) = &self.relation {
            let target = rel
                .target
                .as_ref()
                .ok_or_else(|| SchemaError::UnresolvedRelation {
                    model: model.to_string(),
                    field: self.name.clone(),
                })?;
            // A target without a primary key renders `"None"`, exactly as 1.3.0
            // did; Postgres then rejects the reference.
            let pk = target.pk_column.as_deref().unwrap_or("None");
            out.push_str(" REFERENCES ");
            out.push_str(&quote_ident(&target.table));
            out.push('(');
            out.push_str(&quote_ident(pk));
            out.push_str(") ON DELETE ");
            out.push_str(rel.on_delete.as_sql());
        }
        Ok(out)
    }
}

/// One key of `Model.to_dict()` (see [`ModelSpec::output_plan`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputField {
    pub key: String,
    /// Index into `ModelSpec::fields` of the field whose value the key holds.
    pub field: usize,
}

/// How a query identifier resolved against a model.
#[derive(Debug, PartialEq)]
pub enum Resolved<'a> {
    /// A declared field name or column; quote with `field.quoted_column`.
    Declared(&'a FieldSpec),
    /// A valid identifier that is not declared (deprecated in 1.3.1); the
    /// quoted identifier.
    Undeclared(String),
    /// Not `^[A-Za-z_][A-Za-z0-9_]*$`; must never reach SQL.
    Invalid,
}

/// One model: table, ordered fields, primary key, relations.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSpec {
    pub id: ModelId,
    /// Python class name, for messages.
    pub name: String,
    pub table: String,
    pub quoted_table: String,
    /// Declaration order as the metaclass builds it (own fields first, then
    /// inherited ones not overridden).
    pub fields: Vec<FieldSpec>,
    /// Index into `fields` of the first `primary_key` field.
    pub pk: Option<usize>,
    /// Query identifier -> field index. Columns first, then field names, so a
    /// field name wins over another field's column. Only valid identifiers.
    by_ident: HashMap<String, usize>,
}

impl ModelSpec {
    pub fn new(id: ModelId, name: String, table: String, fields: Vec<FieldSpec>) -> Self {
        let pk = fields.iter().position(|f| f.primary_key);
        let mut by_ident = HashMap::with_capacity(fields.len() * 2);
        for (i, f) in fields.iter().enumerate() {
            if is_identifier(&f.column) {
                by_ident.entry(f.column.clone()).or_insert(i);
            }
        }
        for (i, f) in fields.iter().enumerate() {
            if is_identifier(&f.name) {
                by_ident.insert(f.name.clone(), i);
            }
        }
        let quoted_table = quote_ident(&table);
        Self {
            id,
            name,
            table,
            quoted_table,
            fields,
            pk,
            by_ident,
        }
    }

    pub fn field(&self, name: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|f| f.name == name)
    }

    pub fn pk_field(&self) -> Option<&FieldSpec> {
        self.pk.map(|i| &self.fields[i])
    }

    /// Resolve a query identifier: validate, then look it up.
    pub fn resolve(&self, ident: &str) -> Resolved<'_> {
        if !is_identifier(ident) {
            return Resolved::Invalid;
        }
        match self.by_ident.get(ident) {
            Some(&i) => Resolved::Declared(&self.fields[i]),
            None => Resolved::Undeclared(quote_ident(ident)),
        }
    }

    /// Every query identifier that resolves to a declared column, with its
    /// quoted column.
    pub fn identifiers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.by_ident
            .iter()
            .map(|(k, &i)| (k.as_str(), self.fields[i].quoted_column.as_str()))
    }

    /// The attribute assignments `Model._from_row` makes for one row, in
    /// order: `(attribute, column)`. Every field sets its name; a ForeignKey
    /// whose column differs from its name also sets `<column>` (both hold the
    /// id). A later assignment to the same attribute wins.
    pub fn hydrate_plan(&self) -> Vec<(&str, &str)> {
        let mut out = Vec::with_capacity(self.fields.len() + 2);
        for f in &self.fields {
            out.push((f.name.as_str(), f.column.as_str()));
            if f.relation.is_some() && f.column != f.name {
                out.push((f.column.as_str(), f.column.as_str()));
            }
        }
        out
    }

    /// The keys `Model.to_dict()` produces, in order, each with the index of
    /// the field whose value it holds.
    ///
    /// `to_dict()` writes one key per readable field (the field name, or the
    /// column for a ForeignKey) into a dict, so when two fields produce the same
    /// key the key keeps its first position and the later field's value. This
    /// plan is that dict's shape, decided once.
    pub fn output_plan(&self) -> Vec<OutputField> {
        let mut out: Vec<OutputField> = Vec::with_capacity(self.fields.len());
        for (i, f) in self.fields.iter().enumerate() {
            if !f.readable {
                continue;
            }
            let key = f.output_key();
            match out.iter_mut().find(|o| o.key == key) {
                Some(o) => o.field = i,
                None => out.push(OutputField {
                    key: key.to_string(),
                    field: i,
                }),
            }
        }
        out
    }

    /// For each [`output_plan`](Self::output_plan) entry, the column whose
    /// value a freshly loaded instance returns for it: the column assigned last
    /// (per [`hydrate_plan`](Self::hydrate_plan)) to the attribute `to_dict()`
    /// reads.
    pub fn output_columns(&self) -> Vec<(String, String)> {
        let hydrate = self.hydrate_plan();
        self.output_plan()
            .into_iter()
            .map(|o| {
                let attr = self.fields[o.field].output_attr();
                let column = hydrate
                    .iter()
                    .rev()
                    .find(|(a, _)| *a == attr)
                    .map_or(attr, |(_, c)| *c);
                (o.key, column.to_string())
            })
            .collect()
    }

    pub fn create_table_sql(&self, if_not_exists: bool) -> Result<String, SchemaError> {
        let cols = self
            .fields
            .iter()
            .map(|f| f.column_ddl(&self.name))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!(
            "CREATE TABLE {}{} ({})",
            if if_not_exists { "IF NOT EXISTS " } else { "" },
            self.quoted_table,
            cols.join(", ")
        ))
    }

    pub fn drop_table_sql(&self, if_exists: bool) -> String {
        format!(
            "DROP TABLE {}{}",
            if if_exists { "IF EXISTS " } else { "" },
            self.quoted_table
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, column: &str, ty: &str) -> FieldSpec {
        FieldSpec {
            name: name.into(),
            column: column.into(),
            quoted_column: quote_ident(column),
            pg_type: Some(PgType::parse(ty)),
            primary_key: false,
            nullable: false,
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

    #[test]
    fn pg_type_round_trips() {
        for t in [
            "SERIAL",
            "INTEGER",
            "VARCHAR(100)",
            "DOUBLE PRECISION",
            "TIMESTAMPTZ",
        ] {
            assert_eq!(PgType::parse(t).ddl(), t);
            assert_ne!(PgType::parse(t).kind(), "custom");
        }
        for t in ["VARCHAR(None)", "VARCHAR(010)", "integer", "NUMERIC(10,2)"] {
            assert_eq!(PgType::parse(t), PgType::Custom(t.into()));
            assert_eq!(PgType::parse(t).ddl(), t);
        }
    }

    #[test]
    fn literals() {
        assert_eq!(Literal::Bool(true).to_sql(), "TRUE");
        assert_eq!(Literal::Float("1.5".into()).to_sql(), "1.5");
        assert_eq!(Literal::Int("1; DROP".into()).to_sql(), "'1; DROP'");
        assert_eq!(Literal::Text("it's".into()).to_sql(), "'it''s'");
        let other = Literal::Other("{'a': 1}".into());
        assert_eq!(other.to_sql(), "'{''a'': 1}'");
    }

    #[test]
    fn resolution_prefers_field_names() {
        let a = field("author", "author_id", "INTEGER");
        let b = field("author_id", "other", "INTEGER");
        let spaced = field("x", "my col", "TEXT");
        let m = ModelSpec::new(1, "M".into(), "m".into(), vec![a, b, spaced]);
        match m.resolve("author_id") {
            Resolved::Declared(f) => assert_eq!(f.name, "author_id"),
            other => panic!("{other:?}"),
        }
        assert_eq!(m.resolve("my col"), Resolved::Invalid);
        assert_eq!(m.resolve("nope"), Resolved::Undeclared("\"nope\"".into()));
    }

    #[test]
    fn output_plan_matches_dict_semantics() {
        let mut fk = field("author", "author_id", "INTEGER");
        fk.relation = Some(RelationSpec {
            target: None,
            on_delete: OnDelete::Cascade,
        });
        // A plain field whose *name* is the FK's column: same to_dict key.
        let clash = field("author_id", "aid_raw", "INTEGER");
        let mut hidden = field("secret", "secret", "TEXT");
        hidden.readable = false;
        let m = ModelSpec::new(
            1,
            "M".into(),
            "m".into(),
            vec![field("id", "id", "INTEGER"), fk, hidden, clash],
        );
        let keys: Vec<_> = m
            .output_plan()
            .into_iter()
            .map(|o| (o.key, o.field))
            .collect();
        assert_eq!(keys, vec![("id".into(), 0), ("author_id".into(), 3)]);
        assert_eq!(
            m.hydrate_plan(),
            vec![
                ("id", "id"),
                ("author", "author_id"),
                ("author_id", "author_id"),
                ("secret", "secret"),
                ("author_id", "aid_raw"),
            ]
        );
        // `author_id` was last assigned from column `aid_raw`.
        assert_eq!(
            m.output_columns(),
            vec![
                ("id".into(), "id".into()),
                ("author_id".into(), "aid_raw".into())
            ]
        );
    }

    #[test]
    fn on_delete() {
        assert_eq!(OnDelete::parse(" set   null ").unwrap(), OnDelete::SetNull);
        assert!(OnDelete::parse("CASCADE; DROP").is_err());
    }
}
