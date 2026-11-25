//! The source scan: what of a crate's compiled code reaches what no seed controls.
//!
//! The scan parses the crate (`syn`) from its roots (`src/lib.rs`, `src/main.rs`,
//! `src/bin/*.rs`), following `mod` declarations as rustc does (`#[path]` included), so it
//! reads the files the crate compiles and no others. Code is skipped when its `#[cfg]` is false
//! for a simulation build: `test`, and features the crate does not have enabled
//! (`#[cfg(not(feature = "simulator"))]` is native-only code). `cfg`s it cannot decide (the
//! operating system, `debug_assertions`) are scanned on both sides.
//!
//! Every path is resolved before it is judged: `use` trees with groups and aliases, glob
//! imports, `self`/`super`/`crate`, and re-exports through the crate's own modules, so
//! `use std::sync::Mutex as Lock;` or a `pub use` in another module cannot hide a primitive.
//! Paths resolving into a `switchy_*` crate are what the simulation controls, and allowed.
//! Macro invocations and `macro_rules!` bodies are scanned as tokens.
//!
//! It reports:
//!
//! * a path matching a forbidden pattern ([`FORBIDDEN`], the simulation's and the crate's own),
//!   once per import: a primitive imported under a name is reported at its `use`, not at
//!   each use of the name;
//! * a method call or string literal a pattern names (`.canonicalize`, `/proc/self`);
//! * process-wide mutable state: a `static` holding a lock, an atomic or a lazy value, and
//!   `thread_local!` (fleet workers run many runs on one thread);
//! * a `cfg` naming an operating system outside the files allowed to;
//! * an `extern` block: foreign functions, which reach the host past every simulator.
//!
//! A line that uses a primitive on purpose says why: `// simulation-safe: <reason>` on it or on
//! the line above (for a multi-line item, on any of its lines). `// native-only: begin` …
//! `// native-only: end` marks native-only lines the `cfg`s do not.
//!
//! # Patterns
//!
//! * `a::b::c`: a path whose segments contain `a`, `b`, `c` in a row (`std::fs` matches
//!   `std::fs::read`; `HashMap` matches any path through `HashMap`). A trailing `(`, `::` or
//!   `::{` is ignored.
//! * `.name` (also `.name(` or `.name()`): a method call named `name`; `).name` only when the
//!   receiver is itself a call.
//! * containing `/`: a string literal containing it.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use quote::ToTokens as _;
use syn::{Attribute, Expr, Item, ItemMod, ItemUse, Meta, UseTree, spanned::Spanned as _, visit::Visit};

use crate::manifest::Rules;

/// Primitives a simulation cannot control (see the [patterns](self#patterns)).
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
    "std::process::Command",
    // The host's process id: a run would depend on which process ran it.
    "std::process::id",
    "std::env::var",
    "std::env::var_os",
    "std::env::vars",
    "std::env::vars_os",
    // Files go through `switchy_fs` (the run's file system in simulation), and this process's
    // executable through the code's own platform layer.
    "std::fs",
    "tempfile",
    "std::env::current_exe",
    "/proc/self",
    // `Path`'s own file system queries always ask the host (use `switchy_fs`).
    ").is_dir",
    ").is_file",
    ".exists",
    ".try_exists",
    ".canonicalize",
    ".read_dir",
    ".read_link",
    ".symlink_metadata",
    "tokio::spawn",
    "tokio::time",
    "tokio::task",
    "tokio::runtime",
    "tokio::select",
    "tokio::test",
    // With several receivers, tokio's wakes them in an order no seed decides: use
    // switchy_async's (tokio's natively, deterministic in simulation).
    "tokio::sync::watch",
    "new_v4",
    "rand",
    // Iteration order differs between processes: use `BTreeMap`/`BTreeSet`, or mark a map only
    // ever looked up.
    "HashMap",
    "HashSet",
    // Addresses differ between runs: never order by them.
    ".as_ptr",
    // System calls and foreign functions reach the host directly, past every simulator (an
    // `extern` block is reported too).
    "libc",
    // The platform's math library: its results differ in the last bits between systems, so a
    // seed replays differently on another one. Use `libm`'s functions. (`sqrt`, `mul_add` and
    // `powi` are exact by IEEE 754 or computed alike everywhere, and allowed.)
    ".powf",
    ".exp",
    ".exp2",
    ".exp_m1",
    ".ln",
    ".ln_1p",
    ".log2",
    ".log10",
    ".cbrt",
    ".hypot",
    ".sin",
    ".cos",
    ".tan",
    ".sin_cos",
    ".asin",
    ".acos",
    ".atan",
    ".atan2",
    ".sinh",
    ".cosh",
    ".tanh",
    ".asinh",
    ".acosh",
    ".atanh",
];

/// One thing a scan reports.
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

/// A parsed pattern.
#[derive(Debug, Clone)]
enum Pattern {
    /// Path segments, matched as a run anywhere in a resolved path.
    Path(Vec<String>),
    /// A method call; `true` when the receiver must itself be a call.
    Method(String, bool),
    /// A string literal containing this.
    Literal(String),
}

