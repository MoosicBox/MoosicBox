use serde::{Deserialize, Serialize};

use crate::db::models::Profile;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiProfile {
    pub name: String,
}

impl From<Profile> for ApiProfile {
    fn from(value: Profile) -> Self {
        Self { name: value.name }
    }
}
