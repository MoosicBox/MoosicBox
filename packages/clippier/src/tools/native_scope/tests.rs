use super::*;

#[path = "python_tests.rs"]
mod python_tests;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "clippier-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn scope(&self, tool: &str, args: &[&str]) -> NativeScopeSet {
        let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        NativeScopeSet::resolve(&self.0, &[(tool, &args)]).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn prettier_parent_negation_and_independent_ignore_union() {
    let dir = Fixture::new();
    dir.write(
        ".prettierignore",
        "build/\n!build/keep.js\noutput/*\n!output/keep.js\n",
    );
    dir.write(".gitignore", "output/keep.js\n");
    let mut scope = dir.scope("prettier", &[]);
    assert!(
        !scope
            .allows("prettier", Path::new("build/keep.js"))
            .unwrap()
    );
    assert!(
        !scope
            .allows("prettier", Path::new("output/keep.js"))
            .unwrap()
    );
    assert!(scope.can_prune("prettier", Path::new("build")));
    assert!(!scope.can_prune("prettier", Path::new("output")));
    assert!(
        !scope
            .allows("prettier", Path::new("src/node_modules/pkg/a.js"))
            .unwrap()
    );
    assert_eq!(scope.configs_parsed(), 2);
    let mut scope = dir.scope(
        "prettier",
        &["--ignore-path", ".prettierignore", "--with-node-modules"],
    );
    assert!(
        scope
            .allows("prettier", Path::new("output/keep.js"))
            .unwrap()
    );
    assert!(
        scope
            .allows("prettier", Path::new("src/node_modules/pkg/a.js"))
            .unwrap()
    );
}

#[test]
fn prettier_explicit_ignore_path_uses_its_parent() {
    let dir = Fixture::new();
    dir.write("config/ignore", "/generated/\n");
    let mut scope = dir.scope("prettier", &["--ignore-path=config/ignore"]);
    assert!(
        !scope
            .allows("prettier", Path::new("config/generated/a.js"))
            .unwrap()
    );
    assert!(
        scope
            .allows("prettier", Path::new("generated/a.js"))
            .unwrap()
    );
}

#[test]
fn taplo_empty_missing_and_rule_selectors_are_distinct() {
    let dir = Fixture::new();
    dir.write("taplo.toml", "exclude = ['generated/**']\n[[rule]]\ninclude = ['src/**']\nexclude = ['src/private/**']\n");
    let mut scope = dir.scope("taplo", &[]);
    assert!(
        scope
            .allows("taplo", Path::new("src/private/a.toml"))
            .unwrap()
    );
    assert!(
        !scope
            .allows("taplo", Path::new("generated/a.toml"))
            .unwrap()
    );
    assert!(!scope.allows("taplo", Path::new("a.js")).unwrap());
    assert!(!scope.can_prune("taplo", Path::new("generated")));
    dir.write("taplo.toml", "include = []\n");
    assert!(
        !dir.scope("taplo", &[])
            .allows("taplo", Path::new("a.toml"))
            .unwrap()
    );
}

#[test]
fn taplo_explicit_config_remains_invocation_relative() {
    let dir = Fixture::new();
    dir.write("config/taplo.toml", "include = ['src/*.toml']\n");
    let mut scope = dir.scope("taplo", &["--config", "config/taplo.toml"]);
    assert!(scope.allows("taplo", Path::new("src/a.toml")).unwrap());
    assert!(
        !scope
            .allows("taplo", Path::new("config/src/a.toml"))
            .unwrap()
    );
}

#[test]
fn rustfmt_ancestor_ignore_is_reported_not_silently_activated() {
    let dir = Fixture::new();
    dir.write("rustfmt.toml", "ignore = ['src']\n");
    std::fs::create_dir_all(dir.0.join("src")).unwrap();
    let mut scope = NativeScopeSet::resolve(&dir.0.join("src"), &[("rustfmt", &[])]).unwrap();
    assert!(scope.allows("rustfmt", Path::new("main.rs")).unwrap());
    assert_eq!(scope.configs_parsed(), 1);
    assert!(
        scope
            .diagnostics()
            .iter()
            .any(|line| line.contains("unstable") && line.contains("stable"))
    );
}

