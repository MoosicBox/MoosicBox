#![allow(clippy::module_name_repetitions)]

use maud::{html, Markup, PreEscaped};
use moosicbox_core::sqlite::models::{ApiAlbum, ApiSource};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_paging::Page;

use crate::{formatting::TimeFormat as _, page, pre_escaped, public_img, state::State};

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
pub fn album_page_immediate(album_id: &str, source: Option<ApiSource>) -> Markup {
    let path = pre_escaped!(
        "/albums?full=true&albumId={album_id}{}",
        source.map_or_else(String::new, |x| format!("&source={x}"))
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

#[must_use]
pub fn album_page_content(album: &ApiAlbum, versions: &[ApiAlbumVersion]) -> Markup {
    html! {
        div sx-dir="row" {
            @let size = 200;
            div sx-width=(size) sx-height=(size + 30) {
                (album_cover_img(&album, size))
            }
            div {
                h1 { (album.title) }
                h2 { (album.artist) }
                @if let Some(version) = album.versions.first() {
                    h3 { (version.source) }
                }
            }
        }
        div {
            @if let Some(version) = versions.first() {
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
pub fn album(state: &State, album_id: &str, source: Option<ApiSource>) -> Markup {
    page(state, &album_page_immediate(album_id, source))
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

pub fn show_albums<'a>(albums: impl Iterator<Item = &'a ApiAlbum>, size: u16) -> Markup {
    html! {
        @for album in albums {
            a href=(pre_escaped!("/albums?albumId={}&source={}", album.album_id, album.api_source)) sx-width=(size) sx-height=(size + 30) {
                div sx-width=(size) sx-height=(size + 30) {
                    (album_cover_img(album, size))
                    (album.title)
                }
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