impl Pattern {
    fn parse(text: &str) -> Self {
        let text = text.trim();
        if text.contains('/') {
            return Self::Literal(text.to_owned());
        }
        let trimmed = text
            .trim_end_matches(['(', ')'])
            .trim_end_matches("::{")
            .trim_end_matches("::");
        if let Some(name) = trimmed.strip_prefix(").") {
            return Self::Method(name.to_owned(), true);
        }
        if let Some(name) = trimmed.strip_prefix('.') {
            return Self::Method(name.to_owned(), false);
        }
        Self::Path(
            trimmed
                .split("::")
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }

    fn matches_path(&self, path: &[String]) -> bool {
        match self {
            Self::Path(pattern) => {
                !pattern.is_empty()
                    && path
                        .windows(pattern.len())
                        .any(|window| window == pattern.as_slice())
            }
            _ => false,
        }
    }
}

/// Scan the crate whose sources are under `src`, by its `rules`.
///
/// It forbids `forbidden` (the base list and the simulation's) on top of the crate's own;
/// `features` are the crate's enabled features in the simulation's build.
///
/// # Errors
///
/// * If a source file cannot be read
pub fn scan(
    src: &Path,
    rules: &Rules,
    forbidden: &[String],
    features: &BTreeSet<String>,
) -> std::io::Result<Vec<Finding>> {
    let patterns: Vec<Pattern> = forbidden
        .iter()
        .chain(&rules.forbid)
        .map(|p| Pattern::parse(p))
        .collect();
    let crate_tree = CrateTree::load(src, features)?;
    // `SWITCHY_CARGO_SHOW_UNREACHED`: name the source files no module declaration reached
    // (test-only modules, other platforms' backends, or a declaration the walk cannot see).
    if std::env::var_os("SWITCHY_CARGO_SHOW_UNREACHED").is_some() {
        let reached: BTreeSet<&str> = crate_tree.files.iter().map(|f| f.relative.as_str()).collect();
        let mut all = Vec::new();
        collect_sources(src, src, &mut all);
        for file in all.iter().filter(|f| !reached.contains(f.as_str())) {
            eprintln!("unreached: {}/{file}", src.display());
        }
    }
    let mut findings = Vec::new();
    for module in &crate_tree.files {
        let relative = &module.relative;
        if (!rules.only.is_empty()
            && !rules.only.iter().any(|only| relative.starts_with(only.as_str())))
            || rules
                .native
                .iter()
                .any(|native| relative.starts_with(native.as_str()))
            || rules.exempt.contains(relative)
            || relative.ends_with("_tests.rs")
            || relative == "tests.rs"
            || relative.ends_with("/tests.rs")
        {
            continue;
        }
        let os_allowed = rules
            .os_named_in
            .iter()
            .any(|prefix| relative.starts_with(prefix.as_str()));
        let mut scanner = Scanner {
            tree: &crate_tree,
            module: module.module.clone(),
            patterns: &patterns,
            features,
            os_allowed,
            lines: Lines::new(&module.source),
            findings: Vec::new(),
            reported: BTreeSet::new(),
        };
        match &module.syntax {
            Ok(file) => scanner.items(&file.items),
            Err(e) => scanner.findings.push((1, format!("cannot parse: {e}"))),
        }
        let mut lines = scanner.findings;
        lines.sort();
        lines.dedup();
        findings.extend(lines.into_iter().map(|(line, what)| Finding {
            file: relative.clone(),
            line,
            what,
        }));
    }
    findings.sort();
    Ok(findings)
}

/// Every `.rs` file under `dir`, relative to `src`.
fn collect_sources(src: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(src, &path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(
                path.strip_prefix(src)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    out.sort();
}

/// The lines a reason (`// simulation-safe:`) or a native-only region covers.
struct Lines {
    safe: BTreeSet<usize>,
    native: BTreeSet<usize>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let mut safe = BTreeSet::new();
        let mut native = BTreeSet::new();
        let mut in_native = false;
        for (index, line) in source.lines().enumerate() {
            let number = index + 1;
            let trimmed = line.trim_start();
            if trimmed.starts_with("// native-only: begin") {
                in_native = true;
            }
            if in_native {
                native.insert(number);
            }
            if trimmed.starts_with("// native-only: end") {
                in_native = false;
            }
            // A reason covers its own line and the one below it.
            if line.contains("// simulation-safe:") {
                safe.insert(number);
                safe.insert(number + 1);
            }
        }
        Self { safe, native }
    }

    /// Whether something on lines `start..=end` is covered: by a reason on one of them or on
    /// the line above, or by a native-only region.
    fn covered(&self, start: usize, end: usize) -> bool {
        (start..=end.max(start))
            .any(|line| self.safe.contains(&line) || self.native.contains(&line))
    }
}

/// One module's names: what its `use`s import and the items it defines.
#[derive(Debug, Default, Clone)]
struct Scope {
    /// Imported names, and the paths they stand for (as written in the import).
    uses: BTreeMap<String, Vec<String>>,
    /// Glob imports (`use a::b::*;`).
    globs: Vec<Vec<String>>,
    /// Names the module defines (items, and modules).
    items: BTreeSet<String>,
}

/// A compiled source file.
struct SourceFile {
    relative: String,
    /// Its module's path from the crate root (`["a", "b"]` for `crate::a::b`).
    module: Vec<String>,
    source: String,
    syntax: Result<syn::File, String>,
}

/// The crate's compiled files and every module's names.
struct CrateTree {
    files: Vec<SourceFile>,
    scopes: BTreeMap<Vec<String>, Scope>,
}

impl CrateTree {
    fn load(src: &Path, features: &BTreeSet<String>) -> std::io::Result<Self> {
        let mut tree = Self {
            files: Vec::new(),
            scopes: BTreeMap::new(),
        };
        let mut roots: Vec<PathBuf> = ["lib.rs", "main.rs"]
            .iter()
            .map(|name| src.join(name))
            .filter(|path| path.is_file())
            .collect();
        if let Ok(bins) = std::fs::read_dir(src.join("bin")) {
            let mut bins: Vec<PathBuf> = bins
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|e| e == "rs"))
                .collect();
            bins.sort();
            roots.extend(bins);
        }
        let mut seen = BTreeSet::new();
        for root in roots {
            tree.load_file(src, &root, Vec::new(), features, &mut seen)?;
        }
        Ok(tree)
    }

