#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::AlignItems;
use maud::{Markup, html};
use moosicbox_downloader::api::models::{ApiDownloadItem, ApiDownloadTask, ApiDownloadTaskState};

use crate::{
    DARK_BACKGROUND, albums::album_cover_url, formatting::format_size, page, state::State,
};

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

fn download_task(task: &ApiDownloadTask) -> Markup {
    let id = task.id;
    let item = &task.item;

    html! {
        div sx-dir="row" sx-align-items=(AlignItems::Center) {
            @let cover_width = 80;
            @let cover_height = 80;
            @match item {
                ApiDownloadItem::Track { source, track_id, album_id, title, contains_cover, .. } => {
                    div sx-padding=(10) {
                        img
                            src=(album_cover_url(album_id, source.into(), *contains_cover, cover_width, cover_height))
                            sx-width=(cover_width)
                            sx-height=(cover_height)
                        {}
                    }
                    div sx-padding=(10) {
                        div sx-margin-y=(5) {
                            "Track (" (track_id) ") - " (title) " - " (task.state.to_string()) " - " (source.as_ref())
                            @if task.state == ApiDownloadTaskState::Error {
                                button hx-post=(format!("/retry-download?taskId={}", id)) {
                                    "Retry"
                                }
                            }
                        }
                        div sx-margin-y=(5) {
                            (task.file_path)
                        }
                        div sx-margin-y=(5) {
                            @if task.state == ApiDownloadTaskState::Started {
                                (download_task_progress(task))
                            }
                        }
                    }
                }
                ApiDownloadItem::AlbumCover { source, album_id, title, contains_cover, .. } => {
                    div sx-padding=(10) {
                        img
                            src=(album_cover_url(album_id, source.into(), *contains_cover, cover_width, cover_height))
                            sx-width=(cover_width)
                            sx-height=(cover_height)
                        {}
                    }
                    div sx-padding=(10) {
                        div sx-margin-y=(5) {
                            "Album (" (album_id) ") cover - " (title) " - " (task.state.to_string())
                            @if task.state == ApiDownloadTaskState::Error {
                                button hx-post=(format!("/retry-download?taskId={}", id)) {
                                    "Retry"
                                }
                            }
                        }
                        div sx-margin-y=(5) {
                            (task.file_path)
                        }
                        div sx-margin-y=(5) {
                            @if task.state == ApiDownloadTaskState::Started {
                                (download_task_progress(task))
                            }
                        }
                    }
                }
                ApiDownloadItem::ArtistCover { source, artist_id, album_id, title, contains_cover, .. } => {
                    div sx-padding=(10) {
                        img
                            src=(album_cover_url(album_id, source.into(), *contains_cover, cover_width, cover_height))
                            sx-width=(cover_width)
                            sx-height=(cover_height)
                        {}
                    }
                    div sx-padding=(10) {
                        div sx-margin-y=(5) {
                            "Artist (" (artist_id) ") (album_id: " (album_id) ") cover - " (title) " - " (task.state.to_string())
                            @if task.state == ApiDownloadTaskState::Error {
                                button hx-post=(format!("/retry-download?taskId={}", id)) {
                                    "Retry"
                                }
                            }
                        }
                        div sx-margin-y=(5) {
                            (task.file_path)
                        }
                        div sx-margin-y=(5) {
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
pub fn downloads_page_content(tasks: &[ApiDownloadTask]) -> Markup {
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
        div sx-padding-x=(30) sx-padding-y=(15) {
            @if tasks.is_empty() {
                "No download tasks"
            } @else {
                @for task in tasks {
                    (download_task(task))
                }
            }
        }
    }
}

#[must_use]
pub fn downloads(state: &State, tasks: &[ApiDownloadTask]) -> Markup {
    page(state, &downloads_page_content(tasks))
}
