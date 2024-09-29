use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::PlayerExt;
use crate::Result;

#[command]
pub(crate) async fn update_state<R: Runtime>(
    app: AppHandle<R>,
    payload: UpdateState,
) -> Result<StateResponse> {
    app.player().update_state(payload)
}
