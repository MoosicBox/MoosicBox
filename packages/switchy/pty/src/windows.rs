//! Pseudo-consoles (`ConPTY`) as terminal pairs.
//!
//! The console asks the terminal for the cursor position (`CSI 6 n`) when it starts, so it can
//! take over a cursor already placed, and it blocks the first process until that is answered:
//! the reader of the master must answer (`CSI row ; col R`). It also asks for win32 input mode,
//! which a terminal may ignore.
//!
//! A `conpty.dll` shipped beside the executable (with its `OpenConsole.exe`) is used in place
//! of the system's console host, as newer releases fix many console bugs. Only the executable's
//! own directory is searched for it.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::windows::io::{
    AsRawHandle as _, BorrowedHandle, FromRawHandle as _, OwnedHandle, RawHandle,
};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use windows_sys::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, HANDLE, INVALID_HANDLE_VALUE, S_OK,
    WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Globalization::CompareStringOrdinal;
use windows_sys::Win32::System::Console::{
    COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
};
use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LoadLibraryExW,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ, RegCloseKey,
    RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW,
};
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    PROC_THREAD_ATTRIBUTE_JOB_LIST, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject,
};
use windows_sys::core::HRESULT;

use windows_sys::Win32::System::Pipes::CreatePipe;

use crate::{ChildExit, PtySize, Signal, SpawnCommand, Stdio};

/// Keep the cursor the terminal already has: the console asks for it on start.
const PSEUDOCONSOLE_INHERIT_CURSOR: u32 = 0x1;
/// Let the console reflow its buffer on resize without repainting it into the output.
const PSEUDOCONSOLE_RESIZE_QUIRK: u32 = 0x2;
/// Ask the terminal for win32 input mode, which carries every key event.
const PSEUDOCONSOLE_WIN32_INPUT_MODE: u32 = 0x4;

/// The exit code of a process ended by [`Child::kill`].
const KILLED_EXIT_CODE: u32 = 1;

type Create = unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type Resize = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type Close = unsafe extern "system" fn(HPCON);

/// The pseudo-console functions in use: a sideloaded `conpty.dll`'s, or the system's.
struct Functions {
    create: Create,
    resize: Resize,
    close: Close,
}

fn functions() -> &'static Functions {
    static FUNCTIONS: OnceLock<Functions> = OnceLock::new();
    FUNCTIONS.get_or_init(|| {
        sideloaded().unwrap_or(Functions {
            create: CreatePseudoConsole,
            resize: ResizePseudoConsole,
            close: ClosePseudoConsole,
        })
    })
}

/// The functions of a `conpty.dll` in the executable's directory, which stays loaded.
fn sideloaded() -> Option<Functions> {
    let name = wide(OsStr::new("conpty.dll"));
    // SAFETY: a terminated name; only the application directory is searched.
    let library = unsafe {
        LoadLibraryExW(
            name.as_ptr(),
            null_mut(),
            LOAD_LIBRARY_SEARCH_APPLICATION_DIR,
        )
    };
    if library.is_null() {
        return None;
    }
    // SAFETY: a loaded library and terminated names.
    let (create, resize, close) = unsafe {
        (
            GetProcAddress(library, c"CreatePseudoConsole".as_ptr().cast())?,
            GetProcAddress(library, c"ResizePseudoConsole".as_ptr().cast())?,
            GetProcAddress(library, c"ClosePseudoConsole".as_ptr().cast())?,
        )
    };
    // SAFETY: these exports have the documented pseudo-console signatures.
    unsafe {
        Some(Functions {
            create: std::mem::transmute::<unsafe extern "system" fn() -> isize, Create>(create),
            resize: std::mem::transmute::<unsafe extern "system" fn() -> isize, Resize>(resize),
            close: std::mem::transmute::<unsafe extern "system" fn() -> isize, Close>(close),
        })
    }
}

/// `result` as an error: a Win32 error code where it wraps one.
fn hresult_error(result: HRESULT, doing: &str) -> io::Error {
    let bits = u32::from_ne_bytes(result.to_ne_bytes());
    // FACILITY_WIN32 failures carry a Win32 error code in their low word.
    if bits & 0xFFFF_0000 == 0x8007_0000 {
        let error =
            io::Error::from_raw_os_error(i32::from(u16::try_from(bits & 0xFFFF).unwrap_or(0)));
        return io::Error::new(error.kind(), format!("{doing}: {error}"));
    }
    io::Error::other(format!("{doing}: HRESULT {bits:#010x}"))
}

fn coord(size: PtySize) -> COORD {
    COORD {
        X: i16::try_from(size.cols).unwrap_or(i16::MAX),
        Y: i16::try_from(size.rows).unwrap_or(i16::MAX),
    }
}

/// An open pseudo-console, closed when dropped.
struct Console(HPCON);

