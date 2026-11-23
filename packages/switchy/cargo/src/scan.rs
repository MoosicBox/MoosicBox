//! The source scan: each line of a crate's code a simulation runs that reaches what no seed
//! controls.
//!
//! It reads every `.rs` file under the crate's `src/` (skipping `#[cfg(test)]` items, comments,
//! `// native-only: begin` … `// native-only: end` regions, and the test modules `tests.rs` and
//! `*_tests.rs`) and reports each line that:
//!
//! * uses a forbidden primitive ([`FORBIDDEN`], plus the simulation's and the crate's own)
//!   other than through a `switchy_` crate of the same name;
//! * imports one in a group (`use std::{fs, sync::{Arc, Mutex}};`);
//! * declares process-wide mutable state (a `static` holding a lock, an atomic or a lazily
//!   initialised value), which concurrent simulated runs would share;
//! * chooses code by operating system outside the files the crate allows it.
//!
//! A line using the standard library's version directly on purpose says why with a
//! `// simulation-safe: <reason>` comment (on it or on the line above).

use std::path::{Path, PathBuf};

use crate::manifest::Rules;

/// Primitives a simulation cannot control.
pub const FORBIDDEN: &[&str] = &[
    "std::thread::spawn",
    "std::thread::Builder",
    "std::thread::sleep",
    "std::thread::scope",
    "std::sync::mpsc",
    "std::sync::Mutex",
    "std::sync::RwLock",
    "std::sync::Condvar",
    "std::sync::atomic",
    "std::time::Instant",
    "std::time::SystemTime",
    "SystemTime::now",
    "std::process::Command",
    // The host's process id: a run would depend on which process ran it.
    "process::id()",
    "std::env::var",
    "env::var(",
    "env::var_os(",
    "env::vars(",
    // Files go through `switchy_fs` (the run's file system in simulation), and this process's
    // executable through the code's own platform layer.
    "std::fs",
    "fs::",
    "tempfile::",
    "use tempfile",
    "current_exe(",
    "/proc/self",
    // `Path`'s own file system queries always ask the host (use `switchy_fs`).
    ").is_dir()",
    ").is_file()",
    ".exists()",
    ".try_exists(",
    ".canonicalize()",
    ".read_dir()",
    ".read_link()",
    ".symlink_metadata()",
    "tokio::spawn",
    "tokio::time",
    "tokio::task",
    "tokio::runtime",
    "tokio::select",
    "tokio::test",
    // With several receivers, tokio's wakes them in an order no seed decides: use
    // switchy_async's (tokio's natively, deterministic in simulation).
    "tokio::sync::watch",
    "sync::{mpsc, oneshot, watch}",
    "new_v4",
    "rand::",
    // Iteration order differs between processes: use `BTreeMap`/`BTreeSet`, or mark a map only
    // ever looked up.
    "HashMap",
    "HashSet",
    // Addresses differ between runs: never order by them.
    "as_ptr(",
];

/// One line a scan reports.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// The file, relative to the crate's `src/`.
    pub file: String,
    /// The line, from one.
    pub line: usize,
    /// What it does.
    pub what: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "src/{}:{}: {}", self.file, self.line, self.what)
    }
}

/// Scan the crate whose sources are under `src`, by its `rules`, forbidding `forbidden` (the
/// base list and the simulation's) on top of the crate's own.
///
/// # Errors
///
/// * If the sources cannot be read
pub fn scan(src: &Path, rules: &Rules, forbidden: &[String]) -> std::io::Result<Vec<Finding>> {
    let mut files = Vec::new();
    sources(src, &mut files)?;
    files.sort();
    let forbidden: Vec<String> = forbidden.iter().chain(&rules.forbid).cloned().collect();
    let relative = |path: &Path| {
        path.strip_prefix(src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    };
    // Modules only tests build (`#[cfg(test)] mod name;`), by their files' paths: what they
    // load is test code, like an inline `#[cfg(test)]` module.
    let mut test_only: Vec<String> = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path)?;
        for module in test_modules(&source) {
            test_only.extend(module_paths(&relative(path), &module));
        }
    }
    let mut findings = Vec::new();
    for path in files {
        let relative = relative(&path);
        if test_only
            .iter()
            .any(|module| relative == *module || relative.starts_with(&format!("{}/", module.trim_end_matches(".rs"))))
        {
            continue;
        }
        if (!rules.only.is_empty()
            && !rules.only.iter().any(|only| relative.starts_with(only.as_str())))
            || rules.native.iter().any(|native| relative.starts_with(native.as_str()))
            || rules.exempt.contains(&relative)
            || relative.ends_with("_tests.rs")
            || relative == "tests.rs"
            || relative.ends_with("/tests.rs")
        {
            continue;
        }
        let source = std::fs::read_to_string(&path)?;
        let os_allowed = rules
            .os_named_in
            .iter()
            .any(|prefix| relative.starts_with(prefix.as_str()));
        let mut lines = Vec::new();
        check_source(&relative, &source, &forbidden, os_allowed, &mut lines);
        // A line several patterns match is reported once.
        lines.dedup();
        findings.extend(lines.into_iter().map(|(line, what)| Finding {
            file: relative.clone(),
            line,
            what,
        }));
    }
    Ok(findings)
}

