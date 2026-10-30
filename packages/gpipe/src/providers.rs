//! Administrator-side, free-only hosted runner adapters.
//!
//! These clients never retry dispatches. A timeout can follow a successful remote
//! mutation: retain the correlation and reconcile instead of dispatching again.
//! GitHub workflows must declare the `gpipe_correlation` input and set their
//! `run-name` to exactly `${{ inputs.gpipe_correlation }}` for discovery to work.
//! GitLab pipelines receive `GPIPE_CORRELATION` as a trigger variable.
//!
//! Eligibility and capacity are administrator assertions, not billing discovery.
//! The caller must atomically reserve *all* jobs a workflow can start, enforce
//! its runner classes/timeouts, release reservations only after reconciliation,
//! and ensure cross-repository work belongs to the approved software project.
//! Provider credentials are resolved only when an operation is invoked, remain
//! administrator-side, and must never be included in worker bootstrap inputs.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::{Client, Request, RequestBuilder};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue};
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const CORRELATION_INPUT: &str = "gpipe_correlation";
const CORRELATION_VARIABLE: &str = "GPIPE_CORRELATION";

/// Explicit administrator approval for free hosted capacity.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostedCapacityPolicy {
    /// Must be explicitly true after checking runner pricing, quotas and terms.
    /// Public visibility alone is not confirmation (especially on GitLab).
    pub free_eligibility_confirmed: bool,
    /// Maximum simultaneous jobs, including already reserved jobs.
    pub max_active_jobs: u32,
    /// Absolute approval expiry, in Unix seconds; never an unlimited worker lease.
    pub deadline_unix_seconds: u64,
    /// Exact hosting repository (`owner/repo` or GitLab project path/ID).
    /// This approval covers only software associated with that repository.
    pub repository_scope: String,
}

/// Evidence of an external, atomic capacity reservation; not a lock itself.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapacityReservation {
    /// Unique, durable reservation identity in the caller's accounting store.
    pub reservation_id: String,
    /// Exact hosting repository covered by the reservation.
    pub repository: String,
    /// Positive number of jobs reserved for this dispatch, including fan-out.
    pub reserved_jobs: u32,
    /// Global active/reserved jobs *including* this reservation.
    pub active_jobs_including_reservation: u32,
    /// Deadline that the caller also enforces in workflow/worker configuration.
    pub deadline_unix_seconds: u64,
}

impl HostedCapacityPolicy {
    /// Validate a reservation before any credential resolution or network I/O.
    ///
    /// # Errors
    /// * Returns `Policy` for missing eligibility, scope mismatch, invalid or
    ///   exhausted capacity, expired approval, or an out-of-policy deadline.
    pub fn validate_reservation(
        &self,
        reservation: &CapacityReservation,
        repository: &str,
        now_unix_seconds: u64,
    ) -> Result<(), ProviderError> {
        let approval_deadline = self.deadline_unix_seconds;
        if !self.free_eligibility_confirmed
            || repository.trim().is_empty()
            || self.repository_scope != repository
            || reservation.repository != repository
            || self.deadline_unix_seconds <= now_unix_seconds
            || reservation.reservation_id.trim().is_empty()
            || self.max_active_jobs == 0
            || reservation.reserved_jobs == 0
            || reservation.active_jobs_including_reservation < reservation.reserved_jobs
            || reservation.active_jobs_including_reservation > self.max_active_jobs
            || reservation.deadline_unix_seconds <= now_unix_seconds
            || reservation.deadline_unix_seconds > approval_deadline
        {
            return Err(ProviderError::Policy);
        }
        Ok(())
    }
}

/// Non-secret reference to an administrator environment variable.
/// Values are never serialized or retained in provider configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialEnvironment {
    /// Environment variable name, not its secret value.
    pub variable: String,
}

impl CredentialEnvironment {
    fn resolve(&self) -> Result<String, ProviderError> {
        if self.variable.is_empty()
            || !self.variable.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
            })
        {
            return Err(ProviderError::Configuration);
        }
        std::env::var(&self.variable)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(ProviderError::Credential)
    }
}

/// GitHub.com workflow endpoint. Enterprise/alternate API origins are not allowed.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubConfiguration {
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repository: String,
    /// Workflow ID or file basename (for example `gpipe.yml`).
    pub workflow: String,
    /// Administrator API token with appropriate Actions permissions.
    pub token: CredentialEnvironment,
}

/// GitLab endpoint and separate trigger/API credential references.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitLabConfiguration {
    /// Explicit public HTTPS origin on port 443, for example `https://gitlab.com`;
    /// no path, userinfo, private literals or internal names. DNS addresses are
    /// checked and pinned at send time; redirects and proxies are disabled.
    pub origin: String,
    /// Numeric project ID or namespace/project path, encoded as one API segment.
    pub project: String,
    /// Pipeline trigger token, sent only to the trigger endpoint in its JSON body.
    pub trigger_token: CredentialEnvironment,
    /// API token used only for exact-ID inspection/cancellation.
    pub api_token: CredentialEnvironment,
}

/// Stable, non-secret dispatch data. Inputs must not contain provider credentials.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRequest {
    /// Durable identity reused for reconciliation, never a random retry identity.
    pub correlation: String,
    /// Hosting workflow/pipeline branch or tag (not the tested source revision).
    pub reference: String,
    /// Declared workflow inputs / GitLab trigger variables; correlation is reserved.
    pub inputs: BTreeMap<String, String>,
}

/// Dispatch acceptance is deliberately distinct from execution success.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DispatchOutcome {
    /// Remote run identity returned by the provider.
    Started {
        /// Exact numeric workflow run/pipeline ID.
        id: u64,
    },
    /// Provider accepted dispatch but supplied no identity (GitHub legacy 204).
    AcceptedWithoutId {
        /// Identity to reconcile using the declared workflow run-name contract.
        correlation: String,
    },
    /// Dispatch might have been accepted. Do not retry automatically.
    Unknown {
        /// Original identity retained for investigation/reconciliation.
        correlation: String,
        /// Secret-safe diagnostic, never a response body or transport URL.
        reason: String,
    },
}

