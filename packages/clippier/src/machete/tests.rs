//! Focused native-engine regressions. No Cargo subprocesses or network access.
use super::*;
use std::{fs, path::Path};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname = 'fixture'\nversion = '0.1.0'\n[dependencies]\nused = '1.0.0' # retained\nunused = '1.0.0' # removed\n",
    );
    write(dir.path(), "src/lib.rs", "pub fn run() { used::run(); }\n");
    dir
}
fn options(root: &Path) -> MacheteOptions {
    MacheteOptions {
        manifest_path: Some(root.to_owned()),
        ..Default::default()
    }
}
#[test]
fn exact_plan_and_checked_apply_preserve_lockfile_and_formatting() {
    let dir = fixture();
    write(dir.path(), "Cargo.lock", "lockfile bytes preserved\n");
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(plan.report.has_unused());
    assert_eq!(plan.changes.len(), 1);
    assert!(plan.changes[0].after.contains("used = '1.0.0' # retained"));
    assert!(!plan.changes[0].after.contains("unused ="));
    assert!(
        fs::read_to_string(dir.path().join("Cargo.toml"))
            .unwrap()
            .contains("unused =")
    );
    assert!(apply_machete_plan(&plan).unwrap().applied);
    assert_eq!(
        fs::read_to_string(dir.path().join("Cargo.lock")).unwrap(),
        "lockfile bytes preserved\n"
    );
    assert!(
        !plan_machete(&options(dir.path()))
            .unwrap()
            .report
            .has_unused()
    );
}
#[test]
fn reject_changed_source_config_inventory_and_tampered_changes() {
    let dir = fixture();
    let plan = plan_machete(&options(dir.path())).unwrap();
    write(dir.path(), "src/lib.rs", "unused::run();");
    assert!(apply_machete_plan(&plan).is_err());
    let plan = plan_machete(&options(dir.path())).unwrap();
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nignored = ['unused']\n",
    );
    assert!(apply_machete_plan(&plan).is_err());
    let plan = plan_machete(&options(dir.path())).unwrap();
    write(dir.path(), "src/new.rs", "used::run();");
    assert!(apply_machete_plan(&plan).is_err());
    let mut plan = plan_machete(&options(dir.path())).unwrap();
    plan.changes.push(MacheteChange {
        path: dir.path().join("Cargo.toml"),
        before: String::new(),
        after: String::new(),
    });
    assert!(apply_machete_plan(&plan).is_err());
}
#[test]
fn lexical_paths_macros_derives_attrs_aliases_and_extern_crate() {
    let evidence = source::inspect(
        r##"
        use alias::Thing as Renamed;
        extern crate raw_name as different;
        #[derive(derive_crate::Trait)]
        #[attribute_crate::attribute]
        fn f() { macro_crate::go!(); r#raw_ident::go(); }
        // ignored_comment::reference();
        const TEXT: &str = r#"ignored_string::reference()"#;
    "##,
    );
    assert!(evidence.incomplete.is_empty());
    for name in [
        "alias",
        "raw_name",
        "derive_crate",
        "attribute_crate",
        "macro_crate",
        "raw_ident",
    ] {
        assert!(evidence.names.contains(name), "{name}");
    }
    assert!(!evidence.names.contains("ignored_comment"));
    assert!(!evidence.names.contains("ignored_string"));
}
#[test]
fn features_workspace_aliases_targets_and_build_scripts() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers = ['member']\n[workspace.dependencies]\nrenamed = { package = 'actual', version = '1.0.0' }\n",
    );
    write(
        dir.path(),
        "member/Cargo.toml",
        "[package]\nname='member'\nversion='0.1.0'\n[dependencies]\nrenamed.workspace=true\nfeature_dep={version='1.0.0',optional=true}\n[features]\nselected=['dep:feature_dep']\n[build-dependencies]\nbuilder='1.0.0'\n[target.'cfg(unix)'.dev-dependencies]\ntester='1.0.0'\n",
    );
    write(dir.path(), "member/src/lib.rs", "renamed::go!();");
    write(
        dir.path(),
        "member/build.rs",
        "fn main(){ builder::run(); }",
    );
    write(
        dir.path(),
        "member/tests/test.rs",
        "#[test] fn test(){tester::go();}",
    );
    let mut selected = options(dir.path());
    selected.kinds = vec![
        DependencyKind::Normal,
        DependencyKind::Dev,
        DependencyKind::Build,
    ];
    let plan = plan_machete(&selected).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(plan.changes.is_empty());
    assert_eq!(plan.report.dependencies.len(), 4);
    assert!(
        plan.report
            .dependencies
            .iter()
            .all(|d| d.status == DependencyStatus::Used)
    );
}
#[test]
fn structural_roots_drive_checked_removal_without_hiding_unknowns() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='roots'\nversion='0.1.0'\n[dependencies]\nfutures='1.0.0'\nnot_futures='1.0.0'\nlogging='1.0.0'\nmacro_input='1.0.0'\n",
    );
    write(
        dir.path(),
        "src/lib.rs",
        "use {not_futures::futures::{Thing as futures}, Logging as renamed};\nfn f(){ quoted!(macro_input); }\n/// futures::run();\nconst TEXT:&str=\"futures::run();\";\n",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    for dependency in &plan.report.dependencies {
        assert_eq!(
            dependency.status,
            if dependency.dependency == "futures" {
                DependencyStatus::Unused
            } else {
                DependencyStatus::Used
            },
            "{}",
            dependency.dependency,
        );
    }
    assert_eq!(plan.changes.len(), 1);
    assert!(!plan.changes[0].after.contains("\nfutures="));
    assert!(apply_machete_plan(&plan).unwrap().applied);
}

