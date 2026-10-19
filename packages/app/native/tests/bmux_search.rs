//! Search modal actions and album filter editing must survive real app updates.
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

#[test]
fn album_filter_retains_text_focus_and_editing_across_grid_updates() {
    use bmux_tui::event::Event;
    use hyperchad::{
        renderer::View,
        renderer_bmux::{
            controls::{ControlValue, Controls},
            layout::{self, LayoutOptions},
            state::ViewState,
        },
        transformer::{Element, Input, models::SwapStrategy},
    };

    let root: Container = moosicbox_app_native_ui::albums::albums_page_content(
        &[],
        moosicbox_music_models::AlbumSort::NameAsc,
        None,
    )
    .into();
    fn filter_path(node: &Container) -> Option<Vec<usize>> {
        if matches!(&node.element, Element::Input { input: Input::Text { placeholder, .. }, .. } if placeholder.as_deref() == Some("Filter..."))
        {
            return Some(Vec::new());
        }
        node.children.iter().enumerate().find_map(|(index, child)| {
            let mut path = filter_path(child)?;
            path.insert(0, index);
            Some(path)
        })
    }
    let filter_path = filter_path(&root).expect("album filter");
    let filter_key = layout::node_key(at_path(&root, &filter_path), &filter_path);
    let mut state = ViewState::from_root(root).unwrap();
    let mut controls = Controls::new();
    controls.reconcile(state.root());
    controls.focus(Some(filter_key.as_str()));

    for text in ["a", "b", "c"] {
        let tree = layout::layout(
            state.root(),
            &LayoutOptions::default(),
            &controls.measurer(state.root()),
        )
        .unwrap();
        controls.prepare(&tree);
        controls.event(&Event::Paste(text.into()));
        let edited = controls.value(&filter_key);

        // The real filter action replaces #albums, not the toolbar. Use the
        // actual loading fragment, then simulate the resulting grid response.
        let loading: Container = moosicbox_app_native_ui::albums::load_albums(
            96,
            moosicbox_music_models::AlbumSort::NameAsc,
            &[],
            "abc",
        )
        .into();
        let loading = at_path(&loading, &path_to(&loading, "albums").unwrap()).clone();
        let grid = Container {
            str_id: Some("albums".into()),
            ..Container::default()
        };
        for response in [loading, grid] {
            let report = state
                .apply_response(View::from(response), Some("albums"), SwapStrategy::This)
                .unwrap();
            controls.reconcile_report(state.root(), &report);
            assert_eq!(controls.value(&filter_key), edited);
            assert_eq!(controls.focused(), Some(filter_key.as_str()));
        }
    }
    assert_eq!(
        controls.value(&filter_key),
        Some(ControlValue::Text("abc".into()))
    );

    // A real external value change must still replace the local edit.
    let mut replacement = at_path(state.root(), &filter_path).clone();
    let Element::Input {
        input: Input::Text { value, .. },
        ..
    } = &mut replacement.element
    else {
        panic!("filter must be a text input");
    };
    *value = Some("server".into());
    let report = state
        .apply_response(
            View::from(replacement),
            Some(filter_key.as_str()),
            SwapStrategy::This,
        )
        .unwrap();
    controls.reconcile_report(state.root(), &report);
    assert_eq!(
        controls.value(&filter_key),
        Some(ControlValue::Text("server".into()))
    );
}
