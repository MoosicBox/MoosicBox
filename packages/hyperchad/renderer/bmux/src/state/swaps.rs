use super::tests::{node, primary, tree};
use super::*;

#[test]
fn primary_swap_strategies_preserve_correct_parent_and_order() {
    for (strategy, expected) in [
        (
            SwapStrategy::This,
            vec![node("left"), node("new"), node("right")],
        ),
        (
            SwapStrategy::BeforeBegin,
            vec![node("left"), node("new"), tree_target(), node("right")],
        ),
        (
            SwapStrategy::AfterEnd,
            vec![node("left"), tree_target(), node("new"), node("right")],
        ),
        (
            SwapStrategy::Children,
            vec![
                node("left"),
                Container {
                    children: vec![node("new")],
                    ..node("target")
                },
                node("right"),
            ],
        ),
        (
            SwapStrategy::AfterBegin,
            vec![
                node("left"),
                Container {
                    children: vec![node("new"), node("old")],
                    ..node("target")
                },
                node("right"),
            ],
        ),
        (
            SwapStrategy::BeforeEnd,
            vec![
                node("left"),
                Container {
                    children: vec![node("old"), node("new")],
                    ..node("target")
                },
                node("right"),
            ],
        ),
        (SwapStrategy::Delete, vec![node("left"), node("right")]),
        (
            SwapStrategy::None,
            vec![node("left"), tree_target(), node("right")],
        ),
    ] {
        let root = tree(vec![node("left"), tree_target(), node("right")]);
        let mut state = ViewState::from_root(root).unwrap();
        let report = state
            .apply_response(primary(node("new")), Some("target"), strategy.clone())
            .unwrap();
        assert_eq!(state.root().children, expected, "{strategy:?}");
        assert!(report.retained.contains("left"));
        assert!(report.retained.contains("right"));
        if strategy == SwapStrategy::None {
            assert_eq!(report.generation, 0);
        } else {
            assert_eq!(report.generation, 1);
        }
    }
}

fn tree_target() -> Container {
    Container {
        children: vec![node("old")],
        ..node("target")
    }
}

#[test]
fn root_children_and_edge_insertions_are_supported_without_wrapper_loss() {
    for strategy in [
        SwapStrategy::Children,
        SwapStrategy::AfterBegin,
        SwapStrategy::BeforeEnd,
    ] {
        let mut state = ViewState::from_root(tree_target()).unwrap();
        state
            .apply_response(primary(node("new")), None, strategy.clone())
            .unwrap();
        assert_eq!(state.root().str_id.as_deref(), Some("target"));
        assert!(state.find_node("new").is_some());
        assert_eq!(
            state.find_node("old").is_some(),
            strategy != SwapStrategy::Children
        );
    }
}

#[test]
fn root_sibling_insertions_reject_entire_transaction() {
    for strategy in [SwapStrategy::BeforeBegin, SwapStrategy::AfterEnd] {
        let root = tree(vec![node("child")]);
        let mut state = ViewState::from_root(root.clone()).unwrap();
        let mut view = primary(node("new"));
        view.delete_selectors.push(Selector::Id("child".into()));
        assert_eq!(
            state.apply_response(view, None, strategy).unwrap_err(),
            ApplyError::RootSiblingInsertion
        );
        assert_eq!(state.root(), &root);
        assert_eq!(state.generation(), 0);
    }
}

#[test]
fn delete_strategy_works_with_empty_response_and_safe_root() {
    let mut state = ViewState::from_root(tree(vec![node("target")])).unwrap();
    state
        .apply_response(View::default(), Some("target"), SwapStrategy::Delete)
        .unwrap();
    assert_eq!(state.root().children, [] as [Container; 0]);
    state
        .apply_response(View::default(), None, SwapStrategy::Delete)
        .unwrap();
    assert_eq!(state.root(), &Container::default());
}

#[test]
fn none_ignores_primary_but_applies_fragments_deletes_and_response_metadata() {
    let mut state = ViewState::from_root(tree(vec![node("a"), node("b")])).unwrap();
    let response = ResponseMetadata {
        cookies: vec![hyperchad_renderer::ResponseCookie::secure(
            "session", "value",
        )],
        ..ResponseMetadata::default()
    };
    let view = View {
        primary: Some(tree(vec![node("duplicate"), node("duplicate")])),
        fragments: vec![hyperchad_renderer::ReplaceContainer {
            selector: Selector::Id("a".into()),
            container: node("updated"),
        }],
        delete_selectors: vec![Selector::Id("b".into())],
        response: response.clone(),
    };
    let report = state
        .apply_response(view, None, SwapStrategy::None)
        .unwrap();
    assert_eq!(state.root().children, vec![node("updated")]);
    assert_eq!(report.response, response);
}

