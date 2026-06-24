//! Documentation site builder and default rendering.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use hyperchad::app::{App, AppBuilder, renderer::DefaultRenderer};
use hyperchad::markdown::markdown_to_container;
use hyperchad::router::Router;
use hyperchad::template::{Container, Containers, container};

#[cfg(feature = "assets")]
use hyperchad::renderer::assets::StaticAssetRoute;

use crate::link_map::LinkMap;
use crate::markdown::MarkdownStyle;
use crate::registry::{DocPage, DocsSection, NavSection, PageKind, nav_sections};
use crate::theme::Theme;

/// Default viewport meta tag for responsive documentation sites.
pub static VIEWPORT: LazyLock<String> =
    LazyLock::new(|| "width=device-width, initial-scale=1".to_string());

/// Opt-in markdown scan configuration.
#[derive(Clone)]
pub struct MarkdownScan {
    root: PathBuf,
    route_prefix: String,
    section: Option<&'static str>,
}

impl MarkdownScan {
    /// Create a markdown scan rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            route_prefix: "/docs".to_string(),
            section: None,
        }
    }

    /// Set the route prefix for scanned markdown pages.
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn route_prefix(mut self, route_prefix: impl Into<String>) -> Self {
        self.route_prefix = route_prefix.into();
        self
    }

    /// Set the default sidebar section for scanned `Markdown` pages.
    #[must_use]
    pub const fn section(mut self, section: &'static str) -> Self {
        self.section = Some(section);
        self
    }
}

/// Link rendered in the default docs header.
#[derive(Clone)]
pub struct HeaderLink {
    label: &'static str,
    href: &'static str,
    external: bool,
}

impl HeaderLink {
    /// Create an internal header link.
    #[must_use]
    pub const fn new(label: &'static str, href: &'static str) -> Self {
        Self {
            label,
            href,
            external: false,
        }
    }

    /// Create an external header link.
    #[must_use]
    pub const fn external(label: &'static str, href: &'static str) -> Self {
        Self {
            label,
            href,
            external: true,
        }
    }

    /// Link label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        self.label
    }

    /// Link target.
    #[must_use]
    pub const fn href(&self) -> &'static str {
        self.href
    }

    /// Whether this is an external link.
    #[must_use]
    pub const fn is_external(&self) -> bool {
        self.external
    }
}

/// Context passed to custom docs site shells.
pub struct ShellContext<'a> {
    /// Site model.
    pub site: &'a DocsSite,
    /// Current route path.
    pub current_path: &'a str,
    /// Page title, when available.
    pub title: Option<&'static str>,
    /// Rendered page body.
    pub body: &'a Containers,
    /// Navigation sections derived from the site registry.
    pub sections: Vec<NavSection>,
}

impl ShellContext<'_> {
    /// Render the default sidebar for this shell context.
    #[must_use]
    pub fn sidebar(&self) -> Containers {
        render_sidebar(&self.sections, self.current_path, &self.site.theme)
    }
}

/// Custom page shell function.
pub type PageShell = for<'a> fn(ShellContext<'a>) -> Containers;

/// Builder for a reusable `HyperChad` documentation site.
pub struct DocsSiteBuilder {
    name: &'static str,
    title: String,
    description: String,
    theme: Theme,
    pages: Vec<DocPage>,
    sections: Vec<DocsSection>,
    home: Option<fn() -> Containers>,
    shell: Option<PageShell>,
    brand: Option<(&'static str, &'static str)>,
    header_links: Vec<HeaderLink>,
    global_font: Option<&'static str>,
    scans: Vec<MarkdownScan>,
}

