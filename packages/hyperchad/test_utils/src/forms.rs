//! Form handling utilities for test workflows.
//!
//! This module provides structures and methods for interacting with HTML forms
//! in test scenarios, including field filling, option selection, and file uploads.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

/// Form data containing field values for form filling operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormData {
    /// Map of field names to their values.
    pub fields: BTreeMap<String, FormValue>,
}

impl FormData {
    /// Creates a new empty form data builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fields: BTreeMap::new(),
        }
    }

    /// Adds a text field to the form data.
    #[must_use]
    pub fn text(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields
            .insert(name.into(), FormValue::Text(value.into()));
        self
    }

    /// Adds a numeric field to the form data.
    #[must_use]
    pub fn number(mut self, name: impl Into<String>, value: f64) -> Self {
        self.fields.insert(name.into(), FormValue::Number(value));
        self
    }

    /// Adds a boolean checkbox field to the form data.
    #[must_use]
    pub fn boolean(mut self, name: impl Into<String>, value: bool) -> Self {
        self.fields.insert(name.into(), FormValue::Boolean(value));
        self
    }

    /// Adds a single-select dropdown field to the form data.
    #[must_use]
    pub fn select(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields
            .insert(name.into(), FormValue::Select(value.into()));
        self
    }

    /// Adds a multi-select field to the form data.
    #[must_use]
    pub fn multi_select(mut self, name: impl Into<String>, values: Vec<String>) -> Self {
        self.fields
            .insert(name.into(), FormValue::MultiSelect(values));
        self
    }

    /// Adds a file upload field to the form data.
    #[must_use]
    pub fn file(mut self, name: impl Into<String>, path: impl Into<std::path::PathBuf>) -> Self {
        self.fields
            .insert(name.into(), FormValue::File(path.into()));
        self
    }

    /// Adds a field with a custom value to the form data.
    #[must_use]
    pub fn field(mut self, name: impl Into<String>, value: FormValue) -> Self {
        self.fields.insert(name.into(), value);
        self
    }
}

impl Default for FormData {
    fn default() -> Self {
        Self::new()
    }
}

/// A value for a form field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FormValue {
    /// Text input value.
    Text(String),
    /// Numeric input value.
    Number(f64),
    /// Boolean checkbox value.
    Boolean(bool),
    /// Single select dropdown value.
    Select(String),
    /// Multiple select values.
    MultiSelect(Vec<String>),
    /// File path for file upload.
    File(PathBuf),
}

impl FormValue {
    /// Converts the form value to a string representation.
    #[must_use]
    pub fn as_string(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Boolean(b) => b.to_string(),
            Self::Text(s) | Self::Select(s) => s.clone(),
            Self::MultiSelect(v) => v.join(","),
            Self::File(p) => p.to_string_lossy().to_string(),
        }
    }
}

/// A form interaction step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FormStep {
    /// Fill multiple form fields at once.
    FillForm {
        /// Form data containing field values.
        data: FormData,
    },
    /// Fill a single form field.
    FillField {
        /// CSS selector for the field.
        selector: String,
        /// Value to fill in the field.
        value: FormValue,
    },
    /// Select an option from a dropdown.
    SelectOption {
        /// CSS selector for the dropdown.
        selector: String,
        /// Value of the option to select.
        value: String,
    },
    /// Upload a file to a file input.
    UploadFile {
        /// CSS selector for the file input.
        selector: String,
        /// Path to the file to upload.
        file_path: PathBuf,
    },
    /// Submit a form.
    SubmitForm {
        /// CSS selector for the form.
        selector: String,
    },
    /// Reset a form to its initial state.
    ResetForm {
        /// CSS selector for the form.
        selector: String,
    },
}

