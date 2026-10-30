//! Outbound-only native pull worker for administrator-mapped local repositories.
//!
//! This is a trusted-command worker, not a sandbox. Keep its state directory private.
//! A crash leaves `worker.lock`: an administrator must verify the old process and its
//! children are gone before removing it. Pending completion records are never discarded
//! on delivery failure. Commands MUST NOT print secrets: bounded executor output is
//! persisted and transmitted. The worker token is redacted, but arbitrary credentials
//! (including transformed/encoded tokens) cannot be detected reliably.
//! Reports retain the last 16 step stream tails (384 UTF-8 bytes per stream) within
//! a 16 KiB total report cap; reports remain valid JSON when truncated. The
//! worker does not install tools, initialize submodules, or fetch remote objects.
//! Unix spool publication uses file/directory fsync; Windows uses file sync and
//! same-volume `MoveFileExW` `WRITE_THROUGH`. Protect the state directory with an
//! administrator-owned ACL on Windows. Workspaces are retained for inspection.
//!
//! Generator tasks opt in only with an explicit step environment entry
//! `GPIPE_TASKS_OUTPUT=gpipe.tasks.json`. Successful execution reads that regular,
//! non-symlink file (at most 2 MiB) as `{ "tasks": {...}, "seal": bool }` while
//! heartbeat/watchdog monitoring remains active. Its graph and terminal report are
//! durably spooled together (4 MiB spool cap) and delivered as `CompleteGenerated`
//! using only the worker identity. Graph authorization/validation is coordinator
//! owned; invalid/missing output or a lost lease produces plain failed completion.
//! Existing plain completion spool records remain readable.
//!
//! An exact coordinator `Fenced` completion response is NOT successful delivery:
//! pending evidence is synced to exclusive `fenced-<attempt>.json` before clearing
//! pending and continuing. Conflicts, partial archives and symlinks stop for manual
//! recovery; an identical regular archive permits crash recovery without rewriting.
//! `once` still flushes then polls at most once, and returns after one assignment;
//! daemon mode continues after fencing with the same safe network retry rules.
//! A pre-existing attempt workspace is never deleted or reused: it produces an
//! explicit infrastructure failure instead of silently re-executing after a crash.
//!
//! `WorkerConfig::environment` is an explicit administrator opt-in, not ambient
//! environment inheritance. For desktop Rust, configure `HOME`, `RUSTUP_HOME` and
//! `CARGO_HOME` as needed. These values are passed to trusted commands: NEVER put
//! coordinator/provider credentials here. Task environment values override worker
//! defaults. Both are validated without including keys/values in errors. Output
//! redacts the worker token and values under secret-like names, but this heuristic
//! cannot detect arbitrary/encoded secrets; commands must still never print them.
//!
//! Explicit task `GPIPE_ARTIFACTS` declarations use `ArtifactSpec::from_steps`.
//! Inputs download after checkout; successful command outputs upload before any
//! generated graph is read or completion is spooled. `GPIPE_ARTIFACTS_ALWAYS`
//! selects evidence outputs published even after a command/executor failure;
//! failed evidence uploads retain the original typed outcome and stream tails.
//! No graph is delivered on failure. Heartbeats/watchdog cover all phases.
//! Upload cancellation is checked before every chunk, retry and finalization;
//! in-flight HTTP requests and downloads still require coordinator fencing.
//! Requests carry the durable incarnation header for artifact authorization.
//! This module never logs transfer errors or credentials.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::{Url, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};

use crate::protocol::{
    Assignment, Envelope, PROTOCOL_VERSION, Report, Request, Response, WorkerConfig,
};

const RESPONSE_LIMIT: u64 = 4 * 1024 * 1024;
const GENERATED_LIMIT: u64 = 2 * 1024 * 1024;
const SPOOL_LIMIT: u64 = 4 * 1024 * 1024;
const RPC_TIMEOUT: Duration = Duration::from_secs(3);
const TICK: Duration = Duration::from_millis(100);
const CHECKOUT_TIMEOUT: Duration = Duration::from_secs(300);

struct Rpc {
    client: Client,
    endpoint: Url,
    token: String,
    instance_id: Option<String>,
}

impl Rpc {
    fn new(config: &WorkerConfig) -> Result<Self, String> {
        let endpoint = rpc_url(&config.coordinator)?;
        let token = std::env::var(&config.token_env)
            .map_err(|_| "worker token environment variable is unavailable".to_owned())?;
        if token.is_empty() || token.len() > 4096 || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("worker token must be nonempty printable ASCII".into());
        }
        let client = Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .connect_timeout(RPC_TIMEOUT)
            .timeout(RPC_TIMEOUT)
            .build()
            .map_err(|_| "cannot initialize worker HTTP client".to_owned())?;
        Ok(Self {
            client,
            endpoint,
            token,
            instance_id: None,
        })
    }

    fn bind_instance(&mut self, instance_id: String) -> Result<(), String> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            crate::transfer::WORKER_INSTANCE_HEADER,
            instance_id
                .parse()
                .map_err(|_| "invalid worker instance header")?,
        );
        self.client = Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .connect_timeout(RPC_TIMEOUT)
            .timeout(RPC_TIMEOUT)
            .default_headers(headers)
            .build()
            .map_err(|_| "cannot initialize bound worker HTTP client")?;
        self.instance_id = Some(instance_id);
        Ok(())
    }

    fn call(&self, request: &Request, once: bool) -> Result<Response, String> {
        let claim = matches!(request, Request::Claim);
        let mut backoff = 1;
        loop {
            match self.call_once(request.clone()) {
                Ok(response) => return Ok(response),
                Err(failure) if retry_allowed(&failure, claim, once) => {
                    thread::sleep(Duration::from_secs(backoff));
                    backoff = (backoff * 2).min(30);
                }
                Err(failure) => {
                    return Err(if claim && !failure.not_sent {
                        "claim outcome is ambiguous; stopped without retrying; reconcile coordinator state before restarting".into()
                    } else {
                        failure.message.into()
                    });
                }
            }
        }
    }

    fn call_once(&self, request: Request) -> Result<Response, RpcFailure> {
        let request = bind_request(request, self.instance_id.as_deref());
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.token)
            .json(&Envelope {
                version: PROTOCOL_VERSION,
                request,
            })
            .send()
            .map_err(|error| RpcFailure {
                message: "coordinator request failed (details withheld)",
                retryable: error.is_connect()
                    || error.is_timeout()
                    || error.is_body()
                    || error.is_request(),
                not_sent: error.is_connect(),
            })?;
        if !response.status().is_success() {
            return Err(RpcFailure {
                message: "coordinator rejected HTTP request (details withheld)",
                retryable: response.status().is_server_error() || response.status().as_u16() == 429,
                not_sent: false,
            });
        }
        let mut bytes = Vec::new();
        response
            .take(RESPONSE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| RpcFailure {
                message: "cannot read coordinator response",
                retryable: true,
                not_sent: false,
            })?;
        if bytes.len() as u64 > RESPONSE_LIMIT {
            return Err(RpcFailure {
                message: "coordinator response exceeds limit",
                retryable: false,
                not_sent: false,
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| RpcFailure {
            message: "invalid coordinator response",
            retryable: false,
            not_sent: false,
        })
    }
}

fn bind_request(request: Request, instance: Option<&str>) -> Request {
    let Some(instance) = instance else {
        return request;
    };
    let instance = instance.to_owned();
    match request {
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
        request => request,
    }
}

struct RpcFailure {
    message: &'static str,
    retryable: bool,
    not_sent: bool,
}

const fn retry_allowed(failure: &RpcFailure, claim: bool, once: bool) -> bool {
    !once && failure.retryable && (!claim || failure.not_sent)
}

fn rpc_url(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value).map_err(|_| "invalid coordinator URL".to_owned())?;
    let loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.host().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/" | "/v1/rpc")
    {
        return Err("coordinator requires HTTPS (HTTP only on loopback), no credentials, query, fragment or path prefix".into());
    }
    url.set_path("/v1/rpc");
    Ok(url)
}

fn safe_id(id: &str) -> bool {
    let upper = id.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'));
    !reserved
        && !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn lease_millis(until: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::MAX);
    u64::try_from(Duration::from_secs(until).saturating_sub(now).as_millis()).unwrap_or(u64::MAX)
}

fn elapsed_millis(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

struct DirectoryLock(PathBuf);
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn lock(directory: &Path) -> Result<DirectoryLock, String> {
    let path = directory.join("worker.lock");
    let mut file = private_file(&path).map_err(|_| {
        "cannot acquire worker.lock; if it exists, verify the previous worker and children are stopped before manually removing it".to_owned()
    })?;
    let guard = DirectoryLock(path);
    writeln!(file, "{}", std::process::id())
        .and_then(|()| file.sync_all())
        .map_err(|_| "cannot persist worker lock".to_owned())?;
    sync_directory(directory)?;
    Ok(guard)
}

fn private_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn private_directory(path: &Path) -> Result<(), String> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let meta = fs::symlink_metadata(path).map_err(|_| "cannot inspect worker directory")?;
            if meta.is_dir() && !meta.file_type().is_symlink() {
                Ok(())
            } else {
                Err("worker directory must be a real directory".into())
            }
        }
        Err(_) => Err("cannot create worker directory (parent must exist)".into()),
    }
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| "cannot sync worker directory".to_owned())?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    coordinator: String,
    token_env: String,
    attempt_id: String,
    report: Report,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generated: Option<Generated>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Generated {
    tasks: BTreeMap<String, crate::plan::Task>,
    seal: bool,
}

struct Completion {
    report: Report,
    generated: Option<Generated>,
}

