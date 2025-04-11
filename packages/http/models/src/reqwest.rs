use crate::Method;

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