#[test]
fn biome_capability_scopes_and_negation_are_not_shared() {
    let dir = Fixture::new();
    dir.write("biome.json", r#"{"files":{"includes":["**/*.js", "!**/generated/**", "**/generated/keep.js"]},"formatter":{"includes":["src/**"]},"linter":{"includes":["test/**"]}}"#);
    let mut format = dir.scope("biome", &["format"]);
    assert!(format.allows("biome", Path::new("src/a.js")).unwrap());
    assert!(!format.allows("biome", Path::new("test/a.js")).unwrap());
    assert!(
        !format
            .allows("biome", Path::new("src/generated/a.js"))
            .unwrap()
    );
    assert!(
        format
            .allows("biome", Path::new("src/generated/keep.js"))
            .unwrap()
    );
    let mut lint = dir.scope("biome", &["lint"]);
    assert!(lint.allows("biome", Path::new("test/a.js")).unwrap());
    assert!(!lint.allows("biome", Path::new("src/a.js")).unwrap());
    assert!(
        dir.scope("biome", &["check"])
            .allows("biome", Path::new("src/a.js"))
            .unwrap()
    );
    assert_eq!(format.configs_parsed(), 1);
}

#[test]
fn unknown_biome_patterns_and_extends_never_overexclude() {
    let dir = Fixture::new();
    dir.write(
        "biome.json",
        r#"{"files":{"includes":["**", "!!generated/**"]}}"#,
    );
    let mut scope = dir.scope("biome", &["format"]);
    assert!(scope.allows("biome", Path::new("generated/a.js")).unwrap());
    assert!(
        scope
            .diagnostics()
            .iter()
            .any(|line| line.contains("glob syntax delegated"))
    );
    dir.write(
        "biome.json",
        r#"{"extends":["remote"],"files":{"includes":[]}}"#,
    );
    assert!(
        dir.scope("biome", &["format"])
            .allows("biome", Path::new("a.js"))
            .unwrap()
    );
}

#[test]
fn nested_json_config_does_not_inherit_unsafe_parent_exclusion() {
    for tool in ["biome", "dprint"] {
        let dir = Fixture::new();
        dir.write(
            &format!("{tool}.json"),
            if tool == "biome" {
                r#"{"files":{"includes":[]}}"#
            } else {
                r#"{"includes":[]}"#
            },
        );
        dir.write(&format!("nested/{tool}.json"), "{}");
        let mut scope = dir.scope(tool, &["format"]);
        assert!(scope.allows(tool, Path::new("nested/a.js")).unwrap());
        assert!(!scope.can_prune(tool, Path::new("nested")));
        assert_eq!(scope.configs_parsed(), 2);
    }
}

#[test]
fn dprint_config_relative_parent_matching_and_empty_includes() {
    let dir = Fixture::new();
    dir.write(
        "config/dprint.jsonc",
        "{ // hello\n\"includes\": [\"**/*.ts\",], \"excludes\": [\"generated/\"],}",
    );
    let mut scope = dir.scope("dprint", &["--config=config/dprint.jsonc"]);
    assert!(
        scope
            .allows("dprint", Path::new("config/src/a.ts"))
            .unwrap()
    );
    assert!(
        !scope
            .allows("dprint", Path::new("config/generated/a.ts"))
            .unwrap()
    );
    assert!(
        !scope
            .allows("dprint", Path::new("config/src/a.js"))
            .unwrap()
    );
    dir.write("dprint.json", r#"{"includes":[]}"#);
    assert!(
        !dir.scope("dprint", &[])
            .allows("dprint", Path::new("a.ts"))
            .unwrap()
    );
}

#[test]
fn malformed_configs_and_known_selector_types_fail() {
    for (tool, file, content) in [
        ("taplo", "taplo.toml", "include = [1]"),
        ("taplo", "taplo.toml", "include = ['[']"),
        ("biome", "biome.json", r#"{"files":{"includes":false}}"#),
        ("dprint", "dprint.json", r#"{"excludes":[42]}"#),
        ("rustfmt", "rustfmt.toml", "ignore = false"),
        ("biome", "biome.jsonc", "{/* unclosed"),
    ] {
        let dir = Fixture::new();
        dir.write(file, content);
        let error = NativeScopeSet::resolve(&dir.0, &[(tool, &[])])
            .err()
            .expect("must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{file}: {error}");
    }
}

#[test]
fn json_comments_do_not_corrupt_strings() {
    let parsed =
        jsonc::parse("{\"url\": \"https://example.test/*foo*/\", /* x */ \"list\":[1,],}").unwrap();
    assert_eq!(parsed["url"], "https://example.test/*foo*/");
    assert_eq!(parsed["list"], serde_json::json!([1]));
    assert!(jsonc::parse("{/*").is_err());
}

#[test]
fn catalog_fallback_has_explicit_support_diagnostics() {
    let dir = Fixture::new();
    let mut scope = dir.scope("eslint", &[]);
    assert!(scope.allows("eslint", Path::new("a.js")).unwrap());
    assert!(!scope.can_prune("eslint", Path::new("src")));
    assert!(scope.diagnostics()[0].contains("delegated"));
}

#[test]
fn shared_cache_reuses_parsing_and_rejects_changed_content() {
    let dir = Fixture::new();
    dir.write("biome.json", r#"{"files":{"includes":["**"]}}"#);
    let cache = NativeScopeCache::default();
    let format = vec!["format".to_owned()];
    let lint = vec!["lint".to_owned()];
    let a =
        NativeScopeSet::resolve_with_cache(&dir.0, &[("biome", &format)], cache.clone()).unwrap();
    let b = NativeScopeSet::resolve_with_cache(&dir.0, &[("biome", &lint)], cache).unwrap();
    assert_eq!(a.configs_parsed(), 1);
    assert_eq!(b.configs_parsed(), 1);
    a.validate_snapshot().unwrap();
    dir.write("biome.json", "{}");
    assert!(a.validate_snapshot().is_err());
    assert!(b.validate_snapshot().is_err());
}

#[test]
fn snapshots_detect_precedence_and_absent_ignore_changes() {
    let dir = Fixture::new();
    dir.write("taplo.toml", "include = []");
    let taplo = dir.scope("taplo", &[]);
    taplo.validate_snapshot().unwrap();
    dir.write(".taplo.toml", "include = ['**/*.toml']");
    assert!(taplo.validate_snapshot().is_err());
    let prettier = dir.scope("prettier", &[]);
    prettier.validate_snapshot().unwrap();
    dir.write(".prettierignore", "generated/");
    assert!(prettier.validate_snapshot().is_err());
}

#[test]
fn rustfmt_native_parent_match_differs_from_prettier_parent_exclusion() {
    let dir = Fixture::new();
    let mut builder = GitignoreBuilder::new(&dir.0);
    for pattern in ["foo.rs", "bar_dir/*", "!bar_dir/*/what.rs"] {
        builder.add_line(None, pattern).unwrap();
    }
    let matcher = builder.build().unwrap();
    assert!(
        matcher
            .matched_path_or_any_parents(dir.0.join("src/foo.rs"), false)
            .is_ignore()
    );
    assert!(
        !matcher
            .matched_path_or_any_parents(dir.0.join("bar_dir/baz/what.rs"), false)
            .is_ignore()
    );
}

#[test]
fn biome_empty_includes_survive_registry_vcs_flags() {
    let dir = Fixture::new();
    dir.write("biome.json", r#"{"files":{"includes":[]}}"#);
    for vcs in ["--vcs-enabled=true", "--vcs-enabled=false"] {
        let mut scope = dir.scope(
            "biome",
            &[
                "lint",
                "--vcs-root",
                dir.0.to_str().unwrap(),
                vcs,
                "--vcs-use-ignore-file=false",
            ],
        );
        assert!(!scope.allows("biome", Path::new("a.js")).unwrap());
    }
}

#[test]
fn malformed_python_selectors_fail_instead_of_bypassing_explicit_files() {
    for (tool, selector) in [("ruff", "exclude = false"), ("black", "exclude = []")] {
        let dir = Fixture::new();
        dir.write("pyproject.toml", &format!("[tool.{tool}]\n{selector}\n"));
        let args = vec!["check".to_owned()];
        let error = NativeScopeSet::resolve(&dir.0, &[(tool, &args)])
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}

#[test]
fn dprint_cli_overrides_and_negations_are_explicitly_delegated() {
    let dir = Fixture::new();
    dir.write(
        "dprint.json",
        r#"{"excludes":["generated/", "!generated/keep.ts"]}"#,
    );
    let mut scope = dir.scope("dprint", &[]);
    assert!(
        scope
            .allows("dprint", Path::new("generated/keep.ts"))
            .unwrap()
    );
    assert!(
        scope
            .diagnostics()
            .iter()
            .any(|line| line.contains("negated excludes delegated"))
    );
    dir.write("dprint.json", r#"{"includes":[]}"#);
    let mut scope = dir.scope("dprint", &["--excludes=**/*.js"]);
    assert!(scope.allows("dprint", Path::new("a.ts")).unwrap());
    assert!(
        scope
            .diagnostics()
            .iter()
            .any(|line| line.contains("CLI traversal/pattern overrides delegated"))
    );
}

#[test]
#[cfg(unix)]
fn biome_real_registry_support_flags_preserve_capability_selectors() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Fixture::new();
    dir.write(
        "biome.json",
        r#"{"formatter":{"includes":[]},"linter":{"includes":["**/*.js"]}}"#,
    );
    dir.write("fake-biome", "#!/bin/sh\nexit 0\n");
    let executable = dir.0.join("fake-biome");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    for use_vcs in [true, false] {
        let mut config = crate::tools::ToolsConfig {
            biome_use_vcs_ignore: use_vcs,
            ..crate::tools::ToolsConfig::default()
        };
        config.tools.insert(
            "biome".into(),
            crate::tools::ToolPolicy {
                executable: Some(executable.to_string_lossy().into_owned()),
                ..Default::default()
            },
        );
        let registry = crate::tools::ToolRegistry::new(config, Some(&dir.0)).unwrap();
        let tool = registry.get("biome").unwrap();
        assert!(
            tool.format_args
                .iter()
                .any(|arg| arg == "--files-ignore-unknown=true")
        );
        let mut scope = NativeScopeSet::resolve(&dir.0, &[("biome", &tool.format_args)]).unwrap();
        assert!(!scope.allows("biome", Path::new("source.js")).unwrap());
        assert!(
            !scope
                .diagnostics()
                .iter()
                .any(|line| line.contains("CLI files/enabled/VCS overrides delegated"))
        );
    }
}

#[test]
fn explicit_json_configs_do_not_adopt_nested_configuration() {
    for tool in ["biome", "dprint"] {
        let dir = Fixture::new();
        dir.write(
            &format!("{tool}.json"),
            if tool == "biome" {
                r#"{"formatter":{"includes":[]}}"#
            } else {
                r#"{"includes":[]}"#
            },
        );
        dir.write(&format!("child/{tool}.json"), "invalid JSON");
        let flag = if tool == "biome" {
            "--config-path"
        } else {
            "--config"
        };
        let config = format!("{tool}.json");
        let mut scope = dir.scope(tool, &["format", flag, &config]);
        assert!(!scope.allows(tool, Path::new("child/a.js")).unwrap());
        assert_eq!(scope.configs_parsed(), 1);
    }
}
