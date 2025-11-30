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

    // FormValue::as_string tests
    #[test_log::test]
    fn form_value_as_string_text() {
        let value = FormValue::Text("hello world".to_string());
        assert_eq!(value.as_string(), "hello world");
    }

    #[test_log::test]
    fn form_value_as_string_number_integer() {
        let value = FormValue::Number(42.0);
        assert_eq!(value.as_string(), "42");
    }

    #[test_log::test]
    fn form_value_as_string_number_decimal() {
        let value = FormValue::Number(3.15);
        assert_eq!(value.as_string(), "3.15");
    }

    #[test_log::test]
    fn form_value_as_string_boolean_true() {
        let value = FormValue::Boolean(true);
        assert_eq!(value.as_string(), "true");
    }

    #[test_log::test]
    fn form_value_as_string_boolean_false() {
        let value = FormValue::Boolean(false);
        assert_eq!(value.as_string(), "false");
    }

    #[test_log::test]
    fn form_value_as_string_select() {
        let value = FormValue::Select("option1".to_string());
        assert_eq!(value.as_string(), "option1");
    }

    #[test_log::test]
    fn form_value_as_string_multi_select() {
        let value = FormValue::MultiSelect(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(value.as_string(), "a,b,c");
    }

    #[test_log::test]
    fn form_value_as_string_multi_select_single_item() {
        let value = FormValue::MultiSelect(vec!["only".to_string()]);
        assert_eq!(value.as_string(), "only");
    }

    #[test_log::test]
    fn form_value_as_string_multi_select_empty() {
        let value = FormValue::MultiSelect(vec![]);
        assert_eq!(value.as_string(), "");
    }

    #[test_log::test]
    fn form_value_as_string_file() {
        let value = FormValue::File(PathBuf::from("/path/to/file.txt"));
        assert_eq!(value.as_string(), "/path/to/file.txt");
    }

    // FormData builder tests
    #[test_log::test]
    fn form_data_builder_text() {
        let data = FormData::new()
            .text("username", "john")
            .text("email", "john@example.com");

        assert_eq!(data.fields.len(), 2);
        assert!(matches!(data.fields.get("username"), Some(FormValue::Text(s)) if s == "john"));
        assert!(
            matches!(data.fields.get("email"), Some(FormValue::Text(s)) if s == "john@example.com")
        );
    }

    #[test_log::test]
    fn form_data_builder_number() {
        let data = FormData::new().number("age", 25.0).number("score", 99.5);

        assert_eq!(data.fields.len(), 2);
        assert!(
            matches!(data.fields.get("age"), Some(FormValue::Number(n)) if (*n - 25.0).abs() < f64::EPSILON)
        );
        assert!(
            matches!(data.fields.get("score"), Some(FormValue::Number(n)) if (*n - 99.5).abs() < f64::EPSILON)
        );
    }

    #[test_log::test]
    fn form_data_builder_boolean() {
        let data = FormData::new()
            .boolean("active", true)
            .boolean("disabled", false);

        assert_eq!(data.fields.len(), 2);
        assert!(matches!(
            data.fields.get("active"),
            Some(FormValue::Boolean(true))
        ));
        assert!(matches!(
            data.fields.get("disabled"),
            Some(FormValue::Boolean(false))
        ));
    }

    #[test_log::test]
    fn form_data_builder_select() {
        let data = FormData::new().select("country", "US");

        assert_eq!(data.fields.len(), 1);
        assert!(matches!(data.fields.get("country"), Some(FormValue::Select(s)) if s == "US"));
    }

    #[test_log::test]
    fn form_data_builder_multi_select() {
        let data =
            FormData::new().multi_select("tags", vec!["rust".to_string(), "web".to_string()]);

        assert_eq!(data.fields.len(), 1);
        if let Some(FormValue::MultiSelect(values)) = data.fields.get("tags") {
            assert_eq!(values.len(), 2);
            assert_eq!(values[0], "rust");
            assert_eq!(values[1], "web");
        } else {
            panic!("Expected MultiSelect value");
        }
    }

    #[test_log::test]
    fn form_data_builder_file() {
        let data = FormData::new().file("avatar", "/uploads/photo.png");

        assert_eq!(data.fields.len(), 1);
        if let Some(FormValue::File(path)) = data.fields.get("avatar") {
            assert_eq!(path, &PathBuf::from("/uploads/photo.png"));
        } else {
            panic!("Expected File value");
        }
    }

    #[test_log::test]
    fn form_data_builder_field_custom_value() {
        let data = FormData::new().field("custom", FormValue::Number(123.0));

        assert_eq!(data.fields.len(), 1);
        assert!(
            matches!(data.fields.get("custom"), Some(FormValue::Number(n)) if (*n - 123.0).abs() < f64::EPSILON)
        );
    }

    #[test_log::test]
    fn form_data_overwrites_duplicate_keys() {
        let data = FormData::new().text("key", "first").text("key", "second");

        assert_eq!(data.fields.len(), 1);
        assert!(matches!(data.fields.get("key"), Some(FormValue::Text(s)) if s == "second"));
    }

    // FormStep description tests
    #[test_log::test]
    fn form_step_description_fill_form() {
        let data = FormData::new().text("a", "1").text("b", "2").text("c", "3");
        let step = FormStep::FillForm { data };

        assert_eq!(step.description(), "Fill form with 3 fields");
    }

    #[test_log::test]
    fn form_step_description_fill_field() {
        let step = FormStep::FillField {
            selector: "#email".to_string(),
            value: FormValue::Text("test@example.com".to_string()),
        };

        assert_eq!(
            step.description(),
            "Fill field #email with test@example.com"
        );
    }

    #[test_log::test]
    fn form_step_description_select_option() {
        let step = FormStep::SelectOption {
            selector: "#country".to_string(),
            value: "United States".to_string(),
        };

        assert_eq!(
            step.description(),
            "Select option United States in #country"
        );
    }

    #[test_log::test]
    fn form_step_description_upload_file() {
        let step = FormStep::UploadFile {
            selector: "#avatar".to_string(),
            file_path: PathBuf::from("/path/to/image.jpg"),
        };

        assert_eq!(
            step.description(),
            "Upload file /path/to/image.jpg to #avatar"
        );
    }

    #[test_log::test]
    fn form_step_description_submit_form() {
        let step = FormStep::SubmitForm {
            selector: "#registration-form".to_string(),
        };

        assert_eq!(step.description(), "Submit form #registration-form");
    }

    #[test_log::test]
    fn form_step_description_reset_form() {
        let step = FormStep::ResetForm {
            selector: "#settings-form".to_string(),
        };

        assert_eq!(step.description(), "Reset form #settings-form");
    }
}
