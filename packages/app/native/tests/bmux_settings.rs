//! Saved connection forms must have unique, stable control identities.
#![cfg(feature = "bmux")]

use std::collections::{BTreeMap, BTreeSet};

use hyperchad::{
    renderer_bmux::{
        actions::{
            ActionEngine, ControlValue, Effect, EventContext, EventKind, FormValues, RequestBody,
            Source,
        },
        state::ViewState,
    },
    transformer::{Container, Element, Input},
};
use moosicbox_app_models::Connection;

fn fields(root: &Container) -> BTreeMap<String, (String, Option<String>)> {
    fn visit(root: &Container, fields: &mut BTreeMap<String, (String, Option<String>)>) {
        if let Element::Input {
            name: Some(name),
            input: Input::Text { value, .. },
            ..
        } = &root.element
        {
            let id = root.str_id.clone().expect("connection field identity");
            assert!(fields.insert(id, (name.clone(), value.clone())).is_none());
        }
        for child in &root.children {
            visit(child, fields);
        }
    }
    let mut result = BTreeMap::new();
    visit(root, &mut result);
    result
}

#[test]
fn saved_connection_fields_are_unique_and_stable_across_selection_and_reordering() {
    let mut connections: Vec<_> = ["Office", "Office-", "Office_", "音乐 & home"]
        .into_iter()
        .map(|name| Connection {
            name: name.into(),
            api_url: "http://127.0.0.1:8500".into(),
        })
        .collect();
    let root: Container =
        moosicbox_app_native_ui::settings::connections_content(&connections, Some(&connections[0]))
            .into();
    let before = fields(&root);
    assert_eq!(before.len(), connections.len() * 2);
    assert_eq!(
        before
            .values()
            .map(|(name, _)| name.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["api-url", "name"])
    );
    ViewState::from_root(root).expect("saved connections must not crash duplicate detection");

    connections.reverse();
    let reordered: Container =
        moosicbox_app_native_ui::settings::connections_content(&connections, Some(&connections[1]))
            .into();
    assert_eq!(fields(&reordered), before);
    ViewState::from_root(reordered).unwrap();

    let page: Container = moosicbox_app_native_ui::settings::settings(
        &moosicbox_app_native_ui::state::State::default(),
        "Desktop",
        &connections,
        Some(&connections[1]),
        &[],
    )
    .into();
    ViewState::from_root(page).expect("full Settings page must have unique identities");
}

fn find_path(root: &Container, predicate: &impl Fn(&Container) -> bool) -> Option<Vec<usize>> {
    if predicate(root) {
        return Some(Vec::new());
    }
    root.children.iter().enumerate().find_map(|(index, child)| {
        let mut path = find_path(child, predicate)?;
        path.insert(0, index);
        Some(path)
    })
}

fn at_path<'a>(mut root: &'a Container, path: &[usize]) -> &'a Container {
    for &index in path {
        root = &root.children[index];
    }
    root
}

// This tests serialization and deferred routing only. Never execute the request:
// the real route updates persistent application STATE.
#[test]
fn saved_connection_save_dispatch_serializes_only_its_own_form() {
    use hyperchad::transformer::models::{Route, Selector};

    let mut connections: Vec<_> = ["Office", "Office-", "Office_", "音乐 & home"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| Connection {
            name: name.into(),
            api_url: format!("http://original-{index}.invalid/api"),
        })
        .collect();
    for reverse in [false, true] {
        if reverse {
            connections.reverse();
        }
        for (selected, connection) in connections.iter().enumerate() {
            let mut root: Container = moosicbox_app_native_ui::settings::connections_content(
                &connections,
                Some(connection),
            )
            .into();
            ViewState::from_root(root.clone()).unwrap();
            let declared = fields(&root);
            assert_eq!(declared.len(), 8);
            let url = format!("/settings/connections?name={}", connection.name);
            let form_path = find_path(
                &root,
                &|node| matches!(&node.route, Some(Route::Patch { route, .. }) if route == &url),
            )
            .expect("actual connection PATCH form");
            let form = at_path(&root, &form_path);
            let own_fields = fields(form);
            assert_eq!(own_fields.len(), 2);
            let mut values: FormValues = declared
                .keys()
                .map(|key| (key.clone(), ControlValue::Text(format!("POISON-{key}"))))
                .collect();
            for (key, (name, _)) in own_fields {
                let value = match name.as_str() {
                    "name" => format!("Edited connection {selected}"),
                    "api-url" => format!("http://edited-{selected}.invalid/api"),
                    _ => panic!("unexpected connection field {name}"),
                };
                values.insert(key, ControlValue::Text(value));
            }
            let mut save_path = form_path.clone();
            save_path.extend(
                find_path(form, &|node| {
                    matches!(&node.element, Element::Button { r#type: Some(kind) } if kind == "submit")
                })
                .expect("actual Save submit button, not Select or Delete"),
            );
            let event = EventContext::new(
                Source::at(&root, save_path, 0).unwrap(),
                EventKind::Click,
                0,
            );
            let effects = ActionEngine::default()
                .dispatch(&mut root, &event, &values)
                .unwrap();
            let [Effect::Request(request)] = effects.as_slice() else {
                panic!("Save must emit exactly one deferred request: {effects:?}");
            };
            assert_eq!(request.method, "PATCH");
            assert_eq!(request.url, url);
            assert_eq!(request.target, Selector::Id("settings-connections".into()));
            assert!(!request.navigation);
            assert_eq!(
                request.body,
                RequestBody::UrlEncoded(format!(
                    "name=Edited+connection+{selected}&api-url=http%3A%2F%2Fedited-{selected}.invalid%2Fapi"
                ))
            );
            assert_eq!(fields(&root), declared, "dispatch must not mutate siblings");
        }
    }
}
