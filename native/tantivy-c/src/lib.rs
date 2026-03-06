// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Tantivy FFI wrapper for The Email API
//
// This library provides a C ABI wrapper around Tantivy for use from Inko
// via FFI. It manages index lifecycle, document indexing, and searching.
//
// Usage:
// 1. Open or create an index with tantivy_index_open
// 2. Add documents with tantivy_index_add_doc
// 3. Search with tantivy_index_search
// 4. Commit changes with tantivy_index_commit
// 5. Close with tantivy_index_close

// All extern "C" functions in this FFI layer are inherently unsafe due to
// raw pointer handling. Each function includes detailed # Safety and
// # Memory Ownership documentation. The clippy warning is suppressed to
// avoid repetitive documentation since all functions share the same pattern.
#![allow(clippy::missing_safety_doc)]

use libc::{c_char, c_int, size_t};
use std::ffi::{CStr, CString};
use std::mem::{self, offset_of, ManuallyDrop};
use std::path::Path;
use std::slice;
use std::sync::LazyLock;
use tantivy::{
    collector::TopDocs, query::QueryParser, schema::IndexRecordOption, schema::*, Index,
    IndexReader, IndexWriter, ReloadPolicy,
};

// Default search fields - single source of truth for field names
const DEFAULT_SEARCH_FIELD_SUBJECT: &str = "subject";
const DEFAULT_SEARCH_FIELD_BODY: &str = "body";

// Security limits to prevent resource exhaustion
// IMPORTANT: These values must be kept in sync with src/input_validation.inko
// When modifying these, update the corresponding constants in Inko.
const MAX_FIELDS_PER_DOCUMENT: usize = 1000;
const MAX_FIELD_VALUE_LENGTH: usize = 10 * 1024 * 1024; // 10MB per field
const MAX_QUERY_LENGTH: usize = 10 * 1024; // 10KB query string
const MAX_SEARCH_LIMIT: usize = 10000; // Maximum results per search

// Opaque pointer types for FFI
// The `index` field is required for Tantivy operations but is accessed via the cached query_parser
#[allow(dead_code)]
pub struct TantivyIndexWrapper {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    schema: Schema,
    // Cache commonly used fields to avoid repeated lookups
    // For email schema: subject and body
    // For custom schemas: fields specified in default_search_fields
    subject_field: Option<Field>,
    body_field: Option<Field>,
    // For custom schemas, store all default search fields
    default_search_fields: Vec<Field>,
    // Cache QueryParser to avoid recreation on every search operation
    // Safe to cache since schema is immutable after index creation
    query_parser: QueryParser,
}

impl TantivyIndexWrapper {
    // Get default search fields for query parsing
    // Returns custom default_search_fields if available, otherwise falls back to subject/body
    fn get_default_search_fields(&self) -> Vec<Field> {
        if !self.default_search_fields.is_empty() {
            self.default_search_fields.clone()
        } else {
            let mut fields = Vec::new();
            if let Some(subject) = self.subject_field {
                fields.push(subject);
            }
            if let Some(body) = self.body_field {
                fields.push(body);
            }
            fields
        }
    }
}

// Configuration structure
#[repr(C)]
pub struct TantivyConfig {
    pub index_path: *const c_char,
    pub reader_memory_budget_bytes: usize,
    pub writer_memory_budget_bytes: usize,
    pub num_threads: usize,
    // JSON string defining schema fields. If NULL, uses default email schema.
    // Format: {"fields": [{"name": "field_name", "type": "text|string|u64|i64|f64|bool", "indexed": true, "stored": true}], "default_search_fields": ["field1", "field2"]}
    pub schema_json: *const c_char,
}

const _: () = {
    assert!(mem::size_of::<TantivyConfig>() == 40, "TantivyConfig size mismatch");
    assert!(offset_of!(TantivyConfig, index_path) == 0, "TantivyConfig index_path offset mismatch");
    assert!(
        offset_of!(TantivyConfig, reader_memory_budget_bytes) == 8,
        "TantivyConfig reader_memory_budget_bytes offset mismatch"
    );
    assert!(
        offset_of!(TantivyConfig, writer_memory_budget_bytes) == 16,
        "TantivyConfig writer_memory_budget_bytes offset mismatch"
    );
    assert!(
        offset_of!(TantivyConfig, num_threads) == 24,
        "TantivyConfig num_threads offset mismatch"
    );
    assert!(
        offset_of!(TantivyConfig, schema_json) == 32,
        "TantivyConfig schema_json offset mismatch"
    );
};

// Document field structure
#[repr(C)]
pub struct DocField {
    pub key: *const c_char,
    pub value: *const c_char,
}

const _: () = {
    assert!(mem::size_of::<DocField>() == 16, "DocField size mismatch");
    assert!(offset_of!(DocField, key) == 0, "DocField key offset mismatch");
    assert!(offset_of!(DocField, value) == 8, "DocField value offset mismatch");
};

// Batch document structure
#[repr(C)]
pub struct BatchDocument {
    pub doc_id: *const c_char,
    pub fields: *const DocField,
    pub num_fields: size_t,
}

const _: () = {
    assert!(mem::size_of::<BatchDocument>() == 24, "BatchDocument size mismatch");
    assert!(offset_of!(BatchDocument, doc_id) == 0, "BatchDocument doc_id offset mismatch");
    assert!(offset_of!(BatchDocument, fields) == 8, "BatchDocument fields offset mismatch");
    assert!(
        offset_of!(BatchDocument, num_fields) == 16,
        "BatchDocument num_fields offset mismatch"
    );
};

// Search result structure
#[repr(C)]
pub struct TantivyResult {
    pub doc_id: *mut c_char,
    pub score: f32,
    pub highlight: *mut c_char,
}

const _: () = {
    assert!(mem::size_of::<TantivyResult>() == 24, "TantivyResult size mismatch");
    assert!(offset_of!(TantivyResult, doc_id) == 0, "TantivyResult doc_id offset mismatch");
    assert!(offset_of!(TantivyResult, score) == 8, "TantivyResult score offset mismatch");
    assert!(offset_of!(TantivyResult, highlight) == 16, "TantivyResult highlight offset mismatch");
};

// Aggregation result structure
#[repr(C)]
pub struct TantivyAggregationResult {
    pub key: *mut c_char,
    pub count: u64,
}

const _: () = {
    assert!(
        mem::size_of::<TantivyAggregationResult>() == 16,
        "TantivyAggregationResult size mismatch"
    );
    assert!(
        offset_of!(TantivyAggregationResult, key) == 0,
        "TantivyAggregationResult key offset mismatch"
    );
    assert!(
        offset_of!(TantivyAggregationResult, count) == 8,
        "TantivyAggregationResult count offset mismatch"
    );
};

// Suggestion result structure
#[repr(C)]
pub struct TantivySuggestion {
    pub text: *mut c_char,
    pub score: f32,
}

const _: () = {
    assert!(mem::size_of::<TantivySuggestion>() == 16, "TantivySuggestion size mismatch");
    assert!(offset_of!(TantivySuggestion, text) == 0, "TantivySuggestion text offset mismatch");
    assert!(offset_of!(TantivySuggestion, score) == 8, "TantivySuggestion score offset mismatch");
};

// Schema configuration structures for JSON parsing
#[derive(serde::Deserialize)]
struct SchemaConfig {
    fields: Vec<FieldConfig>,
    #[serde(default)]
    default_search_fields: Vec<String>,
}

#[derive(serde::Deserialize)]
struct FieldConfig {
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    #[serde(default = "default_true")]
    indexed: bool,
    #[serde(default = "default_true")]
    stored: bool,
}

fn default_true() -> bool {
    true
}

const MAX_SCHEMA_FIELDS: usize = 100;

static FIELD_NAME_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").expect("Invalid field name regex pattern")
});

fn validate_field_name(name: &str) -> Result<(), String> {
    if !FIELD_NAME_REGEX.is_match(name) {
        return Err(format!(
            "Invalid field name '{}': must start with a letter or underscore and contain only letters, numbers, and underscores",
            name
        ));
    }

    Ok(())
}

