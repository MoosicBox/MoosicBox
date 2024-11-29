#![allow(clippy::module_name_repetitions)]

use gigachad_actions::StyleAction;
use gigachad_transformer_models::Visibility;
use maud::{html, Markup, PreEscaped};
use moosicbox_core::sqlite::models::{AlbumVersionQuality, ApiAlbum, ApiSource, TrackApiSource};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_paging::Page;

use crate::{formatting::TimeFormat as _, page, pre_escaped, public_img, state::State, Action};

pub fn album_cover_url(album: &ApiAlbum, width: u16, height: u16) -> String {
    if album.contains_cover {
        let api_source: ApiSource = album.album_source.into();
        format!(
            "{}/files/albums/{}/{width}x{height}?moosicboxProfile=master&source={}",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8500"),
            album.album_id,
            api_source,
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

#[must_use]
pub fn album_cover_img(album: &ApiAlbum, size: u16) -> Markup {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        img src=(PreEscaped(album_cover_url(album, request_size, request_size))) sx-width=(size) sx-height=(size);
    }
}

#[must_use]
pub fn album_page_immediate(
    album_id: &str,
    source: Option<ApiSource>,
    version_source: Option<TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> Markup {
    let path = album_page_url(
        album_id,
        true,
        source,
        version_source,
        sample_rate,
        bit_depth,
    );
    html! {
        div hx-get=(path) hx-trigger="load" {
            div sx-dir="row" {
                @let size = 200;
                div sx-width=(size) sx-height=(size + 30) {
                    img src=(public_img!("album.svg")) sx-width=(size) sx-height=(size);
                }
                div {
                    h1 { ("loading...") }
                    h2 { ("loading...") }
                }
            }
            div {
                table {
                    thead {
                        tr{
                            th { ("#") }
                            th { ("Title") }
                            th { ("Artist") }
                            th { ("Time") }
                        }
                    }
                    tbody {
                        tr {
                            td { ("loading...") }
                            td { ("loading...") }
                            td { a { ("loading...") } }
                            td { ("loading...") }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn album_page_content(
    album: &ApiAlbum,
    versions: &[ApiAlbumVersion],
    selected_version: Option<&ApiAlbumVersion>,
) -> Markup {
    fn same_version(a: &AlbumVersionQuality, b: &AlbumVersionQuality) -> bool {
        a.source == b.source && a.sample_rate == b.sample_rate && a.bit_depth == b.bit_depth
    }

    let selected_version = versions
        .iter()
        .find(|x| selected_version.is_some_and(|v| same_version(&(*x).into(), &v.into())))
        .or_else(|| versions.first());

    html! {
        div sx-dir="row" {
            @let size = 200;
            div sx-width=(size) sx-height=(size + 30) {
                (album_cover_img(&album, size))
            }
            div {
                h1 { (album.title) }
                h2 { (album.artist) }
                div sx-dir="row" {
                    @for version in &album.versions {
                        @let selected = selected_version.is_some_and(|x| same_version(version, &x.into()));
                        a href=(
                            album_page_url(
                                &album.album_id.to_string(),
                                false,
                                Some(album.api_source),
                                Some(version.source),
                                version.sample_rate,
                                version.bit_depth,
                            )
                        ) {
                            h3 {
                                (if selected { "*" } else { "" })
                                (version.source)
                                (if selected { "*" } else { "" })
                            }
                        }
                    }
                }
            }
        }
        div sx-dir="row" {
            button
                sx-dir="row"
                sx-width=(130)
                sx-height=(40)
                sx-background="#fff"
                sx-border-radius=(5)
                fx-click=(Action::PlayAlbum {
                    album_id: album.album_id.clone(),
                    api_source: album.api_source,
                    version_source: selected_version.map(|x| x.source),
                    sample_rate: selected_version.and_then(|x| x.sample_rate),
                    bit_depth: selected_version.and_then(|x| x.bit_depth),
                })
            {
                @let icon_size = 12;
                img
                    sx-width=(icon_size)
                    sx-height=(icon_size)
                    src=(public_img!("play-button.svg"));
                ("Play")
            }
            button
                sx-dir="row"
                sx-width=(130)
                sx-height=(40)
                sx-background="#fff"
                sx-border-radius=(5)
                fx-click=(Action::AddAlbumToQueue {
                    album_id: album.album_id.clone(),
                    api_source: album.api_source,
                    version_source: selected_version.map(|x| x.source),
                    sample_rate: selected_version.and_then(|x| x.sample_rate),
                    bit_depth: selected_version.and_then(|x| x.bit_depth),
                })
            {
                @let icon_size = 20;
                img
                    sx-width=(icon_size)
                    sx-height=(icon_size)
                    src=(public_img!("more-options.svg"));
                ("Options")
            }
        }
        @if let Some(version) = selected_version {
            div {
                table {
                    thead {
                        tr{
                            th { ("#") }
                            th { ("Title") }
                            th { ("Artist") }
                            th { ("Time") }
                        }
                    }
                    tbody {
                        @for track in &version.tracks {
                            tr {
                                td { (track.number) }
                                td { (track.title) }
                                td { a href=(pre_escaped!("/artists?artistId={}&source={}", track.artist_id, track.api_source)) { (track.artist) } }
                                td { (track.duration.into_formatted()) }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn album(
    state: &State,
    album_id: &str,
    source: Option<ApiSource>,
    version_source: Option<TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> Markup {
    page(
        state,
        &album_page_immediate(album_id, source, version_source, sample_rate, bit_depth),
    )
}

#[must_use]
pub fn albums_list_start(albums: &Page<ApiAlbum>, size: u16) -> Markup {
    static MAX_PARALLEL_REQUESTS: u32 = 6;
    static MIN_PAGE_THRESHOLD: u32 = 30;
    let limit = albums.limit();
    let offset = albums.offset() + limit;
    let remaining = if albums.has_more() {
        albums.remaining().map_or_else(
            || {
                html! {
                    div
                        hx-get=(pre_escaped!("/albums-list-start?offset={offset}&limit={limit}&size={size}"))
                        hx-trigger="load"
                        sx-hidden=(true)
                    {}
                }
            },
            |remaining| {
                let limit = remaining / MAX_PARALLEL_REQUESTS;
                let last = limit + (remaining % MAX_PARALLEL_REQUESTS);

                html! {
                    @if limit < MIN_PAGE_THRESHOLD {
                        div
                            hx-get=(pre_escaped!("/albums-list?offset={offset}&limit={remaining}&size={size}"))
                            hx-trigger="load"
                            sx-hidden=(true)
                        {}
                    } @else {
                        @for i in 0..MAX_PARALLEL_REQUESTS {
                            @if i == MAX_PARALLEL_REQUESTS - 1 {
                                div
                                    hx-get=(pre_escaped!("/albums-list?offset={}&limit={last}&size={size}", offset + i * limit))
                                    hx-trigger="load"
                                    sx-hidden=(true)
                                {}
                            } @else {
                                div
                                    hx-get=(pre_escaped!("/albums-list?offset={}&limit={limit}&size={size}", offset + i * limit))
                                    hx-trigger="load"
                                    sx-hidden=(true)
                                {}
                            }
                        }
                    }
                }
            },
        )
    } else {
        html! {}
    };

    html! {
        (show_albums(albums.iter(), size))
        (remaining)
    }
}

#[must_use]
pub fn albums_list(albums: &Page<ApiAlbum>, size: u16) -> Markup {
    show_albums(albums.iter(), size)
}

#[must_use]
pub fn album_display(
    album: &ApiAlbum,
    size: u16,
    show_details: bool,
    show_media_controls: bool,
) -> Markup {
    let details = if show_details {
        html! { (album.title) }
    } else {
        html! {}
    };

    let album_cover = if show_media_controls {
        html! {
            div sx-width=(size) sx-height=(size) sx-position="relative" {
                (album_cover_img(album, size))
                div
                    sx-width=(size)
                    sx-height=(size)
                    sx-position="absolute"
                    sx-visibility="hidden"
                    fx-hover=(StyleAction::visibility_self(Visibility::Visible))
                {
                    @let button_size = size / 4;
                    @let icon_size = size / 10;
                    button
                        sx-dir="row"
                        sx-position="absolute"
                        sx-bottom="5%"
                        sx-left="5%"
                        sx-width=(button_size)
                        sx-height=(button_size)
                        sx-justify-content="center"
                        sx-align-items="center"
                        sx-background="#fff"
                        sx-border-radius="100%"
                        fx-click=(Action::PlayAlbum {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source,
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        img
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            src=(public_img!("play-button.svg"));
                    }
                    @let icon_size = size / 7;
                    button
                        sx-dir="row"
                        sx-position="absolute"
                        sx-bottom="5%"
                        sx-right="5%"
                        sx-width=(button_size)
                        sx-height=(button_size)
                        sx-justify-content="center"
                        sx-align-items="center"
                        sx-background="#fff"
                        sx-border-radius="100%"
                        fx-click=(Action::AddAlbumToQueue {
                            album_id: album.album_id.clone(),
                            api_source: album.api_source,
                            version_source: None,
                            sample_rate: None,
                            bit_depth: None,
                        })
                    {
                        img
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            src=(public_img!("more-options.svg"));
                    }
                }
            }
        }
    } else {
        html! { (album_cover_img(album, size)) }
    };

    html! {
        div sx-width=(size) sx-height=(size + if show_details { 30 } else { 0 }) {
            (album_cover)
            (details)
        }
    }
}

#[must_use]
pub fn album_page_url(
    album_id: &str,
    full: bool,
    api_source: Option<ApiSource>,
    version_source: Option<TrackApiSource>,
    sample_rate: Option<u32>,
    bit_depth: Option<u8>,
) -> PreEscaped<String> {
    pre_escaped!(
        "/albums?albumId={album_id}{}{}{}{}{}",
        if full { "&full=true" } else { "" },
        api_source.map_or_else(String::new, |x| format!("&source={x}")),
        version_source.map_or_else(String::new, |x| format!("&versionSource={x}")),
        sample_rate.map_or_else(String::new, |x| format!("&sampleRate={x}")),
        bit_depth.map_or_else(String::new, |x| format!("&bitDepth={x}")),
    )
}

pub fn show_albums<'a>(albums: impl Iterator<Item = &'a ApiAlbum>, size: u16) -> Markup {
    html! {
        @for album in albums {
            a
                href=(album_page_url(&album.album_id.to_string(), false, None, None, None, None))
                sx-width=(size)
                sx-height=(size + 30)
            {
                (album_display(album, size, true, true))
            }
        }
    }
}

#[must_use]
pub fn albums_page_content() -> Markup {
    let size: u16 = 200;

    html! {
        h1 sx-height=(36) { ("Albums") }
        div sx-dir="row" sx-overflow-x="wrap" sx-overflow-y="show" sx-justify-content="space-evenly" sx-gap=(15) {
            div
                hx-get=(pre_escaped!("/albums-list-start?limit=100&size={size}"))
                hx-trigger="load"
                sx-dir="row"
                sx-overflow-x="wrap"
                sx-overflow-y="show"
                sx-justify-content="space-evenly"
                sx-gap=(15)
            {
                @for _ in 0..100 {
                    div sx-width=(size) sx-height=(size + 30) {
                        img src=(public_img!("album.svg")) sx-width=(size) sx-height=(size);
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn albums(state: &State) -> Markup {
    page(state, &albums_page_content())
}
