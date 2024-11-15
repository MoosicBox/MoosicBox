use tauri::{command, AppHandle, Runtime};

use crate::models::{StateResponse, UpdateState};
use crate::PlayerExt;
use crate::Result;

#[command]
pub async fn update_state<R: Runtime>(
    app: AppHandle<R>,
    payload: UpdateState,
) -> Result<StateResponse> {
    app.player().update_state(payload)
}
