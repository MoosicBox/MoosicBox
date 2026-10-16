//! Native, source-aware Cargo sparse-index release discovery.
//!
//! No credentials are loaded or forwarded. Git indexes, local sources, and
//! authenticated indexes fail explicitly instead of falling back to crates.io.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use switchy_async::sync::{Mutex, Semaphore};

/// Error returned by registry discovery.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

const CRATES_IO: &str = "sparse+https://index.crates.io/";
const MAX_INDEX_BYTES: usize = 16 * 1024 * 1024;
type ReleaseCache = Mutex<BTreeMap<String, Arc<Mutex<Option<Vec<Release>>>>>>;

/// Network and cache policy for registry discovery.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct RegistryOptions {
    /// Never access the network; use memory or Cargo's supported local cache.
    pub offline: bool,
    /// Maximum duration of an individual HTTP request, including its body.
    pub timeout_seconds: u64,
    /// Number of retries after the initial attempt for transient failures.
    pub max_retries: u32,
    /// Maximum simultaneous HTTP requests across lookups on this client.
    pub max_concurrency: usize,
}

impl Default for RegistryOptions {
    fn default() -> Self {
        Self {
            offline: false,
            timeout_seconds: 30,
            max_retries: 2,
            max_concurrency: 8,
        }
    }
}

/// A release from the registry index, including releases unsuitable for upgrade.
#[derive(Debug, Clone)]
pub struct Release {
    /// Complete semantic version, including prerelease and build metadata.
    pub version: semver::Version,
    /// Whether Cargo should avoid selecting this release.
    pub yanked: bool,
    /// Declared minimum Rust version, when provided by the publisher.
    pub rust_version: Option<String>,
    /// Declared features and unsuppressed implicit optional-dependency features.
    pub features: BTreeSet<String>,
}

/// A registry client with per-source single-flight release caching.
pub struct RegistryClient {
    http: reqwest::Client,
    options: RegistryOptions,
    config: toml::Value,
    cargo_home: Option<PathBuf>,
    requests: Semaphore,
    releases: ReleaseCache,
    validated_sources: Mutex<BTreeSet<String>>,
}

impl RegistryClient {
    /// Load Cargo configuration and construct an anonymous sparse-index client.
    ///
    /// Cargo-home configuration is merged with ancestor `.cargo` configuration,
    /// with the workspace nearest configuration taking precedence. Environment
    /// registry-index overrides are resolved at lookup time.
    ///
    /// # Errors
    ///
    /// * If options are invalid or the HTTP client cannot be constructed.
    /// * If Cargo configuration cannot be read/parsed or uses unsupported includes.
    pub fn new(workspace_root: &Path, options: RegistryOptions) -> Result<Self, BoxError> {
        if options.timeout_seconds == 0 || options.max_concurrency == 0 {
            return Err("registry timeout and concurrency must be greater than zero".into());
        }
        // Keep retries and semaphore allocation bounded even for untrusted config.
        if options.max_retries > 10 || options.max_concurrency > 1024 {
            return Err("registry retries must be <= 10 and concurrency must be <= 1024".into());
        }
        let cargo_home = cargo_home();
        let config = load_config(workspace_root, cargo_home.as_deref())?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(options.timeout_seconds))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("clippier/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "failed to construct registry HTTP client")?;
        Ok(Self {
            http,
            requests: Semaphore::new(options.max_concurrency),
            options,
            config,
            cargo_home,
            releases: Mutex::new(BTreeMap::new()),
            validated_sources: Mutex::new(BTreeSet::new()),
        })
    }

    /// Return all published versions in ascending semantic-version order.
    ///
    /// `None` selects crates.io (not Cargo's default *publishing* registry).
    /// Explicit registry names and source replacements never fall back to it.
    /// A missing crate returns an empty vector; other failures are errors.
    ///
    /// # Errors
    ///
    /// * If the crate name, registry configuration, or index data is invalid.
    /// * If a source is unsupported or requires authentication.
    /// * If the offline cache is missing, unreadable, or malformed.
    /// * If an HTTP request fails after bounded retries.
    pub async fn releases(
        &self,
        name: &str,
        registry: Option<&str>,
    ) -> Result<Vec<Release>, BoxError> {
        let path = index_path(name)?;
        let source = resolve_source(&self.config, registry)?;
        let offline = self.options.offline || config_offline(&self.config)?;
        self.releases_from_source(name, &path, &source, offline)
            .await
    }

