//! Syntax-backed lexical resolution. Cross-file imports remain explicit index edges.
use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::Span;
use syn::{
    ext::IdentExt,
    spanned::Spanned,
    visit::{self, Visit},
};

use super::super::EvidenceKind;
use super::Observation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Namespace {
    Type,
    Value,
    Macro,
    Lifetime,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Item,
    Generic,
    Local,
    Import,
    Macro,
}
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub namespace: Namespace,
    pub kind: BindingKind,
    pub import: Option<usize>,
}
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub parent: Option<usize>,
    pub module: Vec<String>,
    /// Named items cannot capture outer locals or generic parameters.
    pub item_boundary: bool,
    pub bindings: BTreeMap<(String, Namespace), Vec<Binding>>,
}
#[derive(Debug, Clone)]
pub struct Import {
    pub scope: usize,
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub absolute: bool,
    pub glob: bool,
    pub public: bool,
    pub line: usize,
    pub column: usize,
    pub conditions: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Local,
    Import(usize),
    External,
    Qualified,
}
#[derive(Debug, Clone)]
pub struct Reference {
    pub scope: usize,
    pub path: Vec<String>,
    pub namespace: Namespace,
    pub absolute: bool,
    pub line: usize,
    pub column: usize,
    pub resolution: Resolution,
}
#[derive(Debug, Clone, Default)]
pub struct SyntaxIndex {
    pub scopes: Vec<Scope>,
    pub imports: Vec<Import>,
    pub references: Vec<Reference>,
}

// proc_macro2 columns count Unicode scalar values, not UTF-8 bytes. Build this
// conversion once, rather than rescanning every source prefix for every token.
struct Locations {
    lines: Vec<Vec<usize>>,
}
impl Locations {
    fn new(text: &str) -> Self {
        let mut lines = vec![vec![0]];
        for (offset, ch) in text.char_indices() {
            if ch == '\n' {
                lines.push(vec![offset + 1]);
            } else {
                lines.last_mut().unwrap().push(offset + ch.len_utf8());
            }
        }
        Self { lines }
    }
    fn offset(&self, span: Span) -> usize {
        let start = span.start();
        self.lines
            .get(start.line.saturating_sub(1))
            .and_then(|line| line.get(start.column))
            .copied()
            .unwrap_or(0)
    }
}

pub(super) fn analyze(
    text: &str,
    positions: &[usize],
    conditions: &[Vec<String>],
) -> Result<(Vec<Observation>, BTreeSet<String>, SyntaxIndex), syn::Error> {
    let file = syn::parse_file(text)?;
    Ok(analyze_parsed(text, &file, positions, conditions))
}

pub(super) fn analyze_parsed(
    text: &str,
    file: &syn::File,
    positions: &[usize],
    conditions: &[Vec<String>],
) -> (Vec<Observation>, BTreeSet<String>, SyntaxIndex) {
    let mut resolver = Resolver {
        locations: Locations::new(text),
        positions,
        conditions,
        index: SyntaxIndex {
            scopes: vec![Scope::default()],
            ..SyntaxIndex::default()
        },
        scope: 0,
        observations: Vec::new(),
        gaps: BTreeSet::new(),
    };
    resolver.items(&file.items);
    for attr in &file.attrs {
        resolver.visit_attribute(attr);
    }
    for item in &file.items {
        resolver.visit_item(item);
    }
    (resolver.observations, resolver.gaps, resolver.index)
}

