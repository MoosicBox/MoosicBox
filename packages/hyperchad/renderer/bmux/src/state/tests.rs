use super::*;
use hyperchad_renderer::ReplaceContainer;

pub(super) fn node(id: &str) -> Container {
    Container {
        str_id: Some(id.into()),
        ..Container::default()
    }
}

pub(super) fn tree(children: Vec<Container>) -> Container {
    Container {
        children,
        ..node("root")
    }
}

pub(super) fn primary(container: Container) -> View {
    View {
        primary: Some(container),
        ..View::default()
    }
}

fn fragment(selector: Selector, container: Container) -> View {
    View {
        fragments: vec![ReplaceContainer {
            selector,
            container,
        }],
        ..View::default()
    }
}

fn text(id: &str, value: Option<&str>) -> Container {
    Container {
        element: Element::Input {
            input: Input::Text {
                value: value.map(str::to_owned),
                placeholder: None,
            },
            name: Some("query".into()),
            autofocus: None,
        },
        ..node(id)
    }
}

#[test]
fn default_root_and_full_application() {
    let mut state = ViewState::new();
    assert_eq!(state.root(), &Container::default());
    assert_eq!(state.generation(), 0);
    let report = state
        .apply_view(primary(tree(vec![node("child")])), None)
        .unwrap();
    assert_eq!(report.generation, 1);
    assert!(report.inserted.contains("child"));
    assert!(state.find_node("child").is_some());
}

#[test]
fn full_replacement_preserves_root_not_just_children() {
    let mut state = ViewState::from_root(tree(vec![node("old")])).unwrap();
    let replacement = text("new-root", Some("hello"));
    state
        .apply_view(primary(replacement.clone()), None)
        .unwrap();
    assert_eq!(state.root(), &replacement);
    assert!(state.find_node("old").is_none());
}

#[test]
fn fragments_do_not_replace_unrelated_canonical_content() {
    let mut state = ViewState::from_root(tree(vec![node("a"), node("b")])).unwrap();
    let replacement = text("a", Some("new"));
    let report = state
        .apply_view(
            fragment(Selector::Id("a".into()), replacement.clone()),
            None,
        )
        .unwrap();
    assert_eq!(state.find_node("a"), Some(&replacement));
    assert_eq!(state.find_node("b"), Some(&node("b")));
    assert!(report.retained.contains("b"));
    assert!(report.inserted.contains("a"));
    assert!(report.removed.contains("a"));
}

#[test]
fn primary_targets_trigger_slot_even_after_self_fragment_renames_it() {
    let mut state = ViewState::from_root(tree(vec![node("trigger"), node("sibling")])).unwrap();
    let view = View {
        primary: Some(node("final")),
        fragments: vec![ReplaceContainer {
            selector: Selector::SelfTarget,
            container: node("intermediate"),
        }],
        ..View::default()
    };
    state.apply_view(view, Some("trigger")).unwrap();
    assert_eq!(state.root().children[0].str_id.as_deref(), Some("final"));
    assert!(state.find_node("sibling").is_some());
}

#[test]
fn root_trigger_is_replaceable_by_key_or_id() {
    for target in ["root", "#root"] {
        let mut state = ViewState::from_root(tree(vec![])).unwrap();
        state
            .apply_view(primary(node("new")), Some(target))
            .unwrap();
        assert_eq!(state.root(), &node("new"));
    }
    let mut state = ViewState::new();
    let key = node_key(state.root(), &[]);
    state.apply_view(primary(node("new")), Some(&key)).unwrap();
    assert_eq!(state.root(), &node("new"));
}