    async fn releases_from_source(
        &self,
        name: &str,
        path: &str,
        source: &str,
        offline: bool,
    ) -> Result<Vec<Release>, BoxError> {
        let key = format!("{source}{path}");
        let entry = self.releases.lock().await.entry(key).or_default().clone();
        let mut cached = entry.lock().await;
        if let Some(releases) = cached.as_ref() {
            return Ok(releases.clone());
        }
        let releases = if offline {
            self.read_offline(source, path, name)?
        } else {
            self.validate_source(source).await?;
            self.fetch(&format!("{}{path}", &source[7..]))
                .await?
                .map_or_else(|| Ok(Vec::new()), |bytes| parse_index(&bytes, name))?
        };
        *cached = Some(releases.clone());
        drop(cached);
        Ok(releases)
    }

    async fn validate_source(&self, source: &str) -> Result<(), BoxError> {
        // Serialize initial config checks. Subsequent crate requests remain parallel.
        let mut validated = self.validated_sources.lock().await;
        if validated.contains(source) {
            return Ok(());
        }
        let bytes = self
            .fetch(&format!("{}config.json", &source[7..]))
            .await?
            .ok_or("registry config.json was not found")?;
        let config: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "registry config.json is not valid JSON")?;
        if !config.is_object() {
            return Err("registry config.json must be an object".into());
        }
        if let Some(required) = config.get("auth-required")
            && required
                .as_bool()
                .ok_or("registry auth-required must be a boolean")?
        {
            return Err("authenticated registries are not supported by upgrade discovery".into());
        }
        validated.insert(source.to_owned());
        drop(validated);
        Ok(())
    }

    async fn fetch(&self, url: &str) -> Result<Option<Vec<u8>>, BoxError> {
        for attempt in 0..=self.options.max_retries {
            let permit = self
                .requests
                .acquire()
                .await
                .map_err(|_| "registry request limiter is closed")?;
            let response = self.http.get(url).send().await;
            if let Ok(mut response) = response {
                let status = response.status();
                if status == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    return Err("registry requires authentication or denied access".into());
                }
                if status.is_success() {
                    if response
                        .content_length()
                        .is_some_and(|len| len > MAX_INDEX_BYTES as u64)
                    {
                        return Err("registry response exceeds the index size limit".into());
                    }
                    let mut bytes = Vec::new();
                    // Do not include reqwest errors: they can contain private URLs.
                    while let Some(chunk) = response
                        .chunk()
                        .await
                        .map_err(|_| "failed to read registry response body")?
                    {
                        if chunk.len() > MAX_INDEX_BYTES.saturating_sub(bytes.len()) {
                            return Err("registry response exceeds the index size limit".into());
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    return Ok(Some(bytes));
                }
                if !(status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS) {
                    return Err(format!(
                        "registry returned HTTP {} (redirects are not followed)",
                        status.as_u16()
                    )
                    .into());
                }
            }
            drop(permit);
            if attempt < self.options.max_retries {
                switchy_async::time::sleep(Duration::from_millis(100 * (1_u64 << attempt))).await;
            }
        }
        Err("registry request failed after configured retries (URL omitted)".into())
    }

    fn read_offline(&self, source: &str, path: &str, name: &str) -> Result<Vec<Release>, BoxError> {
        // Cargo's alternate-registry directory hash is an implementation detail.
        // Never guess by scanning caches: equal crate names are not source identity.
        if source != CRATES_IO {
            return Err("offline cache discovery is only supported for the canonical crates.io sparse index".into());
        }
        let home = self
            .cargo_home
            .as_ref()
            .ok_or("Cargo home is unavailable for offline lookup")?;
        let cache = home
            .join("registry/index/index.crates.io-1949cf8c6b5b557f/.cache")
            .join(path);
        let metadata = fs::metadata(&cache)
            .map_err(|_| "crate is not available in Cargo's offline sparse cache")?;
        if metadata.len() > MAX_INDEX_BYTES as u64 {
            return Err("Cargo sparse cache exceeds the index size limit".into());
        }
        let file = fs::File::open(cache).map_err(|_| "cannot read Cargo's offline sparse cache")?;
        let mut bytes = Vec::new();
        file.take(MAX_INDEX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read Cargo's offline sparse cache")?;
        if bytes.len() > MAX_INDEX_BYTES {
            return Err("Cargo sparse cache exceeds the index size limit".into());
        }
        parse_sparse_cache(&bytes, name)
    }
}

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".cargo"))
        })
}

