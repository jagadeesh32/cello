//! GraphQL request planning: parse a query document, select the operation,
//! substitute variables, and enforce depth and complexity limits before any
//! resolver runs.
//!
//! Used by the built-in endpoint (`App.enable_graphql`) and by the Python
//! engine (`cello.graphql.GraphQL.execute`, through `graphql_plan`).

use async_graphql_parser::types::{
    DocumentOperations, ExecutableDocument, OperationType, Selection, SelectionSet,
};
use async_graphql_parser::Positioned;
use async_graphql_value::{ConstValue, Name, Value};
use serde::Serialize;
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;

/// Longest query document accepted (bytes).
pub const MAX_QUERY_LEN: usize = 100_000;

/// Deepest bracket nesting accepted before parsing (guards the parser's
/// recursion; any real query is far shallower).
const MAX_NESTING: usize = 128;

/// Upper bound on selections visited, whatever `max_complexity` says, so
/// fragment fan-out cannot make planning itself expensive.
const HARD_COMPLEXITY: usize = 100_000;

/// Limits applied while planning.
#[derive(Clone, Copy, Debug, Default)]
pub struct Limits {
    /// Deepest field nesting (a top-level field is depth 1).
    pub max_depth: Option<usize>,
    /// Most fields in the operation, fragments expanded.
    pub max_complexity: Option<usize>,
    /// Whether `__schema` / `__type` may be queried.
    pub introspection: bool,
}

/// A field to resolve, with its sub-selection.
#[derive(Clone, Debug, Serialize)]
pub struct PlannedField {
    pub name: String,
    /// Response key: the alias, or the name.
    pub alias: String,
    pub arguments: Map<String, JsonValue>,
    pub selection: Vec<PlannedField>,
}

/// The operation to execute.
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    /// `"query"`, `"mutation"` or `"subscription"`.
    pub operation: &'static str,
    pub fields: Vec<PlannedField>,
    /// The operation selects `__schema` or `__type`.
    pub introspection: bool,
    /// Fields counted (fragments expanded).
    pub complexity: usize,
}

/// Plan `query`.
pub fn plan(
    query: &str,
    operation_name: Option<&str>,
    variables: &Map<String, JsonValue>,
    limits: Limits,
) -> Result<Plan, String> {
    if query.len() > MAX_QUERY_LEN {
        return Err(format!("Query is longer than {MAX_QUERY_LEN} bytes"));
    }
    check_nesting(query)?;
    let doc = async_graphql_parser::parse_query(query).map_err(|e| format!("Syntax error: {e}"))?;
    let op = select_operation(&doc, operation_name)?;

    let vars = bind_variables(&op.node, variables)?;
    let operation = match op.node.ty {
        OperationType::Query => "query",
        OperationType::Mutation => "mutation",
        OperationType::Subscription => "subscription",
    };
    let mut walker = Walker {
        doc: &doc,
        vars: &vars,
        limits,
        complexity: 0,
        visited: 0,
        introspection: false,
        fragment_stack: Vec::new(),
    };
    let fields = walker.selection_set(&op.node.selection_set.node, 1)?;
    if walker.introspection && !limits.introspection {
        return Err("Introspection is disabled".to_string());
    }
    Ok(Plan {
        operation,
        fields,
        introspection: walker.introspection,
        complexity: walker.complexity,
    })
}

/// Reject absurd bracket nesting before the recursive-descent parser sees it.
fn check_nesting(query: &str) -> Result<(), String> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for b in query.bytes() {
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' | b'(' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return Err("Query is nested too deeply".to_string());
                }
            }
            b'}' | b']' | b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