impl Drop for Console {
    fn drop(&mut self) {
        // SAFETY: an open console, closed exactly once.
        unsafe { (functions().close)(self.0) };
    }
}

/// What the master and slave share: the console, closed with the last of them.
/// The console is closed first, then the pipe ends; a writer never taken keeps the console's
/// input open until both sides are gone.
struct Shared {
    console: Console,
    size: Mutex<PtySize>,
    output: File,
    input: Mutex<Option<File>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Console")
            .field(
                "size",
                &*self.size.lock().unwrap_or_else(PoisonError::into_inner),
            )
            .finish_non_exhaustive()
    }
}

pub fn open(size: PtySize) -> io::Result<(Master, Slave)> {
    let (console_input, input) = pipe()?;
    let (output, console_output) = pipe()?;
    let mut console: HPCON = 0;
    // SAFETY: valid handles and out-pointer; the console duplicates the pipe ends it is given.
    let result = unsafe {
        (functions().create)(
            coord(size),
            console_input.as_raw_handle(),
            console_output.as_raw_handle(),
            PSEUDOCONSOLE_INHERIT_CURSOR
                | PSEUDOCONSOLE_RESIZE_QUIRK
                | PSEUDOCONSOLE_WIN32_INPUT_MODE,
            &raw mut console,
        )
    };
    if result != S_OK {
        return Err(hresult_error(result, "cannot create a pseudo-console"));
    }
    // Only the console holds these ends now, so its closing ends the output.
    drop((console_input, console_output));
    let shared = Arc::new(Shared {
        console: Console(console),
        size: Mutex::new(size),
        output: File::from(output),
        input: Mutex::new(Some(File::from(input))),
    });
    Ok((
        Master {
            shared: Arc::clone(&shared),
        },
        Slave { shared },
    ))
}

#[derive(Debug)]
pub struct Master {
    shared: Arc<Shared>,
}

impl Master {
    // The size lock is held across the resize on purpose: it orders concurrent resizes.
    #[allow(clippy::significant_drop_tightening)]
    pub fn resize(&self, size: PtySize) -> io::Result<()> {
        let mut current = self
            .shared
            .size
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: an open console; the size lock orders resizes.
        let result = unsafe { (functions().resize)(self.shared.console.0, coord(size)) };
        if result != S_OK {
            return Err(hresult_error(result, "cannot resize the pseudo-console"));
        }
        *current = size;
        Ok(())
    }

    // Fallible on every platform: Unix asks the terminal.
    #[allow(clippy::unnecessary_wraps)]
    pub fn size(&self) -> io::Result<PtySize> {
        Ok(*self
            .shared
            .size
            .lock()
            .unwrap_or_else(PoisonError::into_inner))
    }

    pub fn try_clone_reader(&self) -> io::Result<Reader> {
        Ok(Reader(self.shared.output.try_clone()?))
    }

    pub fn clone_writer(&self) -> io::Result<Input> {
        self.shared
            .input
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "the terminal's writer was already taken",
                )
            })?
            .try_clone()
            .map(Input)
    }

    // Uniform with Unix; a console's pipes cannot be made non-blocking.
    #[allow(clippy::unused_self)]
    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        Err(unsupported("non-blocking pseudo-console handles"))
    }

    // The console holds its own end open until the master and slave are both dropped.
    #[allow(clippy::unused_self)]
    pub const fn has_peer(&self) -> bool {
        true
    }

    pub fn take_writer(&self) -> io::Result<Input> {
        self.shared
            .input
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .map(Input)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "the terminal's writer was already taken",
                )
            })
    }
}

#[derive(Debug)]
pub struct Reader(File);

impl Reader {
    pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buffer) {
            // The console closed its end.
            Err(error)
                if error.raw_os_error()
                    == Some(i32::try_from(ERROR_BROKEN_PIPE).unwrap_or(i32::MAX)) =>
            {
                Ok(0)
            }
            other => other,
        }
    }
}

impl Reader {
    // Uniform with Unix; anonymous pipes cannot be polled.
    #[allow(clippy::unused_self)]
    pub fn wait_readable(&self, _timeout: std::time::Duration) -> io::Result<bool> {
        Err(unsupported("waiting on pseudo-console output"))
    }
}

#[derive(Debug)]
pub struct Input(File);

impl Input {
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }

    // Uniform with Unix; anonymous pipes cannot be polled.
    #[allow(clippy::unused_self)]
    pub fn wait_writable(&self, _timeout: std::time::Duration) -> io::Result<bool> {
        Err(unsupported("waiting on pseudo-console input"))
    }
}

/// Never made: a pseudo-console process's streams are all the console.
#[derive(Debug)]
pub struct PipeReader(std::convert::Infallible);

