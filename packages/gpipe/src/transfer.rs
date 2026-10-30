//! Explicit, chunked HTTP artifact data plane for native workers.
//!
//! Parent integration MUST authorize every request with the coordinator before
//! calling [`transfer`], resolving a successful producer attempt for downloads
//! and the current active attempt for uploads. This module does not authenticate
//! attempts or decide dependency access. Parent must also bound HTTP bodies
//! before JSON deserialization (512 KiB suffices for a 64 KiB byte-array chunk).
//! Artifacts remain pinned/retained until explicit run disposal; no deletion or
//! automatic retention is implemented here. Successful task completion must be
//! reported only AFTER all outputs have been finalized and acknowledged.
//!
//! Storage/workspaces must be private, trusted local directories. New storage
//! directories/files use Unix modes 0700/0600; existing store roots must deny
//! group/other access. Windows ACL privacy remains the administrator's duty.
//! Existing links
//! and Windows reparse points are rejected component-by-component, but portable
//! std checks cannot prevent a hostile concurrent ancestor replacement or detect
//! preexisting hard-link aliases. Source files must not be modified while being
//! uploaded. A crash can leave an artifact `.lock`; after confirming no operation
//! is active, an operator must remove that lock to resume. Partial uploads are
//! replayable; partial manifests are never visible. Publication requires local
//! hard-link support. File data and manifests are synced; directory sync is used
//! on Unix, while Windows directory-entry power-loss durability is not promised.
//!
//! Worker calls perform real blocking HTTP requests to `/v1/artifacts`. The
//! server handler is blocking filesystem work; parent should dispatch it outside
//! its async request executor and must fence producer completion vs. final upload.
//! The
//! supplied Client MUST have redirects disabled and certificate verification
//! enabled. Requests have a 30-second timeout and at most three transient-error
//! attempts. Per-artifact lock acquisition waits at most three seconds. No bearer
//! tokens, URLs, server bodies, or raw HTTP errors are exposed
//! in returned diagnostics. [`upload_guarded`] checks cancellation before every
//! chunk, retry and finalization; in-flight requests cannot be revoked. Parent
//! must keep lease renewal running and fence completion if the lease is lost.

use std::{
    collections::BTreeSet,
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::plan::CommandStep;
use reqwest::{Url, blocking::Client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Header binding artifact HTTP requests to the registered worker incarnation.
///
/// The server must authorize via `Request::ArtifactAccessFor` using this value;
/// legacy requests without it remain subject to coordinator policy.
pub const WORKER_INSTANCE_HEADER: &str = "x-gpipe-worker-instance";

/// Largest raw byte chunk accepted or returned by the protocol.
pub const CHUNK_BYTES: usize = 64 * 1024;
/// Maximum length of one artifact, including an unfinished upload.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Bound for JSON HTTP bodies (byte arrays encode to at most four bytes per byte).
pub const MAX_BODY_BYTES: usize = 512 * 1024;
const MAX_SPEC_BYTES: usize = 64 * 1024;
const MAX_ARTIFACTS: usize = 128;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Explicit task artifact declarations. Absent fields default to empty lists.
/// JSON field names are lowercase to match `GPIPE_ARTIFACTS` task configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ArtifactSpec {
    /// Files to materialize before commands execute.
    pub inputs: Vec<ArtifactInput>,
    /// Files to publish after execution under the declaration's outcome policy.
    /// `GPIPE_ARTIFACTS` requires success; `GPIPE_ARTIFACTS_ALWAYS` does not.
    pub outputs: Vec<ArtifactOutput>,
}

/// A named output of a predecessor task, copied to a workspace-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactInput {
    /// Producer task ID; parent checks that this is an authorized dependency.
    pub task: String,
    /// Exact producer artifact name, not a storage path.
    pub name: String,
    /// Portable relative file path; missing parent directories are created safely.
    pub path: String,
}

/// A workspace file explicitly selected for publication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactOutput {
    /// Unique artifact name within this producer attempt.
    pub name: String,
    /// Files to publish under the selected policy; paths must already exist.
    /// Directory trees require explicit [`pack_directory`] before upload.
    pub path: String,
}

impl ArtifactSpec {
    /// Read only explicit step environment entries named `GPIPE_ARTIFACTS`.
    /// Repeated semantically equal declarations are accepted; missing entries
    /// do not override a declaration. No process environment is consulted.
    ///
    /// # Errors
    /// * JSON is oversized/malformed, declarations conflict, or validation fails.
    pub fn from_steps(steps: &[crate::plan::CommandStep]) -> Result<Self, String> {
        Self::from_steps_key(steps, "GPIPE_ARTIFACTS")
    }

    /// Read failure evidence outputs from explicit `GPIPE_ARTIFACTS_ALWAYS` entries.
    /// These files are published after execution regardless of command outcome,
    /// but never after cancellation or lease loss. Inputs are forbidden.
    ///
    /// # Errors
    /// * Malformed, oversized, conflicting or invalid declarations, or any inputs.
    pub fn from_always_steps(steps: &[CommandStep]) -> Result<Self, String> {
        let spec = Self::from_steps_key(steps, "GPIPE_ARTIFACTS_ALWAYS")?;
        if !spec.inputs.is_empty() {
            return Err("GPIPE_ARTIFACTS_ALWAYS accepts outputs only".to_owned());
        }
        Ok(spec)
    }

    fn from_steps_key(steps: &[CommandStep], key: &str) -> Result<Self, String> {
        let mut selected: Option<Self> = None;
        for step in steps {
            if let Some(json) = step.env.get(key) {
                if json.len() > MAX_SPEC_BYTES {
                    return Err("artifact specification exceeds 64 KiB".to_owned());
                }
                let spec: Self = serde_json::from_str(json)
                    .map_err(|_| "invalid GPIPE_ARTIFACTS JSON".to_owned())?;
                spec.validate()?;
                if selected.as_ref().is_some_and(|previous| previous != &spec) {
                    return Err("conflicting GPIPE_ARTIFACTS declarations".to_owned());
                }
                selected = Some(spec);
            }
        }
        Ok(selected.unwrap_or_default())
    }

    /// Validate bounded metadata and portable relative paths without filesystem IO.
    /// Inputs must have distinct non-overlapping destination paths (case-folded
    /// for portability). Outputs must have distinct exact names; storage hashes
    /// names so case-distinct names remain distinct even on Windows.
    ///
    /// # Errors
    /// * More than 128 inputs/outputs, invalid IDs/paths, or duplicate/conflicting
    ///   input destinations or output names. Input destinations may intentionally
    ///   be republished under an output name; input/output path reuse is allowed.
    pub fn validate(&self) -> Result<(), String> {
        if self.inputs.len() > MAX_ARTIFACTS || self.outputs.len() > MAX_ARTIFACTS {
            return Err("too many artifact declarations".to_owned());
        }
        let mut paths = BTreeSet::<String>::new();
        for input in &self.inputs {
            identifier(&input.task)?;
            identifier(&input.name)?;
            relative_path(&input.path)?;
            let path = input.path.to_ascii_lowercase();
            for previous in &paths {
                if path == *previous
                    || path.starts_with(&format!("{previous}/"))
                    || previous.starts_with(&format!("{path}/"))
                {
                    return Err("overlapping artifact input destinations".to_owned());
                }
            }
            paths.insert(path);
        }
        let mut names = BTreeSet::new();
        for output in &self.outputs {
            identifier(&output.name)?;
            relative_path(&output.path)?;
            if !names.insert(&output.name) {
                return Err("duplicate artifact output name".to_owned());
            }
        }
        Ok(())
    }
}

