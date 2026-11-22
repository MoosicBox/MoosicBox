//! Behaviour of pseudo-terminal pairs: processes started on the slave, driven from the master.
#![cfg(all(any(unix, windows), not(feature = "simulator")))]

use std::io::{ErrorKind, Read as _, Write as _};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use switchy_pty::{PtyChild, PtyInput, PtyMaster, PtyPair, PtySize, SpawnCommand};

const TIMEOUT: Duration = Duration::from_secs(30);
const CURSOR_QUERY: &[u8] = b"\x1b[6n";

/// A terminal whose output is collected on a thread that also answers cursor queries, as the
/// Windows console waits for one before its first process runs.
struct Terminal {
    master: PtyMaster,
    input: Arc<Mutex<Option<PtyInput>>>,
    output: Arc<Mutex<Vec<u8>>>,
    reader: Option<JoinHandle<()>>,
}

impl Terminal {
    fn open(size: PtySize) -> (Self, switchy_pty::PtySlave) {
        let PtyPair { slave, master } = PtyPair::open(size).unwrap();
        let input = Arc::new(Mutex::new(Some(master.take_writer().unwrap())));
        let output = Arc::new(Mutex::new(Vec::new()));
        let mut reader = master.try_clone_reader().unwrap();
        let reader = {
            let (input, output) = (Arc::clone(&input), Arc::clone(&output));
            std::thread::spawn(move || {
                let mut buffer = [0; 4096];
                let mut answered = 0;
                loop {
                    let read = match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => return,
                        Ok(read) => read,
                    };
                    let mut output = output.lock().unwrap();
                    output.extend_from_slice(&buffer[..read]);
                    let queries = output
                        .windows(CURSOR_QUERY.len())
                        .filter(|window| *window == CURSOR_QUERY)
                        .count();
                    drop(output);
                    for _ in answered..queries {
                        if let Some(input) = input.lock().unwrap().as_mut() {
                            let _ = input.write_all(b"\x1b[1;1R");
                            let _ = input.flush();
                        }
                    }
                    answered = queries;
                }
            })
        };
        (
            Self {
                master,
                input,
                output,
                reader: Some(reader),
            },
            slave,
        )
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }

    /// Wait until the output contains `needle`.
    fn wait_for(&self, needle: &str) -> String {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let text = self.text();
            if text.contains(needle) {
                return text;
            }
            assert!(Instant::now() < deadline, "no {needle:?} in {text:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn type_text(&self, text: &str) {
        let mut input = self.input.lock().unwrap();
        let input = input.as_mut().expect("the writer is still held");
        input.write_all(text.as_bytes()).unwrap();
        input.flush().unwrap();
    }

    /// Close the terminal and wait for its output to end.
    fn close(self) -> String {
        let Self {
            master,
            input,
            output,
            reader,
        } = self;
        input.lock().unwrap().take();
        drop(master);
        reader.unwrap().join().unwrap();
        String::from_utf8_lossy(&output.lock().unwrap()).into_owned()
    }
}

/// Wait for `child` to exit, at most [`TIMEOUT`].
fn wait(child: &mut PtyChild) -> switchy_pty::ChildExit {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(exit) = child.try_wait().unwrap() {
            return exit;
        }
        assert!(Instant::now() < deadline, "the process did not exit");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "switchy-pty-pair-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
fn script(text: &str) -> SpawnCommand {
    let mut command = SpawnCommand::new("sh");
    command.args(["-c", text]);
    command
}

#[cfg(windows)]
fn script(text: &str) -> SpawnCommand {
    let mut command = SpawnCommand::new("cmd.exe");
    command.args(["/v:on", "/c", text]);
    command
}

#[cfg(unix)]
fn long_running() -> SpawnCommand {
    let mut command = SpawnCommand::new("sleep");
    command.arg("30");
    command
}

#[cfg(windows)]
fn long_running() -> SpawnCommand {
    let mut command = SpawnCommand::new("ping");
    command.args(["-n", "30", "127.0.0.1"]);
    command
}

#[test]
fn output_and_exit_code_reach_the_master() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    #[cfg(unix)]
    let command = script("printf 'marker-%s\\n' ok; exit 7");
    #[cfg(windows)]
    let command = script("echo marker-ok& exit /b 7");
    let mut child = slave.spawn(&command).unwrap();
    drop(slave);
    terminal.wait_for("marker-ok");
    let exit = wait(&mut child);
    assert_eq!(exit.exit_code(), 7);
    assert!(!exit.success());
    assert_eq!(exit.signal(), None);
    terminal.close();
}

