use hyperchad_renderer::transformer::{Container, models::Selector};

/// Parses a selector from a test selector string.
///
/// Supports:
/// * `#id`
/// * `.class`
/// * `> .class`
/// * `self`
pub fn parse_selector(
    value: &str,
) -> Result<Selector, hyperchad_renderer::transformer::models::ParseSelectorError> {
    Selector::try_from(value)
}

/// Returns whether `container` matches the selector.
#[must_use]
pub fn matches(container: &Container, selector: &Selector) -> bool {
    match selector {
        Selector::Id(id) => container.str_id.as_ref().is_some_and(|x| x == id),
        Selector::Class(class) | Selector::ChildClass(class) => {
            container.classes.iter().any(|x| x == class)
        }
        Selector::SelfTarget => true,
    }
}

/// Finds the first matching element ID for a selector.
#[must_use]
pub fn find_first_id(root: &Container, selector: &Selector) -> Option<usize> {
    if matches(root, selector) {
        return Some(root.id);
    }

    root.children
        .iter()
        .find_map(|child| find_first_id(child, selector))
}

/// Collects all IDs in depth-first order.
pub fn collect_ids(root: &Container, out: &mut Vec<usize>) {
    out.push(root.id);
    for child in &root.children {
        collect_ids(child, out);
    }
}

/// Returns the path from root to `target_id`.
#[must_use]
pub fn path_to_id(root: &Container, target_id: usize) -> Option<Vec<usize>> {
    if root.id == target_id {
        return Some(vec![root.id]);
    }

    for child in &root.children {
        if let Some(mut path) = path_to_id(child, target_id) {
            path.insert(0, root.id);
            return Some(path);
        }
    }

    None
}
