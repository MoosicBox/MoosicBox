//! `openpty` terminals.

use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::os::fd::{AsFd, AsRawFd as _, BorrowedFd, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::unix::process::CommandExt as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::{ChildExit, PtySize, Signal, SpawnCommand, Stdio};

/// How often, and how many times, a hung-up process is checked for having exited before it is
/// killed outright.
const HANGUP_GRACE: Duration = Duration::from_millis(50);
const HANGUP_CHECKS: u32 = 5;

pub fn open(size: PtySize) -> io::Result<(Master, Slave)> {
    let (master, slave) = open_fds(size)?;
    let tty_name = tty_name(slave.as_fd());
    Ok((
        Master {
            fd: master,
            tty_name,
            took_writer: AtomicBool::new(false),
        },
        Slave { fd: slave },
    ))
}

/// The slave device's path.
fn tty_name(fd: BorrowedFd<'_>) -> Option<PathBuf> {
    let mut buffer = vec![0 as libc::c_char; 128];
    loop {
        // SAFETY: `buffer` is valid for writes of its length.
        let result = unsafe { libc::ttyname_r(fd.as_raw_fd(), buffer.as_mut_ptr(), buffer.len()) };
        match result {
            0 => {
                // SAFETY: ttyname_r wrote a terminated string into `buffer`.
                let name = unsafe { CStr::from_ptr(buffer.as_ptr()) };
                return Some(PathBuf::from(OsStr::from_bytes(name.to_bytes())));
            }
            // Some systems also report a buffer they consider too large this way.
            libc::ERANGE if buffer.len() < 64 * 1024 => buffer.resize(buffer.len() * 2, 0),
            _ => return None,
        }
    }
}

#[derive(Debug)]
pub struct Master {
    fd: OwnedFd,
    tty_name: Option<PathBuf>,
    took_writer: AtomicBool,
}

impl Master {
    pub fn resize(&self, size: PtySize) -> io::Result<()> {
        set_size(self.fd.as_raw_fd(), size)
    }

    pub fn size(&self) -> io::Result<PtySize> {
        let mut size = libc::winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: an open terminal and a winsize valid for writes.
        if unsafe { libc::ioctl(self.fd.as_raw_fd(), libc::TIOCGWINSZ, &raw mut size) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(PtySize {
            cols: size.ws_col,
            rows: size.ws_row,
        })
    }

    pub fn try_clone_reader(&self) -> io::Result<Reader> {
        Ok(Reader(File::from(self.fd.try_clone()?)))
    }

    pub fn take_writer(&self) -> io::Result<Input> {
        if self.took_writer.swap(true, Ordering::AcqRel) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the terminal's writer was already taken",
            ));
        }
        match self.fd.try_clone() {
            Ok(fd) => Ok(Input {
                file: File::from(fd),
                end_on_drop: true,
            }),
            Err(error) => {
                self.took_writer.store(false, Ordering::Release);
                Err(error)
            }
        }
    }

    pub fn clone_writer(&self) -> io::Result<Input> {
        Ok(Input {
            file: File::from(self.fd.try_clone()?),
            end_on_drop: false,
        })
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        set_nonblocking(self.fd.as_raw_fd(), nonblocking)
    }

    pub fn has_peer(&self) -> bool {
        terminal_open(self.fd.as_fd())
    }

    pub fn tty_name(&self) -> Option<&Path> {
        self.tty_name.as_deref()
    }

    pub fn foreground_process_group(&self) -> Option<i32> {
        // SAFETY: tcgetpgrp on an open terminal descriptor.
        match unsafe { libc::tcgetpgrp(self.fd.as_raw_fd()) } {
            group if group > 0 => Some(group),
            _ => None,
        }
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

#[derive(Debug)]
pub struct Reader(File);

impl Reader {
    pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buffer) {
            // The terminal reports that nothing holds its far side open any more as an error.
            Err(error) if error.raw_os_error() == Some(libc::EIO) => Ok(0),
            other => other,
        }
    }

    pub fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
        wait_ready(self.0.as_fd(), libc::POLLIN, timeout)
    }
}

#[derive(Debug)]
pub struct Input {
    file: File,
    end_on_drop: bool,
}

impl Input {
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.file.write(bytes)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }

    pub fn wait_writable(&self, timeout: Duration) -> io::Result<bool> {
        wait_ready(self.file.as_fd(), libc::POLLOUT, timeout)
    }
}

