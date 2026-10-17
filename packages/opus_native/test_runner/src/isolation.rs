//! Same-executable case isolation. No shell, caller-selected executable, or
//! recursive runner invocation. Output capture is bounded and always drained.
use super::{Result, Value, corpus, hash, json, read};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_OUTPUT: usize = 1024 * 1024;

fn capture(mut source: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(bytes)
}

/// Private immutable copy of the running executable, removed after all children.
pub struct ExecutableSnapshot {
    directory: std::path::PathBuf,
    pub path: std::path::PathBuf,
    pub sha256: String,
}

impl ExecutableSnapshot {
    pub fn new() -> Result<Self> {
        // Linux exposes the running inode even if Cargo unlinks/replaces its path.
        #[cfg(target_os = "linux")]
        let source = std::path::PathBuf::from("/proc/self/exe");
        #[cfg(not(target_os = "linux"))]
        let source = std::env::current_exe().map_err(|error| error.to_string())?;
        let bytes = read(&source)?;
        let sha256 = hash(&bytes);
        let mut directory = None;
        for counter in 0..1000 {
            let candidate = std::env::temp_dir().join(format!(
                "moosicbox-opus-runner-{}-{}-{counter}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| error.to_string())?
                    .as_nanos()
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&candidate) {
                Ok(()) => {
                    directory = Some(candidate);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error.to_string()),
            }
        }
        let directory = directory.ok_or("cannot allocate private executable snapshot directory")?;
        let snapshot = Self {
            path: directory.join("runner"),
            directory,
            sha256,
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&snapshot.path)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&snapshot.path, std::fs::Permissions::from_mode(0o500))
                .map_err(|error| error.to_string())?;
        }
        if hash(&read(&snapshot.path)?) != snapshot.sha256 {
            return Err("snapshot verification failed".into());
        }
        Ok(snapshot)
    }
}

impl Drop for ExecutableSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

