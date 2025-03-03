use tauri::{AppHandle, Runtime, command};

use crate::PlayerExt;
use crate::Result;
use crate::models::{StateResponse, UpdateState};

#[command]
pub async fn update_state<R: Runtime>(
    app: AppHandle<R>,
    payload: UpdateState,
) -> Result<StateResponse> {
    app.player().update_state(payload)
}
