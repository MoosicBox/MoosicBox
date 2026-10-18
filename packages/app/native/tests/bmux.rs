//! Terminal acceptance using the real native application view and routes.
#![cfg(feature = "bmux")]

#[test]
fn home_route_and_immediate_visualization_work_before_first_frame() {
    use hyperchad::{
        renderer::{Renderer, View},
        renderer_bmux::{
            BmuxRenderer,
            actions::{ActionEngine, Effect, EventContext, EventKind, Geometry, Source},
            layout::{DefaultLeafMeasure, LayoutOptions, layout},
        },
    };

    let router = moosicbox_app_native::init();
    assert!(router.get_route_func("/").is_some());
    assert!(router.get_route_func("/home").is_some());
    let view = View::from(moosicbox_app_native_ui::home(
        &moosicbox_app_native_ui::state::State::default(),
    ));
    let mut root = view.primary.as_ref().unwrap().clone();
    let tree = layout(&root, &LayoutOptions::default(), &DefaultLeafMeasure).unwrap();
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

    // Inspect the actual semantic effect, not aggregate command counts: the
    // shell also owns modal load requests and decorative image work.
    let canvas = tree
        .get("visualization")
        .expect("real visualization canvas");
    let source = Source::at(&root, canvas.path.clone(), 0).unwrap();
    let mut event = EventContext::new(source, EventKind::Immediate, 0);
    event.geometry.insert(
        "visualization".into(),
        Geometry {
            width: 320.0,
            height: 16.0,
            ..Geometry::default()
        },
    );
    let effects = ActionEngine::default()
        .dispatch(&mut root, &event, &Default::default())
        .unwrap();
    let custom: Vec<_> = effects
        .iter()
        .filter_map(|effect| {
            if let Effect::Custom {
                action,
                value,
                source,
            } = effect
            {
                Some((action, value, source))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(custom.len(), 1, "exactly one visualization custom effect");
    assert!(matches!(
        serde_json::from_str::<moosicbox_app_native_ui::Action>(custom[0].0).unwrap(),
        moosicbox_app_native_ui::Action::RefreshVisualization
    ));
    assert_eq!(
        custom[0].1.as_ref(),
        Some(&hyperchad::actions::logic::Value::Real(320.0))
    );
    assert_eq!(custom[0].2.key, "visualization");

    let (actions, _action_rx) = flume::bounded(64);
    let (resizes, _resize_rx) = flume::bounded(64);
    let renderer = BmuxRenderer::new(Default::default(), actions, resizes);
    let runtime = switchy::unsync::runtime::Builder::new().build().unwrap();
    runtime.block_on(renderer.render(view)).unwrap();
    let mut surface = renderer.into_embedded().unwrap();
    // Exercise the full real view lifecycle as well: missing geometry must fail
    // here, regardless of how many resource/modal commands accompany the effect.
    let update = surface.drain().unwrap();
    assert_eq!(update.lifecycle, bmux_tui_runtime::Lifecycle::Continue);
    surface.prepare(tree.viewport).unwrap();
}
