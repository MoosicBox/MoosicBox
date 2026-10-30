//! Durable, transactionally fenced pull-worker coordination.
//!
//! Every accepted request reads and replaces a versioned SQLite state row under
//! an immediate transaction. Multiple coordinator handles therefore share ID,
//! lease, resource, and idempotency decisions without an in-memory cache.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, path::Path, time::Duration};

use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::{
    plan::{Plan, Task, TaskWhen},
    protocol::{
        Assignment, Envelope, Identity, PROTOCOL_VERSION, Report, Request, Response, ServerConfig,
        WorkerMode,
    },
};

/// Lease duration in caller-supplied epoch seconds. Expiry is inclusive.
pub const LEASE_SECS: u64 = 60;
/// Maximum UTF-8 bytes retained in one completion log.
pub const MAX_REPORT_BYTES: usize = 1_048_576;
/// Maximum number of newest-first run summaries returned by `List`.
pub const LIST_LIMIT: usize = 100;
/// Maximum serialized JSON bytes in one Inspect payload; larger runs return an error.
pub const MAX_INSPECT_BYTES: usize = 2 * 1_048_576;
/// Maximum serialized durable JSON bytes; mutations exceeding it roll back.
pub const MAX_STATE_BYTES: usize = 64 * 1_048_576;
/// Maximum retained runs, including their original submission keys.
pub const MAX_RUNS: usize = 1_024;
/// Maximum retained attempts across all runs, including expired attempts.
pub const MAX_ATTEMPTS: usize = 8_192;
/// Maximum persisted administrator revision approvals across all repositories.
pub const MAX_APPROVALS: usize = 4_096;
/// Maximum serialized generated task batch. Reserved twice (plan and replay).
pub const MAX_GENERATED_BYTES: usize = 2 * 1_048_576;
const REPORT_RESERVATION: usize = 6 * MAX_REPORT_BYTES + 65_536;
const SCHEMA_VERSION: u32 = 2;
const MAX_KEY_BYTES: usize = 256;

/// SQLite-backed coordinator using administrator-supplied placement policy.
///
/// Authentication is performed by the transport; callers must never construct
/// `Identity` from unverified request data. Time must be a trusted epoch clock.
pub struct Coordinator {
    connection: Connection,
    config: ServerConfig,
    /// Fixed production ceiling; tests lower it to exercise full admission cheaply.
    state_limit: usize,
}

/// Version-two durable state. Version one is migrated transactionally on open.
/// Counters never reuse identities, including after restart. Older coordinators
/// reject version two rather than silently ignoring instance fences or overlays.
#[derive(Default, Serialize, Deserialize)]
struct State {
    next_run: u64,
    next_attempt: u64,
    runs: BTreeMap<String, Run>,
    submissions: BTreeMap<String, Submission>,
    approvals: BTreeMap<String, std::collections::BTreeSet<String>>,
    #[serde(default)]
    workers: BTreeMap<String, WorkerRegistration>,
    #[serde(default)]
    worker_modes: BTreeMap<String, WorkerMode>,
}

#[derive(Serialize, Deserialize)]
struct WorkerRegistration {
    instance_id: String,
    lease_until: u64,
}

/// Original submission, kept separately because dynamic expansion changes the plan.
#[derive(Serialize, Deserialize)]
struct Submission {
    plan: Plan,
    dynamic: bool,
    run_id: String,
}

/// Persisted run inspection, including every fenced attempt and current graph.
#[derive(Serialize, Deserialize)]
struct Run {
    plan: Plan,
    dynamic: bool,
    sealed: bool,
    status: RunStatus,
    tasks: BTreeMap<String, TaskStatus>,
    attempts: BTreeMap<String, Attempt>,
}

/// Aggregate lifecycle; an open graph never becomes successful.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RunStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// Task lifecycle; blocked tasks cannot receive attempts.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TaskStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}

/// A retained lease and terminal report; expired attempts can never complete.
#[derive(Serialize, Deserialize)]
struct Attempt {
    task_id: String,
    worker: String,
    #[serde(default)]
    instance_id: Option<String>,
    lease_until: u64,
    status: AttemptStatus,
    report: Option<Report>,
    /// Sticky policy revocation, retaining capacity until acknowledgement/expiry.
    #[serde(default)]
    cancel_requested: bool,
    /// Exact generated payload retained for replay, absent on pre-extension rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generated: Option<GeneratedCompletion>,
}

/// Generator completion identity excludes report, which is retained on the attempt.
#[derive(PartialEq, Eq, Serialize, Deserialize)]
struct GeneratedCompletion {
    tasks: BTreeMap<String, Task>,
    seal: bool,
}

/// Attempt fencing state. Cancellation keeps an active reservation until settled.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum AttemptStatus {
    Active,
    Completed,
    Expired,
}

impl Coordinator {
    /// Open or initialize a durable coordinator database.
    ///
    /// Version-one state is upgraded atomically to version two. This is a one-way
    /// migration: older coordinators must not serve this database afterward.
    ///
    /// # Errors
    /// * SQLite cannot open, lock, initialize, or read the database.
    /// * The stored schema version or JSON state is unsupported or corrupt.
    pub fn open(path: &Path, config: ServerConfig) -> Result<Self, String> {
        let mut connection = Connection::open(path).map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db_error)?;
        connection
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
            .map_err(db_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS gpipe_state (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL, json TEXT NOT NULL);").map_err(db_error)?;
        let initial = serde_json::to_string(&State::default()).map_err(json_error)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO gpipe_state(id, version, json) VALUES (1, ?1, ?2)",
                params![SCHEMA_VERSION, initial],
            )
            .map_err(db_error)?;
        // New fields have serde defaults. Publish the schema fence atomically
        // with validation, so old handles cannot silently ignore durable policy.
        transaction
            .execute(
                "UPDATE gpipe_state SET version = ?1 WHERE id = 1 AND version = 1",
                [SCHEMA_VERSION],
            )
            .map_err(db_error)?;
        let _ = load(&transaction)?;
        transaction.commit().map_err(db_error)?;
        Ok(Self {
            connection,
            config,
            state_limit: MAX_STATE_BYTES,
        })
    }

    /// Execute one authenticated request atomically, including lease expiration.
    ///
    /// Failures return `Response::Error` and roll back the whole transaction.
    /// Worker identities may only claim and settle their own attempts. Logs are
    /// rejected above 1 MiB, rather than silently truncated. List returns at most
    /// 100 summaries. Inspect returns the complete run or an explicit 2 MiB
    /// limit error. State is capped at 64 MiB, 1,024 runs, 8,192 attempts,
    /// and 4,096 persisted approvals. Limit errors roll back, including completion
    /// reports from already admitted attempts are budgeted; history is never pruned.
    /// Repeated Claims return the worker's existing active assignment without
    /// extending its lease. Legacy Claim replays an already cancelled assignment
    /// solely for cancellation polling/acknowledgement; `ClaimFor` returns none.
    /// Policy-revoked assignments return no assignment and
    /// request cancellation on Heartbeat, retaining the reservation until ack/expiry.
    /// Generated completion atomically publishes only new downstream tasks, with
    /// the generator's required labels and no higher CPU/memory/retry ceilings.
    /// Children cannot redelegate generation via `GPIPE_TASKS_OUTPUT`. The exact
    /// report/tasks/seal tuple is retained for retries even after sealing.
    /// Owned expired or cancelled completion attempts return `Response::Fenced`;
    /// cancellation acknowledgements release active reservations without accepting
    /// their evidence. Unknown/foreign attempts and conflicting completed replays
    /// remain errors. Heartbeat fencing continues to use the existing error form.
    /// Active leases reserve worst-case JSON-escaped report space (6 MiB plus
    /// 64 KiB metadata). Generators additionally reserve three times the 2 MiB
    /// generated batch limit. All mutations preserve these reservations; capacity
    /// exhaustion fails admission, not valid terminal delivery. Repository,
    /// revision, placement and resource revocation is sticky and requests worker
    /// cancellation without releasing the active reservation until ack/expiry.
    /// Artifact access returns only the owning active assignment's publication
    /// identity or a successful completed direct dependency's identity within the
    /// same run. Expired, cancelled and policy-revoked leases cannot transfer files.
    #[must_use]
    pub fn handle(&mut self, identity: &Identity, envelope: Envelope, now: u64) -> Response {
        match self.execute(identity, envelope, now) {
            Ok(response) => response,
            Err(message) => Response::Error { message },
        }
    }

    fn execute(
        &mut self,
        identity: &Identity,
        envelope: Envelope,
        now: u64,
    ) -> Result<Response, String> {
        if envelope.version != PROTOCOL_VERSION {
            return Err("unsupported protocol version".into());
        }
        authorize(identity, &envelope.request)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        let mut state = load(&transaction)?;
        expire(&mut state, now);
        revalidate_active(&mut state, &self.config);
        let response =
            dispatch_worker_request(&mut state, &self.config, identity, envelope.request, now)?;
        check_state_limits(&state)?;
        let reserved = completion_reservations(&state)?;
        let json = bounded_json(
            &state,
            self.state_limit.saturating_sub(reserved),
            "durable state including completion reservations",
        )?;
        transaction
            .execute("UPDATE gpipe_state SET json = ?1 WHERE id = 1", [json])
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(response)
    }
}

// `Result::map_err` transfers ownership to these error-to-string adapters.
#[allow(clippy::needless_pass_by_value)]
fn db_error(error: rusqlite::Error) -> String {
    format!("coordinator database: {error}")
}
// `Result::map_err` transfers ownership to this error-to-string adapter.
#[allow(clippy::needless_pass_by_value)]
fn json_error(error: serde_json::Error) -> String {
    format!("coordinator state: {error}")
}

fn load(connection: &Connection) -> Result<State, String> {
    let bytes: u64 = connection
        .query_row(
            "SELECT length(CAST(json AS BLOB)) FROM gpipe_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if bytes > MAX_STATE_BYTES as u64 {
        return Err("durable state exceeds byte limit".into());
    }
    let (version, json): (u32, String) = connection
        .query_row(
            "SELECT version, json FROM gpipe_state WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(db_error)?;
    if version != SCHEMA_VERSION {
        return Err("unsupported coordinator state schema".into());
    }
    let state = serde_json::from_str(&json).map_err(json_error)?;
    check_state_limits(&state)?;
    Ok(state)
}

/// Serializer sink that fails before retaining more than the configured bytes.
struct LimitedJson {
    bytes: Vec<u8>,
    limit: usize,
}

impl std::io::Write for LimitedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("JSON byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn bounded_json(value: &impl Serialize, limit: usize, scope: &str) -> Result<String, String> {
    let mut output = LimitedJson {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut output, value).map_err(|error| {
        format!("{scope}: serialization failed or exceeds {limit} bytes: {error}")
    })?;
    String::from_utf8(output.bytes).map_err(|_| format!("{scope}: invalid JSON encoding"))
}

fn check_state_limits(state: &State) -> Result<(), String> {
    if state.runs.len() > MAX_RUNS || state.submissions.len() > MAX_RUNS {
        return Err(format!(
            "durable state exceeds {MAX_RUNS} retained runs/submissions"
        ));
    }
    let attempts: usize = state.runs.values().map(|run| run.attempts.len()).sum();
    if attempts > MAX_ATTEMPTS {
        return Err(format!(
            "durable state exceeds {MAX_ATTEMPTS} retained attempts"
        ));
    }
    let approvals: usize = state
        .approvals
        .values()
        .map(std::collections::BTreeSet::len)
        .sum();
    if approvals > MAX_APPROVALS {
        return Err(format!(
            "durable state exceeds {MAX_APPROVALS} persisted approvals"
        ));
    }
    Ok(())
}

fn authorize(identity: &Identity, request: &Request) -> Result<(), String> {
    match (identity, request) {
        (
            Identity::Admin,
            Request::RegisterWorker { .. }
            | Request::ClaimFor { .. }
            | Request::HeartbeatFor { .. }
            | Request::CompleteFor { .. }
            | Request::CompleteGeneratedFor { .. }
            | Request::ArtifactAccessFor { .. }
            | Request::Claim
            | Request::Heartbeat { .. }
            | Request::Complete { .. }
            | Request::ArtifactAccess { .. }
            | Request::CompleteGenerated { .. },
        ) => Err("worker identity required".into()),
        (
            Identity::Worker(_),
            Request::RegisterWorker { .. }
            | Request::ClaimFor { .. }
            | Request::HeartbeatFor { .. }
            | Request::CompleteFor { .. }
            | Request::CompleteGeneratedFor { .. }
            | Request::ArtifactAccessFor { .. }
            | Request::Claim
            | Request::Heartbeat { .. }
            | Request::Complete { .. }
            | Request::ArtifactAccess { .. }
            | Request::CompleteGenerated { .. },
        )
        | (Identity::Admin, _) => Ok(()),
        (Identity::Worker(_), _) => Err("operation not authorized".into()),
    }
}

fn next_id(counter: &mut u64, prefix: &str) -> Result<String, String> {
    *counter = counter.checked_add(1).ok_or("identity space exhausted")?;
    Ok(format!("{prefix}{counter:020}"))
}

fn run_mut<'a>(state: &'a mut State, id: &str) -> Result<&'a mut Run, String> {
    state.runs.get_mut(id).ok_or_else(|| "unknown run".into())
}

enum WorkerRequest {
    Register(String),
    Bound {
        instance_id: String,
        request: Request,
    },
    Legacy(Request),
}

impl From<Request> for WorkerRequest {
    fn from(request: Request) -> Self {
        match request {
            Request::RegisterWorker { instance } => Self::Register(instance),
            Request::ClaimFor { instance } => Self::Bound {
                instance_id: instance,
                request: Request::Claim,
            },
            Request::HeartbeatFor {
                instance,
                attempt_id,
            } => Self::Bound {
                instance_id: instance,
                request: Request::Heartbeat { attempt_id },
            },
            Request::CompleteFor {
                instance,
                attempt_id,
                report,
            } => Self::Bound {
                instance_id: instance,
                request: Request::Complete { attempt_id, report },
            },
            Request::CompleteGeneratedFor {
                instance,
                attempt_id,
                report,
                tasks,
                seal,
            } => Self::Bound {
                instance_id: instance,
                request: Request::CompleteGenerated {
                    attempt_id,
                    report,
                    tasks,
                    seal,
                },
            },
            Request::ArtifactAccessFor {
                instance,
                attempt_id,
                producer_task,
            } => Self::Bound {
                instance_id: instance,
                request: Request::ArtifactAccess {
                    attempt_id,
                    producer_task,
                },
            },
            request => Self::Legacy(request),
        }
    }
}