impl Pending {
    fn request(self) -> Request {
        match self.generated {
            Some(generated) => Request::CompleteGenerated {
                attempt_id: self.attempt_id,
                report: self.report,
                tasks: generated.tasks,
                seal: generated.seal,
            },
            None => Request::Complete {
                attempt_id: self.attempt_id,
                report: self.report,
            },
        }
    }
}

fn spool(directory: &Path, pending: &Pending) -> Result<(), String> {
    let bytes = serde_json::to_vec(pending).map_err(|_| "cannot encode completion")?;
    if bytes.len() as u64 > SPOOL_LIMIT {
        return Err("completion exceeds spool limit".into());
    }
    if directory
        .join("pending.json")
        .try_exists()
        .map_err(|_| "cannot inspect pending completion")?
    {
        return Err("refusing to replace pending completion".into());
    }
    let temp = directory.join("pending.tmp");
    let mut file = private_file(&temp).map_err(|_| {
        "cannot create pending.tmp; preserve it for manual crash recovery".to_owned()
    })?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "cannot persist completion".to_owned())?;
    publish_completion(&temp, &directory.join("pending.json"))?;
    sync_directory(directory)
}

fn flush(directory: &Path, config: &WorkerConfig, rpc: &Rpc, once: bool) -> Result<(), String> {
    if directory
        .join("pending.tmp")
        .try_exists()
        .map_err(|_| "cannot inspect spool")?
    {
        return Err("pending.tmp requires manual crash recovery before more work".into());
    }
    let path = directory.join("pending.json");
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("cannot open pending completion".into()),
    };
    let mut bytes = Vec::new();
    file.take(SPOOL_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read pending completion")?;
    if bytes.len() as u64 > SPOOL_LIMIT {
        return Err("pending completion exceeds limit".into());
    }
    let pending: Pending =
        serde_json::from_slice(&bytes).map_err(|_| "invalid pending completion")?;
    if pending.coordinator != rpc.endpoint.as_str()
        || pending.token_env != config.token_env
        || !safe_id(&pending.attempt_id)
    {
        return Err("pending completion does not match worker configuration".into());
    }
    if pending.generated.is_some() && !pending.report.success {
        return Err("invalid failed generator completion in spool".into());
    }
    let attempt_id = pending.attempt_id.clone();
    settle_completion(
        directory,
        &attempt_id,
        &bytes,
        rpc.call(&pending.request(), once)?,
    )
}

fn settle_completion(
    directory: &Path,
    pending_attempt: &str,
    bytes: &[u8],
    response: Response,
) -> Result<(), String> {
    match response {
        Response::Ok => {
            fs::remove_file(directory.join("pending.json"))
                .map_err(|_| "cannot remove delivered completion")?;
            sync_directory(directory)
        }
        Response::Fenced { attempt_id }
            if attempt_id == pending_attempt && safe_id(&attempt_id) =>
        {
            archive_fenced(directory, &attempt_id, bytes)?;
            // Never print coordinator data, task output, credentials or attempt IDs.
            eprintln!(
                "warning: completion was fenced, not accepted as success; evidence archived; continuing"
            );
            Ok(())
        }
        _ => Err("completion not acknowledged; retained for retry before any new claim".into()),
    }
}

fn archive_fenced(directory: &Path, attempt_id: &str, bytes: &[u8]) -> Result<(), String> {
    if !safe_id(attempt_id) || bytes.len() as u64 > SPOOL_LIMIT {
        return Err("invalid fenced evidence identity or size".into());
    }
    let archive = directory.join(format!("fenced-{attempt_id}.json"));
    match private_file(&archive) {
        Ok(mut file) => {
            file.write_all(bytes)
                .and_then(|()| file.sync_all())
                .map_err(|_| "cannot persist fenced evidence; pending retained for recovery")?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // Recover a crash after archive sync but before pending removal only
            // when the existing real file contains exactly the same evidence.
            // Partial or conflicting archives require explicit operator recovery.
            let meta =
                fs::symlink_metadata(&archive).map_err(|_| "cannot inspect fenced archive")?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != bytes.len() as u64
            {
                return Err(
                    "fenced archive exists and conflicts; pending retained for manual recovery"
                        .into(),
                );
            }
            let mut options = OpenOptions::new();
            options.read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("fenced archive is a reparse point".into());
                }
                options.custom_flags(0x0020_0000);
            }
            let mut file = options
                .open(&archive)
                .map_err(|_| "cannot safely open fenced archive")?;
            let meta = file
                .metadata()
                .map_err(|_| "cannot inspect open fenced archive")?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err("fenced archive is a reparse point".into());
                }
            }
            if !meta.is_file() {
                return Err("fenced archive is not a regular file".into());
            }
            let mut existing = Vec::new();
            (&mut file)
                .take(SPOOL_LIMIT + 1)
                .read_to_end(&mut existing)
                .map_err(|_| "cannot read fenced archive")?;
            if existing != bytes {
                return Err(
                    "fenced archive conflicts; pending retained for manual recovery".into(),
                );
            }
            file.sync_all().map_err(|_| "cannot sync fenced archive")?;
        }
        Err(_) => return Err("cannot create exclusive fenced archive; pending retained".into()),
    }
    sync_directory(directory)?;
    fs::remove_file(directory.join("pending.json"))
        .map_err(|_| "cannot clear archived pending completion")?;
    sync_directory(directory)
}

/// Local admission checks; no installation, deletion or tool execution is performed.
#[derive(Clone, Debug, Default)]
pub struct WorkerPreflight {
    /// Minimum free bytes required on the workspace filesystem; zero disables.
    pub minimum_free_bytes: u64,
    /// Administrator-approved absolute executable file paths required locally.
    pub tools: Vec<PathBuf>,
}

/// Check local disk and tool paths without coordinator access or filesystem mutation.
///
/// The worker directory must already exist. On platforms without
/// disk querying support a nonzero minimum fails explicitly.
///
/// # Errors
/// * Invalid environment, missing/non-directory workspace, insufficient disk,
///   unavailable disk query, or a missing/nonabsolute/nonexecutable tool path.
pub fn preflight(config: &WorkerConfig, checks: &WorkerPreflight) -> Result<(), String> {
    validate_environment(&config.environment)?;
    let directory =
        fs::canonicalize(&config.directory).map_err(|_| "worker directory unavailable")?;
    if !directory.is_dir() {
        return Err("worker directory must be a directory".into());
    }
    if checks.minimum_free_bytes != 0 && available_bytes(&directory)? < checks.minimum_free_bytes {
        return Err("worker disk free space is below configured minimum".into());
    }
    for tool in &checks.tools {
        if !tool.is_absolute() {
            return Err("preflight tool paths must be absolute".into());
        }
        let metadata = fs::metadata(tool).map_err(|_| "required local tool unavailable")?;
        if !metadata.is_file() {
            return Err("required local tool is not a regular file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err("required local tool is not executable".into());
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn available_bytes(directory: &Path) -> Result<u64, String> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(directory.as_os_str().as_bytes())
        .map_err(|_| "invalid worker directory")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: path is NUL terminated and stats points to writable statvfs storage.
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err("cannot query worker disk free space".into());
    }
    // SAFETY: successful statvfs initialized stats.
    let stats = unsafe { stats.assume_init() };
    let bytes = u128::from(stats.f_bavail) * u128::from(stats.f_frsize);
    Ok(u64::try_from(bytes).unwrap_or(u64::MAX))
}

#[cfg(windows)]
fn available_bytes(directory: &Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let mut path: Vec<u16> = directory.as_os_str().encode_wide().collect();
    if path.contains(&0) {
        return Err("invalid worker directory".into());
    }
    path.push(0);
    let mut available = 0;
    // SAFETY: path is NUL terminated; available is writable. Other outputs are optional.
    if unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("cannot query worker disk free space".into());
    }
    Ok(available)
}

#[cfg(not(any(unix, windows)))]
fn available_bytes(_: &Path) -> Result<u64, String> {
    Err("disk preflight is unsupported on this platform".into())
}

/// Run with explicit local admission checks, repeated before each claim.
///
/// # Errors
/// * Any error documented by [`preflight`] or [`run`].
pub fn run_with_preflight(
    config: &WorkerConfig,
    once: bool,
    checks: &WorkerPreflight,
) -> Result<(), String> {
    run_inner(config, once, checks)
}

/// Run an instance-bound authenticated worker with no optional preflight minimum.
///
/// Replays pending completion under its persisted incarnation before renewing
/// registration. Clean restarts retain that identity; existing lock/crash-recovery
/// files require operator recovery and are never automatically removed. `once`
/// executes at most one assignment.
/// Ambiguous claims are never replayed. Commands must not print credentials.
///
/// # Errors
/// * Invalid configuration/token/directory, exclusive lock or recovery conflict.
/// * Registration, RPC, checkout, durable completion or assignment validation fails.
pub fn run(config: &WorkerConfig, once: bool) -> Result<(), String> {
    run_inner(config, once, &WorkerPreflight::default())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstanceRecord {
    coordinator: String,
    token_env: String,
    instance_id: String,
}

fn restore_instance(directory: &Path, config: &WorkerConfig, rpc: &mut Rpc) -> Result<(), String> {
    let path = directory.join("worker-instance.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("cannot inspect persisted worker instance".into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 4096 {
        return Err("invalid persisted worker instance; explicit recovery required".into());
    }
    let record: InstanceRecord = serde_json::from_reader(
        File::open(path)
            .map_err(|_| "cannot open worker instance")?
            .take(4097),
    )
    .map_err(|_| "invalid persisted worker instance")?;
    if record.coordinator != rpc.endpoint.as_str()
        || record.token_env != config.token_env
        || record.instance_id.is_empty()
        || record.instance_id.len() > 256
        || record.instance_id.chars().any(char::is_control)
    {
        return Err("persisted worker instance does not match configuration".into());
    }
    rpc.bind_instance(record.instance_id)
}

fn create_instance(directory: &Path, config: &WorkerConfig, rpc: &mut Rpc) -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "invalid system clock")?
        .as_nanos();
    let instance_id = format!("{}-{now}", std::process::id());
    let record = InstanceRecord {
        coordinator: rpc.endpoint.to_string(),
        token_env: config.token_env.clone(),
        instance_id: instance_id.clone(),
    };
    let temporary = directory.join("worker-instance.tmp");
    let mut file = private_file(&temporary)
        .map_err(|_| "worker-instance.tmp requires explicit crash recovery")?;
    serde_json::to_writer(&mut file, &record).map_err(|_| "cannot persist worker instance")?;
    file.sync_all().map_err(|_| "cannot sync worker instance")?;
    fs::rename(temporary, directory.join("worker-instance.json"))
        .map_err(|_| "cannot publish worker instance")?;
    sync_directory(directory)?;
    rpc.bind_instance(instance_id)
}

