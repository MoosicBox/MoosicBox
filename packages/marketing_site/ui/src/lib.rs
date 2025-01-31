#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::branches_sharing_code)]

pub mod download;

use gigachad_transformer_models::{
    AlignItems, JustifyContent, LayoutDirection, Position, TextAlign,
};
use maud::{html, Markup};

#[macro_export]
macro_rules! public_img {
    ($path:expr $(,)?) => {{
        #[cfg(feature = "bundled_images")]
        {
            moosicbox_app_native_image::image!(concat!("../../public/img/", $path))
        }
        #[cfg(not(feature = "bundled_images"))]
        concat!("/public/img/", $path)
    }};
}

#[macro_export]
macro_rules! pre_escaped {
    ($($message:tt)+) => {
        maud::PreEscaped(format!($($message)*))
    };
}

#[must_use]
pub fn header() -> Markup {
    html! {
        header
            sx-dir=(LayoutDirection::Row)
            sx-align-items=(AlignItems::Center)
            sx-background="#080a0b"
        {
            div sx-padding-x=(20) {
                a
                    sx-color="#fff"
                    sx-dir=(LayoutDirection::Row)
                    sx-align-items=(AlignItems::Center)
                    sx-text-decoration="none"
                    href="/"
                {
                    @let icon_size = 40;
                    img
                        sx-width=(icon_size)
                        sx-height=(icon_size)
                        sx-margin-right=(5)
                        src=(public_img!("icon128.png"));

                    h1 sx-font-size=(20) { "MoosicBox" }
                }
            }
            div
                sx-dir=(LayoutDirection::Row)
                sx-align-items=(AlignItems::Center)
                sx-justify-content=(JustifyContent::End)
                sx-flex=(1)
                sx-padding-x=(20)
            {
                a sx-color="#fff" href="/download" {
                    "Download"
                }
                a sx-color="#fff" sx-margin-left=(20) href="https://app.moosicbox.com/login" {
                    "Log in"
                }
                a
                    sx-color="#fff"
                    sx-background="#282a2b"
                    sx-margin-left=(20)
                    sx-border-radius=(5)
                    sx-padding=(8)
                    href="/try-now"
                {
                    "Start Free Trial"
                }
            }
        }
    }
}

#[must_use]
pub fn main(slot: &Markup) -> Markup {
    html! {
        main sx-flex-grow=(1) {
            (slot)
        }
    }
}

#[must_use]
pub fn not_found() -> Markup {
    page(&html! {
        "Page not found"
    })
}

#[must_use]
pub fn home() -> Markup {
    page(&html! {
        div
            sx-height="100%"
            sx-justify-content=(JustifyContent::Center)
        {
            div
                sx-dir=(LayoutDirection::Row)
                sx-align-items=(AlignItems::Center)
                sx-max-height="1000px"
                sx-padding-x=(50)
                sx-gap="calc(min(100, 5%))"
            {
                div sx-flex-grow=(2) {
                    h1 sx-font-size=(50) sx-text-align=(TextAlign::End) {
                        "Listen to your HiFi music anywhere"
                    }
                }
                div
                    sx-dir=(LayoutDirection::Row)
                    sx-position=(Position::Relative)
                    sx-height="100%"
                    sx-flex-grow=(3)
                {
                    div
                        sx-margin-left="calc(10% - (100% / 30))"
                        sx-height="100%"
                        sx-max-height="100%"
                        sx-max-width="calc(100% - calc(10% - (100% / 30)))"
                    {
                        img
                            src=(public_img!("showcase-1.webp"))
                            srcset={(public_img!("showcase-1x240.webp"))" 240w, "(public_img!("showcase-1x540.webp"))" 540w, "(public_img!("showcase-1.webp"))" 1080w"}
                            sizes="70vw"
                            sx-width="100%"
                            sx-height="100%"
                            alt="MoosicBox showcase desktop"
                            sx-fit="contain";
                    }
                    div
                        sx-position="absolute"
                        sx-bottom="50%"
                        sx-translate-y="50%"
                        sx-height="calc(min(65%, 25dvw))"
                        sx-max-height="80%"
                    {
                        img
                            src=(public_img!("showcase-2.webp"))
                            srcset={(public_img!("showcase-2x240.webp"))" 240w, "(public_img!("showcase-2x540.webp"))" 540w, "(public_img!("showcase-2.webp"))" 1080w"}
                            sizes="30vw"
                            sx-width="100%"
                            sx-height="100%"
                            alt="MoosicBox showcase android"
                            sx-fit="contain";
                    }
                }
            }
        }
    })
}

#[must_use]
pub fn page(slot: &Markup) -> Markup {
    html! {
        div
            sx-width="100%"
            sx-height="100%"
            sx-position="relative"
            sx-color="#fff"
            sx-font-family="Gordita, Roboto, Oxygen, Ubuntu, Cantarell, 'Open Sans', 'Helvetica Neue', sans-serif"
            sx-overflow-y="auto"
            sx-justify-content=(JustifyContent::Center)
        {
            (header())
            (main(&slot))
        }
    }
}