#[test]
fn generated_includes_and_missing_modules_refuse_apply() {
    let dir = fixture();
    write(
        dir.path(),
        "src/lib.rs",
        "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(!plan.report.complete);
    assert!(apply_machete_plan(&plan).is_err());
    write(dir.path(), "src/lib.rs", "mod missing;");
    assert!(!plan_machete(&options(dir.path())).unwrap().report.complete);
}
#[test]
fn parent_and_nested_directory_exclusions_and_scoped_ignores_accumulate() {
    let dir = fixture();
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nignored-dirs=['fixtures']\n[[machete.rules]]\npackages=['fixture']\nignored=['unused']\n",
    );
    write(dir.path(), "fixtures/Cargo.toml", "not valid TOML at all");
    write(
        dir.path(),
        "src/clippier.toml",
        "[machete]\nignored-dirs=['generated']\n",
    );
    write(dir.path(), "src/generated/broken.rs", "{ bad source");
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(!plan.report.has_unused());
    assert!(
        plan.report
            .dependencies
            .iter()
            .any(|d| d.dependency == "unused" && d.status == DependencyStatus::Ignored)
    );
}
#[test]
fn literal_external_module_and_build_helper_are_inspected() {
    let dir = fixture();
    write(
        dir.path(),
        "src/lib.rs",
        "#[path = \"../helper.rs\"] mod helper;",
    );
    write(dir.path(), "helper.rs", "unused::go();");
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(
        plan.report
            .dependencies
            .iter()
            .any(|d| d.dependency == "unused" && d.status == DependencyStatus::Used)
    );
}
#[test]
fn directory_paths_are_not_invented_from_unrelated_assignments() {
    let dir = fixture();
    write(
        dir.path(),
        "src/lib.rs",
        "pub fn run() { let path = \"\"; let _ = path; used::run(); }\n#[route(path = \"/\")] fn endpoint() {}\n",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    write(dir.path(), "src/lib.rs", "#[path = \".\"] mod invalid;");
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(!plan.report.complete);
    assert!(
        plan.report
            .diagnostics
            .iter()
            .any(|s| s.contains("directory"))
    );
    assert!(apply_machete_plan(&plan).is_err());
}

#[test]
fn integration_test_modules_resolve_next_to_the_target_root() {
    let dir = fixture();
    write(dir.path(), "tests/integration.rs", "mod helper;");
    write(
        dir.path(),
        "tests/helper.rs",
        "pub fn f() { unused::go(); }",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(!plan.report.has_unused());
}

#[test]
fn explicit_binary_root_resolves_sibling_modules() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n[[bin]]\nname='lambda'\npath='src/lambda_vanilla_js.rs'\n[dependencies]\nused='1.0.0'\n",
    );
    write(
        dir.path(),
        "src/lambda_vanilla_js.rs",
        "mod lambda; fn main(){lambda::run();}",
    );
    write(dir.path(), "src/lambda.rs", "pub fn run(){used::run();}");
    let report = analyze_machete(&options(dir.path())).unwrap();
    assert!(report.complete, "{:?}", report.diagnostics);
    assert!(!report.has_unused());
}

#[test]
fn quoted_module_declarations_are_not_source_inventory() {
    let dir = fixture();
    write(
        dir.path(),
        "src/lib.rs",
        "fn test(){let item = syn::parse_quote!{mod external_module;}; used::run(); unused::run();}\nmacro_rules! quoted { () => {mod generated_module;} }\n",
    );
    let report = analyze_machete(&options(dir.path())).unwrap();
    assert!(report.complete, "{:?}", report.diagnostics);
    assert!(!report.has_unused());
    // Real declarations after token trees must still be checked.
    write(
        dir.path(),
        "src/lib.rs",
        "quote!{mod quoted;} mod actually_missing;",
    );
    let report = analyze_machete(&options(dir.path())).unwrap();
    assert!(!report.complete);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|s| s.contains("actually_missing"))
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|s| s.contains("quoted/mod.rs"))
    );
}

