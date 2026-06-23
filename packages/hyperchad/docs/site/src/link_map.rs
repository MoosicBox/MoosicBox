//! Relative markdown link resolver.

use hyperchad::template::Container;
use hyperchad::transformer::Element;

use crate::registry::DocPage;

/// Walk a rendered container tree and rewrite relative `.md` links to known routes.
pub fn rewrite_relative_links(container: &mut Container, source_path: &str, pages: &[DocPage]) {
    if let Element::Anchor { href, .. } = &mut container.element
        && let Some(current) = href.as_deref()
        && let Some(rewritten) = resolve_href(current, source_path, pages)
    {
        *href = Some(rewritten);
    }

    for child in &mut container.children {
        rewrite_relative_links(child, source_path, pages);
    }
}

/// Resolve a markdown `href` relative to a source path into a docs route.
#[must_use]
pub fn resolve_href(href: &str, source_path: &str, pages: &[DocPage]) -> Option<String> {
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

    pages.iter().find_map(|page| {
        if page.source == Some(resolved.as_str()) {
            let mut route = page.route.to_string();
            if let Some(fragment) = fragment {
                route.push('#');
                route.push_str(fragment);
            }
            Some(route)
        } else {
            None
        }
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
    fn unknown_path_is_ignored() {
        assert_eq!(
            resolve_href("./missing.md", "docs/plugins.md", &pages()),
            None
        );
    }
}