    fn load_file(
        &mut self,
        src: &Path,
        path: &Path,
        module: Vec<String>,
        features: &BTreeSet<String>,
        seen: &mut BTreeSet<PathBuf>,
    ) -> std::io::Result<()> {
        if !seen.insert(path.to_path_buf()) {
            return Ok(());
        }
        let source = std::fs::read_to_string(path)?;
        let relative = path
            .strip_prefix(src)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let syntax = syn::parse_file(&source).map_err(|e| e.to_string());
        let mut children = Vec::new();
        if let Ok(file) = &syntax {
            let dir = module_dir(path);
            self.index_items(&file.items, &module, &dir, features, &mut children);
        }
        self.files.push(SourceFile {
            relative,
            module,
            source,
            syntax,
        });
        for (file, module) in children {
            if file.is_file() {
                self.load_file(src, &file, module, features, seen)?;
            }
        }
        Ok(())
    }

    /// Record the names `items` (in `module`, whose child files live in `dir`) define and
    /// import, and the files of the modules they declare.
    fn index_items(
        &mut self,
        items: &[Item],
        module: &[String],
        dir: &Path,
        features: &BTreeSet<String>,
        children: &mut Vec<(PathBuf, Vec<String>)>,
    ) {
        for item in items {
            if !compiled(item_attrs(item), features) {
                continue;
            }
            match item {
                Item::Use(item_use) => {
                    let mut imports = Vec::new();
                    use_tree(&item_use.tree, &mut Vec::new(), &mut imports);
                    let scope = self.scopes.entry(module.to_vec()).or_default();
                    for (name, path) in imports {
                        match name {
                            Some(name) => {
                                scope.uses.insert(name, path);
                            }
                            None => scope.globs.push(path),
                        }
                    }
                }
                Item::Mod(item_mod) => {
                    let name = item_mod.ident.to_string();
                    self.scopes
                        .entry(module.to_vec())
                        .or_default()
                        .items
                        .insert(name.clone());
                    let mut child = module.to_vec();
                    child.push(name.clone());
                    let path_attr = path_attribute(&item_mod.attrs);
                    if let Some((_, inner)) = &item_mod.content {
                        let inner_dir =
                            path_attr.map_or_else(|| dir.join(&name), |p| dir.join(p));
                        self.index_items(inner, &child, &inner_dir, features, children);
                    } else {
                        let file = path_attr.map_or_else(
                            || {
                                let flat = dir.join(format!("{name}.rs"));
                                if flat.is_file() {
                                    flat
                                } else {
                                    dir.join(&name).join("mod.rs")
                                }
                            },
                            |p| dir.join(p),
                        );
                        children.push((file, child));
                    }
                }
                Item::Macro(item_macro) if item_macro.ident.is_none() => {
                    // A macro wrapping items (`inject_yields_mod! { mod name; }`) declares
                    // them here.
                    if let Some(inner) = wrapped_items(&item_macro.mac.tokens) {
                        self.index_items(&inner, module, dir, features, children);
                    }
                }
                other => {
                    if let Some(name) = item_name(other) {
                        self.scopes
                            .entry(module.to_vec())
                            .or_default()
                            .items
                            .insert(name);
                    }
                }
            }
        }
    }

    /// What `path`, as written in `module`, may stand for: the path outside the crate it
    /// resolves to, or `crate::...` for the crate's own item (several candidates when a glob
    /// import may supply the name). Gives up after a few re-exports.
    fn resolve(&self, module: &[String], path: &[String]) -> Vec<Vec<String>> {
        let mut candidates = self.resolve_depth(module, path, 0, true);
        candidates.sort();
        candidates.dedup();
        candidates
    }

    /// `globs`: whether a name not found may come from the module's glob imports. A path
    /// formed from a glob's own path is not expanded through the globs again: two globs that
    /// import each other's modules (`use crate::*;` beside `pub use generated::*;`) would
    /// otherwise expand without end.
    fn resolve_depth(
        &self,
        module: &[String],
        path: &[String],
        depth: usize,
        globs: bool,
    ) -> Vec<Vec<String>> {
        if path.is_empty() || depth > 16 {
            return vec![path.to_vec()];
        }
        let first = path[0].as_str();
        // An absolute path into the crate: walk its modules, following re-exports.
        let absolute: Option<Vec<String>> = match first {
            "crate" => Some(path[1..].to_vec()),
            "self" => Some(module.iter().chain(&path[1..]).cloned().collect()),
            "super" => {
                let mut base = module.to_vec();
                let mut rest = path;
                while rest.first().is_some_and(|s| s == "super") {
                    base.pop();
                    rest = &rest[1..];
                }
                Some(base.into_iter().chain(rest.iter().cloned()).collect())
            }
            _ => None,
        };
        if let Some(absolute) = absolute {
            return self.resolve_in_crate(&absolute, depth);
        }
        let scope = self.scopes.get(module);
        if let Some(target) = scope.and_then(|s| s.uses.get(first)) {
            let full: Vec<String> = target.iter().chain(&path[1..]).cloned().collect();
            // `use tokio;` names the extern crate itself.
            if target.first().is_some_and(|t| t == first) && target.len() == 1 {
                return vec![full];
            }
            // An import's path resolves where it is written (2018's uniform paths).
            return self.resolve_depth(module, &full, depth + 1, globs);
        }
        if scope.is_some_and(|s| s.items.contains(first)) {
            let absolute: Vec<String> = module.iter().chain(path).cloned().collect();
            return self.resolve_in_crate(&absolute, depth);
        }
        // Not named here: an external crate or the prelude, or something a glob brought in.
        let mut candidates = vec![path.to_vec()];
        if let Some(scope) = scope.filter(|_| globs) {
            for glob in &scope.globs {
                let full: Vec<String> = glob.iter().chain(path).cloned().collect();
                candidates.extend(self.resolve_depth(module, &full, depth + 1, false));
            }
        }
        candidates
    }

