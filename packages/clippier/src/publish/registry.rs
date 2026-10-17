//! Run-scoped crates.io transport: pace reads, honor 429 cooldowns and reuse
//! immutable positive existence results. Missing versions are never cached because
//! publication polls must observe newly uploaded versions.

use super::{
    BTreeMap, BoxError, Duration, Instant, RateLimit, Utc, parse_retry_after, retry_timing,
};

// Exhaustion is fatal for the entire run, not a separate budget per package.
#[derive(Debug)]
pub(super) struct RateLimitExhausted(String);
impl std::fmt::Display for RateLimitExhausted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for RateLimitExhausted {}

pub(super) struct RegistryClient {
    http: reqwest::Client,
    state: tokio::sync::Mutex<State>,
    retries: u16,
    interval: Duration,
}

#[derive(Default)]
struct State {
    last_request: Option<Instant>,
    cooldown: Duration,
    exhausted: Option<String>,
    exists: BTreeMap<String, bool>,
}

impl RegistryClient {
    #[cfg(test)]
    pub(super) fn for_test(retries: u16) -> Self {
        Self {
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            state: tokio::sync::Mutex::new(State::default()),
            retries,
            interval: Duration::ZERO,
        }
    }

    #[cfg(test)]
    pub(super) async fn cache_test_version(&self, name: &str, version: &str, exists: bool) {
        let url = format!("https://index.crates.io/{}", index_path(name).unwrap());
        self.state
            .lock()
            .await
            .exists
            .insert(format!("{url}@{version}"), exists);
    }

    pub(super) fn new(retries: u16) -> Result<Self, BoxError> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent(concat!(
                    "clippier/",
                    env!("CARGO_PKG_VERSION"),
                    " (https://github.com/MoosicBox/MoosicBox)"
                ))
                .timeout(Duration::from_secs(60))
                .build()?,
            state: tokio::sync::Mutex::new(State::default()),
            retries,
            interval: Duration::from_secs(1),
        })
    }

    pub(super) async fn version_exists(&self, name: &str, version: &str) -> Result<bool, BoxError> {
        let url = format!("https://index.crates.io/{}", index_path(name)?);
        self.version_at(&url, version).await
    }

    async fn version_at(&self, url: &str, version: &str) -> Result<bool, BoxError> {
        #[derive(serde::Deserialize)]
        struct Version {
            vers: String,
        }
        let key = format!("{url}@{version}");
        let cached = self.state.lock().await.exists.get(&key).copied();
        if let Some(exists) = cached {
            return Ok(exists);
        }
        let response = self.get(url).await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        let text = response.error_for_status()?.text().await?;
        // Parse every line: a malformed response must never be treated as proof
        // that a version is unpublished. Yanked versions also cannot be reuploaded.
        let mut exists = false;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let entry: Version = serde_json::from_str(line)?;
            exists |= entry.vers == version;
        }
        if exists {
            self.state.lock().await.exists.insert(key, true);
        }
        Ok(exists)
    }

    pub(super) async fn exists(&self, url: &str, cache_missing: bool) -> Result<bool, BoxError> {
        let cached = self.state.lock().await.exists.get(url).copied();
        if let Some(value) = cached {
            return Ok(value);
        }
        let response = self.get(url).await?;
        let status = response.status();
        let exists = if status.is_success() {
            true
        } else if status == reqwest::StatusCode::NOT_FOUND {
            false
        } else {
            return Err(format!("Failed to query {url}: HTTP {status}").into());
        };
        if exists || cache_missing {
            self.state
                .lock()
                .await
                .exists
                .insert(url.to_owned(), exists);
        }
        Ok(exists)
    }

    #[allow(clippy::significant_drop_tightening)] // Hold the gate through cooldowns and all retry attempts.
    pub(super) async fn get(&self, url: &str) -> Result<reqwest::Response, BoxError> {
        // Serialize API reads and share cooldowns across all lookup types, rather
        // than allowing another caller to bypass a Retry-After deadline.
        let mut state = self.state.lock().await;
        if let Some(error) = &state.exhausted {
            return Err(RateLimitExhausted(error.clone()).into());
        }
        for attempt in 0..=self.retries {
            if let Some(last) = state.last_request {
                tokio::time::sleep(
                    state
                        .cooldown
                        .max(self.interval)
                        .saturating_sub(last.elapsed()),
                )
                .await;
            }
            let response = self.http.get(url).send().await?;
            state.last_request = Some(Instant::now());
            state.cooldown = Duration::ZERO;
            if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Ok(response);
            }
            let header = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = response.text().await?;
            let RateLimit { wait, .. } =
                retry_timing(parse_retry_after(&body), header.as_deref(), Utc::now());
            state.cooldown = wait;
            if attempt == self.retries {
                let error = format!(
                    "Failed to query {url}: HTTP 429 Too Many Requests; exhausted {} registry retries (cooldown {}s)",
                    self.retries,
                    wait.as_secs()
                );
                state.exhausted = Some(error.clone());
                return Err(RateLimitExhausted(error).into());
            }
            eprintln!(
                "Registry request rate limited: {url}; waiting {}s before retry ({}/{})",
                wait.as_secs(),
                attempt + 1,
                self.retries
            );
        }
        unreachable!("retry loop always returns")
    }
}

#[cfg(test)]
mod tests;

pub(super) fn index_path(name: &str) -> Result<String, BoxError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("Invalid registry crate name".into());
    }
    let name = name.to_ascii_lowercase();
    Ok(match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    })
}