/// Provider state, not Clippier receipt verification or proof of goal completion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedRun {
    /// Exact provider run/pipeline ID.
    pub id: u64,
    /// Provider-native state; callers must handle new/unknown states conservatively.
    pub status: String,
    /// GitHub conclusion; GitLab communicates its result through `status`.
    pub conclusion: Option<String>,
}

/// Final, receipt-verified upstream result. This type deliberately cannot publish
/// pending/running as a final result; provider acceptance is not verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinalPipelineResult {
    /// Every planned case has been verified successfully.
    Success,
    /// At least one case failed or final reconciliation was incomplete.
    Failure,
    /// Execution was canceled before successful verification.
    Canceled,
}

/// Commit status to publish upstream using an administrator API token only.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitStatusRequest {
    /// Exact full source commit hash (40 or 64 hexadecimal digits), never a ref.
    pub revision: String,
    /// Stable status context/name, at most 100 bytes.
    pub context: String,
    /// Final result established by the caller through receipt verification.
    pub result: FinalPipelineResult,
}

/// Provider's acknowledged commit status, not a new pipeline dispatch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedCommitStatus {
    /// Positive status record ID.
    pub id: u64,
    /// Exact requested source revision.
    pub revision: String,
    /// Provider-native final state.
    pub state: String,
}

