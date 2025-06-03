#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::{AlignItems, JustifyContent, LayoutDirection, TextAlign};
use maud::{Markup, html};
use moosicbox_app_models::{Connection, MusicApiSettings};
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

            section {
                h2 { "Scan Settings" }
                div { "Scan settings content will go here" }
            }

            hr;

            section {
                h2 { "Download Settings" }
                div { "Download settings content will go here" }
            }

            @for settings in music_api_settings {
                hr;

                section {
                    h2 { (settings.name) }
                    (music_api_settings_content(settings))
                }
            }
        }
    }
}

#[must_use]
pub fn music_api_settings_content(settings: &MusicApiSettings) -> Markup {
    html! {
        // FIXME: HTML encode settings.id
        @let id = format!("settings-{}", classify_name(&settings.id));
        div id=(id) {
            @if !settings.authentication_enabled || settings.logged_in {
                div sx-gap=(10) {
                    @if settings.logged_in {
                        p { "Logged in!" }
                    }
                    button
                        hx-post={(pre_escaped!("/music-api/scan?apiSource="))(settings.id)}
                        hx-swap={"#"(id)}
                        id="run-scan-button"
                        type="button"
                        sx-border-radius=(5)
                        sx-background="#111"
                        sx-border="2, #222"
                        sx-padding-x=(10)
                        sx-padding-y=(5)
                    {
                        "Run Scan"
                    }
                    (scan_error_message(&settings.id, None))
                }
            } @else {
                button
                    hx-post={(pre_escaped!("/music-api/auth?apiSource="))(settings.id)}
                    hx-swap={"#"(id)}
                    type="button"
                    sx-border-radius=(5)
                    sx-background="#111"
                    sx-border="2, #222"
                    sx-padding-x=(10)
                    sx-padding-y=(5)
                {
                    "Start web authentication"
                }
                (auth_error_message(&settings.id, None))
            }
        }
    }
}

#[must_use]
pub fn scan_error_message(id: &str, message: Option<&str>) -> Markup {
    html! {
        div id={"settings-scan-error-"(classify_name(id))} {
            @if let Some(message) = message {
                (message)
            }
        }
    }
}

#[must_use]
pub fn auth_error_message(id: &str, message: Option<&str>) -> Markup {
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