impl DocsSiteBuilder {
    /// Create a docs site builder for `name`.
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            title: String::new(),
            description: String::new(),
            theme: Theme::default(),
            pages: Vec::new(),
            sections: Vec::new(),
            home: None,
            shell: None,
            brand: None,
            header_links: Vec::new(),
            global_font: None,
            scans: Vec::new(),
        }
    }

    /// Set the browser/app title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the browser/app description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the visual theme.
    #[must_use]
    pub const fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Set a custom home page renderer.
    #[must_use]
    pub const fn home(mut self, home: fn() -> Containers) -> Self {
        self.home = Some(home);
        self
    }

    /// Set a custom page shell used to wrap docs pages.
    #[must_use]
    pub const fn shell(mut self, shell: PageShell) -> Self {
        self.shell = Some(shell);
        self
    }

    /// Set the default header brand text and link.
    #[must_use]
    pub const fn brand(mut self, label: &'static str, href: &'static str) -> Self {
        self.brand = Some((label, href));
        self
    }

    /// Add a link to the default header.
    #[must_use]
    pub fn header_link(mut self, link: HeaderLink) -> Self {
        self.header_links.push(link);
        self
    }

    /// Add links to the default header.
    #[must_use]
    pub fn header_links(mut self, links: impl IntoIterator<Item = HeaderLink>) -> Self {
        self.header_links.extend(links);
        self
    }

    /// Set the global font family for the default shell.
    #[must_use]
    pub const fn global_font(mut self, font: &'static str) -> Self {
        self.global_font = Some(font);
        self
    }

    /// Register navigation sections in display order.
    #[must_use]
    pub fn sections(mut self, sections: &'static [DocsSection]) -> Self {
        self.sections.extend_from_slice(sections);
        self
    }

    /// Register multiple pages in declaration order.
    #[must_use]
    pub fn pages(mut self, pages: &'static [DocPage]) -> Self {
        self.pages.extend_from_slice(pages);
        self
    }

    /// Register a page.
    #[must_use]
    pub fn page(mut self, page: DocPage) -> Self {
        self.pages.push(page);
        self
    }

    /// Register a generated markdown page.
    #[must_use]
    pub fn generated_page(
        mut self,
        route: &'static str,
        title: &'static str,
        section: Option<&'static str>,
        nav_label: Option<&'static str>,
        generate: fn() -> String,
    ) -> Self {
        self.pages.push(DocPage {
            source: None,
            route,
            title: Some(title),
            section,
            nav_label,
            kind: PageKind::GeneratedMarkdown { generate },
        });
        self
    }

    /// Add an opt-in markdown scan. Scanned pages are appended during build.
    #[must_use]
    pub fn scan_markdown(mut self, scan: MarkdownScan) -> Self {
        self.scans.push(scan);
        self
    }

    /// Build the docs site model.
    ///
    /// # Panics
    ///
    /// Panics if a configured markdown scan cannot read the current directory tree.
    #[must_use]
    pub fn build(mut self) -> DocsSite {
        for scan in self.scans.clone() {
            self.scan_pages(&scan);
        }

        let title = if self.title.is_empty() {
            format!("{} docs", self.name)
        } else {
            self.title
        };
        let description = if self.description.is_empty() {
            format!("Documentation for {}", self.name)
        } else {
            self.description
        };
        let link_map = LinkMap::from_pages(&self.pages);

        DocsSite {
            name: self.name,
            title,
            description,
            theme: self.theme,
            pages: self.pages,
            sections: self.sections,
            home: self.home,
            shell: self.shell,
            brand: self.brand,
            header_links: self.header_links,
            global_font: self.global_font,
            link_map,
        }
    }

    fn scan_pages(&mut self, scan: &MarkdownScan) {
        let root = scan.root.clone();
        if !root.exists() {
            return;
        }
        let mut files = Vec::new();
        collect_markdown_files(&root, &mut files).expect("failed to scan markdown docs");
        files.sort();

        for file in files {
            let Ok(source) = file
                .strip_prefix(".")
                .unwrap_or(&file)
                .to_str()
                .map(str::to_string)
                .ok_or(())
            else {
                continue;
            };
            if self
                .pages
                .iter()
                .any(|page| page.source == Some(source.as_str()))
            {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&file) else {
                continue;
            };
            let route = route_for_markdown(&file, &root, &scan.route_prefix);
            let title = title_from_markdown(&contents).unwrap_or_else(|| title_from_path(&file));
            self.pages.push(DocPage {
                source: Some(leak_string(source)),
                route: leak_string(route),
                title: Some(leak_string(title.clone())),
                section: scan.section,
                nav_label: Some(leak_string(title)),
                kind: PageKind::Markdown {
                    contents: leak_string(contents),
                },
            });
        }
    }
}

