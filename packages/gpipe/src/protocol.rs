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
    /// Optional administrator-approved envelope for worker-generated tasks.
    /// Omission retains the generator's restrictive placement/resource ceilings.
    #[serde(default)]
    pub generation: Option<GenerationPolicy>,
}

/// Admission ceilings for generated work, not operating-system resource enforcement.
///
/// Every allowed label key must be present with an allowed value; extra keys are
/// rejected. This permits cross-platform generation without inheriting planner labels.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationPolicy {
    /// Required placement keys and their authorized values.
    pub allowed_labels: BTreeMap<String, BTreeSet<String>>,
    /// Maximum CPU units per generated task.
    pub max_cpu: u32,
    /// Maximum memory MiB per generated task.
    pub max_memory_mb: u64,
    /// Maximum attempts per generated task.
    pub max_attempts: u32,
    /// Maximum cumulative generated tasks in a run, across all generators/batches.
    pub max_tasks: u32,
}

/// Persisted administrator overlay on configured worker authorization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkerMode {
    /// Admit work under the configured worker policy.
    #[default]
    Active,
    /// Stop new admissions, but allow existing attempts to finish.
    Draining,
    /// Stop admissions and request cancellation of active attempts.
    Revoked,
}

/// Worker permissions are configured by the coordinator, not self-asserted.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerPolicy {
    /// Environment variable containing this worker's unique bearer token.
    pub token_env: String,
    /// Allowed repository identifiers.
    pub repositories: BTreeSet<String>,
    /// Trusted exact-match placement labels. Distinguish execution environment
    /// (for example `runner-os=ubuntu`) from physical host (`host-os=nixos`);
    /// neither is inferred or substituted by the coordinator.
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
    /// Register/renew a worker-created incarnation ID (1..=256 nonblank bytes,
    /// no control characters). Returns Heartbeat with the registration deadline;
    /// cancel is false: poll attempt cancellation separately with `HeartbeatFor`.
    /// A different instance cannot take over until both registration and active
    /// attempt leases expire. Opt-in is permanent: unbound operations fail closed.
    RegisterWorker {
        /// Unique incarnation identity; retain with pending completion evidence.
        instance: String,
    },
    /// Claim or replay the one active assignment for this registered instance.
    /// Requires a live registration; never adopts another incarnation's attempt.
    ClaimFor {
        /// Exact registered incarnation identity.
        instance: String,
    },
    /// Renew an exact-instance attempt lease and poll cancellation.
    /// Requires a live registration. Cancellation never extends the attempt lease.
    HeartbeatFor {
        /// Exact registered incarnation identity.
        instance: String,
        /// Attempt identity.
        attempt_id: String,
    },
    /// Deliver exact-instance terminal evidence; identical retries are accepted.
    /// May acknowledge/replay after registration expiry, but never after another
    /// instance registers. Expired/cancelled attempts return Fenced.
    CompleteFor {
        /// Exact registered incarnation identity.
        instance: String,
        /// Attempt identity.
        attempt_id: String,
        /// Terminal evidence.
        report: Report,
    },
    /// Atomically complete a generator and publish its authorized task batch.
    /// Registration-expiry and replay rules are the same as `CompleteFor`.
    CompleteGeneratedFor {
        /// Exact registered incarnation identity.
        instance: String,
        /// Attempt identity.
        attempt_id: String,
        /// Successful terminal evidence.
        report: Report,
        /// New tasks directly depending on the generator.
        tasks: BTreeMap<String, Task>,
        /// Close further expansion after this batch.
        seal: bool,
    },
    /// Authorize artifact transfer for an exact-instance live assignment.
    /// Requires a live registration as well as the normal artifact access checks.
    ArtifactAccessFor {
        /// Exact registered incarnation identity.
        instance: String,
        /// Consumer attempt identity.
        attempt_id: String,
        /// Successful direct dependency task, or None to publish own artifacts.
        producer_task: Option<String>,
    },
    /// Persist a worker admission overlay. Revocation requests cancellation but
    /// retains reservations until exact-instance acknowledgement or lease expiry.
    SetWorkerMode {
        /// Configured worker identity.
        worker: String,
        /// New admission mode; reactivation never clears sticky cancellations.
        mode: WorkerMode,
    },
    /// Administrator-only read authorization for one exact completed attempt.
    /// Failed reports are allowed for diagnostics, even if a retry is pending.
    /// Returns `Data {producer_attempt_id, run_id, task_id}`; this grants only
    /// download access, never artifact publication or mutation. The data plane
    /// must bind GET/manifest scope to these exact identities separately.
    InspectArtifact {
        /// Exact retained run identity.
        run_id: String,
        /// Exact task identity within that run.
        task_id: String,
        /// Exact completed attempt belonging to that run and task.
        attempt_id: String,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_policy_defaults_and_strict_generation_configuration() {
        let policy: RepositoryPolicy = serde_json::from_value(serde_json::json!({
            "revisions": ["a".repeat(40)]
        }))
        .unwrap();
        assert!(policy.generation.is_none());
        assert!(
            serde_json::from_value::<RepositoryPolicy>(serde_json::json!({
                "revisions": [], "generation": {
                    "allowed_labels": {"os": ["macos", "windows"]},
                    "max_cpu": 4, "max_memory_mb": 512, "max_attempts": 2,
                    "max_tasks": 20, "privileged": true
                }
            }))
            .is_err()
        );
    }

    #[test]
    fn instance_operations_round_trip_and_reject_unknown_fields() {
        let operation = Request::HeartbeatFor {
            instance: "boot-unique".into(),
            attempt_id: "attempt-1".into(),
        };
        let value = serde_json::to_value(&operation).unwrap();
        assert_eq!(value["type"], "HEARTBEAT_FOR");
        assert_eq!(value["attempt_id"], "attempt-1");
        let decoded: Request = serde_json::from_value(value).unwrap();
        assert!(
            matches!(decoded, Request::HeartbeatFor { instance, .. } if instance == "boot-unique")
        );
        assert!(
            serde_json::from_value::<Request>(serde_json::json!({
                "type": "REGISTER_WORKER", "instance": "a", "worker": "another"
            }))
            .is_err()
        );
        let legacy: Request = serde_json::from_value(serde_json::json!({"type": "CLAIM"})).unwrap();
        assert!(matches!(legacy, Request::Claim));
    }
}