#[test]
fn dynamic_text_includes_are_advisory_and_rust_candidates_stay_protected() {
    let dir = fixture();
    write(
        dir.path(),
        "src/lib.rs",
        "const TEXT:&str=include_str!(concat!(env!(\"DIR\"),\"/index.js\")); used::run(); unused::run();",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert_ne!(plan.report.diagnostics.len(), 0);
    assert!(!plan.report.has_unused());
    write(
        dir.path(),
        "src/lib.rs",
        "include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\")); used::run(); unused::run();",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(plan.changes.is_empty());
    write(
        dir.path(),
        "src/lib.rs",
        "include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\")); used::run();",
    );
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(!plan.report.complete);
    assert!(
        plan.report
            .dependencies
            .iter()
            .any(|d| d.dependency == "unused" && d.status == DependencyStatus::Incomplete)
    );
    assert!(plan.changes.is_empty());
    assert!(apply_machete_plan(&plan).is_err());
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nignored=['unused']\n",
    );
    let report = analyze_machete(&options(dir.path())).unwrap();
    assert!(report.complete, "{:?}", report.diagnostics);
    assert!(
        report
            .dependencies
            .iter()
            .any(|d| d.dependency == "unused" && d.status == DependencyStatus::Ignored)
    );
}

#[test]
fn member_manifest_defaults_to_that_member_not_siblings() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Cargo.toml", "[workspace]\nmembers=['a','b']\n");
    for name in ["a", "b"] {
        write(
            dir.path(),
            &format!("{name}/Cargo.toml"),
            &format!("[package]\nname='{name}'\nversion='0.1.0'\n[dependencies]\nunused='1.0.0'\n"),
        );
        write(
            dir.path(),
            &format!("{name}/src/lib.rs"),
            "pub fn run(){}\n",
        );
    }
    let mut options = options(&dir.path().join("a"));
    let plan = plan_machete(&options).unwrap();
    assert_eq!(plan.changes.len(), 1);
    assert!(plan.changes[0].path.ends_with("a/Cargo.toml"));
    assert!(
        plan.report
            .dependencies
            .iter()
            .filter(|d| d.package == "b")
            .all(|d| d.status == DependencyStatus::Skipped)
    );
    options.packages = vec!["b".into()];
    let plan = plan_machete(&options).unwrap();
    assert_eq!(plan.changes.len(), 1);
    assert!(plan.changes[0].path.ends_with("b/Cargo.toml"));
}

#[test]
fn local_library_names_and_aliases_are_resolved_without_cargo() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['app','dep']\n[workspace.dependencies]\ndep={path='dep'}\n",
    );
    write(
        dir.path(),
        "app/Cargo.toml",
        "[package]\nname='app'\nversion='0.1.0'\n[dependencies]\ndep.workspace=true\nalias={package='dep',path='../dep'}\n",
    );
    write(
        dir.path(),
        "app/src/lib.rs",
        "custom_library::run(); alias::run();",
    );
    write(
        dir.path(),
        "dep/Cargo.toml",
        "[package]\nname='dep'\nversion='0.1.0'\n[lib]\nname='custom_library'\n",
    );
    write(dir.path(), "dep/src/lib.rs", "pub fn run(){}");
    let plan = plan_machete(&options(&dir.path().join("app"))).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(!plan.report.has_unused());
    assert!(plan.changes.is_empty());
    // The dependency manifest is evidence, even if only the consumer was selected.
    write(
        dir.path(),
        "dep/Cargo.toml",
        "[package]\nname='dep'\nversion='0.1.0'\n[lib]\nname='changed'\n",
    );
    assert!(apply_machete_plan(&plan).is_err());
}

#[test]
fn inline_module_paths_preserve_the_module_directory_stack() {
    let dir = fixture();
    write(
        dir.path(),
        "src/lib.rs",
        "mod outer { mod inner { mod external; } } used::run();",
    );
    write(dir.path(), "src/outer/inner/external.rs", "unused::run();");
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(!plan.report.has_unused());
    write(
        dir.path(),
        "src/lib.rs",
        "mod outer { mod inner { mod missing; } }",
    );
    let report = analyze_machete(&options(dir.path())).unwrap();
    assert!(!report.complete);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.contains("outer/inner/missing/mod.rs"))
    );
}

