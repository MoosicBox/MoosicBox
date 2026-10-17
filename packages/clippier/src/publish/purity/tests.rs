use super::*;

mod http;

fn archive(files: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(
        fs::File::create(file.path()).unwrap(),
        flate2::Compression::default(),
    ));
    for (path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, path, *bytes).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap();
    file
}

#[test]
fn manifest_normalization_preserves_version_and_meaningful_fields() {
    let left = archive(&[(
        "a-0.5.0/Cargo.toml",
        b"[package]\nname='a'\nversion='0.5.0'\ninclude=['src/**']\n",
    )]);
    let formatted = archive(&[(
        "a-0.5.0/Cargo.toml",
        b"# generated\n[package]\nversion = '0.5.0'\ninclude = ['src/**']\nname = 'a'\n",
    )]);
    let changed = archive(&[(
        "a-0.6.0/Cargo.toml",
        b"[package]\nname='a'\nversion='0.6.0'\ninclude=['src/**']\n",
    )]);
    let entries = |file: &tempfile::NamedTempFile, policy| {
        normalized_archive_entries(file.path(), policy).unwrap()
    };
    assert_eq!(
        entries(&left, ManifestVersionPolicy::Preserve),
        entries(&formatted, ManifestVersionPolicy::Preserve)
    );
    assert_ne!(
        entries(&left, ManifestVersionPolicy::Preserve),
        entries(&changed, ManifestVersionPolicy::Preserve)
    );
    assert_eq!(
        entries(&left, ManifestVersionPolicy::IgnorePackageVersion),
        entries(&changed, ManifestVersionPolicy::IgnorePackageVersion)
    );
    for field in [
        "include=['docs/**']",
        "build='build.rs'",
        "readme='README.md'",
        "rust-version='1.85.0'",
        "[dependencies]\nb='1.0.0'",
        "[features]\nfoo=[]",
    ] {
        let manifest = format!("[package]\nname='a'\nversion='0.5.0'\n{field}\n");
        let changed = archive(&[("a-0.5.0/Cargo.toml", manifest.as_bytes())]);
        assert_ne!(
            entries(&left, ManifestVersionPolicy::Preserve),
            entries(&changed, ManifestVersionPolicy::Preserve),
            "{field}"
        );
    }
}

#[test]
fn normalized_contents_include_lock_build_docs_and_detect_all_change_kinds() {
    let published = archive(&[
        (
            "a-0.5.0/Cargo.toml",
            b"[package]\nname='a'\nversion='0.5.0'\n",
        ),
        ("a-0.5.0/Cargo.lock", b"lock old"),
        ("a-0.5.0/build.rs", b"fn main() {}"),
        ("a-0.5.0/README.md", b"old docs"),
        ("a-0.5.0/src/lib.rs", b"pub fn a() {}\r\n"),
        ("a-0.5.0/Cargo.toml.orig", b"ignored"),
        ("a-0.5.0/.cargo_vcs_info.json", b"ignored"),
    ]);
    let local = archive(&[
        (
            "a-0.5.0/Cargo.toml",
            b"[package]\nname='a'\nversion='0.5.0'\n",
        ),
        ("a-0.5.0/Cargo.lock", b"lock new"),
        ("a-0.5.0/README.md", b"new docs"),
        ("a-0.5.0/src/lib.rs", b"pub fn a() {}\n"),
        ("a-0.5.0/new.txt", b"added"),
    ]);
    let package = PublishPackageInfo::new("a", "0.5.0", true, BTreeSet::new());
    let assessment = compare(
        &package,
        &normalized_archive_entries(local.path(), ManifestVersionPolicy::Preserve).unwrap(),
        &normalized_archive_entries(published.path(), ManifestVersionPolicy::Preserve).unwrap(),
        vec!["b".into()],
    );
    assert_eq!(assessment.status, PurityStatus::Impure);
    assert_eq!(assessment.changed, ["README.md", "src/lib.rs"]);
    assert_eq!(assessment.added, ["new.txt"]);
    assert_eq!(assessment.removed, ["build.rs"]);
}

