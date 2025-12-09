//! `MoosicBox` Tauri application entry point.
//!
//! This module serves as the main entry point for the `MoosicBox` desktop application.
//! It delegates to the library crate to run the Tauri application.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Application entry point.
///
/// Starts the `MoosicBox` Tauri application by delegating to [`moosicbox_lib::run`].
fn main() {
    moosicbox_lib::run()
}
