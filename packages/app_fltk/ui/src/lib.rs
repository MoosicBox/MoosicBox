#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use maud::{html, Markup};

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
        div {
            ("player")
        }
    }
}

#[must_use]
pub fn footer() -> Markup {
    html! {
        footer {
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

#[must_use]
pub fn albums() -> Markup {
    let albums = vec![
        (
            "test1",
            "../../../.local/moosicbox/cache/tidal/Anberlin/Blueprints_For_The_Black_Market/album_320_1352282.jpg",
        ),
        (
            "test2",
            "../../../.local/moosicbox/cache/tidal/Anberlin/Cities/album_320_1345749.jpg",
        ),
    ];

    page(&html! {
        div sx-dir="row" {
            @for (title, img) in albums {
                div {
                    img src=(img) sx-width="200" sx-height="200";
                    (title)
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
