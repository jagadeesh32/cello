//! Lazy parsing utilities for request bodies.
//!
//! Provides:
//! - LazyBody for deferred parsing
//! - TypedParams for type-safe parameter access
//! - Validation helpers

use std::cell::RefCell;
use std::collections::HashMap;
use std::str::FromStr;

use crate::json::parse_json;

// ============================================================================
// Parameter Errors
// ============================================================================

/// Error type for parameter parsing.
#[derive(Debug, Clone)]
pub enum ParamError {
    /// Parameter not found
    Missing(String),
    /// Parameter has invalid type
    InvalidType {
        param: String,
        expected: &'static str,
        actual: String,
    },
    /// Parameter validation failed
    Validation {
        param: String,
        message: String,
    },
}

impl std::fmt::Display for ParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamError::Missing(name) => write!(f, "Missing required parameter: {}", name),
            ParamError::InvalidType {
                param,
                expected,
                actual,
            } => write!(
                f,
                "Invalid type for '{}': expected {}, got '{}'",
                param, expected, actual
            ),
            ParamError::Validation { param, message } => {
                write!(f, "Validation failed for '{}': {}", param, message)
            }
        }
    }
}

impl std::error::Error for ParamError {}

// ============================================================================
// Typed Parameters
// ============================================================================

/// Type-safe parameter accessor.
#[derive(Clone)]
pub struct TypedParams {
    raw: HashMap<String, String>,
}

impl TypedParams {
    /// Create from raw map.
    pub fn from_map(map: &HashMap<String, String>) -> Self {
        Self { raw: map.clone() }
    }

    /// Create empty.
    pub fn new() -> Self {
        Self {
            raw: HashMap::new(),
        }
    }

    /// Get a parameter with type conversion.
    pub fn get<T: FromStr>(&self, key: &str) -> Option<T> {
        self.raw.get(key).and_then(|v| v.parse().ok())
    }

    /// Get a required parameter with type conversion.
    pub fn require<T: FromStr>(&self, key: &str) -> Result<T, ParamError> {
        let value = self
            .raw
            .get(key)
            .ok_or_else(|| ParamError::Missing(key.to_string()))?;

        value.parse().map_err(|_| ParamError::InvalidType {
            param: key.to_string(),
            expected: std::any::type_name::<T>(),
            actual: value.clone(),
        })
    }

    /// Get a parameter with default value.
    pub fn get_or<T: FromStr>(&self, key: &str, default: T) -> T {
        self.get(key).unwrap_or(default)
    }

    /// Get string parameter.
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.raw.get(key).cloned()
    }

    /// Get required string parameter.
    pub fn require_string(&self, key: &str) -> Result<String, ParamError> {
        self.raw
            .get(key)
            .cloned()
            .ok_or_else(|| ParamError::Missing(key.to_string()))
    }

    /// Get integer parameter.
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.get(key)
    }

    /// Get required integer parameter.
    pub fn require_int(&self, key: &str) -> Result<i64, ParamError> {
        self.require(key)
    }

    /// Get float parameter.
    pub fn get_float(&self, key: &str) -> Option<f64> {
        self.get(key)
    }

    /// Get required float parameter.
    pub fn require_float(&self, key: &str) -> Result<f64, ParamError> {
        self.require(key)
    }

    /// Get boolean parameter.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.raw.get(key).map(|v| {
            let lower = v.to_lowercase();
            lower == "true" || lower == "1" || lower == "yes" || lower == "on"
        })
    }

    /// Get required boolean parameter.
    pub fn require_bool(&self, key: &str) -> Result<bool, ParamError> {
        self.get_bool(key)
            .ok_or_else(|| ParamError::Missing(key.to_string()))
    }

    /// Get UUID parameter.
    pub fn get_uuid(&self, key: &str) -> Option<String> {
        self.raw.get(key).and_then(|v| {
            // Validate UUID format
            let uuid_regex = regex::Regex::new(
                r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
            )
            .ok()?;
            if uuid_regex.is_match(v) {
                Some(v.clone())
            } else {
                None
            }
        })
    }

    /// Get required UUID parameter.
    pub fn require_uuid(&self, key: &str) -> Result<String, ParamError> {
        self.get_uuid(key)
            .ok_or_else(|| ParamError::InvalidType {
                param: key.to_string(),
                expected: "UUID",
                actual: self.raw.get(key).cloned().unwrap_or_default(),
            })
    }

    /// Check if parameter exists.
    pub fn has(&self, key: &str) -> bool {
        self.raw.contains_key(key)
    }

    /// Get all keys.
    pub fn keys(&self) -> Vec<String> {
        self.raw.keys().cloned().collect()
    }

    /// Get parameter with validation.
    pub fn get_validated<T, F>(&self, key: &str, validator: F) -> Result<T, ParamError>
    where
        T: FromStr,
        F: FnOnce(&T) -> Result<(), String>,
    {
        let value: T = self.require(key)?;
        validator(&value).map_err(|msg| ParamError::Validation {
            param: key.to_string(),
            message: msg,
        })?;
        Ok(value)
    }
}

