//! Terminal acceptance using the real native application view and routes.
#![cfg(feature = "bmux")]

#[test]
fn home_route_and_immediate_visualization_work_before_first_frame() {
    use hyperchad::{
        renderer::{Renderer, View},
        renderer_bmux::{
            BmuxRenderer,
            layout::{DefaultLeafMeasure, LayoutOptions, layout},
        },
    };

    // The no-default-features bmux build must retain the static home handler.
    let router = moosicbox_app_native::init();
    assert!(router.get_route_func("/").is_some());
    assert!(router.get_route_func("/home").is_some());

    let view = View::from(moosicbox_app_native_ui::home(
        &moosicbox_app_native_ui::state::State::default(),
    ));
    let tree = layout(
        view.primary.as_ref().unwrap(),
        &LayoutOptions::default(),
        &DefaultLeafMeasure,
    )
    .unwrap();
    let visible_text: String = tree
        .nodes
        .iter()
        .filter(|node| node.visible)
        .flat_map(|node| {
            node.text.iter().filter(|fragment| {
                fragment.visible && fragment.bounds.intersection(node.text_clip).width > 0
            })
        })
        .map(|fragment| fragment.text.as_str())
        .collect();
    assert!(visible_text.contains("Configure a server connection in Settings"));

    let (actions, _action_rx) = flume::bounded(64);
    let (resizes, _resize_rx) = flume::bounded(64);
    let renderer = BmuxRenderer::new(Default::default(), actions, resizes);
    let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
    runtime.block_on(renderer.render(view)).unwrap();
    let mut surface = renderer.into_embedded().unwrap();
    // This previously failed with Missing("layout x") in fx-immediate.
    let update = surface.drain().unwrap();
    assert_eq!(update.commands.len(), 1);
    surface.prepare(tree.viewport).unwrap();
}
