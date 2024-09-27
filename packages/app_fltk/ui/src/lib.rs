#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use maud::{html, Markup};

#[must_use]
pub fn sidebar_navigation() -> Markup {
    html! {
        aside {
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
pub fn main() -> Markup {
    html! {
        main class="main-content" {
            ("main")
        }
    }
}

#[must_use]
pub fn home() -> Markup {
    html! {
        div id="root" class="dark" {
            section class="navigation-bar-and-main-content" sx-dir="row" {
                (sidebar_navigation())
                (main())
            }
            (footer())
        }
    }
}
