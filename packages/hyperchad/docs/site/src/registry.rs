//! Page registry types for documentation sites.

use hyperchad::template::Containers;

/// Function that renders a documentation page.
pub type PageRenderer = fn(&DocPage) -> Containers;

/// Registered page kind.
#[derive(Clone)]
pub enum PageKind {
    /// `Markdown` page with embedded source text.
    Markdown {
        /// Markdown source contents.
        contents: &'static str,
    },
    /// Generated page rendered dynamically by the consumer.
    Generated {
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
    /// Optional sidebar section title.
    pub section: Option<&'static str>,
    /// Optional sidebar label.
    pub nav_label: Option<&'static str>,
    /// Page kind.
    pub kind: PageKind,
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

/// Build sidebar sections from registered pages, preserving declaration order.
#[must_use]
pub fn nav_sections(pages: &[DocPage]) -> Vec<NavSection> {
    let mut sections = Vec::<NavSection>::new();

    for page in pages {
        let (Some(section), Some(label)) = (page.section, page.nav_label) else {
            continue;
        };

        if let Some(existing) = sections
            .iter_mut()
            .find(|candidate| candidate.title == section)
        {
            existing.items.push(NavItem {
                label,
                href: page.route,
            });
        } else {
            sections.push(NavSection {
                title: section,
                items: vec![NavItem {
                    label,
                    href: page.route,
                }],
            });
        }
    }

    sections
}