struct Resolver<'a> {
    locations: Locations,
    positions: &'a [usize],
    conditions: &'a [Vec<String>],
    index: SyntaxIndex,
    scope: usize,
    observations: Vec<Observation>,
    gaps: BTreeSet<String>,
}
// syn preserves branch-local absolute use groups as Verbatim because UseTree
// cannot represent their leading colons. Parse only that Rust grammar here;
// never treat arbitrary verbatim tokens as resolved syntax.
/// Expand a verbatim grouped use into representable import branches.
///
/// # Errors
/// * Returns a syntax error when the tokens are not a supported Rust use item.
pub fn verbatim_imports(tokens: proc_macro2::TokenStream) -> syn::Result<Vec<syn::ItemUse>> {
    use syn::parse::{ParseStream, Parser};
    fn tree(
        input: ParseStream,
        prefix: &[syn::Ident],
        absolute: bool,
        out: &mut Vec<(syn::UseTree, bool)>,
    ) -> syn::Result<()> {
        let leading: Option<syn::Token![::]> = input.parse()?;
        if leading.is_some() && !prefix.is_empty() {
            return Err(input.error("absolute import cannot follow a path prefix"));
        }
        let absolute = absolute || leading.is_some();
        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);
            while !content.is_empty() {
                tree(&content, prefix, absolute, out)?;
                if content.is_empty() {
                    break;
                }
                content.parse::<syn::Token![,]>()?;
            }
            return Ok(());
        }
        let mut branch = if input.peek(syn::Token![*]) {
            syn::UseTree::Glob(syn::UseGlob {
                star_token: input.parse()?,
            })
        } else {
            let ident = input.call(syn::Ident::parse_any)?;
            if input.peek(syn::Token![::]) {
                input.parse::<syn::Token![::]>()?;
                let mut prefix = prefix.to_vec();
                prefix.push(ident);
                return tree(input, &prefix, absolute, out);
            }
            if input.peek(syn::Token![as]) {
                syn::UseTree::Rename(syn::UseRename {
                    ident,
                    as_token: input.parse()?,
                    rename: input.call(syn::Ident::parse_any)?,
                })
            } else {
                syn::UseTree::Name(syn::UseName { ident })
            }
        };
        for ident in prefix.iter().rev() {
            branch = syn::UseTree::Path(syn::UsePath {
                ident: ident.clone(),
                colon2_token: syn::Token![::](ident.span()),
                tree: Box::new(branch),
            });
        }
        out.push((branch, absolute));
        Ok(())
    }
    let parser = |input: ParseStream| {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let vis: syn::Visibility = input.parse()?;
        let use_token: syn::Token![use] = input.parse()?;
        let mut branches = Vec::new();
        tree(input, &[], false, &mut branches)?;
        let semi_token: syn::Token![;] = input.parse()?;
        Ok(branches
            .into_iter()
            .map(|(tree, absolute)| syn::ItemUse {
                attrs: attrs.clone(),
                vis: vis.clone(),
                use_token,
                leading_colon: absolute.then(|| syn::Token![::](use_token.span)),
                tree,
                semi_token,
            })
            .collect())
    };
    parser.parse2(tokens)
}

