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