impl FormStep {
    /// Returns a human-readable description of this form step.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::FillForm { data } => format!("Fill form with {} fields", data.fields.len()),
            Self::FillField { selector, value } => {
                format!("Fill field {} with {}", selector, value.as_string())
            }
            Self::SelectOption { selector, value } => {
                format!("Select option {value} in {selector}")
            }
            Self::UploadFile {
                selector,
                file_path,
            } => format!("Upload file {} to {selector}", file_path.display()),
            Self::SubmitForm { selector } => format!("Submit form {selector}"),
            Self::ResetForm { selector } => format!("Reset form {selector}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn test_form_data_new() {
        let form = FormData::new();
        assert_eq!(form.fields.len(), 0);
    }

    #[test]
    fn test_form_data_default() {
        let form = FormData::default();
        assert_eq!(form.fields.len(), 0);
    }

    #[test]
    fn test_form_data_text() {
        let form = FormData::new()
            .text("username", "alice")
            .text("email", "alice@example.com");

        assert_eq!(form.fields.len(), 2);
        match form.fields.get("username") {
            Some(FormValue::Text(value)) => assert_eq!(value, "alice"),
            _ => panic!("Expected text value"),
        }
    }

    #[test]
    fn test_form_data_number() {
        let form = FormData::new().number("age", 25.5);

        assert_eq!(form.fields.len(), 1);
        match form.fields.get("age") {
            Some(FormValue::Number(value)) => assert_eq!(*value, 25.5),
            _ => panic!("Expected number value"),
        }
    }

    #[test]
    fn test_form_data_boolean() {
        let form = FormData::new().boolean("subscribe", true);

        assert_eq!(form.fields.len(), 1);
        match form.fields.get("subscribe") {
            Some(FormValue::Boolean(value)) => assert!(*value),
            _ => panic!("Expected boolean value"),
        }
    }

    #[test]
    fn test_form_data_select() {
        let form = FormData::new().select("country", "US");

        assert_eq!(form.fields.len(), 1);
        match form.fields.get("country") {
            Some(FormValue::Select(value)) => assert_eq!(value, "US"),
            _ => panic!("Expected select value"),
        }
    }

    #[test]
    fn test_form_data_multi_select() {
        let options = vec!["option1".to_string(), "option2".to_string()];
        let form = FormData::new().multi_select("interests", options.clone());

        assert_eq!(form.fields.len(), 1);
        match form.fields.get("interests") {
            Some(FormValue::MultiSelect(values)) => assert_eq!(values, &options),
            _ => panic!("Expected multi-select value"),
        }
    }

    #[test]
    fn test_form_data_file() {
        let path = PathBuf::from("/tmp/test.txt");
        let form = FormData::new().file("attachment", path.clone());

        assert_eq!(form.fields.len(), 1);
        match form.fields.get("attachment") {
            Some(FormValue::File(file_path)) => assert_eq!(file_path, &path),
            _ => panic!("Expected file value"),
        }
    }

    #[test]
    fn test_form_data_field() {
        let form = FormData::new().field("custom", FormValue::Number(42.0));

        assert_eq!(form.fields.len(), 1);
        match form.fields.get("custom") {
            Some(FormValue::Number(value)) => assert_eq!(*value, 42.0),
            _ => panic!("Expected number value"),
        }
    }

    #[test]
    fn test_form_data_builder_chaining() {
        let form = FormData::new()
            .text("name", "Bob")
            .number("age", 30.0)
            .boolean("active", true)
            .select("role", "admin");

        assert_eq!(form.fields.len(), 4);
    }

    #[test]
    fn test_form_value_as_string_text() {
        let value = FormValue::Text("hello".to_string());
        assert_eq!(value.as_string(), "hello");
    }

    #[test]
    fn test_form_value_as_string_number() {
        let value = FormValue::Number(42.5);
        assert_eq!(value.as_string(), "42.5");
    }

    #[test]
    fn test_form_value_as_string_boolean() {
        let value = FormValue::Boolean(true);
        assert_eq!(value.as_string(), "true");

        let value = FormValue::Boolean(false);
        assert_eq!(value.as_string(), "false");
    }

