//! A library of simulated programs that exercise terminals the way real ones do.

use std::time::Duration;

use super::{Disposition, Process, register_program};
use crate::Signal;

fn arg<T: std::str::FromStr>(process: &Process, index: usize, default: T) -> T {
    process
        .args()
        .get(index)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Copy standard input to standard output until end of input.
fn cat(process: &mut Process) -> i32 {
    let mut buffer = [0_u8; 256];
    loop {
        match process.read(&mut buffer) {
            Ok(0) | Err(_) => return 0,
            Ok(read) => {
                if process.write(&buffer[..read]).is_err() {
                    return 1;
                }
            }
        }
    }
}

/// `flood TOTAL CHUNK`: write TOTAL bytes (default 65536) in CHUNK-byte writes (default 997),
/// cycling through `a` to `z` so order is checkable.
fn flood(process: &mut Process) -> i32 {
    let total: usize = arg(process, 1, 65_536);
    let chunk: usize = arg::<usize>(process, 2, 997).max(1);
    let mut written = 0;
    while written < total {
        let size = chunk.min(total - written);
        let bytes: Vec<u8> = (written..written + size)
            .map(|i| b'a' + u8::try_from(i % 26).unwrap_or(0))
            .collect();
        if process.write(&bytes).is_err() {
            return 1;
        }
        written += size;
    }
    0
}

/// `sleep MILLIS`.
fn sleep(process: &mut Process) -> i32 {
    let millis: u64 = arg(process, 1, 1000);
    process.sleep(Duration::from_millis(millis));
    0
}

/// `exit CODE`: write `bye` and exit with CODE.
fn exit(process: &mut Process) -> i32 {
    let _ = process.write(b"bye\n");
    arg(process, 1, 0)
}

/// Ignore hang-ups, then run until killed.
fn ignore_hup(process: &mut Process) -> i32 {
    process.handle(Signal::Hangup, Disposition::Ignore);
    let _ = process.write(b"ignoring hangups\n");
    loop {
        process.sleep(Duration::from_secs(60));
    }
}

/// Never read standard input; run until killed.
fn never_reads(process: &mut Process) -> i32 {
    loop {
        process.sleep(Duration::from_secs(60));
    }
}

/// `linger MILLIS`: ignore hang-ups, sleep, then write `lingered`.
fn linger(process: &mut Process) -> i32 {
    process.handle(Signal::Hangup, Disposition::Ignore);
    let millis: u64 = arg(process, 1, 100);
    process.sleep(Duration::from_millis(millis));
    let _ = process.write(b"lingered\n");
    0
}

/// `daemonize MILLIS`: start `linger MILLIS` in a group of its own holding the terminal, write
/// `parent done` and exit at once.
fn daemonize(process: &mut Process) -> i32 {
    let millis: u64 = arg(process, 1, 100);
    let millis = millis.to_string();
    if process.spawn(&["linger", &millis], true).is_err() {
        return 1;
    }
    let _ = process.write(b"parent done\n");
    0
}

/// `shell N`: start N `sleep 3600` children in this group (as `a & b & wait` does), then wait
/// for a signal.
fn shell(process: &mut Process) -> i32 {
    let children: usize = arg(process, 1, 2);
    for _ in 0..children {
        if process.spawn(&["sleep", "3600000"], false).is_err() {
            return 1;
        }
    }
    let _ = process.write(b"started\n");
    loop {
        process.sleep(Duration::from_secs(60));
    }
}

/// Report the terminal size at start and after every window change: `COLSxROWS`.
fn resize_reporter(process: &mut Process) -> i32 {
    process.handle(Signal::WindowChange, Disposition::Catch);
    loop {
        let size = process.terminal_size().unwrap_or(crate::PtySize::new(0, 0));
        if process
            .write(format!("{}x{}\n", size.cols, size.rows).as_bytes())
            .is_err()
        {
            return 1;
        }
        if process.wait_signal(Duration::from_secs(3600)).is_none() {
            return 0;
        }
    }
}

/// Ask the terminal for the cursor position, wait for the reply, and report it.
fn query_responder(process: &mut Process) -> i32 {
    process.set_raw(true);
    if process.write(b"\x1b[6n").is_err() {
        return 1;
    }
    let mut reply = Vec::new();
    let mut buffer = [0_u8; 32];
    while !reply.ends_with(b"R") {
        match process.read(&mut buffer) {
            Ok(0) | Err(_) => return 1,
            Ok(read) => reply.extend_from_slice(&buffer[..read]),
        }
    }
    let _ = process.write(format!("reply {}\n", reply.len()).as_bytes());
    0
}

/// Print the value of the environment variable named by the first argument.
fn printenv(process: &mut Process) -> i32 {
    let name = process.args().get(1).cloned().unwrap_or_default();
    let value = process.env(&name).unwrap_or("").to_owned();
    i32::from(process.write(format!("{value}\n").as_bytes()).is_err())
}

/// Print the working directory.
fn pwd(process: &mut Process) -> i32 {
    let cwd = process.cwd().display().to_string();
    i32::from(process.write(format!("{cwd}\n").as_bytes()).is_err())
}

/// Write the arguments to standard output (`echo`) or error (`echo-err`).
fn echo(process: &mut Process) -> i32 {
    let line = format!("{}\n", process.args()[1..].join(" "));
    i32::from(process.write(line.as_bytes()).is_err())
}

fn echo_err(process: &mut Process) -> i32 {
    let line = format!("{}\n", process.args()[1..].join(" "));
    i32::from(process.write_fd(2, line.as_bytes()).is_err())
}

/// Register every program of the library for the current run:
///
/// * `cat`: standard input to standard output.
/// * `flood TOTAL CHUNK`: a long, ordered stream of output.
/// * `sleep MILLIS`, `exit CODE`.
/// * `ignore-hup`: survives hang-ups until killed.
/// * `never-reads`: never reads its input.
/// * `linger MILLIS`, `daemonize MILLIS`: a child that outlives its parent and keeps the
///   terminal open.
/// * `shell N`: N children in its process group.
/// * `resize-reporter`: reports every window size change.
/// * `query-responder`: asks the terminal a question and waits for the answer.
/// * `printenv NAME`, `pwd`, `echo ARGS`, `echo-err ARGS`.
pub fn install_default_programs() {
    register_program("cat", cat);
    register_program("flood", flood);
    register_program("sleep", sleep);
    register_program("exit", exit);
    register_program("ignore-hup", ignore_hup);
    register_program("never-reads", never_reads);
    register_program("linger", linger);
    register_program("daemonize", daemonize);
    register_program("shell", shell);
    register_program("resize-reporter", resize_reporter);
    register_program("query-responder", query_responder);
    register_program("printenv", printenv);
    register_program("pwd", pwd);
    register_program("echo", echo);
    register_program("echo-err", echo_err);
}
