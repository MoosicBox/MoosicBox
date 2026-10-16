//! Port of the pinned upstream/fork cargo-upgrade integration corpus.
//!
//! Fixture provenance and licenses live in `fixtures/cargo_edit_upgrade/README.md`.
//! No cargo-edit code is linked or executed. These tests inject the releases
//! published by the original main.rs and scenario mod.rs files. TOML comparison
//! ignores quote/decor differences only: every semantic field is compared.
//!
//! Each named test states its classification. Differences amend only explicitly
//! listed dependency requirements in the original output oracle; all remaining
//! fields and all non-manifest input files must remain unchanged. Rejection tests
//! exercise the native CLI and verify the complete fixture remains untouched.
#![cfg(feature = "upgrade")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier::upgrade::{
    self, LockfilePolicy, PinnedPolicy, RegistryKey, UpgradeOptions, UpgradePolicy, UpgradeStatus,
    registry::Release,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const CASES: &[&str] = &[
    "alt_registry",
    "cli_rust_version",
    "dry_run",
    "exclude_dep",
    "exclude_renamed",
    "ignore_rust_version",
    "implicit_prerelease",
    "invalid_dep",
    "invalid_flag",
    "invalid_manifest",
    "invalid_workspace_root_manifest",
    "locked",
    "locked_dry_run",
    "lockfile",
    "optional_dep",
    "package_rust_version",
    "patched",
    "pinned",
    "preserve_op",
    "preserve_precision_major",
    "preserve_precision_minor",
    "preserve_precision_patch",
    "preserves_inline_table",
    "preserves_std_table",
    "single_dep",
    "skip_compatible",
    "specified",
    "to_version",
    "upgrade_all",
    "upgrade_everything",
    "upgrade_renamed",
    "upgrade_verbose",
    "upgrade_verbose_hint",
    "upgrade_workspace",
    "virtual_manifest",
    "workspace_inheritance",
    "workspace_member_cwd",
    "workspace_member_manifest_path",
];

#[test]
fn corpus_inventory_matches_pinned_sources() {
    let root = fixtures("");
    let sources: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("sources.json")).unwrap()).unwrap();
    let cases = sources["cases"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(CASES.len(), 38);
    assert_eq!(cases, CASES.iter().copied().collect());
    let directories = fs::read_dir(&root)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_type().unwrap().is_dir())
        .map(|e| e.file_name().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(directories, CASES.iter().map(|s| (*s).into()).collect());
    assert_eq!(
        sources["upstream"]["revision"],
        "138d4e2948ddcbf249c31c584b02e1b161a86221"
    );
    assert_eq!(
        sources["fork"]["revision"],
        "d6b8eb6990f8a70ffe0d318b3e025328ea59aa7d"
    );
    for name in CASES {
        assert!(root.join(name).join("in").is_dir());
        assert!(root.join(name).join("out").is_dir());
    }
}

#[test]
fn original_fixture_sha256_digests_are_unchanged() {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let root = fixtures("");
    let hashes: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(root.join("sha256.json")).unwrap()).unwrap();
    assert!(!hashes.is_empty());
    for (relative, expected) in &hashes {
        let bytes = fs::read(root.join(relative)).unwrap();
        let hash = Sha256::digest(&bytes)
            .iter()
            .fold(String::new(), |mut output, byte| {
                write!(output, "{byte:02x}").unwrap();
                output
            });
        assert_eq!(&hash, expected, "original fixture modified: {relative}");
    }
    // Ensure every imported case input/output and original driver is covered,
    // not merely the subset that happened to be listed in a truncated manifest.
    for name in CASES {
        for relative in files(&root.join(name)).keys() {
            let key = format!("{name}/{}", relative.to_string_lossy().replace('\\', "/"));
            assert!(
                hashes.contains_key(&key),
                "missing original fixture digest: {key}"
            );
        }
    }
}

