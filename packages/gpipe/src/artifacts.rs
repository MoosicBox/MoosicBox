//! Explicit local artifact storage for trusted native workers. No network IO.
//!
//! Artifacts are regular files, not directory archives. Names are labels, never
//! paths. Both source and destination paths are confined to a workspace captured
//! by `open` (the current directory), or supplied to `open_with_workspace`.
//! Symlinks/reparse points are rejected in every existing path component, even
//! when they point inside the workspace. Destinations must not already exist.
//!
//! A fail-fast on-disk lock serializes cooperating store users, including pins
//! and pruning. A crashed process can leave `.lock`; an operator must confirm
//! that no operation is active before removing it. Store and workspace ancestors
//! must not be concurrently renamed or mutated by noncooperating processes:
//! portable std path checks are not a defense against hostile filesystem races.
//! Existing hard links are not provenance checked and concurrent in-place source
//! writes are not snapshot-isolated (length changes are detected).
//! Use private worker-owned directories, not adversarial shared directories.
//! Pruning metadata uses memory proportional to the number of store entries;
//! artifact payload size never determines copy-buffer memory usage.
//!
//! Files are streamed through a fixed 64 KiB buffer. Publication uses synced
//! staging files and atomic no-clobber hard links on the same filesystem. A
//! manifest is published only after its blob; interrupted publication may leave
//! harmless orphan blobs/staging files, but never a partially visible manifest.
//! Hard-link support is required. File contents are synced; directory-entry
//! durability across power loss is filesystem-dependent. No permissions, owners,
//! timestamps, executable bits, or directory trees are transported.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const BUFFER_BYTES: usize = 64 * 1024;
const MAX_MANIFEST_BYTES: u64 = 4096;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Immutable content-addressed manifest. `id` hashes the canonical JSON tuple
/// `(version, name, sha256, size_bytes)`, not the serialized struct field order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "SCREAMING_SNAKE_CASE")]
pub struct ArtifactManifest {
    /// Schema version, currently 1.
    pub version: u32,
    /// Lowercase SHA-256 of canonical manifest identity data.
    pub id: String,
    /// Portable label: 1–128 ASCII letters, digits, `_`, `-`, or `.`; no dot labels.
    pub name: String,
    /// Lowercase SHA-256 of file bytes.
    pub sha256: String,
    /// Exact byte length of the blob.
    pub size_bytes: u64,
}

impl ArtifactManifest {
    fn identity(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(&(self.version, &self.name, &self.sha256, self.size_bytes))
            .map_err(|error| error.to_string())?;
        Ok(hex_digest(&Sha256::digest(bytes)))
    }

    fn validate(&self) -> Result<(), String> {
        label(&self.name)?;
        digest_id(&self.id)?;
        digest_id(&self.sha256)?;
        if self.version != 1 || self.identity()? != self.id {
            return Err("invalid artifact manifest identity or version".to_owned());
        }
        Ok(())
    }
}

/// Retention outcome; pinned bytes may intentionally exceed any caller budget.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct PruneResult {
    /// Number of expired, unpinned manifests removed.
    pub manifests_removed: u64,
    /// Number of unreferenced blobs removed, including orphan publications.
    pub blobs_removed: u64,
    /// Sum of removed blob lengths.
    pub bytes_removed: u64,
    /// Number of manifests protected by one or more dependency-owner pins.
    pub pinned_manifests: u64,
}

/// Local content-addressed store. Operations fail explicitly on unavailable,
/// missing, corrupt, busy, or unsafe paths; there is no implicit remote fallback.
#[derive(Debug)]
pub struct ArtifactStore {
    root: PathBuf,
    workspace: PathBuf,
}

