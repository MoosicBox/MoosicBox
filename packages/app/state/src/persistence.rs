use std::{path::Path, sync::Arc};

use hyperchad::state::{StatePersistence as _, sqlite::SqlitePersistence};
use moosicbox_app_models::Connection;
use strum::{AsRefStr, EnumString};

use crate::{AppState, AppStateError, UpdateAppState};

/// Keys used for persisting application state to storage.
///
/// These keys are used to store and retrieve various pieces of state
/// from the persistence layer (`SQLite` database).
#[derive(Debug, Clone, Copy, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum PersistenceKey {
    /// Unique identifier for the current connection
    ConnectionId,
    /// Display name for the current connection
    ConnectionName,
    /// Currently active connection configuration
    Connection,
    /// List of all saved connections
    Connections,
    /// Default location for downloaded files
    DefaultDownloadLocation,
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
    /// Initializes persistence with a file-based `SQLite` database at the specified location.
    ///
    /// This method sets up the persistence layer and loads any previously saved state.
    /// Use this when you need persistent storage across application restarts.
    ///
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

    /// Builder method to initialize persistence with a file-based `SQLite` database.
    ///
    /// Consumes self and returns the configured instance. Equivalent to `set_persistence`
    /// but designed for method chaining during initialization.
    ///
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

    /// Initializes persistence with an in-memory `SQLite` database.
    ///
    /// State will be lost when the application terminates. Useful for testing
    /// or when persistent storage is not needed.
    ///
    /// # Errors
    ///
    /// * If the persistence fails to initialize
    pub async fn set_persistence_in_memory(&mut self) -> Result<&mut Self, AppStateError> {
        *self.persistence.write().await = Some(Arc::new(SqlitePersistence::new_in_memory().await?));
        self.init_persistence().await?;
        Ok(self)
    }

    /// Builder method to initialize persistence with an in-memory `SQLite` database.
    ///
    /// Consumes self and returns the configured instance. Equivalent to `set_persistence_in_memory`
    /// but designed for method chaining during initialization.
    ///
    /// # Errors
    ///
    /// * If the persistence fails to initialize
    pub async fn with_persistence_in_memory(mut self) -> Result<Self, AppStateError> {
        self.set_persistence_in_memory().await?;
        Ok(self)
    }

    /// Gets the persistence layer instance.
    ///
    /// Returns a reference to the `SQLite` persistence layer for direct access
    /// to persistence operations.
    ///
    /// # Panics
    ///
    /// * If the persistence is not set
    #[must_use]
    pub async fn persistence(&self) -> Arc<SqlitePersistence> {
        self.persistence.read().await.clone().unwrap()
    }

    async fn init_persistence(&self) -> Result<(), AppStateError> {
        if let Some(connection) = self.get_current_connection().await? {
            self.current_connection_updated(&connection).await?;
        }
        Ok(())
    }

    /// Retrieves all saved connections from persistent storage.
    ///
    /// Returns an empty list if no connections have been saved.
    ///
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

    /// Retrieves the currently active connection from persistent storage.
    ///
    /// Returns `None` if no connection is currently set as active.
    ///
    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn get_current_connection(&self) -> Result<Option<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence.get(PersistenceKey::Connection).await?)
    }

    /// Sets the currently active connection and saves it to persistent storage.
    ///
    /// This also updates the application state with the connection's API URL and
    /// initializes the music API profiles for the connection.
    ///
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
        use std::collections::BTreeMap;

        use moosicbox_music_api::{MusicApi, profiles::PROFILES};
        use moosicbox_music_models::ApiSource;
        use moosicbox_remote_library::RemoteLibraryMusicApi;

        static PROFILE: &str = "master";

        let mut apis_map: BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>> = BTreeMap::new();

        for api_source in ApiSource::all() {
            apis_map.insert(
                api_source.clone(),
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
            api_url: Some(Some(connection.api_url.clone())),
            ..Default::default()
        })
        .await?;

        Ok(())
    }

    /// Removes the currently active connection from persistent storage.
    ///
    /// Returns the removed connection if one was set, or `None` if no connection
    /// was active.
    ///
    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn remove_current_connection(&self) -> Result<Option<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence.take(PersistenceKey::Connection).await?)
    }

    /// Retrieves the connection name from persistent storage.
    ///
    /// Returns `None` if no connection name has been set.
    ///
    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn get_connection_name(&self) -> Result<Option<String>, AppStateError> {
        let persistence = self.persistence().await;
        Ok(persistence.get(PersistenceKey::ConnectionName).await?)
    }

    /// Updates the connection name in persistent storage.
    ///
    /// Saves the provided name to the persistence layer for future retrieval.
    ///
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

    /// Gets the connection ID from persistent storage, or creates a new one if it doesn't exist.
    ///
    /// The connection ID is a unique identifier for this application instance. If one
    /// doesn't exist in persistence, a new ID is generated and saved automatically.
    ///
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

    /// Adds a new connection to the list of saved connections.
    ///
    /// If this is the first connection being added and no current connection is set,
    /// it will automatically be set as the current connection. Returns the updated
    /// list of all connections.
    ///
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

    /// Deletes a connection from the list of saved connections by name.
    ///
    /// If the deleted connection was the current connection, it will be unset.
    /// Returns the updated list of remaining connections.
    ///
    /// # Errors
    ///
    /// * If the persistence fails
    pub async fn delete_connection(&self, name: &str) -> Result<Vec<Connection>, AppStateError> {
        let persistence = self.persistence().await;
        let mut connections: Vec<Connection> = persistence
            .get(PersistenceKey::Connections)
            .await?
            .unwrap_or_default();

        if let Some(current_connection) = self.get_current_connection().await?
            && current_connection.name == name
        {
            self.remove_current_connection().await?;
        }

        connections.retain(|x| x.name != name);
        persistence
            .set(PersistenceKey::Connections, &connections)
            .await?;
        Ok(connections)
    }

    /// Updates an existing connection in the list of saved connections.
    ///
    /// Finds the connection with the given name and replaces it with the new
    /// connection data. If the updated connection is the current connection,
    /// it will also update the current connection. Returns the updated list
    /// of all connections.
    ///
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

        if let Some(current_connection) = self.get_current_connection().await?
            && current_connection.name == name
        {
            self.set_current_connection(connection.clone()).await?;
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

    pub(crate) async fn persist_default_download_location(
        &self,
        path: impl AsRef<str>,
    ) -> Result<(), AppStateError> {
        let path = path.as_ref();
        let persistence = self.persistence().await;
        persistence
            .set(PersistenceKey::DefaultDownloadLocation, &path.to_string())
            .await?;
        Ok(())
    }
}