fn build_schema_from_json(json_str: &str) -> Result<(Schema, Vec<String>), String> {
    let config: SchemaConfig = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse schema JSON: {}", e))?;

    if config.fields.is_empty() {
        return Err("Schema must define at least one field".to_string());
    }

    if config.fields.len() > MAX_SCHEMA_FIELDS {
        return Err(format!(
            "Too many fields in schema: {} exceeds maximum of {}",
            config.fields.len(),
            MAX_SCHEMA_FIELDS
        ));
    }

    let mut schema_builder = Schema::builder();
    let mut field_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for field in &config.fields {
        validate_field_name(&field.name)?;

        if field_names.contains(&field.name) {
            return Err(format!("Duplicate field name: {}", field.name));
        }
        field_names.insert(field.name.clone());

        let mut options = tantivy::schema::TextOptions::default();

        if field.stored {
            options = options | STORED;
        }

        match field.field_type.as_str() {
            "text" => {
                if field.indexed {
                    options = options | TEXT;
                }
                schema_builder.add_text_field(&field.name, options);
            }
            "string" => {
                if field.indexed {
                    options = options | STRING;
                }
                schema_builder.add_text_field(&field.name, options);
            }
            "u64" => {
                let mut num_options = tantivy::schema::NumericOptions::default();
                if field.indexed {
                    num_options = num_options.set_indexed();
                }
                if field.stored {
                    num_options = num_options.set_stored();
                }
                schema_builder.add_u64_field(&field.name, num_options);
            }
            "i64" => {
                let mut num_options = tantivy::schema::NumericOptions::default();
                if field.indexed {
                    num_options = num_options.set_indexed();
                }
                if field.stored {
                    num_options = num_options.set_stored();
                }
                schema_builder.add_i64_field(&field.name, num_options);
            }
            "f64" => {
                let mut num_options = tantivy::schema::NumericOptions::default();
                if field.indexed {
                    num_options = num_options.set_indexed();
                }
                if field.stored {
                    num_options = num_options.set_stored();
                }
                schema_builder.add_f64_field(&field.name, num_options);
            }
            "bool" => {
                let mut num_options = tantivy::schema::NumericOptions::default();
                if field.indexed {
                    num_options = num_options.set_indexed();
                }
                if field.stored {
                    num_options = num_options.set_stored();
                }
                schema_builder.add_bool_field(&field.name, num_options);
            }
            _ => {
                return Err(format!(
                    "Unknown field type: {}. Allowed types: text, string, u64, i64, f64, bool",
                    field.field_type
                ));
            }
        }
    }

    let schema = schema_builder.build();

    if !config.default_search_fields.is_empty() {
        for field_name in &config.default_search_fields {
            if !field_names.contains(field_name) {
                return Err(format!(
                    "Default search field '{}' is not defined in schema",
                    field_name
                ));
            }
        }
    }

    let default_fields =
        if config.default_search_fields.is_empty() { vec![] } else { config.default_search_fields };

    Ok((schema, default_fields))
}

// Schema definition for emails (default schema)
fn email_schema() -> Schema {
    let mut schema_builder = Schema::builder();

    // Primary key and identification
    schema_builder.add_text_field("id", STRING | STORED);
    schema_builder.add_text_field("account_id", STRING | STORED);
    schema_builder.add_text_field("mailbox", STRING | STORED);
    schema_builder.add_text_field("message_id", STRING | STORED);

    // Full-text searchable fields (default search fields)
    schema_builder.add_text_field(DEFAULT_SEARCH_FIELD_SUBJECT, TEXT | STORED);
    schema_builder.add_text_field(DEFAULT_SEARCH_FIELD_BODY, TEXT | STORED);

    // Email metadata
    schema_builder.add_text_field("from", STRING | STORED);
    schema_builder.add_text_field("to", STRING | STORED);
    schema_builder.add_text_field("cc", STRING | STORED);

    // Date and size for range queries
    schema_builder.add_u64_field("date", INDEXED | STORED);
    schema_builder.add_u64_field("size", INDEXED | STORED);

    // Flags and labels (faceted)
    schema_builder.add_text_field("flags", STRING | STORED);
    schema_builder.add_text_field("labels", STRING | STORED);

    // Threading support
    schema_builder.add_text_field("thread_id", STRING | STORED);
    schema_builder.add_text_field("in_reply_to", STRING | STORED);
    schema_builder.add_text_field("references", TEXT | STORED);

    // Attachments
    schema_builder.add_text_field("attachments", STRING | STORED);

    schema_builder.build()
}

// Helper: Convert C string to Rust String
unsafe fn c_str_to_string(ptr: *const c_char) -> Result<String, &'static str> {
    if ptr.is_null() {
        Ok(String::new())
    } else {
        CStr::from_ptr(ptr).to_str().map(|s| s.to_string()).map_err(|_| "Invalid UTF-8 in C string")
    }
}

