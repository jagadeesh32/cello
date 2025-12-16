//! Route constraints for parameter validation.
//!
//! This module provides:
//! - Built-in constraints (int, uuid, regex, path, etc.)
//! - Custom constraint registration
//! - Constraint registry for reuse

use parking_lot::RwLock;
use regex::Regex;
use std::collections::HashMap;

/// Route constraint trait for parameter validation.
pub trait RouteConstraint: Send + Sync {
    /// Check if a value matches this constraint.
    fn matches(&self, value: &str) -> bool;

    /// Get the constraint name (e.g., "int", "uuid").
    fn name(&self) -> &'static str;

    /// Get the parameter name this constraint applies to.
    fn param_name(&self) -> &str;

    /// Set the parameter name.
    fn set_param_name(&mut self, name: String);

    /// Clone into a boxed trait object.
    fn clone_box(&self) -> Box<dyn RouteConstraint>;
}

/// Integer constraint - matches valid integers.
#[derive(Clone)]
pub struct IntConstraint {
    param: String,
    min: Option<i64>,
    max: Option<i64>,
}

impl IntConstraint {
    pub fn new() -> Self {
        Self {
            param: String::new(),
            min: None,
            max: None,
        }
    }

    pub fn with_range(min: i64, max: i64) -> Self {
        Self {
            param: String::new(),
            min: Some(min),
            max: Some(max),
        }
    }
}

impl Default for IntConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteConstraint for IntConstraint {
    fn matches(&self, value: &str) -> bool {
        match value.parse::<i64>() {
            Ok(n) => {
                if let Some(min) = self.min {
                    if n < min {
                        return false;
                    }
                }
                if let Some(max) = self.max {
                    if n > max {
                        return false;
                    }
                }
                true
            }
            Err(_) => false,
        }
    }

    fn name(&self) -> &'static str {
        "int"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// UUID constraint - matches valid UUIDs.
#[derive(Clone)]
pub struct UuidConstraint {
    param: String,
}

impl UuidConstraint {
    pub fn new() -> Self {
        Self {
            param: String::new(),
        }
    }
}

impl Default for UuidConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteConstraint for UuidConstraint {
    fn matches(&self, value: &str) -> bool {
        // UUID v4 regex pattern
        let uuid_regex = Regex::new(
            r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
        )
        .unwrap();
        uuid_regex.is_match(value)
    }

    fn name(&self) -> &'static str {
        "uuid"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Slug constraint - matches URL-safe slugs.
#[derive(Clone)]
pub struct SlugConstraint {
    param: String,
}

impl SlugConstraint {
    pub fn new() -> Self {
        Self {
            param: String::new(),
        }
    }
}

impl Default for SlugConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteConstraint for SlugConstraint {
    fn matches(&self, value: &str) -> bool {
        // Slug: lowercase letters, numbers, hyphens
        let slug_regex = Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap();
        slug_regex.is_match(value)
    }

    fn name(&self) -> &'static str {
        "slug"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Alpha constraint - matches alphabetic strings.
#[derive(Clone)]
pub struct AlphaConstraint {
    param: String,
}

impl AlphaConstraint {
    pub fn new() -> Self {
        Self {
            param: String::new(),
        }
    }
}

impl Default for AlphaConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteConstraint for AlphaConstraint {
    fn matches(&self, value: &str) -> bool {
        !value.is_empty() && value.chars().all(|c| c.is_alphabetic())
    }

    fn name(&self) -> &'static str {
        "alpha"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Alphanumeric constraint - matches alphanumeric strings.
#[derive(Clone)]
pub struct AlphanumConstraint {
    param: String,
}

impl AlphanumConstraint {
    pub fn new() -> Self {
        Self {
            param: String::new(),
        }
    }
}

impl Default for AlphanumConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteConstraint for AlphanumConstraint {
    fn matches(&self, value: &str) -> bool {
        !value.is_empty() && value.chars().all(|c| c.is_alphanumeric())
    }