/// Check one file's `source` (at `relative`), pushing what it violates onto `violations`.
fn check_source(
    _relative: &str,
    source: &str,
    forbidden: &[String],
    os_allowed: bool,
    violations: &mut Vec<(usize, String)>,
) {
    let raw: Vec<&str> = source.lines().collect();
    // The `use` statement being read: its first line, its text so far, and whether it says why
    // it is safe.
    let mut statement: Option<(usize, String, bool)> = None;
    for (number, line) in production_lines(source) {
        if !os_allowed && names_an_operating_system(line) {
            violations.push((number, format!("names an OS: {}", line.trim())));
        }
        // The reason may also stand on the line above.
        let safe = line.contains("// simulation-safe:")
            || (number >= 2 && raw[number - 2].contains("// simulation-safe:"));
        if statement.is_none() && starts_use(line) {
            statement = Some((number, String::new(), false));
        }
        if let Some((first, text, statement_safe)) = &mut statement {
            *statement_safe |= safe;
            text.push_str(line.split("//").next().unwrap_or_default());
            text.push(' ');
            if line.contains(';') {
                if !*statement_safe {
                    for path in imported_paths(text) {
                        for pattern in forbidden {
                            // What the statement's own text shows is reported line by line.
                            if uses(&path, pattern) && !uses(text, pattern) {
                                violations.push((*first, format!("imports {path}")));
                            }
                        }
                    }
                }
                statement = None;
            }
        }
        if safe {
            continue;
        }
        if process_wide(line) {
            violations.push((number, format!("process-wide: {}", line.trim())));
        }
        for pattern in forbidden {
            if uses(line, pattern) {
                violations.push((number, line.trim().to_owned()));
            }
        }
    }
}

/// The modules `source` declares only for tests: `mod name;` right after `#[cfg(test)]`
/// (or `#[cfg(all(test, ...))]`), with any other attributes between.
fn test_modules(source: &str) -> Vec<String> {
    let mut modules = Vec::new();
    let mut gated = false;
    for line in source.lines() {
        let item = line.trim_start();
        if item.starts_with("#[cfg(test)]") || item.starts_with("#[cfg(all(test,") {
            gated = true;
            continue;
        }
        if gated && item.starts_with("#[") {
            continue;
        }
        if gated {
            let declaration = item
                .strip_prefix("pub(crate) ")
                .or_else(|| item.strip_prefix("pub(super) "))
                .or_else(|| item.strip_prefix("pub "))
                .unwrap_or(item);
            if let Some(name) = declaration
                .strip_prefix("mod ")
                .and_then(|rest| rest.strip_suffix(';'))
            {
                modules.push(name.trim().to_owned());
            }
        }
        gated = false;
    }
    modules
}

/// Where module `name`, declared in the file at `declaring` (relative to `src/`), lives: as
/// rustc looks for it, `name.rs` or `name/mod.rs` beside a `lib.rs`, `main.rs` or `mod.rs`, and
/// under a directory named after any other file.
fn module_paths(declaring: &str, name: &str) -> Vec<String> {
    let (dir, file) = declaring.rsplit_once('/').unwrap_or(("", declaring));
    let base = match file {
        "lib.rs" | "main.rs" | "mod.rs" => dir.to_owned(),
        other => {
            let stem = other.trim_end_matches(".rs");
            if dir.is_empty() {
                stem.to_owned()
            } else {
                format!("{dir}/{stem}")
            }
        }
    };
    let join = |path: &str| {
        if base.is_empty() {
            path.to_owned()
        } else {
            format!("{base}/{path}")
        }
    };
    vec![join(&format!("{name}.rs")), join(&format!("{name}/mod.rs"))]
}