impl Default for TypedParams {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Lazy Body
// ============================================================================

/// Lazy body parser that caches results.
pub struct LazyBody {
    raw: Vec<u8>,
    json_cache: RefCell<Option<Result<serde_json::Value, String>>>,
    form_cache: RefCell<Option<Result<HashMap<String, String>, String>>>,
    text_cache: RefCell<Option<Result<String, String>>>,
}

impl LazyBody {
    /// Create new lazy body.
    pub fn new(data: &[u8]) -> Self {
        Self {
            raw: data.to_vec(),
            json_cache: RefCell::new(None),
            form_cache: RefCell::new(None),
            text_cache: RefCell::new(None),
        }
    }

    /// Get raw bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Get as text (cached).
    pub fn text(&self) -> Result<String, String> {
        let mut cache = self.text_cache.borrow_mut();
        if let Some(ref result) = *cache {
            return result.clone();
        }

        let result = String::from_utf8(self.raw.clone()).map_err(|e| e.to_string());
        *cache = Some(result.clone());
        result
    }

    /// Parse as JSON (cached).
    pub fn json(&self) -> Result<serde_json::Value, String> {
        let mut cache = self.json_cache.borrow_mut();
        if let Some(ref result) = *cache {
            return result.clone();
        }

        let text = self.text()?;
        let result = parse_json(&text);
        *cache = Some(result.clone());
        result
    }

    /// Parse as JSON into typed value.
    pub fn json_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        let value = self.json()?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }

    /// Parse as form data (cached).
    pub fn form(&self) -> Result<HashMap<String, String>, String> {
        let mut cache = self.form_cache.borrow_mut();
        if let Some(ref result) = *cache {
            return result.clone();
        }

        let result = crate::multipart::parse_urlencoded(&self.raw);
        *cache = Some(result.clone());
        result
    }

    /// Get form as typed params.
    pub fn form_params(&self) -> Result<TypedParams, String> {
        let form = self.form()?;
        Ok(TypedParams::from_map(&form))
    }

    /// Check if body is empty.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Get body length.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Clear all caches.
    pub fn clear_cache(&self) {
        *self.json_cache.borrow_mut() = None;
        *self.form_cache.borrow_mut() = None;
        *self.text_cache.borrow_mut() = None;
    }
}

// ============================================================================
// Validation Helpers
// ============================================================================

/// Common validation functions for parameters.
pub struct Validators;

impl Validators {
    /// Validate minimum value.
    pub fn min<T: PartialOrd + std::fmt::Display>(min: T) -> impl Fn(&T) -> Result<(), String> {
        move |value| {
            if *value >= min {
                Ok(())
            } else {
                Err(format!("Value must be >= {}", min))
            }
        }
    }

    /// Validate maximum value.
    pub fn max<T: PartialOrd + std::fmt::Display>(max: T) -> impl Fn(&T) -> Result<(), String> {
        move |value| {
            if *value <= max {
                Ok(())
            } else {
                Err(format!("Value must be <= {}", max))
            }
        }
    }