fn register_instance(rpc: &Rpc, once: bool) -> Result<(), String> {
    let instance_id = rpc
        .instance_id
        .clone()
        .ok_or("worker instance is missing")?;
    match rpc.call(
        &Request::RegisterWorker {
            instance: instance_id,
        },
        once,
    )? {
        Response::Heartbeat {
            cancel: false,
            lease_until,
        } if lease_millis(lease_until) > 0 => Ok(()),
        _ => Err("worker registration rejected or expired".into()),
    }
}

fn run_inner(config: &WorkerConfig, once: bool, checks: &WorkerPreflight) -> Result<(), String> {
    validate_environment(&config.environment)?;
    let mut rpc = Rpc::new(config)?;
    let directory = Path::new(&config.directory);
    private_directory(directory)?;
    let directory = fs::canonicalize(directory).map_err(|_| "cannot resolve worker directory")?;
    let _lock = lock(&directory)?;
    if directory
        .join("worker-instance.tmp")
        .try_exists()
        .map_err(|_| "cannot inspect instance recovery file")?
    {
        return Err("worker-instance.tmp requires explicit crash recovery".into());
    }
    restore_instance(&directory, config, &mut rpc)?;
    // Replay pending evidence under the exact durable incarnation first. A
    // clean restart retains that identity: replacing it would conflict with
    // its still-live registration lease, even after terminal acknowledgement.
    // The exclusive lock prevents concurrent owners; retained attempt workspace
    // checks still prevent a replayed active assignment from executing twice.
    flush(&directory, config, &rpc, once)?;
    if rpc.instance_id.is_none() {
        create_instance(&directory, config, &mut rpc)?;
    }
    private_directory(&directory.join("attempts"))?;
    private_directory(&directory.join("empty-template"))?;
    loop {
        flush(&directory, config, &rpc, once)?;
        preflight(config, checks)?;
        register_instance(&rpc, once)?;
        let Response::Assignment { assignment } = rpc.call(&Request::Claim, once)? else {
            return Err("unexpected claim response".into());
        };
        if let Some(assignment) = assignment {
            if !safe_id(&assignment.attempt_id) {
                return Err("unsafe assignment attempt identity".into());
            }
            let completion = perform(config, &directory, &rpc, &assignment, once);
            spool(
                &directory,
                &Pending {
                    coordinator: rpc.endpoint.to_string(),
                    token_env: config.token_env.clone(),
                    attempt_id: assignment.attempt_id,
                    report: completion.report,
                    generated: completion.generated,
                },
            )?;
            flush(&directory, config, &rpc, once)?;
            if once {
                return Ok(());
            }
        } else {
            if once {
                return Ok(());
            }
            // Keep this incarnation live even when the configured idle poll is
            // longer than the coordinator's registration lease. Failure stops
            // rather than silently running under an expired registration.
            let mut remaining = config.poll_secs.clamp(1, 300);
            while remaining > 0 {
                let interval = remaining.min(10);
                thread::sleep(Duration::from_secs(interval));
                remaining -= interval;
                register_instance(&rpc, true)?;
            }
        }
    }
}

fn perform(
    config: &WorkerConfig,
    directory: &Path,
    rpc: &Rpc,
    assignment: &Assignment,
    once: bool,
) -> Completion {
    let start = Instant::now();
    let initial_lease = lease_millis(assignment.lease_until);
    let cancelled = AtomicBool::new(initial_lease == 0);
    let stopped = AtomicBool::new(false);
    let deadline = AtomicU64::new(initial_lease);
    let result = thread::scope(|scope| {
        // Separate watchdog: a stalled heartbeat request must not extend execution.
        scope.spawn(|| {
            while !stopped.load(Ordering::Acquire) {
                if elapsed_millis(start) >= deadline.load(Ordering::Acquire) {
                    cancelled.store(true, Ordering::Release);
                    break;
                }
                thread::sleep(TICK);
            }
        });
        scope.spawn(|| {
            let mut backoff = 1_u32;
            while !stopped.load(Ordering::Acquire) && !cancelled.load(Ordering::Acquire) {
                match rpc.call(
                    &Request::Heartbeat {
                        attempt_id: assignment.attempt_id.clone(),
                    },
                    true,
                ) {
                    Ok(Response::Heartbeat {
                        cancel: false,
                        lease_until,
                    }) if elapsed_millis(start) < deadline.load(Ordering::Acquire)
                        && lease_millis(lease_until) > 0 =>
                    {
                        backoff = 1;
                        deadline.store(
                            elapsed_millis(start).saturating_add(lease_millis(lease_until)),
                            Ordering::Release,
                        );
                    }
                    Ok(_) => {
                        cancelled.store(true, Ordering::Release);
                        break;
                    }
                    Err(_) if once => {
                        cancelled.store(true, Ordering::Release);
                        break;
                    }
                    Err(_) => {
                        backoff = (backoff * 2).min(30);
                    } // Existing lease remains the limit.
                }
                for _ in 0..(10 * backoff) {
                    if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    thread::sleep(TICK);
                }
            }
        });
        let result = checkout_and_execute(config, directory, assignment, &cancelled, rpc);
        stopped.store(true, Ordering::Release);
        result
    });
    if cancelled.load(Ordering::Acquire)
        || elapsed_millis(start) >= deadline.load(Ordering::Acquire)
    {
        return match result {
            Ok(mut completion) => {
                completion.generated = None;
                completion.report.success = false;
                completion.report.log =
                    annotate_log(&completion.report.log, "attempt cancelled or lease lost");
                completion
            }
            Err(_) => Completion {
                report: Report {
                    success: false,
                    log: "attempt cancelled or lease lost".into(),
                },
                generated: None,
            },
        };
    }
    match result {
        Ok(report) => report,
        Err(message) => Completion {
            report: Report {
                success: false,
                log: message,
            },
            generated: None,
        },
    }
}

fn checkout_and_execute(
    config: &WorkerConfig,
    directory: &Path,
    assignment: &Assignment,
    cancelled: &AtomicBool,
    rpc: &Rpc,
) -> Result<Completion, String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("attempt cancelled or lease lost".into());
    }
    if !matches!(assignment.revision.len(), 40 | 64)
        || !assignment.revision.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("assignment must specify a full hexadecimal commit SHA".into());
    }
    let source = config
        .repositories
        .get(&assignment.repository)
        .ok_or("repository is not mapped on this worker")?;
    let source = fs::canonicalize(source).map_err(|_| "mapped local source is unavailable")?;
    if !source.is_dir() {
        return Err("mapped local source must be a directory".into());
    }
    let workspace = directory.join("attempts").join(&assignment.attempt_id);
    // Never reuse a partial or previously executed attempt workspace.
    fs::create_dir(&workspace)
        .map_err(|_| "attempt workspace already exists or cannot be created")?;
    checkout(
        &source,
        &workspace,
        &directory.join("empty-template"),
        &assignment.revision,
        cancelled,
    )?;
    if cancelled.load(Ordering::Acquire) {
        return Err("attempt cancelled or lease lost".into());
    }
    let spec = crate::transfer::ArtifactSpec::from_steps(&assignment.task.steps)
        .map_err(|_| "invalid or conflicting explicit artifact declarations".to_owned())?;
    let always = crate::transfer::ArtifactSpec::from_always_steps(&assignment.task.steps)
        .map_err(|_| "invalid or conflicting failure evidence declarations".to_owned())?;
    validate_output_policies(&spec, &always)?;
    let steps = merge_environment(&assignment.task.steps, &config.environment)?;
    let secrets = output_secrets(&rpc.token, &config.environment, &assignment.task.steps);
    let transfer =
        |upload: bool| transfer_artifacts(rpc, assignment, &workspace, &spec, cancelled, upload);
    Ok(artifact_execution_with_evidence(
        &assignment.task,
        &workspace,
        cancelled,
        || transfer(false),
        || {
            crate::executor::execute(&steps, &workspace, cancelled)
                .map(|result| execution_report(&result, &secrets))
                .map_err(|_| "executor failed (details withheld)".to_owned())
        },
        || transfer(true),
        || transfer_artifacts(rpc, assignment, &workspace, &always, cancelled, true),
    ))
}

fn validate_output_policies(
    normal: &crate::transfer::ArtifactSpec,
    always: &crate::transfer::ArtifactSpec,
) -> Result<(), String> {
    normal.validate()?;
    always.validate()?;
    let mut combined = normal.clone();
    for output in &always.outputs {
        // The same immutable artifact may be declared under both policies.
        // Upload replay is checksum-verified; different paths for one name
        // remain a conflict rather than silently selecting either source.
        if !combined.outputs.contains(output) {
            combined.outputs.push(output.clone());
        }
    }
    combined
        .validate()
        .map_err(|_| "conflicting artifact output names".to_owned())
}