fn load_config(root: &Path, home: Option<&Path>) -> Result<toml::Value, BoxError> {
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let root = if root.is_file() {
        root.parent().ok_or("workspace manifest has no parent")?
    } else {
        &root
    };
    let mut directories = Vec::new();
    if let Some(home) = home {
        directories.push(home.to_path_buf());
    }
    directories.extend(
        root.ancestors()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|dir| dir.join(".cargo")),
    );
    let mut config = toml::Value::Table(toml::map::Map::new());
    let mut seen = BTreeSet::new();
    // Retain the last occurrence: CARGO_HOME may also be an ancestor .cargo,
    // and its workspace position must still outrank farther ancestors.
    directories.reverse();
    directories.retain(|directory| seen.insert(directory.clone()));
    directories.reverse();
    for directory in directories {
        // Cargo gives the extensionless file precedence when both exist.
        let legacy = directory.join("config");
        let path = if legacy.exists() {
            legacy
        } else {
            directory.join("config.toml")
        };
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("cannot read Cargo registry configuration".into()),
        };
        let value: toml::Value =
            toml::from_str(&text).map_err(|_| "invalid Cargo configuration (contents omitted)")?;
        if value.get("include").is_some() {
            return Err(
                "Cargo configuration includes are not supported by upgrade discovery".into(),
            );
        }
        merge_config(&mut config, value);
    }
    Ok(config)
}

fn merge_config(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                if let Some(existing) = base.get_mut(&key) {
                    merge_config(existing, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (toml::Value::Array(base), toml::Value::Array(mut overlay)) => base.append(&mut overlay),
        (base, overlay) => *base = overlay,
    }
}

fn config_offline(config: &toml::Value) -> Result<bool, BoxError> {
    if let Some(value) = std::env::var_os("CARGO_NET_OFFLINE") {
        return match value.to_str() {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err("CARGO_NET_OFFLINE must be true or false".into()),
        };
    }
    config
        .get("net")
        .and_then(|net| net.get("offline"))
        .map_or(Ok(false), |value| {
            value
                .as_bool()
                .ok_or_else(|| "Cargo net.offline must be a boolean".into())
        })
}

fn env_registry_key(name: &str) -> Result<String, BoxError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("invalid Cargo registry name".into());
    }
    Ok(format!(
        "CARGO_REGISTRIES_{}",
        name.to_ascii_uppercase().replace('-', "_")
    ))
}

fn resolve_source(config: &toml::Value, registry: Option<&str>) -> Result<String, BoxError> {
    resolve_source_with_env(config, registry, |key| std::env::var_os(key))
}

