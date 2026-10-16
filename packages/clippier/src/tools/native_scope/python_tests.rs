use super::{Fixture, NativeScopeSet, Path, PathBuf, io};

#[test]
fn black_default_include_selects_root_and_nested_python_files() {
    let dir = Fixture::new();
    dir.write("pyproject.toml", "[tool.black]\n");
    let mut scope = dir.scope("black", &[]);
    for file in ["root.py", "root.pyi", "nested/root.py", "notebook.ipynb"] {
        assert!(scope.allows("black", Path::new(file)).unwrap(), "{file}");
    }
    for file in ["root.py.txt", "root.txt", "build/root.py", ".venv/root.py"] {
        assert!(!scope.allows("black", Path::new(file)).unwrap(), "{file}");
    }
}

#[test]
fn ruff_explicit_config_prevents_nested_config_adoption() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "include = ['*.py']\nexclude = ['generated']\n");
    dir.write("child/ruff.toml", "this is invalid TOML");
    let mut scope = dir.scope("ruff", &["check", "--config=ruff.toml"]);
    assert!(scope.allows("ruff", Path::new("child/root.py")).unwrap());
    assert!(
        !scope
            .allows("ruff", Path::new("generated/root.py"))
            .unwrap()
    );
    assert_eq!(scope.configs_parsed(), 1);
    scope.validate_snapshot().unwrap();
    assert!(
        scope
            .diagnostics()
            .iter()
            .any(|line| line.contains("recursive intent") && line.contains("force-exclude"))
    );
}

#[test]
fn ruff_nested_unrelated_pyproject_is_not_configuration_but_new_owner_invalidates_snapshot() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "include = ['*.py']\n");
    dir.write("child/pyproject.toml", "[tool.other]\nvalue = true\n");
    let mut scope = dir.scope("ruff", &["check"]);
    assert!(scope.allows("ruff", Path::new("child/root.py")).unwrap());
    scope.validate_snapshot().unwrap();
    dir.write("child/.ruff.toml", "include = []\n");
    assert!(scope.validate_snapshot().is_err());
}

#[test]
fn ruff_malformed_nested_configuration_is_a_parse_error() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "include = ['*.py']\n");
    dir.write("child/ruff.toml", "include = [\n");
    let mut scope = dir.scope("ruff", &["check"]);
    assert_eq!(
        scope
            .allows("ruff", Path::new("child/root.py"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn ruff_defaults_empty_include_extends_and_operation_exclusions() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "include = []\nextend-include = ['*.py']\nexclude = []\nextend-exclude = ['generated']\n[lint]\nexclude = ['lint_only.py']\n[format]\nexclude = ['format_only.py']\n");
    let mut lint = dir.scope("ruff", &["check"]);
    assert!(lint.allows("ruff", Path::new("src/a.py")).unwrap());
    assert!(!lint.allows("ruff", Path::new("src/a.pyi")).unwrap());
    assert!(!lint.allows("ruff", Path::new("generated/a.py")).unwrap());
    assert!(!lint.allows("ruff", Path::new("src/lint_only.py")).unwrap());
    assert!(
        lint.allows("ruff", Path::new("src/format_only.py"))
            .unwrap()
    );
    let mut format = dir.scope("ruff", &["format"]);
    assert!(
        format
            .allows("ruff", Path::new("src/lint_only.py"))
            .unwrap()
    );
    assert!(
        !format
            .allows("ruff", Path::new("src/format_only.py"))
            .unwrap()
    );
    dir.write("ruff.toml", "include = []");
    assert!(
        !dir.scope("ruff", &["check"])
            .allows("ruff", Path::new("a.py"))
            .unwrap()
    );
    dir.write("ruff.toml", "");
    let mut defaults = dir.scope("ruff", &["check"]);
    assert!(defaults.allows("ruff", Path::new("a.py")).unwrap());
    assert!(!defaults.allows("ruff", Path::new(".venv/a.py")).unwrap());
}

#[test]
fn ruff_precedence_unrelated_pyproject_and_explicit_config_root() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "include = []");
    dir.write(".ruff.toml", "include = ['*.py']");
    dir.write("sub/pyproject.toml", "[tool.other]\nx = true");
    let args = vec!["check".to_owned()];
    let mut scope = NativeScopeSet::resolve(&dir.0.join("sub"), &[("ruff", &args)]).unwrap();
    assert!(scope.allows("ruff", Path::new("a.py")).unwrap());
    dir.write("config/ruff.toml", "include = ['src/*.py']");
    let mut explicit = dir.scope("ruff", &["check", "--config=config/ruff.toml"]);
    assert!(explicit.allows("ruff", Path::new("src/a.py")).unwrap());
    assert!(
        !explicit
            .allows("ruff", Path::new("config/src/a.py"))
            .unwrap()
    );
}

