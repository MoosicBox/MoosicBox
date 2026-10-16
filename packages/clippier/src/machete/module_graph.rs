//! Source-instance discovery and finite, namespace-aware local import resolution.
//!
//! This graph deliberately does not invent exports for external crates or macros.
use super::{AnalysisProfile, BoxError, ConditionValue, evaluate_condition};
use quote::ToTokens;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Component, Path, PathBuf},
    rc::Rc,
};
use syn::spanned::Spanned;

pub(super) type ModuleId = usize;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Namespace {
    Type,
    Value,
    Macro,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Binding {
    pub definition: (ModuleId, String, Namespace),
    pub module: Option<ModuleId>,
    /// The module whose descendants can access the binding; None means public.
    pub restricted_to: Option<ModuleId>,
    pub conditional: bool,
}

#[derive(Debug)]
pub(super) struct SourceInstance {
    pub path: PathBuf,
    pub module: ModuleId,
    pub conditions: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct ModuleRange {
    pub path: PathBuf,
    pub module: ModuleId,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Default)]
struct Module {
    parent: Option<ModuleId>,
    direct: BTreeMap<(String, Namespace), BTreeSet<Binding>>,
    bindings: BTreeMap<(String, Namespace), BTreeSet<Binding>>,
}

#[derive(Clone, Copy)]
struct ItemContext<'a> {
    file: &'a Path,
    directory: &'a Path,
    owner: ModuleId,
    inherited: &'a [String],
    profile: Option<&'a AnalysisProfile>,
}

struct Import {
    owner: ModuleId,
    path: Vec<String>,
    alias: Option<String>,
    glob: bool,
    absolute: bool,
    visibility: Option<ModuleId>,
    conditional: bool,
}

#[derive(Default)]
pub(super) struct ModuleGraph {
    modules: Vec<Module>,
    imports: Vec<Import>,
    pub sources: Vec<SourceInstance>,
    pub ranges: Vec<ModuleRange>,
    pub diagnostics: BTreeSet<String>,
    parsed: BTreeMap<PathBuf, Rc<syn::File>>,
    active_files: BTreeSet<PathBuf>,
    opaque: BTreeSet<ModuleId>,
    glob_cache: std::cell::RefCell<BTreeMap<(ModuleId, Vec<String>), bool>>,
    work: usize,
    conditions: super::conditions::ConditionQueries,
}

fn name(ident: &syn::Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_owned()
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for part in path.components() {
        if part == Component::ParentDir {
            result.pop();
        } else if part != Component::CurDir {
            result.push(part.as_os_str());
        }
    }
    result
}

