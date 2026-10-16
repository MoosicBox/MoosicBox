//! Loopback-only HTTP transport tests, including real sanitized Cargo packaging.

// Keep fixture resources alive until all HTTP/Cargo work and assertions finish.
#![allow(clippy::significant_drop_tightening)]

use std::{
    io::Write as _,
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

use super::super::*;

/// One bounded HTTP exchange. Even a test that never connects cannot strand a
/// listener thread; Drop waits only for its bounded accept/read/write deadlines.
struct HttpArtifact {
    url: String,
    server: Option<thread::JoinHandle<()>>,
}

impl HttpArtifact {
    fn raw(response: Vec<u8>) -> Self {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!(
            "http://{}/crates/a/0.5.0/download",
            listener.local_addr().unwrap()
        );
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("loopback accept: {error}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            let request_deadline = Instant::now() + Duration::from_secs(5);
            while !request.ends_with(b"\r\n\r\n") {
                if Instant::now() >= request_deadline {
                    return;
                }
                if socket.read(&mut byte).unwrap_or(0) == 0 {
                    return;
                }
                request.push(byte[0]);
                assert!(request.len() <= 8192, "unbounded HTTP request");
            }
            assert!(request.starts_with(b"GET /crates/a/0.5.0/download HTTP/1.1\r\n"));
            // A size/status rejection may close the connection before the body.
            let _ = socket.write_all(&response);
        });
        Self {
            url,
            server: Some(server),
        }
    }

    fn artifact(bytes: &[u8]) -> Self {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
        .into_bytes();
        response.extend_from_slice(bytes);
        Self::raw(response)
    }
}

impl Drop for HttpArtifact {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            let result = server.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}

fn client() -> crate::publish::RegistryClient {
    // Do not let ambient proxy configuration route loopback tests externally.
    crate::publish::RegistryClient::for_test(0)
}

#[switchy_async::test]
async fn http_status_errors_and_declared_oversize_are_rejected_before_creating_artifact() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("published.crate");
    for status in ["404 Not Found", "500 Internal Server Error"] {
        let server = HttpArtifact::raw(
            format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes(),
        );
        let error = download_artifact(&client(), &server.url, &destination, 16)
            .await
            .unwrap_err();
        assert!(error.to_string().contains(&status[..3]), "{error}");
        assert!(!destination.exists());
    }
    let server = HttpArtifact::raw(
        b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\nConnection: close\r\n\r\n".to_vec(),
    );
    let error = download_artifact(&client(), &server.url, &destination, 16)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("download limit"), "{error}");
    assert!(!destination.exists());
}

#[switchy_async::test]
async fn http_stream_limits_truncation_and_malformed_framing_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("published.crate");
    // Chunked and EOF-delimited responses have no declared length; the actual
    // streamed byte counter must still enforce the bound.
    for response in [
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n11\r\n12345678901234567\r\n0\r\n\r\n".to_vec(),
        b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n12345678901234567".to_vec(),
    ] {
        let server = HttpArtifact::raw(response);
        let error = download_artifact(&client(), &server.url, &destination, 16).await.unwrap_err();
        assert!(error.to_string().contains("download limit"), "{error}");
        assert!(fs::metadata(&destination).unwrap().len() <= 16);
    }
    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 16\r\nConnection: close\r\n\r\nshort".to_vec(),
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nnot-a-chunk-size\r\n".to_vec(),
    ] {
        let server = HttpArtifact::raw(response);
        assert!(download_artifact(&client(), &server.url, &destination, 16).await.is_err());
    }
}

#[switchy_async::test]
async fn http_exact_bound_success_and_invalid_or_truncated_archives() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("published.crate");
    let archive = super::archive(&[(
        "a-0.5.0/Cargo.toml",
        b"[package]\nname='a'\nversion='0.5.0'\n",
    )]);
    let bytes = fs::read(archive.path()).unwrap();
    let server = HttpArtifact::artifact(&bytes);
    download_artifact(
        &client(),
        &server.url,
        &destination,
        u64::try_from(bytes.len()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(fs::read(&destination).unwrap(), bytes);
    assert_eq!(
        normalized_archive_entries(&destination, ManifestVersionPolicy::Preserve).unwrap(),
        normalized_archive_entries(archive.path(), ManifestVersionPolicy::Preserve).unwrap()
    );
    // HTTP can complete successfully while the artifact itself is invalid.
    for invalid in [
        b"not gzip or tar".to_vec(),
        bytes[..bytes.len() / 2].to_vec(),
        bytes[..bytes.len() - 4].to_vec(),
    ] {
        let server = HttpArtifact::artifact(&invalid);
        download_artifact(&client(), &server.url, &destination, 4096)
            .await
            .unwrap();
        assert!(normalized_archive_entries(&destination, ManifestVersionPolicy::Preserve).is_err());
    }
}

#[switchy_async::test]
#[allow(clippy::too_many_lines)] // One sequential published/local artifact lifecycle.
async fn http_artifact_comparison_packages_sanitized_local_contents_and_ignores_excluded_edits() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['a']\nresolver='2'\n",
    )
    .unwrap();
    fs::create_dir(root.join(".cargo")).unwrap();
    // All Cargo invocations inherit offline mode through the copied config.
    fs::write(root.join(".cargo/config.toml"), "[net]\noffline=true\n").unwrap();
    let source = root.join("a");
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(source.join("Cargo.toml"), "[package]\nname='a'\nversion='0.5.0'\nedition='2021'\nlicense='MIT'\ndescription='fixture'\nexclude=['ignored.txt']\n").unwrap();
    fs::write(source.join("src/lib.rs"), "pub fn original() {}\n").unwrap();
    fs::write(source.join("ignored.txt"), "excluded original\n").unwrap();
    let mut package = PublishPackageInfo::new("a", "0.5.0", true, BTreeSet::new());
    package.package_path = Some(source.clone());
    let packages = BTreeMap::from([("a".into(), package.clone())]);
    let config = PublishConfig::default();
    let staged = SanitizedPackage::prepare(root, &packages, &package).unwrap();
    let output = Command::new("cargo")
        .args([
            "package",
            "--offline",
            "--no-verify",
            "--allow-dirty",
            "--registry",
            "crates-io",
            "--manifest-path",
        ])
        .arg(staged.manifest_path())
        .current_dir(staged.root())
        .env("CARGO_TARGET_DIR", staged.target_dir())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = fs::read(staged.target_dir().join("package/a-0.5.0.crate")).unwrap();
    let client = client();
    for (edit, expected) in [
        (None, PurityStatus::Pure),
        (
            Some(("ignored.txt", "excluded changed\n")),
            PurityStatus::Pure,
        ),
        (
            Some(("src/lib.rs", "pub fn edited() {}\n")),
            PurityStatus::Impure,
        ),
    ] {
        if let Some((file, contents)) = edit {
            fs::write(source.join(file), contents).unwrap();
        }
        let server = HttpArtifact::artifact(&bytes);
        let (local, published) =
            artifact_comparison(root, &packages, &package, &config, &client, &server.url)
                .await
                .unwrap();
        let assessment = compare(&package, &local, &published, vec!["pending-b".into()]);
        assert_eq!(assessment.status, expected, "{assessment:?}");
        assert!(!local.contains_key("ignored.txt"));
        if expected == PurityStatus::Impure {
            assert_eq!(assessment.changed, ["src/lib.rs"]);
        }
    }
    let server = HttpArtifact::artifact(b"malformed artifact");
    assert!(
        artifact_comparison(root, &packages, &package, &config, &client, &server.url)
            .await
            .is_err()
    );
}
