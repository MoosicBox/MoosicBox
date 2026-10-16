//! Public-Cargo proof: one packaging invocation shares an unpublished-dependency
//! overlay, but each archive gets its own resolution and verification workspace.
//! Cargo 1.90 stabilized multi-package publishing; these tests require that API.
//! The standalone proofs use no source replacement, patches, inherited lockfile,
//! or network. Production guard tests additionally use the existing isolated
//! Registry fixture (its command-local replacement never touches source config).

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    io::Read as _,
    path::PathBuf,
    process::{Command, Output},
};

struct CargoProof {
    directory: tempfile::TempDir,
}

impl CargoProof {
    fn new(edges: &[(&str, &[&str])]) -> Self {
        let proof = Self {
            directory: tempfile::tempdir().unwrap(),
        };
        let members = edges
            .iter()
            .map(|(name, _)| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(",");
        proof.write(
            "Cargo.toml",
            &format!("[workspace]\nresolver=\"2\"\nmembers=[{members}]\n"),
        );
        for (name, dependencies) in edges {
            let mut dependency_manifest = String::new();
            for dependency in *dependencies {
                writeln!(
                    dependency_manifest,
                    "clippier_reuse_{dependency}={{version=\"=0.1.0\",path=\"../{dependency}\"}}"
                )
                .unwrap();
            }
            proof.write(
                &format!("{name}/Cargo.toml"),
                &format!(
                    "[package]\nname=\"clippier_reuse_{name}\"\nversion=\"0.1.0\"\nedition=\"2021\"\nlicense=\"MIT\"\ndescription=\"packaging proof\"\n[dependencies]\n{dependency_manifest}"
                ),
            );
            proof.write(
                &format!("{name}/src/lib.rs"),
                "pub fn value() -> u8 { 1 }\n",
            );
        }
        proof
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.directory.path().join(relative)
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn replace(&self, relative: &str, from: &str, to: &str) {
        let contents = fs::read_to_string(self.path(relative)).unwrap();
        assert!(contents.contains(from));
        self.write(relative, &contents.replace(from, to));
    }

    fn cargo(&self, arguments: &[&str]) -> Output {
        Command::new("cargo")
            .args(arguments)
            .args(["--offline", "--color", "never"])
            .current_dir(self.directory.path())
            // An empty home also isolates Cargo's temporary packaging registry.
            .env("CARGO_HOME", self.path("cargo-home"))
            .env("CARGO_TARGET_DIR", self.path("target"))
            .env("CARGO_BUILD_BUILD_DIR", self.path("build"))
            .output()
            .unwrap()
    }

    fn package(&self, arguments: &[&str]) -> Output {
        let mut args = vec!["package", "--allow-dirty", "--registry", "crates-io"];
        args.extend_from_slice(arguments);
        self.cargo(&args)
    }

    fn archive_file(&self, name: &str, relative: &str) -> String {
        let file =
            fs::File::open(self.path(&format!("target/package/clippier_reuse_{name}-0.1.0.crate")))
                .unwrap();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap()
                == PathBuf::from(format!("clippier_reuse_{name}-0.1.0/{relative}"))
            {
                let mut text = String::new();
                entry.read_to_string(&mut text).unwrap();
                return text;
            }
        }
        panic!("missing {relative} in archive for {name}");
    }

    fn assert_lock(&self, name: &str, expected: &[&str]) {
        let lock: toml::Value = toml::from_str(&self.archive_file(name, "Cargo.lock")).unwrap();
        let packages = lock["package"].as_array().unwrap();
        let actual = packages
            .iter()
            .map(|package| package["name"].as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        let expected = expected
            .iter()
            .map(|name| format!("clippier_reuse_{name}"))
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);
        for package in packages {
            if package["name"].as_str().unwrap() != format!("clippier_reuse_{name}") {
                assert_eq!(
                    package["source"].as_str(),
                    Some("registry+https://github.com/rust-lang/crates.io-index")
                );
                assert!(package["checksum"].as_str().is_some());
            }
        }
        let manifest: toml::Value = toml::from_str(&self.archive_file(name, "Cargo.toml")).unwrap();
        if let Some(dependencies) = manifest.get("dependencies").and_then(toml::Value::as_table) {
            for dependency in dependencies.values() {
                assert!(dependency.get("path").is_none());
            }
        }
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn assert_result(output: &Output, success: bool, packaging: usize, verifying: usize) {
    let text = stderr(output);
    assert_eq!(output.status.success(), success, "{text}");
    for (label, expected) in [("Packaging ", packaging), ("Verifying ", verifying)] {
        let actual = text
            .lines()
            .filter(|line| line.trim_start().starts_with(label))
            .count();
        assert_eq!(actual, expected, "{label}: {text}");
    }
}

#[test]
fn chain_and_diamond_package_each_node_once_with_independent_locks() {
    for (edges, counts) in [
        (vec![("a", vec![]), ("b", vec!["a"]), ("c", vec!["b"])], 3),
        (
            vec![
                ("a", vec![]),
                ("b", vec!["a"]),
                ("c", vec!["a"]),
                ("d", vec!["b", "c"]),
            ],
            4,
        ),
    ] {
        let edges = edges
            .iter()
            .map(|(n, d)| (*n, d.as_slice()))
            .collect::<Vec<_>>();
        let proof = CargoProof::new(&edges);
        assert_result(&proof.package(&["--workspace"]), true, counts, counts);
        assert!(!proof.path("Cargo.lock").exists());
        proof.assert_lock("a", &["a"]);
        proof.assert_lock("b", &["a", "b"]);
        if counts == 3 {
            proof.assert_lock("c", &["a", "b", "c"]);
        } else {
            proof.assert_lock("c", &["a", "c"]);
            proof.assert_lock("d", &["a", "b", "c", "d"]);
        }
        // Retaining archives does not turn another workspace invocation into a cache hit.
        assert_result(
            &proof.package(&["--workspace", "--no-verify"]),
            true,
            counts,
            0,
        );
    }
}

#[test]
fn package_selection_neither_implicitly_packages_nor_reuses_dependencies() {
    let proof = CargoProof::new(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
    assert_result(&proof.package(&["--workspace", "--no-verify"]), true, 3, 0);
    assert!(
        proof
            .path("target/package/clippier_reuse_b-0.1.0.crate")
            .exists()
    );
    let selected = proof.package(&["-p", "clippier_reuse_c", "--no-verify"]);
    assert_result(&selected, false, 1, 0);
    assert!(stderr(&selected).contains("no matching package named `clippier_reuse_b`"));
    let selected = proof.package(&[
        "-p",
        "clippier_reuse_b",
        "-p",
        "clippier_reuse_c",
        "--no-verify",
    ]);
    assert_result(&selected, false, 1, 0);
    assert!(stderr(&selected).contains("no matching package named `clippier_reuse_a`"));
}

#[test]
fn verification_does_not_unify_sibling_dependency_features() {
    let proof = CargoProof::new(&[("a", &[]), ("b", &["a"]), ("c", &["a"])]);
    proof.write(
        "a/src/lib.rs",
        "#[cfg(all(feature=\"red\",feature=\"blue\"))]\ncompile_error!(\"unified sibling features\");\npub fn value() {}\n",
    );
    proof.replace(
        "a/Cargo.toml",
        "[dependencies]",
        "[features]\nred=[]\nblue=[]\n[dependencies]",
    );
    for (name, feature) in [("b", "red"), ("c", "blue")] {
        proof.replace(
            &format!("{name}/Cargo.toml"),
            "path=\"../a\"",
            &format!("path=\"../a\",features=[\"{feature}\"]"),
        );
    }
    assert_result(&proof.package(&["--workspace"]), true, 3, 3);
    assert!(!proof.path("Cargo.lock").exists());
    let global = proof.cargo(&["build", "--workspace"]);
    assert!(!global.status.success());
    assert!(stderr(&global).contains("unified sibling features"));
}

#[test]
fn archive_resolution_does_not_inherit_a_global_workspace_graph() {
    let proof = CargoProof::new(&[("a", &[]), ("b", &[]), ("c", &["a"]), ("d", &["b"])]);
    for name in ["a", "b"] {
        proof.replace(
            &format!("{name}/Cargo.toml"),
            "[dependencies]",
            "links=\"clippier_reuse_native\"\n[dependencies]",
        );
        proof.write(&format!("{name}/build.rs"), "fn main() {}\n");
    }
    assert_result(&proof.package(&["--workspace"]), true, 4, 4);
    proof.assert_lock("c", &["a", "c"]);
    proof.assert_lock("d", &["b", "d"]);
    assert!(!proof.path("Cargo.lock").exists());
    let global = proof.cargo(&["generate-lockfile"]);
    assert!(!global.status.success());
    assert!(stderr(&global).contains("links"));
}

#[test]
fn no_verify_still_resolves_but_excluding_lockfile_can_report_false_success() {
    let proof = CargoProof::new(&[("a", &[]), ("b", &["a"])]);
    let selected = [
        "-p",
        "clippier_reuse_b",
        "--exclude-lockfile",
        "--no-verify",
    ];
    assert_result(&proof.package(&selected), true, 1, 0);
    assert_result(
        &proof.package(&["-p", "clippier_reuse_b", "--exclude-lockfile"]),
        false,
        1,
        1,
    );
    proof.replace(
        "b/Cargo.toml",
        "path=\"../a\"",
        "path=\"../a\",features=[\"missing\"]",
    );
    assert_result(&proof.package(&["--workspace", "--no-verify"]), false, 2, 0);
}

#[test]
fn verification_builds_archive_contents_not_staged_paths() {
    let proof = CargoProof::new(&[("a", &[]), ("b", &["a"])]);
    proof.replace(
        "b/Cargo.toml",
        "[dependencies]",
        "exclude=[\"src/implementation.rs\"]\n[dependencies]",
    );
    proof.write("b/src/lib.rs", "include!(\"implementation.rs\");\n");
    proof.write("b/src/implementation.rs", "pub fn value() {}\n");
    assert_result(&proof.package(&["--workspace", "--no-verify"]), true, 2, 0);
    let verified = proof.package(&["--workspace"]);
    assert_result(&verified, false, 2, 2);
    assert!(stderr(&verified).contains("implementation.rs"));
}

/// These tests intentionally exercise the product staging/command/guard path,
/// rather than the independent `CargoProof` implementation above.
mod production {
    // Keep the shared-Cargo-home fixture lock through subprocess completion and cleanup.
    #![allow(clippy::significant_drop_tightening)]

    use super::super::{
        PublishConfig, SanitizedPackage, batch, package_command as production_package_command,
        pending_dependencies, prepare_workspace,
        tests::{Fixture, Registry},
    };
    use super::{assert_result, stderr};
    use sha2::{Digest as _, Sha256};
    use std::{
        collections::BTreeSet,
        fmt::Write as _,
        fs,
        io::Read as _,
        time::{Duration, Instant},
    };

    fn package_command(staged: &SanitizedPackage, config: &PublishConfig) -> std::process::Command {
        let mut config = config.clone();
        // Cargo colorizes status labels separately, inserting an ANSI reset
        // between `Packaging` and its separator. Counters require plain text.
        config.color = crate::ColorMode::Never;
        production_package_command(staged, &config)
    }

    fn pending(fixture: &Fixture) -> Vec<String> {
        fixture.packages.keys().cloned().collect()
    }

    fn stage(fixture: &Fixture, names: &[String]) -> SanitizedPackage {
        prepare_workspace(fixture.directory.path(), &fixture.packages, names).unwrap()
    }

    fn archive_lock(staged: &SanitizedPackage, name: &str) -> toml::Value {
        let file = fs::File::open(
            staged
                .target_dir()
                .join(format!("package/{name}-0.1.0.crate")),
        )
        .unwrap();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == std::path::PathBuf::from(format!("{name}-0.1.0/Cargo.lock"))
            {
                let mut text = String::new();
                entry.read_to_string(&mut text).unwrap();
                return toml::from_str(&text).unwrap();
            }
        }
        panic!("missing archive lock for {name}");
    }

    fn archive_graph(staged: &SanitizedPackage, name: &str) -> toml::Value {
        let mut lock = archive_lock(staged, name);
        for package in lock["package"].as_array_mut().unwrap() {
            // Cargo.toml.orig preserves fresh absolute staging paths, so the
            // unpublished archives' checksums need not match across stagings.
            package.as_table_mut().unwrap().remove("checksum");
        }
        lock
    }

    fn assert_no_staged_locks(staged: &SanitizedPackage) {
        let mut directories = vec![staged.root().to_path_buf()];
        while let Some(directory) = directories.pop() {
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                assert_ne!(
                    entry.file_name(),
                    "Cargo.lock",
                    "inherited staged lock: {}",
                    entry.path().display()
                );
                if entry.file_type().unwrap().is_dir() {
                    directories.push(entry.path());
                }
            }
        }
    }

    #[test]
    fn production_batch_guard_accepts_fresh_archives_without_inheriting_source_locks() {
        let mut fixture = Fixture::new(&[
            ("clippier-reuse-core", ""),
            (
                "clippier-reuse-app",
                "[dependencies]\nclippier-reuse-core = \"=0.1.0\"\n",
            ),
        ]);
        fixture
            .packages
            .get_mut("clippier-reuse-app")
            .unwrap()
            .publish_dependencies
            .insert("clippier-reuse-core".into());
        // Invalid poison locks ensure even accidentally loading one is observable.
        let poison = "not a Cargo lockfile\n";
        fs::write(fixture.directory.path().join("Cargo.lock"), poison).unwrap();
        for package in fixture.packages.values() {
            fs::write(
                package.package_path.as_ref().unwrap().join("Cargo.lock"),
                poison,
            )
            .unwrap();
        }
        let names = pending(&fixture);
        let staged = stage(&fixture, &names);
        assert_no_staged_locks(&staged);
        let output = package_command(
            &staged,
            &PublishConfig {
                verify: false,
                ..PublishConfig::default()
            },
        )
        .arg("--offline")
        .env("CARGO_HOME", fixture.directory.path().join("cargo-home"))
        .output()
        .unwrap();
        assert_result(&output, true, 2, 0);
        batch::check_archives(&staged, &fixture.packages, &names).unwrap();
        assert!(!staged.root().join("Cargo.lock").exists());
        for path in std::iter::once(fixture.directory.path().to_path_buf()).chain(
            fixture
                .packages
                .values()
                .map(|package| package.package_path.clone().unwrap()),
        ) {
            assert_eq!(fs::read_to_string(path.join("Cargo.lock")).unwrap(), poison);
        }
        // Also cover the production batch entry point, not only its component calls.
        batch::validate(
            fixture.directory.path(),
            &fixture.packages,
            &names,
            &PublishConfig {
                verify: false,
                ..PublishConfig::default()
            },
        )
        .unwrap();
    }

    #[test]
    #[allow(clippy::too_many_lines)] // Keep batch and independent registry evidence together.
    fn production_guard_rejects_actual_external_bridge_to_an_outside_pending_name() {
        let fixture = Fixture::new(&[
            (
                "clippier-reuse-app",
                "[dependencies]\nclippier-reuse-bridge = \"=0.1.0\"\n",
            ),
            ("clippier-reuse-outside", ""),
        ]);
        let registry = Registry::new();
        // Independent workspace nodes have no guaranteed packaging order. A
        // published same-version candidate keeps both orders resolvable; the
        // checksum below records whether Cargo actually chose the overlay.
        registry.add("clippier-reuse-outside", "0.1.0", None, false);
        registry.add(
            "clippier-reuse-bridge",
            "0.1.0",
            Some(("clippier-reuse-outside", "=0.1.0")),
            false,
        );
        let names = pending(&fixture);
        let staged = stage(&fixture, &names);
        let mut command = package_command(
            &staged,
            &PublishConfig {
                verify: false,
                ..PublishConfig::default()
            },
        );
        registry.configure(&mut command);
        let output = command.output().unwrap();
        assert_result(&output, true, 2, 0);
        let lock = archive_lock(&staged, "clippier-reuse-app");
        let outside = lock["package"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"].as_str() == Some("clippier-reuse-outside"))
            .unwrap();
        assert_eq!(outside["version"].as_str(), Some("0.1.0"));
        let selected_checksum = outside["checksum"].as_str().unwrap();
        let overlay_checksum = Sha256::digest(
            fs::read(
                staged
                    .target_dir()
                    .join("package/clippier-reuse-outside-0.1.0.crate"),
            )
            .unwrap(),
        )
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(&mut hex, "{byte:02x}").unwrap();
            hex
        });
        let registry_checksum = Sha256::digest(
            fs::read(
                registry
                    .directory
                    .path()
                    .join("downloads/clippier-reuse-outside/0.1.0/download"),
            )
            .unwrap(),
        )
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(&mut hex, "{byte:02x}").unwrap();
            hex
        });
        assert!(selected_checksum == overlay_checksum || selected_checksum == registry_checksum);
        eprintln!(
            "external bridge selected outside pending name: version=0.1.0 overlay={} registry={}",
            selected_checksum == overlay_checksum,
            selected_checksum == registry_checksum
        );
        let error = batch::check_archives(&staged, &fixture.packages, &names)
            .unwrap_err()
            .to_string();
        assert!(error.contains("clippier-reuse-app"), "{error}");
        assert!(
            error.contains("clippier-reuse-outside") && error.contains("outside"),
            "{error}"
        );
        // Independent packaging is valid too, but can only choose the published
        // candidate. The guard conservatively rejects either batch selection.
        let independent = stage(&fixture, &["clippier-reuse-app".into()]);
        let mut command = package_command(
            &independent,
            &PublishConfig {
                verify: false,
                ..PublishConfig::default()
            },
        );
        registry.configure(&mut command);
        let output = command.output().unwrap();
        assert_result(&output, true, 1, 0);
        let independent_lock = archive_lock(&independent, "clippier-reuse-app");
        let independent_outside = independent_lock["package"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"].as_str() == Some("clippier-reuse-outside"))
            .unwrap();
        assert_eq!(
            independent_outside["checksum"].as_str(),
            Some(registry_checksum.as_str())
        );
    }

