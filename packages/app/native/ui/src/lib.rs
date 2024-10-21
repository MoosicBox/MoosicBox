#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::branches_sharing_code)]

use maud::{html, Markup, PreEscaped};
use moosicbox_app_native_image::image;
use moosicbox_library_models::{ApiAlbum, ApiArtist, ApiLibraryAlbum, ApiLibraryArtist, ApiTrack};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_paging::Page;

macro_rules! public_img {
    ($path:expr $(,)?) => {
        image!(concat!("../../../../../app-website/public/img/", $path))
    };
}

macro_rules! pre_escaped {
    ($($message:tt)+) => {
        PreEscaped(format!($($message)*))
    };
}

#[must_use]
pub fn sidebar_navigation() -> Markup {
    html! {
        aside sx-width="20%" {
            div class="navigation-bar" {
                div class="navigation-bar-header" {
                    a href="/" sx-dir="row" {
                        @let size = 36;
                        img
                            sx-width=(size)
                            sx-height=(size)
                            class="navigation-bar-header-home-link-logo-icon"
                            src=(public_img!("icon128.png"));

                        h1 class="navigation-bar-header-home-link-text" {
                            ("MoosicBox")
                        }
                    }
                }
                ul {
                    li {
                        a href="/" {
                            ("Home")
                        }
                    }
                    li {
                        a href="/downloads" {
                            ("Downloads")
                        }
                    }
                }
                h1 class="my-collection-header" {
                    ("My Collection")
                }
                ul {
                    li {
                        a href="/albums" {
                            ("Albums")
                        }
                    }
                    li {
                        a href="/artists" {
                            ("Artists")
                        }
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn player() -> Markup {
    html! {
        div sx-height=(100) sx-dir="row" {
            div sx-dir="row" {
                @let size = 70;
                div sx-width=(size) sx-height=(size) {
                    (album_cover_img(&ApiLibraryAlbum { album_id: 1, contains_cover: true, ..Default::default() }, size))
                }
            }
            div sx-dir="row" {
                @let size = 36;
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("chevron-left-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("pause-button-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("chevron-right-white.svg"));
                }
            }
            div sx-dir="row" {
                @let size = 25;
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("audio-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("speaker-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("sessions-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("playlist-white.svg"));
                }
            }
        }
    }
}

#[must_use]
pub fn footer() -> Markup {
    html! {
        footer sx-height=(100) {
            (player())
        }
    }
}

#[must_use]
pub fn main(slot: &Markup) -> Markup {
    html! {
        main class="main-content" sx-overflow-y="auto" {
            (slot)
        }
    }
}

#[must_use]
pub fn home() -> Markup {
    page(&html! {
        ("home")
    })
}

#[must_use]
pub fn downloads() -> Markup {
    page(&html! {
        ("downloads")
    })
}

fn artist_cover_url(artist: &ApiLibraryArtist, width: u16, height: u16) -> String {
    if artist.contains_cover {
        format!(
            "{}/files/artists/{}/{width}x{height}?moosicboxProfile=master",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8500"),
            artist.artist_id
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

fn artist_cover_img(artist: &ApiLibraryArtist, size: u16) -> Markup {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        img src=(artist_cover_url(&artist, request_size, request_size)) sx-width=(size) sx-height=(size);
    }
}

fn album_cover_url(album: &ApiLibraryAlbum, width: u16, height: u16) -> String {
    if album.contains_cover {
        format!(
            "{}/files/albums/{}/{width}x{height}?moosicboxProfile=master",
            std::env::var("MOOSICBOX_HOST")
                .as_deref()
                .unwrap_or("http://localhost:8500"),
            album.album_id
        )
    } else {
        public_img!("album.svg").to_string()
    }
}

fn album_cover_img(album: &ApiLibraryAlbum, size: u16) -> Markup {
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        img src=(album_cover_url(&album, request_size, request_size)) sx-width=(size) sx-height=(size);
    }
}

trait TimeFormat {
    fn into_formatted(self) -> String;
}

impl TimeFormat for f32 {
    fn into_formatted(self) -> String {
        f64::from(self).into_formatted()
    }
}

impl TimeFormat for f64 {
    fn into_formatted(self) -> String {
        #[allow(clippy::cast_sign_loss)]
        #[allow(clippy::cast_possible_truncation)]
        (self.round() as u64).into_formatted()
    }
}

impl TimeFormat for u64 {
    fn into_formatted(self) -> String {
        let hours = self / 60 / 60;
        let minutes = self / 60;
        let seconds = self % 60;

        if hours > 0 {
            format!("{hours}:{minutes}:{seconds:0>2}")
        } else {
            format!("{minutes}:{seconds:0>2}")
        }
    }
}

#[must_use]
pub fn album_page_immediate(album_id: u64) -> Markup {
    html! {
        div hx-get=(pre_escaped!("/albums?full=true&albumId={album_id}")) hx-trigger="load" {
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
pub fn album_page_content(album: ApiAlbum, versions: &[ApiAlbumVersion]) -> Markup {
    let ApiAlbum::Library(album) = album;

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
                        @for track in version.tracks.iter().filter_map(|x| match x {
                            ApiTrack::Library { data, .. } => Some(data),
                            ApiTrack::Tidal { .. } |
                            ApiTrack::Qobuz { .. } |
                            ApiTrack::Yt { .. } => None,
                        }) {
                            tr {
                                td { (track.number) }
                                td { (track.title) }
                                td { a href={"/artists?artistId="(track.artist_id)} { (track.artist) } }
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
pub fn album(album_id: u64) -> Markup {
    page(&album_page_immediate(album_id))
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
                    div hx-get=(pre_escaped!("/albums-list-start?offset={offset}&limit={limit}&size={size}")) hx-trigger="load" {}
                }
            },
            |remaining| {
                let limit = remaining / MAX_PARALLEL_REQUESTS;
                let last = limit + (remaining % MAX_PARALLEL_REQUESTS);

                html! {
                    @if limit < MIN_PAGE_THRESHOLD {
                        div hx-get=(pre_escaped!("/albums-list?offset={offset}&limit={remaining}&size={size}")) hx-trigger="load" {}
                    } @else {
                        @for i in 0..MAX_PARALLEL_REQUESTS {
                            @if i == MAX_PARALLEL_REQUESTS - 1 {
                                div hx-get=(pre_escaped!("/albums-list?offset={}&limit={last}&size={size}", offset + i * limit)) hx-trigger="load" {}
                            } @else {
                                div hx-get=(pre_escaped!("/albums-list?offset={}&limit={limit}&size={size}", offset + i * limit)) hx-trigger="load" {}
                            }
                        }
                    }
                }
            },
        )
    } else {
        html! {}
    };
    let albums = albums.iter().map(|x| {
        let ApiAlbum::Library(album) = x;
        album
    });

    html! {
        @for album in albums {
            a href={"/albums?albumId="(album.album_id)} sx-width=(size) sx-height=(size + 30) {
                div sx-width=(size) sx-height=(size + 30) {
                    (album_cover_img(album, size))
                    (album.title)
                }
            }
        }
        (remaining)
    }
}

#[must_use]
pub fn albums_list(albums: &Page<ApiAlbum>, size: u16) -> Markup {
    let albums = albums.iter().map(|x| {
        let ApiAlbum::Library(album) = x;
        album
    });

    html! {
        @for album in albums {
            a href={"/albums?albumId="(album.album_id)} sx-width=(size) sx-height=(size + 30) {
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
        div sx-dir="row" sx-overflow-x="wrap" sx-overflow-y="show" {
            div
                hx-get=(pre_escaped!("/albums-list-start?limit=100&size={size}"))
                hx-trigger="load"
                sx-dir="row"
                sx-overflow-x="wrap"
                sx-overflow-y="show"
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
pub fn albums() -> Markup {
    page(&albums_page_content())
}

#[must_use]
pub fn artist_page_content(artist: ApiArtist) -> Markup {
    let ApiArtist::Library(artist) = artist;

    html! {
        div sx-dir="row" {
            @let size = 200;
            div sx-width=(size) sx-height=(size + 30) {
                (artist_cover_img(&artist, size))
            }
            div {
                h1 { (artist.title) }
            }
        }
    }
}

#[must_use]
pub fn artist(artist: ApiArtist) -> Markup {
    page(&artist_page_content(artist))
}

#[must_use]
pub fn artists_page_content(artists: Vec<ApiArtist>) -> Markup {
    let artists = artists
        .into_iter()
        .map(|x| {
            let ApiArtist::Library(x) = x;
            x
        })
        .collect::<Vec<_>>();

    let size: u16 = 200;
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        div sx-dir="row" sx-overflow-x="wrap" sx-overflow-y="show" {
            @for artist in &artists {
                a href={"/artists?artistId="(artist.artist_id)} sx-width=(size) sx-height=(size + 30) {
                    div sx-width=(size) sx-height=(size + 30) {
                        img src=(artist_cover_url(artist, request_size, request_size)) sx-width=(size) sx-height=(size);
                        (artist.title)
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn artists(artists: Vec<ApiArtist>) -> Markup {
    page(&artists_page_content(artists))
}

#[must_use]
pub fn page(slot: &Markup) -> Markup {
    html! {
        div id="root" class="dark" {
            section class="navigation-bar-and-main-content" sx-dir="row" {
                (sidebar_navigation())
                (main(&slot))
            }
            (footer())
        }
    }
}
