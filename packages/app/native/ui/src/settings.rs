#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::{AlignItems, JustifyContent, LayoutDirection, TextAlign};
use hyperchad_template::Markup;
use hyperchad_template2::{Containers, container};
use hyperchad_transformer_models::SwapTarget;
use moosicbox_app_models::{
    AuthMethod, Connection, DownloadSettings, MusicApiSettings, ScanSettings,
};
use strum::{AsRefStr, EnumString};

use crate::{formatting::classify_name, page, pre_escaped, state::State};

#[must_use]
pub fn settings_page_content(
    connection_name: &str,
    connections: &[Connection],
    selected: Option<&Connection>,
    music_api_settings: &[MusicApiSettings],
) -> Containers {
    container! {
        Div padding=(20) gap=(10) {
            Section align-items=(AlignItems::Start) {
                Div align-items=(AlignItems::End) gap=(10) {
                    Form
                        hx-post="/settings/connection-name"
                        width="100%"
                        align-items=(AlignItems::End)
                        gap=(5)
                    {
                        Div { "Name: " Input type="text" name="name" value=(connection_name); }
                        Button
                            type="submit"
                            border-radius=(5)
                            background="#111"
                            border="2, #222"
                            padding-x=(10)
                            padding-y=(5)
                        {
                            "Save"
                        }
                    }

                    Div width="100%" text-align=(TextAlign::Start) {
                        H2 { "Connections" }
                    }

                    (connections_content(connections, selected))

                    Div
                        direction=(LayoutDirection::Row)
                        justify-content=(JustifyContent::Center)
                        width="100%"
                        gap=(5)
                    {
                        Button
                            border-radius=(5)
                            background="#111"
                            border="2, #222"
                            padding-x=(10)
                            padding-y=(5)
                            hx-post="/settings/new-connection"
                            hx-swap="#settings-connections"
                        {
                            "New Connection"
                        }
                    }
                }
            }

            Section hx-get="/settings/download-settings" hx-trigger="load" {
                Div id="settings-download-settings-section" {}
            }

            Section hx-get="/settings/scan-settings" hx-trigger="load" {
                Div id="settings-scan-settings-section" {}
            }

            Section hx-get="/settings/music-api-settings" hx-trigger="load" {
                (music_api_settings_section(&music_api_settings))
            }
        }
    }
}

