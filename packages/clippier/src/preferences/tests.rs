use super::*;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n",
    )
    .unwrap();
    root
}

fn write_config(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn resolve(root: &Path, environment: &EnvironmentInputs) -> EffectivePreferences {
    resolve_with_inputs(root, PreferencesOverrides::default(), environment).unwrap()
}

#[test]
fn policy_has_matching_cli_config_and_display_names() {
    use clap::ValueEnum;

    assert_eq!(PurityPolicy::default(), PurityPolicy::Warn);
    for policy in [PurityPolicy::Warn, PurityPolicy::RequirePure] {
        let name = policy.to_string();
        assert_eq!(name.parse::<PurityPolicy>().unwrap(), policy);
        assert_eq!(policy.to_possible_value().unwrap().get_name(), name);
        assert_eq!(serde_json::to_value(policy).unwrap(), name);
        assert_eq!(
            serde_json::from_value::<PurityPolicy>(name.into()).unwrap(),
            policy
        );
    }
    for invalid in ["", "WARN", "require_pure", " warn", "strict"] {
        assert!(invalid.parse::<PurityPolicy>().is_err());
    }
}

#[test]
fn missing_files_use_builtin_default_and_report_provenance() {
    let root = fixture();
    let effective = resolve(root.path(), &EnvironmentInputs::default());
    assert_eq!(effective.publish.purity, PurityPolicy::Warn);
    assert_eq!(
        effective.provenance.publish.purity,
        PreferenceSource::BuiltIn
    );
    assert!(
        effective
            .to_raw_string()
            .contains("publish.purity = warn (built-in)")
    );
    let json = serde_json::to_value(&effective).unwrap();
    assert_eq!(json["publish"]["purity"], "warn");
    assert_eq!(
        json["provenance"]["publish"]["purity"]["source"],
        "BUILT_IN"
    );
}

#[test]
fn precedence_and_each_origin_are_preserved() {
    let root = fixture();
    let xdg = root.path().join("xdg");
    let user_path = xdg.join("clippier/clippier.toml");
    let workspace_path = root.path().join("clippier.toml");
    let mut environment = EnvironmentInputs {
        xdg_config_home: Some(xdg.into_os_string()),
        ..EnvironmentInputs::default()
    };
    write_config(&user_path, "[publish]\npurity = 'require-pure'\n");
    let effective = resolve(root.path(), &environment);
    assert_eq!(effective.publish.purity, PurityPolicy::RequirePure);
    assert_eq!(
        effective.provenance.publish.purity,
        PreferenceSource::UserConfig { path: user_path }
    );

    write_config(&workspace_path, "[publish]\npurity = 'warn'\n");
    let effective = resolve(root.path(), &environment);
    assert_eq!(effective.publish.purity, PurityPolicy::Warn);
    assert_eq!(
        effective.provenance.publish.purity,
        PreferenceSource::WorkspaceConfig {
            path: workspace_path
        }
    );

    environment.purity = Some("require-pure".into());
    let effective = resolve(root.path(), &environment);
    assert_eq!(effective.publish.purity, PurityPolicy::RequirePure);
    assert_eq!(
        effective.provenance.publish.purity,
        PreferenceSource::Environment {
            variable: "CLIPPIER_PUBLISH_PURITY".into()
        }
    );

    let effective = resolve_with_inputs(
        root.path(),
        PreferencesOverrides {
            purity: Some(PurityPolicy::Warn),
        },
        &environment,
    )
    .unwrap();
    assert_eq!(effective.publish.purity, PurityPolicy::Warn);
    assert_eq!(
        effective.provenance.publish.purity,
        PreferenceSource::Override
    );
}

#[test]
fn unspecified_workspace_value_does_not_reset_user_value() {
    let root = fixture();
    let home = root.path().join("home");
    write_config(
        &home.join(".config/clippier/clippier.toml"),
        "[publish]\npurity = 'require-pure'\n",
    );
    write_config(
        &root.path().join("clippier.toml"),
        "[publish]\n[existing]\nvalue = 1\n",
    );
    let effective = resolve(
        root.path(),
        &EnvironmentInputs {
            home: Some(home.into_os_string()),
            ..EnvironmentInputs::default()
        },
    );
    assert_eq!(effective.publish.purity, PurityPolicy::RequirePure);
    assert!(matches!(
        effective.provenance.publish.purity,
        PreferenceSource::UserConfig { .. }
    ));
}

#[test]
fn user_path_obeys_absolute_xdg_and_home_fallback() {
    let home = PathBuf::from("/test/home");
    for xdg in [
        None,
        Some(OsString::from("")),
        Some(OsString::from("relative")),
    ] {
        let environment = EnvironmentInputs {
            xdg_config_home: xdg,
            home: Some(home.clone().into_os_string()),
            purity: None,
        };
        assert_eq!(
            environment.user_config_path().unwrap(),
            home.join(".config/clippier/clippier.toml")
        );
    }
    let environment = EnvironmentInputs {
        xdg_config_home: Some("/test/xdg".into()),
        home: Some(home.into_os_string()),
        purity: None,
    };
    assert_eq!(
        environment.user_config_path().unwrap(),
        PathBuf::from("/test/xdg/clippier/clippier.toml")
    );
    assert!(EnvironmentInputs::default().user_config_path().is_none());
    assert!(
        EnvironmentInputs {
            xdg_config_home: Some("relative".into()),
            ..EnvironmentInputs::default()
        }
        .user_config_path()
        .is_none()
    );
}

#[test]
fn absent_xdg_file_does_not_read_home_file() {
    let root = fixture();
    let home = root.path().join("home");
    write_config(
        &home.join(".config/clippier/clippier.toml"),
        "[publish]\npurity = 'require-pure'\n",
    );
    let environment = EnvironmentInputs {
        xdg_config_home: Some(root.path().join("missing-xdg").into_os_string()),
        home: Some(home.into_os_string()),
        purity: None,
    };
    assert_eq!(
        resolve(root.path(), &environment).publish.purity,
        PurityPolicy::Warn
    );
}

#[test]
fn validates_owned_namespace_but_ignores_unrelated_namespaces() {
    let root = fixture();
    let path = root.path().join("clippier.toml");
    for invalid in [
        "[publish]\npurity = 'strict'",
        "[publish]\npurity = 1",
        "[publish]\npurity = true",
        "[publish]\npurtiy = 'warn'",
        "publish = 'warn'",
        "[publish.commands]\nrun = 'anything'",
        "[publish]\nenv = { SECRET = 'value' }",
    ] {
        write_config(&path, invalid);
        let error = resolve_with_inputs(
            root.path(),
            PreferencesOverrides::default(),
            &EnvironmentInputs::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(&path.display().to_string()), "{error}");
    }
    write_config(
        &path,
        "[unrelated]\ncommands = ['do not execute']\nenv = { SECRET = 'do not install' }\n[formats]\nunknown = true\n[publish]\npurity = 'require-pure'\n",
    );
    assert_eq!(
        resolve(root.path(), &EnvironmentInputs::default())
            .publish
            .purity,
        PurityPolicy::RequirePure
    );
}

#[test]
fn user_configuration_only_extracts_allowlisted_preferences() {
    let root = fixture();
    let xdg = root.path().join("xdg");
    let path = xdg.join("clippier/clippier.toml");
    write_config(
        &path,
        "commands = ['ignored']\n[env]\nCLIPPIER_PUBLISH_PURITY = 'require-pure'\n",
    );
    let environment = EnvironmentInputs {
        xdg_config_home: Some(xdg.into_os_string()),
        ..EnvironmentInputs::default()
    };
    assert_eq!(
        resolve(root.path(), &environment).publish.purity,
        PurityPolicy::Warn
    );
    write_config(&path, "[publish]\ncommand = 'not allowed'\n");
    assert!(
        resolve_with_inputs(root.path(), PreferencesOverrides::default(), &environment).is_err()
    );
}

#[test]
fn malformed_and_unreadable_files_include_path_even_when_overridden() {
    let root = fixture();
    let path = root.path().join("clippier.toml");
    for text in ["[publish", "[publish]\npurity = 'unknown'"] {
        write_config(&path, text);
        let error = resolve_with_inputs(
            root.path(),
            PreferencesOverrides {
                purity: Some(PurityPolicy::Warn),
            },
            &EnvironmentInputs::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains(&path.display().to_string()));
    }
    // Reading a directory as a file fails even when tests run with elevated privileges.
    let directory = root.path().join("config-directory");
    fs::create_dir(&directory).unwrap();
    let error = read_layer(&directory).unwrap_err().to_string();
    assert!(error.contains(&directory.display().to_string()));
}

#[test]
fn invalid_user_file_is_not_hidden_by_workspace_or_cli() {
    let root = fixture();
    let xdg = root.path().join("xdg");
    let path = xdg.join("clippier/clippier.toml");
    write_config(&path, "[publish]\npurity = 'bad'");
    write_config(
        &root.path().join("clippier.toml"),
        "[publish]\npurity = 'warn'",
    );
    let environment = EnvironmentInputs {
        xdg_config_home: Some(xdg.into_os_string()),
        ..EnvironmentInputs::default()
    };
    let error = resolve_with_inputs(
        root.path(),
        PreferencesOverrides {
            purity: Some(PurityPolicy::Warn),
        },
        &environment,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains(&path.display().to_string()));
}

#[test]
fn invalid_environment_errors_even_with_explicit_override() {
    let root = fixture();
    let environment = EnvironmentInputs {
        purity: Some("".into()),
        ..EnvironmentInputs::default()
    };
    let error = resolve_with_inputs(
        root.path(),
        PreferencesOverrides {
            purity: Some(PurityPolicy::Warn),
        },
        &environment,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("CLIPPIER_PUBLISH_PURITY"));
}

#[cfg(unix)]
#[test]
fn non_unicode_environment_reports_variable() {
    use std::os::unix::ffi::OsStringExt;

    let root = fixture();
    let environment = EnvironmentInputs {
        purity: Some(OsString::from_vec(vec![0xff])),
        ..EnvironmentInputs::default()
    };
    let error = resolve_with_inputs(root.path(), PreferencesOverrides::default(), &environment)
        .unwrap_err()
        .to_string();
    assert!(error.contains("CLIPPIER_PUBLISH_PURITY"));
}

#[test]
fn supplied_workspace_manifest_and_nested_paths_use_workspace_config() {
    let root = fixture();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = ['packages/member']\n",
    )
    .unwrap();
    let nested = root.path().join("packages/member/src/deep");
    fs::create_dir_all(&nested).unwrap();
    fs::write(root.path().join("packages/member/src/lib.rs"), "").unwrap();
    let member_manifest = root.path().join("packages/member/Cargo.toml");
    fs::write(
        &member_manifest,
        "[package]\nname = 'member'\nversion = '0.1.0'\n",
    )
    .unwrap();
    write_config(
        &root.path().join("clippier.toml"),
        "[publish]\npurity = 'require-pure'\n",
    );
    write_config(
        &root.path().join("packages/member/clippier.toml"),
        "[publish]\npurity = 'warn'\n",
    );
    for path in [
        root.path().to_path_buf(),
        root.path().join("Cargo.toml"),
        nested,
        member_manifest,
    ] {
        assert_eq!(
            resolve(&path, &EnvironmentInputs::default()).publish.purity,
            PurityPolicy::RequirePure
        );
    }
}