#[test]
fn typed_input_reaches_the_process() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    #[cfg(unix)]
    let command = script("read line; echo \"got-$line\"");
    #[cfg(windows)]
    let command = script("set /p LINE=& echo got-!LINE!");
    let mut child = slave.spawn(&command).unwrap();
    drop(slave);
    terminal.type_text("hello\r");
    terminal.wait_for("got-hello");
    assert!(wait(&mut child).success());
    terminal.close();
}

#[test]
fn resizing_reaches_processes_started_later() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    terminal.master.resize(PtySize::new(100, 40)).unwrap();
    assert_eq!(terminal.master.size().unwrap(), PtySize::new(100, 40));
    #[cfg(unix)]
    let command = script("echo \"size-$(stty size)\"");
    #[cfg(windows)]
    let command = {
        let mut command = SpawnCommand::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-Command",
            "Write-Output (\"size-\" + [Console]::WindowHeight + \" \" + [Console]::WindowWidth)",
        ]);
        command
    };
    let mut child = slave.spawn(&command).unwrap();
    drop(slave);
    terminal.wait_for("size-40 100");
    assert!(wait(&mut child).success());
    terminal.close();
}

#[test]
fn the_writer_is_taken_once() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let error = terminal.master.take_writer().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    drop(slave);
    terminal.close();
}

#[test]
fn environment_and_working_directory_reach_the_process() {
    let dir = scratch_dir();
    let (terminal, slave) = Terminal::open(PtySize::new(200, 24));
    #[cfg(unix)]
    let mut command = script("echo \"env-$SWITCHY_PTY_PAIR_VALUE\"; echo \"cwd-$(pwd -P)\"");
    #[cfg(windows)]
    let mut command = script("echo env-%SWITCHY_PTY_PAIR_VALUE%& echo cwd-!CD!");
    command.env("SWITCHY_PTY_PAIR_VALUE", "present").cwd(&dir);
    assert_eq!(
        command.get_env("SWITCHY_PTY_PAIR_VALUE"),
        Some("present".as_ref())
    );
    let mut child = slave.spawn(&command).unwrap();
    drop(slave);
    terminal.wait_for("env-present");
    let name = dir.file_name().unwrap().to_str().unwrap();
    terminal.wait_for(name);
    assert!(wait(&mut child).success());
    terminal.close();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_working_directory_falls_back_to_home() {
    let home = scratch_dir();
    let (terminal, slave) = Terminal::open(PtySize::new(200, 24));
    #[cfg(unix)]
    let mut command = script("echo \"cwd-$(pwd -P)\"");
    #[cfg(windows)]
    let mut command = script("echo cwd-!CD!");
    #[cfg(unix)]
    command.env("HOME", &home);
    #[cfg(windows)]
    command.env("USERPROFILE", &home);
    command.cwd(home.join("missing"));
    let mut child = slave.spawn(&command).unwrap();
    drop(slave);
    let name = home.file_name().unwrap().to_str().unwrap();
    let text = terminal.wait_for(name);
    assert!(!text.contains("missing"), "{text:?}");
    assert!(wait(&mut child).success());
    terminal.close();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_missing_program_is_reported() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let error = slave
        .spawn(&SpawnCommand::new("switchy-pty-no-such-program"))
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::NotFound, "{error}");
    drop(slave);
    terminal.close();
}

#[cfg(unix)]
#[test]
fn an_unreaped_exit_is_reported_and_reaped_later() {
    let (_terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave.spawn(&script("exit 7")).unwrap();
    drop(slave);
    let exit = child.wait_unreaped().unwrap();
    assert_eq!(exit.exit_code(), 7);
    // Still a zombie: its id is taken until it is reaped.
    let alive = std::process::Command::new("kill")
        .args(["-0", &child.id().to_string()])
        .status()
        .unwrap()
        .success();
    assert!(alive, "a zombie still holds its id");
    assert_eq!(wait(&mut child), exit);
}

#[test]
fn kill_ends_the_process() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave.spawn(&long_running()).unwrap();
    drop(slave);
    std::thread::sleep(Duration::from_millis(200));
    assert!(child.try_wait().unwrap().is_none());
    child.kill().unwrap();
    let exit = wait(&mut child);
    assert!(!exit.success());
    #[cfg(unix)]
    assert!(exit.signal().is_some(), "{exit}");
    #[cfg(windows)]
    assert_eq!(exit.exit_code(), 1);
    // Killing what has exited succeeds.
    child.kill().unwrap();
    terminal.close();
}