/// Secret-safe errors. Transport/body details are intentionally not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderError {
    /// Invalid configuration, URL, identifier, correlation or input payload.
    Configuration,
    /// Free-only approval or caller capacity reservation is invalid.
    Policy,
    /// Referenced credential is absent, non-Unicode, empty or header-incompatible.
    Credential,
    /// HTTP client initialization, request construction or transport failed.
    Transport,
    /// Provider rejected a request; only the status code is retained.
    Http {
        /// HTTP status, without potentially secret-bearing response text.
        status: u16,
    },
    /// Response exceeds the fixed bound or cannot be read completely.
    ResponseBody,
    /// JSON schema, exact identity or correlation is invalid/ambiguous.
    ResponseSchema,
    /// Bounded pagination ended before a complete search; absence is not proven.
    IncompleteSearch,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http { status } => write!(formatter, "hosted provider HTTP status {status}"),
            _ => write!(formatter, "hosted provider error: {self:?}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Blocking provider transport with HTTPS-only URLs and bounded requests.
///
/// Redirects and automatic retries are disabled. Connection establishment has a
/// ten-second timeout and each request has a thirty-second deadline.
/// GitLab DNS resolution adds at most five seconds of caller wait and pins only
/// public addresses. The OS resolver thread can outlive that wait; callers should
/// bound concurrent API operations. No secret-bearing `Debug` implementation is
/// provided.
pub struct HostedProviders {
    client: Client,
}

impl HostedProviders {
    /// Construct the bounded transport; no credentials are read and no I/O occurs.
    /// Environment proxies are disabled to avoid forwarding administrator tokens.
    ///
    /// # Errors
    /// * Returns `Transport` if the TLS/HTTP client cannot be constructed.
    pub fn new() -> Result<Self, ProviderError> {
        let client = transport_builder()
            .build()
            .map_err(|_| ProviderError::Transport)?;
        Ok(Self { client })
    }

    /// Dispatch a GitHub workflow once, with an existing capacity reservation.
    ///
    /// # Errors
    /// * Returns `Policy` when reservation/eligibility is invalid or time is unavailable.
    /// * Returns `Configuration`, `Credential` or `Transport` for local validation,
    ///   credential resolution or request construction failures (before sending).
    /// * Returns `Http` for definite 400/401/403/404/422 rejection. Other HTTP,
    ///   transport and parsing failures become `Unknown`, never an automatic retry.
    pub fn dispatch_github(
        &self,
        configuration: &GitHubConfiguration,
        policy: &HostedCapacityPolicy,
        reservation: &CapacityReservation,
        dispatch: &DispatchRequest,
    ) -> Result<DispatchOutcome, ProviderError> {
        let url = github_url(configuration, &["dispatches"])?;
        authorize(policy, reservation, &github_repository(configuration))?;
        let body = github_dispatch_body(dispatch)?;
        let token = configuration.token.resolve()?;
        let request = self.github_request(Method::POST, url, &token)?.json(&body);
        self.dispatch(request, &dispatch.correlation, true)
    }

    /// Dispatch a GitLab trigger pipeline once, with an existing reservation.
    ///
    /// # Errors
    /// * Returns the same pre-send/rejection errors as `dispatch_github`.
    /// * A malformed/failed response after sending becomes `Unknown`; a GitLab
    ///   successful response must contain a positive pipeline ID.
    pub fn dispatch_gitlab(
        &self,
        configuration: &GitLabConfiguration,
        policy: &HostedCapacityPolicy,
        reservation: &CapacityReservation,
        dispatch: &DispatchRequest,
    ) -> Result<DispatchOutcome, ProviderError> {
        let url = gitlab_url(configuration, &["trigger", "pipeline"])?;
        authorize(policy, reservation, &configuration.project)?;
        let mut body = gitlab_dispatch_body(dispatch)?;
        // Provider token is only added here, never to DispatchRequest/bootstrap data.
        body["token"] = Value::String(configuration.trigger_token.resolve()?);
        let request = self.client.post(url).json(&body);
        self.dispatch(request, &dispatch.correlation, false)
    }

    /// Fetch a GitHub run by its exact positive ID in the configured repository.
    ///
    /// # Errors
    /// * Returns `Configuration` for invalid endpoint/ID; `Credential` for missing
    ///   credentials; `Transport`, `Http`, `ResponseBody` or `ResponseSchema` for
    ///   failed I/O, non-success status, oversized/unreadable body or invalid ID/JSON.
    pub fn github_status(
        &self,
        configuration: &GitHubConfiguration,
        id: u64,
    ) -> Result<HostedRun, ProviderError> {
        let url = github_run_url(configuration, id, &[])?;
        let request = self.github_request(Method::GET, url, &configuration.token.resolve()?)?;
        parse_run(self.read_json(request)?, id)
    }

    /// Fetch a GitLab pipeline by exact positive ID in the configured project.
    ///
    /// # Errors
    /// * Returns the same endpoint/credential/I/O/schema errors as `github_status`.
    pub fn gitlab_status(
        &self,
        configuration: &GitLabConfiguration,
        id: u64,
    ) -> Result<HostedRun, ProviderError> {
        let url = gitlab_run_url(configuration, id, &[])?;
        let request = self.gitlab_request(Method::GET, url, &configuration.api_token.resolve()?)?;
        parse_run(self.read_json(request)?, id)
    }

    /// Request cancellation of an exact GitHub run. Acceptance is not termination;
    /// poll status separately. No force-cancel escalation occurs automatically.
    ///
    /// # Errors
    /// * Returns `Configuration`, `Credential`, `Transport` or `Http` for invalid
    ///   configuration/ID, credential failures, failed I/O or non-success status.
    pub fn cancel_github(
        &self,
        configuration: &GitHubConfiguration,
        id: u64,
    ) -> Result<(), ProviderError> {
        let url = github_run_url(configuration, id, &["cancel"])?;
        let request = self.github_request(Method::POST, url, &configuration.token.resolve()?)?;
        self.cancel(request)
    }

    /// Request cancellation of an exact GitLab pipeline, without claiming it stopped.
    ///
    /// # Errors
    /// * Returns the same validation/credential/I/O errors as `cancel_github`.
    pub fn cancel_gitlab(
        &self,
        configuration: &GitLabConfiguration,
        id: u64,
    ) -> Result<(), ProviderError> {
        let url = gitlab_run_url(configuration, id, &["cancel"])?;
        let request =
            self.gitlab_request(Method::POST, url, &configuration.api_token.resolve()?)?;
        self.cancel(request)
    }

    /// Find a GitHub dispatch by *exact* run-name correlation, not time/branch guesses.
    /// The workflow must set `run-name: ${{ inputs.gpipe_correlation }}`. Input
    /// values are not exposed by the runs listing API. Searches at most `max_pages`
    /// pages of 100 runs; ambiguous matches and truncated searches fail closed.
    /// `None` is a snapshot only: eventual consistency does not authorize redispatch.
    ///
    /// # Errors
    /// * Returns `Configuration` for invalid config/correlation or a page bound
    ///   outside 1..=10; ordinary credential/transport/HTTP/body/schema errors apply.
    /// * Returns `ResponseSchema` for multiple matches and `IncompleteSearch` if
    ///   the page bound or GitHub's 1000-result search cap prevents completeness.
    pub fn find_github_run(
        &self,
        configuration: &GitHubConfiguration,
        correlation: &str,
        max_pages: u32,
    ) -> Result<Option<HostedRun>, ProviderError> {
        validate_correlation(correlation)?;
        if !(1..=10).contains(&max_pages) {
            return Err(ProviderError::Configuration);
        }
        let url = github_url(configuration, &["runs"])?;
        let token = configuration.token.resolve()?;
        let mut found = None;
        for page in 1..=max_pages {
            let request = self
                .github_request(Method::GET, url.clone(), &token)?
                .query(&[
                    ("event", "workflow_dispatch".to_owned()),
                    ("per_page", "100".to_owned()),
                    ("page", page.to_string()),
                ]);
            let listing: RunListing = serde_json::from_value(self.read_json(request)?)
                .map_err(|_| ProviderError::ResponseSchema)?;
            if listing.workflow_runs.len() > 100 {
                return Err(ProviderError::ResponseSchema);
            }
            accumulate_matches(&listing.workflow_runs, correlation, &mut found)?;
            let visited = u64::from(page - 1) * 100
                + u64::try_from(listing.workflow_runs.len())
                    .map_err(|_| ProviderError::ResponseSchema)?;
            if listing.total_count <= visited {
                return Ok(found);
            }
            if listing.workflow_runs.len() < 100 {
                return Err(ProviderError::IncompleteSearch);
            }
        }
        Err(ProviderError::IncompleteSearch)
    }

    /// Publish a final result to an exact GitHub source commit in the configured
    /// repository. Use a source-repository configuration, not the worker-hosting
    /// repository when they differ. The API token needs commit-status permission.
    ///
    /// # Errors
    /// * Returns `Configuration` for invalid repository/revision/context;
    ///   `Credential` for absent/invalid administrator credentials; `Transport`,
    ///   `Http`, `ResponseBody` or `ResponseSchema` for failed I/O, rejection,
    ///   oversized/unreadable body or mismatched acknowledgement. A transport or
    ///   response error can follow publication: do not blindly retry this mutation.
    pub fn publish_github_commit_status(
        &self,
        configuration: &GitHubConfiguration,
        status: &CommitStatusRequest,
    ) -> Result<PublishedCommitStatus, ProviderError> {
        let body = commit_status_body(status, true)?;
        let url = github_commit_status_url(configuration, &status.revision)?;
        let request = self
            .github_request(Method::POST, url, &configuration.token.resolve()?)?
            .json(&body);
        parse_commit_status(&self.read_json(request)?, status, true)
    }

    /// Publish a final result to an exact GitLab source commit in the configured
    /// project. Only the administrator API credential is used, never the trigger
    /// token. GitLab can associate/create an external pipeline for this status;
    /// this does not itself grant capacity or authorize runner execution.
    ///
    /// # Errors
    /// * Returns the same validation, credential, I/O and acknowledgement errors
    ///   as `publish_github_commit_status`. Unknown publication must be reconciled
    ///   by the caller rather than automatically retried.
    pub fn publish_gitlab_commit_status(
        &self,
        configuration: &GitLabConfiguration,
        status: &CommitStatusRequest,
    ) -> Result<PublishedCommitStatus, ProviderError> {
        let body = commit_status_body(status, false)?;
        let url = gitlab_commit_status_url(configuration, &status.revision)?;
        let request = self
            .gitlab_request(Method::POST, url, &configuration.api_token.resolve()?)?
            .json(&body);
        parse_commit_status(&self.read_json(request)?, status, false)
    }

    fn github_request(
        &self,
        method: Method,
        url: Url,
        token: &str,
    ) -> Result<RequestBuilder, ProviderError> {
        Ok(self
            .client
            .request(method, url)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .header(AUTHORIZATION, secret_header(&format!("Bearer {token}"))?))
    }

    fn gitlab_request(
        &self,
        method: Method,
        url: Url,
        token: &str,
    ) -> Result<RequestBuilder, ProviderError> {
        Ok(self
            .client
            .request(method, url)
            .header("PRIVATE-TOKEN", secret_header(token)?))
    }

    fn dispatch(
        &self,
        request: RequestBuilder,
        correlation: &str,
        github: bool,
    ) -> Result<DispatchOutcome, ProviderError> {
        let request = bounded_request(request)?;
        let Ok(response) = self.execute(request) else {
            return Ok(unknown(
                correlation,
                "dispatch transport failed; acceptance unknown",
            ));
        };
        let status = response.status().as_u16();
        if matches!(status, 400 | 401 | 403 | 404 | 422) {
            return Err(ProviderError::Http { status });
        }
        let Ok(body) = read_body(response) else {
            return Ok(unknown(
                correlation,
                "dispatch response unreadable or oversized",
            ));
        };
        Ok(parse_dispatch(status, &body, correlation, github))
    }

    fn read_json(&self, request: RequestBuilder) -> Result<Value, ProviderError> {
        let response = self.execute(bounded_request(request)?)?;
        if !response.status().is_success() {
            return Err(ProviderError::Http {
                status: response.status().as_u16(),
            });
        }
        serde_json::from_slice(&read_body(response)?).map_err(|_| ProviderError::ResponseSchema)
    }

    fn execute(&self, request: Request) -> Result<reqwest::blocking::Response, ProviderError> {
        if request.url().host_str() == Some("api.github.com") {
            return self
                .client
                .execute(request)
                .map_err(|_| ProviderError::Transport);
        }
        // Resolve on a bounded thread, validate every returned address, then pin
        // this request's resolver. DNS rebinding cannot switch to a private host.
        let addresses = resolve_public_addresses(request.url())?;
        let host = request
            .url()
            .host_str()
            .ok_or(ProviderError::Configuration)?;
        let client = transport_builder()
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|_| ProviderError::Transport)?;
        client
            .execute(request)
            .map_err(|_| ProviderError::Transport)
    }

    fn cancel(&self, request: RequestBuilder) -> Result<(), ProviderError> {
        let response = self.execute(bounded_request(request)?)?;
        if !response.status().is_success() {
            return Err(ProviderError::Http {
                status: response.status().as_u16(),
            });
        }
        Ok(())
    }
}