// Helper: Convert Rust String to allocated C string
// Returns null pointer if string contains null bytes
fn string_to_c_string(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

// Helper: Escape special characters in query strings to prevent injection (CWE-78)
//
// NOTE: This function is duplicated in src/tantivy/query_builder.inko
// Both versions must be kept in sync. Any changes to the special character
// list must be applied to both locations.
//
// Full list of Tantivy/Lucene special chars: " ' + - ( ) [ ] : * ? \ ^ ~ { } | ! and whitespace
fn escape_query_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '"' | '\'' | '+' | '-' | '(' | ')' | '[' | ']' | ':' | '*' | '?' | '\\' | ' ' | '^'
            | '~' | '{' | '}' | '|' | '!' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

// Helper: Add a field to a document with proper type handling
// Returns Ok(()) on success, Err(error_message) on failure
// doc_index is used for error messages in batch operations (None for single doc)
fn add_field_to_doc(
    doc: &mut tantivy::TantivyDocument,
    schema: &Schema,
    key: &str,
    value: &str,
    doc_index: Option<usize>,
) -> Result<(), String> {
    let field_entry = match schema.get_field(key) {
        Ok(f) => f,
        Err(_) => return Ok(()), // Skip unknown fields silently
    };

    let field_type = schema.get_field_entry(field_entry);
    let doc_prefix = doc_index.map_or(String::new(), |i| format!("Document {}: ", i));

    match field_type.field_type() {
        FieldType::Str(_) => {
            doc.add_text(field_entry, value);
        }
        FieldType::U64(_) => match value.parse::<u64>() {
            Ok(num) => doc.add_u64(field_entry, num),
            Err(_) => {
                return Err(format!(
                    "{}field '{}' expects u64 but value '{}' is not a valid unsigned integer",
                    doc_prefix, key, value
                ));
            }
        },
        FieldType::I64(_) => match value.parse::<i64>() {
            Ok(num) => doc.add_i64(field_entry, num),
            Err(_) => {
                return Err(format!(
                    "{}field '{}' expects i64 but value '{}' is not a valid signed integer",
                    doc_prefix, key, value
                ));
            }
        },
        FieldType::F64(_) => match value.parse::<f64>() {
            Ok(num) => doc.add_f64(field_entry, num),
            Err(_) => {
                return Err(format!(
                    "{}field '{}' expects f64 but value '{}' is not a valid floating point number",
                    doc_prefix, key, value
                ));
            }
        },
        FieldType::Bool(_) => {
            let bool_val = value.to_lowercase() == "true";
            doc.add_bool(field_entry, bool_val);
        }
        unsupported_type => {
            eprintln!(
                "Warning: Unsupported field type {:?} for field '{}', skipping",
                unsupported_type, key
            );
        }
    }

    Ok(())
}

// Helper: Sanitize error messages to remove filesystem paths
pub(crate) fn sanitize_error_message(msg: &str) -> String {
    if !SANITIZE_ERRORS {
        return msg.to_string();
    }

    // Replace absolute paths with generic placeholders to prevent leaking filesystem structure
    let mut result = String::new();
    let mut chars = msg.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '/' {
            // Check if this looks like an absolute path (starts with / and has more chars)
            let mut path_chars = String::from('/');
            let mut found_path = false;

            // Collect path until we hit whitespace, quotes, or end
            while let Some(&next_c) = chars.peek() {
                if next_c.is_whitespace() || next_c == '"' || next_c == '\'' || next_c == ':' {
                    break;
                }
                path_chars.push(chars.next().unwrap());
                if path_chars.len() > 1 {
                    found_path = true;
                }
            }

            // If we found a path, replace it; otherwise keep the original /
            if found_path {
                result.push_str("<path>");
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

// Compile-time flag for error sanitization
// Debug builds: No sanitization, full information available
// Release builds: Sanitized for production security
#[cfg(debug_assertions)]
pub(crate) const SANITIZE_ERRORS: bool = false;

#[cfg(not(debug_assertions))]
pub(crate) const SANITIZE_ERRORS: bool = true;

// Helper: Create error message C string with fallback
pub(crate) fn create_error_string(msg: &str) -> *mut c_char {
    // Always log full error to stderr for debugging (captured in logs/CI)
    eprintln!("[TANTIVY_ERROR] {}", msg);

    // Sanitize for user-facing messages in release builds
    let user_msg = if SANITIZE_ERRORS { sanitize_error_message(msg) } else { msg.to_string() };

    match CString::new(user_msg.as_str()) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => {
            eprintln!("[TANTIVY_ERROR] Failed to create error string, contained null bytes");
            let cleaned = user_msg.replace('\0', "");
            CString::new(cleaned)
                .unwrap_or_else(|_| {
                    eprintln!("[TANTIVY_ERROR] Failed to create error string after cleaning");
                    CString::new("Error message contains invalid characters").unwrap()
                })
                .into_raw()
        }
    }
}

// Helper: Safely transfer ownership of Vec to C
// Uses ManuallyDrop to make the ownership transfer explicit and intentional
// The caller MUST call the corresponding free function to avoid memory leaks
fn vec_into_raw_parts<T>(vec: Vec<T>) -> (*mut T, usize) {
    let mut vec = ManuallyDrop::new(vec);
    let ptr = vec.as_mut_ptr();
    let len = vec.len();
    (ptr, len)
}

// Helper: Convert a Tantivy document to a JSON Value
// Extracts all stored fields and preserves their native types (numbers, strings, bools)
fn document_to_json_map(
    doc: &tantivy::TantivyDocument,
    schema: &Schema,
    score: Option<f32>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut json_map = serde_json::Map::new();

    // Extract all stored fields from the document, preserving types
    for field in schema.fields() {
        let (field, _entry) = field;
        let field_name = schema.get_field_name(field);

        if let Some(field_value) = doc.get_first(field) {
            let json_value = if let Some(text) = field_value.as_str() {
                serde_json::Value::String(text.to_string())
            } else if let Some(num) = field_value.as_i64() {
                serde_json::Value::Number(serde_json::Number::from(num))
            } else if let Some(num) = field_value.as_u64() {
                serde_json::Value::Number(serde_json::Number::from(num))
            } else if let Some(num) = field_value.as_f64() {
                // f64 requires special handling for JSON
                serde_json::Number::from_f64(num)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(num.to_string()))
            } else if let Some(b) = field_value.as_bool() {
                serde_json::Value::Bool(b)
            } else {
                continue;
            };

            json_map.insert(field_name.to_string(), json_value);
        }
    }

    // Add score if provided
    if let Some(score_val) = score {
        if let Some(score_num) = serde_json::Number::from_f64(score_val as f64) {
            json_map.insert("score".to_string(), serde_json::Value::Number(score_num));
        } else {
            // Fallback for NaN/Infinity
            json_map.insert("score".to_string(), serde_json::Value::String(score_val.to_string()));
        }
    }

    json_map
}

// Open or create a Tantivy index
//
// # Safety
// The caller must ensure:
// - `config` is a valid pointer to a TantivyConfig struct
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
// - Returned pointer must be freed by calling tantivy_index_close
//
// Undefined behavior if:
// - `config` is NULL or points to invalid memory
// - `error_out` is NULL
//
// # Memory Ownership
// - Returns: *mut TantivyIndexWrapper (owned by caller, must free with tantivy_index_close)
// - Borrows: *const TantivyConfig (not freed by this function)
// - Borrows: config.index_path string (not freed by this function)
// - Borrows: config.schema_json string if present (not freed by this function)
// - Allocates: error_out on error (owned by caller, must free with tantivy_error_free)
//
// # Usage
// 1. Create TantivyConfig with index_path and optional schema_json
// 2. Call this function to get index wrapper
// 3. Use the wrapper for all index operations
// 4. Call tantivy_index_close when done to free memory
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_open(
    config: *const TantivyConfig,
    error_out: *mut *mut c_char,
) -> *mut TantivyIndexWrapper {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_open");
        return std::ptr::null_mut();
    }

    if config.is_null() {
        *error_out = create_error_string("Config is null");
        return std::ptr::null_mut();
    }

    let cfg = &*config;

    let index_path = match c_str_to_string(cfg.index_path) {
        Ok(path) => path,
        Err(_) => {
            *error_out = create_error_string("Invalid index path");
            return std::ptr::null_mut();
        }
    };

    // Build schema from JSON config or use default email schema
    let (schema, default_field_names) = if cfg.schema_json.is_null() {
        // Use default email schema
        let schema = email_schema();
        let defaults =
            vec![DEFAULT_SEARCH_FIELD_SUBJECT.to_string(), DEFAULT_SEARCH_FIELD_BODY.to_string()];
        (schema, defaults)
    } else {
        // Parse custom schema from JSON
        let schema_json_str = match c_str_to_string(cfg.schema_json) {
            Ok(s) => s,
            Err(_) => {
                *error_out = create_error_string("Invalid schema JSON string");
                return std::ptr::null_mut();
            }
        };

        match build_schema_from_json(&schema_json_str) {
            Ok((schema, defaults)) => (schema, defaults),
            Err(e) => {
                *error_out = create_error_string(&e);
                return std::ptr::null_mut();
            }
        }
    };

    // Create directory if it doesn't exist
    let path = Path::new(&index_path);
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(path) {
            *error_out = create_error_string(&format!("Failed to create directory: {}", e));
            return std::ptr::null_mut();
        }
    }

    // Open or create index
    let index = match Index::open_in_dir(path) {
        Ok(idx) => idx,
        Err(_) => {
            // If opening failed, try creating a new index
            match Index::create_in_dir(path, schema.clone()) {
                Ok(idx) => idx,
                Err(e) => {
                    *error_out = create_error_string(&format!("Failed to create index: {}", e));
                    return std::ptr::null_mut();
                }
            }
        }
    };

    // Create reader with memory budget
    let reader =
        match index.reader_builder().reload_policy(ReloadPolicy::OnCommitWithDelay).try_into() {
            Ok(r) => r,
            Err(e) => {
                *error_out = create_error_string(&format!("Failed to create reader: {}", e));
                return std::ptr::null_mut();
            }
        };

    // Create writer with memory budget
    let writer = match index.writer(cfg.writer_memory_budget_bytes) {
        Ok(w) => w,
        Err(e) => {
            *error_out = create_error_string(&format!("Failed to create writer: {}", e));
            return std::ptr::null_mut();
        }
    };

    // Cache default search fields for backwards compatibility with email schema
    let subject_field = schema.get_field(DEFAULT_SEARCH_FIELD_SUBJECT).ok();
    let body_field = schema.get_field(DEFAULT_SEARCH_FIELD_BODY).ok();

    // Build list of all default search fields
    let mut default_search_fields = Vec::new();
    for field_name in &default_field_names {
        if let Ok(field) = schema.get_field(field_name) {
            default_search_fields.push(field);
        }
    }

    // Cache QueryParser to avoid recreation on every search operation
    // Safe to cache since schema is immutable after index creation
    let query_parser = if !default_search_fields.is_empty() {
        QueryParser::for_index(&index, default_search_fields.clone())
    } else {
        // Fallback to subject/body fields for email schema
        let mut fields = Vec::new();
        if let Some(subject) = subject_field {
            fields.push(subject);
        }
        if let Some(body) = body_field {
            fields.push(body);
        }
        QueryParser::for_index(&index, fields)
    };

    let wrapper = TantivyIndexWrapper {
        index,
        reader,
        writer,
        schema,
        subject_field,
        body_field,
        default_search_fields,
        query_parser,
    };

    Box::into_raw(Box::new(wrapper))
}

// Close a Tantivy index
//
// # Safety
// The caller must ensure:
// - `index` is a valid pointer to a TantivyIndexWrapper (returned by tantivy_index_open)
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
//
// Undefined behavior if:
// - `index` is NULL or points to invalid memory
// - `error_out` is NULL
// - `index` has already been freed
//
// # Memory Ownership
// - Frees: *mut TantivyIndexWrapper (owned by caller)
// - Does NOT free: error_out (caller must free with tantivy_error_free)
//
// # Usage
// Call when done with the index to free all associated memory.
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_close(
    index: *mut TantivyIndexWrapper,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_close");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let _wrapper = Box::from_raw(index);
    // Drop happens automatically
    0
}