impl Drop for Input {
    /// End the terminal's input: its end-of-file character only takes effect at the start of a
    /// line, so a newline goes first.
    fn drop(&mut self) {
        if !self.end_on_drop {
            return;
        }
        // SAFETY: zeroed is a valid termios to be overwritten.
        let mut settings: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: an open terminal descriptor and a termios valid for writes.
        if unsafe { libc::tcgetattr(self.file.as_raw_fd(), &raw mut settings) } == 0 {
            let end = settings.c_cc[libc::VEOF];
            if end != 0 {
                let _ = self.file.write_all(&[b'\n', end]);
            }
        }
    }
}

#[derive(Debug)]
pub struct PipeReader(File);

impl PipeReader {
    pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        set_nonblocking(self.0.as_raw_fd(), nonblocking)
    }

    pub fn wait_readable(&self, timeout: Duration) -> io::Result<bool> {
        wait_ready(self.0.as_fd(), libc::POLLIN, timeout)
    }
}

#[derive(Debug)]
pub struct PipeWriter(File);

impl PipeWriter {
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[derive(Debug)]
pub struct Slave {
    fd: OwnedFd,
}

impl Slave {
    pub fn spawn(&self, spec: &SpawnCommand) -> io::Result<Child> {
        let argv = spec.argv();
        let program = argv
            .first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no program to run"))?;
        let home = home_dir(spec);
        let dir = spec
            .get_cwd()
            .filter(|dir| Path::new(dir).is_dir())
            .map_or(home, OsStr::to_owned);
        let resolved = resolve(program, Path::new(&dir), spec.get_env("PATH"))?;
        let mut command = std::process::Command::new(resolved);
        let (stdin, stdout, stderr) = spec.stdio();
        command
            .arg0(program)
            .args(&argv[1..])
            .current_dir(&dir)
            .env_clear()
            .envs(spec.environment())
            .stdin(self.stdio(stdin)?)
            .stdout(self.stdio(stdout)?)
            .stderr(self.stdio(stderr)?);
        if !spec.is_exact_environment() && spec.get_env("SHELL").is_none() {
            command.env("SHELL", shell(spec));
        }
        let slave = self.fd.as_raw_fd();
        let controlling = spec.is_controlling_terminal();
        // SAFETY: `prepare_child` makes only async-signal-safe calls. The slave descriptor is
        // still open in the child until it executes the program.
        unsafe {
            command.pre_exec(move || prepare_child(slave, controlling));
        }
        Ok(Child::from(command.spawn()?))
    }

    fn stdio(&self, stdio: Stdio) -> io::Result<std::process::Stdio> {
        Ok(match stdio {
            Stdio::Terminal => std::process::Stdio::from(self.fd.try_clone()?),
            Stdio::Piped => std::process::Stdio::piped(),
            Stdio::Null => std::process::Stdio::null(),
        })
    }
}

/// Whether `path` names a file this process may execute.
fn executable(path: &Path) -> bool {
    access(path, libc::X_OK)
}

fn access(path: &Path, mode: libc::c_int) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: a terminated path string.
    unsafe { libc::access(path.as_ptr(), mode) == 0 }
}

/// Find `program`: as given when it names a directory (relative ones against `cwd`), otherwise
/// on `path`, whose relative entries are taken against `cwd` too.
fn resolve(program: &OsStr, cwd: &Path, path: Option<&OsStr>) -> io::Result<OsString> {
    let program_path = Path::new(program);
    let named_directly = program_path.is_absolute()
        || matches!(
            program_path.components().next(),
            Some(Component::CurDir | Component::ParentDir)
        );
    if named_directly {
        let candidate = cwd.join(program_path);
        return check(&candidate).map(|()| candidate.into_os_string());
    }
    if program_path.components().count() > 1 {
        // A relative path through a directory, like `bin/tool`: relative to `cwd`, as a shell
        // would run it.
        let candidate = cwd.join(program_path);
        return check(&candidate).map(|()| candidate.into_os_string());
    }
    let Some(path) = path else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "cannot run {}: no PATH to look it up on",
                program_path.display()
            ),
        ));
    };
    let mut rejected = Vec::new();
    for directory in std::env::split_paths(path) {
        let candidate = cwd.join(directory).join(program_path);
        if candidate.is_dir() {
            rejected.push(format!("{} is a directory", candidate.display()));
        } else if executable(&candidate) {
            return Ok(candidate.into_os_string());
        } else if access(&candidate, libc::F_OK) {
            rejected.push(format!("{} is not executable", candidate.display()));
        }
    }
    let mut message = format!(
        "cannot run {}: not found on PATH {}",
        program_path.display(),
        path.to_string_lossy()
    );
    for reason in rejected {
        message.push_str("; ");
        message.push_str(&reason);
    }
    Err(io::Error::new(io::ErrorKind::NotFound, message))
}

