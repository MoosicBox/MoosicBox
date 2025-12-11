//! Download management UI components.
//!
//! This module provides UI templates for displaying download tasks, progress tracking,
//! and download history.

#![allow(clippy::module_name_repetitions)]

#[allow(unused_imports)]
use hyperchad::template as hyperchad_template;
use hyperchad::template::{Containers, container};
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

/// Download page tab selection.
#[derive(Debug, PartialEq, Eq, Clone, Copy, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadTab {
    /// Currently active download tasks.
    Current,
    /// Historical download tasks.
    History,
}

impl std::fmt::Display for DownloadTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Renders the progress indicator for a download task.
///
/// Displays bytes downloaded, total size, progress percentage, and download speed if available.
fn download_task_progress(task: &ApiDownloadTask) -> Containers {
    container! {
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
    }
}

/// Renders a download task card with cover art, title, and progress information.
///
/// Displays different information based on the download item type (track, album cover, or artist cover).
#[allow(clippy::too_many_lines)]
fn download_task(host: &str, task: &ApiDownloadTask) -> Containers {
    let id = task.id;
    let item = &task.item;

    container! {
        div
            direction=row
            gap=20
            background=#111
            align-items=center
            padding-x=18
            padding-y=20
            border-radius=6
        {
            @let cover_width = 80;
            @let cover_height = 80;
            @match item {
                ApiDownloadItem::Track { source, track_id, album_id, title, contains_cover, .. } => {
                    div {
                        anchor href=(album_page_url(&album_id.to_string(), false, Some(&source.into()), None, None, None)) {
                            image
                                src=(album_cover_url(host, album_id, &source.into(), *contains_cover, cover_width, cover_height))
                                width=(cover_width)
                                height=(cover_height);
                        }
                    }
                    div gap=5 {
                        div {
                            "Track (" (track_id.to_string()) ") - " (title) " - " (task.state.to_string()) " - " (source.as_ref())
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
                        anchor href=(album_page_url(&album_id.to_string(), false, Some(&source.into()), None, None, None)) {
                            image
                                src=(album_cover_url(host, album_id, &source.into(), *contains_cover, cover_width, cover_height))
                                width=(cover_width)
                                height=(cover_height);
                        }
                    }
                    div gap=5 {
                        div {
                            "Album (" (album_id.to_string()) ") cover - " (title) " - " (task.state.to_string())
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
                        anchor href=(album_page_url(&album_id.to_string(), false, Some(&source.into()), None, None, None)) {
                            image
                                src=(artist_cover_url(host, artist_id, &source.into(), *contains_cover, cover_width, cover_height))
                                width=(cover_width)
                                height=(cover_height);
                        }
                    }
                    div gap=5 {
                        div {
                            "Artist (" (artist_id.to_string()) ") (album_id: " (album_id.to_string()) ") cover - " (title) " - " (task.state.to_string())
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

/// Renders the downloads page content.
///
/// Displays download tasks with tabs for current and historical tasks.
#[must_use]
pub fn downloads_page_content(
    host: &str,
    tasks: &[ApiDownloadTask],
    active_tab: DownloadTab,
) -> Containers {
    container! {
        div
            padding-x=30
            padding-y=15
            background=(DARK_BACKGROUND)
            direction=row
            align-items=center
        {
            h1 { "Downloads" }
        }
        div padding-x=30 padding-y=5 {
            div direction=row gap=5 {
                anchor
                    href={"/downloads?tab="(DownloadTab::Current)}
                    background=(if active_tab == DownloadTab::Current { "#333" } else { "#282828" })
                    padding=10
                    cursor=pointer
                    border-top-radius=10
                {
                    "Current Tasks"
                }
                anchor
                    href={"/downloads?tab="(DownloadTab::History)}
                    background=(if active_tab == DownloadTab::History { "#333" } else { "#282828" })
                    padding=10
                    cursor=pointer
                    border-top-radius=10
                {
                    "History"
                }
            }
            div
                #downloads-content
                background=#333
                gap=10
                padding=10
                border-radius=10
                border-top-left-radius=0
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

/// Renders the complete downloads page within the application layout.
#[must_use]
pub fn downloads(state: &State, tasks: &[ApiDownloadTask], active_tab: DownloadTab) -> Containers {
    let Some(connection) = &state.connection else {
        return container! {};
    };

    page(
        state,
        &downloads_page_content(&connection.api_url, tasks, active_tab),
    )
}