    fn name(&self) -> &'static str {
        "alphanum"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Regex constraint - matches against a custom regex pattern.
#[derive(Clone)]
pub struct RegexConstraint {
    param: String,
    pattern: Regex,
}

impl RegexConstraint {
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            param: String::new(),
            pattern: Regex::new(pattern)?,
        })
    }
}

impl RouteConstraint for RegexConstraint {
    fn matches(&self, value: &str) -> bool {
        self.pattern.is_match(value)
    }

    fn name(&self) -> &'static str {
        "regex"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Path constraint - matches path segments (including slashes).
#[derive(Clone)]
pub struct PathConstraint {
    param: String,
}

impl PathConstraint {
    pub fn new() -> Self {
        Self {
            param: String::new(),
        }
    }
}

impl Default for PathConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteConstraint for PathConstraint {
    fn matches(&self, value: &str) -> bool {
        // Path can contain any characters except null
        !value.is_empty() && !value.contains('\0')
    }

    fn name(&self) -> &'static str {
        "path"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Float constraint - matches valid floating-point numbers.
#[derive(Clone)]
pub struct FloatConstraint {
    param: String,
}

impl FloatConstraint {
    pub fn new() -> Self {
        Self {
            param: String::new(),
        }
    }
}

impl Default for FloatConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteConstraint for FloatConstraint {
    fn matches(&self, value: &str) -> bool {
        value.parse::<f64>().is_ok()
    }

    fn name(&self) -> &'static str {
        "float"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Length constraint - matches strings within a length range.
#[derive(Clone)]
pub struct LengthConstraint {
    param: String,
    min: usize,
    max: usize,
}

impl LengthConstraint {
    pub fn new(min: usize, max: usize) -> Self {
        Self {
            param: String::new(),
            min,
            max,
        }
    }

    pub fn exact(len: usize) -> Self {
        Self::new(len, len)
    }
}

impl RouteConstraint for LengthConstraint {
    fn matches(&self, value: &str) -> bool {
        let len = value.len();
        len >= self.min && len <= self.max
    }

    fn name(&self) -> &'static str {
        "length"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Enum constraint - matches one of a set of allowed values.
#[derive(Clone)]
pub struct EnumConstraint {
    param: String,
    values: Vec<String>,
    case_sensitive: bool,
}

impl EnumConstraint {
    pub fn new(values: Vec<String>) -> Self {
        Self {
            param: String::new(),
            values,
            case_sensitive: true,
        }
    }

    pub fn case_insensitive(mut self) -> Self {
        self.case_sensitive = false;
        self
    }
}

impl RouteConstraint for EnumConstraint {
    fn matches(&self, value: &str) -> bool {
        if self.case_sensitive {
            self.values.iter().any(|v| v == value)
        } else {
            let lower = value.to_lowercase();
            self.values.iter().any(|v| v.to_lowercase() == lower)
        }
    }

    fn name(&self) -> &'static str {
        "enum"
    }

    fn param_name(&self) -> &str {
        &self.param
    }

    fn set_param_name(&mut self, name: String) {
        self.param = name;
    }

    fn clone_box(&self) -> Box<dyn RouteConstraint> {
        Box::new(self.clone())
    }
}

/// Constraint registry for managing and reusing constraints.
pub struct ConstraintRegistry {
    constraints: RwLock<HashMap<String, Box<dyn RouteConstraint>>>,
}

impl ConstraintRegistry {
    /// Create a new constraint registry with built-in constraints.
    pub fn new() -> Self {
        let registry = Self {
            constraints: RwLock::new(HashMap::new()),
        };

        // Register built-in constraints
        registry.register("int", Box::new(IntConstraint::new()));
        registry.register("uuid", Box::new(UuidConstraint::new()));
        registry.register("slug", Box::new(SlugConstraint::new()));
        registry.register("alpha", Box::new(AlphaConstraint::new()));
        registry.register("alphanum", Box::new(AlphanumConstraint::new()));
        registry.register("path", Box::new(PathConstraint::new()));
        registry.register("float", Box::new(FloatConstraint::new()));

        registry
    }

