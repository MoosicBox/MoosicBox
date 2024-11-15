#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Player;
#[cfg(mobile)]
use mobile::Player;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the player APIs.
pub trait PlayerExt<R: Runtime> {
    fn player(&self) -> &Player<R>;
}

impl<R: Runtime, T: Manager<R>> crate::PlayerExt<R> for T {
    fn player(&self) -> &Player<R> {
        self.state::<Player<R>>().inner()
    }
}

/// Initializes the plugin.
#[must_use]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("player")
        .invoke_handler(tauri::generate_handler![commands::update_state])
        .setup(|app, api| {
            #[cfg(mobile)]
            let player = mobile::init(app, &api)?;
            #[cfg(desktop)]
            let player = desktop::init(app, &api);
            app.manage(player);
            Ok(())
        })
        .build()
}
