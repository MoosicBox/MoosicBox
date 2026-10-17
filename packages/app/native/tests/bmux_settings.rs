//! Saved connection forms must have unique, stable control identities.
#![cfg(feature = "bmux")]

use std::collections::{BTreeMap, BTreeSet};

use hyperchad::{
    renderer_bmux::state::ViewState,
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
