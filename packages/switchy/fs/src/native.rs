//! Native helpers shared by the `std` and `tokio` backends on Unix.

use std::{os::fd::AsFd, path::Path};

use rustix::{fs::AtFlags, io::Errno};

fn link_then_unlink<F: AsFd, T: AsFd>(
    from_dir: F,
    from: &Path,
    to_dir: T,
    to: &Path,
) -> std::io::Result<()> {
    rustix::fs::linkat(&from_dir, from, &to_dir, to, AtFlags::empty())?;
    rustix::fs::unlinkat(&from_dir, from, AtFlags::empty())?;
    Ok(())
}

/// Rename without replacing an existing destination.
pub fn rename_noreplace<F: AsFd, T: AsFd>(
    from_dir: F,
    from: &Path,
    to_dir: T,
    to: &Path,
) -> std::io::Result<()> {
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "visionos"
    ))]
    {
        match rustix::fs::renameat_with(
            &from_dir,
            from,
            &to_dir,
            to,
            rustix::fs::RenameFlags::NOREPLACE,
        ) {
            Err(errno)
                if errno == Errno::INVAL || errno == Errno::NOSYS || errno == Errno::NOTSUP =>
            {
                link_then_unlink(from_dir, from, to_dir, to)
            }
            result => result.map_err(Into::into),
        }
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "visionos"
    )))]
    {
        link_then_unlink(from_dir, from, to_dir, to)
    }
}
