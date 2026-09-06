#[cfg(all(test, feature = "format"))]
mod tests {
    use super::super::recommendations::groups;
    use super::super::{RepositoryDiscovery, ToolsConfig};

    #[test]
    fn installed_fallback_preserves_native_preferences_and_all_choices() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("index.js"), "const x = 1;").unwrap();
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let mut result = groups(&mut inventory, &ToolsConfig::default());
        let js = result
            .iter()
            .find(|group| group.formatting && group.title.contains("JavaScript"))
            .unwrap();
        let alternative = js
            .choices
            .iter()
            .find(|choice| !choice.selected)
            .unwrap()
            .name
            .clone();
        let installed =
            std::collections::BTreeMap::from([(alternative.clone(), root.path().join("bin"))]);
        super::super::recommendations::apply_availability(&mut result, &installed);
        let js = result
            .iter()
            .find(|group| group.formatting && group.title.contains("JavaScript"))
            .unwrap();
        assert!(
            js.choices
                .iter()
                .any(|choice| choice.name == alternative && choice.selected)
        );
        assert!(js.choices.iter().any(|choice| choice.installed.is_none()));
        std::fs::write(root.path().join("biome.json"), "{}").unwrap();
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let mut result = groups(&mut inventory, &ToolsConfig::default());
        let installed = std::collections::BTreeMap::from([(
            "prettier".to_owned(),
            root.path().join("prettier"),
        )]);
        super::super::recommendations::apply_availability(&mut result, &installed);
        assert!(
            result
                .iter()
                .filter(|group| group.formatting)
                .flat_map(|group| &group.choices)
                .any(|choice| choice.name == "biome" && choice.selected)
        );
    }

    #[test]
    fn existing_setup_loads_checked_and_skipped_choices() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("index.js"), "const x = 1;").unwrap();
        let config: ToolsConfig = toml::from_str(
            r#"
required = ["prettier"]
skip = ["biome"]
[tools.prettier]
mode = "enabled"
capabilities = ["format"]
format-extensions = ["js"]
"#,
        )
        .unwrap();
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &config);
        let js = result
            .iter()
            .find(|group| group.formatting && group.extensions == ["js"])
            .unwrap();
        assert!(
            js.choices
                .iter()
                .any(|choice| choice.name == "prettier" && choice.selected)
        );
        assert!(
            js.choices
                .iter()
                .any(|choice| choice.name == "biome" && !choice.selected)
        );
        assert!(
            result
                .iter()
                .filter(|group| group.formatting && group.extensions != ["js"])
                .flat_map(|group| &group.choices)
                .filter(|choice| choice.name == "prettier")
                .all(|choice| !choice.selected)
        );
    }

    #[test]
    fn resumption_keeps_repository_groups_without_catalog_only_extensions() {
        let root = tempfile::tempdir().unwrap();
        for file in ["main.c", "main.cpp", "index.js", "module.jsx"] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let fresh = groups(&mut inventory, &ToolsConfig::default());
        let config: ToolsConfig =
            toml::from_str("skip = [\"clang-format\", \"biome\"]\nrequired = [\"prettier\"]")
                .unwrap();
        let resumed = groups(&mut inventory, &config);
        let shape = |groups: &[super::super::recommendations::Group]| {
            groups
                .iter()
                .filter(|group| group.formatting)
                .map(|group| {
                    (
                        group.title.clone(),
                        group.extensions.clone(),
                        group.files.clone(),
                        group
                            .choices
                            .iter()
                            .map(|choice| choice.name.clone())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(shape(&fresh), shape(&resumed));
        assert!(
            resumed
                .iter()
                .filter(|group| group.formatting)
                .all(|group| !group.files.is_empty())
        );
        assert!(
            resumed
                .iter()
                .flat_map(|group| &group.choices)
                .any(|choice| choice.name == "prettier" && choice.active && choice.selected)
        );
    }

    #[test]
    fn source_linters_are_visible_without_automatic_activation() {
        let root = tempfile::tempdir().unwrap();
        for file in ["index.ts", "main.py", "main.rb", "query.sql"] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        for name in ["oxlint", "pyright", "bandit", "rubocop", "sqlfluff"] {
            let choice = result
                .iter()
                .filter(|group| !group.formatting)
                .flat_map(|group| &group.choices)
                .find(|choice| choice.name == name)
                .unwrap();
            assert!(
                !choice.selected,
                "source-only alternative {name} must be opt-in"
            );
        }
    }

    #[test]
    fn additional_ecosystems_are_visible_and_native_config_selects_linter() {
        let root = tempfile::tempdir().unwrap();
        for file in ["main.java", "index.php", "main.py", "readme.rst"] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        std::fs::write(root.path().join("pyproject.toml"), "[tool.ty]\n").unwrap();
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        for name in [
            "google-java-format",
            "phpstan",
            "phpcs",
            "flake8",
            "ty",
            "yapf",
            "autopep8",
            "vale",
            "codespell",
            "cspell",
        ] {
            assert!(
                result
                    .iter()
                    .flat_map(|group| &group.choices)
                    .any(|choice| choice.name == name),
                "missing {name}"
            );
        }
        assert!(
            result
                .iter()
                .flat_map(|group| &group.choices)
                .any(|choice| choice.name == "ty" && choice.selected)
        );
        assert_eq!(inventory.diagnostics().recursive_walks, 1);
    }

    #[test]
    fn extensionless_build_files_share_inventory_and_init_matching() {
        let root = tempfile::tempdir().unwrap();
        for file in [
            "Dockerfile",
            "Dockerfile.dev",
            "BUILD",
            "WORKSPACE",
            "CMakeLists.txt",
            "ordinary.txt",
        ] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        let scope = crate::tools::scope::ScopeMatcher::new(root.path(), &[]).unwrap();
        for (name, count) in [("hadolint", 2), ("buildifier", 2), ("cmake-format", 1)] {
            let entry = crate::tools::tool_catalog_entry(name).unwrap();
            let group = result
                .iter()
                .find(|group| group.choices.iter().any(|choice| choice.name == name))
                .unwrap();
            assert_eq!(group.files.len(), count, "{name}");
            let extensions = entry.extensions(if group.formatting {
                crate::tools::ToolCapability::Format
            } else {
                crate::tools::ToolCapability::Lint
            });
            assert_eq!(
                scope
                    .filter_relative_files(inventory.files(), &extensions)
                    .len(),
                count
            );
        }
        assert_eq!(inventory.diagnostics().recursive_walks, 1);
    }

    #[test]
    fn haskell_zig_and_scala_recommendations_use_native_preferences() {
        let root = tempfile::tempdir().unwrap();
        for file in [
            "Main.hs",
            "Notes.lhs",
            "build.zig",
            "build.zig.zon",
            "Main.scala",
        ] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        std::fs::write(root.path().join("fourmolu.yaml"), "indentation: 4").unwrap();
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        for name in ["ormolu", "fourmolu", "hlint", "zig", "scalafmt"] {
            assert!(
                result
                    .iter()
                    .flat_map(|group| &group.choices)
                    .any(|choice| choice.name == name)
            );
        }
        assert!(
            result
                .iter()
                .flat_map(|group| &group.choices)
                .any(|choice| choice.name == "fourmolu" && choice.selected)
        );
        assert!(
            !result
                .iter()
                .flat_map(|group| &group.choices)
                .any(|choice| choice.name == "ormolu" && choice.selected)
        );
    }

    #[test]
    fn workflow_matching_retains_yaml_formatters_and_excludes_other_yaml() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".github/workflows")).unwrap();
        for file in [".github/workflows/ci.yml", "other.yml", "Main.cs", "app.ex"] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        let workflow = result
            .iter()
            .find(|group| {
                group
                    .choices
                    .iter()
                    .any(|choice| choice.name == "actionlint")
            })
            .unwrap();
        assert_eq!(
            workflow.files,
            vec![std::path::PathBuf::from(".github/workflows/ci.yml")]
        );
        let scope = crate::tools::scope::ScopeMatcher::new(root.path(), &[]).unwrap();
        assert_eq!(
            scope
                .filter_relative_files(
                    inventory.files(),
                    &std::collections::BTreeSet::from(["github-workflow".to_owned()])
                )
                .len(),
            1
        );
        assert_eq!(
            scope
                .filter_relative_files(
                    inventory.files(),
                    &std::collections::BTreeSet::from(["yml".to_owned()])
                )
                .len(),
            2
        );
        for name in ["csharpier", "mix-format"] {
            assert!(
                result
                    .iter()
                    .flat_map(|group| &group.choices)
                    .any(|choice| choice.name == name)
            );
        }
    }

    #[test]
    fn protobuf_ocaml_and_ruby_native_config_is_recommended() {
        let root = tempfile::tempdir().unwrap();
        for file in [
            "api.proto",
            "main.ml",
            "main.mli",
            "main.rb",
            ".ocamlformat",
            ".standard.yml",
        ] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        for name in ["buf", "ocamlformat", "standardrb"] {
            assert!(
                result
                    .iter()
                    .flat_map(|group| &group.choices)
                    .any(|choice| choice.name == name && choice.selected),
                "{name}"
            );
        }
        assert_eq!(inventory.diagnostics().recursive_walks, 1);
    }

    #[test]
    fn fish_jsonnet_typst_are_source_driven_formatter_alternatives() {
        let root = tempfile::tempdir().unwrap();
        for file in [
            "config.fish",
            "main.jsonnet",
            "library.libsonnet",
            "document.typ",
        ] {
            std::fs::write(root.path().join(file), "").unwrap();
        }
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        for (name, count) in [("fish_indent", 1), ("jsonnetfmt", 2), ("typstyle", 1)] {
            let group = result
                .iter()
                .find(|group| group.choices.iter().any(|choice| choice.name == name))
                .unwrap();
            assert_eq!(group.files.len(), count);
            assert!(
                group
                    .choices
                    .iter()
                    .any(|choice| choice.name == name && choice.selected)
            );
        }
    }

    #[test]
    fn config_wins_and_all_alternatives_are_shown() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("index.js"), "const x = 1;\n").unwrap();
        std::fs::write(root.path().join("biome.json"), "{}").unwrap();
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        let js = result
            .iter()
            .find(|group| group.formatting && group.title.contains("JavaScript"))
            .unwrap();
        assert!(js.choices.iter().any(|choice| choice.name == "prettier"));
        assert_eq!(
            js.choices.iter().filter(|choice| choice.selected).count(),
            1
        );
        assert!(
            js.choices
                .iter()
                .any(|choice| choice.name == "biome" && choice.selected)
        );
        assert_eq!(inventory.diagnostics().recursive_walks, 1);
    }
}