#[test]
fn unsafe_ambiguous_and_oversized_archives_are_unknown_not_equal() {
    for files in [
        vec![
            ("a/Cargo.toml", b"[package]".as_slice()),
            ("b/README.md", b"docs".as_slice()),
        ],
        vec![
            ("a/Cargo.toml", b"[package]".as_slice()),
            ("a/Cargo.toml", b"[package]".as_slice()),
        ],
        vec![("a/Cargo.toml", b"not toml".as_slice())],
        vec![("a\\Cargo.toml", b"[package]".as_slice())],
        vec![("a/README.md", b"docs".as_slice())],
    ] {
        assert!(
            normalized_archive_entries(archive(&files).path(), ManifestVersionPolicy::Preserve)
                .is_err()
        );
    }
    let file = tempfile::NamedTempFile::new().unwrap();
    file.as_file().set_len(MAX_DOWNLOAD + 1).unwrap();
    assert!(normalized_archive_entries(file.path(), ManifestVersionPolicy::Preserve).is_err());

    for link in [tar::EntryType::Symlink, tar::EntryType::Link] {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            file.reopen().unwrap(),
            flate2::Compression::default(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(link);
        header.set_size(0);
        header.set_mode(0o644);
        builder
            .append_link(&mut header, "a/link", "../../outside")
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        assert!(normalized_archive_entries(file.path(), ManifestVersionPolicy::Preserve).is_err());
    }
}

#[test]
fn archive_normalization_retains_lockfiles_but_purity_reports_them_separately() {
    let local = archive(&[
        (
            "a-0.5.0/Cargo.toml",
            b"[package]\nname='a'\nversion='0.5.0'\n",
        ),
        ("a-0.5.0/Cargo.lock", b"version = 4\n"),
    ]);
    let published = archive(&[
        (
            "a-0.5.0/Cargo.toml",
            b"[package]\nname='a'\nversion='0.5.0'\n",
        ),
        ("a-0.5.0/Cargo.lock", b"version = 3\n"),
    ]);
    let local = normalized_archive_entries(local.path(), ManifestVersionPolicy::Preserve).unwrap();
    let published =
        normalized_archive_entries(published.path(), ManifestVersionPolicy::Preserve).unwrap();
    assert_ne!(local, published); // Other archive consumers still see lockfile changes.
    let assessment = compare(
        &PublishPackageInfo::new("a", "0.5.0", true, BTreeSet::new()),
        &local,
        &published,
        Vec::new(),
    );
    assert_eq!(assessment.status, PurityStatus::Pure);
    assert!(assessment.lockfile_changed);
    assert!(enforce(PurityPolicy::RequirePure, &[assessment]).is_ok());
}

#[test]
fn root_lockfile_changes_are_separate_and_do_not_block_source_purity() {
    let package = PublishPackageInfo::new("fixture", "0.1.0", true, BTreeSet::new());
    for (local_lock, published_lock) in [
        (Some("new"), Some("old")),
        (Some("new"), None),
        (None, Some("old")),
        (None, None),
        (Some("same"), Some("same")),
    ] {
        let mut local = BTreeMap::from([("src/lib.rs".into(), b"same source".to_vec())]);
        let mut published = local.clone();
        if let Some(lock) = local_lock {
            local.insert("Cargo.lock".into(), lock.as_bytes().to_vec());
        }
        if let Some(lock) = published_lock {
            published.insert("Cargo.lock".into(), lock.as_bytes().to_vec());
        }
        let assessment = compare(&package, &local, &published, vec!["consumer".into()]);
        assert_eq!(assessment.status, PurityStatus::Pure);
        assert_eq!(assessment.lockfile_changed, local_lock != published_lock);
        assert!(enforce(PurityPolicy::RequirePure, std::slice::from_ref(&assessment)).is_ok());
        let text = diagnostic(
            PurityPolicy::RequirePure,
            &assessment,
            crate::ColorMode::Never,
        );
        assert!(!text.contains("local changes"));
        assert!(!text.contains("blocked"));
        if assessment.lockfile_changed {
            assert!(text.contains("cargo install --locked"));
            assert!(summary(&assessment).contains("Cargo.lock differs"));
        }
        let json = serde_json::to_string(&assessment).unwrap();
        assert_eq!(
            serde_json::from_str::<PurityAssessment>(&json).unwrap(),
            assessment
        );
        local.insert("src/lib.rs".into(), b"changed source".to_vec());
        let impure = compare(&package, &local, &published, Vec::new());
        assert_eq!(impure.status, PurityStatus::Impure);
        assert!(enforce(PurityPolicy::RequirePure, &[impure]).is_err());
    }
    // Only the root generated lockfile is special, not arbitrary nested files.
    let local = BTreeMap::from([("fixtures/Cargo.lock".into(), b"fixture".to_vec())]);
    assert_eq!(
        compare(&package, &local, &BTreeMap::new(), Vec::new()).status,
        PurityStatus::Impure
    );
}

#[test]
fn strict_assessments_preserve_details_and_warnings_are_bounded() {
    let assessment = PurityAssessment {
        name: "a".into(),
        version: "0.5.0".into(),
        status: PurityStatus::Unknown,
        affected_pending_consumers: Vec::new(),
        added: (0..100).map(|index| format!("file-{index}")).collect(),
        removed: Vec::new(),
        changed: Vec::new(),
        lockfile_changed: false,
        diagnostics: vec!["unsafe\narchive".into()],
    };
    assert!(enforce(PurityPolicy::Warn, std::slice::from_ref(&assessment)).is_ok());
    let error = enforce(PurityPolicy::RequirePure, std::slice::from_ref(&assessment)).unwrap_err();
    let structured = error.downcast_ref::<PublishPurityError>().unwrap();
    assert_eq!(structured.assessments[0].added.len(), 100);
    let text = summary(&assessment);
    assert_eq!(text, "a@0.5.0: could not verify published contents");
    let text = diagnostic(PurityPolicy::Warn, &assessment, crate::ColorMode::Never);
    assert!(text.contains("warning: a@0.5.0 could not verify"));
    assert!(text.contains("Reason: unsafe archive"));
    assert!(text.contains("Continuing."));
    assert!(!text.contains("added="));
    assert!(!text.contains('\u{1b}'));
    let mut impure = assessment.clone();
    impure.status = PurityStatus::Impure;
    let text = diagnostic(PurityPolicy::Warn, &impure, crate::ColorMode::Always);
    assert!(text.contains("\u{1b}[33;1mwarning"));
    assert!(text.contains("\u{1b}[36;1ma@0.5.0"));
    assert!(text.contains("Consumers will receive the crates.io version"));
    assert!(text.contains("Added: file-0, file-1, file-2 (+97 more)"));
    assert!(!text.contains("file-3"));
    assert!(!text.contains("Reason:"));
    assert_eq!(
        summary(&impure),
        "a@0.5.0: differs from crates.io (100 added)"
    );
    let json = serde_json::to_string(&impure).unwrap();
    assert!(!json.contains("\\u001b"));
    assert!(json.contains("file-99"));
    impure.status = PurityStatus::Pure;
    assert_eq!(
        diagnostic(PurityPolicy::Warn, &impure, crate::ColorMode::Always),
        ""
    );
    let text = diagnostic(
        PurityPolicy::RequirePure,
        &assessment,
        crate::ColorMode::Always,
    );
    assert!(text.contains("\u{1b}[31;1merror"));
    assert!(text.contains("Publishing blocked"));
}

#[test]
fn consumers_follow_transitive_optional_build_target_edges_but_not_unrelated_packages() {
    let mut packages = BTreeMap::new();
    for (name, deps) in [
        ("a", vec![]),
        ("optional-build-target", vec!["a"]),
        ("b", vec!["optional-build-target"]),
        ("independent", vec![]),
    ] {
        packages.insert(
            name.into(),
            PublishPackageInfo::new(
                name,
                "0.5.0",
                true,
                deps.into_iter().map(str::to_string).collect(),
            ),
        );
    }
    assert_eq!(
        consumers(
            &packages,
            &BTreeSet::from(["b".into(), "independent".into()]),
            "a"
        ),
        ["b"]
    );
}