// Add or update a document
//
// # Safety
// The caller must ensure:
// - `index` is a valid pointer to a TantivyIndexWrapper (returned by tantivy_index_open)
// - `doc_id` is a valid null-terminated C string
// - `fields` is a valid pointer to an array of DocField structs
// - `num_fields` is the number of fields in the array
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
//
// Undefined behavior if:
// - `index`, `fields`, or `error_out` is NULL
// - `doc_id` is NULL or not null-terminated
// - `num_fields` doesn't match actual array length
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_add_doc(
    index: *mut TantivyIndexWrapper,
    doc_id: *const c_char,
    fields: *const DocField,
    num_fields: size_t,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_add_doc");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &mut *index;

    // Validate field count to prevent resource exhaustion
    if num_fields > MAX_FIELDS_PER_DOCUMENT {
        *error_out = create_error_string(&format!(
            "Too many fields: {} exceeds maximum of {}",
            num_fields, MAX_FIELDS_PER_DOCUMENT
        ));
        return -1;
    }

    let doc_id_str = match c_str_to_string(doc_id) {
        Ok(id) => id,
        Err(_) => {
            *error_out = create_error_string("Invalid doc_id");
            return -1;
        }
    };

    let mut doc = tantivy::doc!();

    // Add document ID
    if let Ok(id_field) = wrapper.schema.get_field("id") {
        doc.add_text(id_field, &doc_id_str);
    }

    // Add fields
    let fields_slice = slice::from_raw_parts(fields, num_fields);
    for field in fields_slice {
        let key = match c_str_to_string(field.key) {
            Ok(k) => k,
            Err(_) => continue,
        };

        let value = match c_str_to_string(field.value) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Validate field value length to prevent memory exhaustion
        if value.len() > MAX_FIELD_VALUE_LENGTH {
            *error_out = create_error_string(&format!(
                "Field '{}' value too large: {} bytes exceeds maximum of {} bytes",
                key,
                value.len(),
                MAX_FIELD_VALUE_LENGTH
            ));
            return -1;
        }

        // Add field to document using shared helper
        match add_field_to_doc(&mut doc, &wrapper.schema, &key, &value, None) {
            Ok(()) => {}
            Err(e) => {
                *error_out = create_error_string(&e);
                return -1;
            }
        }
    }

    // Add document to writer
    match wrapper.writer.add_document(doc) {
        Ok(_) => 0,
        Err(e) => {
            *error_out = create_error_string(&format!("Failed to add document: {}", e));
            -1
        }
    }
}

// Add multiple documents in a batch (more efficient than individual add_doc calls)
//
// # Safety
// The caller must ensure:
// - `index` is a valid pointer to a TantivyIndexWrapper (returned by tantivy_index_open)
// - `documents` is a valid pointer to an array of BatchDocument structs
// - `num_docs` is the number of documents in the array
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
//
// Undefined behavior if:
// - `index`, `documents`, or `error_out` is NULL
// - `num_docs` doesn't match actual array length
// - Document or field pointers are invalid
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_add_docs_batch(
    index: *mut TantivyIndexWrapper,
    documents: *const BatchDocument,
    num_docs: size_t,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_add_docs_batch");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &mut *index;

    if num_docs == 0 {
        return 0;
    }

    let docs_slice = slice::from_raw_parts(documents, num_docs);
    let mut added_count = 0;

    for batch_doc in docs_slice {
        // Validate field count for this document
        if batch_doc.num_fields > MAX_FIELDS_PER_DOCUMENT {
            *error_out = create_error_string(&format!(
                "Document {}: too many fields ({} exceeds maximum of {})",
                added_count, batch_doc.num_fields, MAX_FIELDS_PER_DOCUMENT
            ));
            return added_count as c_int;
        }

        let doc_id_str = match c_str_to_string(batch_doc.doc_id) {
            Ok(id) => id,
            Err(_) => {
                *error_out =
                    create_error_string(&format!("Document {}: invalid doc_id", added_count));
                return added_count as c_int;
            }
        };

        let mut doc = tantivy::doc!();

        // Add document ID
        if let Ok(id_field) = wrapper.schema.get_field("id") {
            doc.add_text(id_field, &doc_id_str);
        }

        // Add fields
        let fields_slice = slice::from_raw_parts(batch_doc.fields, batch_doc.num_fields);
        for field in fields_slice {
            let key = match c_str_to_string(field.key) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let value = match c_str_to_string(field.value) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Validate field value length
            if value.len() > MAX_FIELD_VALUE_LENGTH {
                *error_out = create_error_string(&format!(
                    "Document {}: field '{}' value too large ({} bytes exceeds maximum of {} bytes)",
                    added_count,
                    key,
                    value.len(),
                    MAX_FIELD_VALUE_LENGTH
                ));
                return added_count as c_int;
            }

            // Add field to document using shared helper
            match add_field_to_doc(&mut doc, &wrapper.schema, &key, &value, Some(added_count)) {
                Ok(()) => {}
                Err(e) => {
                    *error_out = create_error_string(&e);
                    return added_count as c_int;
                }
            }
        }

        // Add document to writer
        match wrapper.writer.add_document(doc) {
            Ok(_) => added_count += 1,
            Err(e) => {
                *error_out =
                    create_error_string(&format!("Failed to add document {}: {}", added_count, e));
                return added_count as c_int;
            }
        }
    }

    added_count as c_int
}

// Delete a document
//
// # Safety
// The caller must ensure:
// - `index` is a valid pointer to a TantivyIndexWrapper (returned by tantivy_index_open)
// - `doc_id` is a valid null-terminated C string
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
//
// Undefined behavior if:
// - `index` or `error_out` is NULL
// - `doc_id` is NULL or not null-terminated
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_delete_doc(
    index: *mut TantivyIndexWrapper,
    doc_id: *const c_char,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_delete_doc");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &mut *index;

    let doc_id_str = match c_str_to_string(doc_id) {
        Ok(id) => id,
        Err(_) => {
            *error_out = create_error_string("Invalid doc_id");
            return -1;
        }
    };

    if let Ok(id_field) = wrapper.schema.get_field("id") {
        let term = tantivy::Term::from_field_text(id_field, &doc_id_str);
        wrapper.writer.delete_term(term);
        0
    } else {
        *error_out = create_error_string("ID field not found in schema");
        -1
    }
}

// Batch delete multiple documents
//
// # Memory Ownership
// - Borrows: *mut TantivyIndexWrapper (not freed by this function)
// - Borrows: doc_ids array and strings (not freed by this function)
// - Returns: count of deleted documents (c_int)
// - Allocates: error_out on error (owned by caller, must free with tantivy_error_free)
//
// # Note
// This function calls delete_term internally for each document ID.
// Tantivy doesn't provide a batch delete API, so this is implemented
// as a loop in Rust to reduce FFI round-trips compared to calling
// tantivy_index_delete_doc for each document.
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_delete_docs(
    index: *mut TantivyIndexWrapper,
    doc_ids: *const *const c_char,
    num_ids: usize,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_delete_docs");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &mut *index;

    if doc_ids.is_null() {
        *error_out = create_error_string("doc_ids array is null");
        return -1;
    }

    let id_field = match wrapper.schema.get_field("id") {
        Ok(field) => field,
        Err(_) => {
            *error_out = create_error_string("ID field not found in schema");
            return -1;
        }
    };

    let mut deleted_count: usize = 0;
    let mut error_count: usize = 0;

    // Delete each document ID
    for i in 0..num_ids {
        let doc_id_ptr = *doc_ids.add(i);

        if doc_id_ptr.is_null() {
            error_count += 1;
            continue;
        }

        let doc_id_str = match c_str_to_string(doc_id_ptr) {
            Ok(id) => id,
            Err(_) => {
                error_count += 1;
                continue;
            }
        };

        let term = tantivy::Term::from_field_text(id_field, &doc_id_str);
        wrapper.writer.delete_term(term);
        deleted_count += 1;
    }

    if error_count > 0 {
        let msg = format!("Deleted {} documents with {} errors", deleted_count, error_count);
        *error_out = create_error_string(&msg);
    }

    deleted_count as c_int
}