#[test]
fn ignore_files_and_presence_tracked_no_ignore() {
    let dir = fixture();
    write(dir.path(), ".gitignore", "src/ignored/\n");
    write(dir.path(), "src/ignored/broken.rs", "{ invalid");
    let mut options = options(dir.path());
    assert!(analyze_machete(&options).unwrap().complete);
    options.no_ignore = Some(true);
    assert!(!analyze_machete(&options).unwrap().complete);
    options.no_ignore = None;
    write(dir.path(), "clippier.toml", "[machete]\nno-ignore=true\n");
    assert!(!analyze_machete(&options).unwrap().complete);
    options.no_ignore = Some(false);
    assert!(analyze_machete(&options).unwrap().complete);
}

#[test]
fn explicit_metadata_maps_library_names_and_protects_manifest_evidence() {
    let dir = fixture();
    let root_manifest = dir.path().join("Cargo.toml");
    let json = serde_json::json!({
        "packages":[
            {"id":"fixture","name":"fixture","manifest_path":root_manifest,"targets":[{"kind":["lib"],"name":"fixture"}]},
            {"id":"used","name":"used","manifest_path":root_manifest,"targets":[{"kind":["lib"],"name":"custom_name"}]}
        ],
        "resolve":{"nodes":[{"id":"fixture","deps":[{"name":"custom_name","pkg":"used"}]}]}
    });
    let metadata = MacheteMetadata::from_cargo_json(&json.to_string()).unwrap();
    assert_eq!(metadata.crate_names[&root_manifest]["used"], "custom_name");
    let serialized = serde_json::to_string(&metadata).unwrap();
    let metadata: MacheteMetadata = serde_json::from_str(&serialized).unwrap();
    write(
        dir.path(),
        "src/lib.rs",
        "custom_name::run(); unused::run();",
    );
    let mut options = options(dir.path());
    options.metadata = Some(metadata);
    let plan = plan_machete(&options).unwrap();
    assert!(plan.report.complete);
    assert!(!plan.report.has_unused());
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='changed'\nversion='0.1.0'\n",
    );
    assert!(plan_machete(&options).is_err());
    assert!(MacheteMetadata::from_cargo_json("{\"packages\":[]}").is_err());
}

#[test]
fn ancestor_configuration_is_inherited_and_snapshotted() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nignored=['unused']\nignored-dirs=['child/fixtures']\n",
    );
    write(
        dir.path(),
        "child/Cargo.toml",
        "[package]\nname='child'\nversion='0.1.0'\n[dependencies]\nunused='1.0.0'\n",
    );
    write(dir.path(), "child/src/lib.rs", "pub fn run(){}\n");
    write(dir.path(), "child/fixtures/broken.rs", "{ bad");
    let options = options(&dir.path().join("child"));
    let plan = plan_machete(&options).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(!plan.report.has_unused());
    assert!(
        plan.report
            .dependencies
            .iter()
            .all(|dependency| dependency.status == DependencyStatus::Ignored)
    );
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nignored=[]\nignored-dirs=['child/fixtures']\n",
    );
    assert!(apply_machete_plan(&plan).is_err());
    assert!(analyze_machete(&options).unwrap().has_unused());
}

#[test]
fn standalone_root_scan_discovers_independent_nested_packages() {
    let dir = fixture();
    write(
        dir.path(),
        "nested/Cargo.toml",
        "[package]\nname='independent'\nversion='0.1.0'\n[dependencies]\nnested_unused='1.0.0'\n",
    );
    write(dir.path(), "nested/src/lib.rs", "pub fn run(){}\n");
    let plan = plan_machete(&options(dir.path())).unwrap();
    assert!(plan.report.complete, "{:?}", plan.report.diagnostics);
    assert!(
        plan.report
            .dependencies
            .iter()
            .any(|dependency| dependency.package == "independent"
                && dependency.status == DependencyStatus::Unused)
    );
    assert_eq!(plan.changes.len(), 2);
}

#[test]
fn dev_dependencies_are_opt_in() {
    let dir = fixture();
    let manifest = dir.path().join("Cargo.toml");
    let contents = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        format!("{contents}\n[dev-dependencies]\ndev_unused='1.0.0'\n"),
    )
    .unwrap();
    let mut options = options(dir.path());
    let report = analyze_machete(&options).unwrap();
    assert!(
        report
            .dependencies
            .iter()
            .any(|dependency| dependency.dependency == "dev_unused"
                && dependency.status == DependencyStatus::Skipped)
    );
    options.kinds = vec![DependencyKind::Dev];
    let report = analyze_machete(&options).unwrap();
    assert!(
        report
            .dependencies
            .iter()
            .any(|dependency| dependency.dependency == "dev_unused"
                && dependency.status == DependencyStatus::Unused)
    );
    assert!(
        report
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind == DependencyKind::Normal)
            .all(|dependency| dependency.status == DependencyStatus::Skipped)
    );
}