    /// Validate range.
    pub fn range<T: PartialOrd + std::fmt::Display>(
        min: T,
        max: T,
    ) -> impl Fn(&T) -> Result<(), String> {
        move |value| {
            if *value >= min && *value <= max {
                Ok(())
            } else {
                Err(format!("Value must be between {} and {}", min, max))
            }
        }
    }

    /// Validate minimum length.
    pub fn min_len(min: usize) -> impl Fn(&String) -> Result<(), String> {
        move |value| {
            if value.len() >= min {
                Ok(())
            } else {
                Err(format!("Value must be at least {} characters", min))
            }
        }
    }

    /// Validate maximum length.
    pub fn max_len(max: usize) -> impl Fn(&String) -> Result<(), String> {
        move |value| {
            if value.len() <= max {
                Ok(())
            } else {
                Err(format!("Value must be at most {} characters", max))
            }
        }
    }

    /// Validate regex pattern.
    pub fn pattern(pattern: &str) -> impl Fn(&String) -> Result<(), String> {
        let regex = regex::Regex::new(pattern).expect("Invalid regex pattern");
        move |value| {
            if regex.is_match(value) {
                Ok(())
            } else {
                Err("Value does not match required pattern".to_string())
            }
        }
    }

    /// Validate email format.
    pub fn email() -> impl Fn(&String) -> Result<(), String> {
        let regex =
            regex::Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();
        move |value| {
            if regex.is_match(value) {
                Ok(())
            } else {
                Err("Invalid email format".to_string())
            }
        }
    }

    /// Validate URL format.
    pub fn url() -> impl Fn(&String) -> Result<(), String> {
        let regex = regex::Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap();
        move |value| {
            if regex.is_match(value) {
                Ok(())
            } else {
                Err("Invalid URL format".to_string())
            }
        }
    }

    /// Combine validators.
    pub fn all<T, F>(validators: Vec<F>) -> impl Fn(&T) -> Result<(), String>
    where
        F: Fn(&T) -> Result<(), String>,
    {
        move |value| {
            for validator in &validators {
                validator(value)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typed_params() {
        let mut map = HashMap::new();
        map.insert("id".to_string(), "123".to_string());
        map.insert("name".to_string(), "test".to_string());
        map.insert("active".to_string(), "true".to_string());
        map.insert("price".to_string(), "19.99".to_string());

        let params = TypedParams::from_map(&map);

        assert_eq!(params.get::<i64>("id"), Some(123));
        assert_eq!(params.get_string("name"), Some("test".to_string()));
        assert_eq!(params.get_bool("active"), Some(true));
        assert_eq!(params.get_float("price"), Some(19.99));
        assert!(params.get::<i64>("missing").is_none());
    }

    #[test]
    fn test_require_params() {
        let mut map = HashMap::new();
        map.insert("id".to_string(), "123".to_string());

        let params = TypedParams::from_map(&map);

        assert!(params.require::<i64>("id").is_ok());
        assert!(params.require::<i64>("missing").is_err());
    }

    #[test]
    fn test_lazy_body() {
        let json_str = r#"{"name": "test", "value": 42}"#;
        let body = LazyBody::new(json_str.as_bytes());

        // First call parses
        let json1 = body.json().unwrap();
        // Second call uses cache
        let json2 = body.json().unwrap();

        assert_eq!(json1, json2);
        assert_eq!(json1["name"], "test");
        assert_eq!(json1["value"], 42);
    }

    #[test]
    fn test_validators() {
        let min_validator = Validators::min(0i64);
        assert!(min_validator(&5).is_ok());
        assert!(min_validator(&-1).is_err());

        let email_validator = Validators::email();
        assert!(email_validator(&"test@example.com".to_string()).is_ok());
        assert!(email_validator(&"invalid".to_string()).is_err());

        let len_validator = Validators::min_len(3);
        assert!(len_validator(&"hello".to_string()).is_ok());
        assert!(len_validator(&"hi".to_string()).is_err());
    }

    #[test]
    fn test_validated_params() {
        let mut map = HashMap::new();
        map.insert("age".to_string(), "25".to_string());

        let params = TypedParams::from_map(&map);

        let result = params.get_validated::<i64, _>("age", Validators::range(0, 150));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 25);
    }
}
