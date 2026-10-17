use super::eval::Datum;
use super::*;
use hyperchad_actions::{
    Target,
    dsl::{Expression, Literal},
};
fn node(id: &str, numeric: usize) -> Container {
    Container {
        str_id: Some(id.into()),
        id: numeric,
        ..Container::default()
    }
}
#[test]
fn every_target_variant_has_a_direct_assertion() {
    let mut root = node("root", 1);
    root.children = vec![node("a", 2), node("b", 3)];
    root.children[0].classes.push("item".into());
    root.children[1].classes.push("item".into());
    let source = Source::at(&root, vec![], 0).unwrap();
    let context = EventContext::new(source.clone(), EventKind::Click, 0);
    let values = FormValues::new();
    let mut engine = ActionEngine::default();
    let mut work = Work::new(&root, &context, &values, &mut engine).unwrap();
    for (target, expected) in [
        (ElementTarget::SelfTarget, vec![vec![]]),
        (ElementTarget::LastChild, vec![vec![1]]),
        (ElementTarget::Id(2), vec![vec![0]]),
        (
            ElementTarget::ById(Target::Literal("b".into())),
            vec![vec![1]],
        ),
        (
            ElementTarget::Selector(Target::Literal("#a, #b".into())),
            vec![vec![0], vec![1]],
        ),
        (
            ElementTarget::Class(Target::Literal("item".into())),
            vec![vec![0], vec![1]],
        ),
        (
            ElementTarget::ChildClass(Target::Literal("item".into())),
            vec![vec![0], vec![1]],
        ),
    ] {
        assert_eq!(work.targets(&target, &source, 0).unwrap(), expected);
    }
}
#[test]
fn all_element_mutator_methods_have_direct_assertions() {
    for (method, args) in [
        ("show", vec![]),
        ("hide", vec![]),
        ("focus", vec![]),
        ("blur", vec![]),
        (
            "set_display",
            vec![Expression::Literal(Literal::Bool(false))],
        ),
        (
            "set_background",
            vec![Expression::Literal(Literal::String("#112233".into()))],
        ),
        ("remove_background", vec![]),
        (
            "set_visibility",
            vec![Expression::Variable("Visibility::Hidden".into())],
        ),
    ] {
        let root = node("root", 1);
        let source = Source::at(&root, vec![], 0).unwrap();
        let owner = Owner {
            source: source.clone(),
            action: vec![0],
        };
        let context = EventContext::new(source, EventKind::Click, 0);
        let values = FormValues::new();
        let mut engine = ActionEngine::default();
        let mut work = Work::new(&root, &context, &values, &mut engine).unwrap();
        let expr = Expression::MethodCall {
            receiver: Box::new(Expression::Variable("self".into())),
            method: method.into(),
            args,
        };
        assert_eq!(work.expression(&expr, &owner, 0).unwrap(), Datum::Unit);
        match method {
            "focus" | "blur" => assert!(
                matches!(work.effects.as_slice(),[Effect::Focus { focused,.. }] if *focused == (method == "focus"))
            ),
            "show" => assert_eq!(work.engine.styles["root"].effective().hidden, Some(false)),
            "hide" | "set_display" => {
                assert_eq!(work.engine.styles["root"].effective().hidden, Some(true));
            }
            "set_background" => {
                assert!(work.engine.styles["root"].effective().background.is_some());
            }
            "remove_background" => {
                assert_eq!(work.engine.styles["root"].effective().background, None);
            }
            "set_visibility" => assert_eq!(
                work.engine.styles["root"].effective().visibility,
                Some(Visibility::Hidden)
            ),
            _ => panic!("test method missing assertion"),
        }
    }
}
#[test]
fn deeply_nested_registration_is_rejected_before_clone() {
    let mut action = ActionType::NoOp;
    for _ in 0..16 {
        action = ActionType::Event {
            name: "nested".into(),
            action: Box::new(action),
        };
    }
    let limits = ActionLimits {
        max_depth: 8,
        ..ActionLimits::default()
    };
    assert!(matches!(
        super::admission::action(&action, &limits),
        Err(ActionError::Limit(_))
    ));
}