/// One authorized upload or download operation.
///
/// Uploads use no `producer_task`; downloads use `Some(task)`, empty data and
/// `finish=false`. Offset is a raw byte position, not a chunk index.
/// Finalization may carry the final data chunk or be an empty request at EOF.
/// Receivers must bound JSON before deserializing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferRequest {
    /// Consumer/current worker attempt used for parent authorization.
    pub attempt_id: String,
    /// None for upload, Some(predecessor task ID) for download.
    pub producer_task: Option<String>,
    /// Exact artifact name.
    pub name: String,
    /// Byte offset into the artifact.
    pub offset: u64,
    /// At most 64 KiB; empty for downloads.
    pub data: Vec<u8>,
    /// Publish an upload at this request's end offset.
    pub finish: bool,
}

/// Artifact endpoint response. Final upload acknowledgments carry the stored
/// SHA-256 so the uploader can compare its independently computed digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum TransferResponse {
    /// Chunk accepted. `sha256` is Some only when `finish=true` was acknowledged.
    Uploaded {
        /// Final lowercase hexadecimal SHA-256, or None for an intermediate chunk.
        sha256: Option<String>,
    },
    /// Download bytes. Only the terminal chunk contains a checksum.
    Chunk {
        /// At most 64 KiB of raw bytes.
        data: Vec<u8>,
        /// True at EOF, including an empty artifact.
        done: bool,
        /// Final lowercase SHA-256 when done=true, otherwise None.
        sha256: Option<String>,
    },
    /// Parent/server rejection; worker never exposes arbitrary message text.
    Error {
        /// Secret-safe server diagnostic.
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    attempt_id: String,
    name: String,
    size: u64,
    sha256: String,
}

/// Execute an already-authorized server-side transfer using a private root.
///
/// `producer_attempt_id` is resolved by the coordinator, NEVER from client input
/// alone. Uploads require it to equal `request.attempt_id`. Data is scoped by
/// SHA-256 hashes of attempt/name IDs, avoiding case-insensitive path collisions.
///
/// # Errors
/// * Invalid IDs/mode/chunk size/offset, missing or corrupt data, replay conflicts,
///   busy/stale artifact lock, symlink/reparse paths, or filesystem failure.
/// * Finalized artifacts cannot be changed. Identical chunk/finalization replays
///   succeed; gaps and conflicting overlaps fail. Partial overlaps verify the
///   existing prefix before appending (supporting interrupted-write recovery).
///
/// Parent should convert Err to `TransferResponse::Error` and must not acknowledge
/// task success until final upload acknowledgments have been verified.
pub fn transfer(
    root: &Path,
    producer_attempt_id: &str,
    request: &TransferRequest,
) -> Result<TransferResponse, String> {
    identifier(producer_attempt_id)?;
    identifier(&request.attempt_id)?;
    identifier(&request.name)?;
    if request.offset > MAX_FILE_BYTES || request.data.len() > CHUNK_BYTES {
        return Err("artifact offset or chunk exceeds limit".to_owned());
    }
    let downloading = if let Some(task) = &request.producer_task {
        identifier(task)?;
        if !request.data.is_empty() || request.finish {
            return Err("invalid artifact download request".to_owned());
        }
        true
    } else {
        if producer_attempt_id != request.attempt_id {
            return Err("artifact upload attempt mismatch".to_owned());
        }
        false
    };
    let root = absolute(root)?;
    let directory = root
        .join(hash_bytes(producer_attempt_id.as_bytes()))
        .join(hash_bytes(request.name.as_bytes()));
    if downloading {
        checked_path(&directory, false)?;
    } else {
        directories(&directory)?;
    }
    private_directory(&root)?;
    private_directory(&directory)?;
    let _lock = ArtifactLock::acquire(&directory)?;
    if downloading {
        serve_chunk(&directory, producer_attempt_id, request)
    } else {
        accept_chunk(&directory, producer_attempt_id, request)
    }
}

struct ArtifactLock(PathBuf);
impl ArtifactLock {
    fn acquire(directory: &Path) -> Result<Self, String> {
        let path = directory.join(".lock");
        let started = Instant::now();
        loop {
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && started.elapsed() < Duration::from_secs(3) =>
                {
                    checked_path(&path, false)?;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return Err("artifact busy or lock unavailable".to_owned()),
            }
        }
    }
}
impl Drop for ArtifactLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

fn accept_chunk(
    directory: &Path,
    attempt: &str,
    request: &TransferRequest,
) -> Result<TransferResponse, String> {
    let end = request
        .offset
        .checked_add(request.data.len() as u64)
        .ok_or("artifact offset overflow")?;
    if end > MAX_FILE_BYTES {
        return Err("artifact exceeds 256 MiB".to_owned());
    }
    let data_path = directory.join("data");
    let manifest_path = directory.join("manifest.json");
    checked_path(&manifest_path, true)?;
    if manifest_path.try_exists().map_err(io_error)? {
        let manifest = read_manifest(directory, attempt, &request.name)?;
        let mut file = regular(&data_path, false)?;
        if end > manifest.size || (request.finish && end != manifest.size) {
            return Err("cannot modify finalized artifact".to_owned());
        }
        verify_range(&mut file, request.offset, &request.data)?;
        if request.finish && hash_file(&mut file)? != manifest.sha256 {
            return Err("finalized artifact checksum mismatch".to_owned());
        }
        if request.finish {
            sync_directory(directory)?;
        }
        return Ok(TransferResponse::Uploaded {
            sha256: request.finish.then_some(manifest.sha256),
        });
    }
    checked_path(&data_path, true)?;
    let mut file = if data_path.try_exists().map_err(io_error)? {
        regular(&data_path, true)?
    } else {
        if request.offset != 0 {
            return Err("artifact upload has a gap".to_owned());
        }
        new_file(&data_path, true).map_err(io_error)?
    };
    let length = file.metadata().map_err(io_error)?.len();
    if length > MAX_FILE_BYTES || request.offset > length || (request.finish && end < length) {
        return Err("artifact gap, oversized data, or premature finish".to_owned());
    }
    let overlap = usize::try_from(
        length
            .saturating_sub(request.offset)
            .min(request.data.len() as u64),
    )
    .map_err(|_| "artifact range overflow".to_owned())?;
    verify_range(&mut file, request.offset, &request.data[..overlap])?;
    if overlap < request.data.len() {
        file.seek(SeekFrom::End(0)).map_err(io_error)?;
        file.write_all(&request.data[overlap..]).map_err(io_error)?;
    }
    file.sync_all().map_err(io_error)?;
    if !request.finish {
        return Ok(TransferResponse::Uploaded { sha256: None });
    }
    let size = file.metadata().map_err(io_error)?.len();
    if size != end {
        return Err("artifact finish offset mismatch".to_owned());
    }
    let sha256 = hash_file(&mut file)?;
    let manifest = Manifest {
        version: 1,
        attempt_id: attempt.to_owned(),
        name: request.name.clone(),
        size,
        sha256: sha256.clone(),
    };
    let bytes =
        serde_json::to_vec(&manifest).map_err(|_| "cannot encode artifact manifest".to_owned())?;
    let (temp, mut output) = Temporary::create(directory)?;
    output.write_all(&bytes).map_err(io_error)?;
    output.sync_all().map_err(io_error)?;
    drop(output);
    fs::hard_link(&temp.0, &manifest_path).map_err(io_error)?;
    sync_directory(directory)?;
    Ok(TransferResponse::Uploaded {
        sha256: Some(sha256),
    })
}

fn serve_chunk(
    directory: &Path,
    attempt: &str,
    request: &TransferRequest,
) -> Result<TransferResponse, String> {
    let manifest = read_manifest(directory, attempt, &request.name)?;
    if request.offset > manifest.size {
        return Err("artifact download offset beyond EOF".to_owned());
    }
    let mut input = regular(&directory.join("data"), false)?;
    input
        .seek(SeekFrom::Start(request.offset))
        .map_err(io_error)?;
    let count = usize::try_from((manifest.size - request.offset).min(CHUNK_BYTES as u64))
        .map_err(|_| "artifact range overflow".to_owned())?;
    let mut data = vec![0; count];
    input.read_exact(&mut data).map_err(io_error)?;
    let done = request.offset + count as u64 == manifest.size;
    Ok(TransferResponse::Chunk {
        data,
        done,
        sha256: done.then_some(manifest.sha256),
    })
}

