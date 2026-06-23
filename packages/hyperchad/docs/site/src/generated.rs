#![allow(clippy::format_push_string)]

//! Markdown generators for CLI and TOML config reference pages.

use std::collections::BTreeMap;

use clap::builder::ValueHint;
use hyperchad_docs_config::{ConfigDocSchema, FieldDoc, NestedFieldDoc};

/// Builder for CLI reference generation.
pub struct CliReference {
    root_name: &'static str,
    command: clap::Command,
    max_depth: usize,
    include_usage: bool,
}

impl CliReference {
    /// Create a CLI reference builder.
    #[must_use]
    pub const fn new(root_name: &'static str, command: clap::Command) -> Self {
        Self {
            root_name,
            command,
            max_depth: usize::MAX,
            include_usage: true,
        }
    }

    /// Set max subcommand recursion depth.
    #[must_use]
    pub const fn max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Toggle usage rendering.
    #[must_use]
    pub const fn include_usage(mut self, include_usage: bool) -> Self {
        self.include_usage = include_usage;
        self
    }

    /// Render markdown.
    #[must_use]
    pub fn render(&self) -> String {
        let mut doc = String::new();
        render_command(
            &mut doc,
            &self.command,
            &[self.root_name],
            0,
            self.max_depth,
            self.include_usage,
        );
        doc
    }
}

/// Builder for config reference generation.
pub struct ConfigReference<T: ConfigDocSchema> {
    intro: String,
    env_overrides: Vec<EnvOverrideDoc>,
    appendices: Vec<String>,
    section_appendices: Vec<SectionAppendix>,
    _marker: std::marker::PhantomData<T>,
}

#[derive(Clone)]
struct SectionAppendix {
    section_name: String,
    markdown: String,
}