/// Session checks happen before dispatch, so legacy credentials cannot operate on
/// an opted-in identity's attempts, including idempotent terminal replays.
#[allow(clippy::too_many_lines)]
fn dispatch_worker_request(
    state: &mut State,
    config: &ServerConfig,
    identity: &Identity,
    request: Request,
    now: u64,
) -> Result<Response, String> {
    let Identity::Worker(worker) = identity else {
        return dispatch(state, config, identity, request, now);
    };
    match WorkerRequest::from(request) {
        WorkerRequest::Register(instance_id) => {
            if !config.workers.contains_key(worker) {
                return Err("unknown worker".into());
            }
            if instance_id.trim().is_empty()
                || instance_id.len() > MAX_KEY_BYTES
                || instance_id.chars().any(char::is_control)
            {
                return Err("invalid worker instance identity".into());
            }
            let same = state
                .workers
                .get(worker)
                .is_some_and(|registration| registration.instance_id == instance_id);
            if !same
                && (state
                    .workers
                    .get(worker)
                    .is_some_and(|registration| registration.lease_until > now)
                    || state.runs.values().any(|run| {
                        run.attempts.values().any(|attempt| {
                            attempt.worker == *worker && attempt.status == AttemptStatus::Active
                        })
                    }))
            {
                return Err("worker instance or attempt lease is still live".into());
            }
            let lease_until = now.checked_add(LEASE_SECS).ok_or("lease clock overflow")?;
            let lease_until = if same {
                lease_until.max(state.workers[worker].lease_until)
            } else {
                lease_until
            };
            state.workers.insert(
                worker.clone(),
                WorkerRegistration {
                    instance_id,
                    lease_until,
                },
            );
            Ok(Response::Heartbeat {
                cancel: false,
                lease_until,
            })
        }
        WorkerRequest::Bound {
            instance_id,
            request,
        } => {
            let registration = state
                .workers
                .get(worker)
                .ok_or("worker registration required")?;
            if registration.instance_id != instance_id {
                return Err("stale worker instance".into());
            }
            // A terminal acknowledgement is allowed after registration expiry;
            // it cannot revive a lease or mutate another instance's attempt.
            if matches!(
                request,
                Request::Claim | Request::Heartbeat { .. } | Request::ArtifactAccess { .. }
            ) && registration.lease_until <= now
            {
                return Err("worker registration expired".into());
            }
            let attempt_id = match &request {
                Request::Heartbeat { attempt_id }
                | Request::Complete { attempt_id, .. }
                | Request::CompleteGenerated { attempt_id, .. }
                | Request::ArtifactAccess { attempt_id, .. } => Some(attempt_id),
                _ => None,
            };
            if let Some(id) = attempt_id {
                let (run, _) = find_attempt(state, identity, id)?;
                if run.attempts[id].instance_id.as_deref() != Some(instance_id.as_str()) {
                    return Err("attempt belongs to another worker instance".into());
                }
            }
            // Bound clients must not receive cancelled work as a fresh claim.
            // Legacy Claim retains its historical same-attempt replay solely
            // for cancellation polling/acknowledgement, never new admission.
            if matches!(request, Request::Claim)
                && state.runs.values().any(|run| {
                    run.status == RunStatus::Cancelled
                        && run.attempts.values().any(|attempt| {
                            attempt.worker == *worker && attempt.status == AttemptStatus::Active
                        })
                })
            {
                return Ok(Response::Assignment { assignment: None });
            }
            let response = dispatch(state, config, identity, request, now)?;
            // Renew only a live registration; completion replay must not resurrect
            // an expired process incarnation.
            let registration = state
                .workers
                .get_mut(worker)
                .ok_or("worker registration required")?;
            if registration.lease_until > now {
                registration.lease_until = registration
                    .lease_until
                    .max(now.checked_add(LEASE_SECS).ok_or("lease clock overflow")?);
            }
            Ok(response)
        }
        WorkerRequest::Legacy(request) => {
            if state.workers.contains_key(worker) {
                return Err("instance-bound worker operation required".into());
            }
            dispatch(state, config, identity, request, now)
        }
    }
}

#[allow(clippy::too_many_lines)]
fn dispatch(
    state: &mut State,
    config: &ServerConfig,
    identity: &Identity,
    request: Request,
    now: u64,
) -> Result<Response, String> {
    match request {
        Request::RegisterWorker { .. }
        | Request::ClaimFor { .. }
        | Request::HeartbeatFor { .. }
        | Request::CompleteFor { .. }
        | Request::CompleteGeneratedFor { .. }
        | Request::ArtifactAccessFor { .. } => Err("worker session dispatch required".into()),
        Request::InspectArtifact {
            run_id,
            task_id,
            attempt_id,
        } => {
            let run = state.runs.get(&run_id).ok_or("unknown run")?;
            let attempt = run
                .attempts
                .get(&attempt_id)
                .ok_or("unknown attempt in run")?;
            if attempt.task_id != task_id || !run.plan.tasks.contains_key(&task_id) {
                return Err("attempt does not belong to requested task".into());
            }
            if attempt.status != AttemptStatus::Completed || attempt.report.is_none() {
                return Err(
                    "artifact inspection requires a completed attempt with retained report".into(),
                );
            }
            Ok(Response::Data {
                value: serde_json::json!({
                    "producer_attempt_id": attempt_id, "run_id": run_id, "task_id": task_id,
                }),
            })
        }
        Request::SetWorkerMode { worker, mode } => {
            if !config.workers.contains_key(&worker) {
                return Err("unknown worker".into());
            }
            state.worker_modes.insert(worker, mode);
            revalidate_active(state, config);
            Ok(Response::Ok)
        }
        Request::Submit { key, plan, dynamic } => {
            if key.trim().is_empty() || key.len() > MAX_KEY_BYTES {
                return Err("submission key must contain 1..=256 bytes and be nonblank".into());
            }
            if let Some(previous) = state.submissions.get(&key) {
                if previous.plan != plan || previous.dynamic != dynamic {
                    return Err(
                        "submission key conflicts with original plan or dynamic flag".into(),
                    );
                }
                return Ok(Response::Submitted {
                    run_id: previous.run_id.clone(),
                });
            }
            plan.validate()?;
            if !config.repositories.contains_key(&plan.repository) {
                return Err("repository is not configured".into());
            }
            let run_id = next_id(&mut state.next_run, "run-")?;
            let tasks = plan
                .tasks
                .keys()
                .map(|id| (id.clone(), TaskStatus::Pending))
                .collect();
            state.runs.insert(
                run_id.clone(),
                Run {
                    plan: plan.clone(),
                    dynamic,
                    sealed: !dynamic,
                    status: RunStatus::Running,
                    tasks,
                    attempts: BTreeMap::new(),
                },
            );
            state.submissions.insert(
                key,
                Submission {
                    plan,
                    dynamic,
                    run_id: run_id.clone(),
                },
            );
            Ok(Response::Submitted { run_id })
        }
        Request::List => {
            let values: Vec<_> = state.runs.iter().rev().take(LIST_LIMIT).map(|(id, run)| serde_json::json!({"run_id": id, "repository": run.plan.repository, "revision": run.plan.revision, "status": run.status, "dynamic": run.dynamic, "sealed": run.sealed, "task_count": run.tasks.len(), "attempt_count": run.attempts.len()})).collect();
            Ok(Response::Data {
                value: serde_json::json!(values),
            })
        }
        Request::Inspect { run_id } => {
            let run = run_mut(state, &run_id)?;
            Ok(Response::Data {
                value: serde_json::from_str(&bounded_json(run, MAX_INSPECT_BYTES, "Inspect")?)
                    .map_err(json_error)?,
            })
        }
        Request::Expand { run_id, tasks } => {
            let run = run_mut(state, &run_id)?;
            if !run.dynamic || run.sealed || run.status != RunStatus::Running {
                return Err("run is not an open dynamic graph".into());
            }
            run.plan.expand(tasks)?;
            for id in run.plan.tasks.keys() {
                run.tasks.entry(id.clone()).or_insert(TaskStatus::Pending);
            }
            refresh(run);
            Ok(Response::Ok)
        }
        Request::Seal { run_id } => {
            let run = run_mut(state, &run_id)?;
            run.sealed = true;
            refresh(run);
            Ok(Response::Ok)
        }
        Request::Cancel { run_id } => {
            let run = run_mut(state, &run_id)?;
            if run.status == RunStatus::Running {
                run.status = RunStatus::Cancelled;
                for status in run.tasks.values_mut() {
                    if matches!(status, TaskStatus::Pending | TaskStatus::Running) {
                        *status = TaskStatus::Cancelled;
                    }
                }
            }
            Ok(Response::Ok)
        }
        Request::Approve {
            repository,
            revision,
        } => {
            if !config.repositories.contains_key(&repository) {
                return Err("repository is not configured".into());
            }
            if !matches!(revision.len(), 40 | 64)
                || !revision.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("approval requires an exact commit digest".into());
            }
            state
                .approvals
                .entry(repository)
                .or_default()
                .insert(revision);
            Ok(Response::Ok)
        }
        Request::ArtifactAccess {
            attempt_id,
            producer_task,
        } => artifact_access(state, identity, &attempt_id, producer_task.as_deref()),
        Request::Claim => claim(state, config, identity, now),
        Request::Heartbeat { attempt_id } => {
            let (run, task) = find_attempt(state, identity, &attempt_id)?;
            let attempt = run.attempts.get_mut(&attempt_id).ok_or("unknown attempt")?;
            if attempt.status != AttemptStatus::Active {
                return Err("stale attempt".into());
            }
            // Cancellation must not indefinitely extend a resource reservation.
            let cancel = run.status == RunStatus::Cancelled
                || run.tasks[&task] == TaskStatus::Cancelled
                || attempt.cancel_requested;
            if !cancel {
                attempt.lease_until = attempt
                    .lease_until
                    .max(now.checked_add(LEASE_SECS).ok_or("lease clock overflow")?);
            }
            Ok(Response::Heartbeat {
                cancel,
                lease_until: attempt.lease_until,
            })
        }
        Request::CompleteGenerated {
            attempt_id,
            report,
            tasks,
            seal,
        } => complete_generated(state, config, identity, &attempt_id, report, tasks, seal),
        Request::Complete { attempt_id, report } => {
            let (run, task) = find_attempt(state, identity, &attempt_id)?;
            let attempt = run.attempts.get_mut(&attempt_id).ok_or("unknown attempt")?;
            if attempt.status == AttemptStatus::Completed {
                return if attempt.generated.is_none() && attempt.report.as_ref() == Some(&report) {
                    Ok(Response::Ok)
                } else {
                    Err("completion conflicts with retained report".into())
                };
            }
            if attempt.status != AttemptStatus::Active
                || run.status == RunStatus::Cancelled
                || attempt.cancel_requested
            {
                // This owned cancellation acknowledgement releases its reservation;
                // no report is accepted. Subsequent retries remain fenced.
                attempt.status = AttemptStatus::Expired;
                settle_revocation(run, &task);
                return Ok(Response::Fenced { attempt_id });
            }
            if report.log.len() > MAX_REPORT_BYTES {
                return Err("report log exceeds 1 MiB".into());
            }
            attempt.status = AttemptStatus::Completed;
            let success = report.success;
            attempt.report = Some(report);
            if run.status != RunStatus::Cancelled {
                let status = if success {
                    TaskStatus::Succeeded
                } else if has_attempts_remaining(run, &task) {
                    TaskStatus::Pending
                } else {
                    TaskStatus::Failed
                };
                run.tasks.insert(task, status);
                refresh(run);
            }
            Ok(Response::Ok)
        }
    }
}