// Search the index
//
// # Safety
// The caller must ensure:
// - `index` is a valid pointer to a TantivyIndexWrapper (returned by tantivy_index_open)
// - `query` is a valid null-terminated C string
// - `results_out` is a valid, non-null pointer to write result array pointer
// - `num_results_out` is a valid, non-null pointer to write result count
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
//
// Undefined behavior if:
// - `index`, `results_out`, `num_results_out`, or `error_out` is NULL
// - `query` is NULL or not null-terminated
//
// # Memory Ownership
// - Returns: *mut TantivyResult array (owned by caller, must free with tantivy_result_free)
// - Borrows: *mut TantivyIndexWrapper (not freed by this function)
// - Borrows: query string (not freed by this function)
// - Allocates: error_out on error (owned by caller, must free with tantivy_error_free)
//
// # Result Array
// The returned TantivyResult array contains:
// - doc_id: *mut c_char (owned, will be freed by tantivy_result_free)
// - score: f32 (value type, no free needed)
// - highlight: *mut c_char (owned, will be freed by tantivy_result_free, currently NULL)
//
// # Usage
// 1. Call tantivy_index_search to populate results_out and num_results_out
// 2. Read results from the array (use num_results_out to iterate)
// 3. Call tantivy_result_free(results_out, num_results_out) to free memory
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_search(
    index: *mut TantivyIndexWrapper,
    query: *const c_char,
    limit: size_t,
    offset: size_t,
    results_out: *mut *mut TantivyResult,
    num_results_out: *mut size_t,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_search");
        return -1;
    }

    if results_out.is_null() || num_results_out.is_null() {
        *error_out = create_error_string("results_out or num_results_out is null");
        return -1;
    }
    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &*index;

    // Validate search limit to prevent resource exhaustion
    if limit > MAX_SEARCH_LIMIT {
        *error_out = create_error_string(&format!(
            "Search limit too large: {} exceeds maximum of {}",
            limit, MAX_SEARCH_LIMIT
        ));
        return -1;
    }

    let query_str = match c_str_to_string(query) {
        Ok(q) => q,
        Err(_) => {
            *error_out = create_error_string("Invalid query string");
            return -1;
        }
    };

    // Validate query length to prevent DoS attacks
    if query_str.len() > MAX_QUERY_LENGTH {
        *error_out = create_error_string(&format!(
            "Query too long: {} bytes exceeds maximum of {} bytes",
            query_str.len(),
            MAX_QUERY_LENGTH
        ));
        return -1;
    }

    // Get default search fields
    let default_fields = wrapper.get_default_search_fields();

    if default_fields.is_empty() {
        *error_out = create_error_string("No default search fields available");
        return -1;
    }

    // Use cached QueryParser to avoid recreation on every search
    let query_parser = &wrapper.query_parser;

    // Use lenient parsing to support field-specific searches on non-default fields
    // (e.g., from:email@example.com, to:recipient@example.com)
    let (parsed_query, errors) = query_parser.parse_query_lenient(&query_str);

    // Log parse errors for debugging but continue with the partial query
    if !errors.is_empty() {
        eprintln!("Query parse warnings for '{}': {:?}", query_str, errors);
    }

    // Execute search
    // Collect enough documents to handle offset + limit for pagination
    // Use checked_add to prevent integer overflow (CWE-190)
    let searcher = wrapper.reader.searcher();
    let total = match offset.checked_add(limit) {
        Some(t) => t,
        None => {
            *error_out = create_error_string("Offset + limit overflow");
            return -1;
        }
    };

    // Validate total against MAX_SEARCH_LIMIT
    if total > MAX_SEARCH_LIMIT {
        *error_out = create_error_string(&format!(
            "Total results too large: {} exceeds maximum of {}",
            total, MAX_SEARCH_LIMIT
        ));
        return -1;
    }

    let top_docs = match searcher.search(&parsed_query, &TopDocs::with_limit(total)) {
        Ok(docs) => docs,
        Err(e) => {
            *error_out = create_error_string(&format!("Search failed: {}", e));
            return -1;
        }
    };

    // Collect results, skipping offset and taking limit
    let mut results = Vec::new();
    for (score, doc_address) in top_docs.iter().skip(offset).take(limit) {
        if let Ok(retrieved_doc) = searcher.doc::<tantivy::TantivyDocument>(*doc_address) {
            if let Ok(id_field) = wrapper.schema.get_field("id") {
                if let Some(id_value) = retrieved_doc.get_first(id_field) {
                    if let Some(id_str) = id_value.as_str() {
                        let doc_id = id_str.to_string();

                        let result = TantivyResult {
                            doc_id: string_to_c_string(&doc_id),
                            score: *score,
                            highlight: std::ptr::null_mut(),
                        };
                        results.push(result);
                    }
                }
            }
        }
    }

    // Transfer ownership to C using ManuallyDrop for safety
    // The caller MUST call tantivy_result_free() to avoid memory leaks
    let (results_ptr, results_len) = vec_into_raw_parts(results);

    *results_out = results_ptr;
    *num_results_out = results_len;

    0
}

// Get document by ID
// Returns a JSON string containing all document fields, or NULL if not found
//
// # Memory Ownership
// - Returns: *mut c_char JSON string (owned by caller, must free with tantivy_string_free)
// - Borrows: *mut TantivyIndexWrapper (not freed by this function)
// - Borrows: doc_id string (not freed by this function)
// - Allocates: error_out on error (owned by caller, must free with tantivy_error_free)
//
// # Return Value
// Returns JSON string with all document fields, or NULL if document not found.
// The caller is responsible for freeing the returned string using tantivy_string_free.
//
// # Example Output
// {"id":"doc-123","subject":"Test","body":"Content","from":"alice@example.com","score":0.95}
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_get_doc(
    index: *mut TantivyIndexWrapper,
    doc_id: *const c_char,
    error_out: *mut *mut c_char,
) -> *mut c_char {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_get_doc");
        return std::ptr::null_mut();
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return std::ptr::null_mut();
    }

    let wrapper = &*index;

    let id_str = match c_str_to_string(doc_id) {
        Ok(id) => id,
        Err(_) => {
            *error_out = create_error_string("Invalid doc_id string");
            return std::ptr::null_mut();
        }
    };

    // Use Term-based query to prevent query injection (CWE-943)
    // Term queries are safe from injection as they don't use QueryParser
    let id_field = match wrapper.schema.get_field("id") {
        Ok(field) => field,
        Err(e) => {
            *error_out = create_error_string(&format!("ID field not found: {}", e));
            return std::ptr::null_mut();
        }
    };

    let term_query = tantivy::query::TermQuery::new(
        tantivy::Term::from_field_text(id_field, &id_str),
        IndexRecordOption::Basic,
    );

    let searcher = wrapper.reader.searcher();
    let top_docs = match searcher.search(&term_query, &TopDocs::with_limit(1)) {
        Ok(docs) => docs,
        Err(e) => {
            *error_out = create_error_string(&format!("Get doc failed: {}", e));
            return std::ptr::null_mut();
        }
    };

    // Get the first (and only) result
    for (score, doc_address) in top_docs.iter() {
        if let Ok(retrieved_doc) = searcher.doc::<tantivy::TantivyDocument>(*doc_address) {
            // Convert document to JSON map
            let json_map = document_to_json_map(&retrieved_doc, &wrapper.schema, Some(*score));

            // Convert to JSON string
            let json_str = match serde_json::to_string(&json_map) {
                Ok(j) => j,
                Err(e) => {
                    *error_out = create_error_string(&format!("JSON serialization failed: {}", e));
                    return std::ptr::null_mut();
                }
            };

            return string_to_c_string(&json_str);
        }
    }

    // Document not found
    std::ptr::null_mut()
}

// Commit pending changes
//
// # Safety
// The caller must ensure:
// - `index` is a valid pointer to a TantivyIndexWrapper (returned by tantivy_index_open)
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
//
// Undefined behavior if:
// - `index` or `error_out` is NULL
//
// # Memory Ownership
// - Borrows: *mut TantivyIndexWrapper (not freed by this function)
// - Allocates: error_out on error (owned by caller, must free with tantivy_error_free)
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_commit(
    index: *mut TantivyIndexWrapper,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_commit");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &mut *index;

    match wrapper.writer.commit() {
        Ok(_) => {
            // Trigger reader reload
            let _ = wrapper.reader.reload();
            0
        }
        Err(e) => {
            *error_out = create_error_string(&format!("Commit failed: {}", e));
            -1
        }
    }
}