#[test]
fn explicit_configuration_and_unknown_keys() {
    let dir = fixture();
    write(dir.path(), "custom.toml", "[machete]\nignored=['unused']\n");
    let mut options = options(dir.path());
    options.config_path = Some(dir.path().join("custom.toml"));
    assert!(!analyze_machete(&options).unwrap().has_unused());
    write(dir.path(), "custom.toml", "[machete]\nunknown=true\n");
    assert!(analyze_machete(&options).is_err());
}

fn precise(root: &Path) -> MacheteOptions {
    MacheteOptions {
        precision: true,
        ..options(root)
    }
}

fn dependency_status(report: &MacheteReport, name: &str) -> DependencyStatus {
    report
        .dependencies
        .iter()
        .find(|dependency| dependency.dependency == name)
        .unwrap()
        .status
}

#[test]
fn precision_local_modules_and_crate_import_aliases_have_lexical_scopes() {
    let dir = fixture();
    for source in [
        "mod used { pub fn run() {} } fn run() { used::run(); }",
        "fn run() { mod used { pub fn run() {} } used::run(); }",
        "use other as used; fn run() { used::run(); }",
    ] {
        write(dir.path(), "src/lib.rs", source);
        let report = analyze_machete(&precise(dir.path())).unwrap();
        assert_eq!(
            dependency_status(&report, "used"),
            DependencyStatus::Unused,
            "{source}"
        );
    }
    for source in [
        "fn run() { { mod used {} } used::run(); }",
        "mod used {} mod nested { fn run() { used::run(); } }",
        "mod used {} fn run() { ::used::run(); }",
        "#[cfg(feature=\"special\")] mod used {} fn run() { used::run(); }",
        "fn run(used: usize) { used::run(); let used = 1; used::run(); }",
    ] {
        write(dir.path(), "src/lib.rs", source);
        let report = analyze_machete(&precise(dir.path())).unwrap();
        assert_eq!(
            dependency_status(&report, "used"),
            DependencyStatus::Used,
            "{source}"
        );
    }
}

#[test]
fn precision_import_reexport_positions_and_alias_package_are_structured() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n[dependencies]\nrenamed={package='real-package',version='1.0.0'}\n",
    );
    write(
        dir.path(),
        "src/lib.rs",
        "// no usage\npub use renamed::Thing as PublicThing;\n",
    );
    let report = analyze_machete(&precise(dir.path())).unwrap();
    let evidence = report
        .evidence
        .iter()
        .find(|entry| entry.alias == "renamed")
        .unwrap();
    assert_eq!(evidence.kind, EvidenceKind::Reexport);
    assert_eq!(evidence.resolved_package, "real-package");
    assert_eq!((evidence.line, evidence.column), (2, 9));
    assert_eq!(evidence.source_target, DependencyKind::Normal);
    assert!(serde_json::to_string(&report).unwrap().contains("REEXPORT"));
}

#[test]
fn precision_unsupported_resolution_fails_closed_without_blanket_macro_gaps() {
    let dir = fixture();
    for source in ["use crate::local::*;", "fn run<T>() {}", "paste! { item };"] {
        write(dir.path(), "src/lib.rs", source);
        let report = analyze_machete(&precise(dir.path())).unwrap();
        assert_eq!(
            dependency_status(&report, "unused"),
            DependencyStatus::Incomplete
        );
        assert_ne!(report.coverage.gaps.len(), 0);
    }
    write(
        dir.path(),
        "src/lib.rs",
        "fn run() { println!(\"hello\"); used::run(); }",
    );
    let report = analyze_machete(&precise(dir.path())).unwrap();
    assert_eq!(
        dependency_status(&report, "unused"),
        DependencyStatus::Unused
    );
}

