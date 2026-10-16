// Keep the shared-Cargo-home registry lock through subprocess completion and cleanup.
#![allow(clippy::significant_drop_tightening)]

use std::fmt::Write as _;

use super::*;
use sha2::{Digest as _, Sha256};

static CARGO_PACKAGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Fixture {
    // Cargo's temporary registry is shared by invocations using the same Cargo home.
    _packaging: std::sync::MutexGuard<'static, ()>,
    directory: tempfile::TempDir,
    packages: BTreeMap<String, PublishPackageInfo>,
}

impl Fixture {
    fn new(specs: &[(&str, &str)]) -> Self {
        let packaging = CARGO_PACKAGING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("Cargo.toml"),
            "[workspace]\nresolver = \"2\"\n",
        )
        .unwrap();
        let mut packages = BTreeMap::new();
        for (name, dependencies) in specs {
            let path = directory.path().join(name);
            fs::create_dir_all(path.join("src")).unwrap();
            fs::write(path.join("src/lib.rs"), "pub fn hello() {}\n").unwrap();
            fs::write(path.join("Cargo.toml"), format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\ndescription = \"fixture\"\n{dependencies}"
            )).unwrap();
            let mut package = PublishPackageInfo::new(*name, "0.1.0", true, BTreeSet::new());
            package.package_path = Some(path);
            packages.insert((*name).into(), package);
        }
        Self {
            _packaging: packaging,
            directory,
            packages,
        }
    }

    fn stage(&self) -> SanitizedPackage {
        prepare_workspace(
            self.directory.path(),
            &self.packages,
            &self.packages.keys().cloned().collect::<Vec<_>>(),
        )
        .unwrap()
    }
}

fn output(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// A real, isolated Cargo registry, including downloadable .crate archives.
/// Tests never query crates.io and never configure a publish/upload endpoint.
struct Registry {
    directory: tempfile::TempDir,
}

impl Registry {
    fn new() -> Self {
        let registry = Self {
            directory: tempfile::tempdir().unwrap(),
        };
        let index = registry.directory.path().join("index");
        fs::create_dir_all(&index).unwrap();
        fs::write(index.join("config.json"), serde_json::json!({
            "dl": format!("file://{}/downloads/{{crate}}/{{version}}/download", registry.directory.path().display())
        }).to_string()).unwrap();
        output(Command::new("git").arg("init").arg(&index));
        registry
    }

    fn add(&self, name: &str, version: &str, dependency: Option<(&str, &str)>, yanked: bool) {
        let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let dependencies = dependency.map_or_else(String::new, |(name, version)| {
            format!("[dependencies]\n{name} = \"{version}\"\n")
        });
        for (file, contents) in [
            (
                "Cargo.toml",
                format!(
                    "[package]\nname=\"{name}\"\nversion=\"{version}\"\nedition=\"2021\"\n{dependencies}"
                ),
            ),
            ("src/lib.rs", "pub fn hello() {}".into()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(
                    &mut header,
                    format!("{name}-{version}/{file}"),
                    contents.as_bytes(),
                )
                .unwrap();
        }
        let bytes = archive.into_inner().unwrap().finish().unwrap();
        let download = self
            .directory
            .path()
            .join("downloads")
            .join(name)
            .join(version);
        fs::create_dir_all(&download).unwrap();
        fs::write(download.join("download"), &bytes).unwrap();
        let deps = dependency
            .into_iter()
            .map(|(name, req)| {
                serde_json::json!({
                    "name": name, "req": req, "features": [], "optional": false,
                    "default_features": true, "target": null, "kind": "normal", "registry": null
                })
            })
            .collect::<Vec<_>>();
        let entry = serde_json::json!({
            "name": name, "vers": version, "deps": deps,
            "cksum": Sha256::digest(&bytes).iter().fold(String::new(), |mut hex, byte| { write!(&mut hex, "{byte:02x}").unwrap(); hex }), "features": {}, "yanked": yanked
        });
        let index = self.directory.path().join("index");
        let file = index.join(&name[..2]).join(&name[2..4]).join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, format!("{entry}\n")).unwrap();
        output(Command::new("git").arg("-C").arg(&index).args(["add", "."]));
        output(Command::new("git").arg("-C").arg(&index).args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture",
        ]));
    }

    fn configure(&self, command: &mut Command) {
        command
            .env("CARGO_HOME", self.directory.path().join("home"))
            .arg("--config")
            .arg("source.crates-io.replace-with=\"fixture\"")
            .arg("--config")
            .arg(format!(
                "source.fixture.registry=\"file://{}/index\"",
                self.directory.path().display()
            ));
    }
}

