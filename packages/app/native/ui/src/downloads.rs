#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::{AlignItems, Cursor};
use maud::{Markup, html};
use moosicbox_downloader::api::models::{ApiDownloadItem, ApiDownloadTask, ApiDownloadTaskState};
use strum::{AsRefStr, EnumString};

use crate::{
    DARK_BACKGROUND,
    albums::{album_cover_url, album_page_url},
    artists::artist_cover_url,
    formatting::format_size,
    page,
    state::State,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadTab {
    Current,
    History,
}

impl std::fmt::Display for DownloadTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

fn download_task_progress(task: &ApiDownloadTask) -> Markup {
    html! {
        div {
            @if let Some(total_bytes) = task.total_bytes {
                (format!("{}/{} MiB - ", format_size(task.bytes), format_size(total_bytes)))
            } @else {
                ""
            }
            {(task.progress as u64)}"%"
            @if let Some(speed) = task.speed {
                (format!(" - {} KiB/s", format_size(speed)))
            } @else {
                ""
            }
        }
        div style=(format!("width: {}%", task.progress)) {}
    }
}

#[allow(clippy::too_many_lines)]
fn download_task(host: &str, task: &ApiDownloadTask) -> Markup {
    let id = task.id;
    let item = &task.item;

    html! {
        div
            sx-dir="row"
            sx-gap=(20)
            sx-background="#111"
            sx-align-items=(AlignItems::Center)
            sx-padding-x=(18)
            sx-padding-y=(20)
            sx-border-radius=(6)
        {
            @let cover_width = 80;
            @let cover_height = 80;
            @match item {
                ApiDownloadItem::Track { source, track_id, album_id, title, contains_cover, .. } => {
                    div {
                        a href=(album_page_url(&album_id.to_string(), false, Some(source.into()), None, None, None)) {
                            img
                                src=(album_cover_url(host, album_id, source.into(), *contains_cover, cover_width, cover_height))
                                sx-width=(cover_width)
                                sx-height=(cover_height)
                            {}
                        }
                    }
                    div sx-gap=(5) {
                        div {
                            "Track (" (track_id) ") - " (title) " - " (task.state.to_string()) " - " (source.as_ref())
                            @if task.state == ApiDownloadTaskState::Error {
                                button hx-post=(format!("/retry-download?taskId={}", id)) {
                                    "Retry"
                                }
                            }
                        }
                        div {
                            (task.file_path)
                        }
                        div {
                            @if task.state == ApiDownloadTaskState::Started {
                                (download_task_progress(task))
                            }
                        }
                    }
                }
                ApiDownloadItem::AlbumCover { source, album_id, title, contains_cover, .. } => {
                    div {
                        a href=(album_page_url(&album_id.to_string(), false, Some(source.into()), None, None, None)) {
                            img
                                src=(album_cover_url(host, album_id, source.into(), *contains_cover, cover_width, cover_height))
                                sx-width=(cover_width)
                                sx-height=(cover_height)
                            {}
                        }
                    }
                    div sx-gap=(5) {
                        div {
                            "Album (" (album_id) ") cover - " (title) " - " (task.state.to_string())
                            @if task.state == ApiDownloadTaskState::Error {
                                button hx-post=(format!("/retry-download?taskId={}", id)) {
                                    "Retry"
                                }
                            }
                        }
                        div {
                            (task.file_path)
                        }
                        div {
                            @if task.state == ApiDownloadTaskState::Started {
                                (download_task_progress(task))
                            }
                        }
                    }
                }
                ApiDownloadItem::ArtistCover { source, artist_id, album_id, title, contains_cover, .. } => {
                    div {
                        a href=(album_page_url(&album_id.to_string(), false, Some(source.into()), None, None, None)) {
                            img
                                src=(artist_cover_url(host, artist_id, source.into(), *contains_cover, cover_width, cover_height))
                                sx-width=(cover_width)
                                sx-height=(cover_height)
                            {}
                        }
                    }
                    div sx-gap=(5) {
                        div {
                            "Artist (" (artist_id) ") (album_id: " (album_id) ") cover - " (title) " - " (task.state.to_string())
                            @if task.state == ApiDownloadTaskState::Error {
                                button hx-post=(format!("/retry-download?taskId={}", id)) {
                                    "Retry"
                                }
                            }
                        }
                        div {
                            (task.file_path)
                        }
                        div {
                            @if task.state == ApiDownloadTaskState::Started {
                                (download_task_progress(task))
                            }
                        }
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn downloads_page_content(
    host: &str,
    tasks: &[ApiDownloadTask],
    active_tab: DownloadTab,
) -> Markup {
    html! {
        div
            sx-padding-x=(30)
            sx-padding-y=(15)
            sx-background=(DARK_BACKGROUND)
            sx-dir="row"
            sx-align-items=(AlignItems::Center)
        {
            h1 { "Downloads" }
        }
        div sx-padding-x=(30) sx-padding-y=(5) {
            div sx-dir="row" sx-gap=(5) {
                a
                    href={"/downloads?tab="(DownloadTab::Current)}
                    sx-background=(if active_tab == DownloadTab::Current { "#333" } else { "#282828" })
                    sx-padding=(10)
                    sx-cursor=(Cursor::Pointer)
                    sx-border-top-radius=(10)
                {
                    "Current Tasks"
                }
                a
                    href={"/downloads?tab="(DownloadTab::History)}
                    sx-background=(if active_tab == DownloadTab::History { "#333" } else { "#282828" })
                    sx-padding=(10)
                    sx-cursor=(Cursor::Pointer)
                    sx-border-top-radius=(10)
                {
                    "History"
                }
            }
            div
                id="downloads-content"
                sx-background="#333"
                sx-gap=(10)
                sx-padding=(10)
                sx-border-radius=(10)
                sx-border-top-left-radius=(0)
            {
                @if tasks.is_empty() {
                    "No download tasks"
                } @else {
                    @for task in tasks {
                        (download_task(host, task))
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn downloads(state: &State, tasks: &[ApiDownloadTask], active_tab: DownloadTab) -> Markup {
    let Some(connection) = &state.connection else {
        return html! {};
    };

    page(
        state,
        &downloads_page_content(&connection.api_url, tasks, active_tab),
    )
}