    #[test]
    fn production_guard_conservatively_rejects_older_registry_version_of_pending_name() {
        let fixture = Fixture::new(&[
            (
                "clippier-reuse-app",
                "[dependencies]\nclippier-reuse-bridge = \"=0.1.0\"\n",
            ),
            ("clippier-reuse-outside", ""),
        ]);
        let registry = Registry::new();
        registry.add("clippier-reuse-outside", "0.0.1", None, false);
        registry.add(
            "clippier-reuse-bridge",
            "0.1.0",
            Some(("clippier-reuse-outside", "=0.0.1")),
            false,
        );
        let names = pending(&fixture);
        let staged = stage(&fixture, &names);
        let mut command = package_command(
            &staged,
            &PublishConfig {
                verify: false,
                ..PublishConfig::default()
            },
        );
        registry.configure(&mut command);
        let output = command.output().unwrap();
        assert_result(&output, true, 2, 0);
        let lock = archive_lock(&staged, "clippier-reuse-app");
        let outside = lock["package"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| package["name"].as_str() == Some("clippier-reuse-outside"))
            .unwrap();
        assert_eq!(outside["version"].as_str(), Some("0.0.1"));
        let error = batch::check_archives(&staged, &fixture.packages, &names)
            .unwrap_err()
            .to_string();
        assert!(error.contains("outside"), "{error}");
        // This is intentionally conservative: the original independent graph is valid.
        let independent = stage(&fixture, &["clippier-reuse-app".into()]);
        let mut command = package_command(
            &independent,
            &PublishConfig {
                verify: false,
                ..PublishConfig::default()
            },
        );
        registry.configure(&mut command);
        let output = command.output().unwrap();
        assert_result(&output, true, 1, 0);
        batch::check_archives(
            &independent,
            &fixture.packages,
            &["clippier-reuse-app".into()],
        )
        .unwrap();
    }