impl ArtifactStore {
    /// Open/create a store, capturing the current directory as workspace.
    ///
    /// # Errors
    /// * Current directory lookup fails, or `open_with_workspace` fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let workspace = std::env::current_dir().map_err(|error| error.to_string())?;
        Self::open_with_workspace(path, workspace)
    }

    /// Open/create a private store with an explicit, existing workspace root.
    /// Store and workspace paths must not contain symlinks, reparse points, or
    /// parent traversal. A store may be outside the workspace.
    ///
    /// # Errors
    /// * A path is unsafe/unavailable, creation fails, or an existing store layout
    ///   is not made up of real directories. Windows supports local drive paths
    ///   only, not UNC/network shares.
    pub fn open_with_workspace(
        path: impl AsRef<Path>,
        workspace: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let workspace = absolute(workspace.as_ref())?;
        check_path(&workspace, false)?;
        if !workspace.is_dir() {
            return Err("workspace is not a directory".to_owned());
        }
        let workspace = fs::canonicalize(workspace).map_err(io_error)?;
        let root = absolute(path.as_ref())?;
        create_directories(&root)?;
        let root = fs::canonicalize(root).map_err(io_error)?;
        for directory in ["blobs", "manifests", "pins", "staging"] {
            create_directories(&root.join(directory))?;
        }
        Ok(Self { root, workspace })
    }

    /// Stream a workspace file into the store and publish its manifest last.
    /// Repeated identical publication is idempotent and verifies existing bytes.
    ///
    /// # Errors
    /// * The source is outside the workspace, unsafe, missing, or not regular.
    /// * Name validation, locking, IO, hashing or atomic publication fails.
    /// * Existing content at the same identity is corrupt or inconsistent.
    pub fn publish(
        &self,
        source: impl AsRef<Path>,
        name: &str,
    ) -> Result<ArtifactManifest, String> {
        label(name)?;
        let _lock = self.lock()?;
        let source = self.workspace_path(source.as_ref(), false)?;
        let mut input = regular_file(&source)?;
        let (staging, mut output) = TempFile::create(&self.root.join("staging"))?;
        let (sha256, size_bytes) = stream_copy(&mut input, &mut output)?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        let blob = self.root.join("blobs").join(&sha256);
        match staging.publish(&blob) {
            Ok(()) => {}
            Err(error) if blob.exists() => {
                verify_blob(&blob, &sha256, size_bytes)?;
                // An existing valid blob is the only permissible collision.
                let _ = error;
            }
            Err(error) => return Err(error),
        }
        let mut manifest = ArtifactManifest {
            version: 1,
            id: String::new(),
            name: name.to_owned(),
            sha256,
            size_bytes,
        };
        manifest.id = manifest.identity()?;
        let manifest_path = self.root.join("manifests").join(&manifest.id);
        let bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
        let (staging, mut output) = TempFile::create(&self.root.join("staging"))?;
        output.write_all(&bytes).map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        if let Err(error) = staging.publish(&manifest_path)
            && (!manifest_path.exists() || read_manifest(&manifest_path)? != manifest)
        {
            return Err(error);
        }
        Ok(manifest)
    }

    /// Load an available manifest by its SHA-256 ID. Blob availability and
    /// checksums are checked by materialization/pinning, not just manifest lookup.
    ///
    /// # Errors
    /// * ID is unsafe, store is busy, manifest is missing/corrupt, or IO fails.
    pub fn manifest(&self, id: &str) -> Result<ArtifactManifest, String> {
        digest_id(id)?;
        let _lock = self.lock()?;
        read_manifest(&self.root.join("manifests").join(id))
    }

    /// Verify a stored manifest and stream its blob to a new workspace file.
    /// Hash/length verification precedes atomic no-clobber publication. Existing
    /// destinations are never overwritten; destination parent must already exist.
    ///
    /// # Errors
    /// * Manifest/blob is missing, changed or corrupt; destination escapes the
    ///   workspace, contains a symlink, exists, or has a missing parent.
    /// * Locking, IO or atomic publication fails. On failure no destination is
    ///   published; temporary-file cleanup is best-effort.
    pub fn materialize(
        &self,
        manifest: &ArtifactManifest,
        destination: impl AsRef<Path>,
    ) -> Result<(), String> {
        manifest.validate()?;
        let _lock = self.lock()?;
        self.require_manifest(manifest)?;
        let destination = self.workspace_path(destination.as_ref(), true)?;
        if destination.starts_with(&self.root) {
            return Err("cannot materialize into artifact store internals".to_owned());
        }
        if destination.try_exists().map_err(io_error)? {
            return Err("artifact destination already exists".to_owned());
        }
        let parent = destination.parent().ok_or("destination has no parent")?;
        check_path(parent, false)?;
        let mut input = regular_file(&self.root.join("blobs").join(&manifest.sha256))?;
        if input.metadata().map_err(io_error)?.len() != manifest.size_bytes {
            return Err("artifact length mismatch".to_owned());
        }
        let (staging, mut output) = TempFile::create(parent)?;
        let (hash, length) = stream_copy(&mut input, &mut output)?;
        if hash != manifest.sha256 || length != manifest.size_bytes {
            return Err("artifact checksum or length mismatch".to_owned());
        }
        output.sync_all().map_err(io_error)?;
        drop(output);
        check_path(&destination, true)?;
        staging.publish(&destination)
    }

    /// Persist an idempotent owner-specific dependency pin. Owners use the label
    /// alphabet but must be lowercase to avoid case-insensitive filesystem aliases.
    /// Different downstream
    /// owners cannot accidentally unpin each other. Pin before scheduling a
    /// dependent consumer; a race with earlier pruning returns an explicit error.
    ///
    /// # Errors
    /// * Owner/manifest invalid, artifact missing/corrupt, store busy, or IO fails.
    pub fn pin(&self, manifest: &ArtifactManifest, owner: &str) -> Result<(), String> {
        owner_id(owner)?;
        manifest.validate()?;
        let _lock = self.lock()?;
        self.require_manifest(manifest)?;
        verify_blob(
            &self.root.join("blobs").join(&manifest.sha256),
            &manifest.sha256,
            manifest.size_bytes,
        )?;
        let directory = self.root.join("pins").join(&manifest.id);
        create_directories(&directory)?;
        let pin = directory.join(owner);
        if pin.try_exists().map_err(io_error)? {
            regular_file(&pin)?;
            return Ok(());
        }
        let (staging, output) = TempFile::create(&self.root.join("staging"))?;
        output.sync_all().map_err(io_error)?;
        drop(output);
        staging.publish(&pin)
    }

    /// Remove only this owner's pin; other dependency owners remain protected.
    ///
    /// # Errors
    /// * ID/owner invalid, pin missing/unsafe, store busy, or IO fails. Empty
    ///   owner directories are left in place and are not treated as pins.
    pub fn unpin(&self, id: &str, owner: &str) -> Result<(), String> {
        digest_id(id)?;
        owner_id(owner)?;
        let _lock = self.lock()?;
        let pin = self.root.join("pins").join(id).join(owner);
        regular_file(&pin)?;
        fs::remove_file(pin).map_err(io_error)
    }

    /// Remove manifests at least `max_age` old by publication mtime, excluding
    /// all pinned manifests. Then remove blobs not referenced by retained
    /// manifests. A zero duration removes every unpinned manifest. Future-dated
    /// entries are retained. Publishing identical content does not refresh age.
    /// Pins survive reopening; no size target can override a pin.
    ///
    /// # Errors
    /// * Store is busy, an entry/pin/manifest is malformed or unsafe, a pin refers
    ///   to a missing manifest, or IO fails. Validation completes before deletion;
    ///   later IO failure can leave partial pruning, never deletion of a pin's
    ///   manifest/blob. Staging files are not automatically reclaimed.
    pub fn prune(&self, max_age: Duration) -> Result<PruneResult, String> {
        let _lock = self.lock()?;
        let now = SystemTime::now();
        let mut pinned = BTreeSet::new();
        for entry in fs::read_dir(self.root.join("pins")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            let id = filename(&path)?;
            digest_id(&id)?;
            check_path(&path, false)?;
            if !path.is_dir() {
                return Err("pin entry is not a directory".to_owned());
            }
            let mut has_pin = false;
            for owner in fs::read_dir(&path).map_err(io_error)? {
                let owner = owner.map_err(io_error)?.path();
                owner_id(&filename(&owner)?)?;
                regular_file(&owner)?;
                has_pin = true;
            }
            if has_pin {
                read_manifest(&self.root.join("manifests").join(&id))?;
                pinned.insert(id);
            }
        }
        let mut retained_blobs = BTreeSet::new();
        let mut remove_manifests = Vec::new();
        for entry in fs::read_dir(self.root.join("manifests")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            let manifest = read_manifest(&path)?;
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .map_err(io_error)?;
            let expired = now.duration_since(modified).is_ok_and(|age| age >= max_age);
            if !expired || pinned.contains(&manifest.id) {
                let blob = regular_file(&self.root.join("blobs").join(&manifest.sha256))?;
                if blob.metadata().map_err(io_error)?.len() != manifest.size_bytes {
                    return Err("retained artifact length mismatch".to_owned());
                }
                retained_blobs.insert(manifest.sha256);
            } else {
                remove_manifests.push(path);
            }
        }
        let mut remove_blobs = Vec::new();
        for entry in fs::read_dir(self.root.join("blobs")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            let id = filename(&path)?;
            digest_id(&id)?;
            let file = regular_file(&path)?;
            if !retained_blobs.contains(&id) {
                remove_blobs.push((path, file.metadata().map_err(io_error)?.len()));
            }
        }
        let mut result = PruneResult {
            pinned_manifests: pinned.len() as u64,
            ..PruneResult::default()
        };
        for path in remove_manifests {
            fs::remove_file(path).map_err(io_error)?;
            result.manifests_removed += 1;
        }
        for (path, bytes) in remove_blobs {
            fs::remove_file(path).map_err(io_error)?;
            result.blobs_removed += 1;
            result.bytes_removed = result.bytes_removed.saturating_add(bytes);
        }
        Ok(result)
    }

    fn require_manifest(&self, manifest: &ArtifactManifest) -> Result<(), String> {
        if read_manifest(&self.root.join("manifests").join(&manifest.id))? != *manifest {
            return Err("supplied manifest differs from stored manifest".to_owned());
        }
        Ok(())
    }

    fn workspace_path(&self, path: &Path, missing_leaf: bool) -> Result<PathBuf, String> {
        let path = if path.is_absolute() {
            absolute(path)?
        } else {
            absolute(&self.workspace.join(path))?
        };
        match path.components().next_back() {
            Some(Component::Normal(_)) => {}
            _ => return Err("artifact path must end in a normal filename".to_owned()),
        }
        check_path(&path, missing_leaf)?;
        // Canonicalize only AFTER rejecting links. This also normalizes local
        // Windows drive/verbatim prefixes before confinement comparisons.
        let path = if missing_leaf && !path.try_exists().map_err(io_error)? {
            let parent = fs::canonicalize(path.parent().ok_or("destination has no parent")?)
                .map_err(io_error)?;
            parent.join(path.file_name().ok_or("destination has no filename")?)
        } else {
            fs::canonicalize(path).map_err(io_error)?
        };
        if !path.starts_with(&self.workspace) || path == self.workspace {
            return Err("artifact path escapes workspace or names its root".to_owned());
        }
        Ok(path)
    }

    fn lock(&self) -> Result<StoreLock, String> {
        check_path(&self.root, false)?;
        for directory in ["blobs", "manifests", "pins", "staging"] {
            let path = self.root.join(directory);
            check_path(&path, false)?;
            if !path.is_dir() {
                return Err("invalid artifact store layout".to_owned());
            }
        }
        let path = self.root.join(".lock");
        fs::create_dir(&path)
            .map_err(|error| format!("artifact store busy or lock unavailable: {error}"))?;
        Ok(StoreLock(path))
    }
}

