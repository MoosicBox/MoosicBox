use std::{future::Future, sync::LazyLock};

use chrono::NaiveDateTime;
use moosicbox_app_native_lib::{renderer::View, router::RouteRequest};
use moosicbox_marketing_site_ui::download::{FileAsset, Os, OsAsset, OsRelease};
use regex::Regex;
use serde::Deserialize;

static CLIENT: LazyLock<gimbal_http::Client> =
    LazyLock::new(|| gimbal_http::Client::builder().build().unwrap());

#[allow(clippy::too_many_lines)]
pub async fn releases_route(req: RouteRequest) -> Result<View, Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    struct GitHubRelease {
        name: String,
        html_url: String,
        published_at: String,
        assets: Vec<GitHubAsset>,
    }

    #[derive(Deserialize)]
    struct GitHubAsset {
        browser_download_url: String,
        name: String,
        size: u64,
    }

    fn github_release_into_os_release<'a>(
        value: &'a GitHubRelease,
        os: &Os<'a>,
    ) -> Result<OsRelease<'a>, Box<dyn std::error::Error>> {
        static WINDOWS_ASSET_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(.+?\.msi|.+?\.exe)").unwrap());
        static MAC_APPLE_SILICON_ASSET_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(.+?\.dmg|.+?_macos.*)").unwrap());
        static MAC_INTEL_ASSET_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(.+?(x64|aarch64).*?\.dmg|.+?_macos_x64.*|.+?_x64_macos.*)").unwrap()
        });
        static LINUX_ASSET_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(.+?\.AppImage|.+?\.deb|.+?_linux.*)").unwrap());
        static ANDROID_ASSET_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(.+?\.aab|.+?\.apk)").unwrap());

        let create_asset = |name: &'a str,
                            asset_name: &str,
                            asset_matcher: &Regex,
                            asset_not_matcher: Option<&Regex>| {
            let mut other_formats: Vec<FileAsset> = value
                .assets
                .iter()
                .filter(|a| {
                    a.name != asset_name
                        && asset_matcher.is_match(&a.name)
                        && asset_not_matcher.is_none_or(|x| !x.is_match(&a.name))
                })
                .map(Into::into)
                .collect();

            let asset = value
                .assets
                .iter()
                .find(|a| a.name == asset_name)
                .map(Into::into)
                .or_else(|| {
                    if other_formats.is_empty() {
                        None
                    } else {
                        Some(other_formats.remove(0))
                    }
                });

            OsAsset {
                name,
                asset,
                other_formats,
            }
        };

        let windows = create_asset("windows", "MoosicBox_x64.msi", &WINDOWS_ASSET_PATTERN, None);
        let mac_apple_silicon = create_asset(
            "mac_apple_silicon",
            "MoosicBox.dmg",
            &MAC_APPLE_SILICON_ASSET_PATTERN,
            Some(&MAC_INTEL_ASSET_PATTERN),
        );
        let mac_intel = create_asset(
            "mac_intel",
            "MoosicBox_x64.dmg",
            &MAC_INTEL_ASSET_PATTERN,
            None,
        );
        let linux = create_asset("linux", "moosicbox_amd64.deb", &LINUX_ASSET_PATTERN, None);
        let android = create_asset("android", "MoosicBox.apk", &ANDROID_ASSET_PATTERN, None);

        let mut assets = vec![windows, mac_apple_silicon, mac_intel, linux, android];

        assets.sort_by(|a, b| {
            if os.lower_name == a.name {
                std::cmp::Ordering::Less
            } else if os.lower_name == b.name {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });

        assets.sort_by(|a, b| {
            if a.name == "mac_intel" && b.name != "mac_apple_silicon" {
                std::cmp::Ordering::Less
            } else if b.name == "mac_intel" && a.name != "mac_apple_silicon" {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });

        let published_at = NaiveDateTime::parse_from_str(&value.published_at, "%Y-%m-%dT%H:%M:%SZ")
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        Ok(OsRelease {
            version: &value.name,
            published_at,
            url: &value.html_url,
            assets,
        })
    }

    impl<'a> From<&'a GitHubAsset> for FileAsset<'a> {
        fn from(value: &'a GitHubAsset) -> Self {
            Self {
                browser_download_url: &value.browser_download_url,
                name: &value.name,
                size: value.size,
            }
        }
    }

    let os = Os {
        lower_name: &req.info.client.os.name.to_lowercase(),
        name: &req.info.client.os.name,
        header: &req.info.client.os.name,
    };

    log::debug!("releases_route: os={os:?}");
    log::debug!("releases_route: requesting GitHub releases");

    let mut releases: Vec<GitHubRelease> = with_retry(3, || async {
        let response = CLIENT
            .get("https://api.github.com/repos/MoosicBox/MoosicBox/releases")
            .header(
                gimbal_http::Header::UserAgent.as_ref(),
                "moosicbox-marketing-site",
            )
            .send()
            .await?
            .text()
            .await?;

        log::debug!("releases_route: received GitHub releases response");
        log::trace!("releases_route: GitHub releases response: '{response}'");

        Ok::<_, Box<dyn std::error::Error>>(serde_json::from_str(&response).map_err(|e| {
            log::warn!("Failed to parse response: {e:?}");
            e
        })?)
    })
    .await?;

    releases.sort_by(|a, b| b.published_at.cmp(&a.published_at));

    let releases: Vec<OsRelease<'_>> = releases
        .iter()
        .map(|x| github_release_into_os_release(x, &os))
        .collect::<Result<Vec<_>, _>>()?;

    moosicbox_marketing_site_ui::download::releases(&releases, &os)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            Box::new(e) as Box<dyn std::error::Error>
        })
}

async fn with_retry<T: Sized, E, F: Future<Output = Result<T, E>> + Send, U: (Fn() -> F) + Send>(
    max_retries: u8,
    func: U,
) -> Result<T, E> {
    let mut attempt = 1;
    loop {
        match func().await {
            Ok(x) => return Ok(x),
            Err(e) => {
                if attempt >= max_retries {
                    break Err(e);
                }
                attempt += 1;
            }
        }
    }
}