/// Whether `line` starts a `use` statement.
fn starts_use(line: &str) -> bool {
    let item = line.trim_start();
    let item = item
        .strip_prefix("pub(crate) ")
        .or_else(|| item.strip_prefix("pub(super) "))
        .or_else(|| item.strip_prefix("pub "))
        .unwrap_or(item);
    item.starts_with("use ")
}

/// The full paths a `use` statement imports (`use std::{sync::{Arc, Mutex}, fs};` imports
/// `std::sync::Arc`, `std::sync::Mutex` and `std::fs`), so grouped imports cannot hide a
/// forbidden one.
fn imported_paths(statement: &str) -> Vec<String> {
    let tree = statement.trim().trim_end_matches(';').trim_end();
    let tree = tree.split_once("use ").map_or(tree, |(_, tree)| tree);
    // Aliases name nothing new: drop them (`a as b` imports `a`).
    let mut words: Vec<&str> = Vec::new();
    let mut tokens = tree.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == "as" {
            // The alias, and whatever punctuation it carries.
            if let Some(alias) = tokens.next() {
                let rest = alias.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
                words.push(rest);
            }
        } else {
            words.push(token);
        }
    }
    let tree: String = words.concat();
    let mut paths = Vec::new();
    expand("", &tree, &mut paths);
    paths
}

/// Expand the use tree `tree` under `prefix` onto `paths`.
fn expand(prefix: &str, tree: &str, paths: &mut Vec<String>) {
    let Some(open) = tree.find('{') else {
        let path = format!("{prefix}{tree}");
        paths.push(path.strip_suffix("::self").unwrap_or(&path).to_string());
        return;
    };
    let head = format!("{prefix}{}", &tree[..open]);
    let rest = &tree[open + 1..];
    let inner = rest.strip_suffix('}').unwrap_or(rest);
    let mut depth = 0;
    let mut start = 0;
    for (index, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                expand_part(&head, &inner[start..index], paths);
                start = index + 1;
            }
            _ => {}
        }
    }
    expand_part(&head, &inner[start..], paths);
}

fn expand_part(head: &str, part: &str, paths: &mut Vec<String>) {
    if !part.is_empty() {
        expand(head, part, paths);
    }
}

/// Whether `line` declares process-wide mutable state, which concurrent simulated runs would
/// share (keep it per run in simulation).
fn process_wide(line: &str) -> bool {
    let item = line.trim_start();
    let item = item
        .strip_prefix("pub(crate) ")
        .or_else(|| item.strip_prefix("pub(super) "))
        .or_else(|| item.strip_prefix("pub "))
        .unwrap_or(item);
    item.starts_with("static ")
        && ["OnceLock", "LazyLock", "OnceCell", "Lazy<", "Atomic", "Mutex", "RwLock"]
            .iter()
            .any(|kind| line.contains(kind))
}

/// Whether `line` chooses code by operating system.
fn names_an_operating_system(line: &str) -> bool {
    line.contains("cfg")
        && ["unix", "windows", "target_os", "target_family"]
            .iter()
            .any(|name| line.contains(name))
}

