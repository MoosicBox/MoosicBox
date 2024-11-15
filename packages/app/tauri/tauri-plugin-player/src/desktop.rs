use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::{InitChannel, InitChannelResponse, StateResponse, UpdateState};

pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    _api: &PluginApi<R, C>,
) -> Player<R> {
    Player(app.clone())
}

/// Access to the player APIs.
pub struct Player<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Player<R> {
    #[allow(
        clippy::unused_self,
        clippy::needless_pass_by_value,
        clippy::unnecessary_wraps
    )]
    pub fn update_state(&self, _payload: UpdateState) -> crate::Result<StateResponse> {
        Ok(StateResponse {})
    }

    #[allow(
        clippy::unused_self,
        clippy::needless_pass_by_value,
        clippy::unnecessary_wraps
    )]
    pub fn init_channel(&self, _payload: InitChannel) -> crate::Result<InitChannelResponse> {
        Ok(InitChannelResponse {})
    }
}