fn select_operation<'a>(
    doc: &'a ExecutableDocument,
    name: Option<&str>,
) -> Result<&'a Positioned<async_graphql_parser::types::OperationDefinition>, String> {
    match &doc.operations {
        DocumentOperations::Single(op) => Ok(op),
        DocumentOperations::Multiple(ops) => match name {
            Some(n) => ops
                .get(n)
                .ok_or_else(|| format!("Unknown operation named \"{n}\"")),
            None if ops.len() == 1 => Ok(ops.values().next().expect("one operation")),
            None => Err(
                "operationName is required when the document has several operations".to_string(),
            ),
        },
    }
}

/// Values for the operation's declared variables: the supplied value, else the
/// default, else (for a nullable variable) null.
fn bind_variables(
    op: &async_graphql_parser::types::OperationDefinition,
    supplied: &Map<String, JsonValue>,
) -> Result<HashMap<Name, ConstValue>, String> {
    let mut vars = HashMap::new();
    for def in &op.variable_definitions {
        let name = def.node.name.node.clone();
        let value =
            match supplied.get(name.as_str()) {
                Some(v) => ConstValue::from_json(v.clone())
                    .map_err(|e| format!("Variable \"${name}\": {e}"))?,
                None => def.node.default_value().cloned().ok_or_else(|| {
                    format!("Variable \"${name}\" of required type was not provided")
                })?,
            };
        vars.insert(name, value);
    }
    Ok(vars)
}

struct Walker<'a> {
    doc: &'a ExecutableDocument,
    vars: &'a HashMap<Name, ConstValue>,
    limits: Limits,
    complexity: usize,
    visited: usize,
    introspection: bool,
    fragment_stack: Vec<Name>,
}