#[must_use]
pub fn scan_settings_content(scan_settings: &ScanSettings) -> Containers {
    container! {
        Div id="settings-scan-settings-section" {
            H2 { "Scan Settings" }
            H3 { "Scan paths:" }
            @if scan_settings.scan_paths.is_empty() {
                "No scan paths"
            } @else {
                Ul {
                    @for path in &scan_settings.scan_paths {
                        Li {
                            (path)
                            Form hx-delete="/settings/scan/scan-path" {
                                Input type="hidden" name="path" value=(path);
                                Button
                                    type="submit"
                                    border-radius=(5)
                                    background="#111"
                                    border="2, #222"
                                    padding-x=(10)
                                    padding-y=(5)
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

#[must_use]
pub fn download_settings_content(download_settings: &DownloadSettings) -> Containers {
    container! {
        Div id="settings-download-settings-section" {
            H2 { "Scan Settings" }
            H3 { "Download locations:" }
            @if download_settings.download_locations.is_empty() {
                "No download locations"
            } @else {
                Ul {
                    @for (_id, location) in &download_settings.download_locations {
                        @let is_default = download_settings.default_download_location.as_ref().is_some_and(|x| x == location);

                        Li {
                            (location)
                            Form hx-delete="/settings/download/download-location" {
                                Input type="hidden" name="location" value=(location);
                                Button
                                    type="submit"
                                    border-radius=(5)
                                    background="#111"
                                    // border="2, #222"
                                    padding-x=(10)
                                    padding-y=(5)
                                {
                                    "Delete"
                                }
                            }
                            @if !is_default {
                                Form hx-post="/settings/download/default-download-location" {
                                    Input type="hidden" name="location" value=(location);
                                    Button
                                        type="submit"
                                        border-radius=(5)
                                        background="#111"
                                        // border="2, #222"
                                        padding-x=(10)
                                        padding-y=(5)
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

#[must_use]
pub fn music_api_settings_section(settings: &[MusicApiSettings]) -> Containers {
    container! {
        Div id="settings-music-api-settings-section" {
            @for settings in settings {
                Section {
                    H2 { (settings.name) }
                    (music_api_settings_content(settings, AuthState::Initial))
                }
            }
        }
    }
}

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
pub enum AuthState {
    #[default]
    Initial,
    Polling,
}

#[must_use]
pub fn music_api_settings_content(
    settings: &MusicApiSettings,
    auth_state: AuthState,
) -> Containers {
    container! {
        @let id = format!("settings-{}", classify_name(&settings.id));
        Div id=(id) {
            @if settings.auth_method.is_none() || settings.logged_in {
                Div gap=(10) {
                    @if settings.logged_in {
                        Div { "Logged in!" }
                    }
                    @if settings.supports_scan {
                        @if settings.scan_enabled {
                            Button
                                type="button"
                                hx-post={"/music-api/scan?apiSource="(settings.id)}
                                hx-swap=(SwapTarget::Id(id))
                                id="run-scan-button"
                                border-radius=(5)
                                background="#111"
                                border="2, #222"
                                padding-x=(10)
                                padding-y=(5)
                            {
                                "Run Scan"
                            }
                        } @else {
                            Button
                                type="button"
                                hx-post={"/music-api/enable-scan-origin?apiSource="(settings.id)}
                                hx-swap=(SwapTarget::Id(id))
                                id="run-scan-button"
                                border-radius=(5)
                                background="#111"
                                border="2, #222"
                                padding-x=(10)
                                padding-y=(5)
                            {
                                "Enable scan origin"
                            }
                        }
                        (scan_error_message(&settings.id, None))
                    }
                }
            } @else if let Some(auth_method) = &settings.auth_method {
                Form
                    hx-post={"/music-api/auth?apiSource="(settings.id)}
                    hx-swap=(SwapTarget::Id(id))
                {
                    @match auth_method {
                        AuthMethod::UsernamePassword => {
                            Input type="hidden" name="type" value="username-password";
                            Input type="text" name="username" placeholder="Username";
                            Input type="password" name="password" placeholder="Password";
                            Button
                                type="submit"
                                border-radius=(5)
                                background="#111"
                                border="2, #222"
                                padding-x=(10)
                                padding-y=(5)
                            {
                                "Login"
                            }
                        }
                        AuthMethod::Poll => {
                            @match auth_state {
                                AuthState::Initial => {
                                    Input type="hidden" name="type" value="poll";
                                    Button
                                        type="submit"
                                        border-radius=(5)
                                        background="#111"
                                        border="2, #222"
                                        padding-x=(10)
                                        padding-y=(5)
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

#[must_use]
pub fn scan_error_message<T: AsRef<str>>(id: T, message: Option<&str>) -> Containers {
    let id = id.as_ref();
    container! {
        Div id={"settings-scan-error-"(classify_name(id))} {
            @if let Some(message) = message {
                (message)
            }
        }
    }
}

#[must_use]
pub fn auth_error_message<T: AsRef<str>>(id: T, message: Option<&str>) -> Containers {
    let id = id.as_ref();
    container! {
        Div id={"settings-auth-error-"(classify_name(id))} {
            @if let Some(message) = message {
                (message)
            }
        }
    }
}

#[must_use]
pub fn connections_content(
    connections: &[Connection],
    current_connection: Option<&Connection>,
) -> Containers {
    container! {
        Div id="settings-connections" gap=(10) {
            @for connection in connections {
                @let current_connection = current_connection.is_some_and(|x| x == connection);
                @let connection_input = |input, placeholder| connection_input(connection, input, placeholder);

                Form
                    hx-patch={"/settings/connections?name="(connection.name)}
                    hx-swap=(SwapTarget::Id("settings-connections".to_string()))
                    width="100%"
                    gap=(5)
                {
                    @if current_connection {
                        Div { "(Selected)" }
                    }
                    Div text-align=(TextAlign::End) {
                        Div { "Name: " (connection_input(ConnectionInput::Name, Some("New connection"))) }
                        Div { "API URL: " (connection_input(ConnectionInput::ApiUrl, None)) }
                    }
                    Div
                        direction=(LayoutDirection::Row)
                        justify-content=(JustifyContent::End)
                        gap=(5)
                        width="100%"
                    {
                        @if !current_connection {
                            Button
                                border-radius=(5)
                                background="#111"
                                border="2, #222"
                                padding-x=(10)
                                padding-y=(5)
                                hx-post={"/settings/select-connection?name="(connection.name)}
                                hx-swap="#settings-connections"
                            {
                                "Select"
                            }
                        }
                        Button
                            border-radius=(5)
                            background="#111"
                            border="2, #222"
                            padding-x=(10)
                            padding-y=(5)
                            hx-delete={"/settings/connections?name="(connection.name)}
                            hx-swap="#settings-connections"
                        {
                            "Delete"
                        }
                        Button
                            type="submit"
                            border-radius=(5)
                            background="#111"
                            border="2, #222"
                            padding-x=(10)
                            padding-y=(5)
                        {
                            "Save"
                        }
                    }
                }
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, EnumString, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ConnectionInput {
    Name,
    ApiUrl,
}

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
        Input
            type="text"
            placeholder=[placeholder]
            value=(value)
            id=(name)
            name=(name);
    }
}

#[must_use]
pub fn settings(
    state: &State,
    connection_name: &str,
    connections: &[Connection],
    selected: Option<&Connection>,
    music_api_settings: &[MusicApiSettings],
) -> Markup {
    page(
        state,
        &pre_escaped!(
            "{}",
            settings_page_content(connection_name, connections, selected, music_api_settings)
                .into_iter()
                .map(|c| c.to_string())
                .collect::<String>()
        ),
    )
}
