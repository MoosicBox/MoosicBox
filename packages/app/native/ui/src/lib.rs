#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use maud::{html, Markup};
use moosicbox_app_native_image::image;
use moosicbox_library_models::{ApiAlbum, ApiLibraryAlbum, ApiTrack};
use moosicbox_menu_models::api::ApiAlbumVersion;

#[must_use]
pub fn sidebar_navigation() -> Markup {
    html! {
        aside sx-width="20%" {
            div class="navigation-bar" {
                div class="navigation-bar-header" {
                    a href="/" sx-dir="row" {
                        img
                            sx-width=(36)
                            sx-height=(36)
                            class="navigation-bar-header-home-link-logo-icon"
                            src=(image!("../../../../../app-website/public/img/icon128.png"));

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
        div sx-height="100" sx-dir="row" {
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
                        src=(image!("../../../../../app-website/public/img/chevron-left-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(image!("../../../../../app-website/public/img/pause-button-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(image!("../../../../../app-website/public/img/chevron-right-white.svg"));
                }
            }
            div sx-dir="row" {
                @let size = 25;
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(image!("../../../../../app-website/public/img/audio-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(image!("../../../../../app-website/public/img/speaker-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(image!("../../../../../app-website/public/img/sessions-white.svg"));
                }
                div sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(image!("../../../../../app-website/public/img/playlist-white.svg"));
                }
            }
        }
    }
}

#[must_use]
pub fn footer() -> Markup {
    html! {
        footer sx-height="100" {
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
        "/img/album.svg".to_string()
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
                                td { (track.artist) }
                                td { (track.duration) }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn album(album: ApiAlbum, versions: &[ApiAlbumVersion]) -> Markup {
    page(&album_page_content(album, versions))
}

#[must_use]
pub fn albums_page_content(albums: Vec<ApiAlbum>) -> Markup {
    let albums = albums
        .into_iter()
        .map(|x| {
            let ApiAlbum::Library(x) = x;
            x
        })
        .collect::<Vec<_>>();

    let size: u16 = 200;
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let request_size = (f64::from(size) * 1.33).round() as u16;

    html! {
        div sx-dir="row" sx-overflow-x="wrap" sx-overflow-y="show" {
            @for album in &albums {
                a href={"/albums?albumId="(album.album_id)} sx-width=(size) sx-height=(size + 30) {
                    div sx-width=(size) sx-height=(size + 30) {
                        img src=(album_cover_url(album, request_size, request_size)) sx-width=(size) sx-height=(size);
                        (album.title)
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn albums(albums: Vec<ApiAlbum>) -> Markup {
    page(&albums_page_content(albums))
}

#[must_use]
pub fn artists() -> Markup {
    page(&html! {
        ("artists")
    })
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