#[test]
fn explicit_package_workspace_path_is_respected() {
    let root = fixture();
    let member = root.path().join("member");
    fs::create_dir_all(member.join("src")).unwrap();
    fs::write(member.join("src/lib.rs"), "").unwrap();
    fs::write(
        member.join("Cargo.toml"),
        "[package]\nname = 'member'\nversion = '0.1.0'\nworkspace = '../other'\n",
    )
    .unwrap();
    write_config(
        &root.path().join("other/Cargo.toml"),
        "[workspace]\nmembers = ['../member']\n",
    );
    write_config(
        &root.path().join("other/clippier.toml"),
        "[publish]\npurity = 'require-pure'\n",
    );
    assert_eq!(
        resolve(&member, &EnvironmentInputs::default())
            .publish
            .purity,
        PurityPolicy::RequirePure
    );
}

#[test]
fn excluded_standalone_package_uses_its_own_preferences() {
    let root = fixture();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\nexclude = ['standalone']\n",
    )
    .unwrap();
    let standalone = root.path().join("standalone");
    fs::create_dir_all(standalone.join("src/deep")).unwrap();
    fs::write(standalone.join("src/lib.rs"), "").unwrap();
    fs::write(
        standalone.join("Cargo.toml"),
        "[package]\nname = 'standalone'\nversion = '0.1.0'\n",
    )
    .unwrap();
    write_config(
        &root.path().join("clippier.toml"),
        "[publish]\npurity = 'require-pure'\n",
    );
    let config = standalone.join("clippier.toml");
    write_config(&config, "[publish]\npurity = 'warn'\n");
    for path in [
        standalone.clone(),
        standalone.join("Cargo.toml"),
        standalone.join("src/deep"),
    ] {
        let effective = resolve(&path, &EnvironmentInputs::default());
        assert_eq!(effective.publish.purity, PurityPolicy::Warn);
        assert_eq!(
            effective.provenance.publish.purity,
            PreferenceSource::WorkspaceConfig {
                path: config.clone()
            }
        );
    }
}

#[test]
fn config_only_directory_without_manifest_uses_supplied_directory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("clippier.toml");
    write_config(&path, "[publish]\npurity = 'require-pure'\n");
    let effective = resolve(root.path(), &EnvironmentInputs::default());
    assert_eq!(effective.publish.purity, PurityPolicy::RequirePure);
    assert_eq!(
        effective.provenance.publish.purity,
        PreferenceSource::WorkspaceConfig { path }
    );
}

#[test]
fn nearest_nested_workspace_wins() {
    let root = fixture();
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    write_config(
        &root.path().join("clippier.toml"),
        "[publish]\npurity = 'require-pure'\n",
    );
    assert_eq!(
        resolve(&nested, &EnvironmentInputs::default())
            .publish
            .purity,
        PurityPolicy::Warn
    );
}
