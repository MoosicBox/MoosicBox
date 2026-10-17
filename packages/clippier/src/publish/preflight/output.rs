//! Single-writer, output-only live Cargo presentation for preflight.

use std::{
    io::{self, IsTerminal as _},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use bmux_tui::{
    ansi::{AnsiLog, ansi_to_lines},
    geometry::Size,
    inline_output::InlineOutput,
    text::Line,
};

use super::super::super::ColorMode;
use super::super::{BoxError, CommandOutput, run_streaming_command};

const FRAME_INTERVAL: Duration = Duration::from_millis(33);

fn interactive() -> bool {
    io::stdout().is_terminal()
        && io::stderr().is_terminal()
        && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
}

fn size() -> Size {
    bmux_tui::crossterm::terminal_size().unwrap_or(Size::new(80, 24))
}

/// Keep successes transient and commit failed transcripts exactly once.
pub(super) fn run(
    command: Command,
    color: ColorMode,
    label: &str,
) -> Result<CommandOutput, BoxError> {
    if !interactive() {
        return run_streaming_command(command);
    }
    eprintln!("Cargo preflight: {label}");
    run_inline(command, color, io::stderr(), size)
}

fn run_inline(
    mut command: Command,
    color: ColorMode,
    mut writer: impl io::Write,
    size: impl Fn() -> Size,
) -> Result<CommandOutput, BoxError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().ok_or("Missing Cargo stdout")?;
    let stderr = child.stderr.take().ok_or("Missing Cargo stderr")?;
    let (sender, receiver) = mpsc::sync_channel(32);
    let readers = [
        spawn_reader(stdout, false, sender.clone()),
        spawn_reader(stderr, true, sender),
    ];
    let mut display = InlineOutput::new(&mut writer, FRAME_INTERVAL);
    let mut out = AnsiLog::new(100, 4096);
    let mut err = AnsiLog::new(100, 4096);
    let mut transcript = Vec::new();
    let mut render_error = None;
    let mut last_frame = Instant::now()
        .checked_sub(FRAME_INTERVAL)
        .unwrap_or_else(Instant::now);
    // A timeout draws the final pending frame even if the producer goes quiet.
    loop {
        match receiver.recv_timeout(FRAME_INTERVAL) {
            Ok((is_stderr, bytes)) => {
                if is_stderr {
                    err.push(&bytes);
                } else {
                    out.push(&bytes);
                }
                transcript.push((is_stderr, bytes));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if render_error.is_none() && last_frame.elapsed() >= FRAME_INTERVAL {
            last_frame = Instant::now();
            let size = size();
            let rows = usize::from(size.height.saturating_sub(2)).min(12);
            let width = usize::from(size.width.saturating_sub(1));
            let mut lines = out.tail(width, rows / 3);
            lines.extend(err.tail(width, rows.saturating_sub(lines.len())));
            if matches!(color, ColorMode::Never) {
                lines = lines
                    .iter()
                    .map(|line| Line::raw(line.plain_text()))
                    .collect();
            }
            if let Err(error) = display.draw(&lines, size, Instant::now(), false) {
                render_error = Some(error);
            }
        }
    }
    // Always drain and reap before returning a reader/presentation error.
    let status = child.wait()?;
    let mut read_error = None;
    for reader in readers {
        match reader.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => read_error = Some(error.to_string()),
            Err(_) => read_error = Some("Cargo output reader panicked".to_string()),
        }
    }
    if let Some(error) = render_error {
        display.retain();
        // The anchor is poisoned: fall back without further cursor operations.
        drop(display);
        for (_, bytes) in &transcript {
            writer.write_all(bytes)?;
        }
        return Err(format!("Preflight output rendering failed: {error}").into());
    }
    display.clear(size())?;
    if !status.success() || read_error.is_some() {
        let lines = failure_lines(&transcript, color);
        display.commit(&lines, size())?;
    }
    if let Some(error) = read_error {
        return Err(error.into());
    }
    Ok(CommandOutput {
        status,
        stderr: if status.success() {
            String::new()
        } else {
            "Complete Cargo output retained above.".to_string()
        },
    })
}

fn failure_lines(transcript: &[(bool, Vec<u8>)], color: ColorMode) -> Vec<Line> {
    // Decode streams separately so stdout cannot change stderr's SGR state.
    let mut lines = vec![Line::raw(
        "Cargo preflight failed (complete stdout, then stderr):",
    )];
    for stream in [false, true] {
        let bytes = transcript
            .iter()
            .filter(|(stderr, _)| *stderr == stream)
            .flat_map(|(_, bytes)| bytes.iter().copied())
            .collect::<Vec<_>>();
        lines.extend(ansi_to_lines(&String::from_utf8_lossy(&bytes)));
    }
    if matches!(color, ColorMode::Never) {
        lines = lines
            .iter()
            .map(|line| Line::raw(line.plain_text()))
            .collect();
    }
    lines
}

fn spawn_reader(
    mut reader: impl io::Read + Send + 'static,
    stderr: bool,
    sender: mpsc::SyncSender<(bool, Vec<u8>)>,
) -> thread::JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut buffer = [0; 8192];
        loop {
            let count = match reader.read(&mut buffer) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                return Ok(());
            }
            if sender.send((stderr, buffer[..count].to_vec())).is_err() {
                return Ok(());
            }
        }
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn command(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn failure_persists_complete_streams_and_does_not_duplicate_in_error() {
        let mut bytes = Vec::new();
        let output = run_inline(
            command("printf 'stdout detail\\n'; printf '\\033[31mactual failure\\033[0m\\n' >&2; exit 1"),
            ColorMode::Always,
            &mut bytes,
            || Size::new(120, 24),
        ).unwrap();
        assert!(!output.status.success());
        let rendered = String::from_utf8(bytes).unwrap();
        let committed = rendered.split("Cargo preflight failed").nth(1).unwrap();
        assert!(committed.contains("stdout detail"));
        assert!(committed.contains("actual failure"));
        assert!(committed.contains("\x1b[0;31m"));
        assert!(!output.stderr.contains("actual failure"));
        assert!(output.stderr.contains("retained above"));
    }

    #[test]
    fn success_clears_region_without_committing_compilation_transcript() {
        let mut bytes = Vec::new();
        let output = run_inline(
            command("printf 'successful compilation\\n' >&2"),
            ColorMode::Never,
            &mut bytes,
            || Size::new(80, 24),
        )
        .unwrap();
        assert!(output.status.success());
        let rendered = String::from_utf8(bytes).unwrap();
        assert!(!rendered.contains("Cargo preflight failed"));
        assert!(rendered.contains("\x1b[2K"));
        assert_eq!(output.stderr, "");
    }

    #[test]
    fn large_failure_keeps_early_and_late_diagnostics_despite_bounded_tail() {
        let mut bytes = Vec::new();
        let output = run_inline(command("printf 'EARLY_ERROR\\n' >&2; i=0; while [ $i -lt 300 ]; do printf 'compilation detail\\n' >&2; i=$((i+1)); done; printf 'LATE_ERROR\\n' >&2; exit 1"), ColorMode::Never, &mut bytes, || Size::new(80, 10)).unwrap();
        assert!(!output.status.success());
        let rendered = String::from_utf8(bytes).unwrap();
        let committed = rendered.split("Cargo preflight failed").nth(1).unwrap();
        assert!(committed.contains("EARLY_ERROR"));
        assert!(committed.contains("LATE_ERROR"));
        assert!(!committed.contains("\x1b[31m"));
    }

    #[test]
    fn spawn_failure_is_reported_without_drawing() {
        let mut bytes = Vec::new();
        assert!(
            run_inline(
                Command::new("/nonexistent/clippier-test-command"),
                ColorMode::Never,
                &mut bytes,
                || Size::new(80, 24)
            )
            .is_err()
        );
        assert_eq!(bytes, Vec::<u8>::new());
    }
}