#[test]
fn profiles_expand_features_cfg_attr_and_never_plan_global_removal() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n[dependencies]\nused='1.0.0'\nunused={version='1.0.0',optional=true}\n[features]\ndefault=['group']\ngroup=['dep:unused']\n",
    );
    write(
        dir.path(),
        "clippier.toml",
        "[machete.profiles.minimal]\ndefault-features=false\nfeatures=[]\n[machete.profiles.full]\nfeatures=['special']\n",
    );
    write(
        dir.path(),
        "src/lib.rs",
        "#[cfg_attr(feature=\"special\", cfg(feature=\"missing\"))]\nfn first(){ used::run(); }\n#[cfg(feature=\"special\")] fn second(){ unused::run(); }",
    );
    let mut options = precise(dir.path());
    options.profile = Some("minimal".into());
    let plan = plan_machete(&options).unwrap();
    assert_eq!(
        dependency_status(&plan.report, "used"),
        DependencyStatus::Used
    );
    assert_eq!(
        dependency_status(&plan.report, "unused"),
        DependencyStatus::Incomplete
    );
    assert!(plan.changes.is_empty());
    assert!(plan.report.coverage.profile_limited);
    options.profile = Some("full".into());
    let report = analyze_machete(&options).unwrap();
    assert_eq!(
        dependency_status(&report, "used"),
        DependencyStatus::Incomplete
    );
    assert_eq!(dependency_status(&report, "unused"), DependencyStatus::Used);
    assert!(
        report
            .evidence
            .iter()
            .any(|e| e.alias == "unused" && e.kind == EvidenceKind::Feature)
    );
}

#[test]
fn three_valued_cfg_does_not_infer_host_or_multivalued_negation() {
    let profile = AnalysisProfile {
        features: vec!["selected".into()],
        cfg: vec![
            "target_os=\"linux\"".into(),
            "target_feature=\"sse\"".into(),
        ],
        ..Default::default()
    };
    assert_eq!(
        evaluate_condition("all(feature=\"selected\", target_os=\"linux\")", &profile),
        ConditionValue::True
    );
    assert_eq!(
        evaluate_condition("any(feature=\"missing\", custom)", &profile),
        ConditionValue::Unknown
    );
    assert_eq!(
        evaluate_condition("target_os=\"windows\"", &profile),
        ConditionValue::False
    );
    assert_eq!(
        evaluate_condition("target_feature=\"avx\"", &profile),
        ConditionValue::Unknown
    );
    assert_eq!(
        evaluate_condition(
            "target_has_atomic=\"64\"",
            &AnalysisProfile {
                cfg: vec!["target_has_atomic=\"8\"".into()],
                ..Default::default()
            }
        ),
        ConditionValue::Unknown
    );
    assert_eq!(
        evaluate_condition("unix", &profile),
        ConditionValue::Unknown
    );
}

#[test]
fn precision_dependency_kind_ownership_does_not_credit_production_to_dev() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n[dependencies]\nused='1.0.0'\n[dev-dependencies]\nused='1.0.0'\nunused='1.0.0'\n",
    );
    write(
        dir.path(),
        "src/lib.rs",
        "used::run(); #[cfg(test)] mod tests { fn run(){ unused::run(); } }",
    );
    let mut options = precise(dir.path());
    options.kinds = vec![DependencyKind::Normal, DependencyKind::Dev];
    let report = analyze_machete(&options).unwrap();
    assert_eq!(
        report
            .dependencies
            .iter()
            .find(|d| d.dependency == "used" && d.kind == DependencyKind::Dev)
            .unwrap()
            .status,
        DependencyStatus::Unused
    );
    let development = report
        .evidence
        .iter()
        .find(|e| e.alias == "unused")
        .unwrap();
    assert_eq!(development.source_target, DependencyKind::Dev);
}

#[test]
fn generated_evidence_is_owner_scoped_and_all_fingerprints_seal_plan() {
    use generated::{GeneratedEvidence, GeneratedSource, GeneratedTargetRole, InputFingerprint};
    let dir = fixture();
    write(dir.path(), "generated/output.rs", "unused::generated();");
    let root = dir.path().canonicalize().unwrap();
    let capture = |path: &str| InputFingerprint::capture(&root, Path::new(path)).unwrap();
    let mut options = precise(dir.path());
    options.generated = Some(GeneratedEvidence {
        sources: vec![GeneratedSource {
            owner_manifest: "Cargo.toml".into(),
            target_role: GeneratedTargetRole::Library,
            profile: "capture".into(),
            output: capture("generated/output.rs"),
            inputs: vec![capture("Cargo.toml")],
        }],
    });
    let plan = plan_machete(&options).unwrap();
    assert_eq!(
        dependency_status(&plan.report, "unused"),
        DependencyStatus::Used
    );
    assert!(
        plan.report
            .evidence
            .iter()
            .any(|e| e.kind == EvidenceKind::Generated && !e.uncertainty.is_empty())
    );
    write(dir.path(), "generated/output.rs", "changed bytes");
    assert!(apply_machete_plan(&plan).is_err());
    assert!(analyze_machete(&options).is_err());
}