impl<T: ConfigDocSchema> ConfigReference<T> {
    /// Create a config reference builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            intro: String::new(),
            env_overrides: Vec::new(),
            appendices: Vec::new(),
            section_appendices: Vec::new(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Set intro markdown.
    #[must_use]
    pub fn intro(mut self, intro: impl Into<String>) -> Self {
        self.intro = intro.into();
        self
    }

    /// Add one environment/path override doc.
    #[must_use]
    pub fn env_override(
        mut self,
        variable: &'static str,
        scope: &'static str,
        description: &'static str,
    ) -> Self {
        self.env_overrides.push(EnvOverrideDoc {
            variable,
            scope,
            description,
        });
        self
    }

    /// Add several environment/path override docs.
    #[must_use]
    pub fn env_overrides<I>(mut self, env_overrides: I) -> Self
    where
        I: IntoIterator<Item = EnvOverrideDoc>,
    {
        self.env_overrides.extend(env_overrides);
        self
    }

    /// Append arbitrary markdown after a generated top-level config section.
    #[must_use]
    pub fn section_appendix(
        mut self,
        section_name: impl Into<String>,
        markdown: impl Into<String>,
    ) -> Self {
        self.section_appendices.push(SectionAppendix {
            section_name: section_name.into(),
            markdown: markdown.into(),
        });
        self
    }

    /// Append arbitrary markdown after generated config sections.
    #[must_use]
    pub fn append_markdown(mut self, markdown: impl Into<String>) -> Self {
        self.appendices.push(markdown.into());
        self
    }

    /// Render markdown.
    #[must_use]
    pub fn render(&self) -> String {
        let mut doc = render_config_reference::<T>(
            &self.intro,
            &self.env_overrides,
            &self.section_appendices,
        );
        for appendix in &self.appendices {
            if !doc.ends_with("\n\n") {
                doc.push_str("\n\n");
            }
            doc.push_str(appendix);
        }
        doc
    }
}

impl<T: ConfigDocSchema> Default for ConfigReference<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata for an environment/path override rendered in config docs.
#[derive(Clone)]
pub struct EnvOverrideDoc {
    /// Environment variable name.
    pub variable: &'static str,
    /// Override scope.
    pub scope: &'static str,
    /// Override behavior.
    pub description: &'static str,
}

/// Generate a CLI reference from a clap command tree.
#[must_use]
pub fn cli_reference(root_name: &'static str, cmd: clap::Command) -> String {
    CliReference::new(root_name, cmd).render()
}

#[allow(clippy::too_many_lines)]
fn render_command(
    doc: &mut String,
    cmd: &clap::Command,
    path: &[&str],
    depth: usize,
    max_depth: usize,
    include_usage: bool,
) {
    let full_path = path.join(" ");
    let heading = match depth {
        0 => "##",
        1 => "###",
        _ => "####",
    };
    doc.push_str(&format!("{heading} `{full_path}`\n\n"));

    if let Some(about) = cmd.get_about() {
        doc.push_str(&format!("{about}\n\n"));
    }

    let options: Vec<_> = cmd
        .get_arguments()
        .filter(|arg| !arg.is_hide_set() && arg.get_id() != "help" && arg.get_id() != "version")
        .collect();
    let positionals: Vec<_> = options.iter().filter(|arg| arg.is_positional()).collect();
    let flags: Vec<_> = options.iter().filter(|arg| !arg.is_positional()).collect();

    if include_usage && (!positionals.is_empty() || !flags.is_empty()) {
        let mut usage = format!("`{full_path}");
        for pos in &positionals {
            let name = pos.get_id().as_str().to_uppercase();
            if pos.is_required_set() {
                usage.push_str(&format!(" <{name}>"));
            } else {
                usage.push_str(&format!(" [{name}]"));
            }
        }
        if !flags.is_empty() {
            usage.push_str(" [OPTIONS]");
        }
        usage.push('`');
        doc.push_str(&format!("**Usage:** {usage}\n\n"));
    }

    if !positionals.is_empty() {
        doc.push_str("**Arguments:**\n\n| Name | Description | Required |\n|------|-------------|----------|\n");
        for arg in positionals {
            let desc = arg.get_help().map(ToString::to_string).unwrap_or_default();
            let required = if arg.is_required_set() { "yes" } else { "no" };
            doc.push_str(&format!(
                "| `{}` | {} | {required} |\n",
                escape_markdown_table_cell(arg.get_id().as_str()),
                escape_markdown_table_cell(&desc),
            ));
        }
        doc.push('\n');
    }

    if !flags.is_empty() {
        doc.push_str("**Options:**\n\n| Flag | Description | Values | Default |\n|------|-------------|--------|---------|\n");
        for flag in flags {
            let mut names = Vec::new();
            if let Some(short) = flag.get_short() {
                names.push(format!("-{short}"));
            }
            if let Some(long) = flag.get_long() {
                names.push(format!("--{long}"));
            }
            let flag_name = if names.is_empty() {
                flag.get_id().to_string()
            } else {
                names.join(", ")
            };
            let desc = flag.get_help().map(ToString::to_string).unwrap_or_default();
            let values = render_possible_values(flag);
            let default = flag
                .get_default_values()
                .iter()
                .map(|value| value.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let default = if default.is_empty() {
                String::new()
            } else {
                format!("`{}`", escape_inline_code(&default))
            };
            doc.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                escape_markdown_table_cell(&flag_name),
                escape_markdown_table_cell(&desc),
                escape_markdown_table_cell(&values),
                escape_markdown_table_cell(&default),
            ));
        }
        doc.push('\n');
    }

    let subcommands: Vec<_> = cmd
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set())
        .collect();
    if !subcommands.is_empty() && depth < 2 {
        doc.push_str("**Subcommands:**\n\n");
        for sub in &subcommands {
            let desc = sub.get_about().map(ToString::to_string).unwrap_or_default();
            doc.push_str(&format!("- `{}` — {desc}\n", sub.get_name()));
        }
        doc.push('\n');
    }

    for sub in subcommands {
        if depth >= max_depth {
            continue;
        }
        let mut child_path = path.to_vec();
        child_path.push(sub.get_name());
        render_command(doc, sub, &child_path, depth + 1, max_depth, include_usage);
    }
}