fn resolve_source_with_env(
    config: &toml::Value,
    registry: Option<&str>,
    env: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Result<String, BoxError> {
    validate_config_shape(config)?;
    let name = registry.unwrap_or("crates-io");
    let env_key = env_registry_key(name)?;
    let registries = config.get("registries");
    let registry_config = registries.and_then(|registries| registries.get(name));
    reject_auth(registry_config, &env_key, &env)?;
    if config
        .get("registry")
        .and_then(|registry| registry.get("credential-provider"))
        .is_some()
        || config
            .get("registry")
            .and_then(|registry| registry.get("global-credential-providers"))
            .is_some()
        || env("CARGO_REGISTRY_TOKEN").is_some()
        || env("CARGO_REGISTRY_CREDENTIAL_PROVIDER").is_some()
        || env("CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS").is_some()
    {
        return Err("configured Cargo authentication is not supported by upgrade discovery".into());
    }
    let protocol = env(&format!("{env_key}_PROTOCOL"));
    let protocol = protocol
        .as_deref()
        .map(|value| value.to_str().ok_or("registry protocol is not UTF-8"))
        .transpose()?
        .or(registry_config
            .and_then(|value| value.get("protocol"))
            .map(|value| value.as_str().ok_or("registry protocol must be a string"))
            .transpose()?);
    if protocol.is_some_and(|protocol| protocol != "sparse") {
        return Err("only the Cargo sparse registry protocol is supported".into());
    }
    let override_index = env(&format!("{env_key}_INDEX"))
        .map(|value| {
            value
                .into_string()
                .map_err(|_| "registry index environment override is not UTF-8")
        })
        .transpose()?;
    let index = if let Some(index) = override_index {
        index
    } else if let Some(value) = registry_config.and_then(|value| value.get("index")) {
        value
            .as_str()
            .ok_or("registry index must be a string")?
            .to_owned()
    } else if name == "crates-io" {
        CRATES_IO.to_owned()
    } else {
        return Err("named Cargo registry has no configured index".into());
    };
    resolve_replacement(config, name, index, &env)
}

fn resolve_replacement(
    config: &toml::Value,
    name: &str,
    mut index: String,
    env: &impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Result<String, BoxError> {
    let registries = config.get("registries");
    // Replacement can be selected by reserved crates-io name or matching index.
    let sources = config.get("source").and_then(toml::Value::as_table);
    let mut current = if name == "crates-io" {
        Some("crates-io".to_owned())
    } else {
        None
    };
    if current.is_none()
        && let Some(sources) = sources
    {
        let matches = sources
            .iter()
            .filter(|(_, source)| {
                source.get("registry").and_then(toml::Value::as_str) == Some(index.as_str())
            })
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err("ambiguous Cargo source replacement for registry index".into());
        }
        current = matches.into_iter().next();
    }
    let mut seen = BTreeSet::new();
    while let Some(name) = current.take() {
        if !seen.insert(name.clone()) {
            return Err("cycle in Cargo source replacement".into());
        }
        let Some(source) = sources.and_then(|sources| sources.get(&name)) else {
            if name == "crates-io" {
                break;
            }
            return Err("Cargo source replacement refers to an undefined source".into());
        };
        if source.get("directory").is_some()
            || source.get("local-registry").is_some()
            || source.get("git").is_some()
        {
            return Err(
                "local, directory, and git Cargo sources are not supported for registry discovery"
                    .into(),
            );
        }
        if let Some(value) = source.get("registry") {
            value
                .as_str()
                .ok_or("Cargo source registry must be a string")?
                .clone_into(&mut index);
        } else if name != "crates-io" {
            return Err("replacement source has no registry index".into());
        }
        if let Some(replacement) = source.get("replace-with") {
            current = Some(
                replacement
                    .as_str()
                    .ok_or("Cargo replace-with must be a string")?
                    .to_owned(),
            );
        }
    }
    let resolved = sparse_source(&index)?;
    // A replacement may itself be a named authenticated registry. Do not ignore
    // its credential arrangement merely because it was selected via [source].
    if let Some(registries) = registries.and_then(toml::Value::as_table) {
        for (name, entry) in registries {
            if let Some(index) = entry.get("index").and_then(toml::Value::as_str)
                && sparse_source(index).ok().as_deref() == Some(resolved.as_str())
            {
                reject_auth(Some(entry), &env_registry_key(name)?, env)?;
            }
        }
    }
    Ok(resolved)
}

fn validate_config_shape(config: &toml::Value) -> Result<(), BoxError> {
    for key in ["source", "registries", "registry", "net"] {
        if let Some(value) = config.get(key) {
            let table = value
                .as_table()
                .ok_or("Cargo registry configuration section must be a table")?;
            if matches!(key, "source" | "registries") {
                for entry in table.values() {
                    if !entry.is_table() {
                        return Err("Cargo source and registry entries must be tables".into());
                    }
                }
            }
        }
    }
    Ok(())
}

fn reject_auth(
    config: Option<&toml::Value>,
    env_key: &str,
    env: &impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Result<(), BoxError> {
    if config.is_some_and(|value| {
        value.get("token").is_some() || value.get("credential-provider").is_some()
    }) || env(&format!("{env_key}_TOKEN")).is_some()
        || env(&format!("{env_key}_CREDENTIAL_PROVIDER")).is_some()
    {
        return Err(
            "configured registry credentials are not supported by upgrade discovery".into(),
        );
    }
    Ok(())
}

fn sparse_source(index: &str) -> Result<String, BoxError> {
    let index = index
        .strip_prefix("sparse+")
        .ok_or("only sparse Cargo registry indexes are supported; configure a sparse+ URL")?;
    let mut url =
        reqwest::Url::parse(index).map_err(|_| "invalid sparse registry URL (URL omitted)")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "sparse registry requires an HTTP(S) URL without credentials, query, or fragment"
                .into(),
        );
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(format!("sparse+{url}"))
}

fn index_path(name: &str) -> Result<String, BoxError> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("invalid Cargo crate name for registry lookup".into());
    }
    let name = name.to_ascii_lowercase();
    Ok(match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    })
}

#[derive(Deserialize)]
struct IndexRelease {
    name: String,
    vers: String,
    #[serde(default)]
    yanked: bool,
    rust_version: Option<String>,
    #[serde(default)]
    features: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    features2: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    deps: Vec<IndexDependency>,
    v: Option<u64>,
}

#[derive(Deserialize)]
struct IndexDependency {
    // Cargo index `name` is the local alias; `package` (if present) is the
    // registry package name and does not determine the implicit feature name.
    name: String,
    #[serde(default)]
    optional: bool,
}

fn index_features(record: &IndexRelease) -> BTreeSet<String> {
    let suppressed = record
        .features
        .values()
        .chain(record.features2.values())
        .flatten()
        .filter_map(|reference| reference.strip_prefix("dep:"))
        .collect::<BTreeSet<_>>();
    // Only `dep:alias` removes an implicit feature. References `alias/feature`
    // and `alias?/feature` address dependency features, not local feature names,
    // and neither suppresses the implicit optional-dependency feature.
    record
        .features
        .keys()
        .chain(record.features2.keys())
        .chain(record.deps.iter().filter_map(|dependency| {
            (dependency.optional && !suppressed.contains(dependency.name.as_str()))
                .then_some(&dependency.name)
        }))
        .cloned()
        .collect()
}