/// Whether `line` uses `pattern` other than through a `switchy_` crate of the same name.
fn uses(line: &str, pattern: &str) -> bool {
    line.match_indices(pattern)
        .any(|(index, _)| !line[..index].ends_with("switchy_"))
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// The lines of `source` outside `#[cfg(test)]` items, `// native-only: begin` …
/// `// native-only: end` regions and comments, numbered from one.
fn production_lines(source: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut skipping = false;
    let mut native_only = false;
    let mut depth: i64 = 0;
    let mut entered = false;
    for (number, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !skipping && trimmed.starts_with("// native-only: begin") {
            native_only = true;
        } else if native_only && trimmed.starts_with("// native-only: end") {
            native_only = false;
            continue;
        }
        if native_only {
            continue;
        }
        if !skipping
            && (trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[cfg(all(test,"))
        {
            skipping = true;
            depth = 0;
            entered = false;
            continue;
        }
        if skipping {
            for c in line.chars() {
                match c {
                    '{' => {
                        depth += 1;
                        entered = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            // An item without a body (`mod tests;`, `use ...;`) ends at its semicolon.
            if (entered && depth <= 0) || (!entered && trimmed.ends_with(';')) {
                skipping = false;
            }
            continue;
        }
        if !trimmed.starts_with("//") {
            lines.push((number + 1, line));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [&str; 0] = [];

    fn check(source: &str, os_allowed: bool) -> Vec<String> {
        let forbidden: Vec<String> = FORBIDDEN.iter().map(ToString::to_string).collect();
        let mut violations = Vec::new();
        check_source("x.rs", source, &forbidden, os_allowed, &mut violations);
        violations
            .into_iter()
            .map(|(line, what)| format!("x.rs:{line}: {what}"))
            .collect()
    }

    #[test]
    fn forbidden_primitives_are_reported() {
        assert_eq!(check("let m = std::sync::Mutex::new(0);", false).len(), 1);
        assert_eq!(check("let m: HashMap<u8, u8> = HashMap::new();", false).len(), 1);
    }

    #[test]
    fn switchy_versions_are_allowed() {
        assert_eq!(check("use switchy_thread::sync::Mutex;", false), NONE);
        assert_eq!(check("let n = switchy_time::now();", false), NONE);
        assert_eq!(check("switchy_fs::sync::read(path)", false), NONE);
    }

    #[test]
    fn a_reason_on_the_line_or_above_opts_out() {
        assert_eq!(check("let m = std::sync::Mutex::new(0); // simulation-safe: x", false), NONE);
        assert_eq!(check("// simulation-safe: x\nlet m = std::sync::Mutex::new(0);", false), NONE);
    }

    #[test]
    fn test_items_and_comments_are_skipped() {
        let source = "// std::sync::Mutex\n#[cfg(test)]\nmod tests {\n    use std::fs;\n}\nfn f() {}\n";
        assert_eq!(check(source, false), NONE);
        assert_eq!(check("#[cfg(test)]\nmod tests;\nuse std::fs;", false).len(), 1);
    }

    #[test]
    fn native_only_regions_are_skipped() {
        let source = "// native-only: begin\nuse std::sync::Mutex;\n#[cfg(windows)]\n// native-only: end\nuse std::sync::Mutex;";
        assert_eq!(check(source, false), ["x.rs:5: use std::sync::Mutex;"]);
    }

    #[test]
    fn grouped_imports_are_expanded() {
        assert_eq!(
            check("use std::{\n    path::PathBuf,\n    sync::{Arc, Mutex},\n};", false),
            ["x.rs:1: imports std::sync::Mutex"]
        );
        assert_eq!(
            check("pub use std::sync::{Arc, RwLock as Lock};", false),
            ["x.rs:1: imports std::sync::RwLock"]
        );
        assert_eq!(check("use std::sync::{Arc, PoisonError};", false), NONE);
        assert_eq!(check("use switchy_thread::sync::{Arc, Mutex};", false), NONE);
        // Reported once, by the line that shows it.
        assert_eq!(check("use std::sync::Mutex;", false).len(), 1);
        assert_eq!(check("use std::{\n    sync::{Arc, Mutex}, // simulation-safe: x\n};", false), NONE);
    }

    #[test]
    fn grouped_std_fs_imports_are_reported() {
        assert_eq!(check("use std::{fs, io};", false).len(), 1);
        assert_eq!(check("use std::{io, path};", false), NONE);
    }

    #[test]
    fn statics_holding_state_are_reported_whatever_their_visibility() {
        assert_eq!(check("static X: OnceLock<u8> = OnceLock::new();", false).len(), 1);
        assert_eq!(check("pub static X: AtomicU64 = AtomicU64::new(0);", false).len(), 1);
        assert_eq!(check("pub(crate) static X: Mutex<u8> = Mutex::new(0);", false).len(), 1);
        assert_eq!(check("static NAMES: &[&str] = &[];", false), NONE);
    }

    #[test]
    fn modules_only_tests_build_are_found_with_their_files() {
        let source = "#[cfg(test)]\nmod compatibility;\n#[cfg(all(test, unix))]\n#[path = \"x\"]\npub(crate) mod more;\nmod real;\n";
        assert_eq!(test_modules(source), ["compatibility", "more"]);
        assert_eq!(module_paths("lib.rs", "a"), ["a.rs", "a/mod.rs"]);
        assert_eq!(module_paths("runtime.rs", "a"), ["runtime/a.rs", "runtime/a/mod.rs"]);
        assert_eq!(module_paths("x/mod.rs", "a"), ["x/a.rs", "x/a/mod.rs"]);
        assert_eq!(module_paths("x/y.rs", "a"), ["x/y/a.rs", "x/y/a/mod.rs"]);
    }

    #[test]
    fn operating_systems_are_named_only_where_allowed() {
        assert_eq!(check("#[cfg(unix)]", false).len(), 1);
        assert_eq!(check("#[cfg(unix)]", true), NONE);
        // A reason does not excuse naming an operating system.
        assert_eq!(check("#[cfg(windows)] // simulation-safe: x", false).len(), 1);
    }
}