// Rollback pending changes
//
// # Safety
// The caller must ensure:
// - `index` is a valid pointer to a TantivyIndexWrapper (returned by tantivy_index_open)
// - `error_out` is a valid, non-null pointer to a pointer where an error can be stored
//
// Undefined behavior if:
// - `index` or `error_out` is NULL
//
// # Memory Ownership
// - Borrows: *mut TantivyIndexWrapper (not freed by this function)
// - Allocates: error_out on error (owned by caller, must free with tantivy_error_free)
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_rollback(
    index: *mut TantivyIndexWrapper,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_rollback");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &mut *index;

    match wrapper.writer.rollback() {
        Ok(_) => 0,
        Err(e) => {
            *error_out = create_error_string(&format!("Rollback failed: {}", e));
            -1
        }
    }
}

// Free search results
//
// # Safety
// The caller must ensure:
// - `results` is a valid pointer to a TantivyResult array (returned by tantivy_index_search)
// - `num_results` matches the number of results in the array
//
// Undefined behavior if:
// - `results` is NULL (function handles this gracefully)
// - `num_results` doesn't match actual array length
// - Results or strings within have already been freed
//
// # Memory Ownership
// - Frees: TantivyResult array and all contained strings (doc_id, highlight)
//
// # Usage
// Call this after processing search results to free all allocated memory.
#[no_mangle]
pub unsafe extern "C" fn tantivy_result_free(results: *mut TantivyResult, num_results: size_t) {
    if results.is_null() {
        return;
    }

    let results_slice = slice::from_raw_parts_mut(results, num_results);
    for result in results_slice {
        if !result.doc_id.is_null() {
            let _ = CString::from_raw(result.doc_id);
        }
        if !result.highlight.is_null() {
            let _ = CString::from_raw(result.highlight);
        }
    }
    let _ = Vec::from_raw_parts(results, num_results, num_results);
}

// Free error string
//
// # Safety
// The caller must ensure:
// - `error` is a valid pointer to a null-terminated C string (returned by FFI functions)
//
// Undefined behavior if:
// - `error` is NULL (function handles this gracefully)
// - `error` has already been freed
// - `error` was not allocated by this library's FFI functions
//
// # Memory Ownership
// - Frees: error string (owned by caller, allocated by FFI functions)
#[no_mangle]
pub unsafe extern "C" fn tantivy_error_free(error: *mut c_char) {
    if !error.is_null() {
        let _ = CString::from_raw(error);
    }
}

// Free string returned by tantivy_index_get_doc
//
// # Safety
// The caller must ensure:
// - `s` is a valid pointer to a null-terminated C string (returned by tantivy_index_get_doc)
//
// Undefined behavior if:
// - `s` is NULL (function handles this gracefully)
// - `s` has already been freed
// - `s` was not allocated by this library's FFI functions
//
// # Memory Ownership
// - Frees: string (owned by caller, allocated by tantivy_index_get_doc)
#[no_mangle]
pub unsafe extern "C" fn tantivy_string_free(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

// Get multiple documents by their IDs
// Missing documents are represented as empty strings in the array
#[no_mangle]
pub unsafe extern "C" fn tantivy_index_get_docs(
    index: *mut TantivyIndexWrapper,
    doc_ids: *const *const c_char,
    num_ids: size_t,
    num_results_out: *mut size_t,
    error_out: *mut *mut c_char,
) -> *mut *mut c_char {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_index_get_docs");
        return std::ptr::null_mut();
    }

    if num_results_out.is_null() {
        *error_out = create_error_string("num_results_out is null");
        return std::ptr::null_mut();
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return std::ptr::null_mut();
    }

    let wrapper = &*index;

    if num_ids == 0 {
        *num_results_out = 0;
        return std::ptr::null_mut();
    }

    let ids_slice = slice::from_raw_parts(doc_ids, num_ids);
    let mut results = Vec::new();

    let id_field = match wrapper.schema.get_field("id") {
        Ok(field) => field,
        Err(e) => {
            *error_out = create_error_string(&format!("ID field not found: {}", e));
            return std::ptr::null_mut();
        }
    };

    let searcher = wrapper.reader.searcher();

    for &id_ptr in ids_slice {
        let id_str = match c_str_to_string(id_ptr) {
            Ok(id) => id,
            Err(_) => {
                results.push(std::ptr::null_mut());
                continue;
            }
        };

        // Use Term-based query to prevent query injection (CWE-943)
        let term_query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(id_field, &id_str),
            IndexRecordOption::Basic,
        );

        let top_docs = match searcher.search(&term_query, &TopDocs::with_limit(1)) {
            Ok(docs) => docs,
            Err(_) => {
                results.push(std::ptr::null_mut());
                continue;
            }
        };

        let mut json_str = std::ptr::null_mut();
        for (_score, doc_address) in top_docs.iter() {
            if let Ok(retrieved_doc) = searcher.doc::<tantivy::TantivyDocument>(*doc_address) {
                // Convert document to JSON map (without score)
                let json_map = document_to_json_map(&retrieved_doc, &wrapper.schema, None);

                if let Ok(j) = serde_json::to_string(&json_map) {
                    json_str = string_to_c_string(&j)
                }
            }
        }

        results.push(json_str);
    }

    // Transfer ownership to C using ManuallyDrop for safety
    // The caller MUST call tantivy_docs_array_free() to avoid memory leaks
    let (results_ptr, results_len) = vec_into_raw_parts(results);

    *num_results_out = results_len;
    results_ptr
}

// Free multiple documents array
#[no_mangle]
pub unsafe extern "C" fn tantivy_docs_array_free(docs: *mut *mut c_char, num_docs: size_t) {
    if docs.is_null() {
        return;
    }

    let docs_slice = slice::from_raw_parts_mut(docs, num_docs);
    for doc_ptr in docs_slice {
        if !doc_ptr.is_null() {
            let _ = CString::from_raw(*doc_ptr);
        }
    }
    let _ = Vec::from_raw_parts(docs, num_docs, num_docs);
}

