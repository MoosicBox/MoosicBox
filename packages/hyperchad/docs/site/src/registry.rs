//! Page registry types for documentation sites.

use hyperchad::template::Containers;

/// Function that renders a documentation page.
pub type PageRenderer = fn(&DocPage) -> Containers;

/// Declared navigation section.
#[derive(Clone, Copy)]
pub struct DocsSection {
    /// Stable section identifier used by pages.
    pub id: &'static str,
    /// Human-readable section title.
    pub title: &'static str,
}

impl DocsSection {
    /// Create a docs navigation section.
    #[must_use]
    pub const fn new(id: &'static str, title: &'static str) -> Self {
        Self { id, title }
    }
}

/// Registered page kind.
#[derive(Clone)]
pub enum PageKind {
    /// `Markdown` page with embedded source text.
    Markdown {
        /// Markdown source contents.
        contents: &'static str,
    },
    /// Generated markdown page rendered dynamically by the consumer.
    GeneratedMarkdown {
        /// Markdown-producing generator.
        generate: fn() -> String,
    },
    /// Custom `HyperChad` page renderer.
    Custom {
        /// Page renderer.
        render: PageRenderer,
    },
}

/// A single docs page registered with the site.
#[derive(Clone)]
pub struct DocPage {
    /// Workspace-relative source path for markdown link rewriting.
    pub source: Option<&'static str>,
    /// Site route.
    pub route: &'static str,
    /// Optional page title.
    pub title: Option<&'static str>,
    /// Optional sidebar section identifier.
    pub section: Option<&'static str>,
    /// Optional sidebar label.
    pub nav_label: Option<&'static str>,
    /// Page kind.
    pub kind: PageKind,
}

impl DocPage {
    /// Create a markdown documentation page.
    #[must_use]
    pub const fn markdown(
        source: &'static str,
        route: &'static str,
        contents: &'static str,
    ) -> Self {
        Self {
            source: Some(source),
            route,
            title: None,
            section: None,
            nav_label: None,
            kind: PageKind::Markdown { contents },
        }
    }

    /// Create a generated markdown documentation page.
    #[must_use]
    pub const fn generated_markdown(route: &'static str, generate: fn() -> String) -> Self {
        Self {
            source: None,
            route,
            title: None,
            section: None,
            nav_label: None,
            kind: PageKind::GeneratedMarkdown { generate },
        }
    }

    /// Create a generated markdown documentation page.
    ///
    /// Alias for [`Self::generated_markdown`] for concise call sites.
    #[must_use]
    pub const fn generated(route: &'static str, generate: fn() -> String) -> Self {
        Self::generated_markdown(route, generate)
    }

    /// Create a custom-rendered documentation page.
    #[must_use]
    pub const fn custom(route: &'static str, render: PageRenderer) -> Self {
        Self {
            source: None,
            route,
            title: None,
            section: None,
            nav_label: None,
            kind: PageKind::Custom { render },
        }
    }

    /// Set the page title.
    #[must_use]
    pub const fn title(mut self, title: &'static str) -> Self {
        self.title = Some(title);
        self
    }

    /// Set the page navigation section identifier.
    #[must_use]
    pub const fn section(mut self, section: &'static str) -> Self {
        self.section = Some(section);
        self
    }

    /// Set the page navigation label.
    #[must_use]
    pub const fn nav_label(mut self, nav_label: &'static str) -> Self {
        self.nav_label = Some(nav_label);
        self
    }

    /// Set both sidebar section identifier and navigation label.
    #[must_use]
    pub const fn nav(mut self, section: &'static str, nav_label: &'static str) -> Self {
        self.section = Some(section);
        self.nav_label = Some(nav_label);
        self
    }
}

/// A sidebar navigation section.
#[derive(Clone)]
pub struct NavSection {
    /// Section title.
    pub title: &'static str,
    /// Items in declaration order.
    pub items: Vec<NavItem>,
}

/// A sidebar navigation item.
#[derive(Clone)]
pub struct NavItem {
    /// Link label.
    pub label: &'static str,
    /// Link target.
    pub href: &'static str,
}