#[allow(clippy::too_many_lines)]
fn complete_generated(
    state: &mut State,
    config: &ServerConfig,
    identity: &Identity,
    attempt_id: &str,
    report: Report,
    tasks: BTreeMap<String, Task>,
    seal: bool,
) -> Result<Response, String> {
    let (run, generator_id) = find_attempt(state, identity, attempt_id)?;
    let attempt = &run.attempts[attempt_id];
    let generated = GeneratedCompletion { tasks, seal };
    // Replay precedes open/sealed checks: successful sealed completions must be
    // recoverable after response loss and restart, without applying them twice.
    if attempt.status == AttemptStatus::Completed {
        return if attempt.report.as_ref() == Some(&report)
            && attempt.generated.as_ref() == Some(&generated)
        {
            Ok(Response::Ok)
        } else {
            Err("completion conflicts with retained generated payload".into())
        };
    }
    if attempt.status != AttemptStatus::Active
        || run.status == RunStatus::Cancelled
        || attempt.cancel_requested
    {
        run.attempts
            .get_mut(attempt_id)
            .ok_or("unknown attempt")?
            .status = AttemptStatus::Expired;
        settle_revocation(run, &generator_id);
        return Ok(Response::Fenced {
            attempt_id: attempt_id.into(),
        });
    }
    if report.log.len() > MAX_REPORT_BYTES {
        return Err("report log exceeds 1 MiB".into());
    }
    if !report.success {
        return Err("generated completion requires a successful report".into());
    }
    if !run.dynamic || run.sealed || run.status != RunStatus::Running {
        return Err("run is not an open dynamic graph".into());
    }
    let generator = &run.plan.tasks[&generator_id];
    if !generator.steps.iter().any(|step| {
        step.env
            .get("GPIPE_TASKS_OUTPUT")
            .is_some_and(|path| path == "gpipe.tasks.json")
    }) {
        return Err("attempt is not an authorized task generator".into());
    }
    let generation = config
        .repositories
        .get(&run.plan.repository)
        .and_then(|policy| policy.generation.as_ref());
    if let Some(policy) = generation {
        let previous: usize = run
            .attempts
            .values()
            .filter_map(|attempt| attempt.generated.as_ref())
            .map(|completion| completion.tasks.len())
            .sum();
        if previous.saturating_add(generated.tasks.len())
            > usize::try_from(policy.max_tasks).unwrap_or(usize::MAX)
        {
            return Err("generated tasks exceed repository task-count policy".into());
        }
    }
    for (id, task) in &generated.tasks {
        if run.plan.tasks.contains_key(id) {
            return Err("generated tasks must use new task identities".into());
        }
        if !task.needs.contains(&generator_id) {
            return Err("generated tasks must directly depend on their generator".into());
        }
        // The envelope is coordinator configuration, never generator output.
        let allowed = generation.map_or_else(
            || {
                generator
                    .labels
                    .iter()
                    .all(|(key, value)| task.labels.get(key) == Some(value))
                    && task.cpu <= generator.cpu
                    && task.memory_mb <= generator.memory_mb
                    && task.max_attempts <= generator.max_attempts
            },
            |policy| {
                task.labels.len() == policy.allowed_labels.len()
                    && policy.allowed_labels.iter().all(|(key, values)| {
                        task.labels
                            .get(key)
                            .is_some_and(|value| values.contains(value))
                    })
                    && task.cpu <= policy.max_cpu
                    && task.memory_mb <= policy.max_memory_mb
                    && task.max_attempts <= policy.max_attempts
            },
        );
        if !allowed
            || task
                .steps
                .iter()
                .any(|step| step.env.contains_key("GPIPE_TASKS_OUTPUT"))
        {
            return Err(
                "generated task exceeds placement/resource/retry policy or delegates generation"
                    .into(),
            );
        }
    }
    // expand validates the entire bounded candidate before changing the plan.
    // The enclosing SQLite transaction also rolls back on serialization limits.
    let _ = bounded_json(&generated.tasks, MAX_GENERATED_BYTES, "generated tasks")?;
    run.plan.expand(generated.tasks.clone())?;
    for id in generated.tasks.keys() {
        run.tasks.insert(id.clone(), TaskStatus::Pending);
    }
    let attempt = run.attempts.get_mut(attempt_id).ok_or("unknown attempt")?;
    attempt.status = AttemptStatus::Completed;
    attempt.report = Some(report);
    run.sealed = generated.seal;
    attempt.generated = Some(generated);
    run.tasks.insert(generator_id, TaskStatus::Succeeded);
    refresh(run);
    Ok(Response::Ok)
}

fn artifact_access(
    state: &mut State,
    identity: &Identity,
    attempt_id: &str,
    producer_task: Option<&str>,
) -> Result<Response, String> {
    // Ownership is checked before looking up dependencies or disclosing IDs.
    let (run, consumer_task) = find_attempt(state, identity, attempt_id)?;
    let attempt = &run.attempts[attempt_id];
    if attempt.status != AttemptStatus::Active
        || attempt.cancel_requested
        || run.status != RunStatus::Running
        || run.tasks[&consumer_task] != TaskStatus::Running
    {
        return Err("artifact access requires an active, authorized assignment".into());
    }
    let (producer_attempt_id, task_id) = if let Some(producer) = producer_task {
        if !run.plan.tasks[&consumer_task]
            .needs
            .iter()
            .any(|id| id == producer)
            || run.tasks.get(producer) != Some(&TaskStatus::Succeeded)
        {
            return Err("artifact producer must be a successful direct dependency".into());
        }
        let (id, _) = run
            .attempts
            .iter()
            .rev()
            .find(|(_, attempt)| {
                attempt.task_id == producer
                    && attempt.status == AttemptStatus::Completed
                    && attempt.report.as_ref().is_some_and(|report| report.success)
            })
            .ok_or("artifact producer has no successful completed attempt")?;
        (id.clone(), producer.to_owned())
    } else {
        (attempt_id.to_owned(), consumer_task)
    };
    // Run IDs are not duplicated on attempts; recover only the already authorized
    // run by its globally unique attempt identity, never from request metadata.
    let run_id = state
        .runs
        .iter()
        .find(|(_, run)| run.attempts.contains_key(attempt_id))
        .map(|(id, _)| id)
        .ok_or("unknown attempt")?;
    Ok(Response::Data {
        value: serde_json::json!({
            "producer_attempt_id": producer_attempt_id, "run_id": run_id, "task_id": task_id,
        }),
    })
}

fn find_attempt<'a>(
    state: &'a mut State,
    identity: &Identity,
    id: &str,
) -> Result<(&'a mut Run, String), String> {
    let Identity::Worker(worker) = identity else {
        return Err("worker identity required".into());
    };
    for run in state.runs.values_mut() {
        if let Some(attempt) = run.attempts.get(id) {
            if &attempt.worker != worker {
                return Err("attempt belongs to another worker".into());
            }
            let task = attempt.task_id.clone();
            return Ok((run, task));
        }
    }
    Err("unknown attempt".into())
}

fn expire(state: &mut State, now: u64) {
    for run in state.runs.values_mut() {
        let mut expired = Vec::new();
        for attempt in run.attempts.values_mut() {
            if attempt.status == AttemptStatus::Active && attempt.lease_until <= now {
                attempt.status = AttemptStatus::Expired;
                expired.push(attempt.task_id.clone());
            }
        }
        if run.status != RunStatus::Cancelled {
            for task in expired {
                let remaining = has_attempts_remaining(run, &task);
                run.tasks.insert(
                    task.clone(),
                    if remaining {
                        TaskStatus::Pending
                    } else {
                        TaskStatus::Failed
                    },
                );
            }
            refresh(run);
        }
    }
}

fn generator_task(task: &Task) -> bool {
    task.steps.iter().any(|step| {
        step.env
            .get("GPIPE_TASKS_OUTPUT")
            .is_some_and(|path| path == "gpipe.tasks.json")
    })
}

fn completion_reservations(state: &State) -> Result<usize, String> {
    let mut reserved = 0_usize;
    for run in state.runs.values() {
        for attempt in run
            .attempts
            .values()
            .filter(|attempt| attempt.status == AttemptStatus::Active)
        {
            let budget = REPORT_RESERVATION
                + if generator_task(&run.plan.tasks[&attempt.task_id]) {
                    // Batch appears in plan and replay identity, plus task-status keys.
                    3 * MAX_GENERATED_BYTES
                } else {
                    0
                };
            reserved = reserved
                .checked_add(budget)
                .ok_or("completion reservation overflow")?;
        }
    }
    if reserved > MAX_STATE_BYTES {
        return Err("completion reservations exhaust state capacity".into());
    }
    Ok(reserved)
}

fn revalidate_active(state: &mut State, config: &ServerConfig) {
    for run in state.runs.values_mut() {
        let approved = config
            .repositories
            .get(&run.plan.repository)
            .is_some_and(|policy| {
                policy.revisions.contains(&run.plan.revision)
                    || state
                        .approvals
                        .get(&run.plan.repository)
                        .is_some_and(|revisions| revisions.contains(&run.plan.revision))
            });
        for attempt in run
            .attempts
            .values_mut()
            .filter(|attempt| attempt.status == AttemptStatus::Active)
        {
            let task = &run.plan.tasks[&attempt.task_id];
            let allowed = approved
                && state.worker_modes.get(&attempt.worker) != Some(&WorkerMode::Revoked)
                && config.workers.get(&attempt.worker).is_some_and(|policy| {
                    policy.repositories.contains(&run.plan.repository)
                        && task
                            .labels
                            .iter()
                            .all(|(key, value)| policy.labels.get(key) == Some(value))
                        && task.cpu <= policy.cpu
                        && task.memory_mb <= policy.memory_mb
                });
            attempt.cancel_requested |= !allowed;
        }
    }
}

fn settle_revocation(run: &mut Run, task: &str) {
    if run.status != RunStatus::Cancelled
        && run.tasks[task] == TaskStatus::Running
        && !run
            .attempts
            .values()
            .any(|attempt| attempt.task_id == task && attempt.status == AttemptStatus::Active)
    {
        let status = if has_attempts_remaining(run, task) {
            TaskStatus::Pending
        } else {
            TaskStatus::Failed
        };
        run.tasks.insert(task.into(), status);
        refresh(run);
    }
}

fn has_attempts_remaining(run: &Run, task: &str) -> bool {
    let count = run
        .attempts
        .values()
        .filter(|attempt| attempt.task_id == task)
        .count();
    u64::try_from(count).is_ok_and(|count| count < u64::from(run.plan.tasks[task].max_attempts))
}

fn refresh(run: &mut Run) {
    if run.status == RunStatus::Cancelled {
        return;
    }
    loop {
        let blocked: Vec<_> = run
            .tasks
            .iter()
            .filter(|(id, status)| {
                **status == TaskStatus::Pending
                    && run.plan.tasks[*id].when == TaskWhen::Success
                    && run.plan.tasks[*id].needs.iter().any(|dep| {
                        matches!(
                            run.tasks[dep],
                            TaskStatus::Failed | TaskStatus::Blocked | TaskStatus::Cancelled
                        )
                    })
            })
            .map(|(id, _)| id.clone())
            .collect();
        if blocked.is_empty() {
            break;
        }
        for id in blocked {
            run.tasks.insert(id, TaskStatus::Blocked);
        }
    }
    let settled = run
        .tasks
        .values()
        .all(|s| !matches!(s, TaskStatus::Pending | TaskStatus::Running));
    if settled {
        if run.tasks.values().any(|s| *s != TaskStatus::Succeeded) {
            run.status = RunStatus::Failed;
        } else if run.sealed {
            run.status = RunStatus::Succeeded;
        }
    }
}