#[test]
fn duplicate_insertion_is_atomic_after_successful_fragment() {
    let root = tree(vec![node("a"), node("b")]);
    let mut state = ViewState::from_root(root.clone()).unwrap();
    let view = View {
        primary: Some(node("a")),
        fragments: vec![hyperchad_renderer::ReplaceContainer {
            selector: Selector::Id("b".into()),
            container: node("updated"),
        }],
        ..View::default()
    };
    assert!(matches!(
        state.apply_response(view, Some("a"), SwapStrategy::BeforeEnd),
        Err(ApplyError::DuplicateIdentity(_))
    ));
    assert_eq!(state.root(), &root);
    assert_eq!(state.generation(), 0);
}

#[test]
fn absent_primary_does_not_empty_children_or_insert_a_default_node() {
    for strategy in [
        SwapStrategy::This,
        SwapStrategy::Children,
        SwapStrategy::BeforeBegin,
        SwapStrategy::AfterBegin,
        SwapStrategy::BeforeEnd,
        SwapStrategy::AfterEnd,
        SwapStrategy::None,
    ] {
        let root = tree_target();
        let mut state = ViewState::from_root(root.clone()).unwrap();
        state
            .apply_response(View::default(), None, strategy)
            .unwrap();
        assert_eq!(state.root(), &root);
        assert_eq!(state.generation(), 0);
    }
}

#[test]
fn children_retains_target_control_and_resets_removed_children() {
    let mut state = ViewState::from_root(tree_target()).unwrap();
    let report = state
        .apply_response(primary(node("new")), Some("target"), SwapStrategy::Children)
        .unwrap();
    assert!(report.retained.contains("target"));
    assert!(report.removed.contains("old"));
    assert!(report.inserted.contains("new"));
}

#[test]
fn mutable_borrows_and_noop_transaction_do_not_invalidate_async_work() {
    let mut state = ViewState::from_root(node("root")).unwrap();
    let captured = state.generation();
    let _ = state.root_mut();
    let _ = state.find_node_mut("root");
    assert!(!state.mutate_root(|_| {}).unwrap());
    assert_eq!(state.generation(), captured);
    let report = state.apply_view(primary(node("root")), None).unwrap();
    assert_eq!(report.generation, captured);
    state
        .apply_response_if_generation(primary(node("next")), None, SwapStrategy::This, captured)
        .unwrap();
    assert_eq!(state.generation(), captured + 1);
}

#[test]
fn explicit_change_and_transactional_change_advance_generation() {
    let mut state = ViewState::from_root(node("root")).unwrap();
    state.root_mut().data.insert("changed".into(), "yes".into());
    assert_eq!(state.generation(), 0);
    assert_eq!(state.mark_changed().unwrap(), 1);
    assert!(
        state
            .mutate_root(|root| {
                root.data.clear();
            })
            .unwrap()
    );
    assert_eq!(state.generation(), 2);
    assert!(matches!(
        state.apply_response_if_generation(View::default(), None, SwapStrategy::None, 1),
        Err(ApplyError::StaleGeneration { .. })
    ));
}

#[test]
fn failed_mutation_transaction_preserves_root_and_epoch() {
    let root = tree(vec![node("child")]);
    let mut state = ViewState::from_root(root.clone()).unwrap();
    assert!(
        state
            .mutate_root(|root| root.children.push(node("child")))
            .is_err()
    );
    assert_eq!(state.root(), &root);
    assert_eq!(state.generation(), 0);
}

#[test]
fn noop_at_exhausted_epoch_is_allowed_but_actual_mutation_is_atomic() {
    let mut state = ViewState {
        root: node("root"),
        generation: u64::MAX,
    };
    state
        .apply_response(View::default(), None, SwapStrategy::None)
        .unwrap();
    assert!(!state.mutate_root(|_| {}).unwrap());
    assert!(
        state
            .mutate_root(|root| root.str_id = Some("changed".into()))
            .is_err()
    );
    assert_eq!(state.root(), &node("root"));
}
