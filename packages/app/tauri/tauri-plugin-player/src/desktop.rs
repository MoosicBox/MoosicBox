use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> crate::Result<Player<R>> {
    Ok(Player(app.clone()))
}

/// Access to the player APIs.
pub struct Player<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Player<R> {
    pub fn update_state(&self, _payload: UpdateState) -> crate::Result<StateResponse> {
        Ok(StateResponse {})
    }

    pub fn init_channel(&self, _payload: InitChannel) -> crate::Result<InitChannelResponse> {
        Ok(InitChannelResponse {})
    }
}
