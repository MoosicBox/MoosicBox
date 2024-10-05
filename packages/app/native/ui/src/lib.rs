#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use maud::{html, Markup};
use moosicbox_library_models::{ApiAlbum, ApiLibraryAlbum};

#[must_use]
pub fn sidebar_navigation() -> Markup {
    html! {
        aside sx-width="20%" {
            div class="navigation-bar" {
                div class="navigation-bar-header" {
                    a href="/" sx-dir="row" {
                        img
                            sx-width="36"
                            sx-height="36"
                            class="navigation-bar-header-home-link-logo-icon"
                            src="/img/icon128.png";

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
        div sx-height="100" {
            ("player")
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
        main class="main-content" sx-background-color {
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

#[must_use]
pub fn albums(albums: Vec<ApiAlbum>) -> Markup {
    let albums = albums
        .into_iter()
        .map(|x| {
            let ApiAlbum::Library(x) = x;
            x
        })
        .collect::<Vec<_>>();

    page(&html! {
        div sx-dir="row" sx-overflow="wrap" {
            @for album in &albums {
                div {
                    img src=(album_cover_url(album, 100, 100)) sx-width="100" sx-height="100";
                    (album.title)
                }
            }
        }
    })
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