struct StoreLock(PathBuf);
impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

struct TempFile(PathBuf);
impl TempFile {
    fn create(directory: &Path) -> Result<(Self, File), String> {
        check_path(directory, false)?;
        for _ in 0..128 {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = directory.join(format!(".gpipe-{}-{sequence}.tmp", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((Self(path), file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        Err("cannot allocate artifact staging file".to_owned())
    }

    fn publish(&self, destination: &Path) -> Result<(), String> {
        check_path(&self.0, false)?;
        check_path(destination, true)?;
        // Unlike rename, hard_link atomically refuses replacement on all targets.
        fs::hard_link(&self.0, destination).map_err(io_error)?;
        Ok(())
    }
}
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn label(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value == "." || value == ".."
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        // Avoid Windows device aliases, including aliases with extensions.
        || matches!(value.split('.').next().unwrap_or("").to_ascii_uppercase().as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9")
        || value.ends_with('.')
    {
        return Err("unsafe artifact name or pin owner".to_owned());
    }
    Ok(())
}

fn owner_id(value: &str) -> Result<(), String> {
    label(value)?;
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err("pin owners must be lowercase to avoid filesystem case aliases".to_owned());
    }
    Ok(())
}

fn digest_id(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("artifact ID must be 64 lowercase SHA-256 hex digits".to_owned());
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("parent traversal is not permitted".to_owned());
    }
    #[cfg(windows)]
    for part in path.components() {
        match part {
            Component::Normal(name) => {
                let name = name.to_str().ok_or("non-Unicode Windows artifact path")?;
                if name.contains(':') || name.ends_with(['.', ' ']) {
                    return Err(
                        "Windows path aliases and alternate data streams are forbidden".to_owned(),
                    );
                }
            }
            Component::Prefix(prefix)
                if !matches!(
                    prefix.kind(),
                    std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)
                ) =>
            {
                return Err("only local Windows drive paths are supported".to_owned());
            }
            _ => {}
        }
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(io_error)?.join(path)
    };
    if !path.is_absolute() {
        return Err("artifact path is not absolute".to_owned());
    }
    Ok(path
        .components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect())
}

fn check_path(path: &Path, missing_leaf: bool) -> Result<(), String> {
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir) {
            return Err("parent traversal is not permitted".to_owned());
        }
        current.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || is_reparse(&metadata) {
                    return Err(format!(
                        "symlink/reparse path is forbidden: {}",
                        current.display()
                    ));
                }
            }
            Err(error)
                if missing_leaf && current == path && error.kind() == io::ErrorKind::NotFound =>
            {
                return Ok(());
            }
            Err(error) => {
                return Err(format!(
                    "unavailable artifact path {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
const fn is_reparse(_: &fs::Metadata) -> bool {
    false
}

fn create_directories(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(error)),
        }
        check_path(&current, false)?;
        if !current.is_dir() {
            return Err("artifact directory is not a directory".to_owned());
        }
    }
    Ok(())
}

fn regular_file(path: &Path) -> Result<File, String> {
    check_path(path, false)?;
    if !fs::symlink_metadata(path).map_err(io_error)?.is_file() {
        return Err("artifact is not a regular file".to_owned());
    }
    let file = File::open(path).map_err(io_error)?;
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err("artifact is not a regular file".to_owned());
    }
    Ok(file)
}

fn read_manifest(path: &Path) -> Result<ArtifactManifest, String> {
    let file = regular_file(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("oversized artifact manifest".to_owned());
    }
    let manifest: ArtifactManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("corrupt artifact manifest: {error}"))?;
    manifest.validate()?;
    if filename(path)? != manifest.id {
        return Err("manifest filename/identity mismatch".to_owned());
    }
    Ok(manifest)
}