// Signatures are uniform with Unix, where these handles exist.
#[allow(clippy::needless_pass_by_ref_mut)]
impl PipeReader {
    pub const fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        match self.0 {}
    }

    pub const fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        match self.0 {}
    }

    pub const fn wait_readable(&self, _timeout: std::time::Duration) -> io::Result<bool> {
        match self.0 {}
    }
}

/// Never made: a pseudo-console process's streams are all the console.
#[derive(Debug)]
pub struct PipeWriter(std::convert::Infallible);

// Signatures are uniform with Unix, where these handles exist.
#[allow(clippy::needless_pass_by_ref_mut)]
impl PipeWriter {
    pub const fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        match self.0 {}
    }

    pub const fn flush(&mut self) -> io::Result<()> {
        match self.0 {}
    }
}

fn unsupported(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{what} is not supported on Windows"),
    )
}

pub const fn behavior() -> crate::Behavior {
    crate::Behavior::of(true)
}

pub fn signal_group(_group: u32, _signal: Signal) -> io::Result<()> {
    Err(unsupported("signalling a process group"))
}

#[derive(Debug)]
pub struct Slave {
    shared: Arc<Shared>,
}

impl Slave {
    pub fn spawn(&self, spec: &SpawnCommand) -> io::Result<Child> {
        self.start_in(spec, None)
    }

    pub fn spawn_in_job(&self, spec: &SpawnCommand, job: BorrowedHandle<'_>) -> io::Result<Child> {
        self.start_in(spec, Some(job))
    }

    fn start_in(&self, spec: &SpawnCommand, job: Option<BorrowedHandle<'_>>) -> io::Result<Child> {
        if spec.stdio() != (Stdio::Terminal, Stdio::Terminal, Stdio::Terminal) {
            return Err(unsupported(
                "redirecting a pseudo-console process's streams",
            ));
        }
        let argv = spec.argv();
        let program = argv
            .first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no program to run"))?;
        for arg in &argv[1..] {
            if arg.encode_wide().any(|unit| unit == 0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("argument {} contains a nul character", arg.display()),
                ));
            }
        }
        let resolved = search_path(program, spec.get_env("PATH"), spec.get_env("PATHEXT"));
        let mut line = Vec::new();
        quote(resolved.as_deref().unwrap_or(program), &mut line);
        if job.is_some() && is_cmd(program) {
            cmd_arguments(&argv[1..], &mut line)?;
        } else {
            for arg in &argv[1..] {
                line.push(u16::from(b' '));
                quote(arg, &mut line);
            }
        }
        line.push(0);
        // Without a resolved path, CreateProcessW looks the program up itself.
        let application = resolved.as_deref().map(wide);
        let environment = if spec.is_exact_environment() {
            exact_environment_block(spec)?
        } else {
            environment_block(spec)
        };
        let cwd = working_directory(spec).map(|dir| wide(dir.as_os_str()));
        start(
            &mut line,
            application.as_deref(),
            &environment,
            cwd.as_deref(),
            self.shared.console.0,
            job,
        )
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "cannot start {} in {}: {error}",
                    String::from_utf16_lossy(&line[..line.len() - 1]),
                    cwd.as_deref().map_or_else(
                        || "the current directory".to_owned(),
                        |cwd| String::from_utf16_lossy(&cwd[..cwd.len() - 1]),
                    ),
                ),
            )
        })
    }
}

/// Whether `program` names the Windows command interpreter.
fn is_cmd(program: &OsStr) -> bool {
    Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("cmd") || name.eq_ignore_ascii_case("cmd.exe")
        })
}

/// `cmd`'s arguments: options as given, then the script after `/C` or `/K` verbatim, wrapped in
/// `/S` quotes. `cmd` parses that remainder as shell source, not as C runtime arguments, so
/// escaping it would change it. Without `/C` or `/K`, every argument is quoted as usual.
fn cmd_arguments(args: &[OsString], line: &mut Vec<u16>) -> io::Result<()> {
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidInput, message.to_owned());
    let script = args.iter().position(|arg| {
        arg.to_str()
            .is_some_and(|arg| arg.eq_ignore_ascii_case("/c") || arg.eq_ignore_ascii_case("/k"))
    });
    let Some(index) = script else {
        for arg in args {
            line.push(u16::from(b' '));
            quote(arg, line);
        }
        return Ok(());
    };
    if index + 2 != args.len() {
        return Err(invalid(
            "cmd /C or /K takes exactly one script argument, last",
        ));
    }
    for option in &args[..index] {
        let units: Vec<u16> = option.encode_wide().collect();
        if units.first() != Some(&u16::from(b'/'))
            || units
                .iter()
                .any(|unit| matches!(*unit, 0x20 | 0x09 | 0x0a | 0x0b | 0x22))
        {
            return Err(invalid("only cmd options may precede /C or /K"));
        }
        line.push(u16::from(b' '));
        line.extend(units);
    }
    line.extend(" /S ".encode_utf16());
    line.extend(args[index].encode_wide());
    line.extend(" \"".encode_utf16());
    line.extend(args[index + 1].encode_wide());
    line.push(u16::from(b'"'));
    Ok(())
}