    #[test]
    fn test_form_value_as_string_select() {
        let value = FormValue::Select("option1".to_string());
        assert_eq!(value.as_string(), "option1");
    }

    #[test]
    fn test_form_value_as_string_multi_select() {
        let value = FormValue::MultiSelect(vec![
            "opt1".to_string(),
            "opt2".to_string(),
            "opt3".to_string(),
        ]);
        assert_eq!(value.as_string(), "opt1,opt2,opt3");
    }

    #[test]
    fn test_form_value_as_string_file() {
        let value = FormValue::File(PathBuf::from("/path/to/file.txt"));
        assert!(value.as_string().contains("file.txt"));
    }

    #[test]
    fn test_form_step_fill_form_description() {
        let form_data = FormData::new()
            .text("field1", "value1")
            .text("field2", "value2");
        let step = FormStep::FillForm { data: form_data };
        assert_eq!(step.description(), "Fill form with 2 fields");
    }

    #[test]
    fn test_form_step_fill_field_description() {
        let step = FormStep::FillField {
            selector: "#email".to_string(),
            value: FormValue::Text("test@example.com".to_string()),
        };
        assert_eq!(
            step.description(),
            "Fill field #email with test@example.com"
        );
    }

    #[test]
    fn test_form_step_select_option_description() {
        let step = FormStep::SelectOption {
            selector: "#country".to_string(),
            value: "Canada".to_string(),
        };
        assert_eq!(step.description(), "Select option Canada in #country");
    }

    #[test]
    fn test_form_step_upload_file_description() {
        let step = FormStep::UploadFile {
            selector: "#file-input".to_string(),
            file_path: PathBuf::from("/uploads/doc.pdf"),
        };
        let desc = step.description();
        assert!(desc.contains("Upload file"));
        assert!(desc.contains("doc.pdf"));
        assert!(desc.contains("#file-input"));
    }

    #[test]
    fn test_form_step_submit_form_description() {
        let step = FormStep::SubmitForm {
            selector: "#contact-form".to_string(),
        };
        assert_eq!(step.description(), "Submit form #contact-form");
    }

    #[test]
    fn test_form_step_reset_form_description() {
        let step = FormStep::ResetForm {
            selector: "#search-form".to_string(),
        };
        assert_eq!(step.description(), "Reset form #search-form");
    }

    #[test]
    fn test_form_data_serialization() {
        let form = FormData::new().text("name", "Alice").number("age", 30.0);

        let json = serde_json::to_string(&form).unwrap();
        let deserialized: FormData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.fields.len(), 2);
    }

    #[test]
    fn test_form_value_text_serialization() {
        let value = FormValue::Text("test".to_string());
        let json = serde_json::to_string(&value).unwrap();
        let deserialized: FormValue = serde_json::from_str(&json).unwrap();
        match deserialized {
            FormValue::Text(text) => assert_eq!(text, "test"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_form_value_number_serialization() {
        let value = FormValue::Number(42.5);
        let json = serde_json::to_string(&value).unwrap();
        let deserialized: FormValue = serde_json::from_str(&json).unwrap();
        match deserialized {
            FormValue::Number(n) => assert_eq!(n, 42.5),
            _ => panic!("Expected Number variant"),
        }
    }

    #[test]
    fn test_form_value_boolean_serialization() {
        let value = FormValue::Boolean(true);
        let json = serde_json::to_string(&value).unwrap();
        let deserialized: FormValue = serde_json::from_str(&json).unwrap();
        match deserialized {
            FormValue::Boolean(b) => assert!(b),
            _ => panic!("Expected Boolean variant"),
        }
    }

    #[test]
    fn test_form_step_serialization() {
        let step = FormStep::SubmitForm {
            selector: "#form".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: FormStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            FormStep::SubmitForm { selector } => assert_eq!(selector, "#form"),
            _ => panic!("Expected SubmitForm variant"),
        }
    }
}
