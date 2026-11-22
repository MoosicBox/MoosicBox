//! Finds async code that `#[inject_yields]` does not cover.
//!
//! `switchy_async`'s `#[inject_yields]` attribute (and `inject_yields_mod!`) rewrites every
//! `.await` inside the item it is applied to, so that under the simulator other tasks may run
//! after each one. This crate reports the async code no such item covers:
//!
//! * `async fn`s, and async methods in impls and traits (with a body);
//! * `async` blocks and `async` closures, in any function (sync ones too).
//!
//! Code is covered when it, or any item around it (function, impl, trait, module, const or
//! static), has `#[inject_yields]` (by any path ending in `inject_yields`), or when it sits in
//! a module loaded with `inject_yields_mod!(mod name;)`. Async code inside an uncovered
//! `async fn`, block or closure is not reported again: covering the outer one covers it.
//! Code inside macro invocations is invisible to the attribute and to this check alike.
//!
//! [`uncovered`] walks a crate's module tree from its roots (`src/lib.rs`, `src/main.rs`,
//! `src/bin/*`), following `mod` declarations as rustc does, then checks the remaining `.rs`
//! files under `src` on their own.
//!
//! ```rust,no_run
//! let findings = switchy_async_cargo::uncovered(std::path::Path::new("."));
//! for finding in findings.iter().filter(|finding| !finding.cfg_test) {
//!     eprintln!("{finding}");
//! }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

use syn::{
    Attribute, Expr, ExprAsync, ExprClosure, ImplItemFn, Item, ItemConst, ItemFn, ItemImpl,
    ItemMod, ItemStatic, ItemTrait, Lit, Meta, TraitItemFn, spanned::Spanned as _, visit::Visit,
};

/// What a [`Finding`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// An `async fn` item.
    AsyncFn,
    /// An `async fn` in an impl or a trait (with a body).
    AsyncMethod,
    /// An `async` block.
    AsyncBlock,
    /// An `async` closure.
    AsyncClosure,
    /// A file that could not be read or parsed, so was not checked.
    Unreadable,
}

/// Async code no `#[inject_yields]` covers (or a file that could not be checked).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Finding {
    /// The file it is in.
    pub file: PathBuf,
    /// Its 1-based line (0 for an unreadable file).
    pub line: usize,
    /// Its 1-based column (0 for an unreadable file).
    pub column: usize,
    /// What it is.
    pub kind: Kind,
    /// The function's name ([`Kind::AsyncFn`], [`Kind::AsyncMethod`]), the function an
    /// `async` block or closure is in, or why a file is [`Kind::Unreadable`].
    pub name: Option<String>,
    /// Whether it is inside a `#[cfg(test)]` item (tests may not need yields).
    pub cfg_test: bool,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}: ", self.file.display(), self.line, self.column)?;
        let name = self.name.as_deref().unwrap_or("?");
        match self.kind {
            Kind::AsyncFn => write!(f, "async fn `{name}`")?,
            Kind::AsyncMethod => write!(f, "async method `{name}`")?,
            Kind::AsyncBlock => write!(f, "async block in `{name}`")?,
            Kind::AsyncClosure => write!(f, "async closure in `{name}`")?,
            Kind::Unreadable => return write!(f, "not checked: {name}"),
        }
        f.write_str(" is not covered by #[inject_yields]")
    }
}

/// The async code under `root/src` (a crate's directory) that `#[inject_yields]` does not
/// cover, ordered by file and position.
#[must_use]
pub fn uncovered(root: &Path) -> Vec<Finding> {
    let src = root.join("src");
    let mut walker = Walker::default();

    let mut roots = vec![src.join("lib.rs"), src.join("main.rs")];
    if let Ok(entries) = std::fs::read_dir(src.join("bin")) {
        let mut bins: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter_map(|path| {
                if path.is_dir() {
                    Some(path.join("main.rs"))
                } else {
                    (path.extension().is_some_and(|ext| ext == "rs")).then_some(path)
                }
            })
            .collect();
        bins.sort();
        roots.extend(bins);
    }
    for file in roots.into_iter().filter(|file| file.is_file()) {
        walker.check_file(&file, Dirs::of_file(&file, true), Scope::default());
    }

    // Files no module declaration reached (a layout this walk does not follow): on their own.
    let mut rest: Vec<PathBuf> = walkdir::WalkDir::new(&src)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    rest.sort();
    for file in rest {
        let mod_rs = is_mod_rs(&file);
        walker.check_file(&file, Dirs::of_file(&file, mod_rs), Scope::default());
    }

    walker.findings.sort();
    walker.findings
}