/// Whether `candidate` can be run, as an error saying why not.
fn check(candidate: &Path) -> io::Result<()> {
    if candidate.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("cannot run {}: it is a directory", candidate.display()),
        ));
    }
    if executable(candidate) {
        return Ok(());
    }
    if access(candidate, libc::F_OK) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("cannot run {}: it is not executable", candidate.display()),
        ));
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("cannot run {}: it does not exist", candidate.display()),
    ))
}

/// The user's entry in the user database: home directory and login shell.
fn user_entry() -> Option<(OsString, OsString)> {
    // SAFETY: zeroed is a valid passwd to be overwritten.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let mut buffer = vec![0 as libc::c_char; 1024];
    loop {
        // SAFETY: every pointer is valid for writes; `buffer` of its length.
        let result = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                &raw mut entry,
                buffer.as_mut_ptr(),
                buffer.len(),
                &raw mut found,
            )
        };
        if result == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if result != 0 || found.is_null() {
            return None;
        }
        let text = |field: *const libc::c_char| {
            if field.is_null() {
                OsString::new()
            } else {
                // SAFETY: getpwuid_r points the fields at terminated strings in `buffer`.
                OsString::from_vec(unsafe { CStr::from_ptr(field) }.to_bytes().to_vec())
            }
        };
        return Some((text(entry.pw_dir), text(entry.pw_shell)));
    }
}

/// The login shell from the user database when it can be run, otherwise `/bin/sh`.
fn login_shell() -> OsString {
    user_entry()
        .map(|(_, shell)| shell)
        .filter(|shell| !shell.is_empty() && executable(Path::new(shell)))
        .unwrap_or_else(|| "/bin/sh".into())
}

/// The command's `SHELL` when it can be run, otherwise the login shell.
fn shell(spec: &SpawnCommand) -> OsString {
    spec.get_env("SHELL")
        .filter(|shell| executable(Path::new(shell)))
        .map_or_else(login_shell, OsStr::to_owned)
}

/// The command's `HOME`, otherwise the user database's, otherwise `/`.
fn home_dir(spec: &SpawnCommand) -> OsString {
    spec.get_env("HOME").map_or_else(
        || {
            user_entry()
                .map(|(home, _)| home)
                .filter(|home| !home.is_empty())
                .unwrap_or_else(|| "/".into())
        },
        OsStr::to_owned,
    )
}

pub fn base_environment() -> Vec<(OsString, OsString)> {
    let mut environment: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    if !environment.iter().any(|(name, _)| name == "SHELL") {
        environment.push(("SHELL".into(), login_shell()));
    }
    environment
}

/// The system's description of `signal`.
pub fn signal_name(signal: libc::c_int) -> String {
    // strsignal may share one buffer between callers.
    static DESCRIBING: Mutex<()> = Mutex::new(());
    let _describing = DESCRIBING.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: strsignal accepts any number and returns a terminated string or null.
    let name = unsafe { libc::strsignal(signal) };
    if name.is_null() {
        format!("Signal {signal}")
    } else {
        // SAFETY: as above; copied out before the lock is released.
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    }
}

#[derive(Debug)]
pub struct Child {
    child: std::process::Child,
}

impl Child {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn take_stdin(&mut self) -> Option<PipeWriter> {
        self.child
            .stdin
            .take()
            .map(|pipe| PipeWriter(File::from(OwnedFd::from(pipe))))
    }