impl Resolver<'_> {
    fn observe_use(&mut self, item: &syn::ItemUse) {
        // Import roots are actual dependency uses, independent of aliases.
        fn roots<'a>(tree: &'a syn::UseTree, out: &mut Vec<&'a syn::Ident>) {
            match tree {
                syn::UseTree::Path(p) => out.push(&p.ident),
                syn::UseTree::Name(p) => out.push(&p.ident),
                syn::UseTree::Rename(p) => out.push(&p.ident),
                syn::UseTree::Group(g) => {
                    for t in &g.items {
                        roots(t, out);
                    }
                }
                syn::UseTree::Glob(_) => {}
            }
        }
        let mut names = Vec::new();
        roots(&item.tree, &mut names);
        for ident in names {
            let name = ident.unraw().to_string();
            if !matches!(name.as_str(), "crate" | "self" | "super" | "Self") {
                // An import of its own root must not hide that root.
                let external = item.leading_colon.is_some()
                    || !self.lookup(&name, Namespace::Type).is_some_and(|binding| {
                        binding
                            .import
                            .is_none_or(|id| self.index.imports[id].path.first() != Some(&name))
                    });
                if external {
                    self.observe(
                        name,
                        ident.span(),
                        if matches!(item.vis, syn::Visibility::Inherited) {
                            EvidenceKind::Import
                        } else {
                            EvidenceKind::Reexport
                        },
                    );
                }
            }
        }
        for attr in &item.attrs {
            self.visit_attribute(attr);
        }
    }
    fn enabled(&self, span: Span) -> bool {
        let offset = self.locations.offset(span);
        self.positions.binary_search(&offset).is_ok()
    }
    fn conditions(&self, span: Span) -> Vec<String> {
        let offset = self.locations.offset(span);
        self.positions
            .binary_search(&offset)
            .ok()
            .and_then(|i| self.conditions.get(i))
            .cloned()
            .unwrap_or_default()
    }
    fn push(&mut self, detached: bool) -> usize {
        let old = self.scope;
        self.index.scopes.push(Scope {
            parent: (!detached).then_some(old),
            module: self.index.scopes[old].module.clone(),
            ..Scope::default()
        });
        self.scope = self.index.scopes.len() - 1;
        old
    }
    fn bind(
        &mut self,
        name: String,
        namespace: Namespace,
        kind: BindingKind,
        import: Option<usize>,
    ) {
        self.index.scopes[self.scope]
            .bindings
            .entry((name.clone(), namespace))
            .or_default()
            .push(Binding {
                name,
                namespace,
                kind,
                import,
            });
    }
    fn lookup(&self, name: &str, ns: Namespace) -> Option<&Binding> {
        let mut scope = Some(self.scope);
        let mut crossed_item = false;
        while let Some(id) = scope {
            if let Some(binding) = self.index.scopes[id]
                .bindings
                .get(&(name.to_owned(), ns))
                .and_then(|b| b.last())
            {
                debug_assert_eq!(binding.name, name);
                debug_assert_eq!(binding.namespace, ns);
                if !crossed_item
                    || !matches!(binding.kind, BindingKind::Local | BindingKind::Generic)
                {
                    return Some(binding);
                }
            }
            crossed_item |= self.index.scopes[id].item_boundary;
            scope = self.index.scopes[id].parent;
        }
        None
    }
    fn observe(&mut self, name: String, span: Span, kind: EvidenceKind) {
        let start = span.start();
        self.observations.push(Observation {
            name,
            line: start.line,
            column: start.column + 1,
            kind,
            conditions: self.conditions(span),
            uncertainty: Vec::new(),
        });
    }
    fn path(&mut self, path: &syn::Path, ns: Namespace, kind: EvidenceKind) {
        let Some(first) = path.segments.first() else {
            return;
        };
        if !self.enabled(first.ident.span()) {
            return;
        }
        let name = first.ident.unraw().to_string();
        let absolute = path.leading_colon.is_some();
        let qualified = matches!(name.as_str(), "self" | "super" | "crate" | "Self");
        // Qualified paths start in the type namespace, even in expressions.
        let ns = if path.segments.len() > 1 {
            Namespace::Type
        } else {
            ns
        };
        let resolution = if qualified {
            Resolution::Qualified
        } else if absolute {
            Resolution::External
        } else if let Some(binding) = self.lookup(&name, ns) {
            binding.import.map_or(Resolution::Local, Resolution::Import)
        } else {
            Resolution::External
        };
        if let Resolution::Import(import) = resolution
            && self.index.imports[import].path.len() > 1
        {
            let target = &self.index.imports[import];
            self.gaps.insert(format!(
                "import target namespace requires module graph resolution: {} ({}{} at {}:{})",
                target.path.join("::"),
                if target.public { "reexport " } else { "" },
                target.alias.as_deref().unwrap_or("*"),
                target.line,
                target.column,
            ));
        }
        let start = first.ident.span().start();
        self.index.references.push(Reference {
            scope: self.scope,
            path: path
                .segments
                .iter()
                .map(|s| s.ident.unraw().to_string())
                .collect(),
            namespace: ns,
            absolute,
            line: start.line,
            column: start.column + 1,
            resolution: resolution.clone(),
        });
        if resolution == Resolution::External {
            // A bare value can name an external function only through an import;
            // crate names themselves occur as path prefixes, not bare values.
            if path.segments.len() > 1 || absolute || ns == Namespace::Macro {
                self.observe(name, first.ident.span(), kind);
            }
        }
        // Visit generic arguments, but never reinterpret path tails as roots.
        for segment in &path.segments {
            self.visit_path_arguments(&segment.arguments);
        }
    }
    fn generics(&mut self, generics: &syn::Generics) {
        for param in &generics.params {
            let (name, ns) = match param {
                syn::GenericParam::Type(p) => (p.ident.unraw().to_string(), Namespace::Type),
                syn::GenericParam::Const(p) => (p.ident.unraw().to_string(), Namespace::Value),
                syn::GenericParam::Lifetime(p) => {
                    (p.lifetime.ident.unraw().to_string(), Namespace::Lifetime)
                }
            };
            self.bind(name, ns, BindingKind::Generic, None);
        }
    }
    fn pattern(&mut self, pat: &syn::Pat) {
        struct Bindings(Vec<String>);
        impl<'ast> Visit<'ast> for Bindings {
            fn visit_pat_ident(&mut self, pat: &'ast syn::PatIdent) {
                self.0.push(pat.ident.unraw().to_string());
                visit::visit_pat_ident(self, pat);
            }
        }
        let mut bindings = Bindings(Vec::new());
        bindings.visit_pat(pat);
        for name in bindings.0 {
            self.bind(name, Namespace::Value, BindingKind::Local, None);
        }
    }
    fn items<'a>(&mut self, items: impl IntoIterator<Item = &'a syn::Item>) {
        let items: Vec<_> = items.into_iter().collect();
        for &item in &items {
            if !self.enabled(item.span()) {
                continue;
            }
            // Keep conditional import edges; only unconditional declarations can
            // establish a lexical shadow before configuration resolution.
            let conditional = !self.conditions(item.span()).is_empty();
            let binding = match item {
                syn::Item::Mod(i) => Some((&i.ident, Namespace::Type)),
                syn::Item::Struct(i) => Some((&i.ident, Namespace::Type)),
                syn::Item::Enum(i) => Some((&i.ident, Namespace::Type)),
                syn::Item::Union(i) => Some((&i.ident, Namespace::Type)),
                syn::Item::Trait(i) => Some((&i.ident, Namespace::Type)),
                syn::Item::TraitAlias(i) => Some((&i.ident, Namespace::Type)),
                syn::Item::Type(i) => Some((&i.ident, Namespace::Type)),
                syn::Item::Fn(i) => Some((&i.sig.ident, Namespace::Value)),
                syn::Item::Const(i) => Some((&i.ident, Namespace::Value)),
                syn::Item::Static(i) => Some((&i.ident, Namespace::Value)),
                _ => None,
            };
            if !conditional && let Some((ident, ns)) = binding {
                self.bind(ident.unraw().to_string(), ns, BindingKind::Item, None);
            }
            if let syn::Item::Struct(i) = item
                && !conditional
                && !matches!(i.fields, syn::Fields::Named(_))
            {
                self.bind(
                    i.ident.unraw().to_string(),
                    Namespace::Value,
                    BindingKind::Item,
                    None,
                );
            }
        }
        for item in items {
            if !self.enabled(item.span()) {
                continue;
            }
            match item {
                syn::Item::Verbatim(tokens) => {
                    if let Ok(imports) = verbatim_imports(tokens.clone()) {
                        for import in imports {
                            self.import_tree(
                                &import.tree,
                                Vec::new(),
                                import.leading_colon.is_some(),
                                &import.vis,
                            );
                        }
                    }
                }
                syn::Item::Use(i) => {
                    self.import_tree(&i.tree, Vec::new(), i.leading_colon.is_some(), &i.vis);
                }
                syn::Item::ExternCrate(i) => {
                    let name = i.ident.unraw().to_string();
                    self.observe(name.clone(), i.ident.span(), EvidenceKind::Import);
                    let alias = i
                        .rename
                        .as_ref()
                        .map_or_else(|| name.clone(), |(_, i)| i.unraw().to_string());
                    self.add_import(vec![name], Some(alias), true, false, &i.vis, i.ident.span());
                }
                _ => {}
            }
        }
    }
    fn import_tree(
        &mut self,
        tree: &syn::UseTree,
        mut prefix: Vec<String>,
        absolute: bool,
        vis: &syn::Visibility,
    ) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.unraw().to_string());
                self.import_tree(&p.tree, prefix, absolute, vis);
            }
            syn::UseTree::Group(g) => {
                for tree in &g.items {
                    self.import_tree(tree, prefix.clone(), absolute, vis);
                }
            }
            syn::UseTree::Name(n) => {
                let alias = if n.ident == "self" {
                    prefix.last().cloned()
                } else {
                    prefix.push(n.ident.unraw().to_string());
                    Some(n.ident.unraw().to_string())
                };
                self.add_import(prefix, alias, absolute, false, vis, n.ident.span());
            }
            syn::UseTree::Rename(n) => {
                if n.ident != "self" {
                    prefix.push(n.ident.unraw().to_string());
                }
                self.add_import(
                    prefix,
                    Some(n.rename.unraw().to_string()),
                    absolute,
                    false,
                    vis,
                    n.ident.span(),
                );
            }
            syn::UseTree::Glob(g) => {
                self.add_import(prefix, None, absolute, true, vis, g.star_token.span);
            }
        }
    }
    fn add_import(
        &mut self,
        path: Vec<String>,
        alias: Option<String>,
        absolute: bool,
        glob: bool,
        vis: &syn::Visibility,
        span: Span,
    ) {
        let start = span.start();
        let namespaces = [Namespace::Type, Namespace::Value, Namespace::Macro];
        let local_name = match path.as_slice() {
            [name] if !absolute => Some(name),
            [qualifier, name] if !absolute && qualifier == "self" => Some(name),
            _ => None,
        };
        let local_namespaces: Vec<_> = local_name
            .into_iter()
            .flat_map(|name| {
                namespaces.into_iter().filter(|ns| {
                    self.lookup(name, *ns)
                        .is_some_and(|binding| binding.kind != BindingKind::Import)
                })
            })
            .collect();
        let id = self.index.imports.len();
        self.index.imports.push(Import {
            scope: self.scope,
            path,
            alias: alias.clone(),
            absolute,
            glob,
            public: !matches!(vis, syn::Visibility::Inherited),
            line: start.line,
            column: start.column + 1,
            conditions: self.conditions(span),
        });
        if glob {
            self.gaps
                .insert("glob import requires cross-file namespace resolution".into());
        } else if self.conditions(span).is_empty()
            && let Some(alias) = alias.filter(|s| s != "_")
        {
            // Import namespaces must be refined by the module graph. These entries
            // retain import identity rather than pretending to be local definitions.
            let namespaces = if local_namespaces.is_empty() {
                if self.index.imports[id].path.len() == 1 {
                    vec![Namespace::Type]
                } else {
                    namespaces.to_vec()
                }
            } else {
                local_namespaces
            };
            for ns in namespaces {
                self.bind(alias.clone(), ns, BindingKind::Import, Some(id));
            }
        }
    }
    fn condition_expr(&mut self, expr: &syn::Expr) {
        match expr {
            syn::Expr::Let(expr) => {
                self.visit_expr(&expr.expr);
                self.visit_pat(&expr.pat);
                self.pattern(&expr.pat);
            }
            syn::Expr::Binary(expr) if matches!(expr.op, syn::BinOp::And(_)) => {
                self.condition_expr(&expr.left);
                self.condition_expr(&expr.right);
            }
            syn::Expr::Paren(expr) => self.condition_expr(&expr.expr),
            _ => self.visit_expr(expr),
        }
    }
}