fn stream_copy(input: &mut File, output: &mut impl Write) -> Result<(String, u64), String> {
    let expected_length = input.metadata().map_err(io_error)?.len();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    let mut digest = Sha256::new();
    let mut length = 0_u64;
    loop {
        let count = match input.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(error)),
        };
        if count == 0 {
            break;
        }
        length = length
            .checked_add(count as u64)
            .ok_or("artifact length overflow")?;
        if length > expected_length {
            return Err("artifact grew during copy".to_owned());
        }
        digest.update(&buffer[..count]);
        output.write_all(&buffer[..count]).map_err(io_error)?;
    }
    if length != expected_length {
        return Err("artifact length changed during copy".to_owned());
    }
    Ok((hex_digest(&digest.finalize()), length))
}

fn verify_blob(path: &Path, hash: &str, length: u64) -> Result<(), String> {
    let mut input = regular_file(path)?;
    if input.metadata().map_err(io_error)?.len() != length {
        return Err("stored artifact length mismatch".to_owned());
    }
    let (actual_hash, actual_length) = stream_copy(&mut input, &mut io::sink())?;
    if actual_hash != hash || actual_length != length {
        return Err("stored artifact checksum or length mismatch".to_owned());
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
}
fn filename(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "non-UTF-8 artifact entry".to_owned())
}
#[allow(clippy::needless_pass_by_value)] // Passed directly to Result::map_err.
fn io_error(error: io::Error) -> String {
    format!("artifact IO: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            // Canonicalize the test base because macOS /tmp is commonly a symlink.
            let base = fs::canonicalize(std::env::temp_dir()).unwrap();
            for _ in 0..128 {
                let path = base.join(format!(
                    "gpipe-artifact-test-{}-{}",
                    std::process::id(),
                    NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("create artifact test workspace: {error}"),
                }
            }
            panic!("cannot allocate artifact test workspace");
        }
        fn store(&self) -> ArtifactStore {
            ArtifactStore::open_with_workspace(self.0.join("store"), &self.0).unwrap()
        }
        fn source(&self, name: &str, content: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, content).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn roundtrip_idempotent_publication_and_missing_failure() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let bytes = vec![42; BUFFER_BYTES * 3 + 7];
        let source = fixture.source("input", &bytes);
        let manifest = store.publish(&source, "build.bin").unwrap();
        assert_eq!(store.publish(source, "build.bin").unwrap(), manifest);
        assert_eq!(store.manifest(&manifest.id).unwrap(), manifest);
        store.materialize(&manifest, "output").unwrap();
        assert_eq!(fs::read(fixture.0.join("output")).unwrap(), bytes);
        assert!(store.materialize(&manifest, "output").is_err());
        fs::remove_file(store.root.join("blobs").join(&manifest.sha256)).unwrap();
        assert!(store.materialize(&manifest, "missing").is_err());
        assert!(!fixture.0.join("missing").exists());
    }

    #[test]
    fn corruption_never_publishes_destination() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let source = fixture.source("input", b"original");
        let manifest = store.publish(&source, "build").unwrap();
        fs::write(store.root.join("blobs").join(&manifest.sha256), b"tampered").unwrap();
        assert!(store.materialize(&manifest, "output").is_err());
        assert!(!fixture.0.join("output").exists());
        assert!(store.pin(&manifest, "consumer").is_err());
        assert!(store.publish(source, "build").is_err());
        assert_eq!(store.manifest(&manifest.id).unwrap(), manifest);
    }

    #[test]
    fn traversal_and_forged_manifests_are_rejected() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fixture.source("input", b"data");
        assert!(store.publish("../input", "build").is_err());
        assert!(store.publish("input", "../build").is_err());
        assert!(store.manifest("../outside").is_err());
        let manifest = store.publish("input", "build").unwrap();
        assert!(store.materialize(&manifest, "../output").is_err());
        assert!(store.materialize(&manifest, "store/injected").is_err());
        let mut forged = manifest;
        forged.size_bytes += 1;
        assert!(store.materialize(&forged, "output").is_err());
        assert!(store.unpin("../outside", "consumer").is_err());
    }

    #[test]
    fn pins_survive_reopen_and_shared_blob_retention() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fixture.source("input", b"shared");
        let first = store.publish("input", "first").unwrap();
        let second = store.publish("input", "second").unwrap();
        store.pin(&first, "consumer-a").unwrap();
        store.pin(&first, "consumer-b").unwrap();
        store.unpin(&first.id, "consumer-a").unwrap();
        drop(store);
        let store = fixture.store();
        let pruned = store.prune(Duration::ZERO).unwrap();
        assert_eq!(pruned.manifests_removed, 1);
        assert_eq!(pruned.pinned_manifests, 1);
        assert_eq!(pruned.blobs_removed, 0);
        assert!(store.manifest(&second.id).is_err());
        store.materialize(&first, "output").unwrap();
        store.unpin(&first.id, "consumer-b").unwrap();
        let pruned = store.prune(Duration::ZERO).unwrap();
        assert_eq!(pruned.manifests_removed, 1);
        assert_eq!(pruned.blobs_removed, 1);
    }

    #[test]
    fn orphan_blob_is_collectible_and_published_manifest_requires_blob() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fixture.source("input", b"abc");
        let manifest = store.publish("input", "build").unwrap();
        assert_eq!(
            manifest.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Simulate interruption after publishing the blob but before manifest.
        fs::remove_file(store.root.join("manifests").join(&manifest.id)).unwrap();
        assert!(store.materialize(&manifest, "output").is_err());
        assert!(!fixture.0.join("output").exists());
        let pruned = store.prune(Duration::ZERO).unwrap();
        assert_eq!(pruned.manifests_removed, 0);
        assert_eq!(pruned.blobs_removed, 1);
        assert_eq!(pruned.bytes_removed, 3);
    }

    #[test]
    fn atomic_no_clobber_and_interrupted_staging() {
        let fixture = Fixture::new();
        let store = fixture.store();
        let (staging, mut file) = TempFile::create(&store.root.join("staging")).unwrap();
        file.write_all(b"partial").unwrap();
        assert_eq!(
            fs::read_dir(store.root.join("manifests")).unwrap().count(),
            0
        );
        let destination = fixture.source("existing", b"keep");
        assert!(staging.publish(&destination).is_err());
        assert_eq!(fs::read(destination).unwrap(), b"keep");
        drop(file);
        drop(staging);
        assert_eq!(fs::read_dir(store.root.join("staging")).unwrap().count(), 0);
    }

    #[test]
    fn lock_serializes_pin_and_prune_and_corrupt_metadata_fails_closed() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fixture.source("input", b"content");
        let manifest = store.publish("input", "build").unwrap();
        let lock = store.lock().unwrap();
        assert!(store.prune(Duration::ZERO).is_err());
        assert!(store.pin(&manifest, "consumer").is_err());
        drop(lock);
        store.pin(&manifest, "consumer").unwrap();
        fs::write(store.root.join("manifests").join(&manifest.id), b"{}").unwrap();
        assert!(store.prune(Duration::ZERO).is_err());
        assert!(store.root.join("blobs").join(&manifest.sha256).exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_sources_destinations_and_store_paths_are_rejected() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let outside = Fixture::new();
        let store = fixture.store();
        let secret = outside.source("secret", b"private");
        symlink(&secret, fixture.0.join("input-link")).unwrap();
        assert!(store.publish("input-link", "build").is_err());
        symlink(&outside.0, fixture.0.join("outside")).unwrap();
        assert!(store.publish("outside/secret", "build").is_err());
        fixture.source("input", b"safe");
        let manifest = store.publish("input", "build").unwrap();
        assert!(store.materialize(&manifest, "outside/output").is_err());
        symlink(&secret, fixture.0.join("output-link")).unwrap();
        assert!(store.materialize(&manifest, "output-link").is_err());
        assert_eq!(fs::read(secret).unwrap(), b"private");
        assert!(!outside.0.join("output").exists());
        assert!(
            ArtifactStore::open_with_workspace(fixture.0.join("outside/store"), &fixture.0)
                .is_err()
        );
    }
}
