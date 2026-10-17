use super::*;

// Run under a killable deadline so a FIFO regression cannot hang the test runner.
#[test]
fn special_file_child() {
    let Some(root) = std::env::var_os("OPUS_FROZEN_SPECIAL_TEST_ROOT") else {
        return;
    };
    assert!(verify_frozen_corpus(Path::new(&root)).is_err());
}

#[cfg(unix)]
fn rejection_with_deadline(root: &Path) {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "frozen::bounded_tests::special_file_child",
            "--nocapture",
        ])
        .env("OPUS_FROZEN_SPECIAL_TEST_ROOT", root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "Verifier did not return an error");
            let output = child.wait_with_output().unwrap();
            assert!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .contains("1 passed"),
                "Deadline child did not execute the verifier test"
            );
            break;
        }
        if start.elapsed() > Duration::from_secs(2) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Frozen verifier blocked on special/oversized file");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[test]
fn genuine_corpus_special_files_fail_within_deadline() {
    use std::{
        os::unix::{fs::symlink, net::UnixListener},
        process::Command,
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(".s-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let manifest = json(MANIFEST).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("frozen/corpus");
    for relative in manifest["files"].as_object().unwrap().keys() {
        let destination = root.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(source.join(relative), destination).unwrap();
    }
    assert!(verify_frozen_corpus(&root).is_ok());
    for relative in [
        "celt/fb/silence_mono/expected.pcm",
        "inventory.json",
        "silk/mb/mixed_stereo/packet.bin",
        "sequences/celt_fb_mono/sequence.json",
    ] {
        let path = root.join(relative);
        let bytes = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(
            Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        rejection_with_deadline(&root);
        fs::remove_file(&path).unwrap();
        let socket_path = root.join("s");
        let listener = UnixListener::bind(&socket_path).unwrap();
        fs::rename(socket_path, &path).unwrap();
        rejection_with_deadline(&root);
        drop(listener);
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        rejection_with_deadline(&root);
        fs::remove_dir(&path).unwrap();
        symlink(source.join(relative), &path).unwrap();
        rejection_with_deadline(&root);
        fs::remove_file(&path).unwrap();
        let file = fs::File::create(&path).unwrap();
        file.set_len(1 << 30).unwrap();
        drop(file);
        rejection_with_deadline(&root);
        fs::write(&path, bytes).unwrap();
    }
    assert!(read(Path::new("/dev"), "null", 0).is_err());
    assert!(verify_frozen_corpus(&root).is_ok());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bounded_reads_and_manifest_inputs() {
    // Unending input simulates growth after fstat: expected+1 must terminate.
    assert!(read_bounded(std::io::repeat(0), 16).is_err());
    assert!(read_bounded(std::io::Cursor::new([0; 15]), 16).is_err());
    assert_eq!(
        read_bounded(std::io::Cursor::new([0; 16]), 16).unwrap(),
        [0; 16]
    );
    assert!(read_bounded(std::io::empty(), u64::MAX).is_err());
    for malformed in [b"".as_slice(), b"{", b"null", b"[]", b"{\"files\":{}}"] {
        assert!(verify(Path::new("."), malformed).is_err());
    }
    assert_eq!(
        verify(Path::new("."), &vec![b' '; MAX_MANIFEST_BYTES + 1]).unwrap_err(),
        "Frozen manifest exceeds size cap"
    );
}
