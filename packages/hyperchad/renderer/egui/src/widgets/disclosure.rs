//! Persistent native disclosure state and pre-calculation traversal policy.
use super::*;
use std::collections::BTreeSet;

pub(super) struct Disclosure {
    authored_open: bool,
    pub(super) open: bool,
}

impl<F, R, C> WidgetPainter<F, R, C> {
    pub(super) fn reconcile_disclosures(&mut self, document: &Document) {
        self.disclosure.retain(|key, _| {
            document
                .get(*key)
                .is_some_and(|node| matches!(node.element, Element::Details { .. }))
        });
        self.summary_owners.clear();
        for key in document.keys() {
            let Some(Container {
                element: Element::Details { open },
                ..
            }) = document.get(key)
            else {
                continue;
            };
            let authored_open = open.unwrap_or(false);
            let state = self.disclosure.entry(key).or_insert(Disclosure {
                authored_open,
                open: authored_open,
            });
            // Unrelated content mutations must not discard native toggle state.
            if state.authored_open != authored_open {
                state.authored_open = authored_open;
                state.open = authored_open;
            }
            if let Some(summary) = document.children(key).into_iter().find(|child| {
                document
                    .get(*child)
                    .is_some_and(|node| matches!(node.element, Element::Summary))
            }) {
                self.summary_owners.insert(summary, key);
            }
        }
        self.refresh_disclosure_policy(document);
    }

    fn refresh_disclosure_policy(&mut self, document: &Document) {
        fn suppress(document: &Document, key: ElementKey, output: &mut BTreeSet<ElementKey>) {
            output.insert(key);
            for child in document.children(key) {
                suppress(document, child, output);
            }
        }
        self.suppressed.clear();
        for (&key, state) in &self.disclosure {
            if state.open {
                continue;
            }
            for child in document.children(key) {
                if self.summary_owners.get(&child) != Some(&key) {
                    suppress(document, child, &mut self.suppressed);
                }
            }
        }
    }

    /// Whether a node participates in native paint, hit testing and accessibility.
    /// Native edit/select content belongs exclusively to its owning control.
    #[must_use]
    pub fn allows_node(&self, key: ElementKey) -> bool {
        !self.native_children.contains(&key) && !self.suppressed.contains(&key)
    }

    /// Apply native disclosure visibility to fresh document-order calculated clones.
    /// Must run before shared calculation, never on authored document storage.
    /// Native control descendants remain in clones for control measurements but
    /// are excluded from independent painting via `allows_node`.
    ///
    /// # Errors
    /// * Returns an error when the clone forest no longer matches document identity.
    pub fn prepare_layout(
        &mut self,
        document: &Document,
        roots: &mut [Container],
    ) -> Result<(), String> {
        self.reconcile_disclosures(document);
        fn apply(
            document: &Document,
            keys: &[ElementKey],
            nodes: &mut [Container],
            summaries: &std::collections::BTreeMap<ElementKey, ElementKey>,
            suppressed: &BTreeSet<ElementKey>,
        ) -> Result<(), String> {
            if keys.len() != nodes.len() {
                return Err("widget layout forest differs from document".to_owned());
            }
            for (&key, node) in keys.iter().zip(nodes) {
                if suppressed.contains(&key) {
                    node.hidden = Some(true);
                }
                if summaries.contains_key(&key) && !suppressed.contains(&key) {
                    use hyperchad_transformer::{Calculation, Number};
                    let existing = node.padding_left.take().unwrap_or(Number::Integer(0));
                    node.padding_left = Some(Number::Calc(Calculation::Add(
                        Box::new(Calculation::Number(Box::new(existing))),
                        Box::new(Calculation::Number(Box::new(Number::Integer(16)))),
                    )));
                }
                if matches!(node.element, Element::Details { .. })
                    && !node
                        .children
                        .iter()
                        .any(|child| matches!(child.element, Element::Summary))
                {
                    // Reserve the native fallback summary line in shared layout;
                    // never inject synthetic document identities or egui child layout.
                    use hyperchad_transformer::{Calculation, Number};
                    let existing = node.padding_top.take().unwrap_or(Number::Integer(0));
                    node.padding_top = Some(Number::Calc(Calculation::Add(
                        Box::new(Calculation::Number(Box::new(existing))),
                        Box::new(Calculation::Number(Box::new(Number::Integer(24)))),
                    )));
                }
                apply(
                    document,
                    &document.children(key),
                    &mut node.children,
                    summaries,
                    suppressed,
                )?;
            }
            Ok(())
        }
        apply(
            document,
            &document.root_keys(),
            roots,
            &self.summary_owners,
            &self.suppressed,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::ViewOrigin;

    #[test]
    fn fallback_summary_reserves_shared_layout_space_without_synthetic_children() {
        let root = Container {
            element: Element::Details { open: Some(true) },
            children: vec![Container::default()],
            ..Default::default()
        };
        let mut doc = Document::new();
        doc.apply_view(root.into(), ViewOrigin::Navigation).unwrap();
        let mut painter = WidgetPainter::new((), (), ());
        let mut roots = doc.roots().to_vec();
        painter.prepare_layout(&doc, &mut roots).unwrap();
        assert_eq!(roots[0].children.len(), 1);
        assert_eq!(
            roots[0]
                .padding_top
                .as_ref()
                .unwrap()
                .calc(100.0, 800.0, 600.0),
            24.0
        );
        assert!(doc.roots()[0].padding_top.is_none());
    }

    #[test]
    fn closed_details_keeps_only_first_summary_and_native_toggle_survives_reconcile() {
        let summary = || Container {
            element: Element::Summary,
            ..Default::default()
        };
        let root = Container {
            element: Element::Details { open: None },
            children: vec![summary(), Container::default(), summary()],
            ..Default::default()
        };
        let mut doc = Document::new();
        doc.apply_view(root.into(), ViewOrigin::Navigation).unwrap();
        let key = doc.root_keys()[0];
        let children = doc.children(key);
        let mut painter = WidgetPainter::new((), (), ());
        let mut roots = doc.roots().to_vec();
        painter.prepare_layout(&doc, &mut roots).unwrap();
        assert!(painter.allows_node(children[0]));
        assert!(!painter.allows_node(children[1]));
        assert!(!painter.allows_node(children[2]));
        assert_eq!(roots[0].children[1].hidden, Some(true));
        assert_ne!(doc.get(children[1]).unwrap().hidden, Some(true));
        painter.disclosure.get_mut(&key).unwrap().open = true;
        let mut fresh = doc.roots().to_vec();
        painter.prepare_layout(&doc, &mut fresh).unwrap();
        assert!(painter.allows_node(children[1]));
        assert_ne!(fresh[0].children[1].hidden, Some(true));
    }
}
