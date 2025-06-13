#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::{AlignItems, JustifyContent, LayoutDirection, TextAlign};
use maud::{Markup, html};
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
) -> Markup {
    html! {
        div sx-padding=(20) sx-gap=(10) {
            section sx-align-items=(AlignItems::Start) {
                div sx-align-items=(AlignItems::End) sx-gap=(10) {
                    form
                        hx-post="/settings/connection-name"
                        sx-width="100%"
                        sx-align-items=(AlignItems::End)
                        sx-gap=(5)
                    {
                        div { "Name: " input type="text" name="name" value=(connection_name); }
                        button
                            type="submit"
                            sx-border-radius=(5)
                            sx-background="#111"
                            sx-border="2, #222"
                            sx-padding-x=(10)
                            sx-padding-y=(5)
                        {
                            "Save"
                        }
                    }

                    div sx-width="100%" sx-text-align=(TextAlign::Start) {
                        h2 { "Connections" }
                    }

                    (connections_content(connections, selected))

                    div
                        sx-dir=(LayoutDirection::Row)
                        sx-justify-content=(JustifyContent::Center)
                        sx-width="100%"
                        sx-gap=(5)
                    {
                        button
                            sx-border-radius=(5)
                            sx-background="#111"
                            sx-border="2, #222"
                            sx-padding-x=(10)
                            sx-padding-y=(5)
                            hx-post="/settings/new-connection"
                            hx-swap="#settings-connections"
                        {
                            "New Connection"
                        }
                    }
                }
            }

            hr;

            section hx-get=(pre_escaped!("/settings/download-settings")) hx-trigger="load" {
                div id="settings-download-settings-section" {}
            }

            hr;

            section hx-get=(pre_escaped!("/settings/scan-settings")) hx-trigger="load" {
                div id="settings-scan-settings-section" {}
            }

            hr;

            section hx-get=(pre_escaped!("/settings/music-api-settings")) hx-trigger="load" {
                (music_api_settings_section(&music_api_settings))
            }
        }
    }
}