fn read_manifest(directory: &Path, attempt: &str, name: &str) -> Result<Manifest, String> {
    let mut bytes = Vec::new();
    regular(&directory.join("manifest.json"), false)?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > 4096 {
        return Err("oversized artifact manifest".to_owned());
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| "invalid artifact manifest".to_owned())?;
    if manifest.version != 1
        || manifest.attempt_id != attempt
        || manifest.name != name
        || manifest.size > MAX_FILE_BYTES
        || !valid_digest(&manifest.sha256)
    {
        return Err("artifact manifest identity or metadata mismatch".to_owned());
    }
    if regular(&directory.join("data"), false)?
        .metadata()
        .map_err(io_error)?
        .len()
        != manifest.size
    {
        return Err("artifact data length differs from manifest".to_owned());
    }
    Ok(manifest)
}

fn verify_range(file: &mut File, offset: u64, expected: &[u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
    let mut bytes = vec![0; expected.len()];
    file.read_exact(&mut bytes).map_err(io_error)?;
    if bytes != expected {
        return Err("artifact upload replay conflicts with stored bytes".to_owned());
    }
    Ok(())
}

/// Download all declared inputs using authenticated bounded HTTP requests.
///
/// Parent directories are created beneath the workspace, rejecting links at each
/// component. Each file becomes visible only after checksum
/// verification via atomic no-clobber publication; earlier inputs remain if a
/// later input fails. No implicit remote lookup or fallback is performed.
///
/// # Errors
/// * Invalid spec/endpoint/IDs/paths, existing destinations, missing artifacts,
///   malformed/oversized network responses, checksum mismatch, or IO failure.
/// * Caller must supply a no-redirect Client; see module security contract.
pub fn download(
    client: &Client,
    url: &str,
    token: &str,
    attempt_id: &str,
    workspace: &Path,
    spec: &ArtifactSpec,
) -> Result<(), String> {
    spec.validate()?;
    identifier(attempt_id)?;
    let endpoint = endpoint(url)?;
    let workspace = workspace_root(workspace)?;
    for input in &spec.inputs {
        let destination = prepare_destination(&workspace, &input.path)?;
        if destination.try_exists().map_err(io_error)? {
            return Err("artifact destination already exists".to_owned());
        }
    }
    for input in &spec.inputs {
        receive_file(&workspace, input, |offset| {
            rpc(
                client,
                &endpoint,
                token,
                &TransferRequest {
                    attempt_id: attempt_id.to_owned(),
                    producer_task: Some(input.task.clone()),
                    name: input.name.clone(),
                    offset,
                    data: Vec::new(),
                    finish: false,
                },
            )
        })?;
    }
    Ok(())
}

/// Upload each explicit output and verify the final server digest.
///
/// Matching bytes are replayed and checked against a local streaming SHA-256.
/// No task success is reported by this function. Caller reports success only
/// after this returns Ok.
///
/// # Errors
/// * Invalid spec/endpoint/IDs/paths, missing/nonregular/oversized or changing
///   output, server rejection, malformed/oversized responses, checksum mismatch,
///   network failure, or IO failure. Earlier uploads persist on later failure.
/// * Caller must supply a no-redirect Client; see module security contract.
pub fn upload(
    client: &Client,
    url: &str,
    token: &str,
    attempt_id: &str,
    workspace: &Path,
    spec: &ArtifactSpec,
) -> Result<(), String> {
    upload_guarded(client, url, token, attempt_id, workspace, spec, || false)
}

/// Upload outputs while checking cancellation before every chunk and finalization.
/// An in-flight HTTP request cannot be revoked; coordinator authorization remains
/// the final fence against expired attempts.
///
/// # Errors
/// * Cancellation, or any error documented by [`upload`].
pub fn upload_guarded(
    client: &Client,
    url: &str,
    token: &str,
    attempt_id: &str,
    workspace: &Path,
    spec: &ArtifactSpec,
    cancelled: impl Fn() -> bool,
) -> Result<(), String> {
    spec.validate()?;
    identifier(attempt_id)?;
    let endpoint = endpoint(url)?;
    let workspace = workspace_root(workspace)?;
    for output in &spec.outputs {
        let file = regular(&workspace_file(&workspace, &output.path, false)?, false)?;
        if file.metadata().map_err(io_error)?.len() > MAX_FILE_BYTES {
            return Err("artifact exceeds 256 MiB".to_owned());
        }
    }
    for output in &spec.outputs {
        send_file(&workspace, output, |offset, data, finish| {
            if cancelled() {
                return Err("artifact upload cancelled".to_owned());
            }
            rpc_guarded(
                client,
                &endpoint,
                token,
                &TransferRequest {
                    attempt_id: attempt_id.to_owned(),
                    producer_task: None,
                    name: output.name.clone(),
                    offset,
                    data,
                    finish,
                },
                &cancelled,
            )
        })?;
    }
    Ok(())
}

fn send_file(
    workspace: &Path,
    output: &ArtifactOutput,
    mut send: impl FnMut(u64, Vec<u8>, bool) -> Result<TransferResponse, String>,
) -> Result<(), String> {
    let mut file = regular(&workspace_file(workspace, &output.path, false)?, false)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size > MAX_FILE_BYTES {
        return Err("artifact exceeds 256 MiB".to_owned());
    }
    let mut digest = Sha256::new();
    let mut offset = 0;
    while offset < size {
        let mut bytes = vec![
            0;
            usize::try_from((size - offset).min(CHUNK_BYTES as u64))
                .map_err(|_| "artifact range overflow")?
        ];
        file.read_exact(&mut bytes).map_err(io_error)?;
        digest.update(&bytes);
        let count = bytes.len() as u64;
        if !matches!(
            send(offset, bytes, false)?,
            TransferResponse::Uploaded { sha256: None }
        ) {
            return Err("invalid artifact upload acknowledgment".to_owned());
        }
        offset += count;
    }
    if file.metadata().map_err(io_error)?.len() != size {
        return Err("artifact changed during upload".to_owned());
    }
    let expected = hex(&digest.finalize());
    match send(offset, Vec::new(), true)? {
        TransferResponse::Uploaded {
            sha256: Some(actual),
        } if actual == expected => Ok(()),
        _ => Err("artifact upload final checksum not acknowledged".to_owned()),
    }
}

fn receive_file(
    workspace: &Path,
    input: &ArtifactInput,
    mut receive: impl FnMut(u64) -> Result<TransferResponse, String>,
) -> Result<(), String> {
    let destination = prepare_destination(workspace, &input.path)?;
    if destination.try_exists().map_err(io_error)? {
        return Err("artifact destination already exists".to_owned());
    }
    let parent = destination
        .parent()
        .ok_or("artifact destination lacks parent")?;
    let (temp, mut file) = Temporary::create(parent)?;
    let mut digest = Sha256::new();
    let mut offset = 0_u64;
    loop {
        let TransferResponse::Chunk { data, done, sha256 } = receive(offset)? else {
            return Err("invalid artifact download response".to_owned());
        };
        if data.len() > CHUNK_BYTES || (!done && data.is_empty()) || (!done && sha256.is_some()) {
            return Err("invalid artifact download chunk".to_owned());
        }
        offset = offset
            .checked_add(data.len() as u64)
            .ok_or("artifact length overflow")?;
        if offset > MAX_FILE_BYTES {
            return Err("artifact exceeds 256 MiB".to_owned());
        }
        digest.update(&data);
        file.write_all(&data).map_err(io_error)?;
        if done {
            if sha256.as_deref() != Some(hex(&digest.finalize()).as_str()) {
                return Err("artifact download checksum mismatch".to_owned());
            }
            break;
        }
    }
    file.sync_all().map_err(io_error)?;
    drop(file);
    checked_path(&destination, true)?;
    fs::hard_link(&temp.0, &destination).map_err(io_error)?;
    sync_directory(parent)
}

fn endpoint(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value).map_err(|_| "invalid artifact endpoint".to_owned())?;
    let loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
            || host == "[::1]"
    });
    if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/" | "/v1/rpc" | "/v1/artifacts")
    {
        return Err(
            "artifact endpoint requires HTTPS or loopback HTTP without credentials/query"
                .to_owned(),
        );
    }
    url.set_path("/v1/artifacts");
    Ok(url)
}