// Perform a terms aggregation (count documents by field values)
// Returns an array of aggregation results sorted by count (descending)
#[no_mangle]
pub unsafe extern "C" fn tantivy_aggregate_terms(
    index: *mut TantivyIndexWrapper,
    field_name: *const c_char,
    query: *const c_char,
    limit: size_t,
    results_out: *mut *mut TantivyAggregationResult,
    num_results_out: *mut size_t,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_aggregate_terms");
        return -1;
    }

    if results_out.is_null() || num_results_out.is_null() {
        *error_out = create_error_string("results_out or num_results_out is null");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &*index;

    // Validate aggregation limit to prevent resource exhaustion
    if limit > MAX_SEARCH_LIMIT {
        *error_out = create_error_string(&format!(
            "Aggregation limit too large: {} exceeds maximum of {}",
            limit, MAX_SEARCH_LIMIT
        ));
        return -1;
    }

    let field_str = match c_str_to_string(field_name) {
        Ok(f) => f,
        Err(_) => {
            *error_out = create_error_string("Invalid field name");
            return -1;
        }
    };

    let field = match wrapper.schema.get_field(&field_str) {
        Ok(f) => f,
        Err(e) => {
            *error_out = create_error_string(&format!("Field not found: {}", e));
            return -1;
        }
    };

    let query_str = match c_str_to_string(query) {
        Ok(q) => q,
        Err(_) => {
            *error_out = create_error_string("Invalid query string");
            return -1;
        }
    };

    // Validate query length to prevent DoS attacks
    if query_str.len() > MAX_QUERY_LENGTH {
        *error_out = create_error_string(&format!(
            "Query too long: {} bytes exceeds maximum of {} bytes",
            query_str.len(),
            MAX_QUERY_LENGTH
        ));
        return -1;
    }

    // Get default search fields
    let default_fields = wrapper.get_default_search_fields();

    if default_fields.is_empty() {
        *error_out = create_error_string("No default search fields available");
        return -1;
    }

    // Use cached QueryParser to avoid recreation on every aggregation
    let query_parser = &wrapper.query_parser;
    let (parsed_query, _errors) = query_parser.parse_query_lenient(&query_str);

    let searcher = wrapper.reader.searcher();

    let search_limit = if limit == 0 { 10000 } else { limit };
    let mut counter: std::collections::HashMap<String, u64> =
        std::collections::HashMap::with_capacity(search_limit);
    match searcher.search(&parsed_query, &TopDocs::with_limit(search_limit)) {
        Ok(top_docs) => {
            for (_score, doc_address) in top_docs.iter() {
                if let Ok(retrieved_doc) = searcher.doc::<tantivy::TantivyDocument>(*doc_address) {
                    if let Some(field_value) = retrieved_doc.get_first(field) {
                        if let Some(value) = field_value.as_str() {
                            *counter.entry(value.to_string()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
        Err(e) => {
            *error_out = create_error_string(&format!("Search failed: {}", e));
            return -1;
        }
    }

    let mut mut_results: Vec<(String, u64)> = counter.into_iter().collect();
    mut_results.sort_by(|a, b| b.1.cmp(&a.1));

    if limit > 0 && mut_results.len() > limit {
        mut_results.truncate(limit);
    }

    let results: Vec<TantivyAggregationResult> = mut_results
        .into_iter()
        .map(|(key, count)| TantivyAggregationResult { key: string_to_c_string(&key), count })
        .collect();

    // Transfer ownership to C using ManuallyDrop for safety
    // The caller MUST call tantivy_suggestions_free() to avoid memory leaks
    let (results_ptr, results_len) = vec_into_raw_parts(results);

    *results_out = results_ptr;
    *num_results_out = results_len;

    0
}

// Free aggregation results
#[no_mangle]
pub unsafe extern "C" fn tantivy_aggregation_results_free(
    results: *mut TantivyAggregationResult,
    num_results: size_t,
) {
    if results.is_null() {
        return;
    }

    let results_slice = slice::from_raw_parts_mut(results, num_results);
    for result in results_slice {
        if !result.key.is_null() {
            let _ = CString::from_raw(result.key);
        }
    }
    let _ = Vec::from_raw_parts(results, num_results, num_results);
}

// Helper function to convert suggestions HashMap to sorted TantivySuggestion array
fn convert_suggestions_to_results(
    suggestions: std::collections::HashMap<String, f32>,
) -> Vec<TantivySuggestion> {
    let mut sorted_suggestions: Vec<(String, f32)> = suggestions.into_iter().collect();
    sorted_suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    sorted_suggestions
        .into_iter()
        .map(|(text, score)| TantivySuggestion { text: string_to_c_string(&text), score })
        .collect()
}

// Autocomplete: Get suggestions for a prefix query
#[no_mangle]
pub unsafe extern "C" fn tantivy_autocomplete(
    index: *mut TantivyIndexWrapper,
    field: *const c_char,
    prefix: *const c_char,
    limit: size_t,
    results_out: *mut *mut TantivySuggestion,
    num_results_out: *mut size_t,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_autocomplete");
        return -1;
    }

    if results_out.is_null() || num_results_out.is_null() {
        *error_out = create_error_string("results_out or num_results_out is null");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &*index;

    // Validate autocomplete limit to prevent resource exhaustion
    if limit > MAX_SEARCH_LIMIT {
        *error_out = create_error_string(&format!(
            "Autocomplete limit too large: {} exceeds maximum of {}",
            limit, MAX_SEARCH_LIMIT
        ));
        return -1;
    }

    let field_str = match c_str_to_string(field) {
        Ok(f) => f,
        Err(_) => {
            *error_out = create_error_string("Invalid field name");
            return -1;
        }
    };

    let field_entry = match wrapper.schema.get_field(&field_str) {
        Ok(f) => f,
        Err(e) => {
            *error_out = create_error_string(&format!("Field not found: {}", e));
            return -1;
        }
    };

    let prefix_str = match c_str_to_string(prefix) {
        Ok(p) => p,
        Err(_) => {
            *error_out = create_error_string("Invalid prefix string");
            return -1;
        }
    };

    // Validate prefix length to prevent DoS attacks
    if prefix_str.len() > MAX_QUERY_LENGTH {
        *error_out = create_error_string(&format!(
            "Prefix too long: {} bytes exceeds maximum of {} bytes",
            prefix_str.len(),
            MAX_QUERY_LENGTH
        ));
        return -1;
    }

    // Escape prefix string to prevent query injection (CWE-78)
    let escaped_prefix = escape_query_string(&prefix_str);

    // Use wildcard query for efficient prefix matching (better than exact TermQuery for autocomplete)
    // The wildcard character (*) is appended AFTER escaping to maintain prefix functionality
    let query_str = format!("{}:{}*", field_str, escaped_prefix);

    // Use cached QueryParser with lenient parsing (supports field-specific queries)
    let query_parser = &wrapper.query_parser;
    let (prefix_query, errors) = query_parser.parse_query_lenient(&query_str);

    // Log parse errors for debugging but continue with the partial query
    if !errors.is_empty() {
        eprintln!("Autocomplete parse warnings for '{}': {:?}", query_str, errors);
    }

    let searcher = wrapper.reader.searcher();
    let top_docs = match searcher.search(&prefix_query, &TopDocs::with_limit(limit)) {
        Ok(docs) => docs,
        Err(e) => {
            *error_out = create_error_string(&format!("Autocomplete search failed: {}", e));
            return -1;
        }
    };

    let mut suggestions: std::collections::HashMap<String, f32> =
        std::collections::HashMap::with_capacity(limit);

    for (score, doc_address) in top_docs.iter() {
        if let Ok(retrieved_doc) = searcher.doc::<tantivy::TantivyDocument>(*doc_address) {
            if let Some(field_value) = retrieved_doc.get_first(field_entry) {
                if let Some(value) = field_value.as_str() {
                    let suggestion = value.to_string();
                    let current_score = *suggestions.get(&suggestion).unwrap_or(&0.0);
                    if *score > current_score {
                        suggestions.insert(suggestion, *score);
                    }
                }
            }
        }
    }

    let results = convert_suggestions_to_results(suggestions);

    // Transfer ownership to C using ManuallyDrop for safety
    // The caller MUST call tantivy_suggestions_free() to avoid memory leaks
    let (results_ptr, results_len) = vec_into_raw_parts(results);

    *results_out = results_ptr;
    *num_results_out = results_len;

    0
}

// Did-you-mean: Get similar terms using fuzzy query
#[no_mangle]
pub unsafe extern "C" fn tantivy_did_you_mean(
    index: *mut TantivyIndexWrapper,
    field: *const c_char,
    term: *const c_char,
    distance: u8,
    limit: size_t,
    results_out: *mut *mut TantivySuggestion,
    num_results_out: *mut size_t,
    error_out: *mut *mut c_char,
) -> c_int {
    if error_out.is_null() {
        eprintln!("[TANTIVY_FFI] error_out is NULL in tantivy_did_you_mean");
        return -1;
    }

    if results_out.is_null() || num_results_out.is_null() {
        *error_out = create_error_string("results_out or num_results_out is null");
        return -1;
    }

    if index.is_null() {
        *error_out = create_error_string("Index is null");
        return -1;
    }

    let wrapper = &*index;

    // Validate fuzzy search limit to prevent resource exhaustion
    if limit > MAX_SEARCH_LIMIT {
        *error_out = create_error_string(&format!(
            "Fuzzy search limit too large: {} exceeds maximum of {}",
            limit, MAX_SEARCH_LIMIT
        ));
        return -1;
    }

    let field_str = match c_str_to_string(field) {
        Ok(f) => f,
        Err(_) => {
            *error_out = create_error_string("Invalid field name");
            return -1;
        }
    };

    let field_entry = match wrapper.schema.get_field(&field_str) {
        Ok(f) => f,
        Err(e) => {
            *error_out = create_error_string(&format!("Field not found: {}", e));
            return -1;
        }
    };

    let term_str = match c_str_to_string(term) {
        Ok(t) => t,
        Err(_) => {
            *error_out = create_error_string("Invalid term string");
            return -1;
        }
    };

    // Validate term length to prevent DoS attacks
    if term_str.len() > MAX_QUERY_LENGTH {
        *error_out = create_error_string(&format!(
            "Term too long: {} bytes exceeds maximum of {} bytes",
            term_str.len(),
            MAX_QUERY_LENGTH
        ));
        return -1;
    }

    let fuzzy_term = tantivy::Term::from_field_text(field_entry, &term_str);
    let fuzzy_query = tantivy::query::FuzzyTermQuery::new(fuzzy_term, distance, true);

    let searcher = wrapper.reader.searcher();
    let top_docs = match searcher.search(&fuzzy_query, &TopDocs::with_limit(limit)) {
        Ok(docs) => docs,
        Err(e) => {
            *error_out = create_error_string(&format!("Fuzzy search failed: {}", e));
            return -1;
        }
    };

    let mut suggestions: std::collections::HashMap<String, f32> =
        std::collections::HashMap::with_capacity(limit);

    for (score, doc_address) in top_docs.iter() {
        if let Ok(retrieved_doc) = searcher.doc::<tantivy::TantivyDocument>(*doc_address) {
            if let Some(field_value) = retrieved_doc.get_first(field_entry) {
                if let Some(value) = field_value.as_str() {
                    let suggestion = value.to_string();
                    let current_score = *suggestions.get(&suggestion).unwrap_or(&0.0);
                    if *score > current_score {
                        suggestions.insert(suggestion, *score);
                    }
                }
            }
        }
    }

    let results = convert_suggestions_to_results(suggestions);

    // Transfer ownership to C using ManuallyDrop for safety
    // The caller MUST call tantivy_suggestions_free() to avoid memory leaks
    let (results_ptr, results_len) = vec_into_raw_parts(results);

    *results_out = results_ptr;
    *num_results_out = results_len;

    0
}

// Free suggestions results
#[no_mangle]
pub unsafe extern "C" fn tantivy_suggestions_free(
    results: *mut TantivySuggestion,
    num_results: size_t,
) {
    if results.is_null() {
        return;
    }

    let results_slice = slice::from_raw_parts_mut(results, num_results);
    for result in results_slice {
        if !result.text.is_null() {
            let _ = CString::from_raw(result.text);
        }
    }
    let _ = Vec::from_raw_parts(results, num_results, num_results);
}

// Get facet counts for a field
// Wrapper around tantivy_aggregate_terms with a fixed limit of 1000
#[no_mangle]
pub unsafe extern "C" fn tantivy_get_facet_counts(
    index: *mut TantivyIndexWrapper,
    field_name: *const c_char,
    query: *const c_char,
    results_out: *mut *mut TantivyAggregationResult,
    num_results_out: *mut size_t,
    error_out: *mut *mut c_char,
) -> c_int {
    tantivy_aggregate_terms(index, field_name, query, 1000, results_out, num_results_out, error_out)
}

// Simple ping function to check if the FFI library is loaded
// Returns 1 if the library is available, 0 otherwise
// This is much more efficient than creating an index just to check availability
#[no_mangle]
pub unsafe extern "C" fn tantivy_ping() -> isize {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_ffi_struct_layouts() {
        println!("=== FFI Struct Layouts ===");
        println!();

        println!("TantivyConfig:");
        println!("  size: {}", mem::size_of::<TantivyConfig>());
        println!("  offset(index_path): {}", offset_of!(TantivyConfig, index_path));
        println!(
            "  offset(reader_memory_budget_bytes): {}",
            offset_of!(TantivyConfig, reader_memory_budget_bytes)
        );
        println!(
            "  offset(writer_memory_budget_bytes): {}",
            offset_of!(TantivyConfig, writer_memory_budget_bytes)
        );
        println!("  offset(num_threads): {}", offset_of!(TantivyConfig, num_threads));
        println!("  offset(schema_json): {}", offset_of!(TantivyConfig, schema_json));
        println!();

        println!("TantivyResult:");
        println!("  size: {}", mem::size_of::<TantivyResult>());
        println!("  offset(doc_id): {}", offset_of!(TantivyResult, doc_id));
        println!("  offset(score): {}", offset_of!(TantivyResult, score));
        println!("  offset(highlight): {}", offset_of!(TantivyResult, highlight));
        println!();

        println!("TantivyAggregationResult:");
        println!("  size: {}", mem::size_of::<TantivyAggregationResult>());
        println!("  offset(key): {}", offset_of!(TantivyAggregationResult, key));
        println!("  offset(count): {}", offset_of!(TantivyAggregationResult, count));
        println!();

        println!("TantivySuggestion:");
        println!("  size: {}", mem::size_of::<TantivySuggestion>());
        println!("  offset(text): {}", offset_of!(TantivySuggestion, text));
        println!("  offset(score): {}", offset_of!(TantivySuggestion, score));
        println!();

        println!("DocField:");
        println!("  size: {}", mem::size_of::<DocField>());
        println!("  offset(key): {}", offset_of!(DocField, key));
        println!("  offset(value): {}", offset_of!(DocField, value));
        println!();

        println!("BatchDocument:");
        println!("  size: {}", mem::size_of::<BatchDocument>());
        println!("  offset(doc_id): {}", offset_of!(BatchDocument, doc_id));
        println!("  offset(fields): {}", offset_of!(BatchDocument, fields));
        println!("  offset(num_fields): {}", offset_of!(BatchDocument, num_fields));
        println!();

        println!("Pointer size (usize): {}", mem::size_of::<usize>());
    }

    #[test]
    fn test_sanitize_error_message_simple_path() {
        let msg = "Failed to open index at /var/lib/tantivy/index";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Failed to open index at /var/lib/tantivy/index");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Failed to open index at <path>");
    }

    #[test]
    fn test_sanitize_error_message_windows_path() {
        let msg = "Error reading file at /home/user/tantivy/data/index";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Error reading file at /home/user/tantivy/data/index");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error reading file at <path>");
    }

    #[test]
    fn test_sanitize_error_message_multiple_paths() {
        let msg = "Index at /var/lib/tantivy/index failed, trying /tmp/backup";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Index at /var/lib/tantivy/index failed, trying /tmp/backup");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Index at <path> failed, trying <path>");
    }

    #[test]
    fn test_sanitize_error_message_no_path() {
        let msg = "Invalid configuration provided";
        let sanitized = sanitize_error_message(msg);

        assert_eq!(sanitized, "Invalid configuration provided");
    }

    #[test]
    fn test_sanitize_error_message_empty_path() {
        let msg = "Error at / with no directory";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Error at / with no directory");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error at / with no directory");
    }

    #[test]
    fn test_sanitize_error_message_path_with_quotes() {
        let msg = "Error at \"/var/lib/tantivy/index\"";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Error at \"/var/lib/tantivy/index\"");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error at \"<path>\"");
    }

    #[test]
    fn test_sanitize_error_message_path_with_colon() {
        let msg = "Failed at /path/to/index: Error occurred";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Failed at /path/to/index: Error occurred");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Failed at <path>: Error occurred");
    }

    #[test]
    fn test_sanitize_error_message_with_whitespace() {
        let msg = "Error at /var/lib/tantivy/index space after path";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Error at /var/lib/tantivy/index space after path");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error at <path> space after path");
    }

    #[test]
    fn test_sanitize_error_message_no_leading_slash() {
        let msg = "Error: could not open path relative/path/index";
        let sanitized = sanitize_error_message(msg);

        assert_eq!(sanitized, "Error: could not open path relative/path/index");
    }

    #[test]
    fn test_sanitize_flag_debug_builds() {
        #[cfg(debug_assertions)]
        assert!(!SANITIZE_ERRORS, "Debug builds should not sanitize errors");

        #[cfg(not(debug_assertions))]
        assert!(SANITIZE_ERRORS, "Release builds should sanitize errors");
    }

    #[test]
    fn test_create_error_string_logs_to_stderr() {
        let _ = create_error_string("Test error message");
    }

    #[test]
    fn test_sanitize_preserves_non_path_content() {
        let msg = "Failed to open index at /var/lib/tantivy/index: Permission denied";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Failed to open index at /var/lib/tantivy/index: Permission denied");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Failed to open index at <path>: Permission denied");
    }

    #[test]
    fn test_sanitize_with_deep_path() {
        let msg = "Error at /usr/local/lib/tantivy/data/backup/index";
        let sanitized = sanitize_error_message(msg);

        #[cfg(debug_assertions)]
        assert_eq!(sanitized, "Error at /usr/local/lib/tantivy/data/backup/index");

        #[cfg(not(debug_assertions))]
        assert_eq!(sanitized, "Error at <path>");
    }
}
