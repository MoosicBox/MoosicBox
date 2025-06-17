#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::branches_sharing_code)]

pub mod download;
pub use hyperchad::template as hyperchad_template;

use hyperchad::{
    actions::logic::if_responsive,
    template::{Containers, container},
    transformer::models::{LayoutDirection, TextAlign},
};

#[macro_export]
macro_rules! public_img {
    ($path:expr $(,)?) => {
        concat!("/public/img/", $path)
    };
}

#[must_use]
pub fn header() -> Containers {
    container! {
        header
            direction=row
            align-items=center
            background=#080a0b
        {
            div #header-logo padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20)) {
                anchor
                    color=#fff
                    direction=row
                    align-items=center
                    text-decoration="none"
                    href="/"
                {
                    @let icon_size = 40;
                    image
                        alt="MoosicBox logo"
                        width=(icon_size)
                        height=(icon_size)
                        margin-right=5
                        src=(public_img!("icon128.png"));

                    h1 font-size=20 { "MoosicBox" }
                }
            }
            div
                #header-menu-items
                direction=row
                align-items=center
                justify-content=end
                flex=1
                padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20))
                col-gap=(if_responsive("mobile").then::<i32>(10).or_else(20))
            {
                anchor color=#fff href="/download" {
                    "Download"
                }
                anchor color=#fff href="https://app.moosicbox.com/login" {
                    "Log in"
                }
                anchor
                    color=#fff
                    background=#282a2b
                    border-radius=5
                    padding=8
                    href="/try-now"
                {
                    span #try-desktop hidden=(if_responsive("mobile").then::<bool>(true).or_else(false)) {
                        "Start Free Trial"
                    }
                    span #try-mobile hidden=(if_responsive("mobile").then::<bool>(false).or_else(true)) {
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
        main flex-grow=1 min-height=0 {
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
        div
            min-height=100%
            justify-content=center
        {
            div
                #pics
                direction=(
                    if_responsive("mobile-large")
                        .then::<LayoutDirection>(LayoutDirection::Column)
                        .or_else(LayoutDirection::Row)
                )
                align-items=center
                max-height=1000
                padding-x=50
                gap=calc(min(100, 5%))
            {
                div flex-grow=2 {
                    h1
                        #splashscreen-motto
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
                div
                    direction=row
                    position=relative
                    height=100%
                    flex-grow=3
                {
                    div
                        margin-left=calc(10% - (100% / 30))
                        height=100%
                        max-height=100%
                        max-width=calc(100% - (10% - (100% / 30)))
                    {
                        image
                            src=(public_img!("showcase-1.webp"))
                            srcset={
                                (public_img!("showcase-1x240.webp"))" 240w, "
                                (public_img!("showcase-1x540.webp"))" 540w, "
                                (public_img!("showcase-1.webp"))" 1080w"
                            }
                            sizes=vw70
                            width=100%
                            height=100%
                            alt="MoosicBox showcase desktop"
                            fit="contain";
                    }
                    div
                        position=absolute
                        bottom=50%
                        translate-y=50%
                        height=calc(min(65%, dvw(50)))
                        max-height=80%
                    {
                        image
                            src=(public_img!("showcase-2.webp"))
                            srcset={
                                (public_img!("showcase-2x240.webp"))" 240w, "
                                (public_img!("showcase-2x540.webp"))" 540w, "
                                (public_img!("showcase-2.webp"))" 1080w"
                            }
                            sizes=vw30
                            width=100%
                            height=100%
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
        div
            width=100%
            height=100%
            position=relative
            color=#fff
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