    #[test]
    fn production_guard_requires_an_archive_lock_even_when_cargo_reports_success() {
        let fixture = Fixture::new(&[("clippier-reuse-root", "")]);
        let names = pending(&fixture);
        let staged = stage(&fixture, &names);
        let output = package_command(
            &staged,
            &PublishConfig {
                verify: false,
                ..PublishConfig::default()
            },
        )
        .args(["--offline", "--exclude-lockfile"])
        .env("CARGO_HOME", fixture.directory.path().join("cargo-home"))
        .output()
        .unwrap();
        assert_result(&output, true, 1, 0);
        let error = batch::check_archives(&staged, &fixture.packages, &names)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Missing packaged lockfile"), "{error}");
    }

    /// Fixture timings include fresh staging and packaging but exclude fixture
    /// construction. They are diagnostics, not real-world speed assertions.
    #[test]
    #[allow(clippy::too_many_lines)] // Show both measured production paths in one comparison.
    fn production_chain_and_diamond_batch_vs_repeated_closure_measurements() {
        for diamond in [false, true] {
            let mut specs = vec![
                ("clippier-reuse-a", ""),
                (
                    "clippier-reuse-b",
                    "[dependencies]\nclippier-reuse-a = \"=0.1.0\"\n",
                ),
                (
                    "clippier-reuse-c",
                    if diamond {
                        "[dependencies]\nclippier-reuse-a = \"=0.1.0\"\n"
                    } else {
                        "[dependencies]\nclippier-reuse-b = \"=0.1.0\"\n"
                    },
                ),
            ];
            if diamond {
                specs.push(("clippier-reuse-d", "[dependencies]\nclippier-reuse-b = \"=0.1.0\"\nclippier-reuse-c = \"=0.1.0\"\n"));
            }
            let mut fixture = Fixture::new(&specs);
            for (name, dependency) in [
                ("clippier-reuse-b", "clippier-reuse-a"),
                (
                    "clippier-reuse-c",
                    if diamond {
                        "clippier-reuse-a"
                    } else {
                        "clippier-reuse-b"
                    },
                ),
            ] {
                fixture
                    .packages
                    .get_mut(name)
                    .unwrap()
                    .publish_dependencies
                    .insert(dependency.into());
            }
            if diamond {
                fixture
                    .packages
                    .get_mut("clippier-reuse-d")
                    .unwrap()
                    .publish_dependencies
                    .extend(["clippier-reuse-b".into(), "clippier-reuse-c".into()]);
            }
            let names = pending(&fixture);
            let pending_set = names.iter().cloned().collect::<BTreeSet<_>>();
            let config = PublishConfig {
                verify: false,
                ..PublishConfig::default()
            };
            let started = Instant::now();
            let staged = stage(&fixture, &names);
            assert_no_staged_locks(&staged);
            let batch_stage_time = started.elapsed();
            let packaging = Instant::now();
            let output = package_command(&staged, &config)
                .arg("--offline")
                .env("CARGO_HOME", fixture.directory.path().join("batch-home"))
                .output()
                .unwrap();
            let batch_package_time = packaging.elapsed();
            assert_result(&output, true, names.len(), 0);
            batch::check_archives(&staged, &fixture.packages, &names).unwrap();
            let mut repeated_stage_time = Duration::ZERO;
            let mut repeated_package_time = Duration::ZERO;
            let mut operations = 0;
            for name in &names {
                let mut closure = BTreeSet::new();
                pending_dependencies(name, &fixture.packages, &pending_set, &mut closure);
                let closure = closure.into_iter().collect::<Vec<_>>();
                let started = Instant::now();
                let independent = stage(&fixture, &closure);
                repeated_stage_time += started.elapsed();
                let packaging = Instant::now();
                let output = package_command(&independent, &config)
                    .arg("--offline")
                    .env("CARGO_HOME", fixture.directory.path().join("repeated-home"))
                    .output()
                    .unwrap();
                repeated_package_time += packaging.elapsed();
                assert_result(&output, true, closure.len(), 0);
                operations += stderr(&output)
                    .lines()
                    .filter(|line| line.trim_start().starts_with("Packaging "))
                    .count();
                batch::check_archives(&independent, &fixture.packages, &closure).unwrap();
                // The root's normalized dependency graph is unchanged by batching.
                assert_eq!(
                    archive_graph(&staged, name),
                    archive_graph(&independent, name)
                );
            }
            assert_eq!(operations, if diamond { 9 } else { 6 });
            eprintln!(
                "production {} fixture: batch invocations=1 packaging={} stage={batch_stage_time:?} package={batch_package_time:?}; repeated invocations={} packaging={operations} stage={repeated_stage_time:?} package={repeated_package_time:?}; no compilation, timings not a speed guarantee",
                if diamond { "diamond" } else { "chain" },
                names.len(),
                names.len()
            );
        }
    }
}