fn render_possible_values(arg: &clap::Arg) -> String {
    let possible_values = arg.get_possible_values();
    if !possible_values.is_empty() {
        return possible_values
            .iter()
            .map(|value| format!("`{}`", value.get_name()))
            .collect::<Vec<_>>()
            .join(", ");
    }

    match arg.get_value_hint() {
        ValueHint::FilePath => "file path".to_string(),
        ValueHint::DirPath => "directory path".to_string(),
        ValueHint::Url => "URL".to_string(),
        ValueHint::CommandName | ValueHint::CommandString => "command".to_string(),
        _ => String::new(),
    }
}

/// Generate a config reference from a root config schema.
#[must_use]
pub fn config_reference<T: ConfigDocSchema>(
    intro: &str,
    env_overrides: &[EnvOverrideDoc],
) -> String {
    render_config_reference::<T>(intro, env_overrides, &[])
}

fn render_config_reference<T: ConfigDocSchema>(
    intro: &str,
    env_overrides: &[EnvOverrideDoc],
    section_appendices: &[SectionAppendix],
) -> String {
    let mut doc = String::new();
    if !intro.is_empty() {
        doc.push_str(intro);
        if !intro.ends_with("\n\n") {
            doc.push_str("\n\n");
        }
    }
    if !env_overrides.is_empty() {
        doc.push_str("## Path & Env Overrides\n\n| Variable | Scope | Behavior |\n|----------|-------|----------|\n");
        for override_doc in env_overrides {
            doc.push_str(&format!(
                "| `{}` | {} | {} |\n",
                escape_markdown_table_cell(override_doc.variable),
                escape_markdown_table_cell(override_doc.scope),
                escape_markdown_table_cell(override_doc.description),
            ));
        }
        doc.push_str("\n---\n\n");
    }

    for field in T::field_docs() {
        if let Some(NestedFieldDoc::Inline { fields, defaults }) = field.nested {
            let (fields, defaults) = flatten_field_docs(&fields, &defaults, "");
            render_section(
                &mut doc,
                field.toml_key,
                field.description,
                &fields,
                &defaults,
            );
            append_section_appendices(&mut doc, field.toml_key, section_appendices);
        }
    }

    doc
}

#[derive(Clone)]
struct RenderField {
    key: String,
    type_display: &'static str,
    description: &'static str,
    enum_values: Option<&'static [&'static str]>,
}

fn append_section_appendices(
    doc: &mut String,
    section_name: &str,
    section_appendices: &[SectionAppendix],
) {
    for appendix in section_appendices
        .iter()
        .filter(|appendix| appendix.section_name == section_name)
    {
        if !doc.ends_with("\n\n") {
            doc.push_str("\n\n");
        }
        doc.push_str(&appendix.markdown);
        if !doc.ends_with("\n\n") {
            doc.push_str("\n\n");
        }
    }
}