/// Built documentation site model.
#[derive(Clone)]
pub struct DocsSite {
    name: &'static str,
    title: String,
    description: String,
    theme: Theme,
    pages: Vec<DocPage>,
    sections: Vec<DocsSection>,
    home: Option<fn() -> Containers>,
    shell: Option<PageShell>,
    brand: Option<(&'static str, &'static str)>,
    header_links: Vec<HeaderLink>,
    global_font: Option<&'static str>,
    link_map: LinkMap,
}

impl DocsSite {
    /// Site name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Site theme.
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Configured global font, or the theme monospace font.
    #[must_use]
    pub const fn font_family(&self) -> &'static str {
        if let Some(font) = self.global_font {
            font
        } else {
            self.theme.mono_font
        }
    }
}

impl DocsSite {
    /// Create a docs site builder for `name`.
    #[must_use]
    pub fn builder(name: &'static str) -> DocsSiteBuilder {
        DocsSiteBuilder::new(name)
    }

    /// Initialize a `HyperChad` app builder for this docs site.
    #[must_use]
    pub fn init(self) -> AppBuilder {
        let title = self.title.clone();
        let description = self.description.clone();
        let background = self.theme.background;
        AppBuilder::new()
            .with_router(self.router())
            .with_background(background)
            .with_title(title)
            .with_description(description)
            .with_size(1100.0, 700.0)
    }

    /// Build a router for this docs site.
    #[must_use]
    pub fn router(self) -> Router {
        let site: &'static Self = Box::leak(Box::new(self));
        let mut router = Router::new()
            .with_static_route(&["/", "/home"], move |_| async move { site.render_home() });

        for page in &site.pages {
            let page_ref = page;
            router = router.with_static_route(&[page.route], move |_| async move {
                site.render_page(page_ref)
            });
        }

        router.with_static_route(
            &["/not-found"],
            move |_| async move { site.render_not_found() },
        )
    }

    /// Return default static asset routes used by generated docs sites.
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn assets(&self) -> Vec<StaticAssetRoute> {
        vec![
            #[cfg(feature = "vanilla-js")]
            crate::assets::vanilla_js_route(),
        ]
    }
}

impl DocsSite {
    fn render_home(&self) -> Containers {
        let body = self.home.map_or_else(
            || {
                container! {
                    div direction=column gap=16 {
                        h1 { (self.title.clone()) }
                        span color=#c9d1d9 { (self.description.clone()) }
                    }
                }
            },
            |home| home(),
        );
        self.wrap_page("/", &body)
    }

    fn render_not_found(&self) -> Containers {
        let body = container! {
            div direction=column gap=16 {
                h1 { "Not found" }
                span { "The requested documentation page does not exist." }
            }
        };
        self.wrap_page("/not-found", &body)
    }

    /// Render a documentation page.
    #[must_use]
    pub fn render_page(&self, page: &DocPage) -> Containers {
        match &page.kind {
            PageKind::Markdown { contents } => self.render_markdown_page(page, contents),
            PageKind::GeneratedMarkdown { generate } => {
                let markdown = generate();
                self.render_markdown_page(page, leak_string(markdown))
            }
            PageKind::Custom { render } => render(page),
        }
    }

    fn render_markdown_page(&self, page: &DocPage, markdown: &'static str) -> Containers {
        let content = self.render_markdown_content(page, markdown);
        let body = Self::render_markdown_body(&vec![content]);
        self.wrap_page_with_title(page.route, page.title, &body)
    }

    /// Render markdown content with registry-owned relative link rewriting.
    #[must_use]
    pub fn render_markdown_content(&self, page: &DocPage, markdown: &str) -> Container {
        let mut content = markdown_to_container(markdown);
        if let Some(source) = page.source {
            self.link_map.rewrite_relative_links(&mut content, source);
        }
        let style = MarkdownStyle::new(&self.theme, self.font_family());
        style.apply(&mut content);
        style.apply_body(&mut content);
        content
    }

    fn render_markdown_body(content: &Containers) -> Containers {
        container! {
            div {
                (content)
            }
        }
    }

