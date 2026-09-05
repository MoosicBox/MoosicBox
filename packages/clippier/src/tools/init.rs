//! Guided setup using the same repository evidence as tool execution.

#[cfg(feature = "tools-tui")]
mod inline;
mod recommendations;
#[cfg(test)]
#[path = "init/recommendations_tests.rs"]
mod recommendations_tests;

use std::io::{BufRead, Write};
use std::path::Path;

use super::{RepositoryDiscovery, ToolsConfig};
use toml_edit::{DocumentMut, Item, Table, value};

/// Guides setup in `root`, previews additions, and applies them on approval.
/// Existing choices and native configuration are preserved. No tools are run.
///
/// # Errors
///
/// * If existing TOML or runner configuration is invalid
/// * If repository discovery, terminal input/output, or file access fails
/// * If input ends before setup completes or the file changes during setup
#[allow(clippy::too_many_lines)]
pub fn initialize(
    root: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> std::io::Result<()> {
    initialize_with_ui(root, input, output, false)
}

/// Runs setup with inline checkboxes when stdin and stdout are terminals.
///
/// # Errors
/// * If setup fails (see [`initialize`]) or terminal interaction fails
pub fn initialize_terminal(root: &Path) -> std::io::Result<()> {
    use std::io::IsTerminal;
    let interactive = cfg!(feature = "tools-tui")
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal();
    initialize_with_ui(
        root,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        interactive,
    )
}

#[allow(clippy::too_many_lines)]
fn initialize_with_ui(
    root: &Path,
    input: &mut impl BufRead,
    output: &mut impl Write,
    interactive: bool,
) -> std::io::Result<()> {
    let destination = root.join("clippier.toml");
    let original = read_existing(&destination)?;
    let mut document = original
        .as_deref()
        .unwrap_or_default()
        .parse::<DocumentMut>()
        .map_err(std::io::Error::other)?;
    let parsed: toml::Value =
        toml::from_str(original.as_deref().unwrap_or_default()).map_err(std::io::Error::other)?;
    let config: ToolsConfig = parsed
        .get("runner")
        .cloned()
        .map(toml::Value::try_into)
        .transpose()
        .map_err(std::io::Error::other)?
        .unwrap_or_default();
    let mut inventory = RepositoryDiscovery::inventory(root, &config.scope)?;
    let evidence = inventory
        .tool_evidence()
        .into_iter()
        .filter(|(name, _)| {
            !config.required.contains(name)
                && !config.skip.contains(name)
                && !config.tools.contains_key(name)
                && !config.executables.contains_key(name)
        })
        .collect::<Vec<_>>();
    if original.is_some()
        && evidence.is_empty()
        && (!interactive || recommendations::groups(&mut inventory, &config).is_empty())
    {
        writeln!(output, "No newly relevant tools; clippier.toml unchanged.")?;
        return Ok(());
    }
    writeln!(output, "Repository: {}", root.canonicalize()?.display())?;
    writeln!(
        output,
        "Native configs remain authoritative. Tools will not be installed or run."
    )?;
    writeln!(
        output,
        "Selected formatter coverage is limited to its chosen file extensions. Native tool exclusions still apply."
    )?;
    let mut selected_policies = std::collections::BTreeMap::new();
    let mut selected = Vec::new();
    let mut skipped = Vec::new();
    if interactive {
        #[allow(unused_mut)]
        let mut groups = recommendations::groups(&mut inventory, &config);
        #[cfg(not(feature = "tools-tui"))]
        for group in &groups {
            writeln!(output, "{}", group.title)?;
            for choice in &group.choices {
                writeln!(output, "{}: {}", choice.name, choice.reason)?;
            }
        }
        #[cfg(feature = "tools-tui")]
        inline::select(&mut groups, output)?;
        (selected, skipped) = recommendations::selections(&groups);
        selected_policies = recommendations::policies(&groups);
    } else {
        for (name, evidence) in evidence {
            writeln!(
                output,
                "\n{name}: {:?} ({})",
                evidence.kind,
                evidence.path.display()
            )?;
            if confirm(input, output, &format!("Enable {name}?"), true)? {
                selected.push(name);
            } else {
                skipped.push(name);
            }
        }
    }
    if selected.is_empty() {
        writeln!(
            output,
            "No tools selected. Automatic discovery remains enabled for tools not skipped."
        )?;
    }
    let required = !selected.is_empty()
        && confirm(
            input,
            output,
            "Require selected tools to be installed (recommended for CI)?",
            true,
        )?;
    // toml_edit treats a comment-only document as trailing decoration. Keep
    // those original bytes before newly added tables rather than moving them.
    let prefix = if document.is_empty() {
        document.set_trailing("");
        original.as_deref().unwrap_or_default()
    } else {
        ""
    };
    apply_choices(&mut document, &selected, &skipped, required);
    if interactive {
        // Explicit choices must work even without native configuration evidence.
        apply_choices(&mut document, &selected, &[], false);
        for (name, policy) in selected_policies {
            let target = &mut document["runner"]["tools"][&name];
            target["mode"] = value("enabled");
            target["capabilities"] = value(
                policy
                    .capabilities
                    .iter()
                    .map(|capability| match capability {
                        super::ToolCapability::Format => "format",
                        super::ToolCapability::Lint => "lint",
                    })
                    .collect::<toml_edit::Array>(),
            );
            if !policy.format_extensions.is_empty() {
                target["format-extensions"] = value(
                    policy
                        .format_extensions
                        .iter()
                        .map(String::as_str)
                        .collect::<toml_edit::Array>(),
                );
            }
        }
    }
    let separator = if !prefix.is_empty() && !prefix.ends_with('\n') && !document.is_empty() {
        "\n"
    } else {
        ""
    };
    let rendered = format!("{prefix}{separator}{document}");
    if original.as_deref() == Some(rendered.as_str()) {
        writeln!(output, "No changes; clippier.toml unchanged.")?;
        return Ok(());
    }
    writeln!(
        output,
        "\n--- proposed clippier.toml ---\n{rendered}--- end ---"
    )?;
    if !confirm(
        input,
        output,
        "Apply these additions to clippier.toml?",
        false,
    )? {
        writeln!(output, "Cancelled; no files written.")?;
        return Ok(());
    }
    if read_existing(&destination)? != original {
        return Err(std::io::Error::other(
            "clippier.toml changed during setup; no changes written",
        ));
    }
    if original.is_some() {
        std::fs::write(&destination, rendered)?;
    } else {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)?;
        file.write_all(rendered.as_bytes())?;
    }
    writeln!(
        output,
        "Updated {}. Review tool selection with clippier check --list and clippier fmt --list.",
        destination.display()
    )?;
    Ok(())
}

