//! Data Transfer Objects (DTO) system for Cello (inspired by Litestar).
//!
//! Provides:
//! - Field filtering (include/exclude)
//! - Field renaming/aliasing
//! - Nested DTO support
//! - Read-only fields
//! - Write-only fields
//! - Max nested depth control
//! - Field exclusion/inclusion
//! - Validation integration

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// DTO Configuration
// ============================================================================

/// Configuration for DTO field mapping.
#[derive(Clone, Debug, Default)]
pub struct DTOConfig {
    /// Fields to include (if empty, include all except excluded)
    pub include: Vec<String>,
    /// Fields to exclude
    pub exclude: Vec<String>,
    /// Field renaming map (original -> new)
    pub rename_fields: HashMap<String, String>,
    /// Read-only fields (cannot be set in input)
    pub read_only: Vec<String>,
    /// Write-only fields (excluded from output)
    pub write_only: Vec<String>,
    /// Maximum nesting depth for nested DTOs
    pub max_nested_depth: usize,
    /// Whether to allow partial updates
    pub partial: bool,
}

impl DTOConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn include(self, fields: Vec<&str>) -> Self {
        let mut new_self = self;
        new_self.include = fields.iter().map(|s| s.to_string()).collect();
        new_self
    }

    pub fn exclude(self, fields: Vec<&str>) -> Self {
        let mut new_self = self;
        new_self.exclude = fields.iter().map(|s| s.to_string()).collect();
        new_self
    }

    pub fn rename(self, field: &str, alias: &str) -> Self {
        let mut new_self = self;
        new_self.rename_fields.insert(field.to_string(), alias.to_string());
        new_self
    }

    pub fn read_only(self, fields: Vec<&str>) -> Self {
        let mut new_self = self;
        new_self.read_only = fields.iter().map(|s| s.to_string()).collect();
        new_self
    }

    pub fn write_only(self, fields: Vec<&str>) -> Self {
        let mut new_self = self;
        new_self.write_only = fields.iter().map(|s| s.to_string()).collect();
        new_self
    }

    pub fn max_depth(self, depth: usize) -> Self {
        let mut new_self = self;
        new_self.max_nested_depth = depth;
        new_self
    }

    pub fn partial(self, partial: bool) -> Self {
        let mut new_self = self;
        new_self.partial = partial;
        new_self
    }

    /// Check if a field should be included.
    pub fn should_include(&self, field_name: &str) -> bool {
        // If include list is specified, only include those fields
        if !self.include.is_empty() {
            return self.include.contains(&field_name.to_string());
        }

        // Otherwise, exclude specified fields
        !self.exclude.contains(&field_name.to_string())
    }

    /// Check if a field is read-only.
    pub fn is_read_only(&self, field_name: &str) -> bool {
        self.read_only.contains(&field_name.to_string())
    }

    /// Check if a field is write-only.
    pub fn is_write_only(&self, field_name: &str) -> bool {
        self.write_only.contains(&field_name.to_string())
    }

    /// Get the alias for a field (if renamed).
    pub fn get_alias(&self, field_name: &str) -> String {
        self.rename_fields.get(field_name).cloned().unwrap_or_else(|| field_name.to_string())
    }
}

// ============================================================================
// DTO Field Types
// ============================================================================

/// Represents a DTO field with its configuration.
#[derive(Clone, Debug)]
pub struct DTOField {
    pub name: String,
    pub alias: String,
    pub included: bool,
    pub read_only: bool,
    pub write_only: bool,
}

impl DTOField {
    pub fn new(name: &str, config: &DTOConfig) -> Self {
        Self {
            name: name.to_string(),
            alias: config.get_alias(name).to_string(),
            included: config.should_include(name),
            read_only: config.is_read_only(name),
            write_only: config.is_write_only(name),
        }
    }
}

// ============================================================================
// DTO Traits
// ============================================================================

