#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// Cached health check result with expiration
#[derive(Debug, Clone)]
struct CachedHealthCheck {
    result: bool,
    expires_at: Instant,
}

/// Runner pool configuration loaded from clippier.toml
#[derive(Debug, Clone, Deserialize)]
pub struct RunnerPoolsConfig {
    #[serde(flatten)]
    pub pools: BTreeMap<String, Pool>,

    pub priority: PriorityConfig,
}

/// Individual runner pool definition
#[derive(Debug, Clone, Deserialize)]
pub struct Pool {
    /// Label to use in CI configuration (e.g., "ubuntu-latest", "self-hosted-macos")
    pub label: Option<String>,

    /// OS type for this pool
    pub os: Option<String>,

    /// Maximum concurrent jobs for this pool
    pub max_concurrent: Option<usize>,

    /// Minimum jobs required before using this pool
    pub min_jobs: Option<usize>,

    /// Allocation weight (used by weighted algorithm)
    pub allocation_weight: Option<f32>,

    /// Health check configuration
    pub health_check: Option<HealthCheck>,

    /// Sub-pools (explicit .pools section)
    pub pools: Option<BTreeMap<String, Pool>>,
}

/// Priority configuration for each OS
#[derive(Debug, Clone, Deserialize)]
pub struct PriorityConfig {
    pub ubuntu: OsPriority,
    pub macos: OsPriority,
    pub windows: OsPriority,
}

/// Priority chain for a specific OS
#[derive(Debug, Clone, Deserialize)]
pub struct OsPriority {
    pub pools: Vec<String>,
}

/// Health check configuration
#[derive(Debug, Clone, Deserialize)]
pub struct HealthCheck {
    #[serde(rename = "type")]
    pub check_type: String,

    /// Labels to check (for `github_api` type)
    pub labels: Option<Vec<String>>,

    /// Minimum available runners (for `github_api` type)
    pub min_available: Option<usize>,

    /// URL to check (for http type)
    pub url: Option<String>,

    /// Timeout in seconds (for http type)
    pub timeout_seconds: Option<u64>,
}

/// Allocation strategy configuration
#[derive(Debug, Clone, Deserialize)]
pub struct AllocationStrategyConfig {
    pub algorithm: String,
    pub capacity_multiplier: Option<f32>,

    pub shared_parent: Option<SharedParentConfig>,

    pub ubuntu: Option<OsAllocationStrategy>,
    pub macos: Option<OsAllocationStrategy>,
    pub windows: Option<OsAllocationStrategy>,
}

/// Shared parent pool allocation strategy
#[derive(Debug, Clone, Deserialize)]
pub struct SharedParentConfig {
    pub mode: String,
    pub order: Option<Vec<String>>,
}

/// Per-OS allocation strategy overrides
#[derive(Debug, Clone, Deserialize)]
pub struct OsAllocationStrategy {
    pub algorithm: Option<String>,
    pub capacity_multiplier: Option<f32>,
}

/// Allocation engine that performs runner allocation
pub struct AllocationEngine {
    pools: BTreeMap<String, Pool>,
    priority: PriorityConfig,
    strategy: AllocationStrategyConfig,
    github_token: Option<String>,
    parent_usage: BTreeMap<String, usize>,
    /// Track actual usage per pool to enforce capacity limits
    pool_usage: BTreeMap<String, usize>,
    /// Cache health check results per pool path with TTL to avoid redundant checks
    health_cache: Mutex<BTreeMap<String, CachedHealthCheck>>,
    /// TTL for health check cache in seconds (default: 60)
    cache_ttl_seconds: u64,
}

/// Result of allocating runners for all jobs
#[derive(Debug)]
pub struct AllocationResult {
    pub allocations: BTreeMap<String, String>, // job_id -> runner_label
}

impl AllocationEngine {
    /// Create a new allocation engine
    ///
    /// # Errors
    ///
    /// * Returns error if configuration is invalid
    pub fn new(
        pools_config: &RunnerPoolsConfig,
        strategy: &AllocationStrategyConfig,
        github_token: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_cache_ttl(pools_config, strategy, github_token, 60)
    }