fn rpc(
    client: &Client,
    endpoint: &Url,
    token: &str,
    request: &TransferRequest,
) -> Result<TransferResponse, String> {
    rpc_guarded(client, endpoint, token, request, &|| false)
}

fn rpc_guarded(
    client: &Client,
    endpoint: &Url,
    token: &str,
    request: &TransferRequest,
    cancelled: &impl Fn() -> bool,
) -> Result<TransferResponse, String> {
    let body =
        serde_json::to_vec(request).map_err(|_| "cannot encode artifact request".to_owned())?;
    if body.len() > MAX_BODY_BYTES {
        return Err("oversized artifact request".to_owned());
    }
    for attempt in 0..3 {
        if cancelled() {
            return Err("artifact transfer cancelled".to_owned());
        }
        let response = client
            .post(endpoint.clone())
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .timeout(Duration::from_secs(30))
            .body(body.clone())
            .send();
        let response = match response {
            Ok(response) => response,
            Err(_) if attempt < 2 => {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(error) => {
                return Err(format!(
                    "artifact network request failed (timeout={}, connect={}, body={})",
                    error.is_timeout(),
                    error.is_connect(),
                    error.is_body()
                ));
            }
        };
        if response.url() != endpoint {
            return Err("artifact redirects are forbidden".to_owned());
        }
        if (response.status().is_server_error() || response.status().as_u16() == 429) && attempt < 2
        {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        if !response.status().is_success() {
            return Err(format!(
                "artifact HTTP request rejected (status {})",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        match response
            .take(MAX_BODY_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
        {
            Ok(_) => {}
            Err(_) if attempt < 2 => {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(error) => {
                return Err(format!(
                    "cannot read artifact HTTP response ({:?})",
                    error.kind()
                ));
            }
        }
        if bytes.len() > MAX_BODY_BYTES {
            return Err("oversized artifact HTTP response".to_owned());
        }
        let response: TransferResponse = serde_json::from_slice(&bytes)
            .map_err(|_| "invalid artifact HTTP response".to_owned())?;
        if matches!(response, TransferResponse::Error { .. }) {
            return Err("artifact request rejected by server".to_owned());
        }
        return Ok(response);
    }
    Err("artifact request exhausted retries".to_owned())
}

fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err("invalid artifact identifier".to_owned());
    }
    Ok(())
}

fn relative_path(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 1024 {
        return Err("invalid artifact relative path".to_owned());
    }
    for component in value.split('/') {
        identifier(component)?;
        let base = component
            .split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let device = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || base
                .strip_prefix("COM")
                .or_else(|| base.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
                });
        if device || component.ends_with('.') || component.eq_ignore_ascii_case(".git") {
            return Err("reserved artifact path component".to_owned());
        }
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("artifact parent traversal is forbidden".to_owned());
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(io_error)?.join(path)
    };
    if !path.is_absolute() {
        return Err("artifact path must be absolute".to_owned());
    }
    #[cfg(windows)]
    for part in path.components() {
        match part {
            Component::Prefix(prefix)
                if !matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                ) =>
            {
                return Err("artifact storage requires a local drive".to_owned());
            }
            Component::Normal(name) => {
                let name = name.to_str().ok_or("invalid Windows artifact path")?;
                if name.contains(':') || name.ends_with(['.', ' ']) {
                    return Err("unsafe Windows artifact path alias".to_owned());
                }
            }
            _ => {}
        }
    }
    Ok(path
        .components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect())
}

fn checked_path(path: &Path, missing_leaf: bool) -> Result<(), String> {
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir) {
            return Err("artifact parent traversal is forbidden".to_owned());
        }
        current.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || reparse(&metadata) {
                    return Err("artifact symlink/reparse path forbidden".to_owned());
                }
            }
            Err(error)
                if missing_leaf && current == path && error.kind() == io::ErrorKind::NotFound =>
            {
                return Ok(());
            }
            Err(error) => return Err(format!("artifact path unavailable ({:?})", error.kind())),
        }
    }
    Ok(())
}
#[cfg(windows)]
fn reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
const fn reparse(_: &fs::Metadata) -> bool {
    false
}

fn directories(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let mut builder = DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&current) {
            Ok(()) => {
                if let Some(parent) = current.parent().filter(|path| !path.as_os_str().is_empty()) {
                    sync_directory(parent)?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(error)),
        }
        checked_path(&current, false)?;
        if !current.is_dir() {
            return Err("artifact directory is not a directory".to_owned());
        }
    }
    Ok(())
}

fn workspace_root(path: &Path) -> Result<PathBuf, String> {
    let path = absolute(path)?;
    checked_path(&path, false)?;
    if !path.is_dir() {
        return Err("artifact workspace is not a directory".to_owned());
    }
    fs::canonicalize(path).map_err(io_error)
}
fn prepare_destination(workspace: &Path, value: &str) -> Result<PathBuf, String> {
    relative_path(value)?;
    checked_path(workspace, false)?;
    let destination = workspace.join(value);
    if !destination.starts_with(workspace) {
        return Err("artifact destination escapes workspace".to_owned());
    }
    let parent = destination
        .parent()
        .ok_or("artifact destination lacks parent")?;
    // directories validates every existing component, including dangling links,
    // and checks newly created directories before descending into them.
    directories(parent)
        .map_err(|error| format!("prepare artifact destination parents: {error}"))?;
    checked_path(&destination, true)?;
    Ok(destination)
}

fn workspace_file(workspace: &Path, value: &str, missing: bool) -> Result<PathBuf, String> {
    relative_path(value)?;
    let path = workspace.join(value);
    if !path.starts_with(workspace) {
        return Err("artifact path escapes workspace".to_owned());
    }
    checked_path(&path, missing)?;
    Ok(path)
}
fn regular(path: &Path, write: bool) -> Result<File, String> {
    checked_path(path, false)?;
    if !fs::symlink_metadata(path).map_err(io_error)?.is_file() {
        return Err("artifact is not a regular file".to_owned());
    }
    OpenOptions::new()
        .read(true)
        .write(write)
        .open(path)
        .map_err(io_error)
}

