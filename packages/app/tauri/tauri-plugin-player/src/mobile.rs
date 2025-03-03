use serde::de::DeserializeOwned;
use tauri::{
    AppHandle, Runtime,
    plugin::{PluginApi, PluginHandle},
};

use crate::models::*;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.moosicbox.playerplugin";

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_player);

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>,
    api: &PluginApi<R, C>,
) -> crate::Result<Player<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "PlayerPlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_player)?;
    Ok(Player(handle))
}

/// Access to the player APIs.
pub struct Player<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Player<R> {
    pub fn update_state(&self, payload: UpdateState) -> crate::Result<StateResponse> {
        self.0
            .run_mobile_plugin("updateState", payload)
            .map_err(Into::into)
    }

    pub fn init_channel(&self, payload: InitChannel) -> crate::Result<InitChannelResponse> {
        self.0
            .run_mobile_plugin("initChannel", payload)
            .map_err(Into::into)
    }
}