/// The async code in `source` (the contents of `file`) that `#[inject_yields]` does not cover,
/// without following its `mod` declarations.
///
/// # Errors
///
/// * If `source` does not parse as a Rust file
pub fn uncovered_in_source(file: &Path, source: &str) -> Result<Vec<Finding>, syn::Error> {
    let syntax = syn::parse_file(source)?;
    let mut walker = Walker {
        follow_modules: false,
        ..Walker::default()
    };
    let mut checker = Checker {
        walker: &mut walker,
        file,
        file_dirs: Dirs::of_file(file, true),
        dirs: Dirs::of_file(file, true),
        scope: Scope::default(),
        function: None,
    };
    checker.visit_file(&syntax);
    Ok(walker.findings)
}

/// Whether `attrs` hold `#[inject_yields]` (by any path ending in `inject_yields`).
#[must_use]
pub fn has_inject_yields(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "inject_yields")
    })
}

/// Whether `attrs` hold `#[cfg(test)]`.
fn has_cfg_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| match &attr.meta {
        Meta::List(list) => list.path.is_ident("cfg") && list.tokens.to_string().trim() == "test",
        _ => false,
    })
}

/// The `#[path = "..."]` attribute's value.
fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(meta) if meta.path.is_ident("path") => match &meta.value {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Str(value) => Some(value.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    })
}

/// Whether `file` is a "mod-rs" file, whose child modules sit beside it.
fn is_mod_rs(file: &Path) -> bool {
    matches!(
        file.file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "main.rs" | "mod.rs")
    )
}

/// Where a module's out-of-line children live (as `switchy_async_macros` resolves them).
#[derive(Debug, Clone)]
struct Dirs {
    /// `mod child;` loads `<children>/child.rs` or `<children>/child/mod.rs`.
    children: PathBuf,
    /// `#[path = "p"] mod child;` loads `<path_base>/p`.
    path_base: PathBuf,
}

impl Dirs {
    fn of_file(file: &Path, mod_rs: bool) -> Self {
        let parent = file.parent().map_or_else(PathBuf::new, Path::to_path_buf);
        let children = if mod_rs {
            parent.clone()
        } else {
            parent.join(file.file_stem().unwrap_or_default())
        };
        Self {
            children,
            path_base: parent,
        }
    }

    fn inline(&self, name: &str) -> Self {
        let dir = self.children.join(name);
        Self {
            children: dir.clone(),
            path_base: dir,
        }
    }

    /// The file of `mod <module>;` declared in a module with these directories, and whether
    /// it is a mod-rs file.
    fn resolve(&self, module: &ItemMod) -> Option<(PathBuf, bool)> {
        let name = module.ident.to_string();
        let candidates = path_attr(&module.attrs).map_or_else(
            || {
                vec![
                    (self.children.join(format!("{name}.rs")), false),
                    (self.children.join(&name).join("mod.rs"), true),
                ]
            },
            |path| vec![(self.path_base.join(path), true)],
        );
        candidates.into_iter().find(|(file, _)| file.is_file())
    }
}

/// What surrounds the code being checked.
#[derive(Debug, Clone, Copy, Default)]
struct Scope {
    /// An enclosing item has `#[inject_yields]`, or the module came from `inject_yields_mod!`.
    covered: bool,
    /// Inside an uncovered `async fn`, block or closure already reported.
    in_reported: bool,
    /// Inside a `#[cfg(test)]` item.
    cfg_test: bool,
}

#[derive(Debug)]
struct Walker {
    findings: Vec<Finding>,
    visited: BTreeSet<PathBuf>,
    follow_modules: bool,
}

impl Default for Walker {
    fn default() -> Self {
        Self {
            findings: Vec::new(),
            visited: BTreeSet::new(),
            follow_modules: true,
        }
    }
}

impl Walker {
    fn check_file(&mut self, file: &Path, dirs: Dirs, scope: Scope) {
        let key = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        if !self.visited.insert(key) {
            return;
        }
        let syntax = std::fs::read_to_string(file)
            .map_err(|e| e.to_string())
            .and_then(|source| syn::parse_file(&source).map_err(|e| e.to_string()));
        match syntax {
            Ok(syntax) => {
                let mut checker = Checker {
                    walker: self,
                    file,
                    file_dirs: dirs.clone(),
                    dirs,
                    scope,
                    function: None,
                };
                checker.visit_file(&syntax);
            }
            Err(error) => self.findings.push(Finding {
                file: file.to_path_buf(),
                line: 0,
                column: 0,
                kind: Kind::Unreadable,
                name: Some(error),
                cfg_test: scope.cfg_test,
            }),
        }
    }
}

