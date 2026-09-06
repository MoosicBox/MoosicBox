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
            .find(|group| group.title.contains("JavaScript"))
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
            .find(|group| group.title.contains("JavaScript"))
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
    fn config_wins_and_all_alternatives_are_shown() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("index.js"), "const x = 1;\n").unwrap();
        std::fs::write(root.path().join("biome.json"), "{}").unwrap();
        let mut inventory = RepositoryDiscovery::discover(root.path()).unwrap();
        let result = groups(&mut inventory, &ToolsConfig::default());
        let js = result
            .iter()
            .find(|group| group.title.contains("JavaScript"))
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