    /// Register a custom constraint.
    pub fn register(&self, name: &str, constraint: Box<dyn RouteConstraint>) {
        self.constraints.write().insert(name.to_string(), constraint);
    }

    /// Get a constraint by name.
    pub fn get(&self, name: &str) -> Option<Box<dyn RouteConstraint>> {
        self.constraints.read().get(name).map(|c| c.clone_box())
    }

    /// Check if a constraint exists.
    pub fn contains(&self, name: &str) -> bool {
        self.constraints.read().contains_key(name)
    }

    /// Get all registered constraint names.
    pub fn names(&self) -> Vec<String> {
        self.constraints.read().keys().cloned().collect()
    }
}

impl Default for ConstraintRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_constraint() {
        let constraint = IntConstraint::new();
        assert!(constraint.matches("123"));
        assert!(constraint.matches("-456"));
        assert!(!constraint.matches("abc"));
        assert!(!constraint.matches("12.34"));
    }

    #[test]
    fn test_int_constraint_range() {
        let constraint = IntConstraint::with_range(1, 100);
        assert!(constraint.matches("50"));
        assert!(constraint.matches("1"));
        assert!(constraint.matches("100"));
        assert!(!constraint.matches("0"));
        assert!(!constraint.matches("101"));
    }

    #[test]
    fn test_uuid_constraint() {
        let constraint = UuidConstraint::new();
        assert!(constraint.matches("550e8400-e29b-41d4-a716-446655440000"));
        assert!(constraint.matches("550E8400-E29B-41D4-A716-446655440000"));
        assert!(!constraint.matches("not-a-uuid"));
        assert!(!constraint.matches("550e8400e29b41d4a716446655440000"));
    }

    #[test]
    fn test_slug_constraint() {
        let constraint = SlugConstraint::new();
        assert!(constraint.matches("hello-world"));
        assert!(constraint.matches("test123"));
        assert!(constraint.matches("a"));
        assert!(!constraint.matches("Hello-World")); // Uppercase not allowed
        assert!(!constraint.matches("hello_world")); // Underscores not allowed
        assert!(!constraint.matches("-hello")); // Can't start with hyphen
    }

    #[test]
    fn test_alpha_constraint() {
        let constraint = AlphaConstraint::new();
        assert!(constraint.matches("hello"));
        assert!(constraint.matches("Hello"));
        assert!(!constraint.matches("hello123"));
        assert!(!constraint.matches(""));
    }

    #[test]
    fn test_path_constraint() {
        let constraint = PathConstraint::new();
        assert!(constraint.matches("dir/subdir/file.txt"));
        assert!(constraint.matches("simple"));
        assert!(!constraint.matches(""));
    }

    #[test]
    fn test_enum_constraint() {
        let constraint = EnumConstraint::new(vec![
            "draft".to_string(),
            "published".to_string(),
            "archived".to_string(),
        ]);
        assert!(constraint.matches("draft"));
        assert!(constraint.matches("published"));
        assert!(!constraint.matches("pending"));
        assert!(!constraint.matches("Draft")); // Case sensitive

        let constraint = constraint.case_insensitive();
        assert!(constraint.matches("DRAFT"));
    }

    #[test]
    fn test_length_constraint() {
        let constraint = LengthConstraint::new(3, 10);
        assert!(constraint.matches("hello"));
        assert!(constraint.matches("abc"));
        assert!(!constraint.matches("ab"));
        assert!(!constraint.matches("hello world"));
    }

    #[test]
    fn test_constraint_registry() {
        let registry = ConstraintRegistry::new();
        assert!(registry.contains("int"));
        assert!(registry.contains("uuid"));
        assert!(!registry.contains("custom"));

        // Register custom constraint
        registry.register("custom", Box::new(AlphaConstraint::new()));
        assert!(registry.contains("custom"));
    }
}