fn transport_builder() -> reqwest::blocking::ClientBuilder {
    Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent("gpipe-hosted-provider")
}

fn public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [a, b, c, _] = address.octets();
            !address.is_private()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_broadcast()
                && !address.is_documentation()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0 && c == 0)
                && !(a == 192 && b == 88 && c == 99)
                && !(a == 198 && (b == 18 || b == 19))
        }
        IpAddr::V6(address) => {
            // Only global unicast; exclude documentation and transition ranges,
            // including mapped IPv4, 6to4, Teredo and special-purpose 2001::/23.
            let segments = address.segments();
            (segments[0] & 0xe000) == 0x2000
                && segments[0] != 0x2002
                && !(segments[0] == 0x2001 && (segments[1] < 0x0200 || segments[1] == 0x0db8))
                && !(segments[0] == 0x3fff && segments[1] < 0x1000)
        }
    }
}

fn validate_public_origin(url: &Url) -> Result<(), ProviderError> {
    let host = url.host_str().ok_or(ProviderError::Configuration)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.port_or_known_default() != Some(443)
    {
        return Err(ProviderError::Configuration);
    }
    if let Ok(address) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if !public_ip(address) {
            return Err(ProviderError::Configuration);
        }
    } else {
        let host = host.trim_end_matches('.');
        if !host.contains('.')
            || [
                "localhost",
                "local",
                "internal",
                "test",
                "invalid",
                "example",
                "onion",
            ]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
        {
            return Err(ProviderError::Configuration);
        }
    }
    Ok(())
}

fn resolve_public_addresses(url: &Url) -> Result<Vec<SocketAddr>, ProviderError> {
    let host = url
        .host_str()
        .ok_or(ProviderError::Configuration)?
        .trim_matches(['[', ']'])
        .to_owned();
    if let Ok(address) = host.parse::<IpAddr>() {
        return if public_ip(address) {
            Ok(vec![SocketAddr::new(address, 443)])
        } else {
            Err(ProviderError::Configuration)
        };
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("gpipe-provider-dns".into())
        .spawn(move || {
            let result = (host.as_str(), 443)
                .to_socket_addrs()
                .map(|addresses| addresses.take(33).collect::<Vec<_>>());
            let _ = sender.send(result);
        })
        .map_err(|_| ProviderError::Transport)?;
    let addresses = receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| ProviderError::Transport)?
        .map_err(|_| ProviderError::Transport)?;
    if addresses.is_empty()
        || addresses.len() > 32
        || addresses.iter().any(|address| !public_ip(address.ip()))
    {
        return Err(ProviderError::Configuration);
    }
    Ok(addresses)
}

fn validate_commit_status(status: &CommitStatusRequest) -> Result<(), ProviderError> {
    if !matches!(status.revision.len(), 40 | 64)
        || !status.revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        || status.context.trim().is_empty()
        || status.context.len() > 100
        || status.context.chars().any(char::is_control)
    {
        return Err(ProviderError::Configuration);
    }
    Ok(())
}

const fn final_state(result: FinalPipelineResult, github: bool) -> &'static str {
    match result {
        FinalPipelineResult::Success => "success",
        FinalPipelineResult::Failure => "failure",
        FinalPipelineResult::Canceled if github => "error",
        FinalPipelineResult::Canceled => "canceled",
    }
}

fn commit_status_body(status: &CommitStatusRequest, github: bool) -> Result<Value, ProviderError> {
    validate_commit_status(status)?;
    let state = final_state(status.result, github);
    if github {
        Ok(json!({"state": state, "context": status.context}))
    } else {
        // GitLab calls a failed commit status `failed`, unlike GitHub `failure`.
        let state = if state == "failure" { "failed" } else { state };
        Ok(json!({"state": state, "name": status.context}))
    }
}

