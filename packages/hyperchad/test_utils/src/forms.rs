use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormData {
    pub fields: BTreeMap<String, FormValue>,
}

impl FormData {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            fields: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn text(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields
            .insert(name.into(), FormValue::Text(value.into()));
        self
    }

    #[must_use]
    pub fn number(mut self, name: impl Into<String>, value: f64) -> Self {
        self.fields.insert(name.into(), FormValue::Number(value));
        self
    }

    #[must_use]
    pub fn boolean(mut self, name: impl Into<String>, value: bool) -> Self {
        self.fields.insert(name.into(), FormValue::Boolean(value));
        self
    }

    #[must_use]
    pub fn select(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields
            .insert(name.into(), FormValue::Select(value.into()));
        self
    }

    #[must_use]
    pub fn multi_select(mut self, name: impl Into<String>, values: Vec<String>) -> Self {
        self.fields
            .insert(name.into(), FormValue::MultiSelect(values));
        self
    }

    #[must_use]
    pub fn file(mut self, name: impl Into<String>, path: impl Into<std::path::PathBuf>) -> Self {
        self.fields
            .insert(name.into(), FormValue::File(path.into()));
        self
    }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FormValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Select(String),
    MultiSelect(Vec<String>),
    File(PathBuf),
}

impl FormValue {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FormStep {
    FillForm {
        data: FormData,
    },
    FillField {
        selector: String,
        value: FormValue,
    },
    SelectOption {
        selector: String,
        value: String,
    },
    UploadFile {
        selector: String,
        file_path: PathBuf,
    },
    SubmitForm {
        selector: String,
    },
    ResetForm {
        selector: String,
    },
}

impl FormStep {
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