#[test]
fn packages_unpublished_chain_using_cargo_native_staging() {
    let fixture = Fixture::new(&[
        ("clippier-fixture-core", ""),
        (
            "clippier-fixture-app",
            "[dependencies]\nrenamed = { package = \"clippier-fixture-core\", version = \"0.1.0\", optional = true }\n[target.'cfg(unix)'.build-dependencies]\nclippier-fixture-core = \"0.1.0\"\n",
        ),
    ]);
    let original = fs::read(
        fixture.packages["clippier-fixture-app"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.toml"),
    )
    .unwrap();
    let staged = fixture.stage();
    let root = staged.root.clone();
    let mut command = package_command(
        &staged,
        &PublishConfig {
            verify: true,
            ..PublishConfig::default()
        },
    );
    command.arg("--offline");
    output(&mut command);
    assert!(
        staged
            .target_dir()
            .join("package/clippier-fixture-app-0.1.0.crate")
            .exists()
    );
    let manifest = fs::read_to_string(
        staged
            .target_dir()
            .join("package/clippier-fixture-app-0.1.0/Cargo.toml"),
    )
    .unwrap();
    let manifest: toml::Value = toml::from_str(&manifest).unwrap();
    assert!(manifest["dependencies"]["renamed"].get("path").is_none());
    assert_eq!(
        fs::read(
            fixture.packages["clippier-fixture-app"]
                .package_path
                .as_ref()
                .unwrap()
                .join("Cargo.toml")
        )
        .unwrap(),
        original
    );
    drop(staged);
    assert!(!root.exists());
}

#[test]
fn fresh_packaging_rejects_yanked_transitive_dependency_despite_source_lock() {
    let fixture = Fixture::new(&[(
        "clippier-fixture-app",
        "[dependencies]\nclippier-fixture-bridge = \"1.0.0\"\n",
    )]);
    let registry = Registry::new();
    registry.add("clippier-fixture-leaf", "6.0.0", None, false);
    registry.add(
        "clippier-fixture-bridge",
        "1.0.0",
        Some(("clippier-fixture-leaf", "^6")),
        false,
    );
    // Generate a valid lock while the dependency is not yet yanked.
    let staged = fixture.stage();
    let mut command = package_command(&staged, &PublishConfig::default());
    registry.configure(&mut command);
    output(&mut command);
    let archive = fs::File::open(
        staged
            .target_dir()
            .join("package/clippier-fixture-app-0.1.0.crate"),
    )
    .unwrap();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut lock = Vec::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry.path().unwrap().ends_with("Cargo.lock") {
            std::io::Read::read_to_end(&mut entry, &mut lock).unwrap();
        }
    }
    assert_ne!(lock, [] as [u8; 0]);
    fs::write(fixture.directory.path().join("Cargo.lock"), &lock).unwrap();
    fs::write(
        fixture.packages["clippier-fixture-app"]
            .package_path
            .as_ref()
            .unwrap()
            .join("Cargo.lock"),
        &lock,
    )
    .unwrap();
    registry.add("clippier-fixture-leaf", "6.0.0", None, true);
    let fresh = fixture.stage();
    assert!(!fresh.root.join("Cargo.lock").exists());
    assert!(
        !fresh
            .root
            .join("source/clippier-fixture-app/Cargo.lock")
            .exists()
    );
    let mut command = package_command(&fresh, &PublishConfig::default());
    registry.configure(&mut command);
    let result = command.output().unwrap();
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("yanked"), "{stderr}");
    assert!(stderr.contains("clippier-fixture-bridge"), "{stderr}");
    assert_eq!(
        fs::read(fixture.directory.path().join("Cargo.lock")).unwrap(),
        lock
    );
}

#[test]
fn verification_is_opt_in_but_packaging_is_not() {
    let fixture = Fixture::new(&[("clippier-fixture-broken", "")]);
    fs::write(
        fixture.packages["clippier-fixture-broken"]
            .package_path
            .as_ref()
            .unwrap()
            .join("src/lib.rs"),
        "compile_error!(\"verification fixture\");",
    )
    .unwrap();
    let staged = fixture.stage();
    let mut command = package_command(&staged, &PublishConfig::default());
    command.arg("--offline");
    output(&mut command);
    let mut command = package_command(
        &staged,
        &PublishConfig {
            verify: true,
            ..PublishConfig::default()
        },
    );
    command.arg("--offline");
    let result = command.output().unwrap();
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("verification fixture"), "{stderr}");
}

