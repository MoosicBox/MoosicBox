use ::std::time::SystemTime;

#[must_use]
pub fn now() -> SystemTime {
    SystemTime::now()
}