/// Trait for DTO implementations.
pub trait DTO<T>: Send + Sync {
    /// Create DTO from model with configuration.
    fn from_model(model: T, config: &DTOConfig) -> Result<Self, DTOError>
    where
        Self: Sized;

    /// Convert DTO back to model.
    fn to_model(&self, config: &DTOConfig) -> Result<T, DTOError>
    where
        Self: Sized;

    /// Get field information.
    fn fields(&self) -> Vec<DTOField>;

    /// Validate the DTO.
    fn validate(&self) -> Result<(), DTOError>;
}

/// Trait for nested DTOs that can handle depth limits.
pub trait NestedDTO: DTO<Self::Model> {
    type Model;

    /// Check if nesting depth is exceeded.
    fn check_depth(&self, current_depth: usize, max_depth: usize) -> Result<(), DTOError> {
        if current_depth >= max_depth {
            return Err(DTOError::DepthExceeded(current_depth, max_depth));
        }
        Ok(())
    }
}

// ============================================================================
// DTO Error Types
// ============================================================================

/// DTO error types.
#[derive(Debug, Clone)]
pub enum DTOError {
    FieldNotIncluded(String),
    ReadOnlyField(String),
    WriteOnlyField(String),
    ValidationError(String),
    SerializationError(String),
    DepthExceeded(usize, usize),
    Custom(String),
}

impl std::fmt::Display for DTOError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DTOError::FieldNotIncluded(field) => {
                write!(f, "Field '{}' is not included in DTO", field)
            }
            DTOError::ReadOnlyField(field) => {
                write!(f, "Field '{}' is read-only", field)
            }
            DTOError::WriteOnlyField(field) => {
                write!(f, "Field '{}' is write-only", field)
            }
            DTOError::ValidationError(msg) => {
                write!(f, "Validation error: {}", msg)
            }
            DTOError::SerializationError(msg) => {
                write!(f, "Serialization error: {}", msg)
            }
            DTOError::DepthExceeded(current, max) => {
                write!(f, "DTO nesting depth {} exceeds maximum {}", current, max)
            }
            DTOError::Custom(msg) => {
                write!(f, "DTO error: {}", msg)
            }
        }
    }
}

impl std::error::Error for DTOError {}

// ============================================================================
// Serde-based DTO Implementation
// ============================================================================

/// Abstract base DTO that uses Serde for serialization.
pub struct AbstractDTO<T> {
    pub data: T,
    pub config: DTOConfig,
}

impl<T> AbstractDTO<T>
where
    T: Clone + Serialize + for<'de> Deserialize<'de>,
{
    pub fn new(data: T, config: DTOConfig) -> Self {
        Self { data, config }
    }

    /// Filter fields according to configuration.
    pub fn filter_fields(&self, value: serde_json::Value) -> Result<serde_json::Value, DTOError> {
        match value {
            serde_json::Value::Object(mut map) => {
                // Remove excluded fields
                let keys_to_remove: Vec<String> = map
                    .keys()
                    .filter(|key| !self.config.should_include(key))
                    .cloned()
                    .collect();

                for key in keys_to_remove {
                    map.remove(&key);
                }

                // Remove write-only fields
                let write_only_to_remove: Vec<String> = map
                    .keys()
                    .filter(|key| self.config.is_write_only(key))
                    .cloned()
                    .collect();

                for key in write_only_to_remove {
                    map.remove(&key);
                }

                // Rename fields
                let mut renamed_map = serde_json::Map::new();
                for (key, value) in map {
                    let new_key = self.config.get_alias(&key);
                    renamed_map.insert(new_key, value);
                }

                Ok(serde_json::Value::Object(renamed_map))
            }
            other => Ok(other),
        }
    }

    /// Convert to JSON with filtering.
    pub fn to_json(&self) -> Result<serde_json::Value, DTOError> {
        let json_value = serde_json::to_value(&self.data)
            .map_err(|e| DTOError::SerializationError(e.to_string()))?;

        self.filter_fields(json_value)
    }

    /// Convert from JSON with validation.
    pub fn from_json(json: serde_json::Value, config: &DTOConfig) -> Result<T, DTOError> {
        // Check for read-only fields in input
        if let serde_json::Value::Object(ref map) = &json {
            for key in map.keys() {
                if config.is_read_only(key) {
                    return Err(DTOError::ReadOnlyField(key.clone()));
                }
            }
        }

        serde_json::from_value(json)
            .map_err(|e| DTOError::SerializationError(e.to_string()))
    }
}

