use hyperchad_renderer::transformer::models::Selector;
use hyperchad_renderer::{View, transformer::Container};

pub mod patch;
pub mod select;

/// Virtual DOM state used by the testing harness.
#[derive(Debug, Clone, Default)]
pub struct DomState {
    root: Option<Container>,
}

impl DomState {
    /// Creates an empty DOM state.
    #[must_use]
    pub const fn new() -> Self {
        Self { root: None }
    }

    /// Returns the current root container.
    #[must_use]
    pub const fn root(&self) -> Option<&Container> {
        self.root.as_ref()
    }

    /// Returns a mutable reference to the root container.
    #[must_use]
    pub fn root_mut(&mut self) -> Option<&mut Container> {
        self.root.as_mut()
    }

    /// Sets the root container.
    pub fn set_root(&mut self, root: Option<Container>) {
        self.root = root;
    }

    /// Applies a `View` update onto the virtual DOM.
    pub fn apply_view(&mut self, view: &View) {
        patch::apply_view(self, view);
    }

    /// Returns whether the selector matches at least one node.
    #[must_use]
    pub fn contains_selector(&self, selector: &Selector) -> bool {
        self.root
            .as_ref()
            .and_then(|root| select::find_first_id(root, selector))
            .is_some()
    }

    /// Collects all element IDs in depth-first order.
    #[must_use]
    pub fn collect_ids(&self) -> Vec<usize> {
        let mut ids = vec![];
        if let Some(root) = &self.root {
            select::collect_ids(root, &mut ids);
        }
        ids
    }

    /// Returns the root-to-target path for the given element ID.
    #[must_use]
    pub fn path_to_root(&self, target_id: usize) -> Option<Vec<usize>> {
        let root = self.root.as_ref()?;
        select::path_to_id(root, target_id).map(|mut path| {
            path.reverse();
            path
        })
    }
}