fn github_commit_status_url(
    configuration: &GitHubConfiguration,
    revision: &str,
) -> Result<Url, ProviderError> {
    identifier(&configuration.owner)?;
    identifier(&configuration.repository)?;
    append_segments(
        Url::parse("https://api.github.com/").map_err(|_| ProviderError::Configuration)?,
        &[
            "repos",
            &configuration.owner,
            &configuration.repository,
            "statuses",
            revision,
        ],
    )
}

fn gitlab_commit_status_url(
    configuration: &GitLabConfiguration,
    revision: &str,
) -> Result<Url, ProviderError> {
    gitlab_url(configuration, &["statuses", revision])
}

fn parse_commit_status(
    value: &Value,
    expected: &CommitStatusRequest,
    github: bool,
) -> Result<PublishedCommitStatus, ProviderError> {
    let id = value
        .get("id")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or(ProviderError::ResponseSchema)?;
    let expected_state = final_state(expected.result, github);
    let expected_state = if !github && expected_state == "failure" {
        "failed"
    } else {
        expected_state
    };
    if value
        .get(if github { "state" } else { "status" })
        .and_then(Value::as_str)
        != Some(expected_state)
        || value
            .get(if github { "context" } else { "name" })
            .and_then(Value::as_str)
            != Some(expected.context.as_str())
    {
        return Err(ProviderError::ResponseSchema);
    }
    // GitHub's create-status response omits the SHA; its exact request URL binds
    // the revision. GitLab includes SHA and must acknowledge exactly that commit.
    if (!github || value.get("sha").is_some())
        && !value
            .get("sha")
            .and_then(Value::as_str)
            .is_some_and(|sha| sha.eq_ignore_ascii_case(&expected.revision))
    {
        return Err(ProviderError::ResponseSchema);
    }
    Ok(PublishedCommitStatus {
        id,
        revision: expected.revision.clone(),
        state: expected_state.to_owned(),
    })
}

fn authorize(
    policy: &HostedCapacityPolicy,
    reservation: &CapacityReservation,
    repository: &str,
) -> Result<(), ProviderError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProviderError::Policy)?;
    policy.validate_reservation(reservation, repository, now.as_secs())
}

fn secret_header(value: &str) -> Result<HeaderValue, ProviderError> {
    if value.trim().is_empty() {
        return Err(ProviderError::Credential);
    }
    let mut value = HeaderValue::from_str(value).map_err(|_| ProviderError::Credential)?;
    value.set_sensitive(true);
    Ok(value)
}

fn bounded_request(builder: RequestBuilder) -> Result<Request, ProviderError> {
    let request = builder.build().map_err(|_| ProviderError::Transport)?;
    if request.body().is_some_and(|body| {
        body.as_bytes()
            .is_none_or(|bytes| bytes.len() > MAX_REQUEST_BYTES)
    }) {
        return Err(ProviderError::Configuration);
    }
    Ok(request)
}

fn read_body(response: reqwest::blocking::Response) -> Result<Vec<u8>, ProviderError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(ProviderError::ResponseBody);
    }
    bounded_body(response)
}

fn bounded_body(reader: impl Read) -> Result<Vec<u8>, ProviderError> {
    let mut body = Vec::new();
    reader
        .take(MAX_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|_| ProviderError::ResponseBody)?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(ProviderError::ResponseBody);
    }
    Ok(body)
}

fn validate_correlation(correlation: &str) -> Result<(), ProviderError> {
    if correlation.is_empty()
        || correlation.len() > 128
        || !correlation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ProviderError::Configuration);
    }
    Ok(())
}

fn validate_dispatch(dispatch: &DispatchRequest) -> Result<(), ProviderError> {
    validate_correlation(&dispatch.correlation)?;
    if dispatch.reference.trim().is_empty()
        || dispatch.reference.len() > 1024
        || dispatch.reference.chars().any(char::is_control)
        || dispatch.inputs.contains_key(CORRELATION_INPUT)
        || dispatch.inputs.contains_key(CORRELATION_VARIABLE)
        || dispatch.inputs.keys().any(|key| key.trim().is_empty())
    {
        return Err(ProviderError::Configuration);
    }
    Ok(())
}

fn github_dispatch_body(dispatch: &DispatchRequest) -> Result<Value, ProviderError> {
    validate_dispatch(dispatch)?;
    if dispatch.inputs.len() >= 25 {
        return Err(ProviderError::Configuration);
    }
    let mut inputs = dispatch.inputs.clone();
    inputs.insert(CORRELATION_INPUT.to_owned(), dispatch.correlation.clone());
    Ok(json!({"ref": dispatch.reference, "inputs": inputs}))
}

fn gitlab_dispatch_body(dispatch: &DispatchRequest) -> Result<Value, ProviderError> {
    validate_dispatch(dispatch)?;
    let mut variables = dispatch.inputs.clone();
    variables.insert(
        CORRELATION_VARIABLE.to_owned(),
        dispatch.correlation.clone(),
    );
    Ok(json!({"ref": dispatch.reference, "variables": variables}))
}

fn github_repository(configuration: &GitHubConfiguration) -> String {
    format!("{}/{}", configuration.owner, configuration.repository)
}

fn identifier(value: &str) -> Result<(), ProviderError> {
    if value.is_empty()
        || value.len() > 255
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ProviderError::Configuration);
    }
    Ok(())
}

fn github_root(configuration: &GitHubConfiguration) -> Result<Url, ProviderError> {
    identifier(&configuration.owner)?;
    identifier(&configuration.repository)?;
    identifier(&configuration.workflow)?;
    let url = Url::parse("https://api.github.com/").map_err(|_| ProviderError::Configuration)?;
    append_segments(
        url,
        &[
            "repos",
            &configuration.owner,
            &configuration.repository,
            "actions",
        ],
    )
}

fn github_url(configuration: &GitHubConfiguration, suffix: &[&str]) -> Result<Url, ProviderError> {
    let root = append_segments(
        github_root(configuration)?,
        &["workflows", &configuration.workflow],
    )?;
    append_segments(root, suffix)
}

fn github_run_url(
    configuration: &GitHubConfiguration,
    id: u64,
    suffix: &[&str],
) -> Result<Url, ProviderError> {
    positive_id(id)?;
    let root = append_segments(github_root(configuration)?, &["runs", &id.to_string()])?;
    append_segments(root, suffix)
}

