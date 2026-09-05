#[cfg(all(test, feature = "format"))]
mod tests {
    use super::super::recommendations::groups;
    use super::super::{RepositoryDiscovery, ToolsConfig};

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