#[test]
fn a_killer_ends_the_process_from_another_thread() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave.spawn(&long_running()).unwrap();
    drop(slave);
    let killer = child.killer().unwrap();
    std::thread::spawn(move || killer.kill().unwrap())
        .join()
        .unwrap();
    let exit = wait(&mut child);
    assert!(!exit.success());
    child.killer().unwrap().kill().unwrap();
    terminal.close();
}

#[cfg(unix)]
#[test]
fn the_process_leads_the_terminals_foreground_group() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let tty = terminal.master.tty_name().expect("a slave device path");
    assert!(tty.exists(), "{}", tty.display());
    let mut child = slave.spawn(&long_running()).unwrap();
    drop(slave);
    let pid = i32::try_from(child.id()).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while terminal.master.foreground_process_group() != Some(pid) {
        assert!(
            Instant::now() < deadline,
            "the process never led the terminal"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    wait(&mut child);
    terminal.close();
}

#[cfg(unix)]
#[test]
fn a_signal_is_reported_by_name() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave.spawn(&script("kill -TERM $$")).unwrap();
    drop(slave);
    let exit = wait(&mut child);
    assert!(!exit.success());
    assert_eq!(exit.exit_code(), 1);
    let signal = exit.signal().expect("a signal ended the process");
    assert!(!signal.is_empty());
    assert!(exit.to_string().starts_with("Terminated by "), "{exit}");
    terminal.close();
}

#[cfg(unix)]
#[test]
fn dropping_the_writer_ends_the_input() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave
        .spawn(&script("cat >/dev/null; echo input-ended"))
        .unwrap();
    drop(slave);
    std::thread::sleep(Duration::from_millis(200));
    terminal.input.lock().unwrap().take();
    terminal.wait_for("input-ended");
    assert!(wait(&mut child).success());
    terminal.close();
}

#[cfg(unix)]
#[test]
fn output_ends_when_the_last_process_holding_the_terminal_exits() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave.spawn(&script("echo done-writing")).unwrap();
    drop(slave);
    assert!(wait(&mut child).success());
    let Terminal { reader, output, .. } = terminal;
    reader.unwrap().join().unwrap();
    assert!(String::from_utf8_lossy(&output.lock().unwrap()).contains("done-writing"));
}

#[cfg(windows)]
#[test]
fn the_console_asks_for_the_cursor_before_the_first_process_runs() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave.spawn(&script("echo after-cursor")).unwrap();
    drop(slave);
    let text = terminal.wait_for("after-cursor");
    assert!(text.starts_with("\x1b[6n"), "{text:?}");
    assert!(wait(&mut child).success());
    terminal.close();
}

#[cfg(windows)]
#[test]
fn output_ends_once_the_console_is_closed() {
    let (terminal, slave) = Terminal::open(PtySize::new(80, 24));
    let mut child = slave.spawn(&script("echo done-writing")).unwrap();
    drop(slave);
    assert!(wait(&mut child).success());
    terminal.wait_for("done-writing");
    // The reader thread ends once the master (and with it the console) is gone.
    let text = terminal.close();
    assert!(text.contains("done-writing"));
}

/// Opening terminals from many threads at once succeeds (macOS's kernel can ask for an open
/// again when they race, which must not surface as an error).
#[test]
fn terminals_open_concurrently() {
    let failures = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let failures = Arc::clone(&failures);
            std::thread::spawn(move || {
                for _ in 0..40 {
                    if let Err(error) = PtyPair::open(PtySize::new(80, 24)) {
                        eprintln!("open failed: {error:?}");
                        failures.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(failures.load(Ordering::Relaxed), 0);
}

#[test]
fn behavior_is_the_operating_systems() {
    assert_eq!(
        switchy_pty::behavior(),
        switchy_pty::Behavior::of(cfg!(windows))
    );
}