#[test]
fn missing_registry_version_fails_and_dry_run_never_builds_upload_command() {
    let fixture = Fixture::new(&[(
        "clippier-fixture-app",
        "[dependencies]\nclippier-fixture-missing = \"99.0.0\"\n",
    )]);
    let registry = Registry::new();
    registry.add("clippier-fixture-missing", "1.0.0", None, false);
    for dry_run in [true, false] {
        let staged = fixture.stage();
        let mut command = package_command(
            &staged,
            &PublishConfig {
                dry_run,
                ..PublishConfig::default()
            },
        );
        registry.configure(&mut command);
        assert_eq!(command.get_args().next().unwrap(), "package");
        let result = command.output().unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("clippier-fixture-missing"));
    }
}

fn completed<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(result) => result,
        std::task::Poll::Pending => panic!("Unexpected upload/network await in preflight/dry-run"),
    }
}

#[test]
fn entire_queue_is_validated_before_any_publish_attempt() {
    let fixture = Fixture::new(&[
        ("clippier-fixture-first", ""),
        ("clippier-fixture-last", ""),
    ]);
    fs::write(
        fixture.packages["clippier-fixture-last"]
            .package_path
            .as_ref()
            .unwrap()
            .join("src/lib.rs"),
        "compile_error!(\"later package failed\");",
    )
    .unwrap();
    let pending = vec![
        "clippier-fixture-first".into(),
        "clippier-fixture-last".into(),
    ];
    for dry_run in [false, true] {
        let mut reports = Vec::new();
        let config = PublishConfig {
            verify: true,
            dry_run,
            color: crate::ColorMode::Never,
            ..PublishConfig::default()
        };
        let error = completed(super::super::publish_pending_queue(
            fixture.directory.path(),
            &fixture.packages,
            &pending,
            &config,
            &reqwest::Client::new(),
            &mut reports,
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("no packages uploaded"), "{error}");
        assert!(error.contains("later package failed"), "{error}");
        assert_eq!(reports, []);
    }
}

#[test]
fn successful_dry_run_stops_before_upload() {
    let fixture = Fixture::new(&[("clippier-fixture-first", "")]);
    let mut reports = Vec::new();
    completed(super::super::publish_pending_queue(
        fixture.directory.path(),
        &fixture.packages,
        &["clippier-fixture-first".into()],
        &PublishConfig {
            dry_run: true,
            ..PublishConfig::default()
        },
        &reqwest::Client::new(),
        &mut reports,
    ))
    .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].status,
        super::super::PublishPackageStatus::DryRun
    );
}

