use uuid::Uuid;

/// Generate a new random UUID v4
#[must_use]
pub fn new_v4() -> Uuid {
    Uuid::new_v4()
}

/// Generate a new random UUID v4 as a string
#[must_use]
pub fn new_v4_string() -> String {
    Uuid::new_v4().to_string()
}
