//! Explicit static registrations, not a general-purpose filesystem host policy.

use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use cap_std::fs::Dir;
use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};
use hyperchad_renderer_bmux::renderer::{Error, HostResponse};

use super::{asset_path, failure};

#[cfg(test)]
#[path = "assets/tests.rs"]
mod tests;

const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
const MAX_READS: usize = 8;

pub(super) struct Assets {
    routes: Vec<Registration>,
    active: Arc<AtomicUsize>,
}

struct Registration {
    asset: StaticAssetRoute,
    // Initialization happens only on a blocking worker. Retaining the capability means
    // replacing a root's pathname later cannot redirect this registration elsewhere.
    root: OnceLock<Dir>,
}

struct ReadSlot(Arc<AtomicUsize>);

impl Drop for ReadSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

impl Assets {
    #[must_use]
    pub(super) fn new(routes: Vec<StaticAssetRoute>) -> Self {
        Self {
            routes: routes
                .into_iter()
                .map(|asset| Registration {
                    asset,
                    root: OnceLock::new(),
                })
                .collect(),
            active: Arc::default(),
        }
    }

    pub(super) async fn load(
        self: &Arc<Self>,
        path: &str,
        limit: usize,
    ) -> Result<Option<HostResponse>, Error> {
        let limit = limit.min(MAX_ASSET_BYTES);
        // Registration order is authoritative, including overlapping directory mounts.
        for (index, registration) in self.routes.iter().enumerate() {
            let Some(route) = asset_path(&registration.asset.route)? else {
                continue;
            };
            let relative = match &registration.asset.target {
                AssetPathTarget::Directory(_) => {
                    let route = route.trim_end_matches('/');
                    let Some(relative) = path.strip_prefix(route).and_then(|s| s.strip_prefix('/'))
                    else {
                        continue;
                    };
                    Some(relative.to_owned())
                }
                _ if route == path => None,
                _ => continue,
            };
            if let AssetPathTarget::FileContents(bytes) = &registration.asset.target {
                if bytes.len() > limit {
                    return Err(failure("registered asset exceeds byte limit"));
                }
                return Ok(Some(HostResponse::Bytes(bytes.to_vec())));
            }
            self.active
                .try_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                    (active < MAX_READS).then_some(active + 1)
                })
                .map_err(|_| failure("registered asset read concurrency limit"))?;
            let slot = ReadSlot(self.active.clone());
            let assets = self.clone();
            let result = switchy::unsync::runtime::Handle::current()
                .spawn_blocking(move || {
                    // Slot remains held even if the caller drops its future while IO continues.
                    let _slot = slot;
                    assets.routes[index].read(relative.as_deref(), limit)
                })
                .await?;
            match result {
                Ok(bytes) => return Ok(Some(HostResponse::Bytes(bytes))),
                Err(error)
                    if registration.asset.not_found_behavior
                        == Some(hyperchad_renderer::assets::AssetNotFoundBehavior::Fallthrough)
                        && error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }
}

impl Registration {
    fn read(&self, relative: Option<&str>, limit: usize) -> Result<Vec<u8>, Error> {
        let (root_path, child) = match &self.asset.target {
            AssetPathTarget::Directory(root) => {
                (root.as_path(), PathBuf::from(relative.unwrap_or_default()))
            }
            AssetPathTarget::File(path) => (
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new(".")),
                PathBuf::from(
                    path.file_name()
                        .ok_or_else(|| failure("registered file has no filename"))?,
                ),
            ),
            AssetPathTarget::FileContents(_) => {
                return Err(failure("invalid filesystem registration"));
            }
        };
        if child.as_os_str().is_empty()
            || child
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(failure("invalid registered asset relative path"));
        }
        if self.root.get().is_none() {
            let opened = Dir::open_ambient_dir(root_path, cap_std::ambient_authority())?;
            // Concurrent first readers may open the same trusted registration root;
            // only the first successfully installed capability remains authoritative.
            let _ = self.root.set(opened);
        }
        let root = self
            .root
            .get()
            .ok_or_else(|| failure("registered root unavailable"))?;
        // cap-std resolves relative to the opened root and forbids escaping symlinks,
        // including during concurrent rename/symlink replacement. No canonicalize/open gap.
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = root.open_with(child, &options)?;
        if !file.metadata()?.is_file() {
            return Err(failure("registered asset is not a regular file"));
        }
        read_bounded(file, limit)
    }
}

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, Error> {
    // Read from the actual opened handle, including a sentinel byte: a growing file
    // cannot bypass the cap by racing a metadata length check. Do not preallocate from
    // untrusted metadata or the caller's byte limit.
    let bound = u64::try_from(limit)?
        .checked_add(1)
        .ok_or_else(|| failure("asset byte limit overflow"))?;
    let mut bytes = Vec::new();
    reader.take(bound).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(failure("registered asset exceeds byte limit"));
    }
    Ok(bytes)
}