fn claim(
    state: &mut State,
    config: &ServerConfig,
    identity: &Identity,
    now: u64,
) -> Result<Response, String> {
    let Identity::Worker(worker) = identity else {
        return Err("worker identity required".into());
    };
    // A sequential worker may have lost the Claim response. Replay the exact
    // durable assignment, including its original deadline, before considering
    // new work. Legacy cancellation replay is only the existing reservation,
    // never new admission. Bound ClaimFor suppresses cancelled replay above.
    // Policy-revoked reservations always return no assignment.
    for (run_id, run) in &state.runs {
        if let Some((attempt_id, attempt)) = run.attempts.iter().find(|(_, attempt)| {
            attempt.worker == *worker && attempt.status == AttemptStatus::Active
        }) {
            if attempt.cancel_requested {
                return Ok(Response::Assignment { assignment: None });
            }
            return Ok(Response::Assignment {
                assignment: Some(Assignment {
                    attempt_id: attempt_id.clone(),
                    run_id: run_id.clone(),
                    task_id: attempt.task_id.clone(),
                    repository: run.plan.repository.clone(),
                    revision: run.plan.revision.clone(),
                    task: run.plan.tasks[&attempt.task_id].clone(),
                    lease_until: attempt.lease_until,
                }),
            });
        }
    }
    if state
        .worker_modes
        .get(worker)
        .is_some_and(|mode| *mode != WorkerMode::Active)
    {
        return Ok(Response::Assignment { assignment: None });
    }
    let policy = config.workers.get(worker).ok_or("unknown worker")?;
    for (run_id, run) in &mut state.runs {
        if run.status != RunStatus::Running || !policy.repositories.contains(&run.plan.repository) {
            continue;
        }
        // Trust is checked against exact repository/revision before any reservation.
        let approved = config
            .repositories
            .get(&run.plan.repository)
            .is_some_and(|repo| {
                repo.revisions.contains(&run.plan.revision)
                    || state
                        .approvals
                        .get(&run.plan.repository)
                        .is_some_and(|revisions| revisions.contains(&run.plan.revision))
            });
        if !approved {
            continue;
        }
        for (task_id, task) in &run.plan.tasks {
            if run.tasks[task_id] != TaskStatus::Pending
                || !task.needs.iter().all(|dep| match task.when {
                    TaskWhen::Success => run.tasks[dep] == TaskStatus::Succeeded,
                    TaskWhen::Always => {
                        !matches!(run.tasks[dep], TaskStatus::Pending | TaskStatus::Running)
                    }
                })
                || !task
                    .labels
                    .iter()
                    .all(|(key, value)| policy.labels.get(key) == Some(value))
                || task.cpu > policy.cpu
                || task.memory_mb > policy.memory_mb
            {
                continue;
            }
            let lease_until = now.checked_add(LEASE_SECS).ok_or("lease clock overflow")?;
            let attempt_id = next_id(&mut state.next_attempt, "attempt-")?;
            run.attempts.insert(
                attempt_id.clone(),
                Attempt {
                    task_id: task_id.clone(),
                    worker: worker.clone(),
                    instance_id: state
                        .workers
                        .get(worker)
                        .map(|registration| registration.instance_id.clone()),
                    lease_until,
                    status: AttemptStatus::Active,
                    report: None,
                    cancel_requested: false,
                    generated: None,
                },
            );
            run.tasks.insert(task_id.clone(), TaskStatus::Running);
            return Ok(Response::Assignment {
                assignment: Some(Assignment {
                    attempt_id,
                    run_id: run_id.clone(),
                    task_id: task_id.clone(),
                    repository: run.plan.repository.clone(),
                    revision: run.plan.revision.clone(),
                    task: task.clone(),
                    lease_until,
                }),
            });
        }
    }
    Ok(Response::Assignment { assignment: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        plan::{CommandStep, Task},
        protocol::{RepositoryPolicy, WorkerPolicy},
    };
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn config() -> ServerConfig {
        let worker = WorkerPolicy {
            token_env: "UNREAD_WORKER_SECRET".into(),
            repositories: ["repo".into()].into(),
            labels: BTreeMap::from([("os".into(), "linux".into())]),
            cpu: 2,
            memory_mb: 128,
        };
        ServerConfig {
            listen: "127.0.0.1:0".into(),
            database: String::new(),
            admin_token_env: "UNREAD_ADMIN_SECRET".into(),
            repositories: BTreeMap::from([(
                "repo".into(),
                RepositoryPolicy {
                    generation: None,
                    revisions: ["a".repeat(40)].into(),
                },
            )]),
            workers: BTreeMap::from([("one".into(), worker.clone()), ("two".into(), worker)]),
        }
    }

    fn task() -> Task {
        Task {
            when: TaskWhen::Success,
            needs: vec![],
            steps: vec![CommandStep {
                shell: "sh".into(),
                run: "true".into(),
                env: BTreeMap::new(),
                timeout_secs: 10,
            }],
            labels: BTreeMap::new(),
            cpu: 1,
            memory_mb: 64,
            max_attempts: 2,
        }
    }

    fn plan() -> Plan {
        Plan {
            repository: "repo".into(),
            revision: "a".repeat(40),
            tasks: BTreeMap::from([("build".into(), task())]),
        }
    }

    fn memory() -> Coordinator {
        Coordinator::open(Path::new(":memory:"), config()).unwrap()
    }
    fn worker() -> Identity {
        Identity::Worker("one".into())
    }
    fn call(c: &mut Coordinator, identity: &Identity, request: Request, now: u64) -> Response {
        c.handle(
            identity,
            Envelope {
                version: PROTOCOL_VERSION,
                request,
            },
            now,
        )
    }
    fn admin(c: &mut Coordinator, request: Request, now: u64) -> Response {
        call(c, &Identity::Admin, request, now)
    }
    fn submit(c: &mut Coordinator, key: &str, plan: Plan, dynamic: bool) -> String {
        match admin(
            c,
            Request::Submit {
                key: key.into(),
                plan,
                dynamic,
            },
            0,
        ) {
            Response::Submitted { run_id } => run_id,
            response => panic!("submit failed: {response:?}"),
        }
    }
    fn claim(c: &mut Coordinator, now: u64) -> Option<Assignment> {
        match call(c, &worker(), Request::Claim, now) {
            Response::Assignment { assignment } => assignment,
            response => panic!("claim failed: {response:?}"),
        }
    }
    fn inspect(c: &mut Coordinator, id: &str, now: u64) -> serde_json::Value {
        match admin(c, Request::Inspect { run_id: id.into() }, now) {
            Response::Data { value } => value,
            response => panic!("inspect failed: {response:?}"),
        }
    }
    fn complete(c: &mut Coordinator, id: &str, success: bool, now: u64) -> Response {
        call(
            c,
            &worker(),
            Request::Complete {
                attempt_id: id.into(),
                report: Report {
                    success,
                    log: "done".into(),
                },
            },
            now,
        )
    }
    // Assertion adapters intentionally own temporary operation responses.
    #[allow(clippy::needless_pass_by_value)]
    fn ok(response: Response) {
        assert!(matches!(response, Response::Ok), "{response:?}");
    }
    // Assertion adapters intentionally own temporary operation responses.
    #[allow(clippy::needless_pass_by_value)]
    fn error(response: Response) {
        assert!(matches!(response, Response::Error { .. }), "{response:?}");
    }

    // Assertion adapters intentionally own temporary operation responses.
    #[allow(clippy::needless_pass_by_value)]
    fn fenced(response: Response, expected: &str) {
        match response {
            Response::Fenced { attempt_id } => assert_eq!(attempt_id, expected),
            other => panic!("expected Fenced for {expected}, received {other:?}"),
        }
    }

    /// Test-owned database paths are unique even between simultaneous test processes.
    struct Database(std::path::PathBuf);
    impl Database {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(std::env::temp_dir().join(format!(
                "gpipe-coordinator-{}-{nanos}-{}.sqlite",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
        fn open(&self) -> Coordinator {
            Coordinator::open(&self.0, config()).unwrap()
        }
    }
    impl Drop for Database {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            for suffix in ["-wal", "-shm"] {
                let mut path = self.0.as_os_str().to_owned();
                path.push(suffix);
                let _ = std::fs::remove_file(std::path::PathBuf::from(path));
            }
        }
    }

    #[test]
    fn restart_preserves_ids_submissions_attempts_and_reports() {
        let database = Database::new();
        let mut c = database.open();
        let first = submit(&mut c, "key", plan(), false);
        let attempt = claim(&mut c, 1).unwrap();
        drop(c);
        let mut c = database.open();
        assert_eq!(submit(&mut c, "key", plan(), false), first);
        assert_eq!(claim(&mut c, 2).unwrap().attempt_id, attempt.attempt_id);
        ok(complete(&mut c, &attempt.attempt_id, true, 3));
        drop(c);
        let mut c = database.open();
        ok(complete(&mut c, &attempt.attempt_id, true, 4));
        assert_eq!(inspect(&mut c, &first, 4)["status"], "SUCCEEDED");
        let second = submit(&mut c, "next", plan(), false);
        assert!(second > first);
        assert!(claim(&mut c, 5).unwrap().attempt_id > attempt.attempt_id);
    }

    #[test]
    fn submission_keys_compare_original_graph_and_dynamic_flag() {
        let mut c = memory();
        let id = submit(&mut c, "key", plan(), true);
        let additions = BTreeMap::from([("later".into(), task())]);
        ok(admin(
            &mut c,
            Request::Expand {
                run_id: id.clone(),
                tasks: additions,
            },
            0,
        ));
        assert_eq!(submit(&mut c, "key", plan(), true), id);
        error(admin(
            &mut c,
            Request::Submit {
                key: "key".into(),
                plan: plan(),
                dynamic: false,
            },
            0,
        ));
        let mut changed = plan();
        changed.tasks.get_mut("build").unwrap().steps[0].run = "false".into();
        error(admin(
            &mut c,
            Request::Submit {
                key: "key".into(),
                plan: changed,
                dynamic: true,
            },
            0,
        ));
    }

    #[test]
    fn revisions_require_exact_approval_and_approvals_survive_restart() {
        let database = Database::new();
        let mut c = database.open();
        let mut p = plan();
        p.revision = "b".repeat(40);
        submit(&mut c, "untrusted", p, false);
        assert!(claim(&mut c, 0).is_none());
        error(admin(
            &mut c,
            Request::Approve {
                repository: "repo".into(),
                revision: "bbbb".into(),
            },
            0,
        ));
        error(admin(
            &mut c,
            Request::Approve {
                repository: "other".into(),
                revision: "b".repeat(40),
            },
            0,
        ));
        ok(admin(
            &mut c,
            Request::Approve {
                repository: "repo".into(),
                revision: "b".repeat(40),
            },
            0,
        ));
        drop(c);
        let mut c = database.open();
        assert_eq!(claim(&mut c, 0).unwrap().revision, "b".repeat(40));
    }

    #[test]
    fn repository_labels_and_aggregate_resources_are_authoritative() {
        let mut cfg = config();
        cfg.workers.get_mut("one").unwrap().memory_mb = 64;
        let mut c = Coordinator::open(Path::new(":memory:"), cfg).unwrap();
        let mut p = plan();
        p.tasks
            .get_mut("build")
            .unwrap()
            .labels
            .insert("os".into(), "windows".into());
        let bad = submit(&mut c, "wrong-label", p, false);
        assert!(claim(&mut c, 0).is_none());
        ok(admin(&mut c, Request::Cancel { run_id: bad }, 0));
        let mut p = plan();
        p.tasks.insert("second".into(), task());
        submit(&mut c, "fits", p, false);
        let first = claim(&mut c, 0).unwrap();
        assert_eq!(claim(&mut c, 0).unwrap().attempt_id, first.attempt_id);
        ok(complete(&mut c, &first.attempt_id, true, 1));
        assert!(claim(&mut c, 1).is_some());
        let mut cfg = config();
        cfg.workers.get_mut("one").unwrap().repositories.clear();
        let mut c = Coordinator::open(Path::new(":memory:"), cfg).unwrap();
        submit(&mut c, "no-repo", plan(), false);
        assert!(claim(&mut c, 0).is_none());
        c.config
            .workers
            .get_mut("one")
            .unwrap()
            .repositories
            .insert("repo".into());
        c.config.workers.get_mut("one").unwrap().cpu = 0;
        assert!(claim(&mut c, 0).is_none());
    }

    #[test]
    fn expiry_retries_fences_results_and_blocks_transitive_dependencies() {
        let mut c = memory();
        let mut p = plan();
        let mut dependent = task();
        dependent.needs = vec!["build".into()];
        p.tasks.insert("test".into(), dependent.clone());
        dependent.needs = vec!["test".into()];
        p.tasks.insert("package".into(), dependent);
        let id = submit(&mut c, "dag", p, false);
        let first = claim(&mut c, 0).unwrap();
        let second = claim(&mut c, LEASE_SECS).unwrap();
        assert_ne!(first.attempt_id, second.attempt_id);
        fenced(
            complete(&mut c, &first.attempt_id, true, LEASE_SECS),
            &first.attempt_id,
        );
        error(call(
            &mut c,
            &worker(),
            Request::Heartbeat {
                attempt_id: first.attempt_id,
            },
            LEASE_SECS,
        ));
        assert!(claim(&mut c, LEASE_SECS * 2).is_none());
        let view = inspect(&mut c, &id, LEASE_SECS * 2);
        assert_eq!(view["status"], "FAILED");
        assert_eq!(view["tasks"]["build"], "FAILED");
        assert_eq!(view["tasks"]["test"], "BLOCKED");
        assert_eq!(view["tasks"]["package"], "BLOCKED");
        fenced(
            complete(&mut c, &second.attempt_id, true, LEASE_SECS * 2),
            &second.attempt_id,
        );
    }

    #[test]
    fn heartbeats_renew_and_only_the_assigned_worker_can_settle() {
        let mut c = memory();
        submit(&mut c, "key", plan(), false);
        let a = claim(&mut c, 0).unwrap();
        let other = Identity::Worker("two".into());
        error(call(
            &mut c,
            &other,
            Request::Heartbeat {
                attempt_id: a.attempt_id.clone(),
            },
            1,
        ));
        error(call(
            &mut c,
            &other,
            Request::Complete {
                attempt_id: a.attempt_id.clone(),
                report: Report {
                    success: true,
                    log: "done".into(),
                },
            },
            1,
        ));
        assert!(matches!(
            call(
                &mut c,
                &worker(),
                Request::Heartbeat {
                    attempt_id: a.attempt_id.clone()
                },
                30
            ),
            Response::Heartbeat {
                cancel: false,
                lease_until: 90
            }
        ));
        assert_eq!(claim(&mut c, 60).unwrap().attempt_id, a.attempt_id);
        ok(complete(&mut c, &a.attempt_id, true, 89));
        ok(complete(&mut c, &a.attempt_id, true, 100));
        error(complete(&mut c, &a.attempt_id, false, 100));
    }

    #[test]
    fn cancellation_holds_capacity_until_acknowledgement_or_expiry() {
        for acknowledge in [false, true] {
            let mut cfg = config();
            cfg.workers.get_mut("one").unwrap().cpu = 1;
            let mut c = Coordinator::open(Path::new(":memory:"), cfg).unwrap();
            let id = submit(&mut c, "first", plan(), false);
            let a = claim(&mut c, 0).unwrap();
            submit(&mut c, "second", plan(), false);
            ok(admin(&mut c, Request::Cancel { run_id: id.clone() }, 1));
            assert_eq!(claim(&mut c, 1).unwrap().attempt_id, a.attempt_id);
            assert!(matches!(
                call(
                    &mut c,
                    &worker(),
                    Request::Heartbeat {
                        attempt_id: a.attempt_id.clone()
                    },
                    30
                ),
                Response::Heartbeat {
                    cancel: true,
                    lease_until: LEASE_SECS
                }
            ));
            if acknowledge {
                fenced(complete(&mut c, &a.attempt_id, true, 31), &a.attempt_id);
                assert!(claim(&mut c, 31).is_some());
            } else {
                assert!(claim(&mut c, LEASE_SECS).is_some());
                fenced(
                    complete(&mut c, &a.attempt_id, true, LEASE_SECS),
                    &a.attempt_id,
                );
            }
            assert_eq!(inspect(&mut c, &id, 31)["status"], "CANCELLED");
        }
    }

    #[test]
    fn dynamic_expansion_is_atomic_idempotent_and_requires_sealing() {
        let mut c = memory();
        let id = submit(&mut c, "dynamic", plan(), true);
        let a = claim(&mut c, 0).unwrap();
        ok(complete(&mut c, &a.attempt_id, true, 1));
        assert_eq!(inspect(&mut c, &id, 1)["status"], "RUNNING");
        let mut next = task();
        next.needs = vec!["build".into()];
        let additions = BTreeMap::from([("test".into(), next)]);
        for _ in 0..2 {
            ok(admin(
                &mut c,
                Request::Expand {
                    run_id: id.clone(),
                    tasks: additions.clone(),
                },
                1,
            ));
        }
        let before = inspect(&mut c, &id, 1);
        let mut invalid = task();
        invalid.needs = vec!["missing".into()];
        error(admin(
            &mut c,
            Request::Expand {
                run_id: id.clone(),
                tasks: BTreeMap::from([("good".into(), task()), ("bad".into(), invalid)]),
            },
            1,
        ));
        assert_eq!(inspect(&mut c, &id, 1), before);
        let mut changed = task();
        changed.cpu = 2;
        error(admin(
            &mut c,
            Request::Expand {
                run_id: id.clone(),
                tasks: BTreeMap::from([("build".into(), changed)]),
            },
            1,
        ));
        let a = claim(&mut c, 2).unwrap();
        assert_eq!(a.task_id, "test");
        ok(complete(&mut c, &a.attempt_id, true, 3));
        assert_eq!(inspect(&mut c, &id, 3)["status"], "RUNNING");
        ok(admin(&mut c, Request::Seal { run_id: id.clone() }, 3));
        assert_eq!(inspect(&mut c, &id, 3)["status"], "SUCCEEDED");
        error(admin(
            &mut c,
            Request::Expand {
                run_id: id,
                tasks: additions,
            },
            3,
        ));
    }

    #[test]
    fn failed_reports_retry_and_sealed_graphs_never_expand() {
        let mut c = memory();
        let id = submit(&mut c, "static", plan(), false);
        error(admin(
            &mut c,
            Request::Expand {
                run_id: id.clone(),
                tasks: BTreeMap::new(),
            },
            0,
        ));
        let first = claim(&mut c, 0).unwrap();
        ok(complete(&mut c, &first.attempt_id, false, 1));
        let second = claim(&mut c, 1).unwrap();
        ok(complete(&mut c, &first.attempt_id, false, 2));
        ok(complete(&mut c, &second.attempt_id, false, 2));
        assert_eq!(inspect(&mut c, &id, 2)["status"], "FAILED");
    }

    #[test]
    fn workers_cannot_manage_runs_and_bad_envelopes_fail_closed() {
        let mut c = memory();
        let id = submit(&mut c, "key", plan(), false);
        for request in [
            Request::Submit {
                key: "other".into(),
                plan: plan(),
                dynamic: true,
            },
            Request::List,
            Request::Inspect { run_id: id.clone() },
            Request::Expand {
                run_id: id.clone(),
                tasks: BTreeMap::new(),
            },
            Request::Seal { run_id: id.clone() },
            Request::Cancel { run_id: id },
            Request::Approve {
                repository: "repo".into(),
                revision: "b".repeat(40),
            },
        ] {
            error(call(&mut c, &worker(), request, 0));
        }
        error(admin(&mut c, Request::Claim, 0));
        error(call(
            &mut c,
            &Identity::Worker("unknown".into()),
            Request::Claim,
            0,
        ));
        error(c.handle(
            &worker(),
            Envelope {
                version: PROTOCOL_VERSION + 1,
                request: Request::Claim,
            },
            0,
        ));
        assert!(claim(&mut c, 0).is_some());
    }

    #[test]
    fn log_boundary_is_in_bytes_and_rejection_does_not_complete() {
        let mut c = memory();
        submit(&mut c, "key", plan(), false);
        let a = claim(&mut c, 0).unwrap();
        error(call(
            &mut c,
            &worker(),
            Request::Complete {
                attempt_id: a.attempt_id.clone(),
                report: Report {
                    success: true,
                    log: "x".repeat(MAX_REPORT_BYTES + 1),
                },
            },
            1,
        ));
        let report = Report {
            success: true,
            log: "é".repeat(MAX_REPORT_BYTES / 2),
        };
        ok(call(
            &mut c,
            &worker(),
            Request::Complete {
                attempt_id: a.attempt_id.clone(),
                report: report.clone(),
            },
            1,
        ));
        ok(call(
            &mut c,
            &worker(),
            Request::Complete {
                attempt_id: a.attempt_id,
                report,
            },
            2,
        ));
    }

    #[test]
    fn list_is_bounded_newest_first_and_omits_logs() {
        let mut c = memory();
        let mut last = String::new();
        for i in 0..=LIST_LIMIT {
            last = submit(&mut c, &format!("key-{i}"), plan(), false);
        }
        let Response::Data { value } = admin(&mut c, Request::List, 0) else {
            panic!("list failed");
        };
        assert_eq!(value.as_array().unwrap().len(), LIST_LIMIT);
        assert_eq!(value[0]["run_id"], last);
        assert!(value[0].get("attempts").is_none());
        assert!(value[0].get("plan").is_none());
    }

    #[test]
    fn handles_share_atomic_claims_and_global_monotonic_ids() {
        let database = Database::new();
        let mut first = database.open();
        let mut second = database.open();
        let id = submit(&mut first, "key", plan(), false);
        assert_eq!(submit(&mut second, "key", plan(), false), id);
        let a = claim(&mut first, 0).unwrap();
        assert_eq!(claim(&mut second, 0).unwrap().attempt_id, a.attempt_id);
        ok(complete(&mut second, &a.attempt_id, true, 1));
        let next = submit(&mut second, "second", plan(), false);
        assert!(next > id);
        assert!(claim(&mut first, 1).unwrap().attempt_id > a.attempt_id);
    }

    #[test]
    fn claim_replays_exact_assignment_without_renewal_and_without_parallel_admission() {
        let database = Database::new();
        let mut c = database.open();
        let mut p = plan();
        p.tasks.insert("second".into(), task());
        let id = submit(&mut c, "work", p, false);
        let original = claim(&mut c, 0).unwrap();
        for now in [1, 10, 59] {
            let replay = claim(&mut c, now).unwrap();
            assert_eq!(
                serde_json::to_value(&original).unwrap(),
                serde_json::to_value(&replay).unwrap()
            );
        }
        let view = inspect(&mut c, &id, 59);
        assert_eq!(view["attempts"].as_object().unwrap().len(), 1);
        assert_eq!(view["tasks"]["second"], "PENDING");
        drop(c);
        let mut c = database.open();
        assert_eq!(claim(&mut c, 59).unwrap().attempt_id, original.attempt_id);
        let retry = claim(&mut c, 60).unwrap();
        assert_ne!(retry.attempt_id, original.attempt_id);
        fenced(
            complete(&mut c, &original.attempt_id, true, 60),
            &original.attempt_id,
        );
    }

    #[test]
    fn serializer_limits_count_escaped_bytes_and_never_truncate() {
        let value = "\0é";
        let exact = serde_json::to_string(value).unwrap();
        assert_eq!(bounded_json(&value, exact.len(), "test").unwrap(), exact);
        assert!(bounded_json(&value, exact.len() - 1, "test").is_err());
        let mut sink = LimitedJson {
            bytes: vec![],
            limit: 1,
        };
        assert!(std::io::Write::write_all(&mut sink, b"ab").is_err());
        assert_eq!(sink.bytes, Vec::<u8>::new());
    }

    #[test]
    fn oversized_inspect_returns_explicit_error_without_pruning_reports() {
        let mut c = memory();
        let mut p = plan();
        p.tasks.insert("second".into(), task());
        let id = submit(&mut c, "large-evidence", p, false);
        // Each valid 1 MiB log expands to 6 MiB of JSON escaping.
        for _ in 0..2 {
            let attempt = claim(&mut c, 0).unwrap();
            ok(call(
                &mut c,
                &worker(),
                Request::Complete {
                    attempt_id: attempt.attempt_id,
                    report: Report {
                        success: true,
                        log: "\0".repeat(MAX_REPORT_BYTES),
                    },
                },
                1,
            ));
        }
        let Response::Error { message } = admin(&mut c, Request::Inspect { run_id: id.clone() }, 1)
        else {
            panic!("Inspect must fail explicitly");
        };
        assert!(message.contains("Inspect"));
        assert!(message.contains(&MAX_INSPECT_BYTES.to_string()));
        let state = load(&c.connection).unwrap();
        assert_eq!(state.runs[&id].attempts.len(), 2);
        for attempt in state.runs[&id].attempts.values() {
            assert_eq!(attempt.report.as_ref().unwrap().log.len(), MAX_REPORT_BYTES);
        }
        assert!(matches!(
            admin(&mut c, Request::List, 1),
            Response::Data { .. }
        ));
    }

    fn store(c: &Coordinator, state: &State) {
        c.connection
            .execute(
                "UPDATE gpipe_state SET json = ?1",
                [serde_json::to_string(state).unwrap()],
            )
            .unwrap();
    }

    #[test]
    fn retention_limits_reject_growth_atomically_and_allow_exact_replays() {
        let mut c = memory();
        let id = submit(&mut c, "key", plan(), false);
        let mut state = load(&c.connection).unwrap();
        for index in 0..MAX_APPROVALS {
            state
                .approvals
                .entry("repo".into())
                .or_default()
                .insert(format!("{index:040x}"));
        }
        store(&c, &state);
        ok(admin(
            &mut c,
            Request::Approve {
                repository: "repo".into(),
                revision: "0".repeat(40),
            },
            0,
        ));
        error(admin(
            &mut c,
            Request::Approve {
                repository: "repo".into(),
                revision: "b".repeat(40),
            },
            0,
        ));
        assert_eq!(
            load(&c.connection).unwrap().approvals["repo"].len(),
            MAX_APPROVALS
        );
        state.approvals.clear();
        for index in 0..MAX_ATTEMPTS {
            state.runs.get_mut(&id).unwrap().attempts.insert(
                format!("old-{index}"),
                Attempt {
                    task_id: "build".into(),
                    worker: "one".into(),
                    instance_id: None,
                    lease_until: 0,
                    status: AttemptStatus::Expired,
                    report: None,
                    cancel_requested: false,
                    generated: None,
                },
            );
        }
        store(&c, &state);
        error(call(&mut c, &worker(), Request::Claim, 0));
        let after = load(&c.connection).unwrap();
        assert_eq!(after.next_attempt, 0);
        assert!(matches!(
            after.runs[&id].tasks["build"],
            TaskStatus::Pending
        ));
        state.runs.get_mut(&id).unwrap().attempts.clear();
        let template = serde_json::to_string(&state.runs[&id]).unwrap();
        for index in 1..MAX_RUNS {
            state.runs.insert(
                format!("retained-{index}"),
                serde_json::from_str(&template).unwrap(),
            );
        }
        store(&c, &state);
        assert_eq!(submit(&mut c, "key", plan(), false), id);
        error(admin(
            &mut c,
            Request::Submit {
                key: "new".into(),
                plan: plan(),
                dynamic: false,
            },
            0,
        ));
        let after = load(&c.connection).unwrap();
        assert_eq!(after.runs.len(), MAX_RUNS);
        assert_eq!(after.next_run, 1);
        assert!(!after.submissions.contains_key("new"));
    }

    #[test]
    fn approval_lexically_rejects_refs_prefixes_whitespace_and_non_hex() {
        let mut c = memory();
        for revision in [
            "main".into(),
            "refs/heads/main".into(),
            "g".repeat(40),
            format!(" {}", "a".repeat(40)),
            format!("{}\n", "a".repeat(40)),
            format!("{}\0", "a".repeat(39)),
            format!("--{}", "a".repeat(38)),
            "a".repeat(39),
            "a".repeat(41),
            "é".repeat(20),
        ] {
            error(admin(
                &mut c,
                Request::Approve {
                    repository: "repo".into(),
                    revision,
                },
                0,
            ));
        }
        assert!(load(&c.connection).unwrap().approvals.is_empty());
        for revision in ["A".repeat(40), "b".repeat(64)] {
            ok(admin(
                &mut c,
                Request::Approve {
                    repository: "repo".into(),
                    revision,
                },
                0,
            ));
        }
    }

    fn generator_plan() -> Plan {
        let mut p = plan();
        p.tasks.get_mut("build").unwrap().steps[0]
            .env
            .insert("GPIPE_TASKS_OUTPUT".into(), "gpipe.tasks.json".into());
        p
    }

    fn generated_request(attempt_id: &str, seal: bool) -> Request {
        let mut child = task();
        child.needs = vec!["build".into()];
        Request::CompleteGenerated {
            attempt_id: attempt_id.into(),
            report: Report {
                success: true,
                log: "generated".into(),
            },
            tasks: BTreeMap::from([("child".into(), child)]),
            seal,
        }
    }

    #[test]
    fn generated_completion_is_atomic_durable_and_replays_after_sealing() {
        for seal in [false, true] {
            let database = Database::new();
            let mut c = database.open();
            let id = submit(&mut c, "generator", generator_plan(), true);
            let attempt = claim(&mut c, 0).unwrap();
            assert!(matches!(
                call(&mut c, &Identity::Worker("two".into()), Request::Claim, 0),
                Response::Assignment { assignment: None }
            ));
            let request = generated_request(&attempt.attempt_id, seal);
            ok(call(&mut c, &worker(), request.clone(), 1));
            drop(c);
            let mut c = database.open();
            ok(call(&mut c, &worker(), request.clone(), 100));
            let snapshot = inspect(&mut c, &id, 100);
            assert_eq!(snapshot["tasks"]["build"], "SUCCEEDED");
            assert_eq!(snapshot["tasks"]["child"], "PENDING");
            assert_eq!(snapshot["sealed"], seal);
            assert_eq!(snapshot["plan"]["repository"], "repo");
            assert_eq!(snapshot["plan"]["revision"], "a".repeat(40));
            let mut conflict = request.clone();
            if let Request::CompleteGenerated { seal, .. } = &mut conflict {
                *seal = !*seal;
            }
            error(call(&mut c, &worker(), conflict, 100));
            let mut conflict = request.clone();
            if let Request::CompleteGenerated { tasks, .. } = &mut conflict {
                tasks.get_mut("child").unwrap().steps[0].run = "different".into();
            }
            error(call(&mut c, &worker(), conflict, 100));
            let mut conflict = request.clone();
            if let Request::CompleteGenerated { report, .. } = &mut conflict {
                report.log = "different".into();
            }
            error(call(&mut c, &worker(), conflict, 100));
            error(call(
                &mut c,
                &worker(),
                Request::Complete {
                    attempt_id: attempt.attempt_id.clone(),
                    report: Report {
                        success: true,
                        log: "generated".into(),
                    },
                },
                100,
            ));
            let child = claim(&mut c, 100).unwrap();
            assert_eq!(child.task_id, "child");
            ok(complete(&mut c, &child.attempt_id, true, 101));
            assert_eq!(
                inspect(&mut c, &id, 101)["status"],
                if seal { "SUCCEEDED" } else { "RUNNING" }
            );
            ok(call(&mut c, &worker(), request, 102));
        }
    }

    #[test]
    fn generated_completion_requires_owner_generator_success_and_open_graph() {
        for mode in 0..6 {
            let mut c = memory();
            let mut p = generator_plan();
            if mode == 0 {
                p.tasks.get_mut("build").unwrap().steps[0].env.clear();
            }
            if mode == 1 {
                p.tasks.get_mut("build").unwrap().steps[0]
                    .env
                    .insert("GPIPE_TASKS_OUTPUT".into(), "./gpipe.tasks.json".into());
            }
            let id = submit(&mut c, "generator", p, mode != 2);
            let a = claim(&mut c, 0).unwrap();
            if mode == 3 {
                ok(admin(&mut c, Request::Seal { run_id: id.clone() }, 0));
            }
            let before = inspect(&mut c, &id, 0);
            let mut request = generated_request(&a.attempt_id, true);
            if mode == 4
                && let Request::CompleteGenerated { report, .. } = &mut request
            {
                report.success = false;
            }
            let identity = if mode == 5 {
                Identity::Worker("two".into())
            } else {
                worker()
            };
            error(call(&mut c, &identity, request.clone(), 1));
            error(admin(&mut c, request, 1));
            assert_eq!(inspect(&mut c, &id, 1), before);
        }
    }

    #[test]
    fn generated_invalid_graph_replacements_and_policy_escalation_roll_back() {
        for mode in 0..10 {
            let mut c = memory();
            let mut p = generator_plan();
            p.tasks
                .get_mut("build")
                .unwrap()
                .labels
                .insert("os".into(), "linux".into());
            let id = submit(&mut c, "generator", p, true);
            let a = claim(&mut c, 0).unwrap();
            let before = inspect(&mut c, &id, 0);
            let mut request = generated_request(&a.attempt_id, true);
            if let Request::CompleteGenerated { tasks, .. } = &mut request {
                let child = tasks.get_mut("child").unwrap();
                child.labels.insert("os".into(), "linux".into());
                match mode {
                    0 => child.needs.push("missing".into()),
                    1 => child.needs.push("child".into()),
                    2 => child.needs.clear(),
                    3 => child.cpu += 1,
                    4 => child.memory_mb += 1,
                    5 => child.max_attempts += 1,
                    6 => child.labels.clear(),
                    7 => {
                        child.steps[0]
                            .env
                            .insert("GPIPE_TASKS_OUTPUT".into(), "gpipe.tasks.json".into());
                    }
                    8 => {
                        tasks.insert("build".into(), task());
                    }
                    _ => child.steps.clear(),
                }
            }
            error(call(&mut c, &worker(), request, 1));
            assert_eq!(inspect(&mut c, &id, 1), before);
            assert_eq!(claim(&mut c, 1).unwrap().attempt_id, a.attempt_id);
        }
    }

    #[test]
    fn generated_cancelled_expired_and_plain_completed_attempts_are_fenced() {
        for mode in 0..3 {
            let mut c = memory();
            let id = submit(&mut c, "generator", generator_plan(), true);
            let a = claim(&mut c, 0).unwrap();
            let now = if mode == 1 { LEASE_SECS } else { 1 };
            if mode == 0 {
                ok(admin(&mut c, Request::Cancel { run_id: id.clone() }, 1));
            }
            if mode == 2 {
                ok(complete(&mut c, &a.attempt_id, true, 1));
            }
            let response = call(
                &mut c,
                &worker(),
                generated_request(&a.attempt_id, true),
                now,
            );
            if mode == 2 {
                error(response);
            } else {
                fenced(response, &a.attempt_id);
            }
            let view = inspect(&mut c, &id, now);
            assert!(view["plan"]["tasks"].get("child").is_none());
            assert_eq!(view["sealed"], false);
        }
    }

    #[test]
    fn generated_empty_batch_can_seal_and_old_attempt_json_still_loads() {
        let mut c = memory();
        let id = submit(&mut c, "generator", generator_plan(), true);
        let a = claim(&mut c, 0).unwrap();
        let request = Request::CompleteGenerated {
            attempt_id: a.attempt_id,
            report: Report {
                success: true,
                log: String::new(),
            },
            tasks: BTreeMap::new(),
            seal: true,
        };
        ok(call(&mut c, &worker(), request.clone(), 1));
        assert_eq!(inspect(&mut c, &id, 1)["status"], "SUCCEEDED");
        ok(call(&mut c, &worker(), request, 2));
        let old: Attempt = serde_json::from_value(serde_json::json!({"task_id":"old", "worker":"one", "lease_until":0, "status":"EXPIRED", "report":null})).unwrap();
        assert!(old.generated.is_none());
    }

    #[test]
    fn fenced_completions_are_owner_scoped_durable_and_release_cancelled_capacity() {
        for generated in [false, true] {
            for cancelled in [false, true] {
                let database = Database::new();
                let mut c = database.open();
                let id = submit(&mut c, "generator", generator_plan(), true);
                let attempt = claim(&mut c, 0).unwrap();
                let request = if generated {
                    generated_request(&attempt.attempt_id, true)
                } else {
                    Request::Complete {
                        attempt_id: attempt.attempt_id.clone(),
                        report: Report {
                            success: true,
                            log: "evidence".into(),
                        },
                    }
                };
                let now = if cancelled { 1 } else { LEASE_SECS };
                if cancelled {
                    ok(admin(&mut c, Request::Cancel { run_id: id.clone() }, now));
                }
                error(call(
                    &mut c,
                    &Identity::Worker("two".into()),
                    request.clone(),
                    now,
                ));
                error(call(
                    &mut c,
                    &Identity::Worker("unknown".into()),
                    request.clone(),
                    now,
                ));
                error(admin(&mut c, request.clone(), now));
                let mut unknown = request.clone();
                match &mut unknown {
                    Request::Complete { attempt_id, .. }
                    | Request::CompleteGenerated { attempt_id, .. } => {
                        *attempt_id = "not-an-attempt".into();
                    }
                    _ => unreachable!(),
                }
                error(call(&mut c, &worker(), unknown, now));
                // Wrong-worker cancellation delivery must not acknowledge the
                // owner's active lease or free its reservation.
                if cancelled {
                    assert_eq!(claim(&mut c, now).unwrap().attempt_id, attempt.attempt_id);
                    let state = load(&c.connection).unwrap();
                    let retained = &state.runs[&id].attempts[&attempt.attempt_id];
                    assert!(matches!(retained.status, AttemptStatus::Active));
                    assert_eq!(retained.lease_until, attempt.lease_until);
                }
                fenced(
                    call(&mut c, &worker(), request.clone(), now),
                    &attempt.attempt_id,
                );
                let state = load(&c.connection).unwrap();
                let retained = &state.runs[&id].attempts[&attempt.attempt_id];
                assert!(matches!(retained.status, AttemptStatus::Expired));
                assert!(retained.report.is_none());
                assert!(retained.generated.is_none());
                assert!(!state.runs[&id].plan.tasks.contains_key("child"));
                drop(c);
                let mut c = database.open();
                fenced(call(&mut c, &worker(), request, now), &attempt.attempt_id);
                error(call(
                    &mut c,
                    &Identity::Worker("two".into()),
                    Request::Complete {
                        attempt_id: attempt.attempt_id.clone(),
                        report: Report {
                            success: true,
                            log: String::new(),
                        },
                    },
                    now,
                ));
                submit(&mut c, "next", plan(), false);
                assert_ne!(claim(&mut c, now).unwrap().attempt_id, attempt.attempt_id);
            }
        }
    }

    #[test]
    fn policy_revocation_is_sticky_cancels_without_releasing_until_ack_or_expiry() {
        for mode in 0..6 {
            for via_claim in [false, true] {
                let mut c = memory();
                let mut p = plan();
                p.tasks
                    .get_mut("build")
                    .unwrap()
                    .labels
                    .insert("os".into(), "linux".into());
                let id = submit(&mut c, "work", p, false);
                let attempt = claim(&mut c, 0).unwrap();
                let original = c.config.clone();
                match mode {
                    0 => c
                        .config
                        .repositories
                        .get_mut("repo")
                        .unwrap()
                        .revisions
                        .clear(),
                    1 => {
                        c.config.repositories.remove("repo");
                    }
                    2 => c
                        .config
                        .workers
                        .get_mut("one")
                        .unwrap()
                        .repositories
                        .clear(),
                    3 => c.config.workers.get_mut("one").unwrap().labels.clear(),
                    4 => c.config.workers.get_mut("one").unwrap().cpu = 0,
                    _ => {
                        c.config.workers.remove("one");
                    }
                }
                if via_claim {
                    assert!(claim(&mut c, 1).is_none());
                }
                assert!(matches!(
                    call(
                        &mut c,
                        &worker(),
                        Request::Heartbeat {
                            attempt_id: attempt.attempt_id.clone()
                        },
                        2
                    ),
                    Response::Heartbeat {
                        cancel: true,
                        lease_until: LEASE_SECS
                    }
                ));
                let state = load(&c.connection).unwrap();
                let retained = &state.runs[&id].attempts[&attempt.attempt_id];
                assert!(retained.cancel_requested);
                assert!(matches!(retained.status, AttemptStatus::Active));
                assert_eq!(completion_reservations(&state).unwrap(), REPORT_RESERVATION);
                c.config = original;
                assert!(claim(&mut c, 3).is_none());
                if via_claim {
                    fenced(
                        complete(&mut c, &attempt.attempt_id, true, 4),
                        &attempt.attempt_id,
                    );
                    assert!(claim(&mut c, 4).is_some());
                } else {
                    assert!(claim(&mut c, LEASE_SECS).is_some());
                }
            }
        }
    }

    #[test]
    fn durable_approval_is_considered_when_revalidating_active_attempts() {
        let mut c = memory();
        ok(admin(
            &mut c,
            Request::Approve {
                repository: "repo".into(),
                revision: "a".repeat(40),
            },
            0,
        ));
        submit(&mut c, "work", plan(), false);
        let attempt = claim(&mut c, 0).unwrap();
        c.config
            .repositories
            .get_mut("repo")
            .unwrap()
            .revisions
            .clear();
        assert_eq!(claim(&mut c, 1).unwrap().attempt_id, attempt.attempt_id);
        assert!(matches!(
            call(
                &mut c,
                &worker(),
                Request::Heartbeat {
                    attempt_id: attempt.attempt_id
                },
                2
            ),
            Response::Heartbeat { cancel: false, .. }
        ));
    }

    #[test]
    fn admission_reserves_worst_case_report_and_rejects_competing_growth() {
        let mut c = memory();
        let id = submit(&mut c, "work", plan(), false);
        let mut state = load(&c.connection).unwrap();
        // Keep the real 1 MiB report / 6x JSON escaping proof, but lower only
        // this handle's state ceiling. No giant history or repeated serialization.
        c.state_limit = REPORT_RESERVATION + 65_536;
        let target = c.state_limit - REPORT_RESERVATION - 4_096;
        let bytes = serde_json::to_vec(&state).unwrap().len();
        let remaining = target - bytes;
        state.runs.get_mut(&id).unwrap().attempts.insert(
            "padding".into(),
            Attempt {
                task_id: "build".into(),
                worker: "two".into(),
                instance_id: None,
                lease_until: 0,
                status: AttemptStatus::Completed,
                report: Some(Report {
                    success: true,
                    log: "x".repeat(remaining.saturating_sub(512)),
                }),
                cancel_requested: false,
                generated: None,
            },
        );
        store(&c, &state);
        let attempt = claim(&mut c, 0).unwrap();
        let mut big_plan = plan();
        big_plan.tasks.get_mut("build").unwrap().steps[0].run = "x".repeat(8_192);
        error(admin(
            &mut c,
            Request::Submit {
                key: "steal-budget".into(),
                plan: big_plan,
                dynamic: false,
            },
            1,
        ));
        // Another worker must not consume the reserved terminal budget either.
        let mut pending = task();
        pending.memory_mb = 0;
        state = load(&c.connection).unwrap();
        state
            .runs
            .get_mut(&id)
            .unwrap()
            .plan
            .tasks
            .insert("pending".into(), pending);
        state
            .runs
            .get_mut(&id)
            .unwrap()
            .tasks
            .insert("pending".into(), TaskStatus::Pending);
        store(&c, &state);
        error(call(
            &mut c,
            &Identity::Worker("two".into()),
            Request::Claim,
            1,
        ));
        // NUL is the six-byte JSON worst case, not merely a 1 MiB ASCII log.
        ok(call(
            &mut c,
            &worker(),
            Request::Complete {
                attempt_id: attempt.attempt_id,
                report: Report {
                    success: true,
                    log: "\0".repeat(MAX_REPORT_BYTES),
                },
            },
            1,
        ));
        assert_eq!(load(&c.connection).unwrap().runs[&id].attempts.len(), 2);
    }

    #[test]
    fn generator_reserves_bounded_batch_and_oversized_batch_rolls_back() {
        let mut c = memory();
        let id = submit(&mut c, "generator", generator_plan(), true);
        let attempt = claim(&mut c, 0).unwrap();
        let state = load(&c.connection).unwrap();
        assert_eq!(
            completion_reservations(&state).unwrap(),
            REPORT_RESERVATION + 3 * MAX_GENERATED_BYTES
        );
        let before = inspect(&mut c, &id, 0);
        let mut request = generated_request(&attempt.attempt_id, true);
        if let Request::CompleteGenerated { tasks, .. } = &mut request {
            let child = tasks.get_mut("child").unwrap();
            child.steps[0].run = "x".repeat(crate::plan::MAX_TEXT_BYTES);
            let step = child.steps[0].clone();
            child.steps.extend([step.clone(), step]);
        }
        error(call(&mut c, &worker(), request, 1));
        assert_eq!(inspect(&mut c, &id, 1), before);
        ok(call(
            &mut c,
            &worker(),
            generated_request(&attempt.attempt_id, true),
            1,
        ));
    }

    #[test]
    fn last_attempt_slot_remains_completable_and_tombstones_block_further_admission() {
        let mut c = memory();
        let id = submit(&mut c, "work", plan(), false);
        let mut state = load(&c.connection).unwrap();
        for index in 0..MAX_ATTEMPTS - 1 {
            state.runs.get_mut(&id).unwrap().attempts.insert(
                format!("old-{index}"),
                Attempt {
                    task_id: "build".into(),
                    worker: "two".into(),
                    instance_id: None,
                    lease_until: 0,
                    status: AttemptStatus::Expired,
                    report: None,
                    cancel_requested: false,
                    generated: None,
                },
            );
        }
        store(&c, &state);
        let attempt = claim(&mut c, 0).unwrap();
        ok(complete(&mut c, &attempt.attempt_id, true, 1));
        ok(complete(&mut c, &attempt.attempt_id, true, 2));
        assert_eq!(
            load(&c.connection).unwrap().runs[&id].attempts.len(),
            MAX_ATTEMPTS
        );
        submit(&mut c, "next", plan(), false);
        error(call(&mut c, &worker(), Request::Claim, 3));
        let state = load(&c.connection).unwrap();
        assert_eq!(state.runs[&id].attempts.len(), MAX_ATTEMPTS);
        assert_eq!(state.next_attempt, 1);
    }

    fn access(c: &mut Coordinator, attempt: &str, producer: Option<&str>, now: u64) -> Response {
        call(
            c,
            &worker(),
            Request::ArtifactAccess {
                attempt_id: attempt.into(),
                producer_task: producer.map(str::to_owned),
            },
            now,
        )
    }

    fn access_data(response: Response) -> serde_json::Value {
        match response {
            Response::Data { value } => value,
            other => panic!("expected access data, received {other:?}"),
        }
    }

    #[test]
    fn artifacts_allow_own_publish_and_successful_direct_dependency_only() {
        let mut c = memory();
        let mut p = plan();
        let mut child = task();
        child.needs = vec!["build".into()];
        p.tasks.insert("consumer".into(), child);
        let id = submit(&mut c, "pipeline", p, false);
        let first = claim(&mut c, 0).unwrap();
        assert_eq!(
            access_data(access(&mut c, &first.attempt_id, None, 1)),
            serde_json::json!({"producer_attempt_id": first.attempt_id, "run_id": id, "task_id":"build"})
        );
        error(access(&mut c, &first.attempt_id, Some("build"), 1));
        ok(complete(&mut c, &first.attempt_id, false, 1));
        let producer = claim(&mut c, 2).unwrap();
        ok(complete(&mut c, &producer.attempt_id, true, 3));
        error(access(&mut c, &producer.attempt_id, None, 3));
        let consumer = claim(&mut c, 4).unwrap();
        assert_eq!(consumer.task_id, "consumer");
        assert_eq!(
            access_data(access(&mut c, &consumer.attempt_id, Some("build"), 5)),
            serde_json::json!({"producer_attempt_id": producer.attempt_id, "run_id": id, "task_id":"build"})
        );
        error(access(&mut c, &consumer.attempt_id, Some("missing"), 5));
        error(access(&mut c, &consumer.attempt_id, Some("consumer"), 5));
        error(access(&mut c, "unknown", None, 5));
        for identity in [
            Identity::Admin,
            Identity::Worker("two".into()),
            Identity::Worker("unknown".into()),
        ] {
            for producer_task in [None, Some("build".into())] {
                let response = call(
                    &mut c,
                    &identity,
                    Request::ArtifactAccess {
                        attempt_id: consumer.attempt_id.clone(),
                        producer_task,
                    },
                    5,
                );
                let Response::Error { message } = response else {
                    panic!("must not disclose artifact identities");
                };
                assert!(!message.contains(&producer.attempt_id));
                assert!(!message.contains(&id));
            }
        }
    }

    #[test]
    fn artifacts_reject_transitive_dependencies_and_cross_run_task_ids() {
        let mut c = memory();
        let mut p = plan();
        let mut mid = task();
        mid.needs = vec!["build".into()];
        p.tasks.insert("middle".into(), mid);
        let mut leaf = task();
        leaf.needs = vec!["middle".into()];
        p.tasks.insert("leaf".into(), leaf);
        submit(&mut c, "chain", p, false);
        for now in 0..2 {
            let a = claim(&mut c, now).unwrap();
            ok(complete(&mut c, &a.attempt_id, true, now));
        }
        let leaf = claim(&mut c, 2).unwrap();
        assert_eq!(leaf.task_id, "leaf");
        error(access(&mut c, &leaf.attempt_id, Some("build"), 2));
        assert_eq!(
            access_data(access(&mut c, &leaf.attempt_id, Some("middle"), 2))["task_id"],
            "middle"
        );
        ok(complete(&mut c, &leaf.attempt_id, true, 3));
        submit(&mut c, "other-run", plan(), false);
        let unrelated = claim(&mut c, 4).unwrap();
        error(access(&mut c, &unrelated.attempt_id, Some("middle"), 4));
    }

    #[test]
    fn artifacts_reject_cancelled_expired_revoked_and_failed_producers() {
        for mode in 0..5 {
            let mut c = memory();
            let mut p = plan();
            let mut child = task();
            child.needs = vec!["build".into()];
            p.tasks.insert("consumer".into(), child);
            let id = submit(&mut c, "pipeline", p, false);
            let producer = claim(&mut c, 0).unwrap();
            ok(complete(&mut c, &producer.attempt_id, true, 1));
            let consumer = claim(&mut c, 2).unwrap();
            let mut now = 3;
            match mode {
                0 => {
                    ok(admin(&mut c, Request::Cancel { run_id: id.clone() }, 3));
                }
                1 => now = 2 + LEASE_SECS,
                2 => c
                    .config
                    .workers
                    .get_mut("one")
                    .unwrap()
                    .repositories
                    .clear(),
                3 => c
                    .config
                    .repositories
                    .get_mut("repo")
                    .unwrap()
                    .revisions
                    .clear(),
                _ => {
                    // Defensive persisted-state checks: a producer status alone
                    // is insufficient without a successful completed report.
                    let mut state = load(&c.connection).unwrap();
                    state
                        .runs
                        .get_mut(&id)
                        .unwrap()
                        .attempts
                        .get_mut(&producer.attempt_id)
                        .unwrap()
                        .report
                        .as_mut()
                        .unwrap()
                        .success = false;
                    store(&c, &state);
                }
            }
            error(access(&mut c, &consumer.attempt_id, Some("build"), now));
            if mode == 4 {
                let mut state = load(&c.connection).unwrap();
                state
                    .runs
                    .get_mut(&id)
                    .unwrap()
                    .tasks
                    .insert("build".into(), TaskStatus::Failed);
                store(&c, &state);
                error(access(&mut c, &consumer.attempt_id, Some("build"), now));
            } else {
                error(access(&mut c, &consumer.attempt_id, None, now));
            }
        }
    }

    #[test]
    fn always_finalizer_waits_for_terminal_dependencies_and_retains_failure() {
        for outcome in [
            TaskStatus::Failed,
            TaskStatus::Cancelled,
            TaskStatus::Blocked,
        ] {
            let mut c = memory();
            let mut p = plan();
            let mut finalizer = task();
            finalizer.when = TaskWhen::Always;
            finalizer.needs = vec!["build".into(), "dependent".into()];
            let mut dependent = task();
            dependent.needs = vec!["build".into()];
            p.tasks.insert("dependent".into(), dependent);
            p.tasks.insert("finalizer".into(), finalizer);
            let id = submit(&mut c, "finalize", p, false);
            let build = claim(&mut c, 0).unwrap();
            assert_eq!(build.task_id, "build");
            assert!(matches!(
                call(&mut c, &Identity::Worker("two".into()), Request::Claim, 0),
                Response::Assignment { assignment: None }
            ));
            // Exercise every terminal non-success state, including transitive
            // blocking; the scheduler must not confuse blocked with pending.
            let mut state = load(&c.connection).unwrap();
            let run = state.runs.get_mut(&id).unwrap();
            run.attempts.get_mut(&build.attempt_id).unwrap().status = AttemptStatus::Completed;
            run.tasks.insert("build".into(), outcome);
            refresh(run);
            store(&c, &state);
            let finalizer = claim(&mut c, 1).unwrap();
            assert_eq!(finalizer.task_id, "finalizer");
            ok(complete(&mut c, &finalizer.attempt_id, true, 2));
            assert_eq!(inspect(&mut c, &id, 2)["status"], "FAILED");
        }
    }

    #[test]
    fn cancellation_never_starts_always_tasks_or_replays_running_work() {
        let mut c = memory();
        let mut p = plan();
        let mut finalizer = task();
        finalizer.when = TaskWhen::Always;
        finalizer.needs = vec!["build".into()];
        p.tasks.insert("finalizer".into(), finalizer);
        let id = submit(&mut c, "cancel", p, false);
        assert!(matches!(
            register(&mut c, "cancel-instance", 0),
            Response::Heartbeat { .. }
        ));
        let Response::Assignment {
            assignment: Some(build),
        } = session(&mut c, "cancel-instance", Request::Claim, 0)
        else {
            panic!("expected assignment");
        };
        ok(admin(&mut c, Request::Cancel { run_id: id.clone() }, 1));
        assert!(matches!(
            session(&mut c, "cancel-instance", Request::Claim, 1),
            Response::Assignment { assignment: None }
        ));
        assert!(matches!(
            session(
                &mut c,
                "cancel-instance",
                Request::Heartbeat {
                    attempt_id: build.attempt_id.clone()
                },
                1
            ),
            Response::Heartbeat {
                cancel: true,
                lease_until: LEASE_SECS
            }
        ));
        fenced(
            session(
                &mut c,
                "cancel-instance",
                Request::Complete {
                    attempt_id: build.attempt_id.clone(),
                    report: Report {
                        success: false,
                        log: "done".into(),
                    },
                },
                2,
            ),
            &build.attempt_id,
        );
        assert!(matches!(
            session(&mut c, "cancel-instance", Request::Claim, 2),
            Response::Assignment { assignment: None }
        ));
        assert_eq!(inspect(&mut c, &id, 2)["status"], "CANCELLED");
    }

    fn session(c: &mut Coordinator, instance: &str, request: Request, now: u64) -> Response {
        let instance = instance.to_owned();
        let request = match request {
            Request::Claim => Request::ClaimFor { instance },
            Request::Heartbeat { attempt_id } => Request::HeartbeatFor {
                instance,
                attempt_id,
            },
            Request::Complete { attempt_id, report } => Request::CompleteFor {
                instance,
                attempt_id,
                report,
            },
            Request::CompleteGenerated {
                attempt_id,
                report,
                tasks,
                seal,
            } => Request::CompleteGeneratedFor {
                instance,
                attempt_id,
                report,
                tasks,
                seal,
            },
            Request::ArtifactAccess {
                attempt_id,
                producer_task,
            } => Request::ArtifactAccessFor {
                instance,
                attempt_id,
                producer_task,
            },
            // Invalid administrator operations remain unbound and must fail
            // normal worker authorization; there is no generic wrapper bypass.
            request => request,
        };
        call(c, &worker(), request, now)
    }

    fn register(c: &mut Coordinator, instance: &str, now: u64) -> Response {
        call(
            c,
            &worker(),
            Request::RegisterWorker {
                instance: instance.into(),
            },
            now,
        )
    }

    #[test]
    fn instances_cannot_steal_live_attempts_and_fence_legacy_and_old_replays() {
        let database = Database::new();
        let mut c = database.open();
        submit(&mut c, "instances", plan(), false);
        assert!(matches!(
            register(&mut c, "a", 0),
            Response::Heartbeat { .. }
        ));
        let Response::Assignment {
            assignment: Some(a),
        } = session(&mut c, "a", Request::Claim, 0)
        else {
            panic!("expected instance assignment");
        };
        error(register(&mut c, "b", 1));
        error(call(&mut c, &worker(), Request::Claim, 1));
        error(session(&mut c, "b", Request::Claim, 1));
        let heartbeat = Request::Heartbeat {
            attempt_id: a.attempt_id.clone(),
        };
        assert!(matches!(
            session(&mut c, "a", heartbeat.clone(), 30),
            Response::Heartbeat {
                lease_until: 90,
                cancel: false
            }
        ));
        error(register(&mut c, "b", 60));
        drop(c);
        let mut c = database.open();
        error(register(&mut c, "b", 61));
        assert!(matches!(
            register(&mut c, "b", 90),
            Response::Heartbeat { .. }
        ));
        error(session(&mut c, "a", heartbeat, 90));
        let completion = Request::Complete {
            attempt_id: a.attempt_id.clone(),
            report: Report {
                success: true,
                log: String::new(),
            },
        };
        error(session(&mut c, "a", completion.clone(), 90));
        error(session(&mut c, "b", completion, 90));
        let Response::Assignment {
            assignment: Some(b),
        } = session(&mut c, "b", Request::Claim, 90)
        else {
            panic!("expected replacement after expiry");
        };
        assert_ne!(a.attempt_id, b.attempt_id);
        let completion = Request::Complete {
            attempt_id: b.attempt_id,
            report: Report {
                success: true,
                log: String::new(),
            },
        };
        ok(session(&mut c, "b", completion.clone(), 91));
        ok(session(&mut c, "b", completion, 200));
        error(session(
            &mut c,
            "b",
            Request::Cancel { run_id: a.run_id },
            200,
        ));
        error(admin(
            &mut c,
            Request::RegisterWorker {
                instance: "admin".into(),
            },
            200,
        ));
    }

    #[test]
    fn registered_workers_cannot_bypass_instance_checks_on_any_assignment_operation() {
        let mut c = memory();
        submit(&mut c, "bound-generator", generator_plan(), true);
        assert!(matches!(
            register(&mut c, "incarnation", 0),
            Response::Heartbeat { .. }
        ));
        let Response::Assignment {
            assignment: Some(a),
        } = session(&mut c, "incarnation", Request::Claim, 0)
        else {
            panic!("expected generator assignment");
        };
        let operations = [
            Request::Claim,
            Request::Heartbeat {
                attempt_id: a.attempt_id.clone(),
            },
            Request::Complete {
                attempt_id: a.attempt_id.clone(),
                report: Report {
                    success: true,
                    log: String::new(),
                },
            },
            generated_request(&a.attempt_id, true),
            Request::ArtifactAccess {
                attempt_id: a.attempt_id.clone(),
                producer_task: None,
            },
        ];
        let before = serde_json::to_value(load(&c.connection).unwrap()).unwrap();
        for operation in operations {
            error(call(&mut c, &worker(), operation.clone(), 1));
            error(session(&mut c, "wrong-instance", operation, 1));
            assert_eq!(
                serde_json::to_value(load(&c.connection).unwrap()).unwrap(),
                before
            );
        }
        assert!(matches!(
            session(
                &mut c,
                "incarnation",
                Request::ArtifactAccess {
                    attempt_id: a.attempt_id.clone(),
                    producer_task: None
                },
                1
            ),
            Response::Data { .. }
        ));
        ok(session(
            &mut c,
            "incarnation",
            generated_request(&a.attempt_id, true),
            2,
        ));
        ok(session(
            &mut c,
            "incarnation",
            generated_request(&a.attempt_id, true),
            100,
        ));
        error(call(
            &mut c,
            &worker(),
            generated_request(&a.attempt_id, true),
            100,
        ));
        error(session(&mut c, "incarnation", Request::Claim, 100));
    }

    #[test]
    fn registration_cannot_adopt_a_live_legacy_attempt() {
        let mut c = memory();
        submit(&mut c, "legacy", plan(), false);
        let a = claim(&mut c, 0).unwrap();
        error(register(&mut c, "new", 1));
        ok(complete(&mut c, &a.attempt_id, true, 2));
        assert!(matches!(
            register(&mut c, "new", 2),
            Response::Heartbeat { .. }
        ));
        error(complete(&mut c, &a.attempt_id, true, 3));
        error(session(
            &mut c,
            "new",
            Request::Complete {
                attempt_id: a.attempt_id,
                report: Report {
                    success: true,
                    log: String::new(),
                },
            },
            3,
        ));
    }

    #[test]
    fn drain_and_revoke_overlay_survive_restart_and_retain_reservations() {
        for acknowledge in [false, true] {
            let database = Database::new();
            let mut c = database.open();
            let id = submit(&mut c, "overlay", plan(), false);
            let a = claim(&mut c, 0).unwrap();
            ok(admin(
                &mut c,
                Request::SetWorkerMode {
                    worker: "one".into(),
                    mode: WorkerMode::Draining,
                },
                1,
            ));
            assert!(matches!(
                call(
                    &mut c,
                    &worker(),
                    Request::Heartbeat {
                        attempt_id: a.attempt_id.clone()
                    },
                    1
                ),
                Response::Heartbeat { cancel: false, .. }
            ));
            error(call(
                &mut c,
                &worker(),
                Request::SetWorkerMode {
                    worker: "one".into(),
                    mode: WorkerMode::Active,
                },
                1,
            ));
            ok(admin(
                &mut c,
                Request::SetWorkerMode {
                    worker: "one".into(),
                    mode: WorkerMode::Revoked,
                },
                2,
            ));
            drop(c);
            let mut c = database.open();
            assert!(claim(&mut c, 3).is_none());
            assert!(matches!(
                call(
                    &mut c,
                    &worker(),
                    Request::Heartbeat {
                        attempt_id: a.attempt_id.clone()
                    },
                    3
                ),
                Response::Heartbeat {
                    cancel: true,
                    lease_until: 61
                }
            ));
            let state = load(&c.connection).unwrap();
            assert!(matches!(
                state.runs[&id].attempts[&a.attempt_id].status,
                AttemptStatus::Active
            ));
            assert_eq!(state.worker_modes["one"], WorkerMode::Revoked);
            if acknowledge {
                fenced(complete(&mut c, &a.attempt_id, false, 4), &a.attempt_id);
            } else {
                assert!(claim(&mut c, 61).is_none());
            }
            assert!(claim(&mut c, 62).is_none());
            ok(admin(
                &mut c,
                Request::SetWorkerMode {
                    worker: "one".into(),
                    mode: WorkerMode::Draining,
                },
                62,
            ));
            assert!(claim(&mut c, 62).is_none());
            ok(admin(
                &mut c,
                Request::SetWorkerMode {
                    worker: "one".into(),
                    mode: WorkerMode::Active,
                },
                62,
            ));
            assert!(claim(&mut c, 62).is_some());
        }
    }

    #[test]
    fn generation_envelope_allows_cross_platform_without_delegating_privilege() {
        for mode in 0..9 {
            let mut c = memory();
            c.config.repositories.get_mut("repo").unwrap().generation =
                Some(crate::protocol::GenerationPolicy {
                    allowed_labels: BTreeMap::from([(
                        "os".into(),
                        ["macos".into(), "windows".into()].into(),
                    )]),
                    max_cpu: 4,
                    max_memory_mb: 512,
                    max_attempts: 3,
                    max_tasks: 2,
                });
            let mut p = generator_plan();
            p.tasks
                .get_mut("build")
                .unwrap()
                .labels
                .insert("os".into(), "linux".into());
            let id = submit(&mut c, "cross-platform", p, true);
            let a = claim(&mut c, 0).unwrap();
            let before = inspect(&mut c, &id, 0);
            let mut child = task();
            child.needs = vec!["build".into()];
            child.labels.insert("os".into(), "macos".into());
            child.cpu = 4;
            child.memory_mb = 512;
            child.max_attempts = 3;
            match mode {
                1 => {
                    child.labels.clear();
                }
                2 => {
                    child.labels.insert("os".into(), "linux".into());
                }
                3 => {
                    child.labels.insert("privileged".into(), "yes".into());
                }
                4 => child.cpu += 1,
                5 => child.memory_mb += 1,
                6 => child.max_attempts += 1,
                7 => {
                    child.steps[0]
                        .env
                        .insert("GPIPE_TASKS_OUTPUT".into(), "anything".into());
                }
                _ => {}
            }
            let mut windows = child.clone();
            windows.labels.insert("os".into(), "windows".into());
            let mut tasks =
                BTreeMap::from([("mac".into(), child.clone()), ("win".into(), windows)]);
            if mode == 8 {
                tasks.insert("extra".into(), child);
            }
            let request = Request::CompleteGenerated {
                attempt_id: a.attempt_id,
                report: Report {
                    success: true,
                    log: String::new(),
                },
                tasks,
                seal: true,
            };
            if mode == 0 {
                ok(call(&mut c, &worker(), request.clone(), 1));
                ok(call(&mut c, &worker(), request, 2));
                assert!(claim(&mut c, 2).is_none());
                let after = inspect(&mut c, &id, 2);
                assert_eq!(after["plan"]["tasks"]["mac"]["labels"]["os"], "macos");
                assert_eq!(after["plan"]["tasks"]["win"]["labels"]["os"], "windows");
            } else {
                error(call(&mut c, &worker(), request, 1));
                assert_eq!(inspect(&mut c, &id, 1), before);
            }
        }
    }

    #[test]
    fn finalizer_runs_after_exhausted_retries_not_between_attempts() {
        let mut c = memory();
        let mut p = plan();
        let mut finalizer = task();
        finalizer.when = TaskWhen::Always;
        finalizer.needs = vec!["build".into()];
        p.tasks.insert("cleanup".into(), finalizer);
        let id = submit(&mut c, "retry-finalizer", p, false);
        let first = claim(&mut c, 0).unwrap();
        ok(complete(&mut c, &first.attempt_id, false, 1));
        let second = claim(&mut c, 2).unwrap();
        assert_eq!(second.task_id, "build");
        ok(complete(&mut c, &second.attempt_id, false, 3));
        let cleanup = claim(&mut c, 4).unwrap();
        assert_eq!(cleanup.task_id, "cleanup");
        ok(complete(&mut c, &cleanup.attempt_id, true, 5));
        assert_eq!(inspect(&mut c, &id, 5)["status"], "FAILED");
    }

    #[test]
    fn generation_task_limit_is_cumulative_across_generators() {
        let mut c = memory();
        c.config.repositories.get_mut("repo").unwrap().generation =
            Some(crate::protocol::GenerationPolicy {
                allowed_labels: BTreeMap::new(),
                max_cpu: 2,
                max_memory_mb: 128,
                max_attempts: 2,
                max_tasks: 1,
            });
        let mut p = generator_plan();
        p.tasks.insert("other".into(), p.tasks["build"].clone());
        let id = submit(&mut c, "cumulative", p, true);
        let first = claim(&mut c, 0).unwrap();
        ok(call(
            &mut c,
            &worker(),
            generated_request(&first.attempt_id, false),
            1,
        ));
        let child = claim(&mut c, 2).unwrap();
        assert_eq!(child.task_id, "child");
        ok(complete(&mut c, &child.attempt_id, true, 3));
        let other = claim(&mut c, 4).unwrap();
        assert_eq!(other.task_id, "other");
        let before = inspect(&mut c, &id, 4);
        let mut task = task();
        task.needs = vec!["other".into()];
        error(call(
            &mut c,
            &worker(),
            Request::CompleteGenerated {
                attempt_id: other.attempt_id,
                report: Report {
                    success: true,
                    log: String::new(),
                },
                tasks: BTreeMap::from([("second-child".into(), task)]),
                seal: true,
            },
            5,
        ));
        assert_eq!(inspect(&mut c, &id, 5), before);
    }

    #[test]
    fn registration_only_lease_and_admin_reactivation_do_not_bypass_fencing() {
        let mut c = memory();
        assert!(matches!(
            register(&mut c, "a", 10),
            Response::Heartbeat {
                lease_until: 70,
                ..
            }
        ));
        assert!(matches!(
            register(&mut c, "a", 0),
            Response::Heartbeat {
                lease_until: 70,
                ..
            }
        ));
        error(register(&mut c, "b", 69));
        assert!(matches!(
            register(&mut c, "b", 70),
            Response::Heartbeat { .. }
        ));
        submit(&mut c, "sticky", plan(), false);
        let Response::Assignment {
            assignment: Some(a),
        } = session(&mut c, "b", Request::Claim, 70)
        else {
            panic!("expected assignment");
        };
        ok(admin(
            &mut c,
            Request::SetWorkerMode {
                worker: "one".into(),
                mode: WorkerMode::Revoked,
            },
            71,
        ));
        ok(admin(
            &mut c,
            Request::SetWorkerMode {
                worker: "one".into(),
                mode: WorkerMode::Active,
            },
            72,
        ));
        assert!(matches!(
            session(
                &mut c,
                "b",
                Request::Heartbeat {
                    attempt_id: a.attempt_id.clone()
                },
                72
            ),
            Response::Heartbeat {
                cancel: true,
                lease_until: 130
            }
        ));
        assert!(matches!(
            session(&mut c, "b", Request::Claim, 72),
            Response::Assignment { assignment: None }
        ));
        error(register(&mut c, "a", 72));
        assert!(matches!(
            session(
                &mut c,
                "b",
                Request::Complete {
                    attempt_id: a.attempt_id,
                    report: Report {
                        success: false,
                        log: String::new()
                    }
                },
                73
            ),
            Response::Fenced { .. }
        ));
    }

    #[test]
    fn version_one_state_migrates_defaults_and_publishes_downgrade_fence() {
        let database = Database::new();
        let mut c = database.open();
        let id = submit(&mut c, "migration", plan(), false);
        let a = claim(&mut c, 0).unwrap();
        let mut value = serde_json::to_value(load(&c.connection).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("workers");
        value.as_object_mut().unwrap().remove("worker_modes");
        value["runs"][&id]["attempts"][&a.attempt_id]
            .as_object_mut()
            .unwrap()
            .remove("instance_id");
        value["runs"][&id]["plan"]["tasks"]["build"]
            .as_object_mut()
            .unwrap()
            .remove("when");
        value["submissions"]["migration"]["plan"]["tasks"]["build"]
            .as_object_mut()
            .unwrap()
            .remove("when");
        c.connection
            .execute(
                "UPDATE gpipe_state SET version = 1, json = ?1",
                [value.to_string()],
            )
            .unwrap();
        drop(c);
        let mut c = database.open();
        let version: u32 = c
            .connection
            .query_row("SELECT version FROM gpipe_state", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let state = load(&c.connection).unwrap();
        assert!(state.workers.is_empty());
        assert!(state.worker_modes.is_empty());
        assert!(
            state.runs[&id].attempts[&a.attempt_id]
                .instance_id
                .is_none()
        );
        assert_eq!(state.runs[&id].plan.tasks["build"].when, TaskWhen::Success);
        ok(complete(&mut c, &a.attempt_id, true, 1));
    }

    #[test]
    fn admin_artifact_inspection_is_exact_completed_attempt_scoped_and_read_only() {
        for success in [false, true] {
            let mut c = memory();
            let mut p = plan();
            p.tasks.insert("other".into(), task());
            let id = submit(&mut c, "admin-artifact", p, false);
            let other_run = submit(&mut c, "different-run", plan(), false);
            let a = claim(&mut c, 0).unwrap();
            let request = Request::InspectArtifact {
                run_id: id.clone(),
                task_id: "build".into(),
                attempt_id: a.attempt_id.clone(),
            };
            error(admin(&mut c, request.clone(), 0));
            error(call(&mut c, &worker(), request.clone(), 0));
            ok(complete(&mut c, &a.attempt_id, success, 1));
            let before = serde_json::to_value(load(&c.connection).unwrap()).unwrap();
            let data = access_data(admin(&mut c, request.clone(), 2));
            assert_eq!(
                data,
                serde_json::json!({
                    "producer_attempt_id": a.attempt_id, "run_id": id, "task_id": "build",
                })
            );
            assert_eq!(
                serde_json::to_value(load(&c.connection).unwrap()).unwrap(),
                before
            );
            error(call(&mut c, &worker(), request.clone(), 2));
            for (run_id, task_id, attempt_id) in [
                (other_run, "build".to_owned(), a.attempt_id.clone()),
                (id.clone(), "other".to_owned(), a.attempt_id.clone()),
                (id.clone(), "missing".to_owned(), a.attempt_id.clone()),
                (id.clone(), "build".to_owned(), "unknown-attempt".to_owned()),
            ] {
                error(admin(
                    &mut c,
                    Request::InspectArtifact {
                        run_id,
                        task_id,
                        attempt_id,
                    },
                    2,
                ));
            }
            // Even a later run cancellation does not hide a completed diagnostic.
            ok(admin(&mut c, Request::Cancel { run_id: id }, 3));
            assert!(matches!(admin(&mut c, request, 3), Response::Data { .. }));
        }
        let mut c = memory();
        let id = submit(&mut c, "expired-artifact", plan(), false);
        let a = claim(&mut c, 0).unwrap();
        error(admin(
            &mut c,
            Request::InspectArtifact {
                run_id: id,
                task_id: "build".into(),
                attempt_id: a.attempt_id,
            },
            LEASE_SECS,
        ));
    }

    #[test]
    fn unsupported_database_schema_and_counter_exhaustion_fail_closed() {
        let database = Database::new();
        let mut c = database.open();
        c.connection
            .execute("UPDATE gpipe_state SET version = 3", [])
            .unwrap();
        error(admin(&mut c, Request::List, 0));
        drop(c);
        assert!(Coordinator::open(&database.0, config()).is_err());
        let mut c = memory();
        let state = State {
            next_run: u64::MAX,
            ..State::default()
        };
        c.connection
            .execute(
                "UPDATE gpipe_state SET json = ?1",
                [serde_json::to_string(&state).unwrap()],
            )
            .unwrap();
        error(admin(
            &mut c,
            Request::Submit {
                key: "key".into(),
                plan: plan(),
                dynamic: false,
            },
            0,
        ));
        assert!(load(&c.connection).unwrap().runs.is_empty());
    }
}