#[test]
fn deletes_all_matching_classes_and_nested_matches_once() {
    let mut a = node("a");
    a.classes.push("gone".into());
    let mut b = node("b");
    b.classes.push("gone".into());
    a.children.push(b.clone());
    b.str_id = Some("c".into());
    let mut state = ViewState::from_root(tree(vec![a, node("keep"), b])).unwrap();
    state
        .apply_view(
            View {
                delete_selectors: vec![Selector::Class("gone".into())],
                ..View::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(state.root().children, vec![node("keep")]);
}

#[test]
fn child_class_is_relative_and_direct_only() {
    let mut direct = node("direct");
    direct.classes.push("match".into());
    let mut nested = node("nested");
    nested.classes.push("match".into());
    let trigger = Container {
        children: vec![
            direct,
            Container {
                children: vec![nested],
                ..node("wrapper")
            },
        ],
        ..node("trigger")
    };
    let mut outside = node("outside");
    outside.classes.push("match".into());
    let mut state = ViewState::from_root(tree(vec![trigger, outside])).unwrap();
    state
        .apply_view(
            View {
                delete_selectors: vec![Selector::ChildClass("match".into())],
                ..View::default()
            },
            Some("trigger"),
        )
        .unwrap();
    assert!(state.find_node("direct").is_none());
    assert!(state.find_node("nested").is_some());
    assert!(state.find_node("outside").is_some());
}

#[test]
fn deletion_rebases_trigger_path_before_primary() {
    let mut state = ViewState::from_root(tree(vec![node("first"), node("trigger")])).unwrap();
    state
        .apply_view(
            View {
                primary: Some(node("result")),
                delete_selectors: vec![Selector::Id("first".into())],
                ..View::default()
            },
            Some("trigger"),
        )
        .unwrap();
    assert_eq!(state.root().children, vec![node("result")]);
}

#[test]
fn deleted_trigger_does_not_redirect_primary_to_neighbor() {
    let root = tree(vec![node("trigger"), node("neighbor")]);
    let mut state = ViewState::from_root(root.clone()).unwrap();
    assert!(
        state
            .apply_view(
                View {
                    primary: Some(node("result")),
                    delete_selectors: vec![Selector::SelfTarget],
                    ..View::default()
                },
                Some("trigger")
            )
            .is_err()
    );
    assert_eq!(state.root(), &root);
    assert_eq!(state.generation(), 0);
}

#[test]
fn root_deletion_restores_safe_default() {
    let mut state = ViewState::from_root(tree(vec![node("child")])).unwrap();
    let report = state
        .apply_view(
            View {
                delete_selectors: vec![Selector::SelfTarget],
                ..View::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(state.root(), &Container::default());
    assert!(report.removed.contains("child"));
}

#[test]
fn missing_selectors_reject_without_dropping_updates() {
    for selector in [
        Selector::Id("missing".into()),
        Selector::Class("missing".into()),
        Selector::ChildClass("missing".into()),
    ] {
        let root = tree(vec![node("a")]);
        let mut state = ViewState::from_root(root.clone()).unwrap();
        assert!(matches!(
            state.apply_view(fragment(selector, node("b")), None),
            Err(ApplyError::MissingTarget(_))
        ));
        assert_eq!(state.root(), &root);
        assert_eq!(state.generation(), 0);
    }
}

#[test]
fn missing_explicit_trigger_is_not_a_full_replacement() {
    let mut state = ViewState::new();
    assert!(
        state
            .apply_view(primary(node("unexpected")), Some("absent"))
            .is_err()
    );
    assert_eq!(state.root(), &Container::default());
}

#[test]
fn duplicate_string_numeric_and_cross_domain_layout_keys_rejected() {
    assert!(matches!(
        ViewState::from_root(tree(vec![node("dup"), node("dup")])),
        Err(ApplyError::DuplicateIdentity(_))
    ));
    let numeric = Container {
        id: 12,
        ..Container::default()
    };
    assert!(ViewState::from_root(tree(vec![numeric.clone(), numeric.clone()])).is_err());
    assert!(ViewState::from_root(tree(vec![numeric, node("12")])).is_err());
    // Zero is the normal unassigned ID, not an identity.
    assert!(ViewState::from_root(tree(vec![Container::default(), Container::default()])).is_ok());
}

#[test]
fn failed_late_operation_rolls_back_earlier_fragments() {
    let root = tree(vec![node("a"), node("b")]);
    let mut state = ViewState::from_root(root.clone()).unwrap();
    let view = View {
        fragments: vec![
            ReplaceContainer {
                selector: Selector::Id("a".into()),
                container: node("updated"),
            },
            ReplaceContainer {
                selector: Selector::Id("b".into()),
                container: node("updated"),
            },
        ],
        ..View::default()
    };
    assert!(matches!(
        state.apply_view(view, None),
        Err(ApplyError::DuplicateIdentity(_))
    ));
    assert_eq!(state.root(), &root);
    assert_eq!(state.generation(), 0);
}

#[test]
fn invalid_existing_tree_is_checked_before_application() {
    let mut state = ViewState::new();
    state.root_mut().children = vec![node("dup"), node("dup")];
    let generation = state.generation();
    assert!(state.apply_view(primary(node("valid")), None).is_err());
    assert_eq!(state.root().children.len(), 2);
    assert_eq!(state.generation(), generation);
}

#[test]
fn keyed_reorder_retains_but_anonymous_reorder_resets() {
    let a = text("a", Some("A"));
    let b = text("b", Some("B"));
    let mut state = ViewState::from_root(tree(vec![a.clone(), b.clone()])).unwrap();
    let report = state
        .apply_view(primary(tree(vec![b.clone(), a.clone()])), None)
        .unwrap();
    assert!(report.retained.contains("a"));
    assert!(report.retained.contains("b"));
    let mut a = a;
    let mut b = b;
    a.str_id = None;
    b.str_id = None;
    let first = node_key(&a, &[0]);
    let second = node_key(&b, &[1]);
    let mut state = ViewState::from_root(tree(vec![a.clone(), b.clone()])).unwrap();
    let report = state.apply_view(primary(tree(vec![b, a])), None).unwrap();
    for key in [first, second] {
        assert!(!report.retained.contains(&key));
        assert!(report.inserted.contains(&key));
        assert!(report.removed.contains(&key));
    }
}

#[test]
fn identical_anonymous_replacements_are_ambiguous_and_reset() {
    let mut anonymous = text("temporary", None);
    anonymous.str_id = None;
    let root = tree(vec![anonymous.clone(), anonymous]);
    let key = node_key(&root.children[0], &[0]);
    let mut state = ViewState::from_root(root.clone()).unwrap();
    let report = state.apply_view(primary(root), None).unwrap();
    assert!(!report.retained.contains(&key));
}

#[test]
fn changed_numeric_identity_cannot_reuse_string_identity_state() {
    let before = text("12", None);
    let mut after = before.clone();
    after.str_id = None;
    after.id = 12;
    let mut state = ViewState::from_root(before).unwrap();
    let report = state.apply_view(primary(after), None).unwrap();
    assert!(report.removed.contains("12"));
    assert!(report.inserted.contains("12"));
    assert!(!report.retained.contains("12"));
}

#[test]
fn controlled_text_reconciles_only_when_external_value_changes() {
    let mut state = ViewState::from_root(text("input", Some("initial"))).unwrap();
    let report = state
        .apply_view(primary(text("input", Some("initial"))), None)
        .unwrap();
    assert!(report.controls[0].compatible);
    assert_eq!(report.controls[0].external_value, None);
    let report = state
        .apply_view(primary(text("input", Some(""))), None)
        .unwrap();
    assert_eq!(
        report.controls[0].external_value,
        Some(ControlValue::Text(String::new()))
    );
    let report = state
        .apply_view(primary(text("input", None)), None)
        .unwrap();
    assert!(report.controls[0].compatible);
    assert_eq!(report.controls[0].current.value, None);
    assert_eq!(report.controls[0].external_value, None);
}

#[test]
fn control_kind_and_name_changes_reset_but_placeholder_changes_do_not() {
    let mut state = ViewState::from_root(text("input", Some("old"))).unwrap();
    let mut updated = text("input", Some("old"));
    if let Element::Input {
        input: Input::Text { placeholder, .. },
        ..
    } = &mut updated.element
    {
        *placeholder = Some("new placeholder".into());
    }
    let report = state.apply_view(primary(updated.clone()), None).unwrap();
    assert!(report.controls[0].compatible);
    if let Element::Input { name, .. } = &mut updated.element {
        *name = Some("different".into());
    }
    let report = state.apply_view(primary(updated), None).unwrap();
    assert!(!report.controls[0].compatible);
    assert_eq!(
        report.controls[0].external_value,
        Some(ControlValue::Text("old".into()))
    );
    let checkbox = Container {
        element: Element::Input {
            input: Input::Checkbox {
                checked: Some(false),
            },
            name: None,
            autofocus: None,
        },
        ..node("input")
    };
    let report = state.apply_view(primary(checkbox), None).unwrap();
    assert!(!report.controls[0].compatible);
    assert_eq!(
        report.controls[0].external_value,
        Some(ControlValue::Bool(false))
    );
}

#[test]
fn source_and_string_id_updates_are_not_lost() {
    let mut before = node("image");
    before.id = 42;
    let mut after = before.clone();
    after.str_id = Some("renamed".into());
    after.data.insert("source".into(), "fresh".into());
    after.actions.push(hyperchad_actions::Action {
        trigger: hyperchad_actions::ActionTrigger::Click,
        effect: hyperchad_actions::ActionEffect::default(),
    });
    let mut state = ViewState::from_root(before).unwrap();
    let report = state.apply_view(primary(after.clone()), None).unwrap();
    assert_eq!(state.root(), &after);
    assert!(report.removed.contains("image"));
    assert!(report.inserted.contains("renamed"));
}

#[test]
fn metadata_only_response_is_preserved() {
    let mut state = ViewState::from_root(node("root")).unwrap();
    let response = ResponseMetadata {
        cookies: vec![hyperchad_renderer::ResponseCookie::secure(
            "session", "value",
        )],
        ..ResponseMetadata::default()
    };
    let report = state
        .apply_view(
            View {
                response: response.clone(),
                ..View::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(report.response, response);
    assert!(report.retained.contains("root"));
    assert_eq!(report.generation, 0);
}

#[test]
fn stale_async_response_is_atomic_and_explicit_mutation_invalidates_it() {
    let mut state = ViewState::from_root(node("root")).unwrap();
    let generation = state.generation();
    state.mark_changed().unwrap();
    assert!(matches!(
        state.apply_view_if_generation(primary(node("stale")), None, generation),
        Err(ApplyError::StaleGeneration { .. })
    ));
    assert_eq!(state.root(), &node("root"));
    let generation = state.generation();
    assert!(state.find_node_mut("missing").is_none());
    assert!(state.is_current_generation(generation));
    state
        .find_node_mut("root")
        .unwrap()
        .data
        .insert("local".into(), "change".into());
    assert!(state.is_current_generation(generation));
    state.mark_changed().unwrap();
    assert!(!state.is_current_generation(generation));
}

#[test]
fn exhausted_generation_does_not_mutate() {
    let mut state = ViewState {
        root: node("root"),
        generation: u64::MAX,
    };
    assert_eq!(
        state.apply_view(primary(node("new")), None).unwrap_err(),
        ApplyError::GenerationExhausted
    );
    assert_eq!(state.root(), &node("root"));
}

#[test]
fn class_replacements_update_every_matching_slot() {
    let mut a = node("a");
    let mut b = node("b");
    a.classes.push("replace".into());
    b.classes.push("replace".into());
    let mut state = ViewState::from_root(tree(vec![a, node("keep"), b])).unwrap();
    let replacement = Container {
        element: Element::Text {
            value: "replacement".into(),
        },
        ..Container::default()
    };
    state
        .apply_view(
            fragment(Selector::Class("replace".into()), replacement.clone()),
            None,
        )
        .unwrap();
    assert_eq!(
        state.root().children,
        vec![replacement.clone(), node("keep"), replacement]
    );
}

#[test]
fn class_replacement_with_repeated_identity_is_rejected_atomically() {
    let mut a = node("a");
    let mut b = node("b");
    a.classes.push("replace".into());
    b.classes.push("replace".into());
    let root = tree(vec![a, b]);
    let mut state = ViewState::from_root(root.clone()).unwrap();
    assert!(matches!(
        state.apply_view(
            fragment(Selector::Class("replace".into()), node("duplicate")),
            None
        ),
        Err(ApplyError::DuplicateIdentity(_))
    ));
    assert_eq!(state.root(), &root);
}

#[test]
fn canonical_image_source_is_updated_even_with_retained_identity() {
    let image = |source: &str| Container {
        element: Element::Image {
            source: Some(source.into()),
            alt: None,
            fit: None,
            source_set: None,
            sizes: None,
            loading: None,
        },
        ..node("image")
    };
    let mut state = ViewState::from_root(image("old.png")).unwrap();
    let next = image("new.png");
    let report = state.apply_view(primary(next.clone()), None).unwrap();
    assert_eq!(state.root(), &next);
    assert!(report.retained.contains("image"));
}

#[test]
fn anonymous_noop_retains_but_anonymous_identical_fragment_resets() {
    let anonymous = Container::default();
    let root = tree(vec![anonymous.clone()]);
    let key = node_key(&anonymous, &[0]);
    let mut state = ViewState::from_root(root).unwrap();
    assert!(
        state
            .apply_view(View::default(), None)
            .unwrap()
            .retained
            .contains(&key)
    );
    let report = state
        .apply_view(fragment(Selector::SelfTarget, anonymous), Some(&key))
        .unwrap();
    assert!(!report.retained.contains(&key));
    assert!(report.removed.contains(&key));
    assert!(report.inserted.contains(&key));
}
