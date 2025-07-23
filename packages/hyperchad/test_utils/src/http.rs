use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestStep {
    pub method: HttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<RequestBody>,
    pub expected_status: Option<u16>,
    pub timeout: Option<std::time::Duration>,
}

impl HttpRequestStep {
    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    #[must_use]
    pub fn post(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    #[must_use]
    pub fn put(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Put,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    #[must_use]
    pub fn delete(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Delete,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_headers(mut self, headers: BTreeMap<String, String>) -> Self {
        self.headers.extend(headers);
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: RequestBody) -> Self {
        self.body = Some(body);
        self
    }

    #[must_use]
    pub fn json(mut self, value: serde_json::Value) -> Self {
        self.body = Some(RequestBody::Json(value));
        self.headers
            .insert("content-type".to_string(), "application/json".to_string());
        self
    }

    #[must_use]
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.body = Some(RequestBody::Text(text.into()));
        self.headers
            .insert("content-type".to_string(), "text/plain".to_string());
        self
    }

    #[must_use]
    pub fn form(mut self, data: BTreeMap<String, String>) -> Self {
        self.body = Some(RequestBody::Form(data));
        self.headers.insert(
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        );
        self
    }

    #[must_use]
    pub const fn expect_status(mut self, status: u16) -> Self {
        self.expected_status = Some(status);
        self
    }

    #[must_use]
    pub const fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn description(&self) -> String {
        format!("{} {}", self.method, self.url)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Delete => write!(f, "DELETE"),
            Self::Patch => write!(f, "PATCH"),
            Self::Head => write!(f, "HEAD"),
            Self::Options => write!(f, "OPTIONS"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestBody {
    Text(String),
    Json(serde_json::Value),
    Form(BTreeMap<String, String>),
    Binary(Vec<u8>),
}