#[test]
fn ruff_nested_configs_select_independently_and_inheritance_fails() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "include = []");
    dir.write("child/ruff.toml", "include = ['*.py']");
    let mut scope = dir.scope("ruff", &["check"]);
    assert!(scope.allows("ruff", Path::new("child/a.py")).unwrap());
    assert!(!scope.allows("ruff", Path::new("root.py")).unwrap());
    dir.write("ruff.toml", "extend = './base.toml'");
    assert_eq!(
        NativeScopeSet::resolve(&dir.0, &[("ruff", &["check".to_owned()])])
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn black_recursive_regex_include_and_all_exclusion_layers() {
    let dir = Fixture::new();
    dir.write("pyproject.toml", "[tool.black]\ninclude = 'a\\.py$'\nexclude = '^/generated/'\nextend-exclude = '^/extended/'\nforce-exclude = '^/forced/'\n");
    let mut scope = dir.scope("black", &[]);
    assert!(scope.allows("black", Path::new("src/a.py")).unwrap());
    assert!(!scope.allows("black", Path::new("src/b.py")).unwrap());
    for folder in ["generated", "extended", "forced"] {
        assert!(
            !scope
                .allows("black", &PathBuf::from(folder).join("a.py"))
                .unwrap()
        );
    }
    dir.write(
        "pyproject.toml",
        "[tool.black]\ninclude = ''\nexclude = ''\n",
    );
    let mut scope = dir.scope("black", &[]);
    assert!(scope.allows("black", Path::new(".venv/a.txt")).unwrap());
}

#[test]
fn black_multiline_verbose_and_python_only_regex_fail_actionably() {
    let dir = Fixture::new();
    dir.write(
        "pyproject.toml",
        "[tool.black]\ninclude = '''\n \\.py$ # Python source\n'''\n",
    );
    let mut scope = dir.scope("black", &[]);
    assert!(scope.allows("black", Path::new("a.py")).unwrap());
    assert!(!scope.allows("black", Path::new("a.txt")).unwrap());
    for pattern in ["(?<=generated/)a", "(a)\\1"] {
        dir.write(
            "pyproject.toml",
            &format!("[tool.black]\nexclude = '{pattern}'"),
        );
        let error = NativeScopeSet::resolve(&dir.0, &[("black", &[])])
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        assert!(error.to_string().contains("regex"));
    }
}

#[test]
fn black_external_config_does_not_rebase_project_regexes() {
    let dir = Fixture::new();
    dir.write("pyproject.toml", "[tool.black]\n");
    dir.write(
        "config/other.toml",
        "[tool.black]\nexclude = '^/generated/'\n",
    );
    let mut scope = dir.scope("black", &["--config", "config/other.toml"]);
    assert!(!scope.allows("black", Path::new("generated/a.py")).unwrap());
    assert!(
        scope
            .allows("black", Path::new("config/generated/a.py"))
            .unwrap()
    );
}

#[test]
fn black_native_recursive_differential_when_installed() {
    let black = Path::new("/etc/profiles/per-user/braden/bin/black");
    if !black.is_file() {
        return;
    }
    let dir = Fixture::new();
    dir.write("pyproject.toml", "[tool.black]\ninclude = 'a\\.py$'\nexclude = '^/generated/'\nextend-exclude = '^/extended/'\nforce-exclude = '^/forced/'\n");
    let files = [
        "a.py",
        "b.py",
        "src/a.py",
        "generated/a.py",
        "extended/a.py",
        "forced/a.py",
    ];
    for path in files {
        dir.write(path, "x=1\n");
    }
    let output = std::process::Command::new(black)
        .args(["--check", "--verbose", "."])
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let output = String::from_utf8_lossy(&output.stderr);
    let mut scope = dir.scope("black", &[]);
    for file in files {
        let selected = output.contains(&format!("would reformat {}", dir.0.join(file).display()));
        assert_eq!(
            scope.allows("black", Path::new(file)).unwrap(),
            selected,
            "{file}: {output}"
        );
    }
}

#[test]
fn black_aliases_match_and_collisions_fail() {
    let dir = Fixture::new();
    dir.write(
        "pyproject.toml",
        "[tool.black]\nextend_exclude = '^/generated/'\nforce_exclude = 'forced'\n",
    );
    let mut scope = dir.scope("black", &[]);
    assert!(!scope.allows("black", Path::new("generated/a.py")).unwrap());
    assert!(!scope.allows("black", Path::new("forced.py")).unwrap());
    dir.write(
        "pyproject.toml",
        "[tool.black]\nextend_exclude = 'one'\nextend-exclude = 'two'\n",
    );
    assert_eq!(
        NativeScopeSet::resolve(&dir.0, &[("black", &[])])
            .err()
            .unwrap()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn black_batch_validation_rejects_nested_project_drift() {
    let dir = Fixture::new();
    dir.write("pyproject.toml", "[tool.black]\n");
    dir.write("child/pyproject.toml", "[tool.black]\nline-length = 100\n");
    let mut scope = dir.scope("black", &[]);
    assert!(scope.validate_files("black", &["root.py".into()]).is_ok());
    assert_eq!(
        scope
            .validate_files("black", &["child/a.py".into()])
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );
    let mut explicit = dir.scope("black", &["--config=pyproject.toml"]);
    assert!(
        explicit
            .validate_files("black", &["child/a.py".into()])
            .is_err()
    );
}

#[test]
fn ruff_default_build_directory_is_excluded() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "");
    assert!(
        !dir.scope("ruff", &["check"])
            .allows("ruff", Path::new("build/a.py"))
            .unwrap()
    );
}

#[test]
fn ruff_monorepo_nearest_settings_preserve_defaults_and_operation_roots() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "include = []\n");
    dir.write("packages/python/pyproject.toml", "[tool.ruff]\nline-length = 100\n[tool.ruff.lint]\nexclude = ['src/lint.py']\n[tool.ruff.format]\nexclude = ['src/fmt.py']\n");
    dir.write("packages/python/deeper/.ruff.toml", "include = []\n");
    let mut lint = dir.scope("ruff", &["check"]);
    let mut format = dir.scope("ruff", &["format"]);
    for (path, lint_allows, format_allows) in [
        ("packages/python/src/a.py", true, true),
        ("packages/python/src/lint.py", false, true),
        ("packages/python/src/fmt.py", true, false),
        ("packages/python/deeper/a.py", false, false),
        ("root.py", false, false),
    ] {
        assert_eq!(
            lint.allows("ruff", Path::new(path)).unwrap(),
            lint_allows,
            "{path}"
        );
        assert_eq!(
            format.allows("ruff", Path::new(path)).unwrap(),
            format_allows,
            "{path}"
        );
    }
    assert_eq!(lint.configs_parsed(), 3);
    lint.validate_snapshot().unwrap();
    dir.write("packages/python/deeper/.ruff.toml", "include = ['*.py']");
    assert!(lint.validate_snapshot().is_err());
}

#[test]
fn nested_ruff_inheritance_still_fails_explicitly() {
    let dir = Fixture::new();
    dir.write("ruff.toml", "");
    dir.write("child/ruff.toml", "extend = '../ruff.toml'");
    assert_eq!(
        dir.scope("ruff", &["check"])
            .allows("ruff", Path::new("child/a.py"))
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );
}