pub fn child(root: &Path, id: &str, expected_executable: &str) -> Result<Value> {
    if std::env::var("MOOSICBOX_ENTROPY_CASE_CHILD").as_deref() != Ok("1") {
        return Err("internal case protocol requires parent marker".into());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    if hash(&read(&executable)?) != expected_executable {
        return Err("child executable identity mismatch".into());
    }
    let (entries, _) = corpus::inventory(root)?;
    let entry = entries.get(id).ok_or("unknown internal case")?;
    // Direct decode only: children never call run()/spawn().
    Ok(super::case(root, id, entry))
}

pub fn run(root: &Path, id: &str, entry: &corpus::Entry, snapshot: &ExecutableSnapshot) -> Value {
    match execute(root, id, &snapshot.path, &snapshot.sha256, TIMEOUT) {
        Ok(record) => record,
        Err((kind, error)) => {
            let mut record = json!({"type":"case", "id":id, "comparison":"exact_uncompensated_pcm", "reproduce":["--case",id], "kind":if entry.sequence {"sequence"} else {"single"}, "status":"harness_failure", "exercised":false, "error":error, "isolation":{"outcome":kind}});
            if kind != "harness_failure"
                && let Ok((packets, identity)) = corpus::packets(root, id, entry)
            {
                record["fixture_sha256"] = json!(identity);
                record["mode"] = json!(super::packet_mode(&packets[0].packet).unwrap_or("unknown"));
                record["exercised"] = json!(true);
                record["status"] = json!(kind);
            }
            record
        }
    }
}

fn execute(
    root: &Path,
    id: &str,
    executable: &Path,
    executable_hash: &str,
    timeout: Duration,
) -> std::result::Result<Value, (&'static str, String)> {
    let failure = |error: String| ("harness_failure", error);
    if hash(&read(executable).map_err(failure)?) != executable_hash {
        return Err(failure("parent executable changed before spawn".into()));
    }
    let process = Command::new(executable)
        .args([
            "--internal-case",
            id,
            "--expected-executable",
            executable_hash,
            "--root",
        ])
        .arg(root)
        .env("MOOSICBOX_ENTROPY_CASE_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| failure(error.to_string()))?;
    let Captured {
        status,
        timed_out,
        stdout,
        stderr,
    } = supervise(process, timeout).map_err(failure)?;
    if timed_out {
        return Err((
            "codec_timeout",
            format!(
                "case exceeded {}ms; child killed and reaped",
                timeout.as_millis()
            ),
        ));
    }

    if !status.success() {
        #[cfg(unix)]
        let signal = {
            use std::os::unix::process::ExitStatusExt;
            status.signal()
        };
        #[cfg(not(unix))]
        let signal: Option<i32> = None;
        return Err((
            if status.code() == Some(2) {
                "harness_failure"
            } else {
                "codec_crash"
            },
            format!(
                "child exit code {:?}, signal {:?}; stderr: {}",
                status.code(),
                signal,
                String::from_utf8_lossy(&stderr)
            ),
        ));
    }
    if stdout.len() > MAX_OUTPUT {
        return Err(failure("child output exceeded bound".into()));
    }
    let mut record: Value = serde_json::from_slice(&stdout)
        .map_err(|error| failure(format!("invalid child JSON: {error}")))?;
    if record["type"] != "case" || record["id"] != id {
        return Err(failure("child response identity mismatch".into()));
    }
    record["isolation"] = json!({"outcome":"completed", "timeout_ms":timeout.as_millis(), "executable_sha256":executable_hash});
    Ok(record)
}

struct Captured {
    status: std::process::ExitStatus,
    timed_out: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn supervise(mut process: std::process::Child, timeout: Duration) -> Result<Captured> {
    let stdout = process.stdout.take().expect("piped stdout");
    let stderr = process.stderr.take().expect("piped stderr");
    let out = thread::spawn(move || capture(stdout, MAX_OUTPUT + 1));
    let err = thread::spawn(move || capture(stderr, 8192));
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match process.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                timed_out = true;
                let _ = process.kill();
                break process.wait();
            }
            Err(error) => {
                let _ = process.kill();
                let _ = process.wait();
                break Err(error);
            }
        }
    };
    let stdout = out
        .join()
        .map_err(|_| "stdout collector panicked".to_owned())?
        .map_err(|error| error.to_string())?;
    let stderr = err
        .join()
        .map_err(|_| "stderr collector panicked".to_owned())?
        .map_err(|error| error.to_string())?;
    Ok(Captured {
        status: status.map_err(|error| error.to_string())?,
        timed_out,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_probe() {
        match std::env::var("MOOSICBOX_RUNNER_TEST_PROBE").as_deref() {
            Ok("exit") => std::process::exit(23),
            Ok("sleep") => thread::sleep(Duration::from_secs(60)),
            #[cfg(target_os = "linux")]
            Ok("replace") => {
                let snapshot = ExecutableSnapshot::new().unwrap();
                let running_path = std::env::current_exe().unwrap();
                assert_eq!(running_path.file_name().unwrap(), "runner");
                assert!(
                    running_path
                        .parent()
                        .unwrap()
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("moosicbox-opus-runner-")
                );
                std::fs::remove_file(&running_path).unwrap();
                // The running executable path now says (deleted). A second
                // snapshot must still read its inode through /proc/self/exe.
                let after_unlink = ExecutableSnapshot::new().unwrap();
                assert_eq!(after_unlink.sha256, snapshot.sha256);
                let status = Command::new(&after_unlink.path)
                    .args(["--exact", "isolation::tests::child_probe"])
                    .env_remove("MOOSICBOX_RUNNER_TEST_PROBE")
                    .status()
                    .unwrap();
                assert!(status.success());
            }
            _ => (),
        }
    }

    fn probe(mode: &str) -> std::process::Child {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "isolation::tests::child_probe", "--nocapture"])
            .env("MOOSICBOX_RUNNER_TEST_PROBE", mode)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    #[test]
    fn timeout_kills_and_reaps_child_and_exit_status_is_preserved() {
        let start = Instant::now();
        let output = supervise(probe("sleep"), Duration::from_millis(50)).unwrap();
        assert!(output.timed_out);
        assert!(!output.status.success());
        assert!(start.elapsed() < Duration::from_secs(5));
        let output = supervise(probe("exit"), Duration::from_secs(5)).unwrap();
        assert!(!output.timed_out);
        assert_eq!(output.status.code(), Some(23));
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let mut process = probe("sleep");
            process.kill().unwrap();
            let output = supervise(process, Duration::from_secs(5)).unwrap();
            assert_eq!(output.status.signal(), Some(9));
            assert!(!output.timed_out);
        }
    }

    #[test]
    fn immutable_snapshot_survives_source_path_replacement_and_cleans_up() {
        let snapshot = ExecutableSnapshot::new().unwrap();
        let path = snapshot.path.clone();
        let directory = snapshot.directory.clone();
        assert_eq!(hash(&read(&path).unwrap()), snapshot.sha256);
        let process = Command::new(&path)
            .args(["--exact", "isolation::tests::child_probe"])
            .env(
                "MOOSICBOX_RUNNER_TEST_PROBE",
                if cfg!(target_os = "linux") {
                    "replace"
                } else {
                    "normal"
                },
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        assert!(
            supervise(process, Duration::from_secs(5))
                .unwrap()
                .status
                .success()
        );
        drop(snapshot);
        assert!(!path.exists());
        assert!(!directory.exists());
    }

    #[test]
    fn bounded_capture_drains_without_retaining_excess() {
        assert_eq!(capture(&[1, 2, 3, 4][..], 2).unwrap(), [1, 2]);
    }
}