fn gitlab_url(configuration: &GitLabConfiguration, suffix: &[&str]) -> Result<Url, ProviderError> {
    let url = Url::parse(&configuration.origin).map_err(|_| ProviderError::Configuration)?;
    validate_public_origin(&url)?;
    if configuration.project.len() > 1024 {
        return Err(ProviderError::Configuration);
    }
    for segment in configuration.project.split('/') {
        identifier(segment)?;
    }
    let root = append_segments(url, &["api", "v4", "projects", &configuration.project])?;
    append_segments(root, suffix)
}

fn gitlab_run_url(
    configuration: &GitLabConfiguration,
    id: u64,
    suffix: &[&str],
) -> Result<Url, ProviderError> {
    positive_id(id)?;
    let root = gitlab_url(configuration, &["pipelines", &id.to_string()])?;
    append_segments(root, suffix)
}

fn append_segments(mut url: Url, segments: &[&str]) -> Result<Url, ProviderError> {
    url.path_segments_mut()
        .map_err(|()| ProviderError::Configuration)?
        .pop_if_empty()
        .extend(segments);
    Ok(url)
}

const fn positive_id(id: u64) -> Result<(), ProviderError> {
    if id == 0 {
        return Err(ProviderError::Configuration);
    }
    Ok(())
}

fn unknown(correlation: &str, reason: &str) -> DispatchOutcome {
    DispatchOutcome::Unknown {
        correlation: correlation.to_owned(),
        reason: reason.to_owned(),
    }
}

fn parse_dispatch(status: u16, body: &[u8], correlation: &str, github: bool) -> DispatchOutcome {
    if github && status == 204 && body.is_empty() {
        return DispatchOutcome::AcceptedWithoutId {
            correlation: correlation.to_owned(),
        };
    }
    if (200..300).contains(&status)
        && let Ok(value) = serde_json::from_slice::<Value>(body)
    {
        // API versions have used different top-level run identity keys.
        let ids = ["id", "run_id", "workflow_run_id"]
            .into_iter()
            .filter(|key| github || *key == "id")
            .filter_map(|key| value.get(key))
            .collect::<Vec<_>>();
        if let Some(id) = ids.first().and_then(|value| value.as_u64())
            && id > 0
            && ids.iter().all(|value| value.as_u64() == Some(id))
        {
            return DispatchOutcome::Started { id };
        }
    }
    unknown(
        correlation,
        "dispatch response did not establish a unique run identity",
    )
}

#[derive(Deserialize)]
struct RunRecord {
    id: u64,
    status: String,
    #[serde(default)]
    conclusion: Option<String>,
}

fn parse_run(value: Value, expected_id: u64) -> Result<HostedRun, ProviderError> {
    let record: RunRecord =
        serde_json::from_value(value).map_err(|_| ProviderError::ResponseSchema)?;
    if record.id != expected_id || record.id == 0 || record.status.trim().is_empty() {
        return Err(ProviderError::ResponseSchema);
    }
    Ok(HostedRun {
        id: record.id,
        status: record.status,
        conclusion: record.conclusion,
    })
}

#[derive(Deserialize)]
struct RunListing {
    total_count: u64,
    workflow_runs: Vec<ListedRun>,
}

#[derive(Deserialize)]
struct ListedRun {
    id: u64,
    status: String,
    #[serde(default)]
    conclusion: Option<String>,
    display_title: String,
    event: String,
}

fn accumulate_matches(
    runs: &[ListedRun],
    correlation: &str,
    found: &mut Option<HostedRun>,
) -> Result<(), ProviderError> {
    for run in runs {
        if run.display_title == correlation && run.event == "workflow_dispatch" {
            if found.is_some() {
                return Err(ProviderError::ResponseSchema);
            }
            *found = Some(run.as_hosted()?);
        }
    }
    Ok(())
}

