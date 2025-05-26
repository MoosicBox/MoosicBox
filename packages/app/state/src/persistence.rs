use std::{path::Path, sync::Arc};

use hyperchad::state::{StatePersistence as _, sqlite::SqlitePersistence};
use moosicbox_app_models::Connection;
use strum::{AsRefStr, EnumString};

use crate::{AppState, AppStateError};

#[derive(Debug, Clone, Copy, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum PersistenceKey {
    Connection,
    Connections,
}

impl std::fmt::Display for PersistenceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl AppState {
    /// # Errors
    ///
    /// * If the persistence fails to initialize
    pub async fn set_persistence(
        &mut self,
        location: impl AsRef<Path>,
    ) -> Result<&mut Self, AppStateError> {
        *self.persistence.write().await = Some(Arc::new(SqlitePersistence::new(location).await?));
        Ok(self)
    }

    /// # Errors
    ///
    /// * If the persistence fails to initialize
    pub async fn with_persistence(
        mut self,
        location: impl AsRef<Path>,
    ) -> Result<Self, AppStateError> {
        self.set_persistence(location).await?;
        Ok(self)
    }

    /// # Errors
    ///
    /// * If the persistence fails to initialize
    pub async fn set_persistence_in_memory(&mut self) -> Result<&mut Self, AppStateError> {
        *self.persistence.write().await = Some(Arc::new(SqlitePersistence::new_in_memory().await?));
        Ok(self)
    }

    /// # Errors
    ///
    /// * If the persistence fails to initialize
    pub async fn with_persistence_in_memory(mut self) -> Result<Self, AppStateError> {
        self.set_persistence_in_memory().await?;
        Ok(self)
    }

    /// # Panics
    ///
    /// * If the persistence is not set
    pub async fn persistence(&self) -> Arc<SqlitePersistence> {
        self.persistence.read().await.clone().unwrap()
    }

    /// # Errors
    ///
    /// * If the persistence fails to get the connections
    pub async fn get_connections(&self) -> Result<Vec<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence
            .get(PersistenceKey::Connections)
            .await?
            .unwrap_or_default())
    }

    /// # Errors
    ///
    /// * If the persistence fails to get the current connection
    pub async fn get_current_connection(&self) -> Result<Option<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence.get(PersistenceKey::Connection).await?)
    }
}