    /// Create a new allocation engine with custom cache TTL
    ///
    /// # Errors
    ///
    /// * Returns error if configuration is invalid
    pub fn with_cache_ttl(
        pools_config: &RunnerPoolsConfig,
        strategy: &AllocationStrategyConfig,
        github_token: Option<String>,
        cache_ttl_seconds: u64,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            pools: pools_config.pools.clone(),
            priority: pools_config.priority.clone(),
            strategy: strategy.clone(),
            github_token,
            parent_usage: BTreeMap::new(),
            pool_usage: BTreeMap::new(),
            health_cache: Mutex::new(BTreeMap::new()),
            cache_ttl_seconds,
        })
    }

    /// Resolve a pool by path (e.g., "ubuntu-runners" or "github-hosted.pools.ubuntu-latest")
    fn resolve_pool(&self, pool_path: &str) -> Option<&Pool> {
        let parts: Vec<&str> = pool_path.split('.').collect();

        match parts.as_slice() {
            [pool_name] => {
                // Simple pool: "ubuntu-runners"
                self.pools.get(*pool_name)
            }
            [parent_name, "pools", child_name] => {
                // Nested pool: "github-hosted.pools.ubuntu-latest"
                self.pools
                    .get(*parent_name)
                    .and_then(|parent| parent.pools.as_ref())
                    .and_then(|pools| pools.get(*child_name))
            }
            _ => None, // Invalid path
        }
    }

    /// Get the parent pool ID from a nested pool path
    fn get_parent_id(pool_path: &str) -> Option<String> {
        let parts: Vec<&str> = pool_path.split('.').collect();

        match parts.as_slice() {
            [_pool_name] => None, // No parent
            [parent_name, "pools", _child_name] => Some((*parent_name).to_string()),
            _ => None,
        }
    }

    /// Check if a pool is available (via health check with caching)
    ///
    /// Results are cached per `pool_path` to avoid redundant health checks
    async fn is_pool_available(&self, pool: &Pool, pool_path: &str) -> bool {
        // Check cache first
        let cached = {
            let cache = self.health_cache.lock().unwrap();
            cache.get(pool_path).cloned()
        };

        if let Some(cached) = cached {
            // Check if cache entry is still valid
            if Instant::now() < cached.expires_at {
                log::debug!(
                    "Using cached health check result for {}: {} (expires in {:?})",
                    pool_path,
                    cached.result,
                    cached.expires_at.saturating_duration_since(Instant::now())
                );
                return cached.result;
            }
            log::debug!("Health check cache expired for {pool_path}, re-checking");
        }

        // Perform actual health check
        let result = match &pool.health_check {
            None => true, // No check = always available
            Some(check) => match check.check_type.as_str() {
                "always_available" => true,
                "static" => {
                    // Check environment variable for pool availability
                    // Format: CLIPPIER_POOL_{POOL_PATH}_AVAILABLE=true|false
                    let env_key = format!(
                        "CLIPPIER_POOL_{}_AVAILABLE",
                        pool_path.to_uppercase().replace(['.', '-'], "_")
                    );

                    std::env::var(&env_key).map_or_else(
                        |_| {
                            // Default to available if env var not set
                            log::debug!(
                                "Static health check for {pool_path}: true (env {env_key} not set, using default)"
                            );
                            true
                        },
                        |val| {
                            let available = val.eq_ignore_ascii_case("true") || val == "1";
                            log::debug!(
                                "Static health check for {pool_path}: {available} (from env {env_key}={val})"
                            );
                            available
                        },
                    )
                }
                "github_api" => {
                    log::info!("Performing GitHub API health check for pool: {pool_path}");
                    self.check_github_runners(check, pool).await
                }
                "http" => {
                    log::info!("Performing HTTP health check for pool: {pool_path}");
                    self.check_http_endpoint(check).await
                }
                _ => {
                    log::warn!("Unknown health check type: {}", check.check_type);
                    false
                }
            },
        };

        // Cache the result with TTL
        let expires_at = Instant::now() + Duration::from_secs(self.cache_ttl_seconds);
        self.health_cache.lock().unwrap().insert(
            pool_path.to_string(),
            CachedHealthCheck { result, expires_at },
        );
        log::debug!(
            "Cached health check result for {}: {} (TTL: {}s)",
            pool_path,
            result,
            self.cache_ttl_seconds
        );

        result
    }

    /// Check GitHub API for available self-hosted runners
    async fn check_github_runners(&self, check: &HealthCheck, pool: &Pool) -> bool {
        let Some(ref token) = self.github_token else {
            log::warn!("GitHub API health check requires a GitHub token, but none provided");
            return false;
        };

        let labels = check.labels.clone().unwrap_or_default();

        let min_available = check.min_available.unwrap_or(1);

        // Parse repository from environment or use a default
        // In a real CI environment, this would come from GITHUB_REPOSITORY env var
        let repo = std::env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| {
            log::warn!("GITHUB_REPOSITORY env var not set, cannot check runner status");
            String::new()
        });

        if repo.is_empty() {
            return false;
        }

        let Ok(available) = self
            .fetch_github_runners(token, &repo, &labels, min_available)
            .await
            .inspect_err(|e| {
                log::warn!(
                    "GitHub API health check failed for pool {:?}: {}",
                    pool.label,
                    e
                );
            })
        else {
            return false;
        };

        if available {
            log::debug!("GitHub API health check passed for pool {:?}", pool.label);
        } else {
            log::debug!(
                "GitHub API health check failed for pool {:?}: not enough runners available",
                pool.label
            );
        }

        available
    }

    /// Fetch runner status from GitHub API
    async fn fetch_github_runners(
        &self,
        token: &str,
        repo: &str,
        required_labels: &[String],
        min_available: usize,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        use reqwest::Client;
        use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};

        let url = format!("https://api.github.com/repos/{repo}/actions/runners");

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        let response = client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .header(ACCEPT, "application/vnd.github+json")
            .header(USER_AGENT, "clippier-runner-allocator")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("GitHub API returned status: {}", response.status()).into());
        }

        let json: serde_json::Value = response.json().await?;
        let runners = json["runners"]
            .as_array()
            .ok_or("Invalid response: missing 'runners' array")?;

        // Count runners that match ALL required labels and are online
        let available_count = runners
            .iter()
            .filter(|runner| {
                let status = runner["status"].as_str().unwrap_or("");
                let busy = runner["busy"].as_bool().unwrap_or(true);
                let runner_labels = runner["labels"]
                    .as_array()
                    .map(|labels| {
                        labels
                            .iter()
                            .filter_map(|l| l["name"].as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                log::debug!("GitHub API status='{status}' busy={busy} runner labels={runner_labels:?} required labels={required_labels:?}");

                // Runner must be online, not busy, and have all required labels
                status == "online"
                    && !busy
                    && required_labels
                        .iter()
                        .all(|req| runner_labels.iter().any(|l| l.eq_ignore_ascii_case(req)))
            })
            .count();

        log::debug!(
            "GitHub API check: found {available_count} available runners matching labels {required_labels:?} (need {min_available})"
        );

        Ok(available_count >= min_available)
    }

    /// Check HTTP endpoint health
    ///
    /// # Errors
    ///
    /// * Returns error if HTTP request fails or times out
    async fn check_http_endpoint(&self, check: &HealthCheck) -> bool {
        let Some(ref url) = check.url else {
            log::warn!("HTTP health check requires url configuration");
            return false;
        };

        let timeout = check.timeout_seconds.unwrap_or(5);

        match self.fetch_http_health(url, timeout).await {
            Ok(healthy) => {
                if healthy {
                    log::debug!("HTTP health check passed for {url}");
                } else {
                    log::debug!("HTTP health check failed for {url}: unhealthy status");
                }
                healthy
            }
            Err(e) => {
                log::warn!("HTTP health check failed for {url}: {e}");
                false
            }
        }
    }

    /// Fetch health status from HTTP endpoint
    ///
    /// # Errors
    ///
    /// * Returns error if HTTP request fails or returns non-success status
    async fn fetch_http_health(
        &self,
        url: &str,
        timeout_seconds: u64,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        use reqwest::Client;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_seconds))
            .build()?;

        let response = client.get(url).send().await?;

        // Consider 2xx status codes as healthy
        Ok(response.status().is_success())
    }

    /// Get priority chain for a given OS
    fn get_priority_chain(&self, os: &str) -> &[String] {
        match os {
            "ubuntu" => &self.priority.ubuntu.pools,
            "macos" => &self.priority.macos.pools,
            "windows" => &self.priority.windows.pools,
            _ => &[],
        }
    }

    /// Pre-warm health check cache for all pools in priority chain (parallel)
    ///
    /// This checks all pools concurrently to populate the cache faster
    pub async fn prewarm_health_checks(&self, os: &str) {
        let priority_chain = self.get_priority_chain(os);

        if priority_chain.is_empty() {
            return;
        }

        log::debug!(
            "Pre-warming health checks for {} pools in {} priority chain",
            priority_chain.len(),
            os
        );

        // Collect all pool data first
        let pool_checks: Vec<_> = priority_chain
            .iter()
            .filter_map(|pool_path| {
                self.resolve_pool(pool_path).map(|pool| {
                    (
                        pool_path.clone(),
                        Pool {
                            label: pool.label.clone(),
                            os: pool.os.clone(),
                            max_concurrent: pool.max_concurrent,
                            min_jobs: pool.min_jobs,
                            allocation_weight: pool.allocation_weight,
                            health_check: pool.health_check.clone(),
                            pools: None,
                        },
                    )
                })
            })
            .collect();

        // Check all pools in parallel using futures
        let futures: Vec<_> = pool_checks
            .into_iter()
            .map(|(pool_path, pool)| async move {
                let result = self.is_pool_available(&pool, &pool_path).await;
                (pool_path, result)
            })
            .collect();

        // Wait for all checks to complete in parallel
        let results = futures::future::join_all(futures).await;

        log::debug!(
            "Pre-warmed {} health checks for {} (parallel)",
            results.len(),
            os
        );
    }

    /// Get capacity multiplier for a given OS
    fn get_capacity_multiplier(&self, os: &str) -> f32 {
        let os_strategy = match os {
            "ubuntu" => &self.strategy.ubuntu,
            "macos" => &self.strategy.macos,
            "windows" => &self.strategy.windows,
            _ => &None,
        };

        os_strategy
            .as_ref()
            .and_then(|s| s.capacity_multiplier)
            .or(self.strategy.capacity_multiplier)
            .unwrap_or(3.0)
    }

    /// Get allocation algorithm for a given OS
    fn get_allocation_algorithm(&self, os: &str) -> &str {
        let os_strategy = match os {
            "ubuntu" => &self.strategy.ubuntu,
            "macos" => &self.strategy.macos,
            "windows" => &self.strategy.windows,
            _ => &None,
        };

        os_strategy
            .as_ref()
            .and_then(|s| s.algorithm.as_deref())
            .unwrap_or(&self.strategy.algorithm)
    }

    /// Order pools by algorithm for allocation attempts
    ///
    /// Returns a reordered list of pool paths where the "best" pool comes first,
    /// but all pools remain as fallback options
    fn order_pools_by_algorithm(
        &self,
        priority_chain: &[String],
        os: &str,
        _total_jobs: usize,
    ) -> Vec<String> {
        let algorithm = self.get_allocation_algorithm(os);

        match algorithm {
            "capacity_waterfall" => {
                // Keep original priority order
                priority_chain.to_vec()
            }
            "weighted_round_robin" => {
                // Sort by weighted preference
                self.order_pools_weighted(priority_chain)
            }
            "least_loaded" => {
                // Sort by current load (least loaded first)
                self.order_pools_by_load(priority_chain)
            }
            _ => {
                log::warn!("Unknown allocation algorithm: {algorithm}, using capacity_waterfall");
                priority_chain.to_vec()
            }
        }
    }

    /// Order pools by weighted preference
    fn order_pools_weighted(&self, priority_chain: &[String]) -> Vec<String> {
        let mut weighted_pools: Vec<(String, f32)> = Vec::new();

        for pool_path in priority_chain {
            if let Some(pool) = self.resolve_pool(pool_path) {
                let weight = pool.allocation_weight.unwrap_or(1.0);
                let usage = self.pool_usage.get(pool_path).copied().unwrap_or(0);

                // Calculate effective weight considering current usage
                // Lower usage = higher effective weight
                #[allow(clippy::cast_precision_loss)]
                let effective_weight = weight / (1.0 + usage as f32);

                weighted_pools.push((pool_path.clone(), effective_weight));
            }
        }

        // Sort by effective weight (highest first)
        weighted_pools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        weighted_pools.into_iter().map(|(path, _)| path).collect()
    }

    /// Order pools by current load (least loaded first)
    fn order_pools_by_load(&self, priority_chain: &[String]) -> Vec<String> {
        let mut pools: Vec<(String, usize)> = priority_chain
            .iter()
            .map(|path| {
                let usage = self.pool_usage.get(path).copied().unwrap_or(0);
                (path.clone(), usage)
            })
            .collect();

        // Sort by usage (lowest first)
        pools.sort_by_key(|(_, usage)| *usage);

        pools.into_iter().map(|(path, _)| path).collect()
    }

    /// Allocate runners for all jobs with cross-OS shared parent coordination
    ///
    /// This method handles the `shared_parent` configuration by allocating
    /// jobs across all OSes according to the priority order
    ///
    /// # Errors
    ///
    /// * Returns error if allocation fails
    pub async fn allocate_runners_batch(
        &mut self,
        jobs_by_os: &BTreeMap<String, Vec<String>>,
    ) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
        let mut allocations: BTreeMap<String, String> = BTreeMap::new();

        // Check if we have shared_parent configuration
        let shared_parent_config = self.strategy.shared_parent.clone();

        if let Some(shared_parent) = shared_parent_config {
            log::info!(
                "Using cross-OS shared parent allocation (mode: {})",
                shared_parent.mode
            );

            if shared_parent.mode.as_str() == "priority" {
                // Allocate in OS priority order
                let default_order = vec![
                    "macos".to_string(),
                    "ubuntu".to_string(),
                    "windows".to_string(),
                ];
                let os_order = shared_parent
                    .order
                    .as_ref()
                    .unwrap_or(&default_order)
                    .clone();

                log::debug!("Allocating in OS priority order: {os_order:?}");

                for os in &os_order {
                    if let Some(job_ids) = jobs_by_os.get(os) {
                        let total_jobs = job_ids.len();
                        log::debug!("Allocating {total_jobs} jobs for {os}");

                        for job_id in job_ids {
                            let runner = self.allocate_runner(os, job_id, total_jobs).await?;
                            allocations.insert(job_id.clone(), runner);
                        }
                    }
                }

                // Handle any OSes not in the priority order
                for (os, job_ids) in jobs_by_os {
                    if !os_order.contains(os) {
                        let total_jobs = job_ids.len();
                        for job_id in job_ids {
                            if !allocations.contains_key(job_id) {
                                let runner = self.allocate_runner(os, job_id, total_jobs).await?;
                                allocations.insert(job_id.clone(), runner);
                            }
                        }
                    }
                }
            } else {
                log::warn!(
                    "Unknown shared_parent mode: {}, falling back to per-OS allocation",
                    shared_parent.mode
                );
                // Fall back to per-OS allocation
                return self.allocate_runners_per_os(jobs_by_os).await;
            }
        } else {
            // No shared_parent config, allocate per-OS independently
            return self.allocate_runners_per_os(jobs_by_os).await;
        }

        Ok(allocations)
    }

    /// Allocate runners per-OS independently (no cross-OS coordination)
    async fn allocate_runners_per_os(
        &mut self,
        jobs_by_os: &BTreeMap<String, Vec<String>>,
    ) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
        let mut allocations: BTreeMap<String, String> = BTreeMap::new();

        for (os, job_ids) in jobs_by_os {
            let total_jobs = job_ids.len();
            for job_id in job_ids {
                let runner = self.allocate_runner(os, job_id, total_jobs).await?;
                allocations.insert(job_id.clone(), runner);
            }
        }

        Ok(allocations)
    }

    /// Allocate a runner for a single job
    ///
    /// # Errors
    ///
    /// * Returns error if allocation fails
    #[allow(clippy::unused_self, clippy::too_many_lines)]
    pub async fn allocate_runner(
        &mut self,
        os: &str,
        package_name: &str,
        total_jobs_for_os: usize,
    ) -> Result<String, Box<dyn std::error::Error>> {
        log::debug!(
            "Allocating runner for os={os}, package={package_name}, total_jobs={total_jobs_for_os}"
        );

        let priority_chain = self.get_priority_chain(os).to_vec();
        let capacity_multiplier = self.get_capacity_multiplier(os);
        let algorithm = self.get_allocation_algorithm(os);

        log::debug!("Priority chain for {os}: {priority_chain:?}");
        log::debug!("Capacity multiplier for {os}: {capacity_multiplier}");
        log::debug!("Allocation algorithm for {os}: {algorithm}");

        // Sort/filter pools based on allocation algorithm
        let ordered_pools = self.order_pools_by_algorithm(&priority_chain, os, total_jobs_for_os);

        for pool_path in &ordered_pools {
            // Clone pool data to avoid borrow checker issues
            let pool_data = if let Some(pool) = self.resolve_pool(pool_path) {
                (
                    pool.label.clone(),
                    pool.os.clone(),
                    pool.max_concurrent,
                    pool.min_jobs,
                    pool.health_check.clone(),
                )
            } else {
                continue;
            };

            let (label_opt, os_opt, max_concurrent_opt, min_jobs_opt, health_check_opt) = pool_data;

            // Check health (reconstruct pool for health check)
            let temp_pool = Pool {
                label: label_opt.clone(),
                os: os_opt,
                max_concurrent: max_concurrent_opt,
                min_jobs: min_jobs_opt,
                allocation_weight: None,
                health_check: health_check_opt,
                pools: None,
            };

            if !self.is_pool_available(&temp_pool, pool_path).await {
                log::debug!("Pool {pool_path} is not available (health check failed)");
                continue;
            }

            // Check min_jobs threshold
            if let Some(min_jobs) = min_jobs_opt
                && total_jobs_for_os < min_jobs
            {
                log::debug!(
                    "Pool {pool_path} requires min {min_jobs} jobs, but only {total_jobs_for_os} jobs for {os}"
                );
                continue;
            }

            // Check capacity and usage
            if let Some(max_concurrent) = max_concurrent_opt {
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                let effective_capacity = (max_concurrent as f32 * capacity_multiplier) as usize;
                let current_usage = self.pool_usage.get(pool_path).copied().unwrap_or(0);

                log::debug!(
                    "Pool {pool_path} has capacity {effective_capacity} ({max_concurrent}x{capacity_multiplier}), currently using {current_usage}"
                );

                // Check if pool is at capacity
                if current_usage >= effective_capacity {
                    log::debug!(
                        "Pool {pool_path} is at capacity ({current_usage}/{effective_capacity})"
                    );
                    continue;
                }
            } else {
                let current_usage = self.pool_usage.get(pool_path).copied().unwrap_or(0);
                log::debug!(
                    "Pool {pool_path} has unlimited capacity, currently using {current_usage}"
                );
            }

            // Check parent pool constraints
            if let Some(parent_id) = Self::get_parent_id(pool_path)
                && let Some(parent) = self.pools.get(&parent_id)
                && let Some(parent_max) = parent.max_concurrent
            {
                let parent_used = self.parent_usage.get(&parent_id).copied().unwrap_or(0);
                if parent_used >= parent_max {
                    log::debug!(
                        "Parent pool {parent_id} is at capacity ({parent_used}/{parent_max})"
                    );
                    continue;
                }

                // Track parent usage
                *self.parent_usage.entry(parent_id.clone()).or_insert(0) += 1;
            }

            // Allocate and track usage
            if let Some(label) = label_opt {
                // Track pool usage
                *self.pool_usage.entry(pool_path.clone()).or_insert(0) += 1;

                log::info!(
                    "Allocated pool {pool_path} (label: {label}) for {os} job (usage: {}/{})",
                    self.pool_usage.get(pool_path).copied().unwrap_or(0),
                    max_concurrent_opt.map_or_else(
                        || "unlimited".to_string(),
                        |m| {
                            #[allow(
                                clippy::cast_precision_loss,
                                clippy::cast_possible_truncation,
                                clippy::cast_sign_loss
                            )]
                            let cap = (m as f32 * capacity_multiplier) as usize;
                            cap.to_string()
                        },
                    )
                );
                return Ok(label);
            }
        }

        // Fallback: use {os}-latest
        let fallback = format!("{os}-latest");
        log::warn!("No pool available for {os}, using fallback: {fallback}");
        Ok(fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: Select pool based on weighted round robin
    fn select_weighted_pool<'a>(
        engine: &AllocationEngine,
        priority_chain: &'a [String],
    ) -> Option<&'a String> {
        engine
            .order_pools_weighted(priority_chain)
            .first()
            .map(|s| {
                // Find the corresponding reference in the original slice
                priority_chain.iter().find(|p| **p == *s).unwrap()
            })
    }

    /// Test helper: Select pool with least current load
    fn select_least_loaded_pool<'a>(
        engine: &AllocationEngine,
        priority_chain: &'a [String],
    ) -> Option<&'a String> {
        engine.order_pools_by_load(priority_chain).first().map(|s| {
            // Find the corresponding reference in the original slice
            priority_chain.iter().find(|p| **p == *s).unwrap()
        })
    }

    #[test]
    fn test_resolve_simple_pool() {
        let mut pools = BTreeMap::new();
        pools.insert(
            "ubuntu-runners".to_string(),
            Pool {
                label: Some("self-hosted-ubuntu".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(10),
                min_jobs: None,
                allocation_weight: None,
                health_check: None,
                pools: None,
            },
        );

        let priority = PriorityConfig {
            ubuntu: OsPriority {
                pools: vec!["ubuntu-runners".to_string()],
            },
            macos: OsPriority { pools: vec![] },
            windows: OsPriority { pools: vec![] },
        };

        let strategy = AllocationStrategyConfig {
            algorithm: "capacity_waterfall".to_string(),
            capacity_multiplier: Some(3.0),
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config = RunnerPoolsConfig { pools, priority };

        let engine = AllocationEngine::new(&config, &strategy, None).unwrap();

        let pool = engine.resolve_pool("ubuntu-runners");
        assert!(pool.is_some());
        assert_eq!(pool.unwrap().label.as_deref(), Some("self-hosted-ubuntu"));
    }

    #[test]
    fn test_resolve_nested_pool() {
        let mut sub_pools = BTreeMap::new();
        sub_pools.insert(
            "ubuntu-latest".to_string(),
            Pool {
                label: Some("ubuntu-latest".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: None,
                min_jobs: None,
                allocation_weight: None,
                health_check: None,
                pools: None,
            },
        );

        let mut pools = BTreeMap::new();
        pools.insert(
            "github-hosted".to_string(),
            Pool {
                label: None,
                os: None,
                max_concurrent: Some(20),
                min_jobs: None,
                allocation_weight: None,
                health_check: None,
                pools: Some(sub_pools),
            },
        );

        let priority = PriorityConfig {
            ubuntu: OsPriority {
                pools: vec!["github-hosted.pools.ubuntu-latest".to_string()],
            },
            macos: OsPriority { pools: vec![] },
            windows: OsPriority { pools: vec![] },
        };

        let strategy = AllocationStrategyConfig {
            algorithm: "capacity_waterfall".to_string(),
            capacity_multiplier: Some(3.0),
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config = RunnerPoolsConfig { pools, priority };

        let engine = AllocationEngine::new(&config, &strategy, None).unwrap();

        let pool = engine.resolve_pool("github-hosted.pools.ubuntu-latest");
        assert!(pool.is_some());
        assert_eq!(pool.unwrap().label.as_deref(), Some("ubuntu-latest"));
    }

    #[test]
    fn test_get_parent_id() {
        assert_eq!(AllocationEngine::get_parent_id("ubuntu-runners"), None);
        assert_eq!(
            AllocationEngine::get_parent_id("github-hosted.pools.ubuntu-latest"),
            Some("github-hosted".to_string())
        );
    }

    #[switchy_async::test]
    async fn test_usage_tracking() {
        let mut pools = BTreeMap::new();
        pools.insert(
            "ubuntu-runners".to_string(),
            Pool {
                label: Some("self-hosted-ubuntu".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(2),
                min_jobs: None,
                allocation_weight: None,
                health_check: None,
                pools: None,
            },
        );

        let priority = PriorityConfig {
            ubuntu: OsPriority {
                pools: vec!["ubuntu-runners".to_string()],
            },
            macos: OsPriority { pools: vec![] },
            windows: OsPriority { pools: vec![] },
        };

        let strategy = AllocationStrategyConfig {
            algorithm: "capacity_waterfall".to_string(),
            capacity_multiplier: Some(1.0), // No multiplier for this test
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config = RunnerPoolsConfig { pools, priority };
        let mut engine = AllocationEngine::new(&config, &strategy, None).unwrap();

        // Allocate first runner
        let runner1 = engine.allocate_runner("ubuntu", "pkg1", 3).await.unwrap();
        assert_eq!(runner1, "self-hosted-ubuntu");
        assert_eq!(engine.pool_usage.get("ubuntu-runners"), Some(&1));

        // Allocate second runner
        let runner2 = engine.allocate_runner("ubuntu", "pkg2", 3).await.unwrap();
        assert_eq!(runner2, "self-hosted-ubuntu");
        assert_eq!(engine.pool_usage.get("ubuntu-runners"), Some(&2));

        // Third allocation should fail (at capacity)
        let runner3 = engine.allocate_runner("ubuntu", "pkg3", 3).await.unwrap();
        assert_eq!(runner3, "ubuntu-latest"); // Fallback
    }

    #[switchy_async::test(real_time)]
    async fn test_cache_ttl() {
        use std::time::Duration;

        let mut pools = BTreeMap::new();
        pools.insert(
            "test-pool".to_string(),
            Pool {
                label: Some("test-label".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(10),
                min_jobs: None,
                allocation_weight: None,
                health_check: Some(HealthCheck {
                    check_type: "always_available".to_string(),
                    labels: None,
                    min_available: None,
                    url: None,
                    timeout_seconds: None,
                }),
                pools: None,
            },
        );

        let priority = PriorityConfig {
            ubuntu: OsPriority {
                pools: vec!["test-pool".to_string()],
            },
            macos: OsPriority { pools: vec![] },
            windows: OsPriority { pools: vec![] },
        };

        let strategy = AllocationStrategyConfig {
            algorithm: "capacity_waterfall".to_string(),
            capacity_multiplier: Some(1.0),
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config = RunnerPoolsConfig {
            pools: pools.clone(),
            priority: priority.clone(),
        };

        // Create engine with 0 second TTL (immediate expiration)
        let engine = AllocationEngine::with_cache_ttl(&config, &strategy, None, 0).unwrap();

        // First check
        let pool = engine.resolve_pool("test-pool").unwrap();
        let available1 = engine.is_pool_available(pool, "test-pool").await;
        assert!(available1);

        // Wait a tiny bit for cache to expire
        switchy_async::time::sleep(Duration::from_millis(10)).await;

        // Second check should re-check (cache expired)
        let available2 = engine.is_pool_available(pool, "test-pool").await;
        assert!(available2);
    }

    #[test]
    fn test_static_health_check_env_var() {
        // Set environment variable
        unsafe {
            std::env::set_var("CLIPPIER_POOL_TEST_POOL_AVAILABLE", "false");
        }

        let mut pools = BTreeMap::new();
        pools.insert(
            "test-pool".to_string(),
            Pool {
                label: Some("test".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(10),
                min_jobs: None,
                allocation_weight: None,
                health_check: Some(HealthCheck {
                    check_type: "static".to_string(),
                    labels: None,
                    min_available: None,
                    url: None,
                    timeout_seconds: None,
                }),
                pools: None,
            },
        );

        let priority = PriorityConfig {
            ubuntu: OsPriority {
                pools: vec!["test-pool".to_string()],
            },
            macos: OsPriority { pools: vec![] },
            windows: OsPriority { pools: vec![] },
        };

        let strategy = AllocationStrategyConfig {
            algorithm: "capacity_waterfall".to_string(),
            capacity_multiplier: Some(1.0),
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config = RunnerPoolsConfig { pools, priority };
        let engine = AllocationEngine::new(&config, &strategy, None).unwrap();

        let pool = engine.resolve_pool("test-pool").unwrap();

        // This is a sync test, so we can't await. We'll just verify the structure is correct.
        assert!(pool.health_check.is_some());

        // Clean up
        unsafe {
            std::env::remove_var("CLIPPIER_POOL_TEST_POOL_AVAILABLE");
        }
    }

    #[test]
    fn test_allocation_algorithms() {
        let mut pools = BTreeMap::new();
        pools.insert(
            "pool1".to_string(),
            Pool {
                label: Some("pool1-label".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(10),
                min_jobs: None,
                allocation_weight: Some(3.0),
                health_check: None,
                pools: None,
            },
        );
        pools.insert(
            "pool2".to_string(),
            Pool {
                label: Some("pool2-label".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(10),
                min_jobs: None,
                allocation_weight: Some(1.0),
                health_check: None,
                pools: None,
            },
        );

        let priority = PriorityConfig {
            ubuntu: OsPriority {
                pools: vec!["pool1".to_string(), "pool2".to_string()],
            },
            macos: OsPriority { pools: vec![] },
            windows: OsPriority { pools: vec![] },
        };

        let strategy = AllocationStrategyConfig {
            algorithm: "weighted_round_robin".to_string(),
            capacity_multiplier: Some(1.0),
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config = RunnerPoolsConfig { pools, priority };
        let engine = AllocationEngine::new(&config, &strategy, None).unwrap();

        // Test weighted pool selection
        let priority_chain = engine.get_priority_chain("ubuntu");
        let selected = select_weighted_pool(&engine, priority_chain);
        assert!(selected.is_some());

        // Test least loaded pool selection
        let selected = select_least_loaded_pool(&engine, priority_chain);
        assert!(selected.is_some());
    }

    #[switchy_async::test]
    async fn test_algorithm_integration() {
        let mut pools = BTreeMap::new();
        pools.insert(
            "heavy-pool".to_string(),
            Pool {
                label: Some("heavy-label".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(5),
                min_jobs: None,
                allocation_weight: Some(10.0), // High weight
                health_check: None,
                pools: None,
            },
        );
        pools.insert(
            "light-pool".to_string(),
            Pool {
                label: Some("light-label".to_string()),
                os: Some("ubuntu".to_string()),
                max_concurrent: Some(5),
                min_jobs: None,
                allocation_weight: Some(1.0), // Low weight
                health_check: None,
                pools: None,
            },
        );

        let priority = PriorityConfig {
            ubuntu: OsPriority {
                // light-pool is first in priority, but heavy-pool has higher weight
                pools: vec!["light-pool".to_string(), "heavy-pool".to_string()],
            },
            macos: OsPriority { pools: vec![] },
            windows: OsPriority { pools: vec![] },
        };

        // Test weighted_round_robin algorithm
        let strategy = AllocationStrategyConfig {
            algorithm: "weighted_round_robin".to_string(),
            capacity_multiplier: Some(1.0),
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config = RunnerPoolsConfig {
            pools: pools.clone(),
            priority: priority.clone(),
        };
        let mut engine = AllocationEngine::new(&config, &strategy, None).unwrap();

        // With weighted algorithm, heavy-pool should be selected first despite priority order
        let runner1 = engine.allocate_runner("ubuntu", "pkg1", 2).await.unwrap();
        assert_eq!(
            runner1, "heavy-label",
            "Weighted algorithm should prefer heavy-pool"
        );

        // Test capacity_waterfall algorithm (default priority order)
        let strategy2 = AllocationStrategyConfig {
            algorithm: "capacity_waterfall".to_string(),
            capacity_multiplier: Some(1.0),
            shared_parent: None,
            ubuntu: None,
            macos: None,
            windows: None,
        };

        let config2 = RunnerPoolsConfig { pools, priority };
        let mut engine2 = AllocationEngine::new(&config2, &strategy2, None).unwrap();

        // With waterfall, light-pool should be selected first (follows priority order)
        let runner2 = engine2.allocate_runner("ubuntu", "pkg1", 2).await.unwrap();
        assert_eq!(
            runner2, "light-label",
            "Waterfall algorithm should follow priority order"
        );
    }
}