impl ListedRun {
    fn as_hosted(&self) -> Result<HostedRun, ProviderError> {
        if self.id == 0 || self.status.trim().is_empty() {
            return Err(ProviderError::ResponseSchema);
        }
        Ok(HostedRun {
            id: self.id,
            status: self.status.clone(),
            conclusion: self.conclusion.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github() -> GitHubConfiguration {
        GitHubConfiguration {
            owner: "example".into(),
            repository: "software".into(),
            workflow: "worker.yml".into(),
            token: CredentialEnvironment {
                variable: "GITHUB_ADMIN_TOKEN".into(),
            },
        }
    }

    fn gitlab(origin: &str) -> GitLabConfiguration {
        GitLabConfiguration {
            origin: origin.into(),
            project: "group/subgroup/software".into(),
            trigger_token: CredentialEnvironment {
                variable: "GITLAB_TRIGGER_TOKEN".into(),
            },
            api_token: CredentialEnvironment {
                variable: "GITLAB_API_TOKEN".into(),
            },
        }
    }

    fn dispatch() -> DispatchRequest {
        DispatchRequest {
            correlation: "reservation-123".into(),
            reference: "main".into(),
            inputs: BTreeMap::from([("plan_digest".into(), "abc".into())]),
        }
    }

    #[test]
    fn reservation_deadlines_and_capacity_edges() {
        let mut policy = HostedCapacityPolicy {
            free_eligibility_confirmed: true,
            max_active_jobs: 2,
            deadline_unix_seconds: 200,
            repository_scope: "example/software".into(),
        };
        let mut reservation = CapacityReservation {
            reservation_id: "r".into(),
            repository: "example/software".into(),
            reserved_jobs: 2,
            active_jobs_including_reservation: 2,
            deadline_unix_seconds: 200,
        };
        assert!(
            policy
                .validate_reservation(&reservation, "example/software", 100)
                .is_ok()
        );
        reservation.deadline_unix_seconds = 201;
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
        reservation.deadline_unix_seconds = 200;
        reservation.reserved_jobs = 0;
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
        reservation.reserved_jobs = 2;
        reservation.active_jobs_including_reservation = 1;
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
        reservation.active_jobs_including_reservation = 2;
        policy.max_active_jobs = 0;
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
        policy.max_active_jobs = 2;
        policy.deadline_unix_seconds = 100;
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
    }

    #[test]
    fn commit_status_requests_and_acknowledgements_are_revision_bound() {
        let provider = HostedProviders::new().unwrap();
        let status = CommitStatusRequest {
            revision: "a".repeat(40),
            context: "gpipe/verified".into(),
            result: FinalPipelineResult::Failure,
        };
        let github_body = commit_status_body(&status, true).unwrap();
        assert_eq!(
            github_body,
            json!({"state":"failure", "context":"gpipe/verified"})
        );
        let gitlab_body = commit_status_body(&status, false).unwrap();
        assert_eq!(
            gitlab_body,
            json!({"state":"failed", "name":"gpipe/verified"})
        );
        let github_request = bounded_request(
            provider
                .github_request(
                    Method::POST,
                    github_commit_status_url(&github(), &status.revision).unwrap(),
                    "offline-token",
                )
                .unwrap()
                .json(&github_body),
        )
        .unwrap();
        assert_eq!(
            github_request.url().path(),
            format!("/repos/example/software/statuses/{}", status.revision)
        );
        assert_eq!(github_request.method(), Method::POST);
        assert!(github_request.headers()[AUTHORIZATION].is_sensitive());
        let gitlab_request = bounded_request(
            provider
                .gitlab_request(
                    Method::POST,
                    gitlab_commit_status_url(&gitlab("https://gitlab.com"), &status.revision)
                        .unwrap(),
                    "offline-admin-token",
                )
                .unwrap()
                .json(&gitlab_body),
        )
        .unwrap();
        assert_eq!(
            gitlab_request.url().path(),
            format!(
                "/api/v4/projects/group%2Fsubgroup%2Fsoftware/statuses/{}",
                status.revision
            )
        );
        assert!(gitlab_request.headers()["PRIVATE-TOKEN"].is_sensitive());
        assert!(
            !String::from_utf8_lossy(gitlab_request.body().unwrap().as_bytes().unwrap())
                .contains("offline-admin-token")
        );
        assert!(
            parse_commit_status(
                &json!({"id":1,"state":"failure","context":status.context}),
                &status,
                true
            )
            .is_ok()
        );
        assert!(
            parse_commit_status(
                &json!({"id":2,"status":"failed","name":status.context,"sha":status.revision}),
                &status,
                false
            )
            .is_ok()
        );
        for response in [
            json!({"id":2,"status":"failed","name":status.context,"sha":"b".repeat(40)}),
            json!({"id":2,"status":"success","name":status.context,"sha":status.revision}),
            json!({"id":2,"status":"failed","name":"different","sha":status.revision}),
            json!({"id":0,"status":"failed","name":status.context,"sha":status.revision}),
            json!({"id":2,"status":"failed","name":status.context}),
        ] {
            assert!(parse_commit_status(&response, &status, false).is_err());
        }
    }

    #[test]
    fn final_result_mapping_and_revision_validation() {
        let mut status = CommitStatusRequest {
            revision: "a".repeat(40),
            context: "gpipe/verified".into(),
            result: FinalPipelineResult::Canceled,
        };
        assert_eq!(commit_status_body(&status, true).unwrap()["state"], "error");
        assert_eq!(
            commit_status_body(&status, false).unwrap()["state"],
            "canceled"
        );
        status.result = FinalPipelineResult::Success;
        assert_eq!(
            commit_status_body(&status, true).unwrap()["state"],
            "success"
        );
        for revision in [
            "main".into(),
            "../other".into(),
            "g".repeat(40),
            "a".repeat(39),
        ] {
            status.revision = revision;
            assert!(commit_status_body(&status, true).is_err());
        }
    }

    #[test]
    fn gitlab_is_public_https_only_without_network() {
        for origin in [
            "http://gitlab.com",
            "https://localhost",
            "https://127.0.0.1",
            "https://10.0.0.1",
            "https://172.16.0.1",
            "https://192.168.0.1",
            "https://169.254.169.254",
            "https://100.64.0.1",
            "https://192.0.2.1",
            "https://[::1]",
            "https://[::ffff:127.0.0.1]",
            "https://[fc00::1]",
            "https://[2001:db8::1]",
            "https://service.internal",
            "https://gitlab.com:8443",
        ] {
            assert!(
                gitlab_url(&gitlab(origin), &["trigger", "pipeline"]).is_err(),
                "{origin}"
            );
        }
        assert!(gitlab_url(&gitlab("https://gitlab.com"), &["trigger", "pipeline"]).is_ok());
        for address in [
            "127.0.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "::ffff:8.8.8.8",
            "2002:808:808::",
        ] {
            assert!(!public_ip(address.parse().unwrap()));
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn reservation_fails_closed() {
        let mut policy = HostedCapacityPolicy {
            free_eligibility_confirmed: false,
            max_active_jobs: 2,
            deadline_unix_seconds: 200,
            repository_scope: "example/software".into(),
        };
        let mut reservation = CapacityReservation {
            reservation_id: "reservation-123".into(),
            repository: "example/software".into(),
            reserved_jobs: 1,
            active_jobs_including_reservation: 2,
            deadline_unix_seconds: 150,
        };
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
        policy.free_eligibility_confirmed = true;
        assert!(
            policy
                .validate_reservation(&reservation, "example/software", 100)
                .is_ok()
        );
        for repository in ["other/software", "example/other"] {
            assert_eq!(
                policy.validate_reservation(&reservation, repository, 100),
                Err(ProviderError::Policy)
            );
        }
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 150),
            Err(ProviderError::Policy)
        );
        reservation.active_jobs_including_reservation = 3;
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
        reservation.active_jobs_including_reservation = 0;
        assert_eq!(
            policy.validate_reservation(&reservation, "example/software", 100),
            Err(ProviderError::Policy)
        );
    }

    #[test]
    fn endpoints_are_strict_and_project_paths_are_encoded() {
        assert_eq!(
            github_url(&github(), &["dispatches"]).unwrap().as_str(),
            "https://api.github.com/repos/example/software/actions/workflows/worker.yml/dispatches"
        );
        assert_eq!(
            gitlab_run_url(&gitlab("https://gitlab.com"), 42, &["cancel"])
                .unwrap()
                .as_str(),
            "https://gitlab.com/api/v4/projects/group%2Fsubgroup%2Fsoftware/pipelines/42/cancel"
        );
        for origin in [
            "http://gitlab.com",
            "https://token@gitlab.com",
            "https://gitlab.com/api",
            "https://gitlab.com/?token=x",
            "https://gitlab.com/#x",
        ] {
            assert!(gitlab_url(&gitlab(origin), &[]).is_err());
        }
        let mut configuration = github();
        for owner in ["..", "evil/x", "evil?x", "https://evil.example"] {
            configuration.owner = owner.into();
            assert!(github_url(&configuration, &[]).is_err());
        }
        assert!(github_run_url(&github(), 0, &[]).is_err());
    }

    #[test]
    fn correlation_is_reserved_and_not_a_secret() {
        let request = dispatch();
        let github = github_dispatch_body(&request).unwrap();
        assert_eq!(github["inputs"][CORRELATION_INPUT], request.correlation);
        let gitlab = gitlab_dispatch_body(&request).unwrap();
        assert_eq!(
            gitlab["variables"][CORRELATION_VARIABLE],
            request.correlation
        );
        assert!(gitlab.get("token").is_none());
        assert!(github.get("token").is_none());
        for key in [CORRELATION_INPUT, CORRELATION_VARIABLE] {
            let mut request = dispatch();
            request.inputs.insert(key.into(), "spoof".into());
            assert!(github_dispatch_body(&request).is_err());
            assert!(gitlab_dispatch_body(&request).is_err());
        }
        for correlation in ["", "a b", "a\nb", "a/b"] {
            assert!(validate_correlation(correlation).is_err());
        }
    }

    #[test]
    fn dispatch_parsing_never_invents_success_or_retries() {
        assert_eq!(
            parse_dispatch(204, b"", "c", true),
            DispatchOutcome::AcceptedWithoutId {
                correlation: "c".into()
            }
        );
        for body in [
            br#"{"id":42}"#.as_slice(),
            br#"{"workflow_run_id":42}"#,
            br#"{"run_id":42}"#,
        ] {
            assert_eq!(
                parse_dispatch(200, body, "c", true),
                DispatchOutcome::Started { id: 42 }
            );
        }
        assert_eq!(
            parse_dispatch(201, br#"{"id":42}"#, "c", false),
            DispatchOutcome::Started { id: 42 }
        );
        for (status, body, github) in [
            (500, b"secret".as_slice(), true),
            (302, b"", true),
            (200, b"{}", true),
            (200, b"{", true),
            (204, b"", false),
            (200, br#"{"id":0}"#, true),
            (200, br#"{"id":1,"run_id":2}"#, true),
        ] {
            let outcome = parse_dispatch(status, body, "stable-correlation", github);
            assert!(
                matches!(outcome, DispatchOutcome::Unknown { ref correlation, .. } if correlation == "stable-correlation")
            );
            assert!(!format!("{outcome:?}").contains("secret"));
        }
    }

    #[test]
    fn discovery_matches_exact_correlation_and_rejects_ambiguity() {
        let listing: RunListing = serde_json::from_value(json!({
            "total_count": 3,
            "workflow_runs": [
                {"id":1,"status":"completed","display_title":"reservation-123-prefix","event":"workflow_dispatch"},
                {"id":2,"status":"running","display_title":"reservation-123","event":"push"},
                {"id":3,"status":"queued","display_title":"reservation-123","event":"workflow_dispatch"}
            ]
        })).unwrap();
        let mut found = None;
        accumulate_matches(&listing.workflow_runs, "reservation-123", &mut found).unwrap();
        assert_eq!(found.as_ref().unwrap().id, 3);
        assert_eq!(
            accumulate_matches(&listing.workflow_runs, "reservation-123", &mut found),
            Err(ProviderError::ResponseSchema)
        );
        let mut absent = None;
        accumulate_matches(&listing.workflow_runs, "different", &mut absent).unwrap();
        assert!(absent.is_none());
    }

    #[test]
    fn exact_status_identity_and_body_bounds() {
        assert!(parse_run(json!({"id":42,"status":"running"}), 42).is_ok());
        assert!(parse_run(json!({"id":43,"status":"running"}), 42).is_err());
        assert!(parse_run(json!({"id":42,"status":""}), 42).is_err());
        assert!(bounded_body(&vec![0; MAX_RESPONSE_BYTES][..]).is_ok());
        assert!(bounded_body(&vec![0; MAX_RESPONSE_BYTES + 1][..]).is_err());
    }

    #[test]
    fn offline_request_building_keeps_tokens_out_of_bootstrap_and_urls() {
        let provider = HostedProviders::new().unwrap();
        let body = github_dispatch_body(&dispatch()).unwrap();
        let request = bounded_request(
            provider
                .github_request(
                    Method::POST,
                    github_url(&github(), &["dispatches"]).unwrap(),
                    "test-only-secret",
                )
                .unwrap()
                .json(&body),
        )
        .unwrap();
        assert!(!request.url().as_str().contains("test-only-secret"));
        assert!(request.headers()[AUTHORIZATION].is_sensitive());
        assert!(
            !String::from_utf8_lossy(request.body().unwrap().as_bytes().unwrap())
                .contains("test-only-secret")
        );
        let gitlab_request = bounded_request(
            provider
                .gitlab_request(
                    Method::POST,
                    gitlab_run_url(&gitlab("https://gitlab.com"), 42, &["cancel"]).unwrap(),
                    "test-only-api-secret",
                )
                .unwrap(),
        )
        .unwrap();
        assert!(gitlab_request.headers()["PRIVATE-TOKEN"].is_sensitive());
        assert!(
            !gitlab_request
                .url()
                .as_str()
                .contains("test-only-api-secret")
        );
        assert_eq!(gitlab_request.method(), Method::POST);
        assert!(secret_header("bad\nvalue").is_err());
        let large = provider
            .client
            .post("https://api.github.com/")
            .body(vec![0; MAX_REQUEST_BYTES + 1]);
        assert!(matches!(
            bounded_request(large),
            Err(ProviderError::Configuration)
        ));
        // Building does not send any of these requests, resolve environment secrets,
        // or require network/DNS access.
    }
}