// ============================================================================
// Concrete DTO Implementations
// ============================================================================

/// User DTO example.
#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct UserDTO {
    pub id: Option<i64>,
    pub username: String,
    pub email: String,
    pub password: Option<String>, // Write-only
    pub created_at: Option<String>, // Read-only
    pub is_active: bool,
    pub roles: Vec<String>,
}

impl DTO<UserDTO> for UserDTO {
    fn from_model(model: UserDTO, config: &DTOConfig) -> Result<Self, DTOError> {
        // Create DTO from model with filtering
        let mut dto = model;

        // Remove write-only fields from output
        if config.is_write_only("password") {
            dto.password = None;
        }

        Ok(dto)
    }

    fn to_model(&self, _config: &DTOConfig) -> Result<UserDTO, DTOError> {
        // Convert DTO back to model
        let model = self.clone();

        // Validate required fields
        if model.username.is_empty() {
            return Err(DTOError::ValidationError("Username is required".to_string()));
        }

        if model.email.is_empty() {
            return Err(DTOError::ValidationError("Email is required".to_string()));
        }

        Ok(model)
    }

    fn fields(&self) -> Vec<DTOField> {
        let config = Self::config();
        vec![
            DTOField::new("id", &config),
            DTOField::new("username", &config),
            DTOField::new("email", &config),
            DTOField::new("password", &config),
            DTOField::new("created_at", &config),
            DTOField::new("is_active", &config),
            DTOField::new("roles", &config),
        ]
    }

    fn validate(&self) -> Result<(), DTOError> {
        if self.username.is_empty() {
            return Err(DTOError::ValidationError("Username cannot be empty".to_string()));
        }

        if self.email.is_empty() {
            return Err(DTOError::ValidationError("Email cannot be empty".to_string()));
        }

        // Email format validation (simple)
        if !self.email.contains('@') {
            return Err(DTOError::ValidationError("Invalid email format".to_string()));
        }

        Ok(())
    }
}

impl UserDTO {
    pub fn config() -> DTOConfig {
        DTOConfig::new()
            .read_only(vec!["id", "created_at"])
            .write_only(vec!["password"])
    }
}

// ============================================================================
// DTO Factory and Utilities
// ============================================================================

/// Factory for creating DTOs with common configurations.
pub struct DTOFactory;

impl DTOFactory {
    /// Create a user DTO for creation (excludes read-only fields).
    pub fn user_create() -> DTOConfig {
        DTOConfig::new()
            .exclude(vec!["id", "created_at"])
            .write_only(vec!["password"])
    }

    /// Create a user DTO for updates (allows partial updates).
    pub fn user_update() -> DTOConfig {
        DTOConfig::new()
            .exclude(vec!["id", "created_at"])
            .partial(true)
    }

    /// Create a user DTO for response (excludes write-only fields).
    pub fn user_response() -> DTOConfig {
        DTOConfig::new()
            .exclude(vec!["password"])
    }

    /// Create a minimal user DTO for listing.
    pub fn user_list() -> DTOConfig {
        DTOConfig::new()
            .include(vec!["id", "username", "is_active"])
    }
}

/// Utility functions for DTO operations.
pub struct DTOUtils;