impl<'ast> Visit<'ast> for Resolver<'_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if !self.enabled(item.span()) {
            return;
        }
        // Associated types are deliberately not predeclared as lexical types.
        match item {
            syn::Item::Mod(module) => {
                for attr in &module.attrs {
                    self.visit_attribute(attr);
                }
                if let Some((_, items)) = &module.content {
                    let old = self.push(true);
                    self.index.scopes[self.scope]
                        .module
                        .push(module.ident.unraw().to_string());
                    self.items(items);
                    for item in items {
                        self.visit_item(item);
                    }
                    self.scope = old;
                }
            }
            syn::Item::Verbatim(tokens) => match verbatim_imports(tokens.clone()) {
                Ok(imports) => {
                    for import in imports {
                        self.observe_use(&import);
                    }
                }
                Err(error) => {
                    self.gaps
                        .insert(format!("unresolved verbatim Rust syntax: {error}"));
                }
            },
            syn::Item::Use(item) => self.observe_use(item),
            syn::Item::ExternCrate(_) => {}
            syn::Item::Macro(item) => self.visit_item_macro(item),
            _ => {
                let old = self.push(false);
                self.index.scopes[self.scope].item_boundary = true;
                let generics = match item {
                    syn::Item::Fn(i) => Some(&i.sig.generics),
                    syn::Item::Struct(i) => Some(&i.generics),
                    syn::Item::Enum(i) => Some(&i.generics),
                    syn::Item::Union(i) => Some(&i.generics),
                    syn::Item::Trait(i) => Some(&i.generics),
                    syn::Item::TraitAlias(i) => Some(&i.generics),
                    syn::Item::Type(i) => Some(&i.generics),
                    syn::Item::Impl(i) => Some(&i.generics),
                    _ => None,
                };
                if let Some(generics) = generics {
                    self.generics(generics);
                }
                visit::visit_item(self, item);
                self.scope = old;
            }
        }
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let old = self.push(false);
        self.items(block.stmts.iter().filter_map(|stmt| match stmt {
            syn::Stmt::Item(item) => Some(item),
            _ => None,
        }));
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
        self.scope = old;
    }
    fn visit_signature(&mut self, sig: &'ast syn::Signature) {
        self.generics(&sig.generics);
        visit::visit_signature(self, sig);
        for arg in &sig.inputs {
            if let syn::FnArg::Typed(arg) = arg {
                self.pattern(&arg.pat);
            } else {
                self.bind("self".into(), Namespace::Value, BindingKind::Local, None);
            }
        }
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let old = self.push(false);
        visit::visit_impl_item_fn(self, item);
        self.scope = old;
    }
    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        let old = self.push(false);
        visit::visit_trait_item_fn(self, item);
        self.scope = old;
    }
    fn visit_impl_item_type(&mut self, item: &'ast syn::ImplItemType) {
        let old = self.push(false);
        self.generics(&item.generics);
        visit::visit_impl_item_type(self, item);
        self.scope = old;
    }
    fn visit_trait_item_type(&mut self, item: &'ast syn::TraitItemType) {
        let old = self.push(false);
        self.generics(&item.generics);
        visit::visit_trait_item_type(self, item);
        self.scope = old;
    }
    fn visit_bound_lifetimes(&mut self, lifetimes: &'ast syn::BoundLifetimes) {
        self.generics(&syn::Generics {
            lt_token: Some(lifetimes.lt_token),
            params: lifetimes.lifetimes.clone(),
            gt_token: Some(lifetimes.gt_token),
            where_clause: None,
        });
        visit::visit_bound_lifetimes(self, lifetimes);
    }
    fn visit_trait_bound(&mut self, bound: &'ast syn::TraitBound) {
        let old = self.push(false);
        visit::visit_trait_bound(self, bound);
        self.scope = old;
    }
    fn visit_type_bare_fn(&mut self, ty: &'ast syn::TypeBareFn) {
        let old = self.push(false);
        visit::visit_type_bare_fn(self, ty);
        self.scope = old;
    }
    fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
        let name = lifetime.ident.unraw().to_string();
        let start = lifetime.span().start();
        self.index.references.push(Reference {
            scope: self.scope,
            path: vec![name.clone()],
            namespace: Namespace::Lifetime,
            absolute: false,
            line: start.line,
            column: start.column + 1,
            resolution: if name == "static"
                || name == "_"
                || self.lookup(&name, Namespace::Lifetime).is_some()
            {
                Resolution::Local
            } else {
                Resolution::Qualified
            },
        });
    }
    fn visit_local(&mut self, local: &'ast syn::Local) {
        for attr in &local.attrs {
            self.visit_attribute(attr);
        }
        self.visit_pat(&local.pat);
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
        self.pattern(&local.pat);
    }
    fn visit_expr_closure(&mut self, expr: &'ast syn::ExprClosure) {
        let old = self.push(false);
        for attr in &expr.attrs {
            self.visit_attribute(attr);
        }
        for input in &expr.inputs {
            self.visit_pat(input);
            self.pattern(input);
        }
        self.visit_return_type(&expr.output);
        self.visit_expr(&expr.body);
        self.scope = old;
    }
    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        let old = self.push(false);
        self.visit_pat(&expr.pat);
        self.pattern(&expr.pat);
        self.visit_block(&expr.body);
        self.scope = old;
    }
    fn visit_expr_if(&mut self, expr: &'ast syn::ExprIf) {
        let old = self.push(false);
        self.condition_expr(&expr.cond);
        self.visit_block(&expr.then_branch);
        self.scope = old;
        if let Some((_, otherwise)) = &expr.else_branch {
            self.visit_expr(otherwise);
        }
    }
    fn visit_expr_while(&mut self, expr: &'ast syn::ExprWhile) {
        let old = self.push(false);
        self.condition_expr(&expr.cond);
        self.visit_block(&expr.body);
        self.scope = old;
    }
    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        let old = self.push(false);
        self.visit_pat(&arm.pat);
        self.pattern(&arm.pat);
        if let Some((_, guard)) = &arm.guard {
            self.visit_expr(guard);
        }
        self.visit_expr(&arm.body);
        self.scope = old;
    }
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if let Some(qself) = &node.qself {
            self.visit_type(&qself.ty);
            if qself.position == 0 {
                for segment in &node.path.segments {
                    self.visit_path_arguments(&segment.arguments);
                }
                return;
            }
        }
        self.path(&node.path, Namespace::Type, EvidenceKind::Direct);
    }
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        if let Some(qself) = &node.qself {
            self.visit_type(&qself.ty);
            if qself.position == 0 {
                for segment in &node.path.segments {
                    self.visit_path_arguments(&segment.arguments);
                }
                return;
            }
        }
        self.path(&node.path, Namespace::Value, EvidenceKind::Direct);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.path(path, Namespace::Type, EvidenceKind::Direct);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.path(&mac.path, Namespace::Macro, EvidenceKind::Macro);
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item.mac.path.is_ident("macro_rules") {
            if let Some(ident) = &item.ident {
                self.bind(
                    ident.unraw().to_string(),
                    Namespace::Macro,
                    BindingKind::Macro,
                    None,
                );
            }
        } else {
            self.visit_macro(&item.mac);
        }
    }
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if !self.enabled(attr.span()) {
            return;
        }
        if attr.path().is_ident("derive") {
            if let Ok(paths) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) {
                for path in paths {
                    self.path(&path, Namespace::Macro, EvidenceKind::Macro);
                }
            }
        } else if !attr.path().is_ident("cfg") && !attr.path().is_ident("cfg_attr") {
            self.path(attr.path(), Namespace::Macro, EvidenceKind::Macro);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inspect(text: &str) -> (Vec<Observation>, SyntaxIndex) {
        // Production positions come from the tokenizer. Populate all character
        // boundaries here so these tests focus on AST resolution, not cfg filtering.
        let positions: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
        let (observations, gaps, index) =
            analyze(text, &positions, &vec![Vec::new(); positions.len()]).unwrap();
        assert!(
            gaps.iter()
                .all(|gap| gap
                    .starts_with("import target namespace requires module graph resolution:")),
            "{gaps:?}"
        );
        (observations, index)
    }
    fn has(text: &str, name: &str) -> bool {
        inspect(text).0.iter().any(|o| o.name == name)
    }
    #[test]
    fn ast_cfg_filtering_preserves_only_active_references() {
        let minimal = super::super::super::AnalysisProfile {
            default_features: false,
            ..Default::default()
        };
        let source = "#[cfg_attr(feature=\"special\", cfg(feature=\"missing\"))] fn first(){ used::run(); } #[cfg(feature=\"special\")] fn second(){ unused::run(); }";
        let evidence = super::super::super::source::inspect_with(source, true, Some(&minimal));
        assert!(evidence.names.contains("used"));
        assert!(
            !evidence.names.contains("unused"),
            "{:#?}",
            evidence.observations
        );
        let full = super::super::super::AnalysisProfile {
            features: vec!["special".into()],
            ..Default::default()
        };
        let evidence = super::super::super::source::inspect_with(source, true, Some(&full));
        assert!(
            !evidence.names.contains("used"),
            "{:#?}",
            evidence.observations
        );
        assert!(evidence.names.contains("unused"));
        let source = "mod nested { #![cfg(feature=\"disabled\")] fn f() { unused::run(); } } fn f() { used::run(); }";
        let evidence = super::super::super::source::inspect_with(source, true, Some(&minimal));
        assert!(evidence.names.contains("used"));
        assert!(
            !evidence.names.contains("unused"),
            "{:#?}",
            evidence.observations
        );
    }
    #[test]
    fn associated_types_do_not_hide_external_crates() {
        assert!(has(
            "trait T { type used; fn f() { used::run(); } }",
            "used"
        ));
    }
    #[test]
    fn macro_paths_use_type_roots_but_bare_macros_have_their_own_namespace() {
        assert!(!has("mod used {} fn f() { used::call!(); }", "used"));
        assert!(has("struct used; fn f() { used!(); }", "used"));
        assert!(!has(
            "macro_rules! used { () => {} } fn f() { used!(); }",
            "used"
        ));
        assert!(has(
            "macro_rules! used { () => {} } fn f() { used::run(); }",
            "used"
        ));
    }
    #[test]
    fn raw_identifiers_share_canonical_names_and_preserve_spans() {
        for source in [
            "use r#dep as external; fn run() { external::run(); }",
            "extern crate r#dep as r#external; fn run() { external::run(); }",
            "pub use {r#other, ::r#dep as r#external}; fn run() { external::run(); }",
            "fn run() { r#dep::run(); }",
        ] {
            let evidence = super::super::super::source::inspect_with(source, true, None);
            assert!(
                evidence.names.contains("dep"),
                "{source}: {:?}",
                evidence.names
            );
            assert!(!evidence.names.contains("r#dep"));
            let index = evidence.syntax.unwrap();
            assert!(
                index
                    .imports
                    .iter()
                    .flat_map(|import| &import.path)
                    .all(|name| !name.starts_with("r#"))
            );
            assert!(
                index
                    .references
                    .iter()
                    .flat_map(|reference| &reference.path)
                    .all(|name| !name.starts_with("r#"))
            );
        }
        for source in [
            "mod r#dep {} fn run() { dep::run(); }",
            "mod dep {} fn run() { r#dep::run(); }",
            "fn run<r#dep>() { dep::run(); }",
            "use other as r#dep; fn run() { dep::run(); }",
        ] {
            let evidence = super::super::super::source::inspect_with(source, true, None);
            assert!(!evidence.names.contains("dep"), "{source}");
        }
        let evidence =
            super::super::super::source::inspect_with("fn run() { r#dep::run(); }", true, None);
        let observation = evidence
            .observations
            .iter()
            .find(|observation| observation.name == "dep")
            .unwrap();
        assert_eq!(observation.column, 12);
    }
    #[test]
    fn grouped_absolute_imports_preserve_all_roots() {
        let source = "pub use {async_trait, ::futures, reqwest};";
        let (observations, index) = inspect(source);
        for name in ["async_trait", "futures", "reqwest"] {
            assert!(
                observations
                    .iter()
                    .any(|observation| observation.name == name),
                "{name}"
            );
        }
        assert!(
            index
                .imports
                .iter()
                .any(|import| import.path == ["futures"] && import.absolute)
        );
        assert!(has(
            "mod futures {} pub use {async_trait, ::futures as external, reqwest};",
            "futures"
        ));
    }
    #[test]
    fn absolute_imports_bypass_local_type_bindings() {
        for source in [
            "mod dependency {} use ::dependency as external; pub fn run() { external::run(); }",
            "mod dependency {} use ::{dependency as external}; pub fn run() { external::run(); }",
            "mod dependency {} pub use ::dependency as external;",
        ] {
            let (observations, index) = inspect(source);
            assert!(
                observations.iter().any(|o| o.name == "dependency"),
                "{source}"
            );
            assert!(
                index
                    .imports
                    .iter()
                    .any(|import| import.absolute && import.path == ["dependency"])
            );
        }
    }
    #[test]
    fn local_import_namespaces_and_forward_items_are_distinct() {
        for source in [
            "use function as used; fn function() {} fn f() { used::run(); }",
            "use VALUE as used; const VALUE: usize = 0; fn f() { used::run(); }",
        ] {
            assert!(has(source, "used"), "{source}");
        }
        assert!(!has(
            "use Thing as used; struct Thing; fn f() { used::run(); }",
            "used"
        ));
    }
    #[test]
    fn nested_items_and_closures_have_distinct_capture_rules() {
        let (_, index) = inspect(
            "fn outer<used>() { let value = 1; let closure = || value; fn inner() { value; used::run(); } }",
        );
        let values: Vec<_> = index
            .references
            .iter()
            .filter(|r| r.path == ["value"])
            .map(|r| &r.resolution)
            .collect();
        assert_eq!(values, [&Resolution::Local, &Resolution::External]);
        assert!(
            index
                .references
                .iter()
                .any(|r| r.path.first().is_some_and(|n| n == "used")
                    && r.resolution == Resolution::External)
        );
    }
    #[test]
    fn generic_binders_cover_headers_and_bodies() {
        for source in [
            "fn f<used: Trait>() { used::run(); }",
            "struct W<used: Trait>(used::Item);",
            "impl<used: Trait> W<used> { fn f() { used::run(); } }",
            "type W<used> = used::Item;",
        ] {
            assert!(!has(source, "used"), "{source}");
        }
    }
    #[test]
    fn aliases_keep_original_dependency_ownership() {
        for source in [
            "use {other as used}; fn f() { used::run(); }",
            "use other::Thing as used; fn f() { used::run(); }",
            "extern crate other as used; fn f() { used::run(); }",
        ] {
            assert!(has(source, "other"));
            assert!(!has(source, "used"));
        }
    }
    #[test]
    fn values_do_not_shadow_type_namespace() {
        assert!(has(
            "fn f(used: usize) { let used = 1; used::run(); }",
            "used"
        ));
        assert!(has("fn f() { let used = used::make(); }", "used"));
        let (_, index) = inspect("fn f() { let value = value; let next = value; }");
        let refs: Vec<_> = index
            .references
            .iter()
            .filter(|r| r.path == ["value"])
            .collect();
        assert_eq!(refs[0].resolution, Resolution::External);
        assert_eq!(refs[1].resolution, Resolution::Local);
    }
    #[test]
    fn control_flow_bindings_do_not_escape() {
        let (_, index) = inspect(
            "fn f() { if let Some(x) = input { consume(x); } else { consume(x); } for x in input { consume(x); } consume(x); let f = |x| x; consume(x); }",
        );
        let refs: Vec<_> = index
            .references
            .iter()
            .filter(|r| r.path == ["x"])
            .map(|r| &r.resolution)
            .collect();
        assert_eq!(
            refs,
            [
                &Resolution::Local,
                &Resolution::External,
                &Resolution::Local,
                &Resolution::External,
                &Resolution::Local,
                &Resolution::External
            ]
        );
    }
    #[test]
    fn unicode_locations_are_character_columns() {
        let (obs, _) = inspect("fn f() { let café = 0; 外部::run(); }");
        let observation = obs.iter().find(|o| o.name == "外部").unwrap();
        assert_eq!((observation.line, observation.column), (1, 24));
    }
}