/// Whether `name` can be an environment block entry's name: not empty, without `=` after an
/// optional leading one (`cmd` keeps names such as `=C:` and `=ExitCode`), and without nul.
fn valid_name(name: &[u16]) -> bool {
    let name = name.strip_prefix(&[u16::from(b'=')]).unwrap_or(name);
    !name.is_empty() && !name.contains(&0) && !name.contains(&u16::from(b'='))
}

/// How Windows orders and compares environment names: ordinally, ignoring case.
fn compare_names(left: &[u16], right: &[u16]) -> io::Result<std::cmp::Ordering> {
    let length = |units: &[u16]| {
        i32::try_from(units.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "environment name too long"))
    };
    // SAFETY: both slices are valid for the lengths passed.
    let result = unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            length(left)?,
            right.as_ptr(),
            length(right)?,
            1,
        )
    };
    match result {
        1 => Ok(std::cmp::Ordering::Less),
        2 => Ok(std::cmp::Ordering::Equal),
        3 => Ok(std::cmp::Ordering::Greater),
        _ => Err(io::Error::last_os_error()),
    }
}

/// Exactly the command's variables, checked and ordered as Windows requires.
fn exact_environment_block(spec: &SpawnCommand) -> io::Result<Vec<u16>> {
    let invalid = |message: String| io::Error::new(io::ErrorKind::InvalidInput, message);
    let mut variables = Vec::new();
    for (name, value) in spec.environment() {
        let name: Vec<u16> = name.encode_wide().collect();
        let value: Vec<u16> = value.encode_wide().collect();
        if !valid_name(&name) {
            return Err(invalid(format!(
                "invalid environment variable name {:?}",
                String::from_utf16_lossy(&name)
            )));
        }
        if value.contains(&0) {
            return Err(invalid(format!(
                "environment variable {} contains a nul character",
                String::from_utf16_lossy(&name)
            )));
        }
        variables.push((name, value));
    }
    sorted_block(variables)
}

/// `variables` ordered by name as Windows orders them, as a terminated block. Names Windows
/// considers equal are an error.
fn sorted_block(mut variables: Vec<(Vec<u16>, Vec<u16>)>) -> io::Result<Vec<u16>> {
    let mut failure = None;
    variables.sort_by(|(left, _), (right, _)| {
        compare_names(left, right).unwrap_or_else(|error| {
            failure.get_or_insert(error);
            std::cmp::Ordering::Equal
        })
    });
    if let Some(error) = failure {
        return Err(error);
    }
    for pair in variables.windows(2) {
        if compare_names(&pair[0].0, &pair[1].0)? == std::cmp::Ordering::Equal {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "environment variables {} and {} differ only in case",
                    String::from_utf16_lossy(&pair[0].0),
                    String::from_utf16_lossy(&pair[1].0)
                ),
            ));
        }
    }
    let mut block = Vec::new();
    for (name, value) in &variables {
        block.extend(name);
        block.push(u16::from(b'='));
        block.extend(value);
        block.push(0);
    }
    // An empty block still needs both terminators.
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

/// `program` found on `path`, as given or with each `PATHEXT` extension, when it is a file.
fn search_path(
    program: &OsStr,
    path: Option<&OsStr>,
    extensions: Option<&OsStr>,
) -> Option<OsString> {
    let path = path?;
    let extensions = extensions.unwrap_or_else(|| OsStr::new(".EXE"));
    for directory in std::env::split_paths(path) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Some(candidate.into_os_string());
        }
        for extension in std::env::split_paths(extensions) {
            let extension = extension.as_os_str().to_string_lossy();
            let extension = extension.strip_prefix('.').unwrap_or(&extension);
            let candidate = candidate.with_extension(extension);
            if candidate.is_file() {
                return Some(candidate.into_os_string());
            }
        }
    }
    None
}

/// The command's directory when it is one, otherwise `USERPROFILE` when that is one, made
/// absolute; `None` keeps this process's.
fn working_directory(spec: &SpawnCommand) -> Option<PathBuf> {
    let directory = spec
        .get_cwd()
        .filter(|dir| Path::new(dir).is_dir())
        .or_else(|| {
            spec.get_env("USERPROFILE")
                .filter(|dir| Path::new(dir).is_dir())
        })?;
    let directory = Path::new(directory);
    if directory.is_relative()
        && let Ok(current) = std::env::current_dir()
    {
        return Some(current.join(directory));
    }
    Some(directory.to_owned())
}

