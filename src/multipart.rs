//! Multipart form data handling.
//!
//! Provides file upload support and form parsing.

use pyo3::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

/// Uploaded file information.
#[pyclass]
#[derive(Clone, Debug)]
pub struct UploadedFile {
    /// Original filename from the client
    #[pyo3(get)]
    pub filename: String,
    
    /// Content type (MIME type)
    #[pyo3(get)]
    pub content_type: String,
    
    /// File content as bytes
    content: Vec<u8>,
    
    /// Temporary file path (if saved to disk)
    #[pyo3(get)]
    pub temp_path: Option<String>,
}

#[pymethods]
impl UploadedFile {
    /// Get the file content as bytes.
    pub fn read(&self) -> Vec<u8> {
        self.content.clone()
    }

    /// Get the file content as text (UTF-8).
    pub fn read_text(&self) -> PyResult<String> {
        String::from_utf8(self.content.clone())
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Get the file size in bytes.
    pub fn size(&self) -> usize {
        self.content.len()
    }

    /// Save the file to the specified path.
    pub fn save(&self, path: &str) -> PyResult<()> {
        std::fs::write(path, &self.content)
            .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
    }

    /// Get file extension from filename.
    pub fn extension(&self) -> Option<String> {
        PathBuf::from(&self.filename)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
    }
}

impl UploadedFile {
    /// Create a new uploaded file.
    pub fn new(filename: String, content_type: String, content: Vec<u8>) -> Self {
        UploadedFile {
            filename,
            content_type,
            content,
            temp_path: None,
        }
    }

    /// Get the raw content bytes.
    pub fn content(&self) -> &[u8] {
        &self.content
    }
}

/// Parsed multipart form data.
#[pyclass]
#[derive(Clone)]
pub struct FormData {
    /// Text fields
    fields: HashMap<String, String>,
    
    /// File fields
    files: HashMap<String, Vec<UploadedFile>>,
}

#[pymethods]
impl FormData {
    /// Create empty form data.
    #[new]
    pub fn new() -> Self {
        FormData {
            fields: HashMap::new(),
            files: HashMap::new(),
        }
    }

    /// Get a text field value.
    pub fn get(&self, name: &str) -> Option<String> {
        self.fields.get(name).cloned()
    }

    /// Get a text field with default.
    #[pyo3(signature = (name, default=None))]
    pub fn get_or(&self, name: &str, default: Option<&str>) -> String {
        self.fields
            .get(name)
            .cloned()
            .unwrap_or_else(|| default.unwrap_or("").to_string())
    }

    /// Get an uploaded file.
    pub fn get_file(&self, name: &str) -> Option<UploadedFile> {
        self.files.get(name).and_then(|files| files.first().cloned())
    }

    /// Get all uploaded files for a field.
    pub fn get_files(&self, name: &str) -> Vec<UploadedFile> {
        self.files.get(name).cloned().unwrap_or_default()
    }

    /// Get all field names.
    pub fn field_names(&self) -> Vec<String> {
        self.fields.keys().cloned().collect()
    }

    /// Get all file field names.
    pub fn file_names(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    /// Check if form has a field.
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.contains_key(name)
    }

    /// Check if form has a file.
    pub fn has_file(&self, name: &str) -> bool {
        self.files.contains_key(name)
    }

    /// Get the number of text fields.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Get the number of file fields.
    pub fn file_count(&self) -> usize {
        self.files.values().map(|f| f.len()).sum()
    }
}

impl Default for FormData {
    fn default() -> Self {
        Self::new()
    }
}

impl FormData {
    /// Add a text field.
    pub fn add_field(&mut self, name: String, value: String) {
        self.fields.insert(name, value);
    }

    /// Add a file field.
    pub fn add_file(&mut self, name: String, file: UploadedFile) {
        self.files.entry(name).or_default().push(file);
    }
}

/// Parse URL-encoded form data.
pub fn parse_urlencoded(body: &[u8]) -> Result<HashMap<String, String>, String> {
    let body_str = std::str::from_utf8(body)
        .map_err(|e| format!("Invalid UTF-8 in form data: {}", e))?;
    
    let mut fields = HashMap::new();
    
    for pair in body_str.split('&') {
        if pair.is_empty() {
            continue;
        }
        
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");
        
        // URL decode
        let key = urlencoding::decode(key)
            .map_err(|e| format!("Failed to decode key: {}", e))?
            .to_string();
        let value = urlencoding::decode(&value.replace('+', " "))
            .map_err(|e| format!("Failed to decode value: {}", e))?
            .to_string();
        
        fields.insert(key, value);
    }
    
    Ok(fields)
}

/// Maximum file size (default 10MB).
pub const DEFAULT_MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Maximum total form size (default 50MB).
pub const DEFAULT_MAX_FORM_SIZE: usize = 50 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uploaded_file() {
        let file = UploadedFile::new(
            "test.txt".to_string(),
            "text/plain".to_string(),
            b"Hello, World!".to_vec(),
        );
        
        assert_eq!(file.filename, "test.txt");
        assert_eq!(file.size(), 13);
        assert_eq!(file.extension(), Some("txt".to_string()));
        assert_eq!(file.read_text().unwrap(), "Hello, World!");
    }

    #[test]
    fn test_form_data() {
        let mut form = FormData::new();
        form.add_field("name".to_string(), "John".to_string());
        form.add_field("email".to_string(), "john@example.com".to_string());
        
        assert_eq!(form.get("name"), Some("John".to_string()));
        assert_eq!(form.get("email"), Some("john@example.com".to_string()));
        assert_eq!(form.get("missing"), None);
        assert_eq!(form.field_count(), 2);
    }

    #[test]
    fn test_parse_urlencoded() {
        let body = b"name=John&email=john%40example.com&message=Hello+World";
        let result = parse_urlencoded(body).unwrap();
        
        assert_eq!(result.get("name"), Some(&"John".to_string()));
        assert_eq!(result.get("email"), Some(&"john@example.com".to_string()));
        assert_eq!(result.get("message"), Some(&"Hello World".to_string()));
    }
}