    pub fn take_stdout(&mut self) -> Option<PipeReader> {
        self.child
            .stdout
            .take()
            .map(|pipe| PipeReader(File::from(OwnedFd::from(pipe))))
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ChildExit>> {
        Ok(self.child.try_wait()?.map(ChildExit::from))
    }

    pub fn wait(&mut self) -> io::Result<ChildExit> {
        self.child.wait().map(ChildExit::from)
    }

    /// `waitid` with `WNOWAIT`: the exit, leaving the process a zombie.
    // Exclusive like `wait`, though waiting without reaping changes nothing here.
    #[allow(clippy::needless_pass_by_ref_mut)]
    pub fn wait_unreaped(&mut self) -> io::Result<ChildExit> {
        let pid = libc::id_t::from(self.child.id());
        // SAFETY: zeroed is a valid siginfo_t to be overwritten.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        loop {
            // SAFETY: `info` is valid for writes; waitid has no other preconditions.
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid,
                    &raw mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if result == 0 {
                break;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        #[cfg(target_os = "linux")]
        // SAFETY: waitid filled `info` for a child state change, where si_status is valid.
        let status = unsafe { info.si_status() };
        #[cfg(not(target_os = "linux"))]
        let status = info.si_status;
        Ok(match info.si_code {
            libc::CLD_EXITED => ChildExit::with_exit_code(u32::from_ne_bytes(status.to_ne_bytes())),
            _ => ChildExit::signaled(status, &signal_name(status)),
        })
    }

    /// Hang up on the process, as a closing terminal would, and kill it if it has not exited
    /// shortly after.
    pub fn kill(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        hang_up(self.child.id())?;
        for check in 0..HANGUP_CHECKS {
            if check > 0 {
                std::thread::sleep(HANGUP_GRACE);
            }
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
        }
        self.child.kill()
    }

    // Fallible on every platform: Windows duplicates a handle.
    #[allow(clippy::unnecessary_wraps)]
    pub fn killer(&self) -> io::Result<Killer> {
        Ok(Killer {
            pid: self.child.id(),
        })
    }
}

impl From<std::process::Child> for Child {
    fn from(child: std::process::Child) -> Self {
        Self { child }
    }
}

#[derive(Debug, Clone)]
pub struct Killer {
    pid: u32,
}

impl Killer {
    pub fn kill(&self) -> io::Result<()> {
        hang_up(self.pid)
    }
}

pub fn signal_group(group: u32, signal: Signal) -> io::Result<()> {
    let group =
        libc::pid_t::try_from(group).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let signal = match signal {
        Signal::Hangup => libc::SIGHUP,
        Signal::Interrupt => libc::SIGINT,
        Signal::Kill => libc::SIGKILL,
        Signal::Terminate => libc::SIGTERM,
        Signal::Quit => libc::SIGQUIT,
        Signal::WindowChange => libc::SIGWINCH,
    };
    // SAFETY: kill has no memory-safety preconditions.
    if unsafe { libc::kill(-group, signal) } == -1 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}

/// Send `SIGHUP` to `pid`; a process that is gone already counts as done.
fn hang_up(pid: u32) -> io::Result<()> {
    let pid =
        libc::pid_t::try_from(pid).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: kill has no memory-safety preconditions.
    if unsafe { libc::kill(pid, libc::SIGHUP) } == -1 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    Ok(())
}

/// Whether anything still holds the terminal's far side open.
fn terminal_open(master: BorrowedFd<'_>) -> bool {
    let mut poll = libc::pollfd {
        fd: master.as_raw_fd(),
        events: 0,
        revents: 0,
    };
    // SAFETY: one valid pollfd, no waiting.
    let ready = unsafe { libc::poll(&raw mut poll, 1, 0) };
    ready == 0 || (poll.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL)) == 0
}

/// Wait until `fd` is ready for `events`, at most `timeout`; whether it is.
fn wait_ready(fd: BorrowedFd<'_>, events: libc::c_short, timeout: Duration) -> io::Result<bool> {
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events,
        revents: 0,
    };
    let timeout = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
    // SAFETY: one valid pollfd.
    match unsafe { libc::poll(&raw mut poll, 1, timeout) } {
        -1 => {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                Ok(false)
            } else {
                Err(error)
            }
        }
        0 => Ok(false),
        _ => Ok(true),
    }
}