fn environment_block(spec: &SpawnCommand) -> Vec<u16> {
    let mut block = Vec::new();
    for (name, value) in spec.environment() {
        block.extend(name.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    // An empty block still needs both terminators.
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

/// Start a process attached to `console`.
fn start(
    line: &mut [u16],
    application: Option<&[u16]>,
    environment: &[u16],
    cwd: Option<&[u16]>,
    console: HPCON,
    job: Option<BorrowedHandle<'_>>,
) -> io::Result<Child> {
    let count = if job.is_some() { 2 } else { 1 };
    let mut size = 0;
    // SAFETY: a size query with no list.
    unsafe { InitializeProcThreadAttributeList(null_mut(), count, 0, &raw mut size) };
    let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
    let attributes = storage.as_mut_ptr().cast();
    // SAFETY: storage of the queried size.
    if unsafe { InitializeProcThreadAttributeList(attributes, count, 0, &raw mut size) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let jobs = job.map(|job| [job.as_raw_handle()]);
    let started = (|| {
        // SAFETY: the console outlives CreateProcessW below.
        if unsafe {
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                console as *const core::ffi::c_void,
                size_of::<HPCON>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // Assigned to the job as it is created, before it can run or start anything.
        // SAFETY: the job list outlives CreateProcessW below.
        if let Some(jobs) = &jobs
            && unsafe {
                UpdateProcThreadAttribute(
                    attributes,
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    jobs.as_ptr().cast(),
                    size_of_val(jobs),
                    null_mut(),
                    null(),
                )
            } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: zeroed is a valid empty startup structure.
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = u32::try_from(size_of_val(&startup)).unwrap_or(u32::MAX);
        // Without explicit (invalid) standard handles, a process may inherit this one's
        // redirected handles instead of using the console.
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
        startup.lpAttributeList = attributes;
        // SAFETY: zeroed is a valid empty process structure.
        let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: every pointer is valid for the call; strings and the block are terminated.
        if unsafe {
            CreateProcessW(
                application.map_or(null(), <[u16]>::as_ptr),
                line.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                environment.as_ptr().cast(),
                cwd.map_or(null(), <[u16]>::as_ptr),
                &raw const startup.StartupInfo,
                &raw mut process,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CreateProcessW returned new handles that nothing else owns.
        let (handle, thread) = unsafe {
            (
                OwnedHandle::from_raw_handle(process.hProcess),
                OwnedHandle::from_raw_handle(process.hThread),
            )
        };
        drop(thread);
        Ok(Child {
            process: Arc::new(handle),
            pid: process.dwProcessId,
        })
    })();
    // SAFETY: initialized above and no longer used.
    unsafe { DeleteProcThreadAttributeList(attributes) };
    started
}

#[derive(Debug)]
pub struct Child {
    process: Arc<OwnedHandle>,
    pid: u32,
}

// Signatures are uniform with Unix, which needs exclusive access to wait for and kill a child.
#[allow(clippy::needless_pass_by_ref_mut, clippy::unused_self)]
impl Child {
    pub const fn id(&self) -> u32 {
        self.pid
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ChildExit>> {
        if !exited(&self.process, 0)? {
            return Ok(None);
        }
        exit_code(&self.process).map(|code| Some(ChildExit::with_exit_code(code)))
    }

    pub fn wait(&mut self) -> io::Result<ChildExit> {
        exited(&self.process, INFINITE)?;
        exit_code(&self.process).map(ChildExit::with_exit_code)
    }

    /// The handle this holds keeps the id from reuse until it closes, so waiting reaps
    /// nothing.
    pub fn wait_unreaped(&mut self) -> io::Result<ChildExit> {
        self.wait()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        terminate(&self.process)
    }

    pub fn killer(&self) -> io::Result<Killer> {
        Ok(Killer {
            process: Arc::new(self.process.try_clone()?),
        })
    }

    pub const fn take_stdin(&mut self) -> Option<PipeWriter> {
        None
    }

    pub const fn take_stdout(&mut self) -> Option<PipeReader> {
        None
    }

    pub fn as_raw_handle(&self) -> RawHandle {
        self.process.as_raw_handle()
    }
}

#[derive(Debug, Clone)]
pub struct Killer {
    process: Arc<OwnedHandle>,
}

impl Killer {
    pub fn kill(&self) -> io::Result<()> {
        terminate(&self.process)
    }
}

/// Whether `process` exits within `timeout` milliseconds.
fn exited(process: &OwnedHandle, timeout: u32) -> io::Result<bool> {
    // SAFETY: an owned process handle.
    match unsafe { WaitForSingleObject(process.as_raw_handle(), timeout) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(io::Error::last_os_error()),
        other => Err(io::Error::other(format!(
            "unexpected wait result {other:#x}"
        ))),
    }
}

fn exit_code(process: &OwnedHandle) -> io::Result<u32> {
    let mut code = 0u32;
    // SAFETY: an owned process handle and a valid out-pointer.
    if unsafe { GetExitCodeProcess(process.as_raw_handle(), &raw mut code) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(code)
}

/// Terminate `process`; one that has exited already counts as done.
fn terminate(process: &OwnedHandle) -> io::Result<()> {
    if exited(process, 0)? {
        return Ok(());
    }
    // SAFETY: an owned process handle.
    if unsafe { TerminateProcess(process.as_raw_handle(), KILLED_EXIT_CODE) } == 0 {
        let error = io::Error::last_os_error();
        // A process exiting meanwhile refuses termination.
        if exited(process, 0)? {
            return Ok(());
        }
        return Err(error);
    }
    Ok(())
}

/// Expand `%NAME%` references in a registry value against this process's environment.
fn expand(value: &[u16]) -> Option<OsString> {
    let mut source = value.to_vec();
    source.push(0);
    // SAFETY: a size query for a terminated source.
    let needed = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), null_mut(), 0) };
    if needed == 0 {
        return None;
    }
    let mut expanded = vec![0u16; usize::try_from(needed).ok()?];
    // SAFETY: `expanded` is valid for `needed` units.
    let written =
        unsafe { ExpandEnvironmentStringsW(source.as_ptr(), expanded.as_mut_ptr(), needed) };
    if written == 0 || written > needed {
        return None;
    }
    let end = expanded
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(expanded.len());
    Some(OsString::from_wide(&expanded[..end]))
}

/// The string values of registry key `subkey` under `root`, expanded.
fn registry_strings(root: HKEY, subkey: &str) -> Vec<(OsString, OsString)> {
    let subkey = wide(OsStr::new(subkey));
    let mut key: HKEY = null_mut();
    // SAFETY: a predefined root, a terminated name and a valid out-pointer.
    if unsafe { RegOpenKeyExW(root, subkey.as_ptr(), 0, KEY_READ, &raw mut key) } != 0 {
        return Vec::new();
    }
    let mut longest_name = 0u32;
    let mut longest_value = 0u32;
    // SAFETY: an open key; only the lengths are asked for.
    let queried = unsafe {
        RegQueryInfoKeyW(
            key,
            null_mut(),
            null_mut(),
            null(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &raw mut longest_name,
            &raw mut longest_value,
            null_mut(),
            null_mut(),
        )
    };
    let mut values = Vec::new();
    if queried == 0 {
        let mut name = vec![0u16; usize::try_from(longest_name).unwrap_or(0) + 1];
        let mut data = vec![0u8; usize::try_from(longest_value).unwrap_or(0) + 2];
        let mut index = 0;
        loop {
            let mut name_length = u32::try_from(name.len()).unwrap_or(u32::MAX);
            let mut data_length = u32::try_from(data.len()).unwrap_or(u32::MAX);
            let mut kind = 0u32;
            // SAFETY: buffers valid for the lengths passed.
            let result = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    name.as_mut_ptr(),
                    &raw mut name_length,
                    null(),
                    &raw mut kind,
                    data.as_mut_ptr(),
                    &raw mut data_length,
                )
            };
            if result == ERROR_MORE_DATA {
                // A value grew since the key was queried.
                name.resize(name.len() * 2, 0);
                data.resize(data.len() * 2, 0);
                continue;
            }
            if result == ERROR_NO_MORE_ITEMS || result != 0 {
                break;
            }
            index += 1;
            if kind != REG_SZ && kind != REG_EXPAND_SZ {
                continue;
            }
            let units: Vec<u16> = data[..usize::try_from(data_length).unwrap_or(0)]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect();
            let end = units
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(units.len());
            let value = if kind == REG_EXPAND_SZ {
                match expand(&units[..end]) {
                    Some(value) => value,
                    None => continue,
                }
            } else {
                OsString::from_wide(&units[..end])
            };
            let name = OsString::from_wide(&name[..usize::try_from(name_length).unwrap_or(0)]);
            values.push((name, value));
        }
    }
    // SAFETY: opened above.
    unsafe { RegCloseKey(key) };
    values
}

/// This process's environment with the system's, then the user's, registered environment
/// applied over it; the user's `PATH` is appended to the system's.
pub fn base_environment() -> Vec<(OsString, OsString)> {
    let key = |name: &OsStr| name.to_string_lossy().to_lowercase();
    let mut environment: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let set = |environment: &mut Vec<(OsString, OsString)>, name: OsString, value: OsString| {
        let wanted = key(&name);
        environment.retain(|(existing, _)| key(existing) != wanted);
        environment.push((name, value));
    };
    for (name, value) in registry_strings(
        HKEY_LOCAL_MACHINE,
        "System\\CurrentControlSet\\Control\\Session Manager\\Environment",
    ) {
        // The system key's USERNAME names the machine account, not the user.
        if key(&name) == "username" {
            continue;
        }
        set(&mut environment, name, value);
    }
    for (name, value) in registry_strings(HKEY_CURRENT_USER, "Environment") {
        let value = if key(&name) == "path" {
            match environment
                .iter()
                .find(|(existing, _)| key(existing) == "path")
            {
                Some((_, system)) => {
                    let mut joined = system.clone();
                    joined.push(";");
                    joined.push(&value);
                    joined
                }
                None => value,
            }
        } else {
            value
        };
        set(&mut environment, name, value);
    }
    environment
}

/// A pipe's two ends.
fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read: HANDLE = null_mut();
    let mut write: HANDLE = null_mut();
    // SAFETY: valid out-pointers; default security, so the ends are not inheritable.
    if unsafe { CreatePipe(&raw mut read, &raw mut write, null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: CreatePipe returned two new handles that nothing else owns.
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    })
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// `arg` quoted as the C runtime splits command lines.
fn quote(arg: &OsStr, line: &mut Vec<u16>) {
    let units: Vec<u16> = arg.encode_wide().collect();
    let needs_quotes = units.is_empty()
        || units
            .iter()
            .any(|unit| matches!(*unit, 0x20 | 0x09 | 0x0a | 0x0b | 0x22));
    if !needs_quotes {
        line.extend(units);
        return;
    }
    line.push(u16::from(b'"'));
    let mut backslashes = 0;
    for unit in units {
        if unit == u16::from(b'\\') {
            backslashes += 1;
            continue;
        }
        let escaped = if unit == u16::from(b'"') {
            backslashes * 2 + 1
        } else {
            backslashes
        };
        line.extend(std::iter::repeat_n(u16::from(b'\\'), escaped));
        line.push(unit);
        backslashes = 0;
    }
    line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes * 2));
    line.push(u16::from(b'"'));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsHandle as _;
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    fn text(units: &[u16]) -> String {
        String::from_utf16(units).unwrap()
    }

    fn cmd_line(args: &[&str]) -> io::Result<String> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let mut line = Vec::new();
        cmd_arguments(&args, &mut line)?;
        Ok(text(&line))
    }

    fn block(variables: &[(&str, &str)]) -> io::Result<Vec<u16>> {
        let mut command = SpawnCommand::new("program");
        command.env_clear().exact_environment();
        for (name, value) in variables {
            command.env(name, value);
        }
        exact_environment_block(&command)
    }

    #[test]
    fn quoting_escapes_backslashes_only_before_quotes() {
        let quoted = |arg: &str| {
            let mut line = Vec::new();
            quote(OsStr::new(arg), &mut line);
            text(&line)
        };
        assert_eq!(quoted("a\\"), "a\\");
        assert_eq!(quoted("a\"b"), "\"a\\\"b\"");
        assert_eq!(quoted("a b\\"), "\"a b\\\\\"");
        assert_eq!(quoted("/D"), "/D");
        assert_eq!(quoted(""), "\"\"");
    }

    #[test]
    fn cmd_scripts_are_passed_verbatim() {
        assert_eq!(
            cmd_line(&["/D", "/C", "echo \"a b\" & exit /b 37"]).unwrap(),
            " /D /S /C \"echo \"a b\" & exit /b 37\""
        );
        assert_eq!(cmd_line(&["/k", "dir"]).unwrap(), " /S /k \"dir\"");
        // Without a script, cmd's arguments are ordinary ones.
        assert_eq!(cmd_line(&["/Q", "a b"]).unwrap(), " /Q \"a b\"");
        assert!(cmd_line(&["/C", "echo", "ambiguous"]).is_err());
        assert!(cmd_line(&["/C"]).is_err());
        assert!(cmd_line(&["not-an-option", "/C", "echo"]).is_err());
        assert!(cmd_line(&["/D x", "/C", "echo"]).is_err());
        assert!(is_cmd(OsStr::new(r"C:\Windows\System32\CMD.EXE")));
        assert!(is_cmd(OsStr::new("cmd")));
        assert!(!is_cmd(OsStr::new("cmdx.exe")));
    }

    #[test]
    fn exact_environment_is_ordered_as_windows_orders_names() {
        let expected: Vec<u16> = "=C:=C:\\work\0a=1\0Path=C:\\Windows\0zz=2\0\0"
            .encode_utf16()
            .collect();
        assert_eq!(
            block(&[
                ("zz", "2"),
                ("Path", "C:\\Windows"),
                ("a", "1"),
                ("=C:", "C:\\work"),
            ])
            .unwrap(),
            expected
        );
        assert_eq!(block(&[]).unwrap(), vec![0, 0]);
    }

    #[test]
    fn exact_environment_keeps_cmd_pseudo_variables() {
        let block = block(&[
            ("=ExitCode", "00000000"),
            ("=ExitCodeAscii", "\u{20ac}"),
            ("=C:", "C:\\work"),
        ])
        .unwrap();
        let block = text(&block);
        assert!(block.contains("=ExitCode=00000000\0"));
        assert!(block.contains("=ExitCodeAscii=\u{20ac}\0"));
    }

    #[test]
    fn exact_environment_rejects_invalid_names() {
        for name in ["=", "==", "=C:=", "A=B", "=ExitCode=Value"] {
            assert!(block(&[(name, "value")]).is_err(), "accepted {name:?}");
        }
        assert!(valid_name(&[u16::from(b'='), 0xd800]));
        assert!(!valid_name(&[]));
        assert!(!valid_name(&[u16::from(b'A'), 0]));
    }

    #[test]
    fn exact_environment_rejects_names_equal_but_for_case() {
        // SpawnCommand already folds ASCII case; the block is checked as Windows compares.
        let variables = vec![
            ("\u{e9}".encode_utf16().collect(), vec![]),
            ("\u{c9}".encode_utf16().collect(), vec![]),
        ];
        assert!(sorted_block(variables).is_err());
        let surrogate = vec![(vec![u16::from(b'A')], vec![0xd800])];
        assert_eq!(sorted_block(surrogate).unwrap(), vec![65, 61, 0xd800, 0, 0]);
    }

    fn kill_on_close_job() -> OwnedHandle {
        // SAFETY: an unnamed job with default security.
        let job = unsafe { CreateJobObjectW(null(), null()) };
        assert!(!job.is_null());
        // SAFETY: a new handle nothing else owns.
        let job = unsafe { OwnedHandle::from_raw_handle(job) };
        // SAFETY: zeroed is a valid empty limit structure.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the structure outlives the call and its size is passed.
        let set = unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                u32::try_from(size_of_val(&limits)).unwrap(),
            )
        };
        assert_ne!(set, 0, "{}", io::Error::last_os_error());
        job
    }

    /// A console whose cursor queries are answered, with its output collected.
    fn console() -> (Master, Slave, std::sync::mpsc::Receiver<Vec<u8>>) {
        let (master, slave) = open(PtySize::new(80, 24)).unwrap();
        let mut reader = master.try_clone_reader().unwrap();
        let mut input = master.take_writer().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0; 4096];
            loop {
                let read = match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => return,
                    Ok(read) => read,
                };
                if buffer[..read].windows(4).any(|window| window == b"\x1b[6n") {
                    let _ = input.write(b"\x1b[1;1R");
                    let _ = input.flush();
                }
                if sender.send(buffer[..read].to_vec()).is_err() {
                    return;
                }
            }
        });
        (master, slave, receiver)
    }

    fn comspec() -> OsString {
        std::env::var_os("COMSPEC").unwrap_or_else(|| r"C:\Windows\System32\cmd.exe".into())
    }

    #[test]
    fn processes_start_inside_the_job_with_exactly_their_environment() {
        let (master, slave, output) = console();
        let job = kill_on_close_job();
        let mut command = SpawnCommand::new(comspec());
        command
            .env_clear()
            .exact_environment()
            .env("SystemRoot", std::env::var_os("SystemRoot").unwrap())
            .env("SWITCHY_PTY_EXACT", "only")
            .args([
                "/D",
                "/C",
                "echo [%SWITCHY_PTY_EXACT%][%USERNAME%]& exit /b 7",
            ]);
        let mut child = slave.spawn_in_job(&command, job.as_handle()).unwrap();
        let mut in_job = 0;
        // SAFETY: a live process handle and an owned job handle.
        let checked =
            unsafe { IsProcessInJob(child.as_raw_handle(), job.as_raw_handle(), &raw mut in_job) };
        assert_ne!(checked, 0, "{}", io::Error::last_os_error());
        assert_ne!(in_job, 0, "the process must start inside the job");
        assert_eq!(child.wait().unwrap().exit_code(), 7);
        drop(slave);
        drop(master);
        let text: Vec<u8> = output.iter().flatten().collect();
        let text = String::from_utf8_lossy(&text);
        assert!(text.contains("[only][%USERNAME%]"), "{text:?}");
    }

    #[test]
    fn closing_a_kill_on_close_job_ends_its_processes() {
        let (master, slave, _output) = console();
        let job = kill_on_close_job();
        let mut command = SpawnCommand::new(comspec());
        command.args(["/D", "/C", "ping -n 60 127.0.0.1 >nul"]);
        let mut child = slave.spawn_in_job(&command, job.as_handle()).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        // The job's only handle.
        drop(job);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "the job did not end its process"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(slave);
        drop(master);
    }
}