fn attributes(
    attrs: &[syn::Attribute],
    conditions: &mut Vec<String>,
    path: &mut Option<String>,
    profile: Option<&AnalysisProfile>,
) {
    fn visit_meta(
        meta: &syn::Meta,
        conditions: &mut Vec<String>,
        path: &mut Option<String>,
        profile: Option<&AnalysisProfile>,
    ) {
        match meta {
            syn::Meta::List(list) if list.path.is_ident("cfg") => {
                conditions.push(list.tokens.to_string());
            }
            syn::Meta::NameValue(value) if value.path.is_ident("path") => {
                if let syn::Expr::Lit(value) = &value.value
                    && let syn::Lit::Str(value) = &value.lit
                {
                    *path = Some(value.value());
                }
            }
            syn::Meta::List(list) if list.path.is_ident("cfg_attr") => {
                use syn::parse::Parser;
                if let Ok(parts) =
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                        .parse2(list.tokens.clone())
                    && let Some(predicate) = parts.first()
                {
                    let predicate = predicate.to_token_stream().to_string();
                    let value = profile.map_or(ConditionValue::Unknown, |profile| {
                        evaluate_condition(&predicate, profile)
                    });
                    for nested in parts.iter().skip(1) {
                        if value == ConditionValue::True {
                            visit_meta(nested, conditions, path, profile);
                        } else if value == ConditionValue::Unknown {
                            let mut nested_conditions = Vec::new();
                            visit_meta(nested, &mut nested_conditions, &mut None, profile);
                            conditions.extend(
                                nested_conditions
                                    .into_iter()
                                    .map(|condition| format!("any(not({predicate}), {condition})")),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for attr in attrs {
        visit_meta(&attr.meta, conditions, path, profile);
    }
}

fn item_definition(item: &syn::Item) -> Option<(String, Vec<Namespace>)> {
    let (name, namespace) = match item {
        syn::Item::Struct(item) => {
            return Some((
                name(&item.ident),
                if matches!(item.fields, syn::Fields::Named(_)) {
                    vec![Namespace::Type]
                } else {
                    vec![Namespace::Type, Namespace::Value]
                },
            ));
        }
        syn::Item::Enum(item) => (&item.ident, Namespace::Type),
        syn::Item::Union(item) => (&item.ident, Namespace::Type),
        syn::Item::Trait(item) => (&item.ident, Namespace::Type),
        syn::Item::Type(item) => (&item.ident, Namespace::Type),
        syn::Item::Fn(item) => (&item.sig.ident, Namespace::Value),
        syn::Item::Const(item) => (&item.ident, Namespace::Value),
        syn::Item::Static(item) => (&item.ident, Namespace::Value),
        _ => return None,
    };
    Some((self::name(name), vec![namespace]))
}

impl ModuleGraph {
    pub(super) fn build(
        root: &Path,
        profile: Option<&AnalysisProfile>,
        mut load: impl FnMut(&Path) -> Result<Option<String>, BoxError>,
    ) -> Result<Self, BoxError> {
        let mut graph = Self::default();
        graph.modules.push(Module::default());
        let root = normalize(root);
        let directory = root.parent().ok_or("source root has no parent")?.to_owned();
        graph.file(&root, &directory, 0, &[], profile, &mut load)?;
        graph.resolve_imports();
        Ok(graph)
    }

    fn descendant(&self, mut module: ModuleId, ancestor: ModuleId) -> bool {
        loop {
            if module == ancestor {
                return true;
            }
            let Some(parent) = self.modules[module].parent else {
                return false;
            };
            module = parent;
        }
    }

    fn visible(&self, binding: &Binding, from: ModuleId) -> bool {
        binding
            .restricted_to
            .is_none_or(|owner| self.descendant(from, owner))
    }

    pub(super) fn parsed(&self, path: &Path) -> Option<&syn::File> {
        self.parsed.get(path).map(AsRef::as_ref)
    }

    /// Resolve only closed local export graphs. External imports, conditional
    /// exports and unexpanded item macros keep the result explicitly unknown.
    pub(super) fn glob_resolved(&self, owner: ModuleId, path: &[String]) -> bool {
        if let Some(result) = self.glob_cache.borrow().get(&(owner, path.to_vec())) {
            return *result;
        }
        let result = self.glob_closed(owner, path);
        self.glob_cache
            .borrow_mut()
            .insert((owner, path.to_vec()), result);
        result
    }

    fn glob_closed(&self, owner: ModuleId, path: &[String]) -> bool {
        let Some(source) = self.module_path(owner, path) else {
            return false;
        };
        let mut pending = vec![source];
        let mut visited = BTreeSet::new();
        while let Some(module) = pending.pop() {
            if !visited.insert(module) {
                continue;
            }
            if self.opaque.contains(&module)
                || self.modules[module]
                    .direct
                    .values()
                    .chain(self.modules[module].bindings.values())
                    .any(|bindings| {
                        bindings.iter().any(|binding| binding.conditional)
                            || bindings
                                .iter()
                                .map(|binding| &binding.definition)
                                .collect::<BTreeSet<_>>()
                                .len()
                                > 1
                    })
            {
                return false;
            }
            for import in self.imports.iter().filter(|import| import.owner == module) {
                if import.conditional || import.absolute {
                    return false;
                }
                let prefix = if import.glob {
                    import.path.as_slice()
                } else {
                    &import.path[..import.path.len().saturating_sub(1)]
                };
                let Some(target) = self.module_path(module, prefix) else {
                    return false;
                };
                if import.glob {
                    pending.push(target);
                } else if let Some(name) = import.path.last()
                    && [Namespace::Type, Namespace::Value, Namespace::Macro]
                        .into_iter()
                        .all(|namespace| self.lookup(target, name, namespace).is_empty())
                {
                    return false;
                }
            }
        }
        true
    }

    pub(super) fn imported(
        &self,
        owner: ModuleId,
        path: &[String],
        name: &str,
        namespace: Namespace,
    ) -> BTreeSet<Binding> {
        self.module_path(owner, path)
            .map_or_else(BTreeSet::new, |source| {
                self.lookup(source, name, namespace)
                    .into_iter()
                    .filter(|binding| self.visible(binding, owner))
                    .collect()
            })
    }

    pub(super) fn child_module(&self, owner: ModuleId, path: &[String]) -> Option<ModuleId> {
        self.module_path(owner, path)
    }

    pub(super) fn module_at(&self, path: &Path, line: usize) -> Option<ModuleId> {
        self.ranges
            .iter()
            .rev()
            .find(|range| range.path == path && range.start_line <= line && line <= range.end_line)
            .map(|range| range.module)
            .or_else(|| {
                self.sources
                    .iter()
                    .find(|source| source.path == path)
                    .map(|source| source.module)
            })
    }

    pub(super) fn lookup(
        &self,
        module: ModuleId,
        name: &str,
        namespace: Namespace,
    ) -> BTreeSet<Binding> {
        let key = (name.to_owned(), namespace);
        // Explicit bindings take priority over glob imports in each namespace.
        let candidates = self.modules[module]
            .direct
            .get(&key)
            .or_else(|| self.modules[module].bindings.get(&key));
        candidates.cloned().unwrap_or_default()
    }

    fn visibility(&self, owner: ModuleId, visibility: &syn::Visibility) -> Option<ModuleId> {
        match visibility {
            syn::Visibility::Public(_) => None,
            syn::Visibility::Restricted(restriction) => {
                if restriction.path.is_ident("crate") {
                    return Some(0);
                }
                if restriction.path.is_ident("self") {
                    return Some(owner);
                }
                if restriction.path.is_ident("super") {
                    return Some(self.modules[owner].parent.unwrap_or(owner));
                }
                let parts = restriction
                    .path
                    .segments
                    .iter()
                    .map(|s| name(&s.ident))
                    .collect::<Vec<_>>();
                self.module_path(owner, &parts).or(Some(owner))
            }
            syn::Visibility::Inherited => Some(owner),
        }
    }

    fn module_path(&self, owner: ModuleId, path: &[String]) -> Option<ModuleId> {
        let mut current = owner;
        for (index, name) in path.iter().enumerate() {
            match name.as_str() {
                "crate" if index == 0 => current = 0,
                "self" if index == 0 => {}
                "super" => current = self.modules[current].parent?,
                _ => {
                    let bindings = self.lookup(current, name, Namespace::Type);
                    let mut targets = bindings
                        .iter()
                        .filter(|b| self.visible(b, owner) && !b.conditional)
                        .filter_map(|b| b.module);
                    current = targets.next()?;
                    if targets.any(|other| other != current) {
                        return None;
                    }
                }
            }
        }
        Some(current)
    }

    fn insert(
        &mut self,
        owner: ModuleId,
        name: String,
        namespace: Namespace,
        visibility: Option<ModuleId>,
        conditional: bool,
        target: Option<ModuleId>,
    ) {
        self.modules[owner]
            .direct
            .entry((name.clone(), namespace))
            .or_default()
            .insert(Binding {
                definition: (owner, name, namespace),
                module: target,
                restricted_to: visibility,
                conditional,
            });
    }

    fn file(
        &mut self,
        path: &Path,
        directory: &Path,
        module: ModuleId,
        inherited: &[String],
        profile: Option<&AnalysisProfile>,
        load: &mut impl FnMut(&Path) -> Result<Option<String>, BoxError>,
    ) -> Result<(), BoxError> {
        self.work += 1;
        if self.work > 4096 || self.modules.len() > 4096 {
            self.diagnostics
                .insert("source-instance work budget exhausted".into());
            return Ok(());
        }
        let path = normalize(path);
        if !self.active_files.insert(path.clone()) {
            self.diagnostics
                .insert(format!("recursive module source: {}", path.display()));
            return Ok(());
        }
        let parsed = if let Some(parsed) = self.parsed.get(&path) {
            Some(Rc::clone(parsed))
        } else if let Some(text) = load(&path)? {
            match syn::parse_file(&text) {
                Ok(file) => {
                    let parsed = Rc::new(file);
                    self.parsed.insert(path.clone(), Rc::clone(&parsed));
                    Some(parsed)
                }
                Err(error) => {
                    self.diagnostics
                        .insert(format!("cannot parse source {}: {error}", path.display()));
                    None
                }
            }
        } else {
            self.diagnostics
                .insert(format!("missing source: {}", path.display()));
            None
        };
        if let Some(parsed) = parsed {
            let mut conditions = inherited.to_vec();
            attributes(&parsed.attrs, &mut conditions, &mut None, profile);
            if self.satisfiable(&conditions)
                && !conditions.iter().any(|c| {
                    profile.is_some_and(|p| evaluate_condition(c, p) == ConditionValue::False)
                })
            {
                self.block_module_diagnostics(&parsed, &path);
                self.sources.push(SourceInstance {
                    path: path.clone(),
                    module,
                    conditions: conditions.clone(),
                });
                self.items(
                    &parsed.items,
                    ItemContext {
                        file: &path,
                        directory,
                        owner: module,
                        inherited: &conditions,
                        profile,
                    },
                    load,
                )?;
            }
        }
        self.active_files.remove(&path);
        Ok(())
    }

    fn satisfiable(&mut self, conditions: &[String]) -> bool {
        match self.conditions.satisfiability(conditions) {
            super::conditions::Satisfiability::Unsat => false,
            super::conditions::Satisfiability::Exhausted => {
                self.diagnostics
                    .insert("condition satisfiability budget exhausted".into());
                true
            }
            super::conditions::Satisfiability::Sat => true,
        }
    }

    fn block_module_diagnostics(&mut self, parsed: &syn::File, file: &Path) {
        #[derive(Default)]
        struct Modules {
            lines: Vec<usize>,
            blocks: usize,
        }
        impl<'ast> syn::visit::Visit<'ast> for Modules {
            fn visit_block(&mut self, block: &'ast syn::Block) {
                self.blocks += 1;
                syn::visit::visit_block(self, block);
                self.blocks -= 1;
            }
            fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
                if self.blocks > 0 && module.content.is_none() {
                    self.lines.push(module.span().start().line);
                } else {
                    syn::visit::visit_item_mod(self, module);
                }
            }
        }
        use syn::visit::Visit;
        let mut modules = Modules::default();
        modules.visit_file(parsed);
        for line in modules.lines {
            self.diagnostics.insert(format!(
                "block-local module requires lexical module context: {}:{line}",
                file.display()
            ));
        }
    }

    fn module_item(
        &mut self,
        item: &syn::ItemMod,
        context: ItemContext<'_>,
        policy: (Option<ModuleId>, bool, Option<String>),
        load: &mut impl FnMut(&Path) -> Result<Option<String>, BoxError>,
    ) -> Result<(), BoxError> {
        let ItemContext {
            file,
            directory,
            owner,
            inherited: conditions,
            profile,
        } = context;
        let (visibility, conditional, explicit_path) = policy;
        let name = name(&item.ident);
        let child = self.modules.len();
        self.modules.push(Module {
            parent: Some(owner),
            ..Module::default()
        });
        self.insert(
            owner,
            name.clone(),
            Namespace::Type,
            visibility,
            conditional,
            Some(child),
        );
        if let Some((brace, items)) = &item.content {
            self.ranges.push(ModuleRange {
                path: file.to_owned(),
                module: child,
                start_line: brace.span.open().start().line,
                end_line: brace.span.close().end().line,
            });
            self.items(
                items,
                ItemContext {
                    file,
                    directory: &directory.join(&name),
                    owner: child,
                    inherited: conditions,
                    profile,
                },
                load,
            )?;
        } else {
            let candidate = if let Some(path) = explicit_path {
                let path_base = if self
                    .sources
                    .iter()
                    .any(|source| source.path == file && source.module == owner)
                {
                    file.parent().unwrap_or(directory)
                } else {
                    directory
                };
                normalize(&path_base.join(path))
            } else {
                let direct = directory.join(format!("{name}.rs"));
                let nested = directory.join(&name).join("mod.rs");
                let first = load(&direct)?.is_some();
                let second = load(&nested)?.is_some();
                if first && second {
                    self.diagnostics.insert(format!(
                        "ambiguous module source: {} and {}",
                        direct.display(),
                        nested.display()
                    ));
                }
                if first { direct } else { nested }
            };
            let base = if candidate.file_name().is_some_and(|n| n == "mod.rs") {
                candidate.parent().unwrap_or(directory).to_owned()
            } else {
                candidate
                    .parent()
                    .unwrap_or(directory)
                    .join(candidate.file_stem().unwrap_or_default())
            };
            self.file(&candidate, &base, child, conditions, profile, load)?;
        }
        Ok(())
    }

    fn include_item(
        &mut self,
        item: &syn::ItemMacro,
        context: ItemContext<'_>,
        load: &mut impl FnMut(&Path) -> Result<Option<String>, BoxError>,
    ) -> Result<(), BoxError> {
        let ItemContext {
            file,
            directory,
            owner,
            inherited,
            profile,
        } = context;
        self.opaque.insert(owner);
        if item.mac.path.is_ident("include") {
            match syn::parse2::<syn::LitStr>(item.mac.tokens.clone()) {
                Ok(literal) => {
                    let included =
                        normalize(&file.parent().unwrap_or(directory).join(literal.value()));
                    self.file(&included, directory, owner, inherited, profile, load)?;
                }
                Err(_) => {
                    self.diagnostics.insert(format!(
                        "advisory: dynamic include requires expansion; unreferenced candidates remain protected: {}:{}",
                        file.display(),
                        item.span().start().line
                    ));
                }
            }
        }
        Ok(())
    }

    fn items(
        &mut self,
        items: &[syn::Item],
        context: ItemContext<'_>,
        load: &mut impl FnMut(&Path) -> Result<Option<String>, BoxError>,
    ) -> Result<(), BoxError> {
        let ItemContext {
            file,
            directory: _,
            owner,
            inherited,
            profile,
        } = context;
        for item in items {
            if matches!(item, syn::Item::Verbatim(_)) {
                self.opaque.insert(owner);
            }
            if let syn::Item::Macro(item) = item {
                self.include_item(item, context, load)?;
                continue;
            }
            let (attrs, visibility) = match item {
                syn::Item::Mod(i) => (&i.attrs, &i.vis),
                syn::Item::Use(i) => (&i.attrs, &i.vis),
                syn::Item::Struct(i) => (&i.attrs, &i.vis),
                syn::Item::Enum(i) => (&i.attrs, &i.vis),
                syn::Item::Union(i) => (&i.attrs, &i.vis),
                syn::Item::Trait(i) => (&i.attrs, &i.vis),
                syn::Item::Type(i) => (&i.attrs, &i.vis),
                syn::Item::Fn(i) => (&i.attrs, &i.vis),
                syn::Item::Const(i) => (&i.attrs, &i.vis),
                syn::Item::Static(i) => (&i.attrs, &i.vis),
                _ => continue,
            };
            if attrs.iter().any(|attr| {
                attr.path().is_ident("cfg_attr")
                    && attr.meta.to_token_stream().to_string().contains("path")
            }) {
                self.diagnostics.insert(format!(
                    "conditional module path requires configuration-specific expansion: {}:{}",
                    file.display(),
                    item.span().start().line
                ));
                self.opaque.insert(owner);
            }
            let mut conditions = inherited.to_vec();
            let mut explicit_path = None;
            attributes(attrs, &mut conditions, &mut explicit_path, profile);
            if !self.satisfiable(&conditions)
                || conditions.iter().any(|c| {
                    profile.is_some_and(|p| evaluate_condition(c, p) == ConditionValue::False)
                })
            {
                continue;
            }
            let conditional = conditions.iter().any(|c| {
                profile.is_none_or(|p| evaluate_condition(c, p) == ConditionValue::Unknown)
            });
            let visibility = self.visibility(owner, visibility);
            match item {
                syn::Item::Mod(item) => self.module_item(
                    item,
                    ItemContext {
                        inherited: &conditions,
                        ..context
                    },
                    (visibility, conditional, explicit_path),
                    load,
                )?,
                syn::Item::Use(item) => {
                    self.use_tree(
                        owner,
                        &item.tree,
                        &[],
                        (visibility, conditional, item.leading_colon.is_some()),
                    );
                }
                _ => {
                    if let Some((name, namespaces)) = item_definition(item) {
                        for namespace in namespaces {
                            self.insert(
                                owner,
                                name.clone(),
                                namespace,
                                visibility,
                                conditional,
                                None,
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn use_tree(
        &mut self,
        owner: ModuleId,
        tree: &syn::UseTree,
        prefix: &[String],
        policy: (Option<ModuleId>, bool, bool),
    ) {
        let (visibility, conditional, absolute) = policy;
        let mut path = prefix.to_vec();
        match tree {
            syn::UseTree::Path(p) => {
                path.push(name(&p.ident));
                self.use_tree(owner, &p.tree, &path, policy);
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.use_tree(owner, item, &path, policy);
                }
            }
            syn::UseTree::Glob(_) => self.imports.push(Import {
                owner,
                path,
                alias: None,
                glob: true,
                absolute,
                visibility,
                conditional,
            }),
            syn::UseTree::Name(name) => {
                let alias = if name.ident == "self" {
                    path.last().cloned()
                } else {
                    path.push(self::name(&name.ident));
                    Some(self::name(&name.ident))
                };
                self.imports.push(Import {
                    owner,
                    path,
                    alias,
                    glob: false,
                    absolute,
                    visibility,
                    conditional,
                });
            }
            syn::UseTree::Rename(rename) => {
                if rename.ident != "self" {
                    path.push(name(&rename.ident));
                }
                self.imports.push(Import {
                    owner,
                    path,
                    alias: Some(name(&rename.rename)),
                    glob: false,
                    absolute,
                    visibility,
                    conditional,
                });
            }
        }
    }

    fn path_dependencies(&self, owner: ModuleId, path: &[String]) -> BTreeSet<ModuleId> {
        let mut result = BTreeSet::from([owner]);
        for end in 1..=path.len() {
            if let Some(module) = self.module_path(owner, &path[..end]) {
                result.insert(module);
            } else {
                break;
            }
        }
        result
    }

    fn resolve_imports(&mut self) {
        // Re-enqueue only after new finite facts appear. Definition identities are
        // stable across reexports, so cycles cannot create unbounded provenance.
        let mut queue = (0..self.imports.len()).collect::<VecDeque<_>>();
        let mut queued = (0..self.imports.len()).collect::<BTreeSet<_>>();
        let mut watchers = BTreeMap::<ModuleId, BTreeSet<usize>>::new();
        while let Some(index) = queue.pop_front() {
            queued.remove(&index);
            let import = &self.imports[index];
            if import.absolute {
                continue;
            }
            for module in self.path_dependencies(import.owner, &import.path) {
                watchers.entry(module).or_default().insert(index);
            }
            let (prefix, terminal) = if import.glob {
                (import.path.as_slice(), None)
            } else {
                let Some((last, prefix)) = import.path.split_last() else {
                    continue;
                };
                (prefix, Some(last))
            };
            let Some(source) = self.module_path(import.owner, prefix) else {
                continue;
            };
            let mut additions = Vec::new();
            for namespace in [Namespace::Type, Namespace::Value, Namespace::Macro] {
                let names = if let Some(name) = terminal {
                    vec![name.clone()]
                } else {
                    self.modules[source]
                        .direct
                        .keys()
                        .chain(self.modules[source].bindings.keys())
                        .filter(|(_, ns)| *ns == namespace)
                        .map(|(n, _)| n.clone())
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect()
                };
                for name in names {
                    for mut binding in self.lookup(source, &name, namespace) {
                        if !self.visible(&binding, import.owner) {
                            continue;
                        }
                        // Do not widen a restricted definition through a reexport.
                        binding.restricted_to = match (binding.restricted_to, import.visibility) {
                            (None, scope) | (scope, None) => scope,
                            (Some(a), Some(b)) if self.descendant(a, b) => Some(a),
                            (Some(_), Some(b)) => Some(b),
                        };
                        binding.conditional |= import.conditional;
                        let name = import.alias.clone().unwrap_or_else(|| name.clone());
                        if name != "_" {
                            additions.push(((name, namespace), binding));
                        }
                    }
                }
            }
            let owner = import.owner;
            let explicit = !import.glob;
            let mut changed = false;
            for (key, binding) in additions {
                let map = if explicit {
                    &mut self.modules[owner].direct
                } else {
                    &mut self.modules[owner].bindings
                };
                changed |= map.entry(key).or_default().insert(binding);
            }
            if changed {
                for &next in watchers.get(&owner).into_iter().flatten() {
                    if queued.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph(files: &[(&str, &str)]) -> ModuleGraph {
        let files = files
            .iter()
            .map(|(p, s)| (PathBuf::from(p), (*s).to_owned()))
            .collect::<BTreeMap<_, _>>();
        ModuleGraph::build(Path::new("/src/lib.rs"), None, |p| {
            Ok(files.get(p).cloned())
        })
        .unwrap()
    }
    #[test]
    fn cross_file_cycles_preserve_identity_and_visibility() {
        let graph = graph(&[
            ("/src/lib.rs", "mod a; mod b; use a::*;"),
            (
                "/src/a.rs",
                "pub use crate::b::*; pub struct Public; struct Private;",
            ),
            ("/src/b.rs", "pub use crate::a::*;"),
        ]);
        assert_eq!(graph.lookup(0, "Public", Namespace::Type).len(), 1);
        assert!(graph.lookup(0, "Private", Namespace::Type).is_empty());
        assert_eq!(graph.sources.len(), 3);
    }
    #[test]
    fn one_snapshot_can_have_multiple_module_instances() {
        let graph = graph(&[
            (
                "/src/lib.rs",
                "#[path=\"shared.rs\"] mod a; #[path=\"shared.rs\"] mod b;",
            ),
            ("/src/shared.rs", "pub struct Item;"),
        ]);
        assert_eq!(graph.parsed.len(), 2);
        assert_eq!(graph.sources.len(), 3);
        assert_ne!(graph.sources[1].module, graph.sources[2].module);
    }
    #[test]
    fn namespace_and_explicit_precedence_are_separate() {
        let graph = graph(&[(
            "/src/lib.rs",
            "mod a { pub fn Name() {} pub struct Other; } use a::*; struct Name {}",
        )]);
        assert_eq!(graph.lookup(0, "Name", Namespace::Type).len(), 1);
        assert_eq!(graph.lookup(0, "Name", Namespace::Value).len(), 1);
    }
    #[test]
    fn inline_module_directory_and_explicit_paths_keep_distinct_bases() {
        let graph = graph(&[
            (
                "/src/lib.rs",
                "mod outer { mod inner { mod leaf; } } mod ordinary;",
            ),
            ("/src/outer/inner/leaf.rs", "pub struct Leaf;"),
            ("/src/ordinary.rs", "#[path=\"sibling.rs\"] mod sibling;"),
            ("/src/sibling.rs", "pub struct Sibling;"),
        ]);
        assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
        assert!(
            graph
                .sources
                .iter()
                .any(|source| source.path == Path::new("/src/sibling.rs"))
        );
        assert_eq!(graph.sources.len(), 4);
    }

    #[test]
    fn explicit_binding_wins_and_distinct_globs_remain_ambiguous() {
        let graph = graph(&[(
            "/src/lib.rs",
            "mod a { pub struct Same; } mod b { pub struct Same; } use a::*; use b::*; mod explicit { use super::a::*; use super::b::Same; }",
        )]);
        assert_eq!(graph.lookup(0, "Same", Namespace::Type).len(), 2);
        let explicit = graph.child_module(0, &["explicit".into()]).unwrap();
        assert_eq!(graph.lookup(explicit, "Same", Namespace::Type).len(), 1);
        assert!(!graph.glob_resolved(explicit, &["super".into()]));
    }

    #[test]
    fn sibling_visibility_and_parent_test_privacy() {
        let graph = graph(&[(
            "/src/lib.rs",
            "mod a { pub(super) struct Parent; struct Secret; mod tests { use super::*; } } mod b { use super::a::*; }",
        )]);
        let a = graph.child_module(0, &["a".into()]).unwrap();
        let b = graph.child_module(0, &["b".into()]).unwrap();
        let tests = graph.child_module(a, &["tests".into()]).unwrap();
        assert_eq!(graph.lookup(b, "Parent", Namespace::Type).len(), 1);
        assert!(graph.lookup(b, "Secret", Namespace::Type).is_empty());
        assert_eq!(graph.lookup(tests, "Secret", Namespace::Type).len(), 1);
    }

    #[test]
    fn unknown_external_exports_and_macros_do_not_become_closed_globs() {
        let graph = graph(&[(
            "/src/lib.rs",
            "mod external { pub use dependency::*; } mod generated { emit!(); } use external::*; use generated::*;",
        )]);
        assert!(!graph.glob_resolved(0, &["external".into()]));
        assert!(!graph.glob_resolved(0, &["generated".into()]));
    }

    #[test]
    fn include_reuses_module_context_and_snapshot() {
        let graph = graph(&[
            ("/src/lib.rs", "mod a { include!(\"included.rs\"); }"),
            ("/src/included.rs", "pub struct Included;"),
        ]);
        let a = graph.child_module(0, &["a".into()]).unwrap();
        assert_eq!(graph.lookup(a, "Included", Namespace::Type).len(), 1);
        assert_eq!(graph.sources[1].module, a);
    }

    #[test]
    fn unsatisfiable_parent_never_loads_child() {
        let files = BTreeMap::from([(
            PathBuf::from("/src/lib.rs"),
            "#[cfg(all(test,not(test)))] mod absent;".to_owned(),
        )]);
        let graph = ModuleGraph::build(Path::new("/src/lib.rs"), None, |path| {
            Ok(files.get(path).cloned())
        })
        .unwrap();
        assert_eq!(graph.sources.len(), 1);
        assert!(graph.diagnostics.is_empty());
    }

    #[test]
    fn absolute_external_globs_never_capture_local_modules() {
        let graph = graph(&[("/src/lib.rs", "mod dep { pub struct Local; } use ::dep::*;")]);
        assert!(graph.lookup(0, "Local", Namespace::Type).is_empty());
        assert!(graph.imports[0].absolute);
    }

    #[test]
    fn block_local_modules_and_unknown_paths_are_protected() {
        let graph = graph(&[
            (
                "/src/lib.rs",
                "fn run(){ #[path=\"hidden.rs\"] mod hidden; } #[cfg_attr(custom,path=\"alternate.rs\")] mod next;",
            ),
            ("/src/next.rs", ""),
        ]);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("block-local module"))
        );
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("conditional module path"))
        );
    }

    #[test]
    fn repeated_source_instantiation_has_a_finite_budget() {
        let mut files = BTreeMap::new();
        for depth in 0..15 {
            let text = if depth == 14 {
                String::new()
            } else {
                format!(
                    "#[path=\"{}.rs\"] mod a; #[path=\"{}.rs\"] mod b;",
                    depth + 1,
                    depth + 1
                )
            };
            files.insert(PathBuf::from(format!("/src/{depth}.rs")), text);
        }
        let graph = ModuleGraph::build(Path::new("/src/0.rs"), None, |path| {
            Ok(files.get(path).cloned())
        })
        .unwrap();
        assert!(graph.sources.len() <= 4096);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("work budget exhausted"))
        );
    }

    #[test]
    fn external_modules_in_every_item_body_are_protected() {
        for source in [
            "struct Runner; impl Runner { fn run() { #[path=\"hidden.rs\"] mod hidden; hidden::run(); } }",
            "trait Runner { fn run() { #[path=\"hidden.rs\"] mod hidden; hidden::run(); } }",
            "const VALUE: () = { #[path=\"hidden.rs\"] mod hidden; };",
            "static VALUE: () = { #[path=\"hidden.rs\"] mod hidden; };",
        ] {
            let graph = graph(&[("/src/lib.rs", source)]);
            assert!(
                graph
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.contains("block-local module")),
                "{source}"
            );
        }
    }

    #[test]
    fn raw_identifier_module_and_import_bindings_share_normalized_names() {
        let graph = graph(&[(
            "/src/lib.rs",
            "mod r#type { pub struct r#Union; } use r#type::*;",
        )]);
        assert_eq!(graph.lookup(0, "Union", Namespace::Type).len(), 1);
        assert!(graph.child_module(0, &["type".into()]).is_some());
    }

    #[test]
    fn inline_block_modules_are_not_external_module_gaps() {
        let graph = graph(&[(
            "/src/lib.rs",
            "fn run() { mod local { pub fn run() {} } local::run(); }",
        )]);
        assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
    }

    #[test]
    fn dynamic_include_keeps_advisory_candidate_protection() {
        let graph = graph(&[(
            "/src/lib.rs",
            "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));",
        )]);
        assert!(
            graph
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.starts_with("advisory:"))
        );
        assert!(!graph.diagnostics.is_empty());
    }

    #[test]
    fn inherited_cfg_is_attached_to_source_instances() {
        let graph = graph(&[
            ("/src/lib.rs", "#[cfg(any(test, feature=\"x\"))] mod child;"),
            ("/src/child.rs", "pub fn f() {}"),
        ]);
        assert_eq!(graph.sources[1].conditions.len(), 1);
    }
}
