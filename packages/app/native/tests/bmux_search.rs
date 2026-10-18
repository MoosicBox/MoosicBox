//! Search modal actions must dispatch against the real app view without crashing.
#![cfg(feature = "bmux")]

use hyperchad::{
    renderer_bmux::actions::{ActionEngine, Effect, EventContext, EventKind, FormValues, Source},
    transformer::Container,
};

fn path_to(root: &Container, id: &str) -> Option<Vec<usize>> {
    if root.str_id.as_deref() == Some(id) {
        return Some(Vec::new());
    }
    root.children.iter().enumerate().find_map(|(index, child)| {
        let mut path = path_to(child, id)?;
        path.insert(0, index);
        Some(path)
    })
}

fn at_path<'a>(mut root: &'a Container, path: &[usize]) -> &'a Container {
    for index in path {
        root = &root.children[*index];
    }
    root
}

#[test]
fn search_button_opens_modal_focuses_and_selects_input() {
    let mut root: Container = moosicbox_app_native_ui::search::search(
        &moosicbox_app_native_ui::state::State::default(),
        &[],
        false,
        false,
    )
    .into();
    let source = Source::at(&root, path_to(&root, "search-button").unwrap(), 0).unwrap();
    let event = EventContext::new(source, EventKind::Click, 0);
    let mut engine = ActionEngine::default();
    let effects = engine
        .dispatch(&mut root, &event, &FormValues::new())
        .unwrap();
    use hyperchad::transformer::models::Visibility;
    for (id, visibility) in [
        ("search", Visibility::Visible),
        ("search-button", Visibility::Hidden),
    ] {
        assert_eq!(
            at_path(&root, &path_to(&root, id).unwrap()).visibility,
            Some(visibility),
        );
    }
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Focus { source, focused: true } if source.key == "search-input"
    )));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Select { source } if source.key == "search-input"
    )));
}