#[test]
fn audit_discovery_keeps_preselector_inventory_and_ignored_source_usage() {
    let dir = fixture();
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nignored=['used']\n[[machete.rules]]\npackages=['fixture']\nignored=['unused']\nreason='reviewed requirement'\nreview-after='2099-01-01'\n",
    );
    let mut options = precise(dir.path());
    options.dependencies = vec!["unused".into()];
    let report = audit_machete(&options).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == audit::AuditKind::UnmatchedIgnore)
    );
}

#[test]
fn precision_target_labels_use_custom_manifest_roles_not_directory_guesses() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n[lib]\nname='custom_lib'\npath='custom/root.rs'\n[[example]]\nname='demo'\npath='custom/demo.rs'\n[dependencies]\nused='1.0.0'\n",
    );
    write(dir.path(), "custom/root.rs", "used::run();");
    write(dir.path(), "custom/demo.rs", "used::run();");
    let report = analyze_machete(&precise(dir.path())).unwrap();
    assert!(
        report
            .evidence
            .iter()
            .any(|entry| entry.source_target_role == "LIBRARY"
                && entry.source_target_name.as_deref() == Some("custom_lib"))
    );
    assert!(
        report
            .evidence
            .iter()
            .any(|entry| entry.source_target_role == "EXAMPLE"
                && entry.source_target_name.as_deref() == Some("demo"))
    );
}

#[test]
fn precision_development_only_reference_reaches_hygiene_recommendation() {
    let dir = fixture();
    write(dir.path(), "src/lib.rs", "pub fn empty() {}");
    write(dir.path(), "tests/check.rs", "used::run();");
    let report = audit_machete(&precise(dir.path())).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == audit::AuditKind::NormalToDev)
    );
}

#[test]
fn audit_rule_dependency_intersection_is_occurrence_scoped() {
    let dir = fixture();
    write(
        dir.path(),
        "clippier.toml",
        "[[machete.rules]]\ndependencies=['unused']\nignored=['used']\nreason='narrow rule'\n",
    );
    let report = audit_machete(&precise(dir.path())).unwrap();
    assert!(report.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == audit::AuditKind::UnmatchedIgnore
        && diagnostic.location.dependency.as_deref() == Some("used")));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == audit::AuditKind::IgnoredButUsed)
    );
}

#[test]
fn profile_inner_cfg_does_not_erase_following_outer_source() {
    let dir = fixture();
    write(
        dir.path(),
        "clippier.toml",
        "[machete.profiles.minimal]\ndefault-features=false\n",
    );
    write(
        dir.path(),
        "src/lib.rs",
        "mod nested { #![cfg(feature=\"disabled\")] unused::run(); }\nused::run();",
    );
    let mut options = precise(dir.path());
    options.profile = Some("minimal".into());
    let report = analyze_machete(&options).unwrap();
    assert_eq!(dependency_status(&report, "used"), DependencyStatus::Used);
    assert_eq!(
        dependency_status(&report, "unused"),
        DependencyStatus::Incomplete
    );
}

#[test]
fn audit_review_date_is_explicit_and_rule_deadline_is_preserved() {
    let dir = fixture();
    write(
        dir.path(),
        "clippier.toml",
        "[[machete.rules]]\nignored=['unused']\nreason='temporary retention'\nreview-after='2026-01-01'\n",
    );
    let mut options = precise(dir.path());
    options.review_date = Some("2026-10-16".into());
    let report = audit_machete(&options).unwrap();
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == audit::AuditKind::SuppressionReviewOverdue)
    );
}

#[test]
fn audit_inherited_feature_overrides_and_optional_normal_do_not_imply_redundancy() {
    let dir = fixture();
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers=['member']\n[workspace.dependencies]\nused='1.0.0'\n",
    );
    write(
        dir.path(),
        "member/Cargo.toml",
        "[package]\nname='member'\nversion='0.1.0'\n[dependencies]\nused={workspace=true,optional=true,features=['one']}\n[dev-dependencies]\nused={workspace=true,features=['two']}\n",
    );
    write(dir.path(), "member/src/lib.rs", "used::run();");
    let mut options = precise(dir.path());
    options.kinds = vec![DependencyKind::Normal, DependencyKind::Dev];
    let report = audit_machete(&options).unwrap();
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == audit::AuditKind::RedundantOccurrence)
    );
}

#[test]
fn precision_conditional_shadow_and_macro_token_bindings_do_not_erase_usage() {
    let dir = fixture();
    for source in [
        "macro_rules! tokens { () => { mod used {} } } used::run();",
        "#[cfg(custom)] use other as used; used::run();",
        "use crate::local::Thing as used; used::run();",
    ] {
        write(dir.path(), "src/lib.rs", source);
        let report = analyze_machete(&precise(dir.path())).unwrap();
        assert_ne!(
            dependency_status(&report, "used"),
            DependencyStatus::Unused,
            "{source}"
        );
    }
}

