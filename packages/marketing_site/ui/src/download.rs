use std::sync::LazyLock;

use chrono::NaiveDateTime;
use hyperchad::{
    actions::logic::if_responsive,
    template::{Containers, container},
    transformer::models::{AlignItems, LayoutDirection},
};
use regex::Regex;

use crate::page;

pub use hyperchad::template as hyperchad_template;

#[must_use]
pub fn download() -> Containers {
    page(&container! {
        div align-items=center padding-x=20 {
            div width=100% max-width=1000 padding-y=20 {
                h1 border-bottom="2, #ccc" padding-bottom=20 margin-bottom=10 { "Downloads" }
                div #releases hidden=(true) hx-get="/releases" hx-trigger=load {}
            }
        }
    })
}

#[derive(Default, Clone, Debug)]
pub struct Os<'a> {
    pub lower_name: &'a str,
    pub name: &'a str,
    pub header: &'a str,
}

#[derive(Default, Clone, Debug)]
pub struct OsRelease<'a> {
    pub version: &'a str,
    pub published_at: NaiveDateTime,
    pub url: &'a str,
    pub assets: Vec<OsAsset<'a>>,
}

#[derive(Default, Clone, Debug)]
pub struct OsAsset<'a> {
    pub name: &'a str,
    pub asset: Option<FileAsset<'a>>,
    pub other_formats: Vec<FileAsset<'a>>,
}

#[derive(Default, Clone, Debug)]
pub struct FileAsset<'a> {
    pub browser_download_url: &'a str,
    pub name: &'a str,
    pub size: u64,
}

fn format_class_name(value: &str) -> String {
    static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\w-]").unwrap());
    REGEX
        .replace_all(&value.split_whitespace().collect::<Vec<_>>().join("-"), "_")
        .to_lowercase()
}

fn get_os_header(asset: &str) -> &str {
    match asset {
        "windows" => "Windows",
        "mac_intel" => "macOS",
        "linux" => "Linux",
        "android" => "Android",
        _ => asset,
    }
}

fn format_size(size: u64) -> String {
    bytesize::ByteSize::b(size).to_string()
}

fn format_date(date: &NaiveDateTime) -> String {
    // January 08, 2025 03:09:08
    date.format("%B %d, %Y %H:%M:%S").to_string()
}

#[must_use]
pub fn releases(releases: &[OsRelease], os: &Os) -> Containers {
    container! {
        div #releases {
            @for release in releases {
                div id=(format_class_name(release.version)) padding-y=20 {
                    h2
                        id={(format_class_name(release.version))"-header"}
                        direction=(
                            if_responsive("mobile")
                                .then::<LayoutDirection>(LayoutDirection::Column)
                                .or_else(LayoutDirection::Row)
                        )
                        align-items=(
                            if_responsive("mobile")
                                .then::<AlignItems>(AlignItems::Start)
                                .or_else(AlignItems::End)
                        )
                        col-gap=10
                    {
                        div { "Release " (release.version) }
                        div font-size=16 margin-bottom=2 color=#ccc {
                            (format_date(&release.published_at))
                        }
                        div font-size=16 margin-bottom=2 {
                            "[" anchor color=#fff target="_blank" href=(release.url) { "GitHub" } "]"
                        }
                    }
                    @for release_asset in &release.assets {
                        @if let Some(asset) = &release_asset.asset {
                            div {
                                @if os.lower_name == release_asset.name {
                                    div color=#888 {
                                        "// We think you are running " (os.header)
                                    }
                                }
                                h3 { (get_os_header(release_asset.name)) }
                                "Download "
                                anchor color=#fff href=(asset.browser_download_url) { (asset.name) }
                                span color=#ccc font-size=12 { " (" (format_size(asset.size)) ")" }
                                ul margin=0 {
                                    @for other_asset in &release_asset.other_formats {
                                        li {
                                            anchor color=#fff href=(other_asset.browser_download_url) { (other_asset.name) }
                                            span color=#ccc font-size=12 { " (" (format_size(other_asset.size)) ")" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
