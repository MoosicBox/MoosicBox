#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::branches_sharing_code)]

pub mod download;
pub use hyperchad::template2 as hyperchad_template2;

use hyperchad::{
    actions::logic::if_responsive,
    template2::{Containers, container},
    transformer::models::{LayoutDirection, TextAlign},
};

#[macro_export]
macro_rules! public_img {
    ($path:expr $(,)?) => {
        concat!("/public/img/", $path)
    };
}

#[macro_export]
macro_rules! pre_escaped {
    ($($message:tt)+) => {
        hyperchad_template::PreEscaped(format!($($message)*))
    };
}

#[must_use]
pub fn header() -> Containers {
    container! {
        Header
            direction=row
            align-items=center
            background="#080a0b"
        {
            Div id="header-logo" padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20)) {
                Anchor
                    color="#fff"
                    direction=row
                    align-items=center
                    text-decoration="none"
                    href="/"
                {
                    @let icon_size = 40;
                    Image
                        alt="MoosicBox logo"
                        width=(icon_size)
                        height=(icon_size)
                        margin-right=5
                        src=(public_img!("icon128.png"));

                    H1 font-size=20 { "MoosicBox" }
                }
            }
            Div
                id="header-menu-items"
                direction=row
                align-items=center
                justify-content=end
                flex=1
                padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20))
                col-gap=(if_responsive("mobile").then::<i32>(10).or_else(20))
            {
                Anchor color="#fff" href="/download" {
                    "Download"
                }
                Anchor color="#fff" href="https://app.moosicbox.com/login" {
                    "Log in"
                }
                Anchor
                    color="#fff"
                    background="#282a2b"
                    border-radius=5
                    padding=8
                    href="/try-now"
                {
                    Span id="try-desktop" hidden=(if_responsive("mobile").then::<bool>(true).or_else(false)) {
                        "Start Free Trial"
                    }
                    Span id="try-mobile" hidden=(if_responsive("mobile").then::<bool>(false).or_else(true)) {
                        "Try"
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn main(slot: &Containers) -> Containers {
    container! {
        Main flex-grow=1 min-height=0 {
            (slot)
        }
    }
}

#[must_use]
pub fn try_now() -> Containers {
    page(&container! {
        "Try now"
    })
}

#[must_use]
pub fn not_found() -> Containers {
    page(&container! {
        "Page not found"
    })
}

#[must_use]
pub fn home() -> Containers {
    page(&container! {
        Div
            min-height="100%"
            justify-content=center
        {
            Div
                id="pics"
                direction=(
                    if_responsive("mobile-large")
                        .then::<LayoutDirection>(LayoutDirection::Column)
                        .or_else(LayoutDirection::Row)
                )
                align-items=center
                max-height="1000px"
                padding-x=50
                gap="calc(min(100, 5%))"
            {
                Div flex-grow=2 {
                    H1
                        id="splashscreen-motto"
                        font-size=50
                        text-align=(
                            if_responsive("mobile-large")
                                .then::<TextAlign>(TextAlign::Center)
                                .or_else(TextAlign::End)
                        )
                    {
                        "Listen to your HiFi music anywhere"
                    }
                }
                Div
                    direction=row
                    position=relative
                    height="100%"
                    flex-grow=3
                {
                    Div
                        margin-left="calc(10% - (100% / 30))"
                        height="100%"
                        max-height="100%"
                        max-width="calc(100% - calc(10% - (100% / 30)))"
                    {
                        Image
                            src=(public_img!("showcase-1.webp"))
                            srcset={
                                (public_img!("showcase-1x240.webp"))" 240w, "
                                (public_img!("showcase-1x540.webp"))" 540w, "
                                (public_img!("showcase-1.webp"))" 1080w"
                            }
                            sizes="70vw"
                            width="100%"
                            height="100%"
                            alt="MoosicBox showcase desktop"
                            fit="contain";
                    }
                    Div
                        position=absolute
                        bottom="50%"
                        translate-y="50%"
                        height="calc(min(65%, 50dvw))"
                        max-height="80%"
                    {
                        Image
                            src=(public_img!("showcase-2.webp"))
                            srcset={
                                (public_img!("showcase-2x240.webp"))" 240w, "
                                (public_img!("showcase-2x540.webp"))" 540w, "
                                (public_img!("showcase-2.webp"))" 1080w"
                            }
                            sizes="30vw"
                            width="100%"
                            height="100%"
                            alt="MoosicBox showcase android"
                            fit="contain";
                    }
                }
            }
        }
    })
}

#[must_use]
pub fn page(slot: &Containers) -> Containers {
    container! {
        Div
            width="100%"
            height="100%"
            position=relative
            color="#fff"
            font-family="Gordita, Roboto, Oxygen, Ubuntu, Cantarell, 'Open Sans', 'Helvetica Neue', sans-serif"
            overflow-x=hidden
            overflow-y=auto
            justify-content=center
        {
            (header())
            (main(&slot))
        }
    }
}