fn set_nonblocking(fd: RawFd, nonblocking: bool) -> io::Result<()> {
    // SAFETY: fcntl on an owned, open descriptor.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    let flags = if nonblocking {
        flags | libc::O_NONBLOCK
    } else {
        flags & !libc::O_NONBLOCK
    };
    // SAFETY: as above.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn set_size(fd: RawFd, size: PtySize) -> io::Result<()> {
    let size = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: `fd` is an open terminal and `size` outlives the call.
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &raw const size) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Make the terminal at `fd` the calling process's controlling terminal. Async-signal-safe.
// The ioctl request's integer type differs between platforms (`u32` constant against a
// `c_ulong` parameter on macOS, matching types on Linux), so no single conversion is lossless
// everywhere without a lint; the value always fits.
#[allow(clippy::cast_lossless)]
fn take_controlling_terminal(fd: RawFd) -> io::Result<()> {
    // SAFETY: ioctl on an open terminal descriptor with an integer argument.
    if unsafe { libc::ioctl(fd, libc::TIOCSCTTY as _, 0) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Most descriptors scanned for [`close_inherited_on_exec`] where they cannot be closed in
/// one call.
const DESCRIPTOR_SCAN_LIMIT: libc::c_int = 65_536;

/// Mark every descriptor above standard error close-on-exec, so descriptors the caller left
/// inheritable do not leak into the program. Marked rather than closed: a failed exec still
/// reports its error through the spawning library's own descriptor. Async-signal-safe.
fn close_inherited_on_exec() {
    #[cfg(target_os = "linux")]
    {
        const CLOSE_RANGE_CLOEXEC: libc::c_uint = 1 << 2;
        // SAFETY: close_range with the close-on-exec flag only changes descriptor flags.
        if unsafe {
            libc::syscall(
                libc::SYS_close_range,
                3,
                libc::c_uint::MAX,
                CLOSE_RANGE_CLOEXEC,
            )
        } == 0
        {
            return;
        }
    }
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit writes one rlimit.
    let highest = if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut limit) } == 0 {
        libc::c_int::try_from(limit.rlim_cur)
            .unwrap_or(DESCRIPTOR_SCAN_LIMIT)
            .min(DESCRIPTOR_SCAN_LIMIT)
    } else {
        DESCRIPTOR_SCAN_LIMIT
    };
    for fd in 3..highest {
        // SAFETY: fcntl on a descriptor number; unused numbers fail harmlessly.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags != -1 && flags & libc::FD_CLOEXEC == 0 {
                libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }
    }
}

fn close_on_exec(fd: RawFd) -> io::Result<()> {
    // SAFETY: fcntl on an owned, open descriptor.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// macOS's internal "open the device again" status (`EREDRIVEOPEN` in XNU), which `openpty`
/// lets through when terminals are opened concurrently. The open is simply retried.
const REDRIVE_OPEN: i32 = -6;
/// Attempts at opening a terminal pair while the kernel asks for the open again.
const OPEN_ATTEMPTS: usize = 32;

fn open_fds(size: PtySize) -> io::Result<(OwnedFd, OwnedFd)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    let mut size = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    for attempt in 1.. {
        // SAFETY: the out-pointers are valid for writes; null name and termios are allowed.
        let result = unsafe {
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut size,
            )
        };
        if result != -1 {
            break;
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(REDRIVE_OPEN) || attempt == OPEN_ATTEMPTS {
            return Err(error);
        }
    }
    // SAFETY: openpty returned two new descriptors that nothing else owns.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    close_on_exec(master.as_raw_fd())?;
    close_on_exec(slave.as_raw_fd())?;
    Ok((master, slave))
}

/// Set up the child between fork and exec. Async-signal-safe (signal, setsid, ioctl,
/// getrlimit, fcntl, syscall only).
fn prepare_child(slave_fd: RawFd, controlling: bool) -> io::Result<()> {
    // Start from default handling: dispositions the caller ignores (a terminal UI often
    // ignores interrupts) would otherwise be inherited. The signal mask and SIGPIPE are
    // already reset.
    for signal in [
        libc::SIGCHLD,
        libc::SIGHUP,
        libc::SIGINT,
        libc::SIGQUIT,
        libc::SIGTERM,
        libc::SIGALRM,
        libc::SIGTSTP,
        libc::SIGTTIN,
        libc::SIGTTOU,
    ] {
        // SAFETY: restoring a valid signal's default disposition.
        unsafe { libc::signal(signal, libc::SIG_DFL) };
    }
    // A new session: the process leads its own group, so signals reach everything it
    // starts, and it is detached from the caller's terminal.
    // SAFETY: setsid has no memory-safety preconditions.
    if unsafe { libc::setsid() } == -1 {
        return Err(io::Error::last_os_error());
    }
    if controlling {
        take_controlling_terminal(slave_fd)?;
    }
    close_inherited_on_exec();
    Ok(())
}
