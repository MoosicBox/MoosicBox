use serde::{Deserialize, Serialize};

use crate::db::models::Profile;

/// API representation of a `MoosicBox` profile.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiProfile {
    /// Name of the profile
    pub name: String,
}

impl From<Profile> for ApiProfile {
    fn from(value: Profile) -> Self {
        Self { name: value.name }
    }
}
