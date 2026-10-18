//! Actual Albums geometry must not drift as mixed-size artwork completes.
#![cfg(feature = "bmux")]

use bmux_tui::component::{Constraints, LogicalSize};
use hyperchad::{
    renderer_bmux::{
        controls::Controls,
        layout::{self, LayoutOptions, LeafMeasure},
    },
    transformer::{Container, Element},
};
use std::collections::BTreeMap;

#[test]
fn explicit_album_and_shell_geometry_is_independent_of_decoded_pixel_sizes() {
    let state = moosicbox_app_native_ui::state::State::default();
    let root: Container = moosicbox_app_native_ui::albums::albums(
        &state,
        &[],
        moosicbox_music_models::AlbumSort::NameAsc,
        None,
    )
    .into();
    let mut controls = Controls::new();
    controls.reconcile(&root);
    let measure = controls.measurer(&root);
    let options = LayoutOptions {
        viewport: bmux_tui::geometry::Size::new(140, 60),
        ..Default::default()
    };
    let before = layout::layout(&root, &options, &measure).unwrap();
    let mut decoded = BTreeMap::new();
    for node in &before.nodes {
        let mut source = &root;
        for index in &node.path {
            source = &source.children[*index];
        }
        if matches!(source.element, Element::Image { .. } | Element::Canvas) {
            let index = decoded.len();
            decoded.insert(
                std::ptr::from_ref(source) as usize,
                match index % 3 {
                    0 => (512_u32, 512_u32),
                    1 => (1200, 300),
                    _ => (80, 640),
                },
            );
            let media = |node: &Container, constraints: Constraints| {
                if let Some((width, height)) = decoded.get(&(std::ptr::from_ref(node) as usize)) {
                    return LogicalSize::new(
                        u64::from(width.div_ceil(8)),
                        u64::from(height.div_ceil(16)),
                    );
                }
                measure.measure(node, constraints)
            };
            let after = layout::layout(&root, &options, &media).unwrap();
            for (old, new) in before.nodes.iter().zip(&after.nodes) {
                assert_eq!(
                    old.bounds,
                    new.bounds,
                    "bounds drift for {} after {} decodes",
                    old.key,
                    decoded.len()
                );
                assert_eq!(
                    old.content_extents,
                    new.content_extents,
                    "extents drift for {} after {} decodes",
                    old.key,
                    decoded.len()
                );
            }
        }
    }
    assert!(decoded.len() >= 100);
}
