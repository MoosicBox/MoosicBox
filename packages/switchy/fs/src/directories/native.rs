use std::path::PathBuf;

#[cfg(unix)]
pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(account_home)
}

#[cfg(all(
    unix,
    any(target_os = "android", target_os = "ios", target_os = "emscripten")
))]
fn account_home() -> Option<PathBuf> {
    None
}

#[cfg(all(
    unix,
    not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
))]
fn account_home() -> Option<PathBuf> {
    use std::{
        ffi::{CStr, OsString},
        os::unix::ffi::OsStringExt,
    };
    // Bound account database lookup memory; retry ERANGE without guessing a path.
    let mut buffer = vec![0_u8; 1024];
    loop {
        let mut record = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = std::ptr::null_mut();
        // SAFETY: record and buffer are writable for their advertised sizes. The
        // returned record points into buffer, which remains alive until copied.
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                record.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &raw mut result,
            )
        };
        if status == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0 || result.is_null() {
            return None;
        }
        // SAFETY: successful getpwuid_r with non-null result initializes record.
        let record = unsafe { record.assume_init() };
        if record.pw_dir.is_null() {
            return None;
        }
        // SAFETY: pw_dir is a NUL-terminated string provided by getpwuid_r.
        let bytes = unsafe { CStr::from_ptr(record.pw_dir) }.to_bytes();
        return (!bytes.is_empty()).then(|| PathBuf::from(OsString::from_vec(bytes.to_vec())));
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn xdg_path(
    value: Option<std::ffi::OsString>,
    home: impl FnOnce() -> Option<PathBuf>,
    suffix: &str,
) -> Option<PathBuf> {
    value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|path| path.join(suffix)))
}

#[cfg(unix)]
pub(super) fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        home_dir().map(|home| home.join("Library/Application Support"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        xdg_path(std::env::var_os("XDG_CONFIG_HOME"), home_dir, ".config")
    }
}

#[cfg(unix)]
pub(super) fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        config_dir()
    }
    #[cfg(not(target_os = "macos"))]
    {
        xdg_path(std::env::var_os("XDG_DATA_HOME"), home_dir, ".local/share")
    }
}

#[cfg(unix)]
pub(super) fn data_local_dir() -> Option<PathBuf> {
    data_dir()
}

#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::{System::Com::CoTaskMemFree, UI::Shell::SHGetKnownFolderPath};
    let mut path = std::ptr::null_mut();
    // SAFETY: id is a valid known-folder identifier, output is writable, and a
    // null token selects the current user. The returned allocation is freed below.
    let status = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &raw mut path) };
    let result = if status >= 0 && !path.is_null() {
        // SAFETY: on success Windows returns an allocated NUL-terminated UTF-16 string.
        let mut len = 0;
        unsafe {
            while *path.add(len) != 0 {
                len += 1;
            }
            Some(PathBuf::from(OsString::from_wide(
                std::slice::from_raw_parts(path, len),
            )))
        }
    } else {
        None
    };
    // SAFETY: this pointer was returned by SHGetKnownFolderPath; null is permitted.
    unsafe { CoTaskMemFree(path.cast()) };
    result
}

#[cfg(windows)]
pub(super) fn home_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile)
}
#[cfg(windows)]
pub(super) fn config_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_RoamingAppData)
}
#[cfg(windows)]
pub(super) fn data_dir() -> Option<PathBuf> {
    config_dir()
}
#[cfg(windows)]
pub(super) fn data_local_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn home_dir() -> Option<PathBuf> {
    None
}
#[cfg(not(any(unix, windows)))]
pub(super) fn config_dir() -> Option<PathBuf> {
    None
}
#[cfg(not(any(unix, windows)))]
pub(super) fn data_dir() -> Option<PathBuf> {
    None
}
#[cfg(not(any(unix, windows)))]
pub(super) fn data_local_dir() -> Option<PathBuf> {
    None
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    use super::xdg_path;
    use std::{ffi::OsString, path::PathBuf};
    #[test]
    fn xdg_overrides_require_absolute_paths() {
        for value in [None, Some(OsString::new()), Some("relative".into())] {
            assert_eq!(
                xdg_path(value, || Some("/home/user".into()), ".config"),
                Some(PathBuf::from("/home/user/.config"))
            );
        }
        assert_eq!(
            xdg_path(
                Some("/explicit".into()),
                || panic!("must not resolve home"),
                ".config"
            ),
            Some(PathBuf::from("/explicit"))
        );
        assert_eq!(xdg_path(None, || None, ".config"), None);
    }
}
