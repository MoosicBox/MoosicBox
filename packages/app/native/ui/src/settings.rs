//! Settings and configuration UI components.
//!
//! This module provides UI templates for managing application settings including
//! server connections, download locations, scan paths, and music API authentication.

#![allow(clippy::module_name_repetitions)]

#[allow(unused_imports)]
use hyperchad::template as hyperchad_template;
use hyperchad::{
    template::{Containers, container},
    transformer::models::Selector,
};
use moosicbox_app_models::{
    AuthMethod, Connection, DownloadSettings, MusicApiSettings, ScanSettings,
};
use strum::{AsRefStr, EnumString};

use crate::{formatting::classify_name, page, state::State};

/// Renders the settings page content.
///
/// Displays forms for managing connection name, server connections,
/// download settings, scan settings, and music API authentication.
#[must_use]
pub fn settings_page_content(
    connection_name: &str,
    connections: &[Connection],
    selected: Option<&Connection>,
    music_api_settings: &[MusicApiSettings],
) -> Containers {
    container! {
        div padding=20 gap=10 {
            section align-items=start {
                div align-items=end gap=10 {
                    form
                        hx-post="/settings/connection-name"
                        width=100%
                        align-items=end
                        gap=5
                    {
                        div { "Name: " input type=text name="name" value=(connection_name); }
                        button
                            type=submit
                            border-radius=5
                            background=#111
                            border="2, #222"
                            padding-x=10
                            padding-y=5
                        {
                            "Save"
                        }
                    }

                    div width=100% text-align=start {
                        h2 { "Connections" }
                    }

                    (connections_content(connections, selected))

                    div
                        direction=row
                        justify-content=center
                        width=100%
                        gap=5
                    {
                        button
                            border-radius=5
                            background=#111
                            border="2, #222"
                            padding-x=10
                            padding-y=5
                            hx-post="/settings/new-connection"
                            hx-target="#settings-connections"
                        {
                            "New Connection"
                        }
                    }
                }
            }

            section hx-get="/settings/download-settings" hx-trigger="load" {
                div #settings-download-settings-section {}
            }

            section hx-get="/settings/scan-settings" hx-trigger="load" {
                div #settings-scan-settings-section {}
            }

            section hx-get="/settings/music-api-settings" hx-trigger="load" {
                (music_api_settings_section(&music_api_settings))
            }
        }
    }
}

