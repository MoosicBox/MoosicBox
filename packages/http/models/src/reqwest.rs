use crate::{Method, StatusCode};

impl From<Method> for reqwest::Method {
    fn from(value: Method) -> Self {
        match value {
            Method::Get => Self::GET,
            Method::Post => Self::POST,
            Method::Put => Self::PUT,
            Method::Patch => Self::PATCH,
            Method::Delete => Self::DELETE,
            Method::Head => Self::HEAD,
            Method::Options => Self::OPTIONS,
            Method::Connect => Self::CONNECT,
            Method::Trace => Self::TRACE,
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<reqwest::StatusCode> for StatusCode {
    fn from(value: reqwest::StatusCode) -> Self {
        Self::from_u16(value.as_u16())
    }
}