fn parse_index(bytes: &[u8], name: &str) -> Result<Vec<Release>, BoxError> {
    let text = std::str::from_utf8(bytes).map_err(|_| "registry index is not UTF-8")?;
    let mut releases = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let record: IndexRelease =
            serde_json::from_str(line).map_err(|_| "invalid registry index release JSON")?;
        if !record.name.eq_ignore_ascii_case(name) {
            return Err("registry index record has a mismatched crate name".into());
        }
        if record.v.is_some_and(|version| version > 2) {
            return Err("registry index uses an unsupported schema version".into());
        }
        let version = semver::Version::parse(&record.vers)
            .map_err(|_| "registry index contains an invalid semantic version")?;
        let features = index_features(&record);
        let release = Release {
            version: version.clone(),
            yanked: record.yanked,
            rust_version: record.rust_version,
            features,
        };
        if releases.insert(version, release).is_some() {
            return Err("registry index contains duplicate release versions".into());
        }
    }
    Ok(releases.into_values().collect())
}

fn parse_sparse_cache(bytes: &[u8], name: &str) -> Result<Vec<Release>, BoxError> {
    // Cargo sparse cache v3: byte 3, little-endian index format 2,
    // NUL-terminated etag, then alternating version and JSON record fields.
    if bytes.len() > MAX_INDEX_BYTES || bytes.len() < 6 || bytes[..5] != [3, 2, 0, 0, 0] {
        return Err("unsupported Cargo sparse cache format".into());
    }
    if bytes.last() != Some(&0) {
        return Err("truncated Cargo sparse cache terminator".into());
    }
    let mut fields = bytes[5..].split(|byte| *byte == 0);
    fields.next().ok_or("missing Cargo sparse cache header")?;
    let mut json = Vec::new();
    while let Some(version) = fields.next() {
        if version.is_empty() {
            if fields.next().is_some() {
                return Err("malformed Cargo sparse cache terminator".into());
            }
            break;
        }
        let record = fields.next().ok_or("truncated Cargo sparse cache record")?;
        let parsed: IndexRelease =
            serde_json::from_slice(record).map_err(|_| "invalid Cargo sparse cache JSON")?;
        if parsed.vers.as_bytes() != version {
            return Err("Cargo sparse cache version mismatch".into());
        }
        json.extend_from_slice(record);
        json.push(b'\n');
    }
    parse_index(&json, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(text: &str) -> toml::Value {
        toml::from_str(text).unwrap()
    }

    fn source(config: &toml::Value, name: Option<&str>) -> Result<String, BoxError> {
        resolve_source_with_env(config, name, |_| None)
    }

    #[test]
    fn cargo_index_paths_are_ascii_lowercase() {
        for (name, path) in [
            ("A", "1/a"),
            ("Ab", "2/ab"),
            ("ABC", "3/a/abc"),
            ("Serde_JSON", "se/rd/serde_json"),
        ] {
            assert_eq!(index_path(name).unwrap(), path);
        }
        for name in ["", "../secret", "a/b", "é", "a?token=x"] {
            assert!(index_path(name).is_err());
        }
    }

    #[test]
    fn options_are_defaulted_and_strict() {
        let options: RegistryOptions = serde_json::from_str(r#"{"max-concurrency":3}"#).unwrap();
        assert_eq!(options.max_concurrency, 3);
        assert_eq!(options.timeout_seconds, 30);
        assert!(serde_json::from_str::<RegistryOptions>(r#"{"max_concurrency":3}"#).is_err());
    }

    #[test]
    fn merge_preserves_ancestor_registry_fields() {
        let mut base = config(
            "[registries.company]\nindex = 'sparse+https://example.invalid/index/'\n[net]\noffline = true",
        );
        merge_config(
            &mut base,
            config(
                "[registries.other]\nindex = 'sparse+https://other.invalid/'\n[net]\noffline = false",
            ),
        );
        assert!(base["registries"]["company"]["index"].as_str().is_some());
        assert_eq!(base["net"]["offline"].as_bool(), Some(false));
    }

    #[test]
    fn replacement_chains_and_cycles() {
        let good = config(
            "[source.crates-io]\nreplace-with = 'mirror'\n[source.mirror]\nregistry = 'sparse+https://example.invalid/index/'",
        );
        assert_eq!(
            source(&good, None).unwrap(),
            "sparse+https://example.invalid/index/"
        );
        let cycle = config(
            "[source.crates-io]\nreplace-with = 'mirror'\n[source.mirror]\nregistry = 'sparse+https://example.invalid/'\nreplace-with = 'crates-io'",
        );
        assert!(source(&cycle, None).is_err());
        let missing = config("[source.crates-io]\nreplace-with = 'missing'");
        assert!(source(&missing, None).is_err());
    }

    #[test]
    fn unknown_and_unsupported_registries_do_not_fall_back() {
        assert!(source(&config(""), Some("clippier-test-unknown-registry")).is_err());
        for index in [
            "https://github.com/rust-lang/crates.io-index",
            "sparse+file:///tmp/index/",
            "sparse+https://user:secret@example.invalid/",
            "sparse+https://example.invalid/?token=secret",
        ] {
            let error = sparse_source(index).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
        let local = config(
            "[source.crates-io]\nreplace-with = 'local'\n[source.local]\ndirectory = '/tmp/vendor'",
        );
        assert!(source(&local, None).is_err());
    }

    #[test]
    fn metadata_and_sorting_include_yanked_prereleases_and_features2() {
        let bytes = br#"{"name":"demo","vers":"2.0.0","yanked":true,"rust_version":"1.80","features":{"default":[]},"features2":{"extra":["dep:optional"]},"v":2}
{"name":"demo","vers":"1.0.0-beta.1","features":{}}
{"name":"demo","vers":"1.0.0","features":{}}
"#;
        let releases = parse_index(bytes, "DEMO").unwrap();
        assert_eq!(releases[0].version.to_string(), "1.0.0-beta.1");
        assert_eq!(releases[1].version.to_string(), "1.0.0");
        assert!(releases[2].yanked);
        assert_eq!(releases[2].rust_version.as_deref(), Some("1.80"));
        assert_eq!(
            releases[2].features,
            BTreeSet::from(["default".into(), "extra".into()])
        );
        for bad in [
            br#"{"name":"other","vers":"1.0.0"}"#.as_slice(),
            br#"{"name":"demo","vers":"bad"}"#,
            br#"{"name":"demo","vers":"1.0.0","v":3}"#,
            b"not json",
        ] {
            assert!(parse_index(bad, "demo").is_err());
        }
    }

    #[test]
    fn optional_dependencies_expose_local_alias_features() {
        let releases = parse_index(br#"{"name":"demo","vers":"1.0.0","deps":[{"name":"codec","optional":true},{"name":"alias","package":"actual-package","optional":true},{"name":"required","optional":false}],"features":{"declared":[]}}"#, "demo").unwrap();
        assert_eq!(
            releases[0].features,
            BTreeSet::from(["alias".into(), "codec".into(), "declared".into()])
        );
    }

    #[test]
    fn namespaced_dependency_references_suppress_only_implicit_features() {
        for map in ["features", "features2"] {
            let row = format!(
                r#"{{"name":"demo","vers":"1.0.0","deps":[{{"name":"codec","optional":true}},{{"name":"other","optional":true}}],"{map}":{{"bundle":["dep:codec"],"codec":[]}}}}"#
            );
            let releases = parse_index(row.as_bytes(), "demo").unwrap();
            // An explicitly declared feature is retained even when the implicit
            // feature of the same name is suppressed by dep:codec.
            assert_eq!(
                releases[0].features,
                BTreeSet::from(["bundle".into(), "codec".into(), "other".into()])
            );
            let row = row.replace(",\"codec\":[]", "");
            let releases = parse_index(row.as_bytes(), "demo").unwrap();
            assert_eq!(
                releases[0].features,
                BTreeSet::from(["bundle".into(), "other".into()])
            );
        }
    }

    #[test]
    fn dependency_feature_and_weak_references_do_not_suppress_implicit_features() {
        let releases = parse_index(br#"{"name":"demo","vers":"1.0.0","deps":[{"name":"strong","optional":true},{"name":"weak","optional":true},{"name":"hidden","optional":true}],"features":{"bundle":["strong/encode","weak?/encode"]},"features2":{"extended":["dep:hidden","hidden?/decode"]},"v":2}"#, "demo").unwrap();
        assert_eq!(
            releases[0].features,
            BTreeSet::from([
                "bundle".into(),
                "extended".into(),
                "strong".into(),
                "weak".into()
            ])
        );
    }

    #[test]
    fn cargo_sparse_cache_records_are_validated() {
        let mut bytes = vec![3, 2, 0, 0, 0];
        bytes.extend_from_slice(b"etag: test\0");
        bytes.extend_from_slice(b"1.0.0\0{\"name\":\"demo\",\"vers\":\"1.0.0\"}\0");
        assert_eq!(parse_sparse_cache(&bytes, "demo").unwrap().len(), 1);
        bytes[0] = 4;
        assert!(parse_sparse_cache(&bytes, "demo").is_err());
    }

    #[test]
    fn named_sources_protocol_and_auth_are_explicit() {
        let named = config(
            "[registries.company]\nindex = 'sparse+https://company.invalid/index/'\n[source.company-source]\nregistry = 'sparse+https://company.invalid/index/'\nreplace-with = 'mirror'\n[source.mirror]\nregistry = 'sparse+https://mirror.invalid/'",
        );
        assert_eq!(
            source(&named, Some("company")).unwrap(),
            "sparse+https://mirror.invalid/"
        );
        let env = |key: &str| match key {
            "CARGO_REGISTRIES_COMPANY_INDEX" => Some("sparse+https://override.invalid/".into()),
            _ => None,
        };
        assert_eq!(
            resolve_source_with_env(&named, Some("company"), env).unwrap(),
            "sparse+https://override.invalid/"
        );
        let auth = config(
            "[registries.company]\nindex = 'sparse+https://company.invalid/'\ntoken = 'never-print-this'",
        );
        let error = source(&auth, Some("company")).unwrap_err().to_string();
        assert!(!error.contains("never-print-this"));
        assert!(source(&config("[registries.crates-io]\nprotocol = 'git'"), None).is_err());
        assert!(source(&config("source = 'invalid'"), None).is_err());
    }

    #[test]
    fn malformed_cache_and_duplicate_releases_fail() {
        let row = br#"{"name":"demo","vers":"1.0.0"}"#;
        let mut duplicate = row.to_vec();
        duplicate.push(b'\n');
        duplicate.extend_from_slice(row);
        assert!(parse_index(&duplicate, "demo").is_err());
        let mut cache = vec![3, 2, 0, 0, 0];
        cache.extend_from_slice(b"etag\0wrong-version\0");
        cache.extend_from_slice(row);
        cache.push(0);
        assert!(parse_sparse_cache(&cache, "demo").is_err());
        cache.pop();
        assert!(parse_sparse_cache(&cache, "demo").is_err());
    }

    fn mock_registry(
        responses: Vec<String>,
    ) -> (RegistryClient, String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "mock request timed out"
                            );
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("mock accept failed: {error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap();
                requests.push(String::from_utf8_lossy(&request[..count]).into_owned());
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        let client = RegistryClient {
            http: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(1))
                .build()
                .unwrap(),
            options: RegistryOptions {
                max_retries: 0,
                ..RegistryOptions::default()
            },
            config: config(""),
            cargo_home: None,
            requests: Semaphore::new(1),
            releases: Mutex::new(BTreeMap::new()),
            validated_sources: Mutex::new(BTreeSet::new()),
        };
        (client, format!("sparse+http://{address}/"), worker)
    }

    fn response(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[switchy_async::test(real_time)]
    async fn auth_required_and_http_denials_stop_before_crate_lookup() {
        for reply in [
            response("200 OK", r#"{"auth-required":true}"#),
            response("401 Unauthorized", "private credential details"),
            response("403 Forbidden", "private credential details"),
        ] {
            let (client, source, worker) = mock_registry(vec![reply]);
            let error = client
                .releases_from_source("demo", "de/mo/demo", &source, false)
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains("auth") || error.contains("denied"));
            assert!(!error.contains("private credential details"));
            let requests = worker.join().unwrap();
            assert_eq!(requests.len(), 1);
            assert!(requests[0].starts_with("GET /config.json "));
        }
    }

    #[switchy_async::test(real_time)]
    async fn missing_config_errors_but_missing_crate_is_empty_and_cached() {
        let (client, source, worker) = mock_registry(vec![response("404 Not Found", "")]);
        assert!(
            client
                .releases_from_source("demo", "de/mo/demo", &source, false)
                .await
                .unwrap_err()
                .to_string()
                .contains("config.json")
        );
        assert_eq!(worker.join().unwrap().len(), 1);

        let (client, source, worker) = mock_registry(vec![
            response("200 OK", "{}"),
            response("404 Not Found", ""),
        ]);
        assert!(
            client
                .releases_from_source("demo", "de/mo/demo", &source, false)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            client
                .releases_from_source("demo", "de/mo/demo", &source, true)
                .await
                .unwrap()
                .is_empty()
        );
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].starts_with("GET /de/mo/demo "));
    }

    #[switchy_async::test(real_time)]
    async fn malformed_and_oversized_http_metadata_are_rejected() {
        for body in ["not JSON", "[]", r#"{"auth-required":"yes"}"#] {
            let (client, source, worker) = mock_registry(vec![response("200 OK", body)]);
            assert!(client.validate_source(&source).await.is_err());
            assert_eq!(worker.join().unwrap().len(), 1);
        }
        let oversized = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_INDEX_BYTES + 1
        );
        let (client, source, worker) = mock_registry(vec![oversized]);
        assert!(
            client
                .validate_source(&source)
                .await
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
        assert_eq!(worker.join().unwrap().len(), 1);

        let (client, source, worker) = mock_registry(vec![
            response("200 OK", "{}"),
            response("200 OK", r#"{"name":"demo","vers":"1.0.0","features":[]}"#),
        ]);
        assert!(
            client
                .releases_from_source("demo", "de/mo/demo", &source, false)
                .await
                .unwrap_err()
                .to_string()
                .contains("JSON")
        );
        assert_eq!(worker.join().unwrap().len(), 2);
    }

    #[test]
    fn cargo_home_ancestor_and_legacy_config_precedence() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let parent = temp.path().join("workspace");
        let child = parent.join("child");
        for directory in [&home, &parent.join(".cargo"), &child.join(".cargo")] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            home.join("config.toml"),
            "[registries.fixture]\nindex = 'sparse+https://home.invalid/'\n[net]\noffline = true",
        )
        .unwrap();
        fs::write(
            parent.join(".cargo/config.toml"),
            "[registries.fixture]\nindex = 'sparse+https://parent.invalid/'",
        )
        .unwrap();
        fs::write(
            child.join(".cargo/config.toml"),
            "[registries.fixture]\nindex = 'sparse+https://ignored.invalid/'",
        )
        .unwrap();
        fs::write(
            child.join(".cargo/config"),
            "[registries.fixture]\nindex = 'sparse+https://child.invalid/'",
        )
        .unwrap();
        let loaded = load_config(&child, Some(&home)).unwrap();
        assert_eq!(
            source(&loaded, Some("fixture")).unwrap(),
            "sparse+https://child.invalid/"
        );
        assert_eq!(loaded["net"]["offline"].as_bool(), Some(true));
        // A Cargo home that is itself an ancestor .cargo still takes its
        // workspace precedence rather than being discarded as a duplicate.
        let loaded = load_config(&child, Some(&child.join(".cargo"))).unwrap();
        assert_eq!(
            source(&loaded, Some("fixture")).unwrap(),
            "sparse+https://child.invalid/"
        );
    }

    // Loopback only: a real HTTP response exercises reqwest status/body handling.
    #[switchy_async::test(real_time)]
    async fn http_retries_and_release_cache_are_native() {
        use std::io::Write as _;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut requests = Vec::new();
            while requests.len() < 3 && std::time::Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("mock accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap();
                requests.push(String::from_utf8_lossy(&request[..count]).to_string());
                let (status, body) = match requests.len() {
                    1 => (
                        "200 OK",
                        "{\"dl\":\"https://unused.invalid\",\"auth-required\":false}",
                    ),
                    2 => ("503 Service Unavailable", ""),
                    _ => (
                        "200 OK",
                        "{\"name\":\"Demo\",\"vers\":\"1.2.3\",\"features\":{}}\n",
                    ),
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            requests
        });
        // Construct directly so host Cargo config/environment cannot affect the mock.
        let client = RegistryClient {
            http: reqwest::Client::builder().no_proxy().build().unwrap(),
            options: RegistryOptions::default(),
            config: config(&format!(
                "[registries.clippier-loopback-fixture]\nindex = 'sparse+http://{address}/'"
            )),
            cargo_home: None,
            requests: Semaphore::new(2),
            releases: Mutex::new(BTreeMap::new()),
            validated_sources: Mutex::new(BTreeSet::new()),
        };
        // Explicit source and offline policy isolate tests from host Cargo env.
        let source = format!("sparse+http://{address}/");
        let (first, second) = switchy_async::join!(
            client.releases_from_source("Demo", "de/mo/demo", &source, false),
            client.releases_from_source("Demo", "de/mo/demo", &source, false)
        );
        assert_eq!(first.unwrap()[0].version.to_string(), "1.2.3");
        assert_eq!(second.unwrap()[0].version.to_string(), "1.2.3");
        assert_eq!(
            client
                .releases_from_source("Demo", "de/mo/demo", &source, true)
                .await
                .unwrap()
                .len(),
            1
        );
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].starts_with("GET /config.json "));
        assert!(requests[2].starts_with("GET /de/mo/demo "));
        assert!(
            requests
                .iter()
                .all(|request| !request.to_ascii_lowercase().contains("authorization:"))
        );
    }
}