#[must_use]
pub fn scan_settings_content(scan_settings: &ScanSettings) -> Markup {
    html! {
        div id="settings-scan-settings-section" {
            h2 { "Scan Settings" }
            h3 { "Scan paths:" }
            @if scan_settings.scan_paths.is_empty() {
                "No scan paths"
            } @else {
                ul {
                    @for path in &scan_settings.scan_paths {
                        li {
                            (path)
                            form hx-delete=(pre_escaped!("/settings/scan/download-location")) {
                                input type="hidden" name="location" value=(path);
                                button
                                    type="submit"
                                    sx-border-radius=(5)
                                    sx-background="#111"
                                    sx-border="2, #222"
                                    sx-padding-x=(10)
                                    sx-padding-y=(5)
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
pub fn download_settings_content(download_settings: &DownloadSettings) -> Markup {
    html! {
        div id="settings-download-settings-section" {
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
                            form hx-delete=(pre_escaped!("/settings/download/download-location")) {
                                input type="hidden" name="location" value=(location);
                                button
                                    type="submit"
                                    sx-border-radius=(5)
                                    sx-background="#111"
                                    sx-border="2, #222"
                                    sx-padding-x=(10)
                                    sx-padding-y=(5)
                                {
                                    "Delete"
                                }
                            }
                            @if !is_default {
                                form hx-post=(pre_escaped!("/settings/download/default-download-location")) {
                                    input type="hidden" name="location" value=(location);
                                    button
                                        type="submit"
                                        sx-border-radius=(5)
                                        sx-background="#111"
                                        sx-border="2, #222"
                                        sx-padding-x=(10)
                                        sx-padding-y=(5)
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
pub fn music_api_settings_section(settings: &[MusicApiSettings]) -> Markup {
    html! {
        div id="settings-music-api-settings-section" {
            @for settings in settings {
                hr;

                section {
                    h2 { (settings.name) }
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
pub fn music_api_settings_content(settings: &MusicApiSettings, auth_state: AuthState) -> Markup {
    html! {
        @let id = format!("settings-{}", classify_name(&settings.id));
        div id=(id) {
            @if settings.auth_method.is_none() || settings.logged_in {
                div sx-gap=(10) {
                    @if settings.logged_in {
                        div { "Logged in!" }
                    }
                    @if settings.supports_scan {
                        @if settings.scan_enabled {
                            button
                                type="button"
                                hx-post={(pre_escaped!("/music-api/scan?apiSource="))(settings.id)}
                                hx-swap={"#"(id)}
                                id="run-scan-button"
                                sx-border-radius=(5)
                                sx-background="#111"
                                sx-border="2, #222"
                                sx-padding-x=(10)
                                sx-padding-y=(5)
                            {
                                "Run Scan"
                            }
                        } @else {
                            button
                                type="button"
                                hx-post={(pre_escaped!("/music-api/enable-scan-origin?apiSource="))(settings.id)}
                                hx-swap={"#"(id)}
                                id="run-scan-button"
                                sx-border-radius=(5)
                                sx-background="#111"
                                sx-border="2, #222"
                                sx-padding-x=(10)
                                sx-padding-y=(5)
                            {
                                "Enable scan origin"
                            }
                        }
                        (scan_error_message(&settings.id, None))
                    }
                }
            } @else if let Some(auth_method) = &settings.auth_method {
                form
                    hx-post={(pre_escaped!("/music-api/auth?apiSource="))(settings.id)}
                    hx-swap={"#"(id)}
                {
                    @match auth_method {
                        AuthMethod::UsernamePassword => {
                            input type="hidden" name="type" value="username-password";
                            input type="text" name="username" placeholder="Username";
                            input type="password" name="password" placeholder="Password";
                            button
                                type="submit"
                                sx-border-radius=(5)
                                sx-background="#111"
                                sx-border="2, #222"
                                sx-padding-x=(10)
                                sx-padding-y=(5)
                            {
                                "Login"
                            }
                        }
                        AuthMethod::Poll => {
                            @match auth_state {
                                AuthState::Initial => {
                                    input type="hidden" name="type" value="poll";
                                    button
                                        type="submit"
                                        sx-border-radius=(5)
                                        sx-background="#111"
                                        sx-border="2, #222"
                                        sx-padding-x=(10)
                                        sx-padding-y=(5)
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
pub fn scan_error_message<T: AsRef<str>>(id: T, message: Option<&str>) -> Markup {
    let id = id.as_ref();
    html! {
        div id={"settings-scan-error-"(classify_name(id))} {
            @if let Some(message) = message {
                (message)
            }
        }
    }
}

#[must_use]
pub fn auth_error_message<T: AsRef<str>>(id: T, message: Option<&str>) -> Markup {
    let id = id.as_ref();
    html! {
        div id={"settings-auth-error-"(classify_name(id))} {
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
) -> Markup {
    html! {
        div id="settings-connections" sx-gap=(10) {
            @for connection in connections {
                @let current_connection = current_connection.is_some_and(|x| x == connection);
                @let connection_input = |input, placeholder| connection_input(connection, input, placeholder);

                form
                    hx-patch={(pre_escaped!("/settings/connections?name="))(connection.name)}
                    hx-swap="#settings-connections"
                    sx-width="100%"
                    sx-gap=(5)
                {
                    @if current_connection {
                        div { "(Selected)" }
                    }
                    div sx-text-align=(TextAlign::End) {
                        div { "Name: " (connection_input(ConnectionInput::Name, Some("New connection"))) }
                        div { "API URL: " (connection_input(ConnectionInput::ApiUrl, None)) }
                    }
                    div
                        sx-dir=(LayoutDirection::Row)
                        sx-justify-content=(JustifyContent::End)
                        sx-gap=(5)
                        sx-width="100%"
                    {
                        @if !current_connection {
                            button
                                sx-border-radius=(5)
                                sx-background="#111"
                                sx-border="2, #222"
                                sx-padding-x=(10)
                                sx-padding-y=(5)
                                hx-post={(pre_escaped!("/settings/select-connection?name="))(connection.name)}
                                hx-swap="#settings-connections"
                            {
                                "Select"
                            }
                        }
                        button
                            sx-border-radius=(5)
                            sx-background="#111"
                            sx-border="2, #222"
                            sx-padding-x=(10)
                            sx-padding-y=(5)
                            hx-delete={(pre_escaped!("/settings/connections?name="))(connection.name)}
                            hx-swap="#settings-connections"
                        {
                            "Delete"
                        }
                        button
                            type="submit"
                            sx-border-radius=(5)
                            sx-background="#111"
                            sx-border="2, #222"
                            sx-padding-x=(10)
                            sx-padding-y=(5)
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
) -> Markup {
    html! {
        @let name = input.as_ref();
        @let value = match input {
            ConnectionInput::Name => connection.name.clone(),
            ConnectionInput::ApiUrl => connection.api_url.clone(),
        };
        input
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
        &settings_page_content(connection_name, connections, selected, music_api_settings),
    )
}
