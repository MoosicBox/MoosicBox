use std::{path::Path, sync::Arc};

use hyperchad::state::{StatePersistence as _, sqlite::SqlitePersistence};
use moosicbox_app_models::Connection;
use strum::{AsRefStr, EnumString};

use crate::{AppState, AppStateError, UpdateAppState};

#[derive(Debug, Clone, Copy, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum PersistenceKey {
    ConnectionId,
    ConnectionName,
    Connection,
    Connections,
}

impl From<PersistenceKey> for String {
    fn from(value: PersistenceKey) -> Self {
        value.to_string()
    }
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
        self.init_persistence().await?;
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
        self.init_persistence().await?;
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

    async fn init_persistence(&self) -> Result<(), AppStateError> {
        if let Some(connection) = self.get_current_connection().await? {
            self.current_connection_updated(&connection).await?;
        }
        Ok(())
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn get_connections(&self) -> Result<Vec<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence
            .get(PersistenceKey::Connections)
            .await?
            .unwrap_or_default())
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn get_current_connection(&self) -> Result<Option<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence.get(PersistenceKey::Connection).await?)
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn set_current_connection(
        &self,
        connection: impl AsRef<Connection>,
    ) -> Result<(), AppStateError> {
        let connection = connection.as_ref();

        self.persistence()
            .await
            .set(PersistenceKey::Connection, connection)
            .await?;

        self.current_connection_updated(connection).await?;

        Ok(())
    }

    async fn current_connection_updated(
        &self,
        connection: &Connection,
    ) -> Result<(), AppStateError> {
        use std::collections::HashMap;

        use moosicbox_music_api::{MusicApi, profiles::PROFILES};
        use moosicbox_music_models::ApiSource;
        use moosicbox_remote_library::RemoteLibraryMusicApi;

        static PROFILE: &str = "master";

        let mut apis_map: HashMap<ApiSource, Arc<Box<dyn MusicApi>>> = HashMap::new();

        for api_source in ApiSource::all() {
            apis_map.insert(
                api_source,
                Arc::new(Box::new(moosicbox_music_api::CachedMusicApi::new(
                    RemoteLibraryMusicApi::new(
                        connection.api_url.clone(),
                        api_source,
                        PROFILE.to_string(),
                    ),
                ))),
            );
        }

        PROFILES.upsert(PROFILE.to_string(), Arc::new(apis_map));

        self.set_state(UpdateAppState {
            api_url: Some(connection.api_url.clone()),
            ..Default::default()
        })
        .await?;

        Ok(())
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn remove_current_connection(&self) -> Result<Option<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence.take(PersistenceKey::Connection).await?)
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn get_connection_name(&self) -> Result<Option<String>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence.get(PersistenceKey::ConnectionName).await?)
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn update_connection_name(
        &self,
        name: impl Into<String>,
    ) -> Result<(), AppStateError> {
        let persistence = self.persistence().await;
        let name = name.into();
        persistence
            .set(PersistenceKey::ConnectionName, &name)
            .await?;
        Ok(())
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn get_or_init_connection_id(&self) -> Result<String, AppStateError> {
        const KEY: PersistenceKey = PersistenceKey::ConnectionId;

        let persistence = self.persistence().await;

        Ok(if let Some(connection_id) = persistence.get(KEY).await? {
            connection_id
        } else {
            let connection_id = nanoid::nanoid!();

            persistence.set(KEY, &connection_id).await?;

            connection_id
        })
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn add_connection(
        &self,
        connection: impl Into<Connection>,
    ) -> Result<Vec<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        let connection = connection.into();
        let mut connections: Vec<Connection> = persistence
            .get(PersistenceKey::Connections)
            .await?
            .unwrap_or_default();

        if self.get_current_connection().await?.is_none() {
            self.set_current_connection(connection.clone()).await?;
        }

        connections.push(connection);

        persistence
            .set(PersistenceKey::Connections, &connections)
            .await?;
        Ok(connections)
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn delete_connection(&self, name: &str) -> Result<Vec<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        let mut connections: Vec<Connection> = persistence
            .get(PersistenceKey::Connections)
            .await?
            .unwrap_or_default();

        if let Some(current_connection) = self.get_current_connection().await? {
            if current_connection.name == name {
                self.remove_current_connection().await?;
            }
        }

        connections.retain(|x| x.name != name);
        persistence
            .set(PersistenceKey::Connections, &connections)
            .await?;
        Ok(connections)
    }

    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn update_connection(
        &self,
        name: &str,
        connection: impl Into<Connection>,
    ) -> Result<Vec<Connection>, AppStateError> {
        let connection = connection.into();

        let persistence = self.persistence().await;
        let mut connections: Vec<Connection> = persistence
            .get(PersistenceKey::Connections)
            .await?
            .unwrap_or_default();

        if let Some(current_connection) = self.get_current_connection().await? {
            if current_connection.name == name {
                self.set_current_connection(connection.clone()).await?;
            }
        }

        for existing in &mut connections {
            if existing.name == name {
                *existing = connection;
                persistence
                    .set(PersistenceKey::Connections, &connections)
                    .await?;
                break;
            }
        }

        Ok(connections)
    }
}
