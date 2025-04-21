use actix_web::http::StatusCode;

#[allow(clippy::fallible_impl_from)]
impl From<crate::StatusCode> for StatusCode {
    fn from(value: crate::StatusCode) -> Self {
        Self::from_u16(value.into()).unwrap()
    }
}

impl From<StatusCode> for crate::StatusCode {
    fn from(value: StatusCode) -> Self {
        Self::from_u16(value.into())
    }
}