fn transfer_artifacts(
    rpc: &Rpc,
    assignment: &Assignment,
    workspace: &Path,
    spec: &crate::transfer::ArtifactSpec,
    cancelled: &AtomicBool,
    upload: bool,
) -> Result<(), String> {
    // Upload cancellation is checked before every chunk/finalization; downloads
    // remain bounded by one file transfer plus its HTTP timeout.
    let count = if upload {
        spec.outputs.len()
    } else {
        spec.inputs.len()
    };
    let mut failed = false;
    for index in 0..count {
        if cancelled.load(Ordering::Acquire) {
            return Err("attempt cancelled or lease lost".into());
        }
        let single = if upload {
            crate::transfer::ArtifactSpec {
                inputs: vec![],
                outputs: vec![spec.outputs[index].clone()],
            }
        } else {
            crate::transfer::ArtifactSpec {
                inputs: vec![spec.inputs[index].clone()],
                outputs: vec![],
            }
        };
        let result = if upload {
            crate::transfer::upload_guarded(
                &rpc.client,
                rpc.endpoint.as_str(),
                &rpc.token,
                &assignment.attempt_id,
                workspace,
                &single,
                || cancelled.load(Ordering::Acquire),
            )
        } else {
            crate::transfer::download(
                &rpc.client,
                rpc.endpoint.as_str(),
                &rpc.token,
                &assignment.attempt_id,
                workspace,
                &single,
            )
        };
        if result.is_err() {
            if !upload {
                return Err("artifact download failed (details withheld)".to_owned());
            }
            // A missing receipt must not suppress remaining stage logs/coverage.
            failed = true;
        }
    }
    if cancelled.load(Ordering::Acquire) {
        return Err("attempt cancelled or lease lost".into());
    }
    if failed {
        return Err("artifact upload failed (details withheld)".to_owned());
    }
    Ok(())
}

#[cfg(test)]
fn artifact_execution(
    task: &crate::plan::Task,
    workspace: &Path,
    cancelled: &AtomicBool,
    download: impl FnOnce() -> Result<(), String>,
    execute: impl FnOnce() -> Result<Report, String>,
    upload: impl FnOnce() -> Result<(), String>,
) -> Completion {
    artifact_execution_with_evidence(
        task,
        workspace,
        cancelled,
        download,
        execute,
        upload,
        || Ok(()),
    )
}

fn artifact_execution_with_evidence(
    task: &crate::plan::Task,
    workspace: &Path,
    cancelled: &AtomicBool,
    download: impl FnOnce() -> Result<(), String>,
    execute: impl FnOnce() -> Result<Report, String>,
    upload: impl FnOnce() -> Result<(), String>,
    evidence: impl FnOnce() -> Result<(), String>,
) -> Completion {
    let failed = |message: &str| Completion {
        report: Report {
            success: false,
            log: message.into(),
        },
        generated: None,
    };
    if cancelled.load(Ordering::Acquire) {
        return failed("attempt cancelled or lease lost");
    }
    if download().is_err() {
        return failed("artifact download failed (details withheld)");
    }
    if cancelled.load(Ordering::Acquire) {
        return failed("attempt cancelled or lease lost");
    }
    let mut report =
        execute().unwrap_or_else(|_| failed("executor failed (details withheld)").report);
    // Evidence is attempted even after an executor infrastructure error, but
    // cancellation never grants permission to publish stale attempt data.
    let command_succeeded = report.success;
    if !cancelled.load(Ordering::Acquire) && evidence().is_err() {
        report.success = false;
        report.log = annotate_log(
            &report.log,
            "failure evidence upload failed (details withheld)",
        );
    }
    if command_succeeded && !cancelled.load(Ordering::Acquire) && upload().is_err() {
        report.success = false;
        report.log = annotate_log(&report.log, "artifact upload failed (details withheld)");
    }
    if cancelled.load(Ordering::Acquire) {
        report.success = false;
        report.log = annotate_log(&report.log, "attempt cancelled or lease lost");
    }
    finish_execution(report, task, workspace)
}

fn validate_environment(environment: &BTreeMap<String, String>) -> Result<(), String> {
    if environment.len() > crate::plan::MAX_MAP_ENTRIES {
        return Err("too many explicitly configured environment entries".into());
    }
    for (key, value) in environment {
        let mut bytes = key.bytes();
        if !bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || key.len() > crate::plan::MAX_TEXT_BYTES
            || value.len() > crate::plan::MAX_TEXT_BYTES
            || value.contains('\0')
        {
            return Err("invalid explicit environment name or value (details withheld)".into());
        }
    }
    Ok(())
}

fn merge_environment(
    steps: &[crate::plan::CommandStep],
    environment: &BTreeMap<String, String>,
) -> Result<Vec<crate::plan::CommandStep>, String> {
    validate_environment(environment)?;
    steps
        .iter()
        .map(|step| {
            validate_environment(&step.env)?;
            let mut merged = step.clone();
            merged.env = environment.clone();
            for (key, value) in &step.env {
                // Windows environment names are case insensitive. Task overrides must
                // not depend on the sorted map's lexical insertion order.
                #[cfg(windows)]
                merged
                    .env
                    .retain(|existing, _| !existing.eq_ignore_ascii_case(key));
                merged.env.insert(key.clone(), value.clone());
            }
            validate_environment(&merged.env)?;
            Ok(merged)
        })
        .collect()
}

fn secret_name(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "API_KEY",
        "APIKEY",
        "PRIVATE_KEY",
        "ACCESS_KEY",
    ]
    .iter()
    .any(|part| key.contains(part))
}

fn output_secrets<'a>(
    token: &'a str,
    environment: &'a BTreeMap<String, String>,
    steps: &'a [crate::plan::CommandStep],
) -> Vec<&'a str> {
    let mut secrets = vec![token];
    secrets.extend(
        environment
            .iter()
            .chain(steps.iter().flat_map(|step| step.env.iter()))
            .filter(|(key, value)| secret_name(key) && !value.is_empty())
            .map(|(_, value)| value.as_str()),
    );
    // Longest first prevents a shorter secret hiding only part of a longer one.
    secrets.sort_by_key(|value| std::cmp::Reverse(value.len()));
    secrets.dedup();
    secrets
}

fn redact_secrets(text: &str, secrets: &[&str], truncated: bool) -> String {
    secrets.iter().fold(text.to_owned(), |text, secret| {
        redact_output(&text, secret, truncated)
    })
}

fn finish_execution(mut report: Report, task: &crate::plan::Task, workspace: &Path) -> Completion {
    let generator = task.steps.iter().any(|step| {
        step.env
            .get("GPIPE_TASKS_OUTPUT")
            .is_some_and(|value| value == "gpipe.tasks.json")
    });
    let generated = if generator && report.success {
        match read_generated(workspace) {
            Ok(generated) => Some(generated),
            Err(message) => {
                report.success = false;
                report.log = annotate_log(&report.log, &message);
                None
            }
        }
    } else {
        None
    };
    Completion { report, generated }
}

fn read_generated(workspace: &Path) -> Result<Generated, String> {
    let path = workspace.join("gpipe.tasks.json");
    let metadata =
        fs::symlink_metadata(&path).map_err(|_| "generator output is missing or inaccessible")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("generator output must be a regular non-symlink file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Do not follow a raced symlink or block opening a raced FIFO.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        if metadata.file_attributes() & 0x400 != 0 {
            // FILE_ATTRIBUTE_REPARSE_POINT
            return Err("generator output must not be a reparse point".into());
        }
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options
        .open(&path)
        .map_err(|_| "cannot safely open generator output")?;
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect generator output")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("generator output must not be a reparse point".into());
        }
    }
    if !metadata.is_file() || metadata.len() > GENERATED_LIMIT {
        return Err("generator output must be a regular file at most 2 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(GENERATED_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read generator output")?;
    if bytes.len() as u64 > GENERATED_LIMIT {
        return Err("generator output exceeds 2 MiB".into());
    }
    // Never include parser errors or file contents: task environments may contain secrets.
    serde_json::from_slice(&bytes).map_err(|_| "invalid generator output JSON schema".into())
}

fn git(template: &Path) -> Command {
    // Resolve Git using the administrator's PATH before clearing the child
    // environment (Git is commonly outside /usr/bin on Nix systems).
    let executable = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|path| path.join(if cfg!(windows) { "git.exe" } else { "git" }))
                .find(|path| path.is_absolute() && path.is_file())
        })
        .unwrap_or_else(|| {
            PathBuf::from(if cfg!(windows) {
                "git.exe"
            } else {
                "/usr/bin/git"
            })
        });
    let mut command = Command::new(executable);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .env("GIT_TEMPLATE_DIR", template)
        .arg("-c")
        .arg(format!("core.hooksPath={}", template.display()))
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "credential.helper=",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.file.allow=always",
            "-c",
            "submodule.recurse=false",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
    }
    command
}

const fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

fn checkout(
    source: &Path,
    workspace: &Path,
    template: &Path,
    revision: &str,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    let mut clone = git(template);
    clone
        .args(["clone", "--no-hardlinks", "--no-checkout", "--"])
        .arg(source)
        .arg(workspace);
    git_wait(&mut clone, cancelled)?;
    // Empty global/system configuration, an empty template and no inherited env
    // prevent administrator/source checkout hooks and credential helpers executing.
    let mut verify = git(template);
    verify
        .arg("-C")
        .arg(workspace)
        .args(["cat-file", "-e", &format!("{revision}^{{commit}}")]);
    git_wait(&mut verify, cancelled)?;
    let mut detach = git(template);
    detach
        .arg("-C")
        .arg(workspace)
        .args(["checkout", "--detach", "--force", revision, "--"]);
    git_wait(&mut detach, cancelled)?;
    // Annotated tag SHAs must not be accepted as an "exact" commit revision.
    let head = fs::read_to_string(workspace.join(".git/HEAD"))
        .map_err(|_| "cannot verify detached checkout")?;
    if !head.trim().eq_ignore_ascii_case(revision) {
        return Err("checkout does not match exact requested commit SHA".into());
    }
    Ok(())
}