const VERSIONS: &[&str] = &[
    "0.1.1-alpha.1+my-package",
    "0.1.1+my-package",
    "0.2.0+my-package",
    "0.2.3+my-package",
    "0.4.1+my-package",
    "20.0.0+my-package",
    "99999.0.0-alpha.1+my-package",
    "99999.0.0+my-package",
    "99999.1.0-alpha.1+my-package",
];
const EVERYTHING: &[&str] = &[
    "docopt",
    "pad",
    "serde",
    "serde_json",
    "syn",
    "tar",
    "ftp",
    "toml_edit",
    "semver",
    "renamed",
    "assert_cli",
    "tempdir",
    "toml",
    "openssl",
    "rget",
    "geo",
];
const OPERATORS: &[&str] = &[
    "default",
    "exact",
    "lessthan",
    "lessorequal",
    "greaterthan",
    "greaterorequal",
    "wildcard",
    "caret",
    "tilde",
];

fn fixtures(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cargo_edit_upgrade")
        .join(name)
}

fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let mut entries = fs::read_dir(path)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            assert!(
                !entry.file_type().unwrap().is_symlink(),
                "corpus must not follow fixture symlinks"
            );
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), result);
            } else {
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn project(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (relative, bytes) in files(&fixtures(name).join("in")) {
        let path = dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    dir
}

fn release(version: &str, rust: Option<&str>) -> Release {
    Release {
        version: version.parse().unwrap(),
        yanked: false,
        rust_version: rust.map(str::to_owned),
        features: BTreeSet::new(),
    }
}

fn metadata(name: &str) -> BTreeMap<RegistryKey, Vec<Release>> {
    let registry = (name == "alt_registry").then(|| "alternative".to_owned());
    let mut data = BTreeMap::new();
    let mut add = |package: &str, versions: &[&str]| {
        data.insert(
            RegistryKey {
                name: package.into(),
                registry: registry.clone(),
            },
            versions.iter().map(|v| release(v, None)).collect(),
        );
    };
    if matches!(
        name,
        "cli_rust_version" | "package_rust_version" | "ignore_rust_version"
    ) {
        let mut versions = vec![
            release("0.1.1", Some("1.60.0")),
            release("0.1.2", Some("1.64.0")),
            release("0.1.3", Some("1.68.0")),
            release("0.2.0", Some("1.68.0")),
        ];
        if name == "cli_rust_version" {
            versions.push(release("0.3.0", Some("1.72.0")));
        }
        data.insert(
            RegistryKey {
                name: "my-package".into(),
                registry: None,
            },
            versions,
        );
    } else if matches!(name, "pinned" | "preserve_op" | "patched") {
        for package in OPERATORS {
            add(package, VERSIONS);
        }
    } else if matches!(
        name,
        "exclude_dep"
            | "exclude_renamed"
            | "to_version"
            | "upgrade_everything"
            | "upgrade_verbose"
            | "upgrade_verbose_hint"
    ) {
        for package in EVERYTHING {
            add(package, VERSIONS);
        }
    } else if name == "skip_compatible" {
        add("test_breaking", &["0.2.0"]);
        add("test_nonbreaking", &["0.1.1", "0.1.2"]);
    } else {
        for package in [
            "my-package",
            "my-package1",
            "my-package2",
            "unrelated-crate",
        ] {
            add(package, VERSIONS);
        }
    }
    data
}

fn options(name: &str, root: &Path) -> UpgradeOptions {
    // Original --package selects a DEPENDENCY, not a workspace package.
    let mut options = UpgradeOptions {
        manifest_path: Some(root.join("Cargo.toml")),
        policy: Some(if name == "skip_compatible" {
            UpgradePolicy::Compatible
        } else {
            UpgradePolicy::Incompatible
        }),
        lockfile: Some(LockfilePolicy::None),
        ..UpgradeOptions::default()
    };
    if matches!(
        name,
        "cli_rust_version"
            | "exclude_dep"
            | "package_rust_version"
            | "preserve_op"
            | "preserve_precision_major"
            | "preserve_precision_minor"
            | "preserve_precision_patch"
            | "upgrade_everything"
            | "upgrade_renamed"
            | "upgrade_verbose"
            | "upgrade_verbose_hint"
    ) {
        options.pinned = Some(PinnedPolicy::Upgrade);
    }
    if name == "cli_rust_version" {
        options.msrv = Some("1.70.0".into());
    }
    if name == "exclude_dep" {
        options.exclude = vec!["docopt".into()];
    }
    if name == "exclude_renamed" {
        options.exclude = vec!["regex".into()];
    }
    if name == "specified" {
        options.dependencies = vec!["my-package1".into()];
    }
    if matches!(
        name,
        "workspace_member_cwd" | "workspace_member_manifest_path"
    ) {
        options.manifest_path = Some(root.join("one/Cargo.toml"));
        options.dependencies = vec!["my-package".into()];
    }
    options
}

/// Explicit semantic amendments to cargo-edit's oracle, never inferred from the
/// native output. Build metadata is NOT stripped here: unwanted +metadata in a
/// requirement is a real regression and must fail the corpus comparison.
fn differences(name: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    match name {
        // Native incompatible mode preserves bounded ranges and wildcard pins;
        // cargo-edit --pinned rewrites these constraints more aggressively.
        "preserve_op" => vec![
            ("Cargo.toml", "dependencies.lessthan", "<0.2"),
            ("Cargo.toml", "dependencies.lessorequal", "<=0.2"),
        ],
        // Existing prereleases do not opt into an unrelated prerelease line.
        "implicit_prerelease" => vec![("Cargo.toml", "dependencies.my-package", "99999.0.0")],
        // Path sources are skipped by default instead of upgrading the registry
        // publishing requirement alongside a local source.
        "upgrade_everything" | "exclude_dep" | "upgrade_verbose" | "upgrade_verbose_hint" => {
            vec![("Cargo.toml", "build-dependencies.serde.version", "1.0")]
        }
        // Original driver publishes toml_edit but its unchanged output oracle
        // does not upgrade alias te. Native deliberately selects the underlying
        // package name, so only that precise requirement differs; rx stays excluded.
        "exclude_renamed" => vec![("Cargo.toml", "dependencies.te.version", "99999.0.0")],
        // Missing release metadata leaves an optional dependency untouched.
        _ => vec![],
    }
}

fn set_requirement(document: &mut toml::Value, dotted: &str, value: &str) {
    let parts = dotted.split('.').collect::<Vec<_>>();
    let mut cursor = document;
    for part in &parts[..parts.len() - 1] {
        cursor = cursor.get_mut(*part).unwrap();
    }
    *cursor.get_mut(*parts.last().unwrap()).unwrap() = toml::Value::String(value.into());
}

async fn run(name: &str) {
    let directory = project(name);
    let root = directory.path();
    let before = files(root);
    let input = upgrade::load_upgrade_input(&options(name, root)).unwrap();
    let plan = upgrade::plan_upgrade_with_releases(&input, &metadata(name)).unwrap();
    assert_eq!(files(root), before, "{name}: planning wrote fixture files");
    if name == "dry_run" {
        assert!(
            !plan.changes().is_empty(),
            "dry-run must actually plan upgrades"
        );
        assert_eq!(
            files(root),
            files(&fixtures(name).join("out")),
            "dry-run must retain original bytes"
        );
        return;
    }
    let report = upgrade::apply_upgrade_plan(&plan).await.unwrap();
    assert!(report.applied);
    assert!(!report.pending);
    let actual = files(root);
    let expected = files(&fixtures(name).join("out"));
    let amended = differences(name);
    for (path, bytes) in &expected {
        if path.file_name().is_some_and(|n| n == "Cargo.toml") {
            let mut oracle: toml::Value =
                toml::from_str(std::str::from_utf8(bytes).unwrap()).unwrap();
            for (file, field, value) in &amended {
                if path == Path::new(file) {
                    set_requirement(&mut oracle, field, value);
                }
            }
            let native: toml::Value =
                toml::from_str(std::str::from_utf8(actual.get(path).unwrap()).unwrap()).unwrap();
            assert_eq!(
                native,
                oracle,
                "{name}: output oracle mismatch at {} (explicit differences: {amended:?})",
                path.display()
            );
        }
    }
    // Every manifest is accounted for, including any not listed in the subset
    // output oracle. Non-manifests (including locks) must remain byte-identical:
    // lock resolution parity is explicitly outside this manifest-injection port.
    for (path, bytes) in &before {
        if path.file_name().is_none_or(|n| n != "Cargo.toml") || !expected.contains_key(path) {
            assert_eq!(
                actual.get(path),
                Some(bytes),
                "{name}: unrelated field/file changed: {}",
                path.display()
            );
        }
    }
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        actual.keys().collect::<Vec<_>>(),
        "{name}: unexpected file creation/removal"
    );
    if matches!(
        name,
        "workspace_inheritance" | "workspace_member_cwd" | "workspace_member_manifest_path"
    ) {
        assert!(
            report.dependencies.len() >= 4,
            "workspace consumers must be reported"
        );
    }
    if matches!(
        name,
        "upgrade_everything" | "exclude_dep" | "upgrade_verbose" | "upgrade_verbose_hint"
    ) {
        assert!(
            report
                .dependencies
                .iter()
                .any(|d| d.alias == "serde" && d.status == UpgradeStatus::Skipped)
        );
    }
}

fn reject(name: &str, arguments: &[&str], diagnostic: &str) {
    let directory = project(name);
    let before = files(directory.path());
    let output = Command::new(env!("CARGO_BIN_EXE_clippier"))
        .current_dir(directory.path())
        .args(["upgrade", "plan", "--offline", "--no-lockfile"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "{name}: unsupported/invalid invocation unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(diagnostic),
        "{name}: expected diagnostic {diagnostic:?}, got {stderr}"
    );
    assert_eq!(
        files(directory.path()),
        before,
        "{name}: rejected invocation mutated inputs"
    );
}

macro_rules! corpus {
    ($($name:ident),+ $(,)?)=>{$(
        #[switchy_async::test]
        async fn $name(){run(stringify!($name)).await;}
    )+};
}
// Equivalent manifest semantics, ignoring TOML string quote/decor only.
corpus!(
    alt_registry,
    cli_rust_version,
    optional_dep,
    package_rust_version,
    preserve_precision_major,
    preserve_precision_minor,
    preserve_precision_patch,
    preserves_inline_table,
    preserves_std_table,
    single_dep,
    skip_compatible,
    specified,
    upgrade_all,
    upgrade_renamed,
    upgrade_workspace,
    virtual_manifest,
    workspace_inheritance,
    workspace_member_cwd,
    workspace_member_manifest_path
);
// Tested intentional native differences: no workspace writes in dry-run; safe
// source/prerelease/range/pin policies. Full remaining output oracle is checked.
corpus!(
    dry_run,
    exclude_dep,
    exclude_renamed,
    implicit_prerelease,
    pinned,
    preserve_op,
    patched,
    upgrade_everything,
    upgrade_verbose,
    upgrade_verbose_hint
);
// Manifest-only equivalent; the original scenario also refreshed Cargo.lock.
// This test explicitly protects its original lock and does not claim resolver parity.
corpus!(lockfile);

#[test]
fn invalid_dep() {
    reject("invalid_dep", &["--dependency", "failure"], "dependency");
}
#[test]
fn invalid_flag() {
    reject("invalid_flag", &["--flag"], "unexpected argument");
}
#[test]
fn invalid_manifest() {
    reject("invalid_manifest", &[], "TOML");
}
#[test]
fn invalid_workspace_root_manifest() {
    reject("invalid_workspace_root_manifest", &[], "TOML");
}
// Unsupported cargo-edit flags are rejected, not silently ignored or translated
// into a success with different semantics. These are explicit migration gaps.
#[test]
fn locked() {
    reject("locked", &["--locked"], "unexpected argument");
}
#[test]
fn locked_dry_run() {
    reject("locked_dry_run", &["--locked"], "unexpected argument");
}
#[test]
fn ignore_rust_version() {
    reject(
        "ignore_rust_version",
        &["--ignore-rust-version"],
        "unexpected argument",
    );
}
#[test]
fn to_version() {
    // Preserve the original exact-version request: cargo-edit accepts name@version
    // combined with both selection modes disabled; native rejects those flags.
    reject(
        "to_version",
        &[
            "--dependency",
            "docopt@99999.0.0",
            "--compatible=false",
            "--incompatible=false",
        ],
        "unexpected value",
    );
    // Native also has no independent exact-target override flag.
    reject(
        "to_version",
        &["--target", "99999.0.0"],
        "unexpected argument",
    );
}