struct Checker<'a> {
    walker: &'a mut Walker,
    file: &'a Path,
    /// The directories of the file's top-level module, which `inject_yields_mod!` resolves
    /// against wherever it is invoked in the file.
    file_dirs: Dirs,
    dirs: Dirs,
    scope: Scope,
    /// The innermost named function, for naming blocks and closures.
    function: Option<String>,
}

impl Checker<'_> {
    /// Run `f` with `scope` (and `dirs`), restoring both afterwards.
    fn within(&mut self, scope: Scope, dirs: Option<Dirs>, f: impl FnOnce(&mut Self)) {
        let saved_scope = std::mem::replace(&mut self.scope, scope);
        let saved_dirs = dirs.map(|dirs| std::mem::replace(&mut self.dirs, dirs));
        f(self);
        self.scope = saved_scope;
        if let Some(dirs) = saved_dirs {
            self.dirs = dirs;
        }
    }

    /// The scope inside an item with `attrs`.
    fn item_scope(&self, attrs: &[Attribute]) -> Scope {
        Scope {
            covered: self.scope.covered || has_inject_yields(attrs),
            cfg_test: self.scope.cfg_test || has_cfg_test(attrs),
            ..self.scope
        }
    }

    /// Report `kind` at `span` unless `scope` covers it or it is inside a reported one.
    /// Returns the scope for its contents.
    fn report(
        &mut self,
        scope: Scope,
        kind: Kind,
        span: proc_macro2::Span,
        name: Option<String>,
    ) -> Scope {
        if scope.covered || scope.in_reported {
            return scope;
        }
        let start = span.start();
        self.walker.findings.push(Finding {
            file: self.file.to_path_buf(),
            line: start.line,
            column: start.column + 1,
            kind,
            name,
            cfg_test: scope.cfg_test,
        });
        Scope {
            in_reported: true,
            ..scope
        }
    }

    fn check_module(&mut self, module: &ItemMod, scope: Scope, dirs: &Dirs) {
        if let Some((_, items)) = &module.content {
            self.within(
                scope,
                Some(dirs.inline(&module.ident.to_string())),
                |this| {
                    for item in items {
                        this.visit_item(item);
                    }
                },
            );
        } else if self.walker.follow_modules
            && let Some((file, mod_rs)) = dirs.resolve(module)
        {
            self.walker
                .check_file(&file, Dirs::of_file(&file, mod_rs), scope);
        }
    }

    /// Check a function named `name` with `attrs`, whose body `visit` walks.
    fn check_fn(
        &mut self,
        attrs: &[Attribute],
        name: String,
        is_async: bool,
        kind: Kind,
        span: proc_macro2::Span,
        visit: impl FnOnce(&mut Self),
    ) {
        let mut scope = self.item_scope(attrs);
        if is_async {
            scope = self.report(scope, kind, span, Some(name.clone()));
        }
        let saved = self.function.replace(name);
        self.within(scope, None, visit);
        self.function = saved;
    }
}

