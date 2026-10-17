use super::*;
use hyperchad_actions::{Action, Target};
fn node(id: &str) -> Container {
    Container {
        str_id: Some(id.into()),
        ..Container::default()
    }
}
fn hover() -> Action {
    Action {
        trigger: ActionTrigger::Hover,
        effect: ActionEffect {
            action: ActionType::Style {
                target: ElementTarget::SelfTarget,
                action: StyleAction::SetDisplay(false),
            },
            delay_off: Some(10),
            throttle: None,
            unique: None,
        },
    }
}
fn context(root: &Container, path: &[usize], generation: u64, time: u64) -> EventContext {
    EventContext::new(
        Source::at(root, path.to_vec(), generation).unwrap(),
        EventKind::Hover,
        time,
    )
}
#[test]
fn unrelated_generation_and_path_move_preserve_hover_baseline_and_deadline() {
    let mut root = node("root");
    let mut source = node("source");
    source.actions.push(hover());
    root.children.push(source);
    let mut engine = ActionEngine::default();
    let enter = context(&root, &[0], 0, 0);
    engine
        .dispatch(&mut root, &enter, &FormValues::new())
        .unwrap();
    let leave = context(&root, &[0], 0, 1);
    engine.release(&mut root, &leave).unwrap();
    assert_eq!(engine.next_deadline(), Some(11));
    root.children.insert(0, node("unrelated"));
    engine
        .reconcile_removed(&root, 1, &BTreeSet::new())
        .unwrap();
    assert_eq!(engine.next_deadline(), Some(11));
    engine.advance(&mut root, 11).unwrap();
    assert_eq!(root.children[1].hidden, None);
}
#[test]
fn hover_exit_after_unrelated_partial_update_still_reverses() {
    let mut root = node("root");
    root.actions.push(hover());
    let mut engine = ActionEngine::default();
    let enter = context(&root, &[], 0, 0);
    engine
        .dispatch(&mut root, &enter, &FormValues::new())
        .unwrap();
    root.children.push(node("new-fragment"));
    engine.reconcile(&root, 1).unwrap();
    let leave = context(&root, &[], 1, 2);
    engine.release(&mut root, &leave).unwrap();
    engine.advance(&mut root, 12).unwrap();
    assert_eq!(root.hidden, None);
}
#[test]
fn same_key_replacement_does_not_inherit_old_styles_or_timers() {
    let mut root = node("root");
    root.actions.push(hover());
    let mut engine = ActionEngine::default();
    let enter = context(&root, &[], 0, 0);
    engine
        .dispatch(&mut root, &enter, &FormValues::new())
        .unwrap();
    root = node("root");
    root.hidden = Some(false);
    engine
        .reconcile_removed(&root, 1, &BTreeSet::from(["root".into()]))
        .unwrap();
    engine.advance(&mut root, 100).unwrap();
    assert_eq!(root.hidden, Some(false));
    assert!(engine.next_deadline().is_none());
}
#[test]
fn removed_owner_restores_retained_external_target() {
    let mut root = node("root");
    let mut source = node("source");
    let mut action = hover();
    action.effect.action = ActionType::Style {
        target: ElementTarget::ById(Target::Literal("target".into())),
        action: StyleAction::SetDisplay(false),
    };
    source.actions.push(action);
    root.children = vec![source, node("target")];
    let mut engine = ActionEngine::default();
    let enter = context(&root, &[0], 0, 0);
    engine
        .dispatch(&mut root, &enter, &FormValues::new())
        .unwrap();
    assert_eq!(root.children[1].hidden, Some(true));
    root.children.remove(0);
    engine
        .reconcile_removed(&root, 1, &BTreeSet::from(["source".into()]))
        .unwrap();
    engine.advance(&mut root, 1).unwrap();
    assert_eq!(root.children[0].hidden, None);
}
#[test]
fn retained_event_registration_is_rebound_to_current_generation() {
    let mut root = node("root");
    root.actions.push(Action {
        trigger: ActionTrigger::Immediate,
        effect: ActionType::Event {
            name: "custom".into(),
            action: Box::new(ActionType::Custom {
                action: "registered".into(),
            }),
        }
        .into(),
    });
    let mut engine = ActionEngine::default();
    let mut event = context(&root, &[], 0, 0);
    event.kind = EventKind::Immediate;
    engine
        .dispatch(&mut root, &event, &FormValues::new())
        .unwrap();
    root.children.push(node("fragment"));
    engine.reconcile(&root, 1).unwrap();
    event = context(&root, &[], 1, 1);
    event.kind = EventKind::Named("custom".into());
    assert!(
        matches!(engine.dispatch(&mut root,&event,&FormValues::new()).unwrap().as_slice(),[Effect::Custom { source,.. }] if source.generation == 1)
    );
}
