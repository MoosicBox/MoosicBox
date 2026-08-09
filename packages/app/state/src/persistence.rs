//! Persistent storage functionality for `MoosicBox` application state.
//!
//! This module provides persistence capabilities using `SQLite` as the backing store,
//! allowing application state to be saved and restored across application restarts.
//!
//! # Features
//!
//! * File-based or in-memory `SQLite` storage
//! * Connection management (add, update, delete, list)
//! * Connection name and ID persistence
//! * Default download location storage
//!
//! # Example
//!
//! ```no_run
//! # use moosicbox_app_state::AppState;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create state with file-based persistence
//! let state = AppState::new()
//!     .with_persistence("/path/to/state.db")
//!     .await?;
//!
//! // Or use in-memory persistence for testing
//! let test_state = AppState::new()
//!     .with_persistence_in_memory()
//!     .await?;
//! # Ok(())
//! # }
//! ```

use std::{path::Path, sync::Arc};

use hyperchad::state::{StatePersistence as _, sqlite::SqlitePersistence};
use moosicbox_app_models::Connection;
use strum::{AsRefStr, EnumString};

use crate::{AppState, AppStateError, ConnectionConfig};

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

    #[allow(clippy::unused_async)]
    async fn init_persistence(&self) -> Result<(), AppStateError> {
        Ok(())
    }

    /// Activates the persisted connection with a complete runtime context.
    ///
    /// The endpoint, profile, connection name, and stable connection identifier are
    /// applied in one state update so connection-dependent services cannot observe a
    /// partially initialized context.
    ///
    /// # Errors
    ///
    /// * If the persistence layer cannot load the current connection or identity
    /// * If activating the connection state fails
    pub async fn activate_persisted_connection(
        &self,
        profile: impl Into<String>,
    ) -> Result<(), AppStateError> {
        let Some(connection) = self.get_current_connection().await? else {
            self.disconnect().await?;
            return Ok(());
        };
        let connection_name = self.get_connection_name().await?;
        let connection_id = self.get_or_init_connection_id().await?;
        let config = ConnectionConfig::new(connection.api_url, profile, connection_id)?
            .with_connection_name(connection_name);

        self.activate_connection(config).await
    }

    /// Selects and immediately activates a persisted remote connection.
    ///
    /// Persistence remains the owner of the user's backend selection while the
    /// runtime lifecycle receives one complete validated configuration.
    ///
    /// # Errors
    ///
    /// * If persistence cannot save the selected connection
    /// * If the connection identity cannot be loaded or created
    /// * If the connection cannot form a valid runtime configuration
    /// * If activation fails
    pub async fn select_connection(
        &self,
        connection: impl AsRef<Connection>,
        profile: impl Into<String>,
    ) -> Result<(), AppStateError> {
        let connection = connection.as_ref();
        let persistence = self.persistence().await;
        persistence
            .set(PersistenceKey::Connection, connection)
            .await?;
        persistence
            .set(PersistenceKey::ConnectionName, &connection.name)
            .await?;

        let connection_id = self.get_or_init_connection_id().await?;
        let config = ConnectionConfig::new(connection.api_url.clone(), profile, connection_id)?
            .with_connection_name(Some(connection.name.clone()));
        self.replace_connection(config).await
    }

    /// Activates a runtime endpoint while preserving the persisted connection identity.
    ///
    /// This is used by bundled mode, where the endpoint is selected at runtime and
    /// must not be stored as a synthetic remote connection.
    ///
    /// # Errors
    ///
    /// * If the persistence layer cannot load or create the connection identity
    /// * If the endpoint cannot form a valid runtime connection configuration
    /// * If activating the connection fails
    pub async fn activate_endpoint(
        &self,
        endpoint: impl Into<String>,
        profile: impl Into<String>,
        connection_name: impl Into<String>,
    ) -> Result<(), AppStateError> {
        let connection_id = self.get_or_init_connection_id().await?;
        let config = ConnectionConfig::new(endpoint, profile, connection_id)?
            .with_connection_name(Some(connection_name.into()));
        self.activate_connection(config).await
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

    /// Saves the currently selected remote connection without changing runtime state.
    ///
    /// Use [`Self::select_connection`] when a user action must both persist and
    /// activate a remote connection.
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
            self.disconnect().await?;
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
            self.select_connection(&connection, "master").await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionConfig, ConnectionStatus};

    #[test]
    fn test_connection_config_rejects_incomplete_values() {
        assert!(ConnectionConfig::new("", "master", "connection").is_err());
        assert!(ConnectionConfig::new("ftp://example.com", "master", "connection").is_err());
        assert!(ConnectionConfig::new("https://example.com", "", "connection").is_err());
        assert!(ConnectionConfig::new("https://example.com", "master", "").is_err());
    }

    #[test]
    fn test_connection_status_defaults_to_unconfigured() {
        assert_eq!(ConnectionStatus::default(), ConnectionStatus::Unconfigured);
    }

    #[test_log::test(switchy_async::test)]
    async fn unchanged_connection_does_not_restart_lifecycle() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");
        let config = ConnectionConfig::new("http://127.0.0.1:9", "master", "stable")
            .expect("Invalid connection");

        assert!(
            state
                .replace_connection_if_changed(config.clone())
                .await
                .expect("Failed to activate connection")
        );
        let generation = state.connection_generation();
        assert!(
            !state
                .replace_connection_if_changed(config)
                .await
                .expect("Failed to compare connection")
        );
        assert_eq!(state.connection_generation(), generation);
    }

    #[test_log::test(switchy_async::test)]
    async fn changed_credentials_replace_connection_atomically() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");
        let first = ConnectionConfig::new("http://127.0.0.1:9", "master", "stable")
            .expect("Invalid connection")
            .with_credentials(Some("client".to_string()), None, None);
        let replacement = first.clone().with_credentials(
            Some("client".to_string()),
            Some("signature".to_string()),
            None,
        );

        state
            .replace_connection(first)
            .await
            .expect("Failed to activate connection");
        let generation = state.connection_generation();
        assert!(
            state
                .replace_connection_if_changed(replacement.clone())
                .await
                .expect("Failed to replace credentials")
        );
        assert!(state.connection_generation() > generation);
        assert_eq!(
            state.connection_config.read().await.as_ref(),
            Some(&replacement)
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn switching_while_retrying_invalidates_the_retry_generation() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");
        let first = ConnectionConfig::new("http://127.0.0.1:9", "master", "first")
            .expect("Invalid first connection");
        let second = ConnectionConfig::new("http://127.0.0.1:10", "master", "second")
            .expect("Invalid second connection");

        state
            .activate_connection(first)
            .await
            .expect("Failed to activate first connection");
        state
            .retry_connection()
            .await
            .expect("Failed to retry connection");
        let retry_generation = state.connection_generation();

        state
            .replace_connection(second.clone())
            .await
            .expect("Failed to switch connection");

        assert!(!state.is_active_connection_generation(retry_generation));
        assert!(
            !state
                .fail_connection(retry_generation, "stale retry failed")
                .await
        );
        assert_eq!(state.connection_config.read().await.as_ref(), Some(&second));
        assert_eq!(
            state.connection_status().await,
            ConnectionStatus::Connecting
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn shutdown_during_retry_clears_runtime_ownership() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");
        let config = ConnectionConfig::new("http://127.0.0.1:9", "master", "connection")
            .expect("Invalid connection");

        state
            .activate_connection(config)
            .await
            .expect("Failed to activate connection");
        state
            .retry_connection()
            .await
            .expect("Failed to retry connection");
        let retry_generation = state.connection_generation();

        state.disconnect().await.expect("Failed to disconnect");

        assert!(!state.is_active_connection_generation(retry_generation));
        assert!(state.connection_config.read().await.is_none());
        assert_eq!(
            state.connection_status().await,
            ConnectionStatus::Unconfigured
        );
        assert!(
            !state
                .fail_connection(retry_generation, "stale retry failed")
                .await
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_replace_retry_and_disconnect_advance_generation() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");
        let first = ConnectionConfig::new("http://127.0.0.1:9", "master", "first")
            .expect("Invalid first connection");
        let second = ConnectionConfig::new("http://127.0.0.1:10", "master", "second")
            .expect("Invalid second connection");

        state
            .activate_connection(first)
            .await
            .expect("Failed to activate first connection");
        let first_generation = state.connection_generation();

        state
            .replace_connection(second)
            .await
            .expect("Failed to replace connection");
        let second_generation = state.connection_generation();
        assert!(second_generation > first_generation);
        assert!(!state.is_active_connection_generation(first_generation));

        state
            .retry_connection()
            .await
            .expect("Failed to retry connection");
        let retry_generation = state.connection_generation();
        assert!(retry_generation > second_generation);

        state.disconnect().await.expect("Failed to disconnect");
        assert!(state.connection_generation() > retry_generation);
        assert_eq!(
            state.connection_status().await,
            ConnectionStatus::Unconfigured
        );
        assert!(state.connection_config.read().await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_activate_persisted_connection_applies_complete_context() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");
        let connection = Connection {
            name: "Test Server".to_string(),
            api_url: "http://127.0.0.1:9".to_string(),
        };
        let persistence = state.persistence().await;
        persistence
            .set(PersistenceKey::Connection, &connection)
            .await
            .expect("Failed to persist current connection");
        persistence
            .set(PersistenceKey::ConnectionName, &connection.name)
            .await
            .expect("Failed to persist connection name");

        state
            .activate_persisted_connection("master")
            .await
            .expect("Failed to activate persisted connection");

        assert_eq!(
            state.api_url.read().await.as_deref(),
            Some("http://127.0.0.1:9")
        );
        assert_eq!(state.profile.read().await.as_deref(), Some("master"));
        assert_eq!(
            state.connection_name.read().await.as_deref(),
            Some("Test Server")
        );
        assert!(state.connection_id.read().await.is_some());
        assert_eq!(
            state.connection_status().await,
            ConnectionStatus::Connecting
        );
        let config = state
            .connection_config
            .read()
            .await
            .clone()
            .expect("Missing active connection configuration");
        assert_eq!(config.api_url(), "http://127.0.0.1:9");
        assert_eq!(config.profile(), "master");
        assert!(!config.connection_id().is_empty());

        state
            .close_ws_connection()
            .await
            .expect("Failed to close websocket connection");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_with_persistence_in_memory() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        assert!(state.persistence.read().await.is_some());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_add_and_get_connections() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let connection = Connection {
            name: "Test Server".to_string(),
            api_url: "https://test.example.com".to_string(),
        };

        let connections = state
            .add_connection(connection.clone())
            .await
            .expect("Failed to add connection");

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "Test Server");

        let retrieved_connections = state
            .get_connections()
            .await
            .expect("Failed to get connections");

        assert_eq!(retrieved_connections.len(), 1);
        assert_eq!(retrieved_connections[0], connection);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_set_and_get_current_connection() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let connection = Connection {
            name: "Current Server".to_string(),
            api_url: "https://current.example.com".to_string(),
        };

        state
            .set_current_connection(&connection)
            .await
            .expect("Failed to set current connection");

        let current = state
            .get_current_connection()
            .await
            .expect("Failed to get current connection")
            .expect("No current connection found");

        assert_eq!(current, connection);
        assert_eq!(
            state.connection_status().await,
            ConnectionStatus::Unconfigured
        );
        assert!(state.connection_config.read().await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_select_connection_persists_and_replaces_runtime_atomically() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");
        let connection = Connection {
            name: "Selected Server".to_string(),
            api_url: "http://127.0.0.1:9".to_string(),
        };

        state
            .select_connection(&connection, "master")
            .await
            .expect("Failed to select connection");

        assert_eq!(
            state
                .get_current_connection()
                .await
                .expect("Failed to load selected connection"),
            Some(connection.clone())
        );
        assert_eq!(
            state.connection_status().await,
            ConnectionStatus::Connecting
        );
        let config = state
            .connection_config
            .read()
            .await
            .clone()
            .expect("Missing active runtime connection");
        assert_eq!(config.api_url(), connection.api_url);
        assert_eq!(config.profile(), "master");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_remove_current_connection() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let connection = Connection {
            name: "Temp Server".to_string(),
            api_url: "https://temp.example.com".to_string(),
        };

        state
            .set_current_connection(&connection)
            .await
            .expect("Failed to set current connection");

        let removed = state
            .remove_current_connection()
            .await
            .expect("Failed to remove current connection")
            .expect("No connection to remove");

        assert_eq!(removed, connection);

        let current = state
            .get_current_connection()
            .await
            .expect("Failed to get current connection");

        assert!(current.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_delete_connection() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let connection1 = Connection {
            name: "Server 1".to_string(),
            api_url: "https://server1.example.com".to_string(),
        };

        let connection2 = Connection {
            name: "Server 2".to_string(),
            api_url: "https://server2.example.com".to_string(),
        };

        state
            .add_connection(connection1.clone())
            .await
            .expect("Failed to add connection 1");
        state
            .add_connection(connection2.clone())
            .await
            .expect("Failed to add connection 2");

        let remaining = state
            .delete_connection("Server 1")
            .await
            .expect("Failed to delete connection");

        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "Server 2");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_delete_current_connection() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let connection = Connection {
            name: "Current".to_string(),
            api_url: "https://current.example.com".to_string(),
        };

        state
            .add_connection(connection.clone())
            .await
            .expect("Failed to add connection");

        // First connection is automatically set as current
        let current = state
            .get_current_connection()
            .await
            .expect("Failed to get current connection");
        assert!(current.is_some());

        state
            .delete_connection("Current")
            .await
            .expect("Failed to delete connection");

        let current_after = state
            .get_current_connection()
            .await
            .expect("Failed to get current connection");
        assert!(current_after.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_update_connection() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let connection = Connection {
            name: "Original".to_string(),
            api_url: "https://original.example.com".to_string(),
        };

        state
            .add_connection(connection.clone())
            .await
            .expect("Failed to add connection");

        let updated_connection = Connection {
            name: "Updated".to_string(),
            api_url: "https://updated.example.com".to_string(),
        };

        let connections = state
            .update_connection("Original", updated_connection.clone())
            .await
            .expect("Failed to update connection");

        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "Updated");
        assert_eq!(connections[0].api_url, "https://updated.example.com");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_get_or_init_connection_id() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let connection_id1 = state
            .get_or_init_connection_id()
            .await
            .expect("Failed to get connection ID");

        assert!(!connection_id1.is_empty());

        // Getting again should return the same ID
        let connection_id2 = state
            .get_or_init_connection_id()
            .await
            .expect("Failed to get connection ID");

        assert_eq!(connection_id1, connection_id2);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_connection_name_persistence() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        state
            .update_connection_name("My Connection")
            .await
            .expect("Failed to update connection name");

        let name = state
            .get_connection_name()
            .await
            .expect("Failed to get connection name")
            .expect("No connection name found");

        assert_eq!(name, "My Connection");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_app_state_default_download_location() {
        let state = AppState::new()
            .with_persistence_in_memory()
            .await
            .expect("Failed to create in-memory persistence");

        let path = "/downloads/music";
        state
            .set_default_download_location(path.to_string())
            .await
            .expect("Failed to set default download location");

        let retrieved_path = state.get_default_download_location();

        assert_eq!(retrieved_path, Some(path.to_string()));
    }
}
