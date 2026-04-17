use std::collections::BTreeMap;

use serde_json::json;

/// HTTP lifecycle event kinds supported by the testing harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpEventKind {
    BeforeRequest,
    AfterRequest,
    RequestSuccess,
    RequestError,
    RequestAbort,
    RequestTimeout,
}

impl HttpEventKind {
    /// Returns the internal action trigger name.
    #[must_use]
    pub const fn event_type(self) -> &'static str {
        match self {
            Self::BeforeRequest => crate::client::core::event_types::HTTP_BEFORE_REQUEST,
            Self::AfterRequest => crate::client::core::event_types::HTTP_AFTER_REQUEST,
            Self::RequestSuccess => crate::client::core::event_types::HTTP_REQUEST_SUCCESS,
            Self::RequestError => crate::client::core::event_types::HTTP_REQUEST_ERROR,
            Self::RequestAbort => crate::client::core::event_types::HTTP_REQUEST_ABORT,
            Self::RequestTimeout => crate::client::core::event_types::HTTP_REQUEST_TIMEOUT,
        }
    }
}

/// Serializable payload for HTTP lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpEventPayload {
    pub url: String,
    pub method: String,
    pub status: Option<u16>,
    pub headers: Option<BTreeMap<String, String>>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

impl HttpEventPayload {
    /// Converts the payload into a compact JSON string.
    #[must_use]
    pub fn to_json_string(&self) -> String {
        json!({
            "url": self.url,
            "method": self.method,
            "status": self.status,
            "headers": self.headers,
            "duration_ms": self.duration_ms,
            "error": self.error,
        })
        .to_string()
    }
}