    /// Resolve `crate::<absolute>` through the crate's modules and their re-exports.
    fn resolve_in_crate(&self, absolute: &[String], depth: usize) -> Vec<Vec<String>> {
        for split in (0..absolute.len()).rev() {
            let (module, rest) = absolute.split_at(split);
            let Some(scope) = self.scopes.get(module) else {
                continue;
            };
            let Some(name) = rest.first() else {
                continue;
            };
            if let Some(target) = scope.uses.get(name) {
                let full: Vec<String> = target.iter().chain(&rest[1..]).cloned().collect();
                return self.resolve_depth(module, &full, depth + 1, true);
            }
            if scope.items.contains(name) {
                break;
            }
        }
        vec![
            std::iter::once("crate".to_owned())
                .chain(absolute.iter().cloned())
                .collect(),
        ]
    }
}

/// The items a macro invocation's tokens hold, when they are items with a module or an import
/// among them (`inject_yields_mod! { mod name; }`): the macro passes them through, so they are
/// the module's own.
fn wrapped_items(tokens: &TokenStream) -> Option<Vec<Item>> {
    let file = syn::parse2::<syn::File>(tokens.clone()).ok()?;
    file.items
        .iter()
        .any(|item| matches!(item, Item::Mod(_) | Item::Use(_)))
        .then_some(file.items)
}

/// The directory a module file's child modules live in.
fn module_dir(path: &Path) -> PathBuf {
    let dir = path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_path_buf();
    match path.file_name().and_then(|n| n.to_str()) {
        Some("lib.rs" | "main.rs" | "mod.rs") => dir,
        _ => dir.join(
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default(),
        ),
    }
}

fn path_attribute(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(meta) if meta.path.is_ident("path") => match &meta.value {
            Expr::Lit(lit) => match &lit.lit {
                syn::Lit::Str(s) => Some(s.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    })
}

/// An import: the name it brings in (`None` for a glob) and the path it stands for.
type Import = (Option<String>, Vec<String>);

/// The imports of a `use` tree.
fn use_tree(tree: &UseTree, prefix: &mut Vec<String>, out: &mut Vec<Import>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            use_tree(&path.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Name(name) => {
            let ident = name.ident.to_string();
            if ident == "self" {
                if let Some(last) = prefix.last() {
                    out.push((Some(last.clone()), prefix.clone()));
                }
            } else {
                let mut path = prefix.clone();
                path.push(ident.clone());
                out.push((Some(ident), path));
            }
        }
        UseTree::Rename(rename) => {
            let ident = rename.ident.to_string();
            let mut path = prefix.clone();
            if ident != "self" {
                path.push(ident);
            }
            let alias = rename.rename.to_string();
            // `as _` brings in no name, but is still an import.
            let name = if alias == "_" {
                format!("_{}", path.join("::"))
            } else {
                alias
            };
            out.push((Some(name), path));
        }
        UseTree::Glob(_) => out.push((None, prefix.clone())),
        UseTree::Group(group) => {
            for tree in &group.items {
                use_tree(tree, prefix, out);
            }
        }
    }
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

fn item_name(item: &Item) -> Option<String> {
    Some(match item {
        Item::Const(i) => i.ident.to_string(),
        Item::Enum(i) => i.ident.to_string(),
        Item::Fn(i) => i.sig.ident.to_string(),
        Item::Static(i) => i.ident.to_string(),
        Item::Struct(i) => i.ident.to_string(),
        Item::Trait(i) => i.ident.to_string(),
        Item::Type(i) => i.ident.to_string(),
        Item::Union(i) => i.ident.to_string(),
        Item::Macro(i) => i.ident.as_ref()?.to_string(),
        _ => return None,
    })
}

/// What a `cfg` predicate is for a simulation build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cfg {
    True,
    False,
    /// It depends on the host (`unix`, `target_os`, `debug_assertions`, ...).
    Unknown,
}

fn eval_cfg(meta: &Meta, features: &BTreeSet<String>) -> Cfg {
    match meta {
        Meta::Path(path) => {
            if path.is_ident("test") || path.is_ident("doc") || path.is_ident("miri") {
                Cfg::False
            } else {
                Cfg::Unknown
            }
        }
        Meta::NameValue(nv) if nv.path.is_ident("feature") => match &nv.value {
            Expr::Lit(lit) => match &lit.lit {
                syn::Lit::Str(s) if features.contains(&s.value()) => Cfg::True,
                syn::Lit::Str(_) => Cfg::False,
                _ => Cfg::Unknown,
            },
            _ => Cfg::Unknown,
        },
        Meta::NameValue(_) => Cfg::Unknown,
        Meta::List(list) => {
            let Ok(args) = list.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) else {
                return Cfg::Unknown;
            };
            let values: Vec<Cfg> = args.iter().map(|m| eval_cfg(m, features)).collect();
            if list.path.is_ident("not") {
                match values.first() {
                    Some(Cfg::True) => Cfg::False,
                    Some(Cfg::False) => Cfg::True,
                    _ => Cfg::Unknown,
                }
            } else if list.path.is_ident("all") {
                if values.contains(&Cfg::False) {
                    Cfg::False
                } else if values.iter().all(|v| *v == Cfg::True) {
                    Cfg::True
                } else {
                    Cfg::Unknown
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Cfg::True) {
                    Cfg::True
                } else if values.iter().all(|v| *v == Cfg::False) {
                    Cfg::False
                } else {
                    Cfg::Unknown
                }
            } else {
                Cfg::Unknown
            }
        }
    }
}

