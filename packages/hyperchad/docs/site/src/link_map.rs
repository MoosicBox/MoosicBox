//! Relative markdown link resolver.

use std::collections::BTreeMap;

use hyperchad::template::Container;
use hyperchad::transformer::Element;

use crate::registry::DocPage;

/// Source-path to route lookup for registered markdown pages.
#[derive(Clone, Debug, Default)]
pub struct LinkMap {
    source_to_route: BTreeMap<&'static str, &'static str>,
}

impl LinkMap {
    /// Build a link map from registered docs pages.
    #[must_use]
    pub fn from_pages(pages: &[DocPage]) -> Self {
        Self {
            source_to_route: pages
                .iter()
                .filter_map(|page| page.source.map(|source| (source, page.route)))
                .collect(),
        }
    }

    /// Resolve a markdown `href` relative to `source_path` into a docs route.
    #[must_use]
    pub fn resolve_href(&self, href: &str, source_path: &str) -> Option<String> {
        resolve_href_with_lookup(href, source_path, |source| {
            self.source_to_route.get(source).copied()
        })
    }

    /// Walk a rendered container tree and rewrite relative `.md` links to known routes.
    pub fn rewrite_relative_links(&self, container: &mut Container, source_path: &str) {
        rewrite_relative_links_with(container, source_path, |href, source| {
            self.resolve_href(href, source)
        });
    }
}

/// Walk a rendered container tree and rewrite relative `.md` links to known routes.
pub fn rewrite_relative_links(container: &mut Container, source_path: &str, pages: &[DocPage]) {
    LinkMap::from_pages(pages).rewrite_relative_links(container, source_path);
}

/// Resolve a markdown `href` relative to a source path into a docs route.
#[must_use]
pub fn resolve_href(href: &str, source_path: &str, pages: &[DocPage]) -> Option<String> {
    let link_map = LinkMap::from_pages(pages);
    link_map.resolve_href(href, source_path)
}

fn rewrite_relative_links_with(
    container: &mut Container,
    source_path: &str,
    resolve: impl Fn(&str, &str) -> Option<String> + Copy,
) {
    if let Element::Anchor { href, .. } = &mut container.element
        && let Some(current) = href.as_deref()
        && let Some(rewritten) = resolve(current, source_path)
    {
        *href = Some(rewritten);
    }

    for child in &mut container.children {
        rewrite_relative_links_with(child, source_path, resolve);
    }
}

fn resolve_href_with_lookup(
    href: &str,
    source_path: &str,
    lookup_route: impl Fn(&str) -> Option<&'static str>,
) -> Option<String> {
    if href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with('#')
        || href.starts_with('/')
        || !href.contains(".md")
    {
        return None;
    }

    let (path_part, fragment) = href.split_once('#').map_or((href, None), |(path, frag)| {
        (path, if frag.is_empty() { None } else { Some(frag) })
    });
    let base = parent_dir(source_path);
    let resolved = normalize_join(&base, path_part)?;

    lookup_route(&resolved).map(|route| {
        let mut route = route.to_string();
        if let Some(fragment) = fragment {
            route.push('#');
            route.push_str(fragment);
        }
        route
    })
}

#[must_use]
fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(parent, _)| parent.to_string())
}

#[must_use]
fn normalize_join(base: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = if base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };

    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }

    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use crate::registry::PageKind;

    use super::*;

    fn pages() -> Vec<DocPage> {
        vec![DocPage {
            source: Some("docs/bpdl-spec.md"),
            route: "/docs/bpdl-spec",
            title: None,
            section: None,
            nav_label: None,
            kind: PageKind::Markdown { contents: "" },
        }]
    }

    #[test]
    fn relative_path_resolves() {
        assert_eq!(
            resolve_href("./bpdl-spec.md#grammar", "docs/plugins.md", &pages()),
            Some("/docs/bpdl-spec#grammar".to_string())
        );
    }

    #[test]
    fn parent_path_resolves() {
        assert_eq!(
            resolve_href("../bpdl-spec.md", "docs/plugins/index.md", &pages()),
            Some("/docs/bpdl-spec".to_string())
        );
    }

    #[test]
    fn unknown_path_is_ignored() {
        assert_eq!(
            resolve_href("./missing.md", "docs/plugins.md", &pages()),
            None
        );
    }
}