fn save_generated_capture(root: &Path, json: &Path, dependency: &str) {
    use generated::{GeneratedEvidence, GeneratedSource, GeneratedTargetRole, InputFingerprint};
    write(
        root,
        "generated/captured.rs",
        &format!("{dependency}::generated();"),
    );
    let capture = |path: &str| InputFingerprint::capture(root, Path::new(path)).unwrap();
    let evidence = GeneratedEvidence {
        sources: vec![GeneratedSource {
            owner_manifest: "Cargo.toml".into(),
            target_role: GeneratedTargetRole::Library,
            profile: "capture".into(),
            output: capture("generated/captured.rs"),
            inputs: vec![capture("Cargo.toml")],
        }],
    };
    fs::create_dir_all(json.parent().unwrap()).unwrap();
    fs::write(json, serde_json::to_vec(&evidence).unwrap()).unwrap();
}

#[test]
fn config_evidence_path_resolves_from_ancestor_and_json_bytes_seal_plan() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("project");
    write(
        &root,
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n[dependencies]\nused='1.0.0'\nunused='1.0.0'\n",
    );
    write(&root, "src/lib.rs", "used::run();");
    write(
        parent.path(),
        "clippier.toml",
        "[machete]\nevidence-path='captures/evidence.json'\n",
    );
    let json = parent.path().join("captures/evidence.json");
    save_generated_capture(&root, &json, "unused");
    let plan = plan_machete(&precise(&root)).unwrap();
    assert_eq!(
        dependency_status(&plan.report, "unused"),
        DependencyStatus::Used
    );
    assert!(plan.snapshots.contains_key(&json));
    fs::write(&json, "{\"sources\":[]}").unwrap();
    assert!(apply_machete_plan(&plan).is_err());
}

#[test]
fn configured_capture_absence_is_snapshotted_and_arrival_invalidates_apply() {
    let dir = fixture();
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nevidence-path='capture.json'\n",
    );
    let plan = plan_machete(&precise(dir.path())).unwrap();
    assert_eq!(
        plan.snapshots.get(&dir.path().join("capture.json")),
        Some(&None)
    );
    write(dir.path(), "capture.json", "{\"sources\":[]}");
    assert!(apply_machete_plan(&plan).is_err());
}

#[test]
fn child_and_explicit_evidence_config_override_parent_and_options_override_all() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("project");
    write(
        &root,
        "Cargo.toml",
        "[package]\nname='fixture'\nversion='0.1.0'\n[dependencies]\nunused='1.0.0'\n",
    );
    write(&root, "src/lib.rs", "pub fn empty() {}");
    write(
        parent.path(),
        "clippier.toml",
        "[machete]\nevidence-path='bad.json'\n",
    );
    write(parent.path(), "bad.json", "not json");
    write(
        &root,
        "clippier.toml",
        "[machete]\nevidence-path='child.json'\n",
    );
    write(&root, "child.json", "{\"sources\":[]}");
    let plan = plan_machete(&precise(&root)).unwrap();
    assert!(!plan.snapshots.contains_key(&parent.path().join("bad.json")));
    let config = parent.path().join("policy/explicit.toml");
    write(
        parent.path(),
        "policy/explicit.toml",
        "[machete]\nevidence-path='capture.json'\n",
    );
    save_generated_capture(&root, &parent.path().join("policy/capture.json"), "unused");
    let mut options = precise(&root);
    options.config_path = Some(config);
    assert_eq!(
        dependency_status(&analyze_machete(&options).unwrap(), "unused"),
        DependencyStatus::Used
    );
    write(parent.path(), "policy/capture.json", "invalid json");
    assert!(analyze_machete(&options).is_err());
    options.generated = Some(generated::GeneratedEvidence::default());
    let plan = plan_machete(&options).unwrap();
    assert_eq!(
        dependency_status(&plan.report, "unused"),
        DependencyStatus::Unused
    );
    assert!(
        !plan
            .snapshots
            .contains_key(&parent.path().join("policy/capture.json"))
    );
}

#[test]
fn configured_generated_capture_revalidates_source_fingerprints() {
    let dir = fixture();
    write(
        dir.path(),
        "clippier.toml",
        "[machete]\nevidence-path='capture.json'\n",
    );
    save_generated_capture(dir.path(), &dir.path().join("capture.json"), "unused");
    assert_eq!(
        dependency_status(&analyze_machete(&precise(dir.path())).unwrap(), "unused"),
        DependencyStatus::Used
    );
    write(dir.path(), "generated/captured.rs", "changed");
    assert!(analyze_machete(&precise(dir.path())).is_err());
}