/// Whether code with these attributes is (or may be) compiled in a simulation build.
fn compiled(attrs: &[Attribute], features: &BTreeSet<String>) -> bool {
    attrs.iter().all(|attr| {
        if attr.path().is_ident("test") {
            return false;
        }
        if !attr.path().is_ident("cfg") {
            return true;
        }
        let Meta::List(list) = &attr.meta else {
            return true;
        };
        list.parse_args::<Meta>()
            .map_or(true, |meta| eval_cfg(&meta, features) != Cfg::False)
    })
}

fn names_an_operating_system(tokens: &TokenStream) -> bool {
    tokens.clone().into_iter().any(|tree| match tree {
        TokenTree::Ident(ident) => {
            let name = ident.to_string();
            ["unix", "windows", "target_os", "target_family"].contains(&name.as_str())
        }
        TokenTree::Group(group) => names_an_operating_system(&group.stream()),
        _ => false,
    })
}

fn is_parenthesized(tree: Option<&TokenTree>) -> bool {
    matches!(tree, Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
}

fn is_colon(tree: Option<&TokenTree>) -> bool {
    matches!(tree, Some(TokenTree::Punct(p)) if p.as_char() == ':')
}

/// Scans one file.
struct Scanner<'a> {
    tree: &'a CrateTree,
    module: Vec<String>,
    patterns: &'a [Pattern],
    features: &'a BTreeSet<String>,
    os_allowed: bool,
    lines: Lines,
    findings: Vec<(usize, String)>,
    /// Names whose imports were judged already (reported, or marked safe), so their uses are
    /// not reported again.
    reported: BTreeSet<String>,
}