    /// Wrap rendered content in the configured page shell.
    #[must_use]
    pub fn wrap_page(&self, current_path: &str, body: &Containers) -> Containers {
        self.wrap_page_with_title(current_path, None, body)
    }

    fn wrap_page_with_title(
        &self,
        current_path: &str,
        title: Option<&'static str>,
        body: &Containers,
    ) -> Containers {
        let sections = nav_sections(&self.sections, &self.pages);
        if let Some(shell) = self.shell {
            return shell(ShellContext {
                site: self,
                current_path,
                title,
                body,
                sections,
            });
        }

        let border = (self.theme.border, 1);
        let brand = self.brand.unwrap_or((self.name, "/"));
        let header_links = self.header_links.clone();
        let font_family = self.global_font.unwrap_or(self.theme.mono_font);
        container! {
            div direction=column min-height="100vh" background=(self.theme.background) color=(self.theme.text_primary) font-family=(font_family) {
                header direction=row align-items=center justify-content=space-between padding-x=24 padding-y=16 border-bottom=(border) {
                    anchor href=(brand.1) color=(self.theme.accent) text-decoration="none" font-size=20 font-weight=700 { (brand.0) }
                    div direction=row align-items=center justify-content=end gap=24 {
                        @for link in header_links {
                            @if link.external {
                                anchor href=(link.href) target="_blank" color=(self.theme.text_secondary) text-decoration="none" font-size=14 { (link.label) }
                            } @else {
                                anchor href=(link.href) color=(self.theme.text_secondary) text-decoration="none" font-size=14 { (link.label) }
                            }
                        }
                    }
                }
                div direction=row flex=1 {
                    aside direction=column width=260 min-width=260 background=(self.theme.surface) border-right=(border) padding-y=24 overflow-y=auto {
                        (render_sidebar(&sections, current_path, &self.theme))
                    }
                    main flex=1 padding=32 overflow-x=auto {
                        div max-width=900 direction=column gap=24 {
                            @if let Some(title) = title {
                                h1 { (title) }
                            }
                            (body)
                        }
                    }
                }
            }
        }
    }
}

fn render_sidebar(sections: &[NavSection], current_path: &str, theme: &Theme) -> Containers {
    container! {
        div direction=column gap=24 padding-x=16 {
            @for section in sections {
                div direction=column gap=8 {
                    div color=(theme.text_muted) font-size=12 font-weight=700 {
                        (section.title)
                    }
                    @for item in &section.items {
                        anchor
                            href=(item.href)
                            color=(if item.href == current_path { theme.accent } else { theme.text_secondary })
                            text-decoration="none"
                            padding-y=4
                        {
                            (item.label)
                        }
                    }
                }
            }
        }
    }
}

/// Build the application from an app builder.
///
/// # Errors
///
/// Returns an error if the `HyperChad` app fails to build.
pub fn build_app(builder: AppBuilder) -> Result<App<DefaultRenderer>, hyperchad::app::Error> {
    use hyperchad::renderer::Renderer as _;

    let mut app = builder.build_default()?;
    app.renderer.add_responsive_trigger(
        "mobile".into(),
        hyperchad::renderer::transformer::ResponsiveTrigger::MaxWidth(
            hyperchad::renderer::transformer::Number::Integer(600),
        ),
    );
    app.renderer.add_responsive_trigger(
        "tablet".into(),
        hyperchad::renderer::transformer::ResponsiveTrigger::MaxWidth(
            hyperchad::renderer::transformer::Number::Integer(900),
        ),
    );
    Ok(app)
}

fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    Ok(())
}

fn route_for_markdown(path: &Path, root: &Path, route_prefix: &str) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut parts: Vec<String> = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(ToString::to_string))
        .collect();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".md").to_string();
        if last == "README" {
            parts.pop();
        }
    }
    let suffix = parts.join("/");
    if suffix.is_empty() {
        route_prefix.to_string()
    } else {
        format!("{}/{suffix}", route_prefix.trim_end_matches('/'))
    }
}

fn title_from_markdown(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::to_string))
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Docs")
        .replace(['-', '_'], " ")
}

fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}