impl<'ast> Visit<'ast> for Checker<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.check_fn(
            &item.attrs,
            item.sig.ident.to_string(),
            item.sig.asyncness.is_some(),
            Kind::AsyncFn,
            item.sig.span(),
            |this| syn::visit::visit_item_fn(this, item),
        );
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.check_fn(
            &item.attrs,
            item.sig.ident.to_string(),
            item.sig.asyncness.is_some(),
            Kind::AsyncMethod,
            item.sig.span(),
            |this| syn::visit::visit_impl_item_fn(this, item),
        );
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        self.check_fn(
            &item.attrs,
            item.sig.ident.to_string(),
            item.sig.asyncness.is_some() && item.default.is_some(),
            Kind::AsyncMethod,
            item.sig.span(),
            |this| syn::visit::visit_trait_item_fn(this, item),
        );
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let scope = self.item_scope(&item.attrs);
        self.within(scope, None, |this| syn::visit::visit_item_impl(this, item));
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        let scope = self.item_scope(&item.attrs);
        self.within(scope, None, |this| syn::visit::visit_item_trait(this, item));
    }

    fn visit_item_const(&mut self, item: &'ast ItemConst) {
        let scope = self.item_scope(&item.attrs);
        self.within(scope, None, |this| syn::visit::visit_item_const(this, item));
    }

    fn visit_item_static(&mut self, item: &'ast ItemStatic) {
        let scope = self.item_scope(&item.attrs);
        self.within(scope, None, |this| {
            syn::visit::visit_item_static(this, item);
        });
    }

    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        let scope = Scope {
            in_reported: false,
            ..self.item_scope(&module.attrs)
        };
        let dirs = self.dirs.clone();
        let saved = self.function.take();
        self.check_module(module, scope, &dirs);
        self.function = saved;
    }

    fn visit_item(&mut self, item: &'ast Item) {
        if let Item::Macro(item_macro) = item
            && item_macro
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "inject_yields_mod")
            && let Ok(module) = item_macro.mac.parse_body::<ItemMod>()
        {
            let scope = Scope {
                covered: true,
                in_reported: false,
                ..self.item_scope(&module.attrs)
            };
            // As the macro does: against the invoking file, which may be a crate root with
            // another name than `lib.rs` or `main.rs`.
            let mut layouts = vec![self.file_dirs.clone()];
            if !is_mod_rs(self.file) {
                layouts.push(Dirs::of_file(self.file, true));
            }
            let dirs = layouts
                .iter()
                .find(|dirs| module.content.is_some() || dirs.resolve(&module).is_some())
                .unwrap_or(&layouts[0])
                .clone();
            self.check_module(&module, scope, &dirs);
            return;
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_expr_async(&mut self, expr: &'ast ExprAsync) {
        let scope = self.report(
            self.scope,
            Kind::AsyncBlock,
            expr.span(),
            self.function.clone(),
        );
        self.within(scope, None, |this| syn::visit::visit_expr_async(this, expr));
    }

    fn visit_expr_closure(&mut self, expr: &'ast ExprClosure) {
        let scope = if expr.asyncness.is_some() {
            self.report(
                self.scope,
                Kind::AsyncClosure,
                expr.span(),
                self.function.clone(),
            )
        } else {
            self.scope
        };
        self.within(scope, None, |this| {
            syn::visit::visit_expr_closure(this, expr);
        });
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// `(kind, name, line)` of each finding in `code`.
    fn check(code: &str) -> Vec<(Kind, String, usize)> {
        uncovered_in_source(Path::new("src/lib.rs"), code)
            .unwrap()
            .into_iter()
            .map(|finding| (finding.kind, finding.name.unwrap_or_default(), finding.line))
            .collect()
    }

    fn kinds(code: &str) -> Vec<(Kind, String)> {
        check(code)
            .into_iter()
            .map(|(kind, name, _)| (kind, name))
            .collect()
    }

    fn found(kind: Kind, name: &str) -> (Kind, String) {
        (kind, name.to_owned())
    }

    #[test]
    fn reports_async_fns_and_methods() {
        let code = "
            async fn free() {}
            fn sync() {}
            struct S;
            impl S {
                async fn method(&self) {}
                fn sync(&self) {}
            }
            impl Trait for S {
                async fn trait_method(&self) {}
            }
            trait T {
                async fn declared(&self);
                async fn defaulted(&self) {}
            }
        ";
        assert_eq!(
            kinds(code),
            [
                found(Kind::AsyncFn, "free"),
                found(Kind::AsyncMethod, "method"),
                found(Kind::AsyncMethod, "trait_method"),
                found(Kind::AsyncMethod, "defaulted"),
            ]
        );
    }

    #[test]
    fn reports_async_blocks_and_closures_in_sync_functions() {
        let code = "
            fn spawner() {
                spawn(async move { x.await });
                let c = async |y| y.await;
                let d = || async { z.await };
                let e = || 1;
            }
        ";
        assert_eq!(
            check(code),
            [
                (Kind::AsyncBlock, "spawner".to_owned(), 3),
                (Kind::AsyncClosure, "spawner".to_owned(), 4),
                (Kind::AsyncBlock, "spawner".to_owned(), 5),
            ]
        );
    }

    #[test]
    fn reports_only_the_outermost_uncovered_code() {
        let code = "
            async fn outer() {
                spawn(async { async {}.await });
            }
            fn f() {
                let g = async || async {};
            }
        ";
        assert_eq!(
            kinds(code),
            [
                found(Kind::AsyncFn, "outer"),
                found(Kind::AsyncClosure, "f")
            ]
        );
    }

    #[test]
    fn attributes_cover_everything_inside() {
        let code = "
            #[inject_yields]
            fn spawner() { spawn(async move { x.await }); }
            #[switchy_async::inject_yields]
            async fn qualified() {}
            #[inject_yields]
            impl S {
                async fn method(&self) {}
                fn sync(&self) { let _ = async {}; }
            }
            impl S {
                #[inject_yields]
                async fn covered(&self) {}
                async fn uncovered(&self) {}
            }
            #[inject_yields]
            trait T { async fn defaulted(&self) {} }
            #[inject_yields]
            mod m {
                async fn inner() {}
                mod deeper { fn f() { let _ = async {}; } }
            }
            #[inject_yields]
            static FUTURE: fn() = || { let _ = async {}; };
        ";
        assert_eq!(kinds(code), [found(Kind::AsyncMethod, "uncovered")]);
    }

    #[test]
    fn similar_attributes_do_not_cover() {
        let code = "
            #[inject_yield]
            async fn a() {}
            #[yields]
            async fn b() {}
        ";
        assert_eq!(
            kinds(code),
            [found(Kind::AsyncFn, "a"), found(Kind::AsyncFn, "b")]
        );
    }

    #[test]
    fn marks_cfg_test_code() {
        let code = "
            async fn real() {}
            #[cfg(test)]
            mod tests {
                async fn helper() {}
            }
            #[cfg(not(test))]
            async fn other() {}
        ";
        let findings = uncovered_in_source(Path::new("src/lib.rs"), code).unwrap();
        let marked: Vec<_> = findings
            .iter()
            .map(|finding| (finding.name.clone().unwrap(), finding.cfg_test))
            .collect();
        assert_eq!(
            marked,
            [
                ("real".to_owned(), false),
                ("helper".to_owned(), true),
                ("other".to_owned(), false)
            ]
        );
    }

    #[test]
    fn leaves_code_inside_macro_invocations() {
        assert_eq!(kinds("fn f() { spawn!(async { x.await }); }"), []);
    }

    #[test]
    fn displays_findings() {
        let findings = uncovered_in_source(
            Path::new("src/lib.rs"),
            "fn f() {\n    let _ = async {};\n}",
        )
        .unwrap();
        assert_eq!(
            findings[0].to_string(),
            "src/lib.rs:2:13: async block in `f` is not covered by #[inject_yields]"
        );
    }

    /// A directory of its own under the system's temporary directory.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("switchy_async_cargo_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, code: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, code).unwrap();
    }

    #[test]
    fn follows_the_module_tree() {
        let dir = scratch("tree");
        let src = dir.join("src");
        write(
            &src.join("lib.rs"),
            "mod plain;\n#[inject_yields]\nmod attributed;\ninject_yields_mod!(mod loaded;);\n#[cfg(test)]\nmod tests;\n",
        );
        write(&src.join("plain.rs"), "mod child;\nasync fn plain() {}\n");
        write(&src.join("plain/child.rs"), "async fn child() {}\n");
        write(&src.join("attributed.rs"), "mod child;\nasync fn a() {}\n");
        write(
            &src.join("attributed/child.rs"),
            "fn b() { let _ = async {}; }\n",
        );
        write(
            &src.join("loaded/mod.rs"),
            "#[path = \"other.rs\"]\nmod other;\n",
        );
        write(&src.join("loaded/other.rs"), "async fn c() {}\n");
        write(&src.join("tests.rs"), "async fn helper() {}\n");
        write(&src.join("stray.rs"), "async fn stray() {}\n");
        write(&src.join("bin/tool.rs"), "async fn tool() {}\n");
        write(&src.join("broken.rs"), "fn {\n");

        let findings = uncovered(&dir);
        let summary: Vec<_> = findings
            .iter()
            .map(|finding| {
                (
                    finding
                        .file
                        .strip_prefix(&src)
                        .unwrap()
                        .display()
                        .to_string(),
                    finding.kind,
                    finding.cfg_test,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("bin/tool.rs".to_owned(), Kind::AsyncFn, false),
                ("broken.rs".to_owned(), Kind::Unreadable, false),
                ("plain/child.rs".to_owned(), Kind::AsyncFn, false),
                ("plain.rs".to_owned(), Kind::AsyncFn, false),
                ("stray.rs".to_owned(), Kind::AsyncFn, false),
                ("tests.rs".to_owned(), Kind::AsyncFn, true),
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