fn git_wait(command: &mut Command, cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        return Err("attempt cancelled or lease lost".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0004); // CREATE_SUSPENDED
    }
    let mut child = command.spawn().map_err(|_| "cannot launch local git")?;
    #[cfg(windows)]
    let _job = match windows_checkout::Job::attach(&child) {
        Ok(job) => job,
        Err(()) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cannot contain Git process in Windows Job Object".into());
        }
    };
    let start = Instant::now();
    loop {
        if cancelled.load(Ordering::Acquire) || start.elapsed() >= CHECKOUT_TIMEOUT {
            #[cfg(unix)]
            {
                // SAFETY: child is the leader of a new process group created above.
                unsafe {
                    libc::kill(
                        -i32::try_from(child.id()).unwrap_or(i32::MAX),
                        libc::SIGKILL,
                    );
                }
            }
            #[cfg(windows)]
            _job.kill();
            let _ = child.kill();
            let _ = child.wait();
            return Err("checkout cancelled, lease lost, or checkout timeout exceeded".into());
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("local git checkout failed (details withheld)".into()),
            Ok(None) => thread::sleep(TICK),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("cannot monitor local git".into());
            }
        }
    }
}

/// Preserve both stream tails from the final 16 steps (384 bytes each).
/// This avoids a large early stdout consuming the report before stderr is retained.
fn execution_report(result: &crate::executor::ExecutionResult, secrets: &[&str]) -> Report {
    let omitted = result.steps.len().saturating_sub(16);
    let steps: Vec<_> = result
        .steps
        .iter()
        .skip(omitted)
        .map(|step| {
            let tail = |text: &str| {
                let text = redact_secrets(text, secrets, false);
                let mut start = text.len().saturating_sub(384);
                while !text.is_char_boundary(start) {
                    start += 1;
                }
                text[start..].to_owned()
            };
            serde_json::json!({
                "step": step.step_index, "success": step.success,
                "outcome": step.outcome, "exit_code": step.exit_code,
                "stdout_complete": step.stdout.complete, "stderr_complete": step.stderr.complete,
                "stdout": tail(&step.stdout.text), "stderr": tail(&step.stderr.text),
                "stdout_truncated": step.stdout.truncated || step.stdout.text.len() > 384,
                "stderr_truncated": step.stderr.truncated || step.stderr.text.len() > 384,
            })
        })
        .collect();
    let value = serde_json::json!({"success": result.success, "outcome": result.outcome,
        "exit_code": result.exit_code, "omitted_steps": omitted, "steps": steps});
    Report {
        success: result.success,
        log: bounded_report_json(value),
    }
}

// Bound the serialized representation, not raw text: JSON escaping can expand
// each byte sixfold. Never cut a serialized JSON document in the middle.
fn bounded_report_json(mut value: serde_json::Value) -> String {
    loop {
        let text = serde_json::to_string(&value).expect("JSON value serialization");
        if text.len() <= LOG_LIMIT {
            return text;
        }
        value["report_truncated"] = true.into();
        let mut reduced = false;
        if let Some(steps) = value["steps"].as_array_mut() {
            for step in steps {
                for stream in ["stdout", "stderr"] {
                    if let Some(text) = step[stream].as_str().filter(|text| !text.is_empty()) {
                        let mut start = text.len() / 2 + 1;
                        while !text.is_char_boundary(start) {
                            start += 1;
                        }
                        step[stream] = text[start..].into();
                        step[format!("{stream}_truncated")] = true.into();
                        reduced = true;
                    }
                }
            }
        }
        if !reduced {
            // Metadata here is bounded (16 steps and fixed diagnostic messages).
            return serde_json::json!({"success": value["success"],
                "outcome": value["outcome"], "exit_code": value["exit_code"],
                "report_truncated": true})
            .to_string();
        }
    }
}

fn annotate_log(log: &str, message: &str) -> String {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(log)
        && value.is_object()
    {
        let diagnostics = value
            .as_object_mut()
            .expect("object")
            .entry("diagnostics")
            .or_insert_with(|| serde_json::json!([]));
        if let Some(items) = diagnostics.as_array_mut() {
            items.push(message.into());
        }
        return bounded_report_json(value);
    }
    bounded_log(&format!("{message}\n{log}"), false)
}

const LOG_LIMIT: usize = 16 * 1024;

#[cfg(test)]
#[derive(Default)]
struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}
#[cfg(test)]
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let remaining = LOG_LIMIT.saturating_sub(self.bytes.len());
        let count = remaining.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..count]);
        if count != bytes.len() {
            self.truncated = true;
            return Err(std::io::Error::other("bounded executor report"));
        }
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn redact_output(text: &str, token: &str, truncated: bool) -> String {
    let encoded = serde_json::to_string(token).unwrap_or_default();
    let escaped = encoded
        .get(1..encoded.len().saturating_sub(1))
        .unwrap_or(token);
    let mut text = text.to_owned();
    for secret in [token, escaped] {
        if secret.is_empty() {
            continue;
        }
        if truncated {
            // Withhold a truncated credential prefix at the serialization boundary.
            for length in (1..secret.len()).rev() {
                if secret.is_char_boundary(length) && text.ends_with(&secret[..length]) {
                    text.truncate(text.len() - length);
                    text.push_str("[REDACTED]");
                    break;
                }
            }
        }
        text = text.replace(secret, "[REDACTED]");
    }
    text
}

fn bounded_log(text: &str, truncated: bool) -> String {
    let mut end = text.len().min(LOG_LIMIT - 32);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut log = text[..end].to_owned();
    if truncated || end < text.len() {
        log.push_str("\n[output truncated]");
    }
    log
}

fn publish_completion(source: &Path, target: &Path) -> Result<(), String> {
    #[cfg(not(windows))]
    fs::rename(source, target).map_err(|_| "cannot publish completion".to_owned())?;
    #[cfg(windows)]
    windows_checkout::publish(source, target)?;
    Ok(())
}

#[cfg(windows)]
mod windows_checkout {
    use std::{
        ffi::c_void,
        os::windows::{ffi::OsStrExt, io::AsRawHandle},
        path::Path,
        process::Child,
        ptr,
    };
    type Handle = *mut c_void;
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimit {
        process_time: i64,
        job_time: i64,
        flags: u32,
        minimum_working_set: usize,
        maximum_working_set: usize,
        active_process_limit: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io_counters: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: i32,
            information: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtResumeProcess(process: Handle) -> i32;
    }

