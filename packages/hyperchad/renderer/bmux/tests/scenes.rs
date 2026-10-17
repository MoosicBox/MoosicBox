//! Deterministic canonical-tree/layout acceptance; no alternate renderer implementation.

#[path = "../examples/support/mod.rs"]
mod support;

use bmux_tui::geometry::Size;
use hyperchad_renderer::View;
use hyperchad_renderer_bmux::{
    layout::{DefaultLeafMeasure, LayoutOptions, layout},
    state::ViewState,
};
use hyperchad_transformer::{Container, Element};
use support::{Workflow, document};

fn options(width: u16, height: u16) -> LayoutOptions {
    LayoutOptions {
        viewport: Size::new(width, height),
        ..LayoutOptions::default()
    }
}
fn count(root: &Container, predicate: fn(&Element) -> bool) -> usize {
    usize::from(predicate(&root.element))
        + root
            .children
            .iter()
            .map(|child| count(child, predicate))
            .sum::<usize>()
}

#[test]
fn all_three_workflows_have_real_controls_media_and_spanning_rich_cells() {
    for workflow in [
        Workflow::Dashboard,
        Workflow::Documents,
        Workflow::MusicLibrary,
    ] {
        let root = document(workflow);
        let state = ViewState::from_root(root).expect("unique canonical identities");
        assert!(count(state.root(), |e| matches!(e, Element::Input { .. })) >= 2);
        assert!(count(state.root(), |e| matches!(e, Element::Anchor { .. })) >= 4);
        assert_eq!(
            count(state.root(), |e| matches!(e, Element::Image { .. })),
            1
        );
        assert_eq!(count(state.root(), |e| matches!(e, Element::Canvas)), 1);
        assert_eq!(
            count(state.root(), |e| matches!(e, Element::Textarea { .. })),
            1
        );
        assert_eq!(
            count(state.root(), |e| matches!(e, Element::Select { .. })),
            1
        );
        assert!(
            count(state.root(), |e| matches!(
                e,
                Element::TD {
                    columns: Some(_),
                    ..
                } | Element::TH {
                    columns: Some(_),
                    ..
                }
            )) >= 2
        );
        for (width, height) in [(120, 48), (80, 24), (40, 16)] {
            let scene = layout(state.root(), &options(width, height), &DefaultLeafMeasure)
                .expect("production layout");
            assert_eq!(scene.viewport, Size::new(width, height));
            assert!(scene.get("search").is_some());
            assert!(scene.get("library-table").is_some());
            assert!(scene.get("track-span").is_some());
            for node in &scene.nodes {
                let clip = node.clip.terminal();
                assert!(u32::from(clip.x) + u32::from(clip.width) <= u32::from(width));
                assert!(u32::from(clip.y) + u32::from(clip.height) <= u32::from(height));
            }
        }
    }
}

#[test]
fn unrelated_fragment_preserves_canonical_editor_and_stable_geometry_identity() {
    for workflow in [
        Workflow::Dashboard,
        Workflow::Documents,
        Workflow::MusicLibrary,
    ] {
        let mut state = ViewState::from_root(document(workflow)).unwrap();
        // Canonical state application is tested here; native key dispatch is tested
        // separately through BmuxProgram, rather than simulated by these edits.
        let editor = state.find_node_mut("notes").unwrap();
        let Element::Textarea { value, .. } = &mut editor.element else {
            panic!("textarea");
        };
        *value = "Unsaved local edit 星".into();
        let before = layout(state.root(), &options(80, 24), &DefaultLeafMeasure).unwrap();
        state
            .apply_view(
                View::builder()
                    .with_fragment(support::text(50, "status", "Updated in background"))
                    .build(),
                None,
            )
            .unwrap();
        let Element::Textarea { value, .. } = &state.find_node("notes").unwrap().element else {
            panic!("textarea");
        };
        assert_eq!(value, "Unsaved local edit 星");
        for size in [(40, 16), (120, 48), (80, 24)] {
            let after =
                layout(state.root(), &options(size.0, size.1), &DefaultLeafMeasure).unwrap();
            assert_eq!(
                before.get("notes").unwrap().key,
                after.get("notes").unwrap().key
            );
            assert!(after.get("status").is_some());
        }
    }
}

#[test]
fn hostile_raw_text_is_not_terminal_control_data_in_layout_fragments() {
    for raw in [false, true] {
        let payload = "unsafe\u{1b}]52;c;c2VjcmV0\u{7}\u{1b}[2J\u{9b}31m\u{90}q\u{9c} safe 星";
        let root = support::node(
            1,
            "hostile",
            if raw {
                Element::Raw {
                    value: payload.into(),
                }
            } else {
                Element::Text {
                    value: payload.into(),
                }
            },
            vec![],
        );
        let scene = layout(&root, &options(100, 10), &DefaultLeafMeasure).unwrap();
        let text: String = scene
            .nodes
            .iter()
            .flat_map(|n| &n.text)
            .map(|f| f.text.as_str())
            .collect();
        assert!(!text.is_empty());
        assert!(!text.chars().any(char::is_control), "raw={raw}: {text:?}");
        assert!(text.contains("safe"));
        assert!(text.contains('星'));
    }
}