#[test]
fn root_package_and_nested_members_keep_relative_layout() {
    let fixture = Fixture::new(&[("clippier-fixture-nested", "")]);
    let root_manifest = fixture.directory.path().join("Cargo.toml");
    fs::write(&root_manifest, "[package]\nname=\"clippier-fixture-root\"\nversion=\"0.1.0\"\nedition=\"2021\"\nlicense=\"MIT\"\n[workspace]\nmembers=[\"clippier-fixture-nested\"]\n").unwrap();
    fs::create_dir(fixture.directory.path().join("src")).unwrap();
    fs::write(
        fixture.directory.path().join("src/lib.rs"),
        "pub fn hello() {}",
    )
    .unwrap();
    let mut packages = fixture.packages.clone();
    let mut root = PublishPackageInfo::new("clippier-fixture-root", "0.1.0", true, BTreeSet::new());
    root.package_path = Some(fixture.directory.path().to_path_buf());
    packages.insert(root.name.clone(), root);
    let staged = prepare_workspace(
        fixture.directory.path(),
        &packages,
        &packages.keys().cloned().collect::<Vec<_>>(),
    )
    .unwrap();
    let mut command = package_command(&staged, &PublishConfig::default());
    command.arg("--offline");
    output(&mut command);
    assert!(
        staged
            .root
            .join("source/clippier-fixture-nested/src/lib.rs")
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn nested_packages_with_symlinks_are_copied_once_in_either_order() {
    let mut fixture = Fixture::new(&[
        ("clippier-fixture-parent", ""),
        ("clippier-fixture-child", ""),
    ]);
    let parent = fixture.packages["clippier-fixture-parent"]
        .package_path
        .clone()
        .unwrap();
    let child = parent.join("nested");
    fs::rename(
        fixture.packages["clippier-fixture-child"]
            .package_path
            .as_ref()
            .unwrap(),
        &child,
    )
    .unwrap();
    fixture
        .packages
        .get_mut("clippier-fixture-child")
        .unwrap()
        .package_path = Some(child.clone());
    let manifest_path = child.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace("[package]", "[package]\nexclude = [\"dangling-link\"]"),
    )
    .unwrap();
    std::os::unix::fs::symlink("src/lib.rs", child.join("source-link")).unwrap();
    std::os::unix::fs::symlink("missing", child.join("dangling-link")).unwrap();
    let modules = child.join("node_modules");
    fs::create_dir(&modules).unwrap();
    std::os::unix::fs::symlink(".pnpm/missing", modules.join("dependency")).unwrap();
    for pending in [
        vec![
            "clippier-fixture-parent".into(),
            "clippier-fixture-child".into(),
        ],
        vec![
            "clippier-fixture-child".into(),
            "clippier-fixture-parent".into(),
        ],
    ] {
        let staged =
            prepare_workspace(fixture.directory.path(), &fixture.packages, &pending).unwrap();
        let copied = staged.root.join("source/clippier-fixture-parent/nested");
        assert_eq!(
            fs::read_link(copied.join("source-link")).unwrap(),
            Path::new("src/lib.rs")
        );
        assert_eq!(
            fs::read_link(copied.join("dangling-link")).unwrap(),
            Path::new("missing")
        );
        assert!(!copied.join("node_modules").exists());
        let mut command = package_command(&staged, &PublishConfig::default());
        command.arg("--offline");
        output(&mut command);
    }
    assert!(modules.join("dependency").is_symlink());
    assert_eq!(
        fs::read_link(child.join("dangling-link")).unwrap(),
        Path::new("missing")
    );
}

#[cfg(unix)]
#[test]
fn copy_errors_identify_conflicting_symlink_paths() {
    let fixture = Fixture::new(&[("clippier-fixture-app", "")]);
    let source = fixture.directory.path().join("copy-source");
    let destination = fixture.directory.path().join("copy-destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    std::os::unix::fs::symlink("missing", source.join("link")).unwrap();
    fs::write(destination.join("link"), "existing file").unwrap();
    let error = copy_dir(&source, &destination).unwrap_err().to_string();
    assert!(
        error.contains(&source.join("link").display().to_string()),
        "{error}"
    );
    assert!(
        error.contains(&destination.join("link").display().to_string()),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(destination.join("link")).unwrap(),
        "existing file"
    );
}

#[test]
fn pending_links_do_not_override_older_registry_requirements() {
    let fixture = Fixture::new(&[
        ("clippier-fixture-core", ""),
        (
            "clippier-fixture-app",
            "[dependencies]\nclippier-fixture-core = \"=0.0.1\"\n",
        ),
    ]);
    let staged = fixture.stage();
    let manifest = read_toml(&staged.root.join("source/clippier-fixture-app/Cargo.toml")).unwrap();
    assert!(
        manifest["dependencies"]["clippier-fixture-core"]
            .as_str()
            .is_some()
    );
}

#[test]
fn reports_distinguish_packaging_from_compilation() {
    let mut package = super::super::PublishPackageReport::new(
        "fixture",
        "0.1.0",
        super::super::PublishPackageStatus::DryRun,
    );
    package.packaging_validated = true;
    let report = super::super::PublishReport::new(vec![package.clone()]);
    let raw = report.to_raw_string(crate::ColorMode::Never);
    assert!(raw.contains("validated (not uploaded)"));
    assert!(raw.contains("Compilation verification: not requested"));
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["packages"][0]["packaging_validated"], true);
    assert_eq!(json["packages"][0]["compilation_verified"], false);
    package.compilation_verified = true;
    assert!(
        super::super::PublishReport::new(vec![package])
            .to_raw_string(crate::ColorMode::Never)
            .contains("Compilation verification: passed")
    );
}

#[test]
fn failed_preparation_cleans_staging_without_modifying_source() {
    let fixture = Fixture::new(&[("clippier-fixture-app", "")]);
    let path = fixture.packages["clippier-fixture-app"]
        .package_path
        .as_ref()
        .unwrap()
        .join("Cargo.toml");
    let manifest =
        "[package]\nname=\"clippier-fixture-app\"\nversion=\"0.1.0\"\nlicense.workspace=true\n";
    fs::write(&path, manifest).unwrap();
    let prefix = format!("clippier-preflight-{}-", std::process::id());
    let roots = || {
        fs::read_dir(std::env::temp_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().starts_with(&prefix))
            .collect::<BTreeSet<_>>()
    };
    let before = roots();
    assert!(
        prepare_workspace(
            fixture.directory.path(),
            &fixture.packages,
            &["clippier-fixture-app".into()]
        )
        .is_err()
    );
    assert_eq!(roots(), before);
    assert_eq!(fs::read_to_string(path).unwrap(), manifest);
}

#[test]
fn collection_continues_after_independent_failures_and_labels_blocked_checks() {
    let mut fixture = Fixture::new(&[
        ("first", ""),
        ("second", ""),
        ("dependent", ""),
        ("healthy", ""),
    ]);
    fixture
        .packages
        .get_mut("dependent")
        .unwrap()
        .publish_dependencies
        .insert("first".into());
    let pending = ["first", "second", "dependent", "healthy"].map(str::to_string);
    let mut calls = Vec::new();
    let error = collect_package_checks(
        &fixture.packages,
        &pending,
        &PublishConfig::default(),
        vec!["manifest issue".into()],
        |closure| {
            calls.push(closure.to_vec());
            if closure.contains(&"first".to_string()) {
                Err("zip ^6 is yanked".into())
            } else if closure.contains(&"second".to_string()) {
                Err("zip ^3 is yanked".into())
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[2], ["dependent", "first"]);
    for expected in [
        "manifest issue",
        "zip ^6",
        "zip ^3",
        "potentially blocked by previously failed pending dependencies: first",
        "Packaging validated: 1",
        "not checked: 0",
        "no packages uploaded",
    ] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn fail_fast_leaves_remaining_checks_explicitly_unchecked() {
    let fixture = Fixture::new(&[("first", ""), ("second", "")]);
    let mut calls = 0;
    let error = collect_package_checks(
        &fixture.packages,
        &["first".into(), "second".into()],
        &PublishConfig {
            fail_fast: true,
            ..PublishConfig::default()
        },
        Vec::new(),
        |_| {
            calls += 1;
            Err("first failure".into())
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(calls, 1);
    assert!(error.contains("Stopped early (--fail-fast)"), "{error}");
    assert!(error.contains("not checked: 1"), "{error}");
}

#[test]
fn real_cargo_collects_two_distinct_yanked_dependency_failures() {
    let fixture = Fixture::new(&[
        (
            "clippier-fixture-first",
            "[dependencies]\nclippier-fixture-oldzip = \"6\"\n",
        ),
        (
            "clippier-fixture-second",
            "[dependencies]\nclippier-fixture-otherzip = \"3\"\n",
        ),
        ("clippier-fixture-healthy", ""),
    ]);
    let registry = Registry::new();
    registry.add("clippier-fixture-oldzip", "6.0.0", None, true);
    registry.add("clippier-fixture-otherzip", "3.0.0", None, true);
    let pending = [
        "clippier-fixture-first",
        "clippier-fixture-second",
        "clippier-fixture-healthy",
    ]
    .map(str::to_string);
    let config = PublishConfig::default();
    let error = collect_package_checks(
        &fixture.packages,
        &pending,
        &config,
        Vec::new(),
        |closure| {
            let staged = prepare_workspace(fixture.directory.path(), &fixture.packages, closure)?;
            let mut command = package_command(&staged, &config);
            registry.configure(&mut command);
            let result = command.output()?;
            if result.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&result.stderr).into_owned().into())
            }
        },
    )
    .unwrap_err()
    .to_string();
    for expected in [
        "clippier-fixture-oldzip",
        "clippier-fixture-otherzip",
        "6.0.0 is yanked",
        "3.0.0 is yanked",
        "Packaging validated: 1",
        "not checked: 0",
    ] {
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn source_overrides_are_rejected_instead_of_producing_false_success() {
    let fixture = Fixture::new(&[("clippier-fixture-app", "")]);
    fs::create_dir(fixture.directory.path().join(".cargo")).unwrap();
    fs::write(
        fixture.directory.path().join(".cargo/config.toml"),
        "[source.crates-io]\nreplace-with = \"vendored\"\n",
    )
    .unwrap();
    let error = validate(
        fixture.directory.path(),
        &fixture.packages,
        &["clippier-fixture-app".into()],
        &PublishConfig::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("source overrides"), "{error}");
    assert!(error.contains("no packages uploaded"), "{error}");
}