/// Build sidebar sections from declared sections and pages.
#[must_use]
pub fn nav_sections(sections: &[DocsSection], pages: &[DocPage]) -> Vec<NavSection> {
    sections
        .iter()
        .filter_map(|section| {
            let items: Vec<NavItem> = pages
                .iter()
                .filter(|page| page.section == Some(section.id))
                .filter_map(|page| {
                    page.nav_label.map(|label| NavItem {
                        label,
                        href: page.route,
                    })
                })
                .collect();

            if items.is_empty() {
                None
            } else {
                Some(NavSection {
                    title: section.title,
                    items,
                })
            }
        })
        .collect()
}

/// Declare a compile-time markdown documentation page.
#[macro_export]
macro_rules! docs_markdown_page {
    (
        source: $source:literal,
        route: $route:literal,
        title: None,
        section: $section:expr,
        nav_label: $nav_label:expr,
        contents: $contents:expr $(,)?
    ) => {
        $crate::DocPage::markdown($source, $route, $contents).nav($section, $nav_label)
    };
    (
        source: $source:literal,
        route: $route:literal,
        title: None,
        section: $section:expr,
        nav_label: $nav_label:expr $(,)?
    ) => {
        $crate::DocPage::markdown(
            $source,
            $route,
            include_str!(concat!("../../../../", $source)),
        )
        .nav($section, $nav_label)
    };
    (
        source: $source:literal,
        route: $route:literal,
        title: $title:expr,
        section: $section:expr,
        nav_label: $nav_label:expr,
        contents: $contents:expr $(,)?
    ) => {
        $crate::DocPage::markdown($source, $route, $contents)
            .title($title)
            .nav($section, $nav_label)
    };
    (
        source: $source:literal,
        route: $route:literal,
        title: $title:expr,
        section: $section:expr,
        nav_label: $nav_label:expr $(,)?
    ) => {
        $crate::DocPage::markdown(
            $source,
            $route,
            include_str!(concat!("../../../../", $source)),
        )
        .title($title)
        .nav($section, $nav_label)
    };
}

/// Declare a generated markdown documentation page.
#[macro_export]
macro_rules! docs_generated_page {
    (
        route: $route:literal,
        title: None,
        section: $section:expr,
        nav_label: $nav_label:expr,
        generate: $generate:expr $(,)?
    ) => {
        $crate::DocPage::generated_markdown($route, $generate).nav($section, $nav_label)
    };
    (
        route: $route:literal,
        title: $title:expr,
        section: $section:expr,
        nav_label: $nav_label:expr,
        generate: $generate:expr $(,)?
    ) => {
        $crate::DocPage::generated_markdown($route, $generate)
            .title($title)
            .nav($section, $nav_label)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate() -> String {
        "# Generated".to_string()
    }

    #[test]
    fn nav_sets_section_and_label() {
        let page = DocPage::generated("/docs/generated", generate).nav("reference", "Generated");

        assert_eq!(page.section, Some("reference"));
        assert_eq!(page.nav_label, Some("Generated"));
    }

    #[test]
    fn nav_sections_preserve_declared_section_and_page_order() {
        let sections = [
            DocsSection::new("getting-started", "Getting Started"),
            DocsSection::new("reference", "Reference"),
        ];
        let pages = [
            DocPage::generated("/docs/cli", generate).nav("reference", "CLI"),
            DocPage::generated("/docs", generate).nav("getting-started", "Overview"),
            DocPage::generated("/docs/config", generate).nav("reference", "Config"),
            DocPage::generated("/hidden", generate),
        ];

        let nav = nav_sections(&sections, &pages);

        assert_eq!(nav.len(), 2);
        assert_eq!(nav[0].title, "Getting Started");
        assert_eq!(nav[0].items[0].label, "Overview");
        assert_eq!(nav[1].title, "Reference");
        assert_eq!(nav[1].items[0].label, "CLI");
        assert_eq!(nav[1].items[1].label, "Config");
    }
}