    pub struct Job(Handle);
    impl Job {
        pub fn attach(child: &Child) -> Result<Self, ()> {
            // SAFETY: unnamed job with default security; Job owns its handle.
            let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if handle.is_null() {
                return Err(());
            }
            let job = Self(handle);
            let mut limits = ExtendedLimit::default();
            limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            let size = u32::try_from(std::mem::size_of::<ExtendedLimit>()).map_err(|_| ())?;
            // SAFETY: valid job/process handles and correctly sized initialized structure.
            if unsafe { SetInformationJobObject(handle, 9, (&raw const limits).cast(), size) } == 0
                || unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) } == 0
            {
                return Err(());
            }
            // SAFETY: child was created suspended and cannot spawn before assignment.
            if unsafe { NtResumeProcess(child.as_raw_handle()) } < 0 {
                return Err(());
            }
            Ok(job)
        }
        pub fn kill(&self) {
            // SAFETY: self owns a valid job handle.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: close the sole owned handle, terminating any remaining descendants.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    pub fn publish(source: &Path, target: &Path) -> Result<(), String> {
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: NUL-terminated paths live through this call. WRITE_THROUGH (8)
        // waits for the move to reach disk; no REPLACE_EXISTING, no cross-volume move.
        if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), 8) } == 0 {
            return Err("cannot durably publish completion".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generator_task(marker: Option<&str>) -> crate::plan::Task {
        crate::plan::Task {
            when: crate::plan::TaskWhen::default(),
            needs: vec![],
            labels: BTreeMap::new(),
            cpu: 1,
            memory_mb: 0,
            max_attempts: 1,
            steps: vec![crate::plan::CommandStep {
                shell: "sh".into(),
                run: "true".into(),
                timeout_secs: 1,
                env: marker
                    .map(|value| BTreeMap::from([("GPIPE_TASKS_OUTPUT".into(), value.into())]))
                    .unwrap_or_default(),
            }],
        }
    }

    #[test]
    fn requests_bind_to_actual_typed_instance_operations() {
        assert!(matches!(bind_request(Request::Claim, Some("boot")),
            Request::ClaimFor { instance } if instance == "boot"));
        assert!(
            matches!(bind_request(Request::Heartbeat { attempt_id: "a".into() }, Some("boot")),
            Request::HeartbeatFor { instance, attempt_id } if instance == "boot" && attempt_id == "a")
        );
        assert!(
            matches!(bind_request(Request::Complete { attempt_id: "a".into(),
            report: Report { success: false, log: "failed".into() } }, Some("boot")),
            Request::CompleteFor { instance, report, .. } if instance == "boot" && !report.success)
        );
        assert!(
            matches!(bind_request(Request::ArtifactAccess { attempt_id: "a".into(), producer_task: None }, Some("boot")),
            Request::ArtifactAccessFor { instance, .. } if instance == "boot")
        );
        assert!(matches!(bind_request(Request::Claim, None), Request::Claim));
    }

    fn local_config(directory: &Path) -> WorkerConfig {
        WorkerConfig {
            coordinator: "https://example.invalid/v1/rpc".into(),
            token_env: "TOKEN".into(),
            directory: directory.to_str().unwrap().into(),
            repositories: BTreeMap::new(),
            environment: BTreeMap::new(),
            poll_secs: 1,
        }
    }

    fn local_rpc() -> Rpc {
        Rpc {
            client: Client::builder().no_proxy().build().unwrap(),
            endpoint: rpc_url("https://example.invalid").unwrap(),
            token: "test-token".into(),
            instance_id: None,
        }
    }

    #[test]
    fn preflight_checks_tools_and_disk_without_installing_or_removing() {
        let temp = tempfile::tempdir().unwrap();
        let config = local_config(temp.path());
        assert!(preflight(&config, &WorkerPreflight::default()).is_ok());
        assert!(
            preflight(
                &config,
                &WorkerPreflight {
                    minimum_free_bytes: u64::MAX,
                    tools: vec![]
                }
            )
            .is_err()
        );
        assert!(
            preflight(
                &config,
                &WorkerPreflight {
                    minimum_free_bytes: 0,
                    tools: vec!["relative/tool".into()]
                }
            )
            .is_err()
        );
        assert!(
            preflight(
                &config,
                &WorkerPreflight {
                    minimum_free_bytes: 0,
                    tools: vec![temp.path().join("missing")]
                }
            )
            .is_err()
        );
        assert!(!temp.path().join("missing").exists());
        fs::write(temp.path().join("keep"), "evidence").unwrap();
        assert!(
            preflight(
                &config,
                &WorkerPreflight {
                    minimum_free_bytes: u64::MAX,
                    tools: vec![]
                }
            )
            .is_err()
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("keep")).unwrap(),
            "evidence"
        );
    }

    #[test]
    fn instance_records_restore_exact_replay_identity_and_deny_other_config() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = local_config(temp.path());
        let mut original = local_rpc();
        create_instance(temp.path(), &config, &mut original).unwrap();
        let mut restored = local_rpc();
        restore_instance(temp.path(), &config, &mut restored).unwrap();
        assert_eq!(restored.instance_id, original.instance_id);
        config.token_env = "OTHER_TOKEN".into();
        assert!(restore_instance(temp.path(), &config, &mut restored).is_err());
        fs::write(temp.path().join("worker.lock"), "stale-owner").unwrap();
        assert!(lock(temp.path()).is_err());
        assert_eq!(
            fs::read_to_string(temp.path().join("worker.lock")).unwrap(),
            "stale-owner"
        );
    }

    #[test]
    fn artifact_policies_share_only_exact_output_declarations() {
        use crate::transfer::{ArtifactOutput, ArtifactSpec};
        let normal = ArtifactSpec {
            inputs: vec![],
            outputs: vec![ArtifactOutput {
                name: "receipt".into(),
                path: "evidence/receipt.json".into(),
            }],
        };
        assert!(validate_output_policies(&normal, &normal).is_ok());
        let mut different_path = normal.clone();
        different_path.outputs[0].path = "other/receipt.json".into();
        assert!(validate_output_policies(&normal, &different_path).is_err());
        let mut internally_duplicated = normal.clone();
        internally_duplicated
            .outputs
            .push(normal.outputs[0].clone());
        assert!(validate_output_policies(&normal, &internally_duplicated).is_err());
        let mut distinct_name = normal.clone();
        distinct_name.outputs[0].name = "receipt-copy".into();
        // Existing artifact semantics permit publishing a file under two names.
        assert!(validate_output_policies(&normal, &distinct_name).is_ok());
    }

    #[test]
    fn failure_evidence_preserves_typed_command_failure_and_upload_diagnostic() {
        let temp = tempfile::tempdir().unwrap();
        for evidence_fails in [false, true] {
            let called = std::cell::Cell::new(false);
            let completion = artifact_execution_with_evidence(
                &generator_task(None),
                temp.path(),
                &AtomicBool::new(false),
                || Ok(()),
                || {
                    Ok(Report {
                        success: false,
                        log: serde_json::json!({
                            "success": false, "outcome": "FAILED", "exit_code": 42,
                            "steps": [{"stderr": "compiler diagnostic"}]
                        })
                        .to_string(),
                    })
                },
                || panic!("normal output after command failure"),
                || {
                    called.set(true);
                    if evidence_fails {
                        Err("secret error".into())
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(called.get());
            assert!(!completion.report.success);
            let value: serde_json::Value = serde_json::from_str(&completion.report.log).unwrap();
            assert_eq!(value["exit_code"], 42);
            assert_eq!(value["outcome"], "FAILED");
            assert_eq!(value["steps"][0]["stderr"], "compiler diagnostic");
            assert_eq!(value.get("diagnostics").is_some(), evidence_fails);
            assert!(!completion.report.log.contains("secret error"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn real_failed_executor_uploads_evidence_and_never_normal_outputs() {
        let temp = tempfile::tempdir().unwrap();
        let mut task = generator_task(None);
        task.steps[0].run =
            "printf 'stage failed\\n' > stage.log; printf 'compiler error\\n' >&2; exit 23".into();
        task.steps[0].timeout_secs = 5;
        let cancelled = AtomicBool::new(false);
        let evidence = std::cell::Cell::new(false);
        let completion = artifact_execution_with_evidence(
            &task,
            temp.path(),
            &cancelled,
            || Ok(()),
            || {
                crate::executor::execute(&task.steps, temp.path(), &cancelled)
                    .map(|result| execution_report(&result, &[]))
            },
            || panic!("normal output after real failed command"),
            || {
                evidence.set(true);
                assert_eq!(
                    fs::read_to_string(temp.path().join("stage.log")).unwrap(),
                    "stage failed\n"
                );
                Err("upload unavailable".into())
            },
        );
        assert!(evidence.get());
        assert!(!completion.report.success);
        let value: serde_json::Value = serde_json::from_str(&completion.report.log).unwrap();
        assert_eq!(value["outcome"], "FAILED");
        assert_eq!(value["exit_code"], 23);
        assert!(
            value["steps"][0]["stderr"]
                .as_str()
                .unwrap()
                .contains("compiler error")
        );
        assert!(value["diagnostics"].is_array());
    }

    #[test]
    fn executor_infrastructure_error_still_attempts_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let evidence = std::cell::Cell::new(false);
        let completion = artifact_execution_with_evidence(
            &generator_task(None),
            temp.path(),
            &AtomicBool::new(false),
            || Ok(()),
            || Err("secret infrastructure error".into()),
            || panic!("normal outputs after executor error"),
            || {
                evidence.set(true);
                Ok(())
            },
        );
        assert!(evidence.get());
        assert!(!completion.report.success);
        assert!(!completion.report.log.contains("secret"));
    }

    #[test]
    fn cancellation_after_execution_prevents_all_uploads() {
        let temp = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let completion = artifact_execution_with_evidence(
            &generator_task(None),
            temp.path(),
            &cancelled,
            || Ok(()),
            || {
                cancelled.store(true, Ordering::Release);
                Ok(Report {
                    success: false,
                    log: r#"{"outcome":"CANCELLED","exit_code":null,"steps":[]}"#.into(),
                })
            },
            || panic!("normal stale upload"),
            || panic!("stale evidence upload"),
        );
        assert!(!completion.report.success);
        assert!(serde_json::from_str::<serde_json::Value>(&completion.report.log).is_ok());
    }

    #[test]
    fn escaped_streams_stay_bounded_valid_json_with_typed_outcome() {
        let result = crate::executor::ExecutionResult {
            success: false,
            outcome: crate::executor::ExecutionOutcome::TimedOut,
            exit_code: None,
            steps: (0..16)
                .map(|step_index| crate::executor::StepResult {
                    step_index,
                    success: false,
                    outcome: crate::executor::ExecutionOutcome::TimedOut,
                    exit_code: None,
                    stdout: crate::executor::LogTail {
                        text: "\u{0001}".repeat(384),
                        truncated: false,
                        complete: true,
                    },
                    stderr: crate::executor::LogTail {
                        text: "\u{0002}".repeat(384),
                        truncated: false,
                        complete: false,
                    },
                })
                .collect(),
        };
        let report = execution_report(&result, &[]);
        assert!(report.log.len() <= LOG_LIMIT);
        let value: serde_json::Value = serde_json::from_str(&report.log).unwrap();
        assert_eq!(value["outcome"], "TIMED_OUT");
        assert!(value["exit_code"].is_null());
        assert_eq!(value["steps"][15]["outcome"], "TIMED_OUT");
        assert_eq!(value["report_truncated"], true);
    }

    #[test]
    fn artifacts_download_execute_upload_then_generator() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("gpipe.tasks.json"),
            br#"{"tasks":{},"seal":true}"#,
        )
        .unwrap();
        let order = std::cell::RefCell::new(Vec::new());
        let completion = artifact_execution(
            &generator_task(Some("gpipe.tasks.json")),
            temp.path(),
            &AtomicBool::new(false),
            || {
                order.borrow_mut().push("download");
                fs::write(temp.path().join("input"), "producer bytes").unwrap();
                Ok(())
            },
            || {
                order.borrow_mut().push("execute");
                assert_eq!(
                    fs::read_to_string(temp.path().join("input")).unwrap(),
                    "producer bytes"
                );
                fs::write(temp.path().join("output"), "consumer bytes").unwrap();
                Ok(Report {
                    success: true,
                    log: "stdout evidence".into(),
                })
            },
            || {
                order.borrow_mut().push("upload");
                assert_eq!(
                    fs::read_to_string(temp.path().join("output")).unwrap(),
                    "consumer bytes"
                );
                Ok(())
            },
        );
        assert_eq!(*order.borrow(), ["download", "execute", "upload"]);
        assert!(completion.report.success);
        assert!(completion.generated.unwrap().seal);
    }

    #[test]
    fn artifact_failures_and_cancellation_never_publish_generated_graph() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("gpipe.tasks.json"),
            br#"{"tasks":{},"seal":true}"#,
        )
        .unwrap();
        let task = generator_task(Some("gpipe.tasks.json"));
        let cancelled = AtomicBool::new(false);
        let completion = artifact_execution(
            &task,
            temp.path(),
            &cancelled,
            || Err("secret download error".into()),
            || panic!("execution after failed download"),
            || panic!("upload after failed download"),
        );
        assert!(!completion.report.success);
        assert!(completion.generated.is_none());
        assert!(!completion.report.log.contains("secret"));
        let completion = artifact_execution(
            &task,
            temp.path(),
            &cancelled,
            || Ok(()),
            || {
                Ok(Report {
                    success: true,
                    log: "retained stderr".into(),
                })
            },
            || Err("secret upload error".into()),
        );
        assert!(!completion.report.success);
        assert!(completion.generated.is_none());
        assert!(completion.report.log.contains("retained stderr"));
        assert!(!completion.report.log.contains("secret"));
        let completion = artifact_execution(
            &task,
            temp.path(),
            &cancelled,
            || Ok(()),
            || {
                Ok(Report {
                    success: false,
                    log: "command failed".into(),
                })
            },
            || panic!("upload after command failure"),
        );
        assert!(!completion.report.success);
        assert!(completion.generated.is_none());
        let completion = artifact_execution(
            &task,
            temp.path(),
            &cancelled,
            || {
                cancelled.store(true, Ordering::Release);
                Ok(())
            },
            || panic!("execute after lease loss"),
            || panic!("upload after lease loss"),
        );
        assert!(!completion.report.success);
        assert!(completion.generated.is_none());
    }

    #[test]
    fn artifact_specs_use_only_explicit_task_steps_and_reject_conflicts() {
        let mut task = generator_task(None);
        assert_eq!(
            crate::transfer::ArtifactSpec::from_steps(&task.steps).unwrap(),
            crate::transfer::ArtifactSpec::default()
        );
        task.steps[0].env.insert("GPIPE_ARTIFACTS".into(), r#"{"inputs":[{"task":"producer","name":"data","path":"input"}],"outputs":[{"name":"result","path":"output"}]}"#.into());
        let spec = crate::transfer::ArtifactSpec::from_steps(&task.steps).unwrap();
        assert_eq!(spec.inputs[0].task, "producer");
        assert_eq!(spec.outputs[0].path, "output");
        task.steps.push(task.steps[0].clone());
        assert!(crate::transfer::ArtifactSpec::from_steps(&task.steps).is_ok());
        task.steps[1]
            .env
            .insert("GPIPE_ARTIFACTS".into(), "{}".into());
        assert!(crate::transfer::ArtifactSpec::from_steps(&task.steps).is_err());
    }

    #[test]
    fn explicit_worker_environment_merges_without_inheriting_ambient_values() {
        let config = WorkerConfig {
            coordinator: "https://example.com".into(),
            token_env: "TOKEN".into(),
            directory: "worker".into(),
            repositories: BTreeMap::new(),
            poll_secs: 1,
            environment: BTreeMap::from([
                ("HOME".into(), "/approved/home".into()),
                ("RUSTUP_HOME".into(), "/approved/rustup".into()),
                ("CARGO_HOME".into(), "/approved/cargo".into()),
            ]),
        };
        let mut task = generator_task(None);
        task.steps[0].env.insert("HOME".into(), "/task/home".into());
        task.steps[0].env.insert("TASK_ONLY".into(), "yes".into());
        let original = task.steps.clone();
        let merged = merge_environment(&task.steps, &config.environment).unwrap();
        assert_eq!(merged[0].env["HOME"], "/task/home");
        assert_eq!(merged[0].env["RUSTUP_HOME"], "/approved/rustup");
        assert_eq!(merged[0].env["CARGO_HOME"], "/approved/cargo");
        assert_eq!(merged[0].env["TASK_ONLY"], "yes");
        assert_eq!(merged[0].env.len(), 4);
        assert_eq!(task.steps, original);
        assert_eq!(config.environment["HOME"], "/approved/home");
        assert!(
            merge_environment(&generator_task(None).steps, &BTreeMap::new()).unwrap()[0]
                .env
                .is_empty()
        );
    }

    #[test]
    fn invalid_explicit_environment_is_secret_safe() {
        for (key, value) in [
            ("", "secret-value"),
            ("A\0B", "secret-value"),
            ("A=B", "secret-value"),
            ("1BAD", "secret-value"),
            ("GOOD", "secret-value\0bad"),
        ] {
            let env = BTreeMap::from([(key.to_owned(), value.to_owned())]);
            let error = validate_environment(&env).unwrap_err();
            assert!(!error.contains("secret-value"));
            assert!(merge_environment(&generator_task(None).steps, &env).is_err());
            let mut task = generator_task(None);
            task.steps[0].env = env;
            assert!(merge_environment(&task.steps, &BTreeMap::new()).is_err());
        }
    }

    #[test]
    fn secret_like_explicit_values_and_worker_token_are_redacted() {
        let env = BTreeMap::from([
            ("API_TOKEN".into(), "worker-secret".into()),
            ("HOME".into(), "/approved/home".into()),
        ]);
        let mut task = generator_task(None);
        task.steps[0]
            .env
            .insert("DB_PASSWORD".into(), "task-secret".into());
        let secrets = output_secrets("own-token", &env, &task.steps);
        let text = redact_secrets(
            "own-token worker-secret task-secret /approved/home",
            &secrets,
            false,
        );
        assert_eq!(text, "[REDACTED] [REDACTED] [REDACTED] /approved/home");
    }

    #[test]
    #[cfg(windows)]
    fn task_environment_overrides_windows_case_insensitively() {
        let env = BTreeMap::from([("HOME".into(), "worker".into())]);
        let mut task = generator_task(None);
        task.steps[0].env.insert("home".into(), "task".into());
        let merged = merge_environment(&task.steps, &env).unwrap();
        assert_eq!(merged[0].env.len(), 1);
        assert_eq!(merged[0].env["home"], "task");
    }

    #[test]
    fn fenced_completion_archives_exact_bytes_and_allows_next_spool() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"durable generated graph and report evidence";
        fs::write(temp.path().join("pending.json"), bytes).unwrap();
        settle_completion(
            temp.path(),
            "a1",
            bytes,
            Response::Fenced {
                attempt_id: "a1".into(),
            },
        )
        .unwrap();
        assert_eq!(fs::read(temp.path().join("fenced-a1.json")).unwrap(), bytes);
        assert!(!temp.path().join("pending.json").exists());
        // An archived successful-looking report is evidence only, never accepted
        // completion; daemon can deliver the next assignment independently.
        let pending = Pending {
            coordinator: "https://example.com/v1/rpc".into(),
            token_env: "TOKEN".into(),
            attempt_id: "a2".into(),
            report: Report {
                success: false,
                log: "failed".into(),
            },
            generated: None,
        };
        spool(temp.path(), &pending).unwrap();
        assert!(temp.path().join("pending.json").exists());
    }

    #[test]
    fn fenced_mismatch_or_rejection_preserves_pending() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"pending";
        fs::write(temp.path().join("pending.json"), bytes).unwrap();
        for response in [
            Response::Fenced {
                attempt_id: "other".into(),
            },
            Response::Error {
                message: "authorization rejected".into(),
            },
            Response::Fenced {
                attempt_id: "../bad".into(),
            },
        ] {
            assert!(settle_completion(temp.path(), "a1", bytes, response).is_err());
            assert_eq!(fs::read(temp.path().join("pending.json")).unwrap(), bytes);
        }
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn fenced_archive_never_overwrites_and_recovers_exact_synced_copy() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"pending";
        fs::write(temp.path().join("pending.json"), bytes).unwrap();
        fs::write(temp.path().join("fenced-a1.json"), b"conflict").unwrap();
        assert!(archive_fenced(temp.path(), "a1", bytes).is_err());
        assert_eq!(
            fs::read(temp.path().join("fenced-a1.json")).unwrap(),
            b"conflict"
        );
        assert!(temp.path().join("pending.json").exists());
        fs::write(temp.path().join("fenced-a1.json"), bytes).unwrap();
        archive_fenced(temp.path(), "a1", bytes).unwrap();
        assert!(!temp.path().join("pending.json").exists());
    }

    #[test]
    #[cfg(unix)]
    fn fenced_archive_rejects_symlink_even_with_matching_evidence() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("pending.json"), b"pending").unwrap();
        fs::write(temp.path().join("target"), b"pending").unwrap();
        std::os::unix::fs::symlink("target", temp.path().join("fenced-a1.json")).unwrap();
        assert!(archive_fenced(temp.path(), "a1", b"pending").is_err());
        assert!(temp.path().join("pending.json").exists());
        assert_eq!(fs::read(temp.path().join("target")).unwrap(), b"pending");
    }

    #[test]
    fn generated_completion_is_opt_in_success_only_and_durable() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("gpipe.tasks.json"),
            serde_json::to_vec(&serde_json::json!({
                "tasks": { "downstream": generator_task(None) }, "seal": true
            }))
            .unwrap(),
        )
        .unwrap();
        let success = || Report {
            success: true,
            log: "success".into(),
        };
        for marker in [None, Some("other.json"), Some("../gpipe.tasks.json")] {
            assert!(
                finish_execution(success(), &generator_task(marker), temp.path())
                    .generated
                    .is_none()
            );
        }
        let task = generator_task(Some("gpipe.tasks.json"));
        assert!(
            finish_execution(
                Report {
                    success: false,
                    log: "failed".into()
                },
                &task,
                temp.path()
            )
            .generated
            .is_none()
        );
        let completion = finish_execution(success(), &task, temp.path());
        assert!(completion.report.success);
        let pending = Pending {
            coordinator: "https://example.com/v1/rpc".into(),
            token_env: "TOKEN".into(),
            attempt_id: "a1".into(),
            report: completion.report,
            generated: completion.generated,
        };
        spool(temp.path(), &pending).unwrap();
        let restored: Pending =
            serde_json::from_slice(&fs::read(temp.path().join("pending.json")).unwrap()).unwrap();
        match restored.request() {
            Request::CompleteGenerated {
                attempt_id,
                report,
                tasks,
                seal,
            } => {
                assert_eq!(attempt_id, "a1");
                assert!(report.success);
                assert!(seal);
                assert_eq!(tasks["downstream"], generator_task(None));
            }
            _ => panic!("generated request was not durably preserved"),
        }
    }

    #[test]
    fn invalid_generated_output_fails_without_graph() {
        let temp = tempfile::tempdir().unwrap();
        let task = generator_task(Some("gpipe.tasks.json"));
        let check = || {
            let completion = finish_execution(
                Report {
                    success: true,
                    log: "executor output".into(),
                },
                &task,
                temp.path(),
            );
            assert!(!completion.report.success);
            assert!(completion.generated.is_none());
            assert!(completion.report.log.contains("executor output"));
            assert!(!completion.report.log.contains("secret-value"));
        };
        check();
        let path = temp.path().join("gpipe.tasks.json");
        for bytes in [
            b"not json secret-value".as_slice(),
            b"{\"tasks\":{},\"seal\":\"secret-value\"}",
            b"{\"tasks\":{}}",
            b"{\"tasks\":{},\"seal\":true,\"extra\":1}",
        ] {
            fs::write(&path, bytes).unwrap();
            check();
        }
        fs::write(
            &path,
            vec![b' '; usize::try_from(GENERATED_LIMIT + 1).unwrap()],
        )
        .unwrap();
        check();
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        check();
    }

    #[test]
    fn generated_output_limit_is_inclusive() {
        let temp = tempfile::tempdir().unwrap();
        let mut bytes = br#"{"tasks":{},"seal":false}"#.to_vec();
        bytes.resize(usize::try_from(GENERATED_LIMIT).unwrap(), b' ');
        fs::write(temp.path().join("gpipe.tasks.json"), &bytes).unwrap();
        let generated = read_generated(temp.path()).unwrap();
        assert!(!generated.seal);
        assert!(generated.tasks.is_empty());
        bytes.push(b' ');
        fs::write(temp.path().join("gpipe.tasks.json"), &bytes).unwrap();
        assert!(read_generated(temp.path()).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn generated_output_rejects_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join("real.json"),
            b"{\"tasks\":{},\"seal\":true}",
        )
        .unwrap();
        std::os::unix::fs::symlink("real.json", temp.path().join("gpipe.tasks.json")).unwrap();
        assert!(read_generated(temp.path()).is_err());
    }

    #[test]
    fn legacy_plain_completion_still_decodes() {
        let pending: Pending = serde_json::from_value(serde_json::json!({
            "coordinator":"https://example.com/v1/rpc", "token_env":"TOKEN",
            "attempt_id":"a1", "report":{"success":true,"log":"done"}
        }))
        .unwrap();
        assert!(matches!(pending.request(), Request::Complete { .. }));
    }

    #[test]
    fn retries_never_replay_ambiguous_claims() {
        let mut failure = RpcFailure {
            message: "test",
            retryable: true,
            not_sent: false,
        };
        assert!(!retry_allowed(&failure, true, false));
        assert!(retry_allowed(&failure, false, false));
        assert!(!retry_allowed(&failure, false, true));
        failure.not_sent = true;
        assert!(retry_allowed(&failure, true, false));
        assert!(!retry_allowed(&failure, true, true));
        failure.retryable = false;
        assert!(!retry_allowed(&failure, false, false));
    }

    #[test]
    fn report_retains_both_streams_and_omits_environment() {
        let result = crate::executor::ExecutionResult {
            success: false,
            outcome: crate::executor::ExecutionOutcome::Failed,
            exit_code: Some(1),
            steps: vec![crate::executor::StepResult {
                step_index: 0,
                success: false,
                outcome: crate::executor::ExecutionOutcome::Failed,
                exit_code: Some(1),
                stdout: crate::executor::LogTail {
                    text: format!("{}stdout own-token", "x".repeat(1000)),
                    truncated: false,
                    complete: true,
                },
                stderr: crate::executor::LogTail {
                    text: "stderr own-token".into(),
                    truncated: false,
                    complete: true,
                },
            }],
        };
        let report = execution_report(&result, &["own-token"]);
        assert!(!report.success);
        assert!(report.log.contains("stdout"));
        assert!(report.log.contains("stderr"));
        assert!(!report.log.contains("own-token"));
        assert!(report.log.contains("[REDACTED]"));
        assert!(!report.log.contains("env"));
        assert!(report.log.len() <= LOG_LIMIT);
    }

    #[test]
    fn output_is_bounded_and_token_redacted() {
        assert_eq!(
            redact_output("before tokensecret after", "tokensecret", false),
            "before [REDACTED] after"
        );
        assert_eq!(
            redact_output("prefix tokense", "tokensecret", true),
            "prefix [REDACTED]"
        );
        let escaped = serde_json::to_string("a\"b\\c").unwrap();
        assert!(!redact_output(&escaped, "a\"b\\c", false).contains("a\\\"b"));
        let mut output = BoundedOutput::default();
        assert!(serde_json::to_writer(&mut output, &"x".repeat(LOG_LIMIT * 2)).is_err());
        assert!(output.truncated);
        assert_eq!(output.bytes.len(), LOG_LIMIT);
        let log = bounded_log(&"é".repeat(LOG_LIMIT), false);
        assert!(log.len() <= LOG_LIMIT);
        assert!(log.ends_with("[output truncated]"));
    }

    #[test]
    fn coordinator_url_security() {
        for value in [
            "https://example.com",
            "http://127.0.0.1:1234",
            "http://[::1]:9",
            "http://localhost",
        ] {
            assert_eq!(rpc_url(value).unwrap().path(), "/v1/rpc");
        }
        for value in [
            "http://example.com",
            "http://127.0.0.1.evil.test",
            "ftp://localhost",
            "https://user:secret@example.com",
            "https://example.com?token=secret",
            "https://example.com/#secret",
            "https://example.com/prefix",
        ] {
            assert!(rpc_url(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn attempt_ids_cannot_escape() {
        for id in [
            "", ".", "..", "../other", "a/b", "a\\b", "/tmp", "a:b", "a\0b",
        ] {
            assert!(!safe_id(id));
        }
        for id in ["CON", "nul", "COM1", "lpt9"] {
            assert!(!safe_id(id));
        }
        assert!(safe_id("COM10"));
        assert!(!safe_id(&"a".repeat(129)));
        assert!(safe_id("attempt_0123-ABC"));
    }

    #[test]
    fn completion_is_durable_and_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let pending = Pending {
            coordinator: "https://example.com/v1/rpc".into(),
            token_env: "TOKEN".into(),
            attempt_id: "attempt-1".into(),
            generated: None,
            report: Report {
                success: true,
                log: "done".into(),
            },
        };
        spool(temp.path(), &pending).unwrap();
        let restored: Pending =
            serde_json::from_slice(&fs::read(temp.path().join("pending.json")).unwrap()).unwrap();
        assert_eq!(restored.report, pending.report);
        assert_eq!(restored.attempt_id, pending.attempt_id);
        assert!(spool(temp.path(), &pending).is_err());
        assert!(!temp.path().join("pending.tmp").exists());
    }

    #[test]
    fn directory_lock_is_exclusive() {
        let temp = tempfile::tempdir().unwrap();
        let guard = lock(temp.path()).unwrap();
        assert!(lock(temp.path()).is_err());
        drop(guard);
        assert!(lock(temp.path()).is_ok());
    }

    #[test]
    fn expired_checkout_never_starts_git() {
        let temp = tempfile::tempdir().unwrap();
        assert!(
            checkout(
                temp.path(),
                temp.path(),
                temp.path(),
                &"a".repeat(40),
                &AtomicBool::new(true)
            )
            .is_err()
        );
    }

    #[test]
    fn checkout_is_detached_and_does_not_change_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let template = temp.path().join("template");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&template).unwrap();
        let cancelled = AtomicBool::new(false);
        let mut init = git(&template);
        init.arg("init").arg(&source);
        git_wait(&mut init, &cancelled).unwrap();
        fs::write(source.join("file"), "first").unwrap();
        let mut add = git(&template);
        add.arg("-C").arg(&source).args(["add", "file"]);
        git_wait(&mut add, &cancelled).unwrap();
        let mut commit = git(&template);
        commit.arg("-C").arg(&source).args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "first",
        ]);
        git_wait(&mut commit, &cancelled).unwrap();
        let output = git(&template)
            .arg("-C")
            .arg(&source)
            .args(["rev-parse", "HEAD"])
            .stdout(Stdio::piped())
            .output()
            .unwrap();
        assert!(output.status.success());
        let revision = String::from_utf8(output.stdout).unwrap();
        fs::write(source.join("file"), "uncommitted source change").unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        checkout(&source, &workspace, &template, revision.trim(), &cancelled).unwrap();
        assert_eq!(fs::read_to_string(workspace.join("file")).unwrap(), "first");
        assert_eq!(
            fs::read_to_string(source.join("file")).unwrap(),
            "uncommitted source change"
        );
        let head = fs::read_to_string(workspace.join(".git/HEAD")).unwrap();
        assert_eq!(head.trim(), revision.trim());
        assert!(!head.starts_with("ref:"));
    }
}