impl DTOUtils {
    /// Convert a model to DTO with config.
    pub fn to_dto<T, D>(model: T, config: &DTOConfig) -> Result<D, DTOError>
    where
        D: DTO<T>,
    {
        D::from_model(model, config)
    }

    /// Convert DTO to model with config.
    pub fn from_dto<T, D>(dto: &D, config: &DTOConfig) -> Result<T, DTOError>
    where
        D: DTO<T>,
    {
        dto.to_model(config)
    }

    /// Validate a DTO.
    pub fn validate<T, D>(dto: &D) -> Result<(), DTOError>
    where
        D: DTO<T>,
    {
        dto.validate()
    }

    /// Get fields information for a DTO.
    pub fn fields<T, D>(dto: &D) -> Vec<DTOField>
    where
        D: DTO<T>,
    {
        dto.fields()
    }
}

// ============================================================================
// Middleware Integration
// ============================================================================

/// DTO middleware for automatic field filtering.
pub struct DTOMiddleware {
    configs: HashMap<String, DTOConfig>,
}

impl DTOMiddleware {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Register a DTO config for a path.
    pub fn register_config(self, path: &str, config: DTOConfig) -> Self {
        let mut new_self = self;
        new_self.configs.insert(path.to_string(), config);
        new_self
    }

    /// Get config for a path.
    pub fn get_config(&self, path: &str) -> Option<&DTOConfig> {
        self.configs.get(path)
    }
}

impl Default for DTOMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dto_config() {
        let config = DTOConfig::new()
            .include(vec!["id", "name"])
            .exclude(vec!["password"])
            .read_only(vec!["created_at"])
            .write_only(vec!["password"])
            .rename("created_at", "createdAt");

        assert!(config.should_include("id"));
        assert!(config.should_include("name"));
        assert!(!config.should_include("password"));
        assert!(config.is_read_only("created_at"));
        assert!(config.is_write_only("password"));
        assert_eq!(config.get_alias("created_at"), "createdAt");
    }

    #[test]
    fn test_user_dto_validation() {
        let dto = UserDTO {
            id: Some(1),
            username: "".to_string(),
            email: "invalid-email".to_string(),
            password: Some("secret".to_string()),
            created_at: Some("2023-01-01".to_string()),
            is_active: true,
            roles: vec!["user".to_string()],
        };

        // Should fail validation
        assert!(dto.validate().is_err());

        let valid_dto = UserDTO {
            id: Some(1),
            username: "john".to_string(),
            email: "john@example.com".to_string(),
            password: Some("secret".to_string()),
            created_at: Some("2023-01-01".to_string()),
            is_active: true,
            roles: vec!["user".to_string()],
        };

        // Should pass validation
        assert!(valid_dto.validate().is_ok());
    }

    #[test]
    fn test_dto_factory_configs() {
        let create_config = DTOFactory::user_create();
        assert!(!create_config.should_include("id"));
        assert!(create_config.is_write_only("password"));

        let update_config = DTOFactory::user_update();
        assert!(update_config.partial);

        let response_config = DTOFactory::user_response();
        assert!(!response_config.should_include("password"));

        let list_config = DTOFactory::user_list();
        assert!(list_config.should_include("id"));
        assert!(!list_config.should_include("email"));
    }

    #[test]
    fn test_dto_field_filtering() {
        let config = DTOConfig::new()
            .exclude(vec!["password"])
            .write_only(vec!["password"]);

        let dto = AbstractDTO::new(
            serde_json::json!({
                "id": 1,
                "username": "john",
                "password": "secret",
                "email": "john@example.com"
            }),
            config,
        );

        let filtered = dto.filter_fields(dto.data.clone()).unwrap();

        if let serde_json::Value::Object(map) = filtered {
            assert!(map.contains_key("id"));
            assert!(map.contains_key("username"));
            assert!(map.contains_key("email"));
            assert!(!map.contains_key("password"));
        } else {
            panic!("Expected object");
        }
    }
}