fn render_section(
    doc: &mut String,
    section_name: &str,
    section_description: &str,
    fields: &[RenderField],
    defaults: &BTreeMap<String, String>,
) {
    doc.push_str(&format!("## `{section_name}`\n\n"));
    if !section_description.is_empty() {
        doc.push_str(section_description);
        doc.push_str("\n\n");
    }
    doc.push_str(
        "| Key | Type | Default | Description |\n|-----|------|---------|-------------|\n",
    );
    for field in fields {
        let default = defaults.get(&field.key).map_or(String::new(), |value| {
            format!("`{}`", escape_inline_code(value))
        });
        let mut description = field.description.to_string();
        if let Some(values) = field.enum_values {
            description.push_str(" Valid values: ");
            description.push_str(
                &values
                    .iter()
                    .map(|value| format!("`{value}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            description.push('.');
        }
        doc.push_str(&format!(
            "| `{}` | `{}` | {} | {} |\n",
            escape_markdown_table_cell(&field.key),
            escape_markdown_table_cell(field.type_display),
            escape_markdown_table_cell(&default),
            escape_markdown_table_cell(&description),
        ));
    }
    doc.push_str("\n---\n\n");
}

fn flatten_field_docs(
    fields: &[FieldDoc],
    defaults: &BTreeMap<String, String>,
    prefix: &str,
) -> (Vec<RenderField>, BTreeMap<String, String>) {
    let mut flattened_fields = Vec::new();
    let mut flattened_defaults = BTreeMap::new();

    for field in fields {
        let full_key = dotted_key(prefix, field.toml_key);
        match field.nested.clone() {
            Some(NestedFieldDoc::Inline { fields, defaults }) => {
                let (child_fields, child_defaults) =
                    flatten_field_docs(&fields, &defaults, &full_key);
                flattened_fields.extend(child_fields);
                flattened_defaults.extend(child_defaults);
            }
            Some(NestedFieldDoc::Map {
                key_placeholder,
                value_fields,
                value_defaults,
            }) => {
                let map_prefix = dotted_key(&full_key, key_placeholder);
                let (child_fields, child_defaults) =
                    flatten_field_docs(&value_fields, &value_defaults, &map_prefix);
                flattened_fields.extend(child_fields);
                flattened_defaults.extend(child_defaults);
            }
            Some(NestedFieldDoc::List {
                index_placeholder,
                item_fields,
                item_defaults,
            }) => {
                let list_prefix = dotted_key(&full_key, index_placeholder);
                let (child_fields, child_defaults) =
                    flatten_field_docs(&item_fields, &item_defaults, &list_prefix);
                flattened_fields.extend(child_fields);
                flattened_defaults.extend(child_defaults);
            }
            None => {
                if let Some(default) = defaults.get(field.toml_key) {
                    flattened_defaults.insert(full_key.clone(), default.clone());
                }
                flattened_fields.push(RenderField {
                    key: full_key,
                    type_display: field.type_display,
                    description: field.description,
                    enum_values: field.enum_values,
                });
            }
        }
    }

    (flattened_fields, flattened_defaults)
}

fn dotted_key(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn escape_markdown_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', "<br>")
}

fn escape_inline_code(value: &str) -> String {
    value.replace('`', "\\`")
}

#[cfg(test)]
mod tests {
    use hyperchad_docs_config::{ConfigDocSchema, FieldDoc, NestedFieldDoc};
    use std::collections::BTreeMap;

    use super::*;

    #[derive(Default)]
    struct TestConfig;

    impl ConfigDocSchema for TestConfig {
        fn section_name() -> &'static str {
            "server"
        }

        fn section_description() -> &'static str {
            "Server settings."
        }

        fn default_values() -> BTreeMap<String, String> {
            BTreeMap::new()
        }

        fn field_docs() -> Vec<FieldDoc> {
            vec![FieldDoc {
                toml_key: "server",
                description: "Server settings.",
                type_display: "table",
                enum_values: None,
                nested: Some(NestedFieldDoc::Inline {
                    fields: vec![FieldDoc {
                        toml_key: "host",
                        description: "Bind host.",
                        type_display: "string",
                        enum_values: None,
                        nested: None,
                    }],
                    defaults: BTreeMap::from([("host".to_string(), "127.0.0.1".to_string())]),
                }),
            }]
        }
    }

    #[test]
    fn config_reference_appends_section_markdown_after_matching_section() {
        let doc = ConfigReference::<TestConfig>::new()
            .section_appendix(
                "server",
                "### Server examples\n\n```toml\n[server]\nhost = \"0.0.0.0\"\n```",
            )
            .render();

        assert!(doc.contains("## `server`"));
        assert!(doc.contains("### Server examples"));
        assert!(doc.contains("host = \"0.0.0.0\""));
    }
}