impl Scanner<'_> {
    fn covered(&self, span: Span) -> bool {
        self.lines.covered(span.start().line, span.end().line)
    }

    fn report(&mut self, span: Span, what: String) {
        if !self.covered(span) {
            self.findings.push((span.start().line, what));
        }
    }

    fn forbidden_path(&self, path: &[String]) -> Option<Vec<String>> {
        self.tree
            .resolve(&self.module, path)
            .into_iter()
            .filter(|resolved| !resolved.first().is_some_and(|s| s.starts_with("switchy")))
            .find(|resolved| self.patterns.iter().any(|p| p.matches_path(resolved)))
    }

    fn item(&mut self, item: &Item) {
        if !compiled(item_attrs(item), self.features) {
            return;
        }
        match item {
            Item::Mod(item_mod) => self.module_item(item_mod),
            Item::Use(item_use) => self.use_item(item_use),
            Item::Static(item_static) => {
                self.attributes(&item_static.attrs);
                let ty = item_static.ty.to_token_stream().to_string();
                if [
                    "OnceLock", "LazyLock", "OnceCell", "Lazy <", "Atomic", "Mutex", "RwLock",
                ]
                .iter()
                .any(|kind| ty.contains(kind))
                {
                    self.report(
                        item_static.span(),
                        format!("process-wide: static {}: {ty}", item_static.ident),
                    );
                }
                syn::visit::visit_item_static(self, item_static);
            }
            other => self.visit_item(other),
        }
    }

    /// A module's items: its imports first, as they apply to the whole module wherever they
    /// are written.
    fn items(&mut self, items: &[Item]) {
        // Items a macro wraps (`inject_yields_mod! { mod name; }`) are the module's own.
        let mut expanded: Vec<Item> = Vec::new();
        for item in items {
            match item {
                Item::Macro(item_macro)
                    if item_macro.ident.is_none()
                        && compiled(&item_macro.attrs, self.features) =>
                {
                    match wrapped_items(&item_macro.mac.tokens) {
                        Some(inner) => expanded.extend(inner),
                        None => expanded.push(item.clone()),
                    }
                }
                other => expanded.push(other.clone()),
            }
        }
        for item in &expanded {
            if let Item::Use(item_use) = item
                && compiled(&item_use.attrs, self.features)
            {
                self.use_item(item_use);
            }
        }
        for item in &expanded {
            if !matches!(item, Item::Use(_)) {
                self.item(item);
            }
        }
    }

    fn module_item(&mut self, item_mod: &ItemMod) {
        self.attributes(&item_mod.attrs);
        // A file module is scanned as its own file; an inline one here, in its own scope.
        if let Some((_, items)) = &item_mod.content {
            let outer = self.module.clone();
            self.module.push(item_mod.ident.to_string());
            self.items(items);
            self.module = outer;
        }
    }

    fn use_item(&mut self, item_use: &ItemUse) {
        self.attributes(&item_use.attrs);
        let mut imports = Vec::new();
        use_tree(&item_use.tree, &mut Vec::new(), &mut imports);
        let safe = self.covered(item_use.span());
        let line = item_use.span().start().line;
        for (name, path) in imports {
            let Some(resolved) = self.forbidden_path(&path) else {
                continue;
            };
            if let Some(name) = name {
                self.reported.insert(name);
            }
            if !safe {
                let written = path.join("::");
                let resolved = resolved.join("::");
                let what = if written == resolved {
                    format!("imports {written}")
                } else {
                    format!("imports {written} (= {resolved})")
                };
                self.findings.push((line, what));
            }
        }
    }

    fn attributes(&mut self, attrs: &[Attribute]) {
        if self.os_allowed {
            return;
        }
        for attr in attrs {
            let is_cfg = attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr");
            if let (true, Meta::List(list)) = (is_cfg, &attr.meta)
                && names_an_operating_system(&list.tokens)
            {
                // A reason does not excuse naming an operating system.
                self.findings.push((
                    attr.span().start().line,
                    format!("names an OS: {}", attr.to_token_stream()),
                ));
            }
        }
    }

    /// Paths, method calls and literals in a macro's tokens.
    fn tokens(&mut self, tokens: &TokenStream) {
        let trees: Vec<TokenTree> = tokens.clone().into_iter().collect();
        let mut index = 0;
        while index < trees.len() {
            match &trees[index] {
                TokenTree::Group(group) => self.tokens(&group.stream()),
                TokenTree::Literal(literal) => {
                    let text = literal.to_string();
                    self.literal(literal.span(), &text);
                }
                TokenTree::Punct(punct) if punct.as_char() == '.' => {
                    if let Some(TokenTree::Ident(name)) = trees.get(index + 1)
                        && is_parenthesized(trees.get(index + 2))
                    {
                        let after_call = index > 0 && is_parenthesized(trees.get(index - 1));
                        self.method(name.span(), &name.to_string(), after_call);
                    }
                }
                TokenTree::Ident(ident) => {
                    // A path: ident (:: ident)*.
                    let mut path = vec![ident.to_string()];
                    let mut next = index + 1;
                    while is_colon(trees.get(next)) && is_colon(trees.get(next + 1)) {
                        let Some(TokenTree::Ident(segment)) = trees.get(next + 2) else {
                            break;
                        };
                        path.push(segment.to_string());
                        next += 3;
                    }
                    // A method name after `.` is not a path.
                    let after_dot = index > 0
                        && matches!(&trees[index - 1], TokenTree::Punct(p) if p.as_char() == '.');
                    if !after_dot {
                        self.path(ident.span(), &path);
                    }
                    index = next;
                    continue;
                }
                TokenTree::Punct(_) => {}
            }
            index += 1;
        }
    }

    fn path(&mut self, span: Span, path: &[String]) {
        // A name whose import was judged already stands for what it imported.
        if path
            .first()
            .is_some_and(|first| self.reported.contains(first))
        {
            return;
        }
        if let Some(resolved) = self.forbidden_path(path) {
            self.report(span, format!("uses {}", resolved.join("::")));
        }
    }

    fn method(&mut self, span: Span, name: &str, after_call: bool) {
        let hit = self.patterns.iter().any(|p| match p {
            Pattern::Method(method, needs_call) => {
                method == name && (!needs_call || after_call)
            }
            Pattern::Path(segments) => segments.len() == 1 && segments[0] == name,
            Pattern::Literal(_) => false,
        });
        if hit {
            self.report(span, format!("calls .{name}()"));
        }
    }

    fn literal(&mut self, span: Span, text: &str) {
        let hit = self.patterns.iter().find_map(|p| match p {
            Pattern::Literal(needle) if text.contains(needle.as_str()) => Some(needle.clone()),
            _ => None,
        });
        if let Some(needle) = hit {
            self.report(span, format!("names {needle}"));
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Mod(_) | Item::Use(_) | Item::Static(_) => self.item(item),
            Item::Macro(item_macro) => {
                if compiled(&item_macro.attrs, self.features) {
                    self.attributes(&item_macro.attrs);
                    let name = item_macro
                        .mac
                        .path
                        .segments
                        .last()
                        .map(|s| s.ident.to_string());
                    if name.as_deref() == Some("thread_local") {
                        self.report(
                            item_macro.span(),
                            "process-wide: thread_local!".to_owned(),
                        );
                    }
                    self.visit_path(&item_macro.mac.path);
                    self.tokens(&item_macro.mac.tokens);
                }
            }
            Item::ForeignMod(foreign) => {
                if compiled(&foreign.attrs, self.features) {
                    self.attributes(&foreign.attrs);
                    self.report(
                        foreign.span(),
                        "foreign functions: an `extern` block".to_owned(),
                    );
                }
            }
            other => {
                if compiled(item_attrs(other), self.features) {
                    self.attributes(item_attrs(other));
                    syn::visit::visit_item(self, other);
                }
            }
        }
    }

    fn visit_item_use(&mut self, item_use: &'ast ItemUse) {
        self.use_item(item_use);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if compiled(&item.attrs, self.features) {
            self.attributes(&item.attrs);
            syn::visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if compiled(&item.attrs, self.features) {
            syn::visit::visit_trait_item_fn(self, item);
        }
    }

    fn visit_stmt(&mut self, stmt: &'ast syn::Stmt) {
        let attrs: &[Attribute] = match stmt {
            syn::Stmt::Local(local) => &local.attrs,
            syn::Stmt::Macro(mac) => &mac.attrs,
            syn::Stmt::Item(item) => item_attrs(item),
            syn::Stmt::Expr(..) => &[],
        };
        if compiled(attrs, self.features) {
            syn::visit::visit_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'ast Expr) {
        let attrs: &[Attribute] = match expr {
            Expr::Block(e) => &e.attrs,
            Expr::Call(e) => &e.attrs,
            Expr::MethodCall(e) => &e.attrs,
            Expr::Macro(e) => &e.attrs,
            Expr::Match(e) => &e.attrs,
            _ => &[],
        };
        if compiled(attrs, self.features) {
            syn::visit::visit_expr(self, expr);
        }
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        if compiled(&arm.attrs, self.features) {
            syn::visit::visit_arm(self, arm);
        }
    }

    fn visit_field(&mut self, field: &'ast syn::Field) {
        if compiled(&field.attrs, self.features) {
            syn::visit::visit_field(self, field);
        }
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        if compiled(&variant.attrs, self.features) {
            syn::visit::visit_variant(self, variant);
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        self.path(path.span(), &segments);
        // Generic arguments hold paths of their own.
        syn::visit::visit_path(self, path);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let after_call = matches!(*call.receiver, Expr::Call(_) | Expr::MethodCall(_));
        self.method(call.method.span(), &call.method.to_string(), after_call);
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
        self.literal(literal.span(), &literal.value());
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.visit_path(&mac.path);
        self.tokens(&mac.tokens);
    }

    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        // Attribute macros (`#[tokio::test]`); a `cfg`'s words are not paths.
        let path = attr.path();
        if !(path.is_ident("cfg") || path.is_ident("cfg_attr") || path.is_ident("doc")) {
            self.visit_path(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    const NONE: [&str; 0] = [];

    /// Scan `files` (path under `src/`, source) as a crate with `features` enabled.
    fn scan_crate(files: &[(&str, &str)], rules: &Rules, features: &[&str]) -> Vec<String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "switchy-cargo-scan-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for (path, source) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        let forbidden: Vec<String> = FORBIDDEN.iter().map(ToString::to_string).collect();
        let features = features.iter().map(ToString::to_string).collect();
        let findings = scan(&dir, rules, &forbidden, &features).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        findings.iter().map(ToString::to_string).collect()
    }

    fn check(source: &str) -> Vec<String> {
        scan_crate(&[("lib.rs", source)], &Rules::default(), &[])
    }

    #[test]
    fn forbidden_primitives_are_reported() {
        assert_eq!(
            check("fn f() { let m = std::sync::Mutex::new(0); }"),
            ["src/lib.rs:1: uses std::sync::Mutex::new"]
        );
        assert_eq!(
            check("fn f() { let m: HashMap<u8, u8> = HashMap::new(); }").len(),
            2
        );
        assert_eq!(check("fn f() { let id = std::process::id(); }").len(), 1);
    }

    #[test]
    fn foreign_functions_are_reported() {
        assert_eq!(
            check("unsafe extern \"C\" {\n    fn getpid() -> i32;\n}"),
            ["src/lib.rs:1: foreign functions: an `extern` block"]
        );
        assert_eq!(
            check("fn f() -> i32 { unsafe { libc::getpid() } }").len(),
            1
        );
        assert_eq!(
            check("#[cfg(feature = \"native\")]\nunsafe extern \"C\" { fn getpid() -> i32; }"),
            NONE
        );
    }

    #[test]
    fn the_platforms_math_library_is_reported_and_exact_operations_are_not() {
        assert_eq!(check("fn f(x: f64) -> f64 { x.powf(0.5) }").len(), 1);
        assert_eq!(check("fn f(x: f64) -> f64 { x.sin() + x.ln() }").len(), 2);
        assert_eq!(
            check("fn f(x: f64) -> f64 { x.sqrt() + x.powi(3) + x.mul_add(2.0, 1.0) }"),
            NONE
        );
        assert_eq!(check("fn f(x: f64) -> f64 { libm::pow(x, 0.5) }"), NONE);
    }

    #[test]
    fn switchy_versions_are_allowed() {
        assert_eq!(
            check("use switchy_thread::sync::Mutex;\nfn f() { Mutex::new(0); }"),
            NONE
        );
        assert_eq!(check("fn f() { let n = switchy_time::now(); }"), NONE);
        assert_eq!(
            check("use switchy_fs as fs;\nfn f() { fs::sync::read(\"x\"); }"),
            NONE
        );
    }

    #[test]
    fn aliases_and_groups_resolve_to_what_they_import() {
        assert_eq!(
            check("use std::{path::PathBuf, sync::{Arc, Mutex as Lock}};\nfn f() { Lock::new(0); }"),
            ["src/lib.rs:1: imports std::sync::Mutex"]
        );
        assert_eq!(check("use std::sync::{Arc, PoisonError};"), NONE);
        assert_eq!(check("use std::{fs, io};").len(), 1);
    }

    #[test]
    fn the_crates_own_re_exports_are_followed() {
        let findings = scan_crate(
            &[
                ("lib.rs", "mod sync;\nuse crate::sync::Lock;\nfn f() { Lock::new(0); }"),
                ("sync.rs", "pub use std::sync::Mutex as Lock;"),
            ],
            &Rules::default(),
            &[],
        );
        assert_eq!(
            findings,
            [
                "src/lib.rs:2: imports crate::sync::Lock (= std::sync::Mutex)",
                "src/sync.rs:1: imports std::sync::Mutex",
            ]
        );
    }

    #[test]
    fn an_import_marked_safe_covers_uses_written_before_it() {
        assert_eq!(
            check("struct S(Mutex<u8>);\n// simulation-safe: x\nuse std::sync::Mutex;"),
            NONE
        );
    }

    #[test]
    fn glob_imports_are_followed() {
        assert_eq!(
            check("use std::sync::*;\nfn f() { Mutex::new(0); }"),
            ["src/lib.rs:2: uses std::sync::Mutex::new"]
        );
    }

    #[test]
    fn glob_imports_of_each_other_resolve_quickly() {
        // As in objc2's generated bindings: the module globs the crate, and the crate
        // re-exports the module's items. Expanding each glob through the others again
        // grew exponentially.
        let findings = scan_crate(
            &[
                ("lib.rs", "mod generated;\npub use generated::*;\npub fn g() {}"),
                (
                    "generated.rs",
                    "use core::ffi::*;\nuse core::ptr::*;\nuse crate::*;\nuse std::sync::*;\n\
                     fn f() { a::b::c::d::e(); Mutex::new(0); }",
                ),
            ],
            &Rules::default(),
            &[],
        );
        assert_eq!(findings, ["src/generated.rs:5: uses std::sync::Mutex::new"]);
    }

    #[test]
    fn a_reason_on_the_line_or_above_opts_out() {
        assert_eq!(
            check("fn f() { let m = std::sync::Mutex::new(0); } // simulation-safe: x"),
            NONE
        );
        assert_eq!(
            check("// simulation-safe: x\nuse std::sync::Mutex;\nfn f() { Mutex::new(0); }"),
            NONE
        );
        assert_eq!(
            check("use std::{\n    sync::{Arc, Mutex}, // simulation-safe: x\n};"),
            NONE
        );
    }

    #[test]
    fn code_a_simulation_build_does_not_compile_is_skipped() {
        let source = "#[cfg(test)]\nmod tests { use std::fs; }\n\
                      #[cfg(not(feature = \"simulator\"))]\nfn native() { std::fs::read(\"x\"); }\n\
                      #[cfg(feature = \"simulator\")]\nfn simulated() { std::fs::read(\"x\"); }";
        assert_eq!(
            scan_crate(&[("lib.rs", source)], &Rules::default(), &["simulator"]),
            ["src/lib.rs:6: uses std::fs::read"]
        );
    }

    #[test]
    fn only_compiled_files_are_read() {
        let findings = scan_crate(
            &[
                ("lib.rs", "mod a;\n#[cfg(test)]\nmod compatibility;"),
                ("a.rs", "fn f() { std::fs::read(\"x\"); }"),
                ("compatibility.rs", "fn f() { std::fs::read(\"x\"); }"),
                ("stray.rs", "fn f() { std::fs::read(\"x\"); }"),
            ],
            &Rules::default(),
            &[],
        );
        assert_eq!(findings, ["src/a.rs:1: uses std::fs::read"]);
    }

    #[test]
    fn modules_a_macro_declares_are_followed() {
        let findings = scan_crate(
            &[
                ("lib.rs", "switchy_async::inject_yields_mod! { mod a; }"),
                ("a.rs", "use std::time::Instant as Clock;\nfn f() { Clock::now(); }"),
            ],
            &Rules::default(),
            &[],
        );
        assert_eq!(findings, ["src/a.rs:1: imports std::time::Instant"]);
    }

    #[test]
    fn macro_bodies_are_scanned_as_tokens() {
        assert_eq!(
            check("fn f() { dbg!(std::env::var(\"X\")); }"),
            ["src/lib.rs:1: uses std::env::var"]
        );
        assert_eq!(
            check("macro_rules! m { () => { std::thread::sleep(d) }; }"),
            ["src/lib.rs:1: uses std::thread::sleep"]
        );
        assert_eq!(
            check("fn f() { tokio::select! { _ = a => {} } }"),
            ["src/lib.rs:1: uses tokio::select"]
        );
    }

    #[test]
    fn methods_and_literals_named_by_patterns_are_reported() {
        assert_eq!(
            check("fn f(p: &Path) { p.canonicalize(); }"),
            ["src/lib.rs:1: calls .canonicalize()"]
        );
        assert_eq!(
            check("fn f() { Path::new(x).is_dir(); }"),
            ["src/lib.rs:1: calls .is_dir()"]
        );
        assert_eq!(check("fn f(m: &Metadata) { m.is_dir(); }"), NONE);
        assert_eq!(
            check("fn f() { open(\"/proc/self/exe\"); }"),
            ["src/lib.rs:1: names /proc/self"]
        );
    }

    #[test]
    fn process_wide_state_is_reported() {
        assert_eq!(check("static X: OnceLock<u8> = OnceLock::new();").len(), 1);
        assert_eq!(
            check("pub(crate) static X: AtomicU64 = AtomicU64::new(0);").len(),
            1
        );
        assert_eq!(check("static NAMES: &[&str] = &[];"), NONE);
        assert_eq!(
            check("thread_local! { static X: u8 = 0; }"),
            ["src/lib.rs:1: process-wide: thread_local!"]
        );
    }

    #[test]
    fn operating_systems_are_named_only_where_allowed() {
        assert_eq!(check("#[cfg(unix)]\nfn f() {}").len(), 1);
        let allowed = Rules {
            os_named_in: vec!["lib.rs".into()],
            ..Rules::default()
        };
        assert_eq!(
            scan_crate(&[("lib.rs", "#[cfg(unix)]\nfn f() {}")], &allowed, &[]),
            NONE
        );
        // A reason does not excuse naming an operating system.
        assert_eq!(
            check("#[cfg(windows)] // simulation-safe: x\nfn f() {}").len(),
            1
        );
    }

    #[test]
    fn native_only_regions_are_skipped() {
        let source = "// native-only: begin\nuse std::sync::Mutex;\n// native-only: end\n\
                      use std::sync::RwLock;";
        assert_eq!(check(source), ["src/lib.rs:4: imports std::sync::RwLock"]);
    }

    #[test]
    fn patterns_parse_as_paths_methods_and_literals() {
        let path = |p: &[&str]| p.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert!(Pattern::parse("std::fs").matches_path(&path(&["std", "fs", "read"])));
        assert!(Pattern::parse("Session::new(").matches_path(&path(&["a", "Session", "new"])));
        assert!(!Pattern::parse("std::env::var").matches_path(&path(&["std", "env", "vars"])));
        assert!(matches!(Pattern::parse(".exists()"), Pattern::Method(m, false) if m == "exists"));
        assert!(matches!(Pattern::parse(").is_dir()"), Pattern::Method(m, true) if m == "is_dir"));
        assert!(matches!(Pattern::parse("/proc/self"), Pattern::Literal(_)));
    }
}
