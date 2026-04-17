use hyperchad_renderer::{
    View,
    transformer::{Container, models::Selector},
};

use crate::dom::{DomState, select};

/// Applies a `View` update to the virtual DOM.
pub fn apply_view(dom: &mut DomState, view: &View) {
    if let Some(primary) = &view.primary {
        if matches!(view.fragments.as_slice(), []) && matches!(view.delete_selectors.as_slice(), [])
        {
            dom.set_root(Some(primary.clone()));
        } else if dom.root().is_none() {
            dom.set_root(Some(primary.clone()));
        } else if let Some(root) = dom.root_mut() {
            *root = primary.clone();
        }
    }

    for fragment in &view.fragments {
        replace_first(dom, &fragment.selector, fragment.container.clone());
    }

    for selector in &view.delete_selectors {
        delete_matching(dom, selector);
    }
}

fn replace_first(dom: &mut DomState, selector: &Selector, replacement: Container) {
    let Some(root) = dom.root_mut() else {
        if matches!(selector, Selector::SelfTarget) {
            dom.set_root(Some(replacement));
        }
        return;
    };

    if select::matches(root, selector) {
        *root = replacement;
        return;
    }

    let _replaced = replace_child_recursive(root, selector, &replacement);
}

fn replace_child_recursive(
    current: &mut Container,
    selector: &Selector,
    replacement: &Container,
) -> bool {
    for child in &mut current.children {
        if select::matches(child, selector) {
            *child = replacement.clone();
            return true;
        }
    }

    for child in &mut current.children {
        if replace_child_recursive(child, selector, replacement) {
            return true;
        }
    }

    false
}

fn delete_matching(dom: &mut DomState, selector: &Selector) {
    let Some(root) = dom.root_mut() else {
        return;
    };

    if select::matches(root, selector) {
        dom.set_root(None);
        return;
    }

    delete_from_children(root, selector);
}

fn delete_from_children(current: &mut Container, selector: &Selector) {
    current
        .children
        .retain(|child| !select::matches(child, selector));
    for child in &mut current.children {
        delete_from_children(child, selector);
    }
}