impl Walker<'_> {
    fn resolve(&self, value: &Value) -> Result<ConstValue, String> {
        value.clone().into_const_with(|name| {
            self.vars
                .get(&name)
                .cloned()
                .ok_or_else(|| format!("Variable \"${name}\" is not defined"))
        })
    }

    /// `@skip(if:)` / `@include(if:)`.
    fn included(
        &self,
        directives: &[Positioned<async_graphql_parser::types::Directive>],
    ) -> Result<bool, String> {
        for d in directives {
            let name = d.node.name.node.as_str();
            if name != "skip" && name != "include" {
                continue;
            }
            let cond = match d.node.get_argument("if") {
                Some(v) => self.resolve(&v.node)?,
                None => return Err(format!("@{name} requires an \"if\" argument")),
            };
            let cond = matches!(cond, ConstValue::Boolean(true));
            if (name == "skip" && cond) || (name == "include" && !cond) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn visit(&mut self) -> Result<(), String> {
        self.visited += 1;
        if self.visited > HARD_COMPLEXITY {
            return Err("Query is too complex".to_string());
        }
        Ok(())
    }

    fn selection_set(
        &mut self,
        set: &SelectionSet,
        depth: usize,
    ) -> Result<Vec<PlannedField>, String> {
        let mut out = Vec::new();
        for item in &set.items {
            self.visit()?;
            if !self.included(item.node.directives())? {
                continue;
            }
            match &item.node {
                Selection::Field(field) => {
                    let field = &field.node;
                    if let Some(max) = self.limits.max_depth {
                        if depth > max {
                            return Err(format!("Query is nested deeper than {max} levels"));
                        }
                    }
                    self.complexity += 1;
                    if let Some(max) = self.limits.max_complexity {
                        if self.complexity > max {
                            return Err(format!("Query complexity exceeds {max}"));
                        }
                    }
                    let name = field.name.node.as_str();
                    if name == "__schema" || name == "__type" {
                        self.introspection = true;
                    }
                    let mut arguments = Map::new();
                    for (arg, value) in &field.arguments {
                        let v = self
                            .resolve(&value.node)?
                            .into_json()
                            .map_err(|e| format!("Argument \"{}\": {e}", arg.node))?;
                        arguments.insert(arg.node.to_string(), v);
                    }
                    let selection = self.selection_set(&field.selection_set.node, depth + 1)?;
                    out.push(PlannedField {
                        name: name.to_string(),
                        alias: field.response_key().node.to_string(),
                        arguments,
                        selection,
                    });
                }
                Selection::FragmentSpread(spread) => {
                    let fname = &spread.node.fragment_name.node;
                    if self.fragment_stack.contains(fname) {
                        return Err(format!("Fragment \"{fname}\" spreads itself"));
                    }
                    let fragment = self
                        .doc
                        .fragments
                        .get(fname)
                        .ok_or_else(|| format!("Unknown fragment \"{fname}\""))?;
                    self.fragment_stack.push(fname.clone());
                    let fields = self.selection_set(&fragment.node.selection_set.node, depth)?;
                    self.fragment_stack.pop();
                    out.extend(fields);
                }
                Selection::InlineFragment(inline) => {
                    out.extend(self.selection_set(&inline.node.selection_set.node, depth)?);
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            max_depth: Some(3),
            max_complexity: Some(10),
            introspection: false,
        }
    }

    fn p(q: &str) -> Result<Plan, String> {
        plan(q, None, &Map::new(), limits())
    }

    #[test]
    fn operation_type_comes_from_the_ast() {
        // A comment or a name before `mutation` used to be read as a query.
        let pl = p("# hi\nmutation Create { createUser(name: \"a\") { id } }").unwrap();
        assert_eq!(pl.operation, "mutation");
        assert_eq!(pl.fields[0].name, "createUser");
        assert_eq!(pl.fields[0].arguments["name"], "a");
        assert_eq!(p("query Q { users { id } }").unwrap().operation, "query");
    }

    #[test]
    fn depth_and_complexity_are_enforced() {
        assert!(p("{ a { b { c } } }").is_ok());
        assert!(p("{ a { b { c { d } } } }").unwrap_err().contains("deeper"));
        assert!(p("{ a b c d e f g h i j k }")
            .unwrap_err()
            .contains("complexity"));
        // Fragments are expanded before counting.
        let q = "query { ...F ...F } fragment F on Query { a b c d e f }";
        assert!(p(q).unwrap_err().contains("complexity"));
        assert!(p("{ ...F } fragment F on Query { ...F }")
            .unwrap_err()
            .contains("spreads itself"));
    }

    #[test]
    fn introspection_is_found_anywhere() {
        assert!(p("{ __schema { types { name } } }")
            .unwrap_err()
            .contains("Introspection"));
        let q = "{ ...I } fragment I on Query { __type(name: \"X\") { name } }";
        assert!(p(q).unwrap_err().contains("Introspection"));
        assert!(p("{ __typename }").is_ok());
    }

    #[test]
    fn variables_aliases_and_operation_selection() {
        let mut vars = Map::new();
        vars.insert("id".into(), JsonValue::from(7));
        let q = "query A($id: Int!) { u: user(id: $id) { name } } query B { x }";
        let pl = plan(q, Some("A"), &vars, limits()).unwrap();
        assert_eq!(pl.fields[0].alias, "u");
        assert_eq!(pl.fields[0].arguments["id"], 7);
        assert!(plan(q, None, &vars, limits()).is_err());
        assert!(plan(
            "query A($id: Int!) { user(id: $id) }",
            None,
            &Map::new(),
            limits()
        )
        .unwrap_err()
        .contains("not provided"));
    }

    #[test]
    fn skip_and_include() {
        let pl = p("{ a @skip(if: true) b @include(if: false) c }").unwrap();
        assert_eq!(pl.fields.len(), 1);
        assert_eq!(pl.fields[0].name, "c");
    }

    #[test]
    fn nesting_and_syntax_errors() {
        let deep = format!("{{ a(x: {}1{}) }}", "[".repeat(200), "]".repeat(200));
        assert!(p(&deep).unwrap_err().contains("nested"));
        assert!(p("{ a ").unwrap_err().contains("Syntax"));
        // Braces inside strings do not count.
        let s = format!("{{ a(x: \"{}\") }}", "{".repeat(300));
        assert!(p(&s).is_ok());
    }
}
