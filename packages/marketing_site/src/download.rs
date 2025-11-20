//! GitHub releases integration for the download page.
//!
//! This module handles fetching and parsing GitHub releases for the `MoosicBox` project,
//! organizing download assets by operating system, and rendering the download page with
//! appropriate assets based on the client's detected OS.

use std::{future::Future, sync::LazyLock};

use chrono::NaiveDateTime;
use hyperchad::{renderer::View, router::RouteRequest};
use moosicbox_marketing_site_ui::download::{FileAsset, Os, OsAsset, OsRelease};
use regex::Regex;
use serde::Deserialize;

static CLIENT: LazyLock<switchy_http::Client> =
    LazyLock::new(|| switchy_http::Client::builder().build().unwrap());

/// Handles the `/releases` route to display GitHub releases for download.
///
/// Fetches release information from the GitHub API, parses assets for different
/// operating systems, and renders the download page with appropriate assets for
/// the requesting client's OS.
///
/// # Errors
///
/// * If fetching GitHub releases fails after retries
/// * If parsing the GitHub API response fails
/// * If parsing release published dates fails
///
/// # Panics
///
/// * If the HTTP client fails to build (during static initialization)
/// * If any regex pattern compilation fails (during static initialization)
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
                switchy_http::Header::UserAgent.as_ref(),
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

    Ok(moosicbox_marketing_site_ui::download::releases(&releases, &os).into())
}

/// Retries an async operation up to a maximum number of attempts.
///
/// Executes the provided async function repeatedly until it succeeds or the
/// maximum number of retries is reached. Returns the first successful result
/// or the last error encountered.
///
/// # Errors
///
/// * Returns the error from the last failed attempt if all retries are exhausted
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };

    #[switchy_async::test]
    async fn test_with_retry_succeeds_on_first_attempt() {
        let result = with_retry(3, || async { Ok::<i32, String>(42) }).await;
        assert_eq!(result, Ok(42));
    }

    #[switchy_async::test]
    async fn test_with_retry_succeeds_after_failures() {
        let attempt_count = Arc::new(AtomicU8::new(0));
        let attempt_count_clone = Arc::clone(&attempt_count);

        let result = with_retry(3, move || {
            let count = attempt_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 { Err("Not yet") } else { Ok(100) }
            }
        })
        .await;

        assert_eq!(result, Ok(100));
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
    }

    #[switchy_async::test]
    async fn test_with_retry_fails_after_max_retries() {
        let attempt_count = Arc::new(AtomicU8::new(0));
        let attempt_count_clone = Arc::clone(&attempt_count);

        let result = with_retry(3, move || {
            let count = attempt_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, &str>("Always fails")
            }
        })
        .await;

        assert_eq!(result, Err("Always fails"));
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
    }

    #[switchy_async::test]
    async fn test_with_retry_with_single_retry() {
        let attempt_count = Arc::new(AtomicU8::new(0));
        let attempt_count_clone = Arc::clone(&attempt_count);

        let result = with_retry(1, move || {
            let count = attempt_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, &str>("Fail")
            }
        })
        .await;

        assert_eq!(result, Err("Fail"));
        assert_eq!(attempt_count.load(Ordering::SeqCst), 1);
    }

    #[switchy_async::test]
    async fn test_with_retry_succeeds_on_last_attempt() {
        let attempt_count = Arc::new(AtomicU8::new(0));
        let attempt_count_clone = Arc::clone(&attempt_count);

        let result = with_retry(3, move || {
            let count = attempt_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 { Err("Not yet") } else { Ok(999) }
            }
        })
        .await;

        assert_eq!(result, Ok(999));
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3);
    }
}