fn confirm(
    input: &mut impl BufRead,
    output: &mut impl Write,
    question: &str,
    default: bool,
) -> std::io::Result<bool> {
    loop {
        write!(
            output,
            "{question} {} ",
            if default { "[Y/n]" } else { "[y/N]" }
        )?;
        output.flush()?;
        let mut answer = String::new();
        if input.read_line(&mut answer)? == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Setup input ended; configuration not written",
            ));
        }
        match answer.trim().to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => writeln!(output, "Please answer yes or no.")?,
        }
    }
}

fn read_existing(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn apply_choices(
    document: &mut DocumentMut,
    selected: &[String],
    skipped: &[String],
    required: bool,
) {
    for (key, names) in [
        ("required", if required { selected } else { &[] }),
        ("skip", skipped),
    ] {
        if names.is_empty() {
            continue;
        }
        if document.get("runner").is_none() {
            document["runner"] = Item::Table(Table::new());
        }
        if document["runner"].get(key).is_none() {
            document["runner"][key] = value(toml_edit::Array::new());
        }
        let array = document["runner"][key]
            .as_array_mut()
            .expect("validated runner array");
        for name in names {
            array.push(name.as_str());
        }
    }
    // Persist accepted non-required choices so subsequent init runs do not ask again.
    // Auto retains native-config evidence and formatter ownership behavior.
    if !required {
        for name in selected {
            if document.get("runner").is_none() {
                document["runner"] = Item::Table(Table::new());
            }
            if document["runner"].get("tools").is_none() {
                document["runner"]["tools"] = if document["runner"].is_inline_table() {
                    value(toml_edit::InlineTable::new())
                } else {
                    Item::Table(Table::new())
                };
            }
            let mut policy = toml_edit::InlineTable::new();
            policy.insert("mode", "auto".into());
            document["runner"]["tools"][name] = value(policy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "format")]
    fn setup_creates_valid_config_and_preserves_existing_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = 'example'\nversion = '0.1.0'\n",
        )
        .unwrap();
        let mut inventory =
            RepositoryDiscovery::inventory(root.path(), &super::super::ScopeConfig::default())
                .unwrap();
        let evidence = inventory.tool_evidence();
        assert!(evidence.contains_key("rustfmt"));
        assert_eq!(inventory.diagnostics().recursive_walks, 1);
        let responses = format!("{}y\ny\n", "y\n".repeat(evidence.len()));
        let mut output = Vec::new();
        initialize(root.path(), &mut responses.as_bytes(), &mut output).unwrap();
        let path = root.path().join("clippier.toml");
        let contents = std::fs::read_to_string(&path).unwrap();
        let value: toml::Value = toml::from_str(&contents).unwrap();
        let config: super::super::ToolsConfig = value["runner"].clone().try_into().unwrap();
        assert!(config.required.contains(&"rustfmt".to_owned()));
        initialize(root.path(), &mut &b""[..], &mut output).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), contents);
    }

    #[test]
    #[cfg(feature = "format")]
    fn cancellation_does_not_write() {
        let root = tempfile::tempdir().unwrap();
        initialize(root.path(), &mut &b"n\nn\n"[..], &mut Vec::new()).unwrap();
        assert!(!root.path().join("clippier.toml").exists());
    }

    #[test]
    fn generated_config_round_trips() {
        let mut document = DocumentMut::new();
        apply_choices(
            &mut document,
            &["rustfmt".into()],
            &["prettier".into()],
            true,
        );
        let text = document.to_string();
        let value: toml::Value = toml::from_str(&text).unwrap();
        let config: super::super::ToolsConfig = value["runner"].clone().try_into().unwrap();
        assert_eq!(config.required, ["rustfmt"]);
        assert_eq!(config.skip, ["prettier"]);
    }

    #[test]
    fn edits_preserve_unrelated_bytes() {
        let original = "# header\n[runner] # runner comment\nrequired = ['rustfmt'] # keep\nskip = [ 'prettier', ]\n\n[unrelated]\nanswer   = 42 # untouched\n";
        let mut document = original.parse::<DocumentMut>().unwrap();
        apply_choices(&mut document, &["taplo".into()], &[], true);
        assert_eq!(
            document.to_string(),
            original.replace("['rustfmt']", "['rustfmt', \"taplo\"]")
        );
    }

    #[test]
    fn inline_tables_support_non_required_choices() {
        let mut document = "runner = { skip = ['prettier'] }\n"
            .parse::<DocumentMut>()
            .unwrap();
        apply_choices(&mut document, &["taplo".into()], &[], false);
        let parsed: toml::Value = toml::from_str(&document.to_string()).unwrap();
        assert_eq!(
            parsed["runner"]["tools"]["taplo"]["mode"].as_str(),
            Some("auto")
        );
    }

    #[test]
    #[cfg(feature = "format")]
    fn existing_config_only_gets_approved_new_choices() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("clippier.toml");
        let original = "# preserve me\n[runner]\nskip = ['rustfmt']\n[runner.tools.clippy]\nmode = 'disabled'\n[custom]\nvalue = 'untouched'\n";
        std::fs::write(&path, original).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = 'example'\nversion = '0.1.0'\n",
        )
        .unwrap();
        let mut discovery =
            RepositoryDiscovery::inventory(root.path(), &super::super::ScopeConfig::default())
                .unwrap();
        let candidates = discovery
            .tool_evidence()
            .keys()
            .filter(|name| !["rustfmt", "clippy"].contains(&name.as_str()))
            .count();
        assert!(candidates > 0);
        let answers = format!("{}n\nn\n", "y\n".repeat(candidates));
        initialize(root.path(), &mut answers.as_bytes(), &mut Vec::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        let answers = format!("{}n\ny\n", "y\n".repeat(candidates));
        initialize(root.path(), &mut answers.as_bytes(), &mut Vec::new()).unwrap();
        let updated = std::fs::read_to_string(&path).unwrap();
        assert!(updated.contains("skip = ['rustfmt']"));
        assert!(updated.contains("mode = 'disabled'"));
        assert!(updated.contains("[custom]\nvalue = 'untouched'"));
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        initialize(root.path(), &mut &b""[..], &mut Vec::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), updated);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
    }

    #[test]
    #[cfg(feature = "format")]
    fn new_evidence_is_suggested_and_invalid_config_is_untouched() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("clippier.toml");
        std::fs::write(&path, "# existing\n").unwrap();
        initialize(root.path(), &mut &b""[..], &mut Vec::new()).unwrap();
        std::fs::write(root.path().join("rustfmt.toml"), "max_width = 100\n").unwrap();
        let mut output = Vec::new();
        initialize(root.path(), &mut &b"n\ny\n"[..], &mut output).unwrap();
        assert!(String::from_utf8(output).unwrap().contains("rustfmt:"));
        let updated = std::fs::read_to_string(&path).unwrap();
        assert!(updated.starts_with("# existing\n"));
        initialize(root.path(), &mut &b""[..], &mut Vec::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), updated);
        let invalid = "[runner]\nskip = 42\n";
        std::fs::write(&path, invalid).unwrap();
        assert!(initialize(root.path(), &mut &b""[..], &mut Vec::new()).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), invalid);
    }

    #[test]
    fn confirmation_retries_and_handles_eof() {
        let mut output = Vec::new();
        assert!(confirm(&mut &b"invalid\nyes\n"[..], &mut output, "Continue?", false).unwrap());
        assert!(!confirm(&mut &b"\n"[..], &mut output, "Continue?", false).unwrap());
        assert_eq!(
            confirm(&mut &b""[..], &mut output, "Continue?", false)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
}
