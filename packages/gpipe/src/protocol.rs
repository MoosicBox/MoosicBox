//! Versioned coordinator/worker wire contract.

use crate::plan::{Plan, Task};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Supported wire protocol version. Incompatible clients fail closed.
pub const PROTOCOL_VERSION: u32 = 1;

/// Configuration for a single coordinator. Secrets are referenced by environment name.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Listen address; use loopback behind an authenticated TLS reverse proxy remotely.
    pub listen: String,
    /// SQLite database path.
    pub database: String,
    /// Environment variable containing the administrator bearer token.
    pub admin_token_env: String,
    /// Repository authorization policy.
    pub repositories: BTreeMap<String, RepositoryPolicy>,
    /// Worker token identities and placement permissions.
    pub workers: BTreeMap<String, WorkerPolicy>,
}

/// Repository-specific, revision-bound execution policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryPolicy {
    /// Exact commit hashes approved to execute.
    pub revisions: BTreeSet<String>,
}

/// Worker permissions are configured by the coordinator, not self-asserted.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerPolicy {
    /// Environment variable containing this worker's unique bearer token.
    pub token_env: String,
    /// Allowed repository identifiers.
    pub repositories: BTreeSet<String>,
    /// Trusted placement labels, such as os and architecture.
    pub labels: BTreeMap<String, String>,
    /// Maximum admitted CPU units.
    pub cpu: u32,
    /// Maximum admitted memory in MiB.
    pub memory_mb: u64,
}

/// Native worker configuration. Repository sources are administrator-controlled.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerConfig {
    /// Coordinator URL. Remote usage requires HTTPS.
    pub coordinator: String,
    /// Bearer token environment variable.
    pub token_env: String,
    /// Directory for isolated workspaces and durable pending completion records.
    pub directory: String,
    /// Allowed repository ID to local source checkout path mapping.
    pub repositories: BTreeMap<String, String>,
    /// Approved worker-local environment variables (for example HOME and `RUSTUP_HOME`).
    /// Never put provider or coordinator credentials here: commands receive these values.
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    /// Poll interval when no work is available.
    pub poll_secs: u64,
}

/// Role derived from an authenticated token, never supplied by the request body.
#[derive(Clone, Debug)]
pub enum Identity {
    /// Administrator with run management rights.
    Admin,
    /// Worker with only assignment-scoped rights.
    Worker(String),
}

/// Request envelope; every message includes an explicit protocol version.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    /// Protocol version.
    pub version: u32,
    /// Typed operation.
    pub request: Request,
}

/// Operations on durable orchestration state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum Request {
    /// Submit an immutable plan with a caller-owned idempotency key.
    Submit {
        /// Submission identity.
        key: String,
        /// Resolved plan.
        plan: Plan,
        /// Keep graph open for explicit expansion until sealed.
        dynamic: bool,
    },
    /// Inspect all runs without task logs.
    List,
    /// Inspect a run, including task attempts and results.
    Inspect {
        /// Run identity.
        run_id: String,
    },
    /// Add tasks atomically; exact replays are idempotent.
    Expand {
        /// Run identity.
        run_id: String,
        /// New tasks.
        tasks: BTreeMap<String, Task>,
    },
    /// Prevent further graph expansion and allow successful finalization.
    Seal {
        /// Run identity.
        run_id: String,
    },
    /// Cancel pending work and request termination of running attempts.
    Cancel {
        /// Run identity.
        run_id: String,
    },
    /// Approve one exact revision for a configured repository.
    Approve {
        /// Repository identity.
        repository: String,
        /// Immutable revision.
        revision: String,
    },
    /// Authorize artifact transfer for an active assignment; producer must be a successful direct dependency.
    ArtifactAccess {
        /// Consumer/producer attempt performing the transfer.
        attempt_id: String,
        /// None for publishing own artifacts; Some for reading a dependency.
        producer_task: Option<String>,
    },
    /// Claim one eligible ready task.
    Claim,
    /// Renew an active attempt; cancellation is returned explicitly.
    Heartbeat {
        /// Attempt identity.
        attempt_id: String,
    },
    /// Atomically complete an authorized generator and publish its downstream graph.
    CompleteGenerated {
        /// Attempt identity.
        attempt_id: String,
        /// Successful terminal evidence.
        report: Report,
        /// New tasks, each depending on the generator task.
        tasks: BTreeMap<String, Task>,
        /// Close further expansion after this batch.
        seal: bool,
    },
    /// Deliver a terminal result. Duplicate identical delivery is accepted.
    Complete {
        /// Attempt identity.
        attempt_id: String,
        /// Terminal evidence.
        report: Report,
    },
}

/// One task assignment with a fenced attempt identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Assignment {
    /// Attempt identity.
    pub attempt_id: String,
    /// Run identity.
    pub run_id: String,
    /// Task identity.
    pub task_id: String,
    /// Allowed source repository identity.
    pub repository: String,
    /// Exact source commit.
    pub revision: String,
    /// Resolved task.
    pub task: Task,
    /// Lease deadline in epoch seconds.
    pub lease_until: u64,
}

/// Bounded terminal evidence. Large artifacts use a separate data plane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// Whether all steps succeeded.
    pub success: bool,
    /// Bounded execution details or infrastructure error.
    pub log: String,
}

/// Coordinator response. Errors never imply that a mutation should be blindly retried.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Response {
    /// Operation settled successfully.
    Ok,
    /// A submitted run's identity.
    Submitted {
        /// Run identity.
        run_id: String,
    },
    /// JSON inspection data.
    Data {
        /// State snapshot.
        value: serde_json::Value,
    },
    /// A ready assignment, or no eligible work.
    Assignment {
        /// Optional admitted task.
        assignment: Option<Assignment>,
    },
    /// Lease renewal result.
    Heartbeat {
        /// Worker must terminate the process tree when true.
        cancel: bool,
        /// New lease deadline.
        lease_until: u64,
    },
    /// A completion is permanently fenced; workers retain evidence but may move on.
    Fenced {
        /// Exact rejected attempt identity.
        attempt_id: String,
    },
    /// Operation was rejected.
    Error {
        /// Secret-safe reason.
        message: String,
    },
}