fn new_file(path: &Path, read: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(read).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn private_directory(path: &Path) -> Result<(), String> {
    checked_path(path, false)?;
    let metadata = fs::metadata(path).map_err(io_error)?;
    if !metadata.is_dir() {
        return Err("artifact directory is not a directory".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("artifact storage must not grant group/other permissions".to_owned());
        }
    }
    Ok(())
}

struct Temporary(PathBuf);
impl Temporary {
    fn create(directory: &Path) -> Result<(Self, File), String> {
        checked_path(directory, false)?;
        for _ in 0..128 {
            let path = directory.join(format!(
                ".gpipe-transfer-{}-{}.tmp",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            match new_file(&path, false) {
                Ok(file) => return Ok((Self(path), file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        Err("cannot allocate artifact temporary file".to_owned())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn hash_file(file: &mut File) -> Result<String, String> {
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size > MAX_FILE_BYTES {
        return Err("artifact exceeds 256 MiB".to_owned());
    }
    let mut digest = Sha256::new();
    let mut buffer = vec![0; CHUNK_BYTES];
    let mut offset = 0_u64;
    while offset < size {
        let count = usize::try_from((size - offset).min(CHUNK_BYTES as u64))
            .map_err(|_| "artifact range overflow")?;
        file.read_exact(&mut buffer[..count]).map_err(io_error)?;
        digest.update(&buffer[..count]);
        offset += count as u64;
    }
    if file.metadata().map_err(io_error)?.len() != size {
        return Err("artifact changed during checksum".to_owned());
    }
    Ok(hex(&digest.finalize()))
}
fn hash_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io_error)
}
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn sync_directory(_: &Path) -> Result<(), String> {
    Ok(())
}
#[allow(clippy::needless_pass_by_value)]
const BUNDLE_MAGIC: &[u8; 8] = b"GPIPEB01";
const MAX_BUNDLE_FILES: usize = 4096;
const MAX_BUNDLE_MANIFEST: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleEntry {
    path: String,
    bytes: u64,
    sha256: String,
    // Only portable ordinary permission bits, never setuid/setgid/sticky.
    mode: u32,
}

fn bundle_entries(
    root: &Path,
    relative: &str,
    entries: &mut Vec<BundleEntry>,
    visited: &mut usize,
) -> Result<(), String> {
    if relative.split('/').count() > 64 {
        return Err("bundle directory depth exceeded".into());
    }
    let directory = root.join(relative);
    checked_path(&directory, false)?;
    let mut children = Vec::new();
    for child in fs::read_dir(directory).map_err(io_error)? {
        if children.len() >= MAX_BUNDLE_FILES {
            return Err("too many bundle directory entries".into());
        }
        children.push(child.map_err(io_error)?);
    }
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        *visited += 1;
        if *visited > MAX_BUNDLE_FILES {
            return Err("too many bundle directory entries".into());
        }
        let name = child
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF-8 bundle path")?;
        let path = if relative.is_empty() {
            name
        } else {
            format!("{relative}/{name}")
        };
        relative_path(&path)?;
        checked_path(&child.path(), false)?;
        let metadata = fs::symlink_metadata(child.path()).map_err(io_error)?;
        if metadata.is_dir() {
            bundle_entries(root, &path, entries, visited)?;
        } else if metadata.is_file() {
            if entries.len() >= MAX_BUNDLE_FILES {
                return Err("too many bundle files".into());
            }
            let bytes = entries
                .iter()
                .try_fold(metadata.len(), |sum, entry| sum.checked_add(entry.bytes))
                .ok_or("bundle size overflow")?;
            if bytes > MAX_FILE_BYTES {
                return Err("bundle exceeds 256 MiB".into());
            }
            let mut file = regular(&child.path(), false)?;
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o777
            };
            #[cfg(not(unix))]
            let mode = if metadata.permissions().readonly() {
                0o444
            } else {
                0o644
            };
            entries.push(BundleEntry {
                path,
                bytes: metadata.len(),
                sha256: hash_file(&mut file)?,
                mode,
            });
        } else {
            return Err("bundle accepts regular files only".into());
        }
    }
    Ok(())
}

fn validate_bundle(entries: &[BundleEntry], header_bytes: u64) -> Result<u64, String> {
    if entries.len() > MAX_BUNDLE_FILES {
        return Err("too many bundle files".into());
    }
    let mut total = header_bytes;
    let mut paths = BTreeSet::new();
    let mut previous = None;
    for entry in entries {
        relative_path(&entry.path)?;
        if !valid_digest(&entry.sha256) || entry.mode & !0o777 != 0 {
            return Err("invalid bundle file metadata".into());
        }
        if previous.is_some_and(|path: &str| path >= entry.path.as_str()) {
            return Err("bundle manifest must be strictly sorted".into());
        }
        previous = Some(entry.path.as_str());
        let path = entry.path.to_ascii_lowercase();
        if paths.iter().any(|previous: &String| {
            previous == &path
                || path.starts_with(&format!("{previous}/"))
                || previous.starts_with(&format!("{path}/"))
        }) {
            return Err("overlapping bundle paths".into());
        }
        paths.insert(path);
        total = total
            .checked_add(entry.bytes)
            .ok_or("bundle size overflow")?;
        if total > MAX_FILE_BYTES {
            return Err("bundle exceeds 256 MiB".into());
        }
    }
    Ok(total)
}

/// Pack a directory as a deterministic `GPIPEB01` bundle.
///
/// Uses a sorted JSON file manifest followed by raw file bytes. Includes SHA-256 and ordinary Unix permissions
/// (including executable bits); excludes timestamps, owners and empty directories.
/// The output must not exist or be inside the source. No compression/dependencies.
///
/// # Errors
/// * Unsafe/nonportable paths, symlinks/reparse points, special files, IO errors,
///   changed source, over 4096 directory entries, depth over 64, manifest over 1 MiB, or total
///   bundle over 256 MiB. Trusted local filesystem writers must remain quiescent.
pub fn pack_directory(source: &Path, output: &Path) -> Result<(), String> {
    let root = workspace_root(source)?;
    let output = absolute(output)?;
    checked_path(&output, true)?;
    if output.starts_with(&root) {
        return Err("bundle output cannot be inside source".into());
    }
    let mut entries = Vec::new();
    bundle_entries(&root, "", &mut entries, &mut 0)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = serde_json::to_vec(&entries).map_err(|_| "cannot serialize bundle manifest")?;
    if manifest.len() > MAX_BUNDLE_MANIFEST {
        return Err("bundle manifest too large".into());
    }
    validate_bundle(&entries, 16 + manifest.len() as u64)?;
    let parent = output.parent().ok_or("bundle output lacks parent")?;
    let (temp, mut file) = Temporary::create(parent)?;
    file.write_all(BUNDLE_MAGIC).map_err(io_error)?;
    file.write_all(&(manifest.len() as u64).to_le_bytes())
        .map_err(io_error)?;
    file.write_all(&manifest).map_err(io_error)?;
    for entry in &entries {
        let mut input = regular(&workspace_file(&root, &entry.path, false)?, false)?;
        if input.metadata().map_err(io_error)?.len() != entry.bytes
            || hash_file(&mut input)? != entry.sha256
        {
            return Err("bundle source changed".into());
        }
        input.seek(SeekFrom::Start(0)).map_err(io_error)?;
        let mut digest = Sha256::new();
        let mut remaining = entry.bytes;
        let mut buffer = vec![0; CHUNK_BYTES];
        while remaining > 0 {
            let count = usize::try_from(remaining.min(CHUNK_BYTES as u64))
                .map_err(|_| "bundle size overflow")?;
            input.read_exact(&mut buffer[..count]).map_err(io_error)?;
            digest.update(&buffer[..count]);
            file.write_all(&buffer[..count]).map_err(io_error)?;
            remaining -= count as u64;
        }
        if hex(&digest.finalize()) != entry.sha256
            || input.metadata().map_err(io_error)?.len() != entry.bytes
        {
            return Err("bundle source changed".into());
        }
    }
    file.sync_all().map_err(io_error)?;
    // No overwrite: hard-link publication matches the artifact transfer contract.
    fs::hard_link(&temp.0, &output).map_err(io_error)?;
    sync_directory(parent)
}

/// Extract a bounded bundle into a new directory, never overwriting existing data.
///
/// All manifest paths, sizes and modes are validated before creating destinations.
/// Checks every file digest, denies traversal/symlinks/reparse points, preserves
/// ordinary executable bits on Unix and rejects executable modes on non-Unix
/// rather than silently losing them. Does not execute any bundled content.
///
/// # Errors
/// * Invalid/truncated/oversized bundle, unsupported mode, checksum mismatch,
///   unsafe paths, existing destination, or filesystem errors. A partially written
///   destination is retained on failure for explicit operator recovery, not reused.
/// * Concurrent hostile filesystem mutation is outside the trusted worker contract.
pub fn unpack_directory(bundle: &Path, destination: &Path) -> Result<(), String> {
    let bundle = absolute(bundle)?;
    checked_path(&bundle, false)?;
    let mut file = regular(&bundle, false)?;
    let size = file.metadata().map_err(io_error)?.len();
    if size > MAX_FILE_BYTES {
        return Err("bundle exceeds 256 MiB".into());
    }
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic).map_err(io_error)?;
    if &magic != BUNDLE_MAGIC {
        return Err("unsupported bundle format".into());
    }
    let mut length = [0_u8; 8];
    file.read_exact(&mut length).map_err(io_error)?;
    let length =
        usize::try_from(u64::from_le_bytes(length)).map_err(|_| "bundle manifest too large")?;
    if length > MAX_BUNDLE_MANIFEST {
        return Err("bundle manifest too large".into());
    }
    let mut manifest = vec![0; length];
    file.read_exact(&mut manifest).map_err(io_error)?;
    let entries: Vec<BundleEntry> =
        serde_json::from_slice(&manifest).map_err(|_| "invalid bundle manifest")?;
    if validate_bundle(&entries, 16 + length as u64)? != size {
        return Err("bundle length mismatch".into());
    }
    #[cfg(not(unix))]
    if entries.iter().any(|entry| entry.mode & 0o111 != 0) {
        return Err("executable bundle modes require Unix".into());
    }
    let destination = absolute(destination)?;
    checked_path(&destination, true)?;
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&destination).map_err(io_error)?;
    for entry in entries {
        let path = prepare_destination(&destination, &entry.path)?;
        let mut output = new_file(&path, false).map_err(io_error)?;
        let mut digest = Sha256::new();
        let mut remaining = entry.bytes;
        let mut buffer = vec![0; CHUNK_BYTES];
        while remaining > 0 {
            let count = usize::try_from(remaining.min(CHUNK_BYTES as u64))
                .map_err(|_| "bundle size overflow")?;
            file.read_exact(&mut buffer[..count]).map_err(io_error)?;
            digest.update(&buffer[..count]);
            output.write_all(&buffer[..count]).map_err(io_error)?;
            remaining -= count as u64;
        }
        if hex(&digest.finalize()) != entry.sha256 {
            return Err("bundle checksum mismatch".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            output
                .set_permissions(fs::Permissions::from_mode(entry.mode))
                .map_err(io_error)?;
        }
        #[cfg(not(unix))]
        {
            let mut permissions = output.metadata().map_err(io_error)?.permissions();
            permissions.set_readonly(entry.mode & 0o222 == 0);
            output.set_permissions(permissions).map_err(io_error)?;
        }
        output.sync_all().map_err(io_error)?;
    }
    sync_directory(&destination)
}

// Takes ownership as a map_err adapter while exposing only secret-safe metadata.
#[allow(clippy::needless_pass_by_value)]
fn io_error(error: io::Error) -> String {
    format!(
        "artifact filesystem operation failed ({:?}, OS code {:?})",
        error.kind(),
        error.raw_os_error()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn bundles_are_deterministic_and_round_trip_nested_regular_files() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source");
        fs::create_dir_all(source.join("logs")).unwrap();
        fs::write(source.join("receipt.json"), b"{\"success\":false}").unwrap();
        fs::write(source.join("logs/test.log"), b"actual failure\n").unwrap();
        fs::write(source.join("empty"), b"").unwrap();
        let first = fixture.0.join("first.bundle");
        let second = fixture.0.join("second.bundle");
        pack_directory(&source, &first).unwrap();
        pack_directory(&source, &second).unwrap();
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
        let destination = fixture.0.join("unpacked");
        unpack_directory(&first, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("logs/test.log")).unwrap(),
            b"actual failure\n"
        );
        assert_eq!(fs::read(destination.join("empty")).unwrap(), b"");
        assert!(unpack_directory(&first, &destination).is_err());
        assert!(pack_directory(&source, &source.join("self.bundle")).is_err());
        assert!(pack_directory(&source, &first).is_err());
    }

    fn malformed_bundle(fixture: &Fixture, entries: &serde_json::Value, payload: &[u8]) -> PathBuf {
        let path = fixture.0.join("malformed.bundle");
        let manifest = serde_json::to_vec(entries).unwrap();
        let mut bytes = BUNDLE_MAGIC.to_vec();
        bytes.extend_from_slice(&(manifest.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&manifest);
        bytes.extend_from_slice(payload);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn bundle_extraction_rejects_traversal_overlap_oversize_modes_and_corruption() {
        for path in [
            "../escape",
            "/absolute",
            "a/../escape",
            "a\\escape",
            ".git/config",
        ] {
            let fixture = Fixture::new();
            let input = malformed_bundle(
                &fixture,
                &serde_json::json!([
                    {"path":path,"bytes":0,"sha256":hash_bytes(b""),"mode":384}
                ]),
                b"",
            );
            let destination = fixture.0.join("output");
            assert!(unpack_directory(&input, &destination).is_err(), "{path}");
            assert!(!destination.exists());
        }
        for entries in [
            serde_json::json!([
                {"path":"a","bytes":0,"sha256":hash_bytes(b""),"mode":384},
                {"path":"a/b","bytes":0,"sha256":hash_bytes(b""),"mode":384}]),
            serde_json::json!([{ "path":"a","bytes":MAX_FILE_BYTES,"sha256":hash_bytes(b""),"mode":384 }]),
            serde_json::json!([{ "path":"a","bytes":0,"sha256":hash_bytes(b""),"mode":4095 }]),
        ] {
            let fixture = Fixture::new();
            let input = malformed_bundle(&fixture, &entries, b"");
            let destination = fixture.0.join("output");
            assert!(unpack_directory(&input, &destination).is_err());
            assert!(!destination.exists());
        }
        let fixture = Fixture::new();
        let input = malformed_bundle(
            &fixture,
            &serde_json::json!([
                {"path":"a","bytes":1,"sha256":hash_bytes(b"x"),"mode":384}
            ]),
            b"y",
        );
        assert!(unpack_directory(&input, &fixture.0.join("output")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn bundles_deny_symlinks_and_preserve_executable_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let fixture = Fixture::new();
        let source = fixture.0.join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("runner"), b"#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(source.join("runner"), fs::Permissions::from_mode(0o751)).unwrap();
        let bundle = fixture.0.join("valid.bundle");
        pack_directory(&source, &bundle).unwrap();
        let destination = fixture.0.join("output");
        unpack_directory(&bundle, &destination).unwrap();
        assert_eq!(
            fs::metadata(destination.join("runner"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o751
        );
        symlink("runner", source.join("alias")).unwrap();
        assert!(pack_directory(&source, &fixture.0.join("bad.bundle")).is_err());
        let alias = fixture.0.join("alias");
        symlink(&destination, &alias).unwrap();
        assert!(unpack_directory(&bundle, &alias.join("child")).is_err());
        assert!(!destination.join("child").exists());
    }

    #[test]
    fn cancelled_upload_never_starts_an_http_request() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("receipt.json"), b"{}").unwrap();
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .unwrap();
        let spec = ArtifactSpec {
            inputs: vec![],
            outputs: vec![ArtifactOutput {
                name: "receipt".into(),
                path: "receipt.json".into(),
            }],
        };
        let result = upload_guarded(
            &client,
            "http://127.0.0.1:1",
            "test-token",
            "attempt-1",
            &fixture.0,
            &spec,
            || true,
        );
        assert_eq!(result.unwrap_err(), "artifact upload cancelled");
    }

    #[test]
    fn always_declarations_accept_outputs_only_and_reject_conflicts() {
        let mut step = CommandStep {
            shell: "sh".into(),
            run: "false".into(),
            timeout_secs: 1,
            env: BTreeMap::from([(
                "GPIPE_ARTIFACTS_ALWAYS".into(),
                r#"{"outputs":[{"name":"receipt","path":"receipt.json"}]}"#.into(),
            )]),
        };
        let spec = ArtifactSpec::from_always_steps(&[step.clone()]).unwrap();
        assert_eq!(spec.outputs.len(), 1);
        assert_eq!(
            ArtifactSpec::from_steps(&[step.clone()]).unwrap().outputs,
            [] as [ArtifactOutput; 0]
        );
        step.env.insert(
            "GPIPE_ARTIFACTS_ALWAYS".into(),
            r#"{"inputs":[{"task":"t","name":"n","path":"p"}]}"#.into(),
        );
        assert!(ArtifactSpec::from_always_steps(&[step]).is_err());
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let base = fs::canonicalize(std::env::temp_dir()).unwrap();
            for _ in 0..128 {
                let path = base.join(format!(
                    "gpipe-transfer-test-{}-{}",
                    std::process::id(),
                    NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("test directory: {error}"),
                }
            }
            panic!("cannot allocate test directory");
        }
        fn root(&self) -> PathBuf {
            self.0.join("store")
        }
        fn artifact(&self) -> PathBuf {
            self.root()
                .join(hash_bytes(b"attempt-1"))
                .join(hash_bytes(b"build"))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn request(offset: u64, data: &[u8], finish: bool) -> TransferRequest {
        TransferRequest {
            attempt_id: "attempt-1".to_owned(),
            producer_task: None,
            name: "build".to_owned(),
            offset,
            data: data.to_vec(),
            finish,
        }
    }
    fn get(offset: u64) -> TransferRequest {
        TransferRequest {
            attempt_id: "consumer-1".to_owned(),
            producer_task: Some("build-task".to_owned()),
            ..request(offset, &[], false)
        }
    }

    #[test]
    fn replay_overlap_finish_and_immutable_contents() {
        let fixture = Fixture::new();
        let root = fixture.root();
        let chunk = request(0, b"abc", false);
        assert_eq!(
            transfer(&root, "attempt-1", &chunk).unwrap(),
            TransferResponse::Uploaded { sha256: None }
        );
        transfer(&root, "attempt-1", &chunk).unwrap();
        // Simulates retry after a partial append: verify abc before appending def.
        transfer(&root, "attempt-1", &request(0, b"abcdef", false)).unwrap();
        assert!(transfer(&root, "attempt-1", &request(0, b"changed", false)).is_err());
        assert!(transfer(&root, "attempt-1", &request(7, b"gap", false)).is_err());
        assert!(transfer(&root, "attempt-1", &get(0)).is_err()); // not published
        let finish = request(6, &[], true);
        let expected = TransferResponse::Uploaded {
            sha256: Some(hash_bytes(b"abcdef")),
        };
        assert_eq!(transfer(&root, "attempt-1", &finish).unwrap(), expected);
        assert_eq!(transfer(&root, "attempt-1", &finish).unwrap(), expected);
        transfer(&root, "attempt-1", &chunk).unwrap();
        assert!(transfer(&root, "attempt-1", &request(6, b"new", false)).is_err());
        assert_eq!(
            transfer(&root, "attempt-1", &get(0)).unwrap(),
            TransferResponse::Chunk {
                data: b"abcdef".to_vec(),
                done: true,
                sha256: Some(hash_bytes(b"abcdef"))
            }
        );
    }

    #[test]
    fn core_worker_roundtrip_multiple_chunks_and_empty_file() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source");
        let target = fixture.0.join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        for bytes in [
            (0..CHUNK_BYTES * 2 + 17)
                .map(|index| index.to_le_bytes()[0])
                .collect::<Vec<_>>(),
            Vec::new(),
        ] {
            // Separate roots for each independent immutable publication.
            let root = fixture.root().join(hash_bytes(&bytes));
            fs::write(source.join("output"), &bytes).unwrap();
            send_file(
                &source,
                &ArtifactOutput {
                    name: "build".into(),
                    path: "output".into(),
                },
                |offset, data, finish| {
                    transfer(&root, "attempt-1", &request(offset, &data, finish))
                },
            )
            .unwrap();
            let input = ArtifactInput {
                task: "build-task".into(),
                name: "build".into(),
                path: "input".into(),
            };
            receive_file(&target, &input, |offset| {
                transfer(&root, "attempt-1", &get(offset))
            })
            .unwrap();
            assert_eq!(fs::read(target.join("input")).unwrap(), bytes);
            assert!(
                receive_file(&target, &input, |offset| transfer(
                    &root,
                    "attempt-1",
                    &get(offset)
                ))
                .is_err()
            );
            fs::remove_file(target.join("input")).unwrap();
        }
    }

    #[test]
    fn corrupt_data_never_materializes_and_final_replay_detects_it() {
        let fixture = Fixture::new();
        transfer(&fixture.root(), "attempt-1", &request(0, b"original", true)).unwrap();
        fs::write(fixture.artifact().join("data"), b"modified").unwrap();
        assert!(transfer(&fixture.root(), "attempt-1", &request(8, &[], true)).is_err());
        let target = fixture.0.join("target");
        fs::create_dir(&target).unwrap();
        let input = ArtifactInput {
            task: "build-task".into(),
            name: "build".into(),
            path: "input".into(),
        };
        assert!(
            receive_file(&target, &input, |offset| transfer(
                &fixture.root(),
                "attempt-1",
                &get(offset)
            ))
            .is_err()
        );
        assert!(!target.join("input").exists());
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
        fs::write(fixture.artifact().join("manifest.json"), b"{}").unwrap();
        assert!(transfer(&fixture.root(), "attempt-1", &get(0)).is_err());
    }

    #[test]
    fn traversal_mode_limits_and_missing_artifact_fail() {
        let fixture = Fixture::new();
        for bad in ["../escape", "/absolute", "a\\b", "", ".."] {
            let mut bad_request = request(0, b"data", true);
            bad_request.name = bad.into();
            assert!(transfer(&fixture.root(), "attempt-1", &bad_request).is_err());
        }
        for bad in [
            "../file",
            "/file",
            "a/../file",
            "a\\file",
            "C:file",
            ".git/config",
            "NUL.txt",
            "a//file",
            "a/./file",
        ] {
            assert!(relative_path(bad).is_err(), "{bad}");
        }
        assert!(transfer(&fixture.root(), "../escape", &request(0, b"x", true)).is_err());
        assert!(transfer(&fixture.root(), "other-attempt", &request(0, b"x", true)).is_err());
        assert!(transfer(&fixture.root(), "attempt-1", &get(0)).is_err());
        assert!(
            transfer(
                &fixture.root(),
                "attempt-1",
                &request(0, &vec![1; CHUNK_BYTES + 1], false)
            )
            .is_err()
        );
        assert!(
            transfer(
                &fixture.root(),
                "attempt-1",
                &request(MAX_FILE_BYTES, b"x", false)
            )
            .is_err()
        );
        let mut mixed = get(0);
        mixed.finish = true;
        assert!(transfer(&fixture.root(), "attempt-1", &mixed).is_err());
    }

    #[test]
    fn reject_bad_download_digest_and_wrong_upload_acknowledgment() {
        let fixture = Fixture::new();
        let input = ArtifactInput {
            task: "task".into(),
            name: "build".into(),
            path: "input".into(),
        };
        assert!(
            receive_file(&fixture.0, &input, |_| Ok(TransferResponse::Chunk {
                data: b"bytes".to_vec(),
                done: true,
                sha256: Some(hash_bytes(b"other")),
            }))
            .is_err()
        );
        assert!(!fixture.0.join("input").exists());
        fs::write(fixture.0.join("output"), b"bytes").unwrap();
        let output = ArtifactOutput {
            name: "build".into(),
            path: "output".into(),
        };
        assert!(
            send_file(&fixture.0, &output, |_, _, finish| Ok(
                TransferResponse::Uploaded {
                    sha256: finish.then(|| hash_bytes(b"other")),
                }
            ))
            .is_err()
        );
    }

    #[test]
    fn specs_default_conflicts_and_paths() {
        let step = |json: &str| crate::plan::CommandStep {
            shell: "sh".into(),
            run: "true".into(),
            env: BTreeMap::from([("GPIPE_ARTIFACTS".into(), json.into())]),
            timeout_secs: 1,
        };
        assert_eq!(
            ArtifactSpec::from_steps(&[]).unwrap(),
            ArtifactSpec::default()
        );
        assert!(
            ArtifactSpec::from_steps(&[step("{}"), step("{\"inputs\":[],\"outputs\":[]}")]).is_ok()
        );
        assert!(
            ArtifactSpec::from_steps(&[
                step("{}"),
                step("{\"outputs\":[{\"name\":\"build\",\"path\":\"out\"}]}")
            ])
            .is_err()
        );
        let spec: ArtifactSpec =
            serde_json::from_str("{\"outputs\":[{\"name\":\"build\",\"path\":\"out\"}]}").unwrap();
        spec.validate().unwrap();
        let mut duplicate = spec.clone();
        duplicate.outputs.push(spec.outputs[0].clone());
        assert!(duplicate.validate().is_err());
        let overlap = ArtifactSpec {
            inputs: vec![
                ArtifactInput {
                    task: "task".into(),
                    name: "one".into(),
                    path: "A".into(),
                },
                ArtifactInput {
                    task: "task".into(),
                    name: "two".into(),
                    path: "a/child".into(),
                },
            ],
            outputs: vec![],
        };
        assert!(overlap.validate().is_err());
    }

    #[test]
    fn locked_and_unpublished_artifacts_cannot_be_downloaded() {
        let fixture = Fixture::new();
        transfer(&fixture.root(), "attempt-1", &request(0, b"partial", false)).unwrap();
        assert!(transfer(&fixture.root(), "attempt-1", &get(0)).is_err());
        let lock = ArtifactLock::acquire(&fixture.artifact()).unwrap();
        assert!(transfer(&fixture.root(), "attempt-1", &request(7, b"complete", true)).is_err());
        drop(lock);
        transfer(&fixture.root(), "attempt-1", &request(7, b"complete", true)).unwrap();
        assert!(transfer(&fixture.root(), "attempt-1", &get(0)).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_store_source_and_destination_escape_rejected() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let outside = Fixture::new();
        fs::write(outside.0.join("secret"), b"secret").unwrap();
        symlink(&outside.0, fixture.0.join("linked-store")).unwrap();
        assert!(
            transfer(
                &fixture.0.join("linked-store"),
                "attempt-1",
                &request(0, b"x", true)
            )
            .is_err()
        );
        symlink(&outside.0, fixture.0.join("escape")).unwrap();
        assert!(workspace_file(&fixture.0, "escape/secret", false).is_err());
        assert!(workspace_file(&fixture.0, "escape/output", true).is_err());
        symlink(outside.0.join("secret"), fixture.0.join("leaf")).unwrap();
        assert!(workspace_file(&fixture.0, "leaf", false).is_err());
        assert!(workspace_file(&fixture.0, "leaf", true).is_err());
        assert_eq!(fs::read(outside.0.join("secret")).unwrap(), b"secret");
    }

    #[test]
    fn streamed_digest_covers_every_chunk_and_rejects_middle_corruption() {
        let fixture = Fixture::new();
        let bytes: Vec<u8> = (0..CHUNK_BYTES * 3 + 11)
            .map(|index| (index ^ (index >> 8) ^ (index >> 16)).to_le_bytes()[0])
            .collect();
        let expected = hash_bytes(&bytes);
        let path = fixture.0.join("source");
        fs::write(&path, &bytes).unwrap();
        let mut file = regular(&path, false).unwrap();
        file.seek(SeekFrom::Start(19)).unwrap();
        assert_eq!(hash_file(&mut file).unwrap(), expected);
        let input = ArtifactInput {
            task: "task".into(),
            name: "build".into(),
            path: "destination".into(),
        };
        let result = receive_file(&fixture.0, &input, |offset| {
            let start = usize::try_from(offset).unwrap();
            let end = (start + CHUNK_BYTES).min(bytes.len());
            let mut data = bytes[start..end].to_vec();
            if start == CHUNK_BYTES {
                data[17] ^= 1;
            }
            let done = end == bytes.len();
            Ok(TransferResponse::Chunk {
                data,
                done,
                sha256: done.then(|| expected.clone()),
            })
        });
        assert!(result.is_err());
        assert!(!fixture.0.join("destination").exists());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn nested_download_creates_missing_parents_and_never_clobbers() {
        let fixture = Fixture::new();
        let input = ArtifactInput {
            task: "task".into(),
            name: "build".into(),
            path: "incoming/deep/payload.txt".into(),
        };
        let response = || TransferResponse::Chunk {
            data: b"payload".to_vec(),
            done: true,
            sha256: Some(hash_bytes(b"payload")),
        };
        receive_file(&fixture.0, &input, |_| Ok(response())).unwrap();
        let destination = fixture.0.join(&input.path);
        assert_eq!(fs::read(&destination).unwrap(), b"payload");
        assert!(
            receive_file(&fixture.0, &input, |_| panic!(
                "must reject existing destination before requesting data"
            ))
            .is_err()
        );
        assert_eq!(fs::read(&destination).unwrap(), b"payload");
        fs::write(fixture.0.join("not-directory"), b"keep").unwrap();
        assert!(prepare_destination(&fixture.0, "not-directory/child/file").is_err());
        assert!(prepare_destination(&fixture.0, "../escape/file").is_err());
    }

    #[test]
    fn filesystem_diagnostics_expose_kind_not_arbitrary_error_text() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "sensitive-token");
        let message = io_error(error);
        assert!(message.contains("PermissionDenied"));
        assert!(!message.contains("sensitive-token"));
    }

    #[cfg(unix)]
    #[test]
    fn missing_download_parents_do_not_follow_directory_or_leaf_symlinks() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let outside = Fixture::new();
        symlink(&outside.0, fixture.0.join("escape")).unwrap();
        assert!(prepare_destination(&fixture.0, "escape/missing/payload.txt").is_err());
        assert!(!outside.0.join("missing").exists());
        symlink(outside.0.join("absent"), fixture.0.join("dangling")).unwrap();
        assert!(prepare_destination(&fixture.0, "dangling/child/payload.txt").is_err());
        assert!(!outside.0.join("absent").exists());
        fs::create_dir(fixture.0.join("incoming")).unwrap();
        symlink(
            outside.0.join("absent-file"),
            fixture.0.join("incoming/payload.txt"),
        )
        .unwrap();
        assert!(prepare_destination(&fixture.0, "incoming/payload.txt").is_err());
        assert!(!outside.0.join("absent-file").exists());
    }

    #[test]
    fn endpoint_security_and_wire_contract() {
        assert_eq!(
            endpoint("https://example.invalid/v1/rpc").unwrap().path(),
            "/v1/artifacts"
        );
        assert!(endpoint("http://127.0.0.1:1234").is_ok());
        for bad in [
            "http://example.invalid",
            "https://user:secret@example.invalid",
            "https://example.invalid/?secret=1",
            "file:///tmp/data",
            "https://example.invalid/other",
        ] {
            assert!(endpoint(bad).is_err());
        }
        let response = TransferResponse::Uploaded {
            sha256: Some(hash_bytes(b"abc")),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("UPLOADED"));
        assert_eq!(
            serde_json::from_str::<TransferResponse>(&json).unwrap(),
            response
        );
        assert_eq!(
            hash_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