/// Renders the scan settings section content.
///
/// Displays the list of configured scan paths with delete controls.
#[must_use]
pub fn scan_settings_content(scan_settings: &ScanSettings) -> Containers {
    container! {
        div #settings-scan-settings-section {
            h2 { "Scan Settings" }
            h3 { "Scan paths:" }
            @if scan_settings.scan_paths.is_empty() {
                "No scan paths"
            } @else {
                ul {
                    @for path in &scan_settings.scan_paths {
                        li {
                            (path)
                            form hx-delete="/settings/scan/scan-path" {
                                input type=hidden name="path" value=(path);
                                button
                                    type=submit
                                    border-radius=5
                                    background=#111
                                    border="2, #222"
                                    padding-x=10
                                    padding-y=5
                                {
                                    "Delete"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Renders the download settings section content.
///
/// Displays configured download locations with controls to delete or set as default.
#[must_use]
pub fn download_settings_content(download_settings: &DownloadSettings) -> Containers {
    container! {
        div #settings-download-settings-section {
            h2 { "Scan Settings" }
            h3 { "Download locations:" }
            @if download_settings.download_locations.is_empty() {
                "No download locations"
            } @else {
                ul {
                    @for (_id, location) in &download_settings.download_locations {
                        @let is_default = download_settings.default_download_location.as_ref().is_some_and(|x| x == location);

                        li {
                            (location)
                            form hx-delete="/settings/download/download-location" {
                                input type=hidden name="location" value=(location);
                                button
                                    type=submit
                                    border-radius=5
                                    background=#111
                                    border="2, #222"
                                    padding-x=10
                                    padding-y=5
                                {
                                    "Delete"
                                }
                            }
                            @if !is_default {
                                form hx-post="/settings/download/default-download-location" {
                                    input type=hidden name="location" value=(location);
                                    button
                                        type=submit
                                        border-radius=5
                                        background=#111
                                        border="2, #222"
                                        padding-x=10
                                        padding-y=5
                                    {
                                        "Set as default"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Renders the music API settings section.
///
/// Displays all configured music API services with their authentication and scan settings.
#[must_use]
pub fn music_api_settings_section(settings: &[MusicApiSettings]) -> Containers {
    container! {
        div #settings-music-api-settings-section {
            @for settings in settings {
                section {
                    h2 { (settings.name) }
                    (music_api_settings_content(settings, AuthState::Initial))
                }
            }
        }
    }
}

/// Authentication state for music API services.
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub enum AuthState {
    /// Initial state before authentication starts.
    #[default]
    Initial,
    /// Currently polling for authentication completion.
    Polling,
}

/// Renders the music API settings section content.
///
/// Displays authentication forms, scan controls, and status information for a music API.
#[must_use]
pub fn music_api_settings_content(
    settings: &MusicApiSettings,
    auth_state: AuthState,
) -> Containers {
    container! {
        @let id = format!("settings-{}", classify_name(&settings.id));
        div id=(id) {
            @if settings.auth_method.is_none() || settings.logged_in {
                div gap=10 {
                    @if settings.logged_in {
                        div { "Logged in!" }
                    }
                    @if settings.supports_scan {
                        @if settings.scan_enabled {
                            button
                                type=button
                                hx-post={"/music-api/scan?apiSource="(settings.id)}
                                hx-target=(Selector::Id(id))
                                #run-scan-button
                                border-radius=5
                                background=#111
                                border="2, #222"
                                padding-x=10
                                padding-y=5
                            {
                                "Run Scan"
                            }
                        } @else {
                            button
                                type=button
                                hx-post={"/music-api/enable-scan-origin?apiSource="(settings.id)}
                                hx-target=(Selector::Id(id))
                                #run-scan-button
                                border-radius=5
                                background=#111
                                border="2, #222"
                                padding-x=10
                                padding-y=5
                            {
                                "Enable scan origin"
                            }
                        }
                        (scan_error_message(&settings.id, None))
                    }
                }
            } @else if let Some(auth_method) = &settings.auth_method {
                form
                    hx-post={"/music-api/auth?apiSource="(settings.id)}
                    hx-target=(Selector::Id(id))
                {
                    @match auth_method {
                        AuthMethod::UsernamePassword => {
                            input type=hidden name="type" value="username-password";
                            input type=text name="username" placeholder="Username";
                            input type=password name="password" placeholder="Password";
                            button
                                type=submit
                                border-radius=5
                                background=#111
                                border="2, #222"
                                padding-x=10
                                padding-y=5
                            {
                                "Login"
                            }
                        }
                        AuthMethod::Poll => {
                            @match auth_state {
                                AuthState::Initial => {
                                    input type=hidden name="type" value="poll";
                                    button
                                        type=submit
                                        border-radius=5
                                        background=#111
                                        border="2, #222"
                                        padding-x=10
                                        padding-y=5
                                    {
                                        "Start web authentication"
                                    }
                                }
                                AuthState::Polling => {
                                    "Polling..."
                                }
                            }
                        }
                    }
                    (auth_error_message(&settings.id, None))
                }
            }
        }
    }
}

/// Renders a scan error message container.
///
/// Displays an error message if provided, or an empty container for dynamic updates.
#[must_use]
pub fn scan_error_message<T: AsRef<str>>(id: T, message: Option<&str>) -> Containers {
    let id = id.as_ref();
    container! {
        div id={"settings-scan-error-"(classify_name(id))} {
            @if let Some(message) = message {
                (message)
            }
        }
    }
}

/// Renders an authentication error message container.
///
/// Displays an error message if provided, or an empty container for dynamic updates.
#[must_use]
pub fn auth_error_message<T: AsRef<str>>(id: T, message: Option<&str>) -> Containers {
    let id = id.as_ref();
    container! {
        div id={"settings-auth-error-"(classify_name(id))} {
            @if let Some(message) = message {
                (message)
            }
        }
    }
}

/// Renders the connections list with edit and delete controls.
///
/// Displays all configured server connections with forms to modify or remove them.
#[must_use]
pub fn connections_content(
    connections: &[Connection],
    current_connection: Option<&Connection>,
) -> Containers {
    container! {
        div #settings-connections gap=10 {
            @for connection in connections {
                @let current_connection = current_connection.is_some_and(|x| x == connection);
                @let connection_input = |input, placeholder| connection_input(connection, input, placeholder);

                form
                    hx-patch={"/settings/connections?name="(connection.name)}
                    hx-target=(Selector::Id("settings-connections".to_string()))
                    width=100%
                    gap=5
                {
                    @if current_connection {
                        div { "(Selected)" }
                    }
                    div text-align=end {
                        div { "Name: " (connection_input(ConnectionInput::Name, Some("New connection"))) }
                        div { "API URL: " (connection_input(ConnectionInput::ApiUrl, None)) }
                    }
                    div
                        direction=row
                        justify-content=end
                        gap=5
                        width=100%
                    {
                        @if !current_connection {
                            button
                                border-radius=5
                                background=#111
                                border="2, #222"
                                padding-x=10
                                padding-y=5
                                hx-post={"/settings/select-connection?name="(connection.name)}
                                hx-target="#settings-connections"
                            {
                                "Select"
                            }
                        }
                        button
                            border-radius=5
                            background=#111
                            border="2, #222"
                            padding-x=10
                            padding-y=5
                            hx-delete={"/settings/connections?name="(connection.name)}
                            hx-target="#settings-connections"
                        {
                            "Delete"
                        }
                        button
                            type=submit
                            border-radius=5
                            background=#111
                            border="2, #222"
                            padding-x=10
                            padding-y=5
                        {
                            "Save"
                        }
                    }
                }
            }
        }
    }
}

/// Connection form input field types.
#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumString, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ConnectionInput {
    /// Connection name field.
    Name,
    /// API URL field.
    ApiUrl,
}

/// Renders an input field for connection configuration.
///
/// Creates a text input element with the current connection value pre-filled.
fn connection_input(
    connection: &Connection,
    input: ConnectionInput,
    placeholder: Option<&str>,
) -> Containers {
    container! {
        @let name = input.as_ref();
        @let value = match input {
            ConnectionInput::Name => connection.name.clone(),
            ConnectionInput::ApiUrl => connection.api_url.clone(),
        };
        input
            type=text
            placeholder=[placeholder]
            value=(value)
            id=(name)
            name=(name);
    }
}

/// Renders the complete settings page within the application layout.
#[must_use]
pub fn settings(
    state: &State,
    connection_name: &str,
    connections: &[Connection],
    selected: Option<&Connection>,
    music_api_settings: &[MusicApiSettings],
) -> Containers {
    page(
        state,
        &settings_page_content(connection_name, connections, selected, music_api_settings),
    )
}
