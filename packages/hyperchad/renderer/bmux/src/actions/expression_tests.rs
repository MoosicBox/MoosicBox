//! Direct assertions for every expression/statement variant, including explicit rejections.
use super::eval::Datum;
use super::*;
use hyperchad_actions::{
    dsl::*,
    logic::{CalcValue, Value},
};

fn lit(value: i64) -> Expression {
    Expression::Literal(Literal::Integer(value))
}
fn boolean(value: bool) -> Expression {
    Expression::Literal(Literal::Bool(value))
}
fn text(value: &str) -> Expression {
    Expression::Literal(Literal::String(value.into()))
}
fn var(value: &str) -> Expression {
    Expression::Variable(value.into())
}
fn binary(left: Expression, op: BinaryOp, right: Expression) -> Expression {
    Expression::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
    }
}
fn evaluate(expr: &Expression) -> Result<Datum, ActionError> {
    let mut root = Container {
        str_id: Some("root".into()),
        id: 5,
        ..Container::default()
    };
    root.data.insert("test".into(), "data".into());
    let source = Source::at(&root, vec![], 0)?;
    let owner = Owner {
        source: source.clone(),
        action: vec![0],
    };
    let mut context = EventContext::new(source, EventKind::Click, 0);
    context.value = Some("payload".into());
    context.keys.push(Key::Enter);
    context.pointer = Some((12.0, 23.0));
    context.geometry.insert(
        "root".into(),
        Geometry {
            x: 2.0,
            y: 3.0,
            width: 20.0,
            height: 30.0,
        },
    );
    context.http = Some(HttpEventContext {
        url: "/http".into(),
        method: "POST".into(),
        status: Some(201),
        headers: Some(BTreeMap::from([("x".into(), "y".into())])),
        duration_ms: Some(8),
        error: Some("problem".into()),
    });
    let mut engine = ActionEngine::default();
    let values = FormValues::new();
    Work::new(&root, &context, &values, &mut engine)?.expression(expr, &owner, 0)
}
fn block(statements: Vec<Statement>) -> Expression {
    Expression::Block(Block { statements })
}
fn arm(pattern: Pattern, body: Expression) -> MatchArm {
    MatchArm { pattern, body }
}
#[test]
#[allow(clippy::too_many_lines)] // Table-like exhaustive enum assertions remain in one fixture.
fn every_expression_variant_has_an_assertion() {
    assert_eq!(evaluate(&lit(3)).unwrap(), Datum::Integer(3));
    assert_eq!(
        evaluate(&Expression::Literal(Literal::Float(1.5))).unwrap(),
        Datum::Value(Value::Real(1.5))
    );
    assert_eq!(
        evaluate(&Expression::Literal(Literal::Unit)).unwrap(),
        Datum::Unit
    );
    assert_eq!(
        evaluate(&text("hello")).unwrap(),
        Datum::Value(Value::String("hello".into()))
    );
    assert_eq!(evaluate(&boolean(true)).unwrap(), Datum::Bool(true));
    assert_eq!(
        evaluate(&var("event_value")).unwrap(),
        Datum::Value(Value::String("payload".into()))
    );
    for expr in [
        Expression::ElementRef(Box::new(text("#root"))),
        Expression::ElementByIdRef(Box::new(text("root"))),
    ] {
        assert_eq!(evaluate(&expr).unwrap(), Datum::Element(vec![]));
    }
    assert_eq!(
        evaluate(&Expression::Grouping(Box::new(lit(4)))).unwrap(),
        Datum::Integer(4)
    );
    for expr in [
        Expression::Array(vec![lit(1), lit(2)]),
        Expression::Tuple(vec![lit(1), lit(2)]),
    ] {
        assert_eq!(
            evaluate(&expr).unwrap(),
            Datum::Array(vec![Datum::Integer(1), Datum::Integer(2)])
        );
    }
    assert_eq!(
        evaluate(&Expression::Range {
            start: None,
            end: Some(Box::new(lit(2))),
            inclusive: true
        })
        .unwrap(),
        Datum::Array(vec![
            Datum::Integer(0),
            Datum::Integer(1),
            Datum::Integer(2)
        ])
    );
    assert!(matches!(
        evaluate(&Expression::Range {
            start: None,
            end: None,
            inclusive: false
        }),
        Err(ActionError::Unsupported(_))
    ));
    assert_eq!(
        evaluate(&Expression::If {
            condition: Box::new(boolean(false)),
            then_branch: Box::new(lit(1)),
            else_branch: Some(Box::new(lit(2)))
        })
        .unwrap(),
        Datum::Integer(2)
    );
    assert_eq!(
        evaluate(&Expression::If {
            condition: Box::new(boolean(false)),
            then_branch: Box::new(lit(1)),
            else_branch: None
        })
        .unwrap(),
        Datum::Unit
    );
    assert_eq!(
        evaluate(&Expression::Match {
            expr: Box::new(lit(2)),
            arms: vec![
                arm(Pattern::Literal(Literal::Integer(1)), lit(10)),
                arm(Pattern::Variable("matched".into()), var("matched"))
            ]
        })
        .unwrap(),
        Datum::Integer(2)
    );
    assert_eq!(
        evaluate(&Expression::Match {
            expr: Box::new(lit(2)),
            arms: vec![arm(Pattern::Wildcard, lit(7))]
        })
        .unwrap(),
        Datum::Integer(7)
    );
    let variant = Pattern::Variant {
        enum_name: "Visibility".into(),
        variant: "Hidden".into(),
        fields: vec![],
    };
    assert_eq!(
        evaluate(&Expression::Match {
            expr: Box::new(var("Visibility::Hidden")),
            arms: vec![arm(variant, lit(9))]
        })
        .unwrap(),
        Datum::Integer(9)
    );
    assert_eq!(
        evaluate(&Expression::Call {
            function: "min".into(),
            args: vec![lit(3), lit(1)]
        })
        .unwrap(),
        Datum::Value(Value::Real(1.0))
    );
    assert_eq!(
        evaluate(&Expression::Call {
            function: "element_by_id".into(),
            args: vec![text("root")]
        })
        .unwrap(),
        Datum::Element(vec![])
    );
    assert_eq!(
        evaluate(&Expression::MethodCall {
            receiver: Box::new(text("hello")),
            method: "len".into(),
            args: vec![]
        })
        .unwrap(),
        Datum::Integer(5)
    );
    assert_eq!(
        evaluate(&Expression::Field {
            object: Box::new(var("self")),
            field: "width".into()
        })
        .unwrap(),
        Datum::Value(Value::Real(20.0))
    );
    assert!(matches!(
        evaluate(&Expression::Closure {
            params: vec!["x".into()],
            body: Box::new(var("x"))
        }),
        Err(ActionError::Unsupported(_))
    ));
    assert!(matches!(
        evaluate(&Expression::RawRust("code".into())),
        Err(ActionError::Unsupported(_))
    ));
    assert!(matches!(
        evaluate(&Expression::Call {
            function: "unknown".into(),
            args: vec![]
        }),
        Err(ActionError::Unsupported(_))
    ));
    assert!(matches!(
        evaluate(&Expression::MethodCall {
            receiver: Box::new(lit(1)),
            method: "unknown".into(),
            args: vec![]
        }),
        Err(ActionError::Unsupported(_))
    ));
}
#[test]
fn all_binary_and_unary_operators_are_asserted() {
    for (op, expected) in [
        (BinaryOp::Add, 8),
        (BinaryOp::Subtract, 4),
        (BinaryOp::Multiply, 12),
        (BinaryOp::Divide, 3),
        (BinaryOp::Modulo, 0),
        (BinaryOp::BitAnd, 2),
        (BinaryOp::BitOr, 6),
        (BinaryOp::BitXor, 4),
    ] {
        assert_eq!(
            evaluate(&binary(lit(6), op, lit(2))).unwrap(),
            Datum::Integer(expected)
        );
    }
    for (op, expected) in [
        (BinaryOp::Equal, false),
        (BinaryOp::NotEqual, true),
        (BinaryOp::Less, false),
        (BinaryOp::LessEqual, false),
        (BinaryOp::Greater, true),
        (BinaryOp::GreaterEqual, true),
    ] {
        assert_eq!(
            evaluate(&binary(lit(6), op, lit(2))).unwrap(),
            Datum::Bool(expected)
        );
    }
    assert_eq!(
        evaluate(&binary(boolean(true), BinaryOp::And, boolean(false))).unwrap(),
        Datum::Bool(false)
    );
    assert_eq!(
        evaluate(&binary(boolean(false), BinaryOp::Or, boolean(true))).unwrap(),
        Datum::Bool(true)
    );
    assert_eq!(
        evaluate(&binary(boolean(false), BinaryOp::And, var("undefined"))).unwrap(),
        Datum::Bool(false)
    );
    for (op, expr, expected) in [
        (UnaryOp::Not, boolean(true), Datum::Bool(false)),
        (UnaryOp::Minus, lit(3), Datum::Integer(-3)),
        (UnaryOp::Plus, lit(3), Datum::Integer(3)),
        (UnaryOp::Ref, lit(3), Datum::Integer(3)),
    ] {
        assert_eq!(
            evaluate(&Expression::Unary {
                op,
                expr: Box::new(expr)
            })
            .unwrap(),
            expected
        );
    }
    assert!(evaluate(&binary(lit(1), BinaryOp::Divide, lit(0))).is_err());
    assert!(evaluate(&binary(lit(i64::MAX), BinaryOp::Add, lit(1))).is_err());
}
#[test]
fn every_statement_variant_is_asserted() {
    assert_eq!(
        evaluate(&block(vec![
            Statement::Let {
                name: "a".into(),
                value: lit(4)
            },
            Statement::Expression(var("a"))
        ]))
        .unwrap(),
        Datum::Integer(4)
    );
    assert_eq!(
        evaluate(&block(vec![
            Statement::Block(Block {
                statements: vec![Statement::Let {
                    name: "a".into(),
                    value: lit(5)
                }]
            }),
            Statement::Expression(var("a"))
        ]))
        .unwrap(),
        Datum::Integer(5)
    );
    assert_eq!(
        evaluate(&block(vec![
            Statement::If {
                condition: boolean(false),
                then_block: Block { statements: vec![] },
                else_block: Some(Block {
                    statements: vec![Statement::Let {
                        name: "a".into(),
                        value: lit(6)
                    }]
                })
            },
            Statement::Expression(var("a"))
        ]))
        .unwrap(),
        Datum::Integer(6)
    );
    assert_eq!(
        evaluate(&block(vec![
            Statement::For {
                pattern: "i".into(),
                iter: Expression::Array(vec![lit(1), lit(2)]),
                body: Block {
                    statements: vec![Statement::Let {
                        name: "last".into(),
                        value: var("i")
                    }]
                }
            },
            Statement::Expression(var("last"))
        ]))
        .unwrap(),
        Datum::Integer(2)
    );
    assert_eq!(
        evaluate(&block(vec![
            Statement::Let {
                name: "running".into(),
                value: boolean(true)
            },
            Statement::While {
                condition: var("running"),
                body: Block {
                    statements: vec![Statement::Let {
                        name: "running".into(),
                        value: boolean(false)
                    }]
                }
            },
            Statement::Expression(var("running"))
        ]))
        .unwrap(),
        Datum::Bool(false)
    );
    assert_eq!(
        evaluate(&block(vec![
            Statement::Match {
                expr: lit(2),
                arms: vec![arm(
                    Pattern::Wildcard,
                    block(vec![Statement::Let {
                        name: "matched".into(),
                        value: lit(7)
                    }])
                )]
            },
            Statement::Expression(var("matched"))
        ]))
        .unwrap(),
        Datum::Integer(7)
    );
}
#[test]
#[allow(clippy::too_many_lines)] // Every computed variant uses the same geometry/HTTP fixture.
fn all_http_context_fields_and_computed_values_are_asserted() {
    for (field, expected) in [
        ("url", Datum::Value(Value::String("/http".into()))),
        ("method", Datum::Value(Value::String("POST".into()))),
        ("status", Datum::Integer(201)),
        ("duration_ms", Datum::Integer(8)),
        ("error", Datum::Value(Value::String("problem".into()))),
        (
            "headers",
            Datum::Array(vec![Datum::Array(vec![
                Datum::Value(Value::String("x".into())),
                Datum::Value(Value::String("y".into())),
            ])]),
        ),
    ] {
        assert_eq!(
            evaluate(&Expression::Field {
                object: Box::new(var("http")),
                field: field.into()
            })
            .unwrap(),
            expected
        );
    }
    let root = Container {
        id: 5,
        str_id: Some("root".into()),
        data: BTreeMap::from([("test".into(), "data".into())]),
        ..Container::default()
    };
    let source = Source::at(&root, vec![], 0).unwrap();
    let owner = Owner {
        source: source.clone(),
        action: vec![0],
    };
    let mut context = EventContext::new(source, EventKind::Click, 0);
    context.value = Some("payload".into());
    context.keys = vec![Key::Enter];
    context.pointer = Some((12.0, 23.0));
    context.geometry.insert(
        "root".into(),
        Geometry {
            x: 2.0,
            y: 3.0,
            width: 20.0,
            height: 30.0,
        },
    );
    let target = ElementTarget::SelfTarget;
    for (calc, expected) in [
        (
            CalcValue::Visibility {
                target: target.clone(),
            },
            Value::Visibility(Visibility::Visible),
        ),
        (
            CalcValue::Display {
                target: target.clone(),
            },
            Value::Display(true),
        ),
        (
            CalcValue::Id {
                target: target.clone(),
            },
            Value::String("root".into()),
        ),
        (
            CalcValue::DataAttrValue {
                attr: "test".into(),
                target: target.clone(),
            },
            Value::String("data".into()),
        ),
        (CalcValue::EventValue, Value::String("payload".into())),
        (
            CalcValue::WidthPx {
                target: target.clone(),
            },
            Value::Real(20.0),
        ),
        (
            CalcValue::HeightPx {
                target: target.clone(),
            },
            Value::Real(30.0),
        ),
        (
            CalcValue::PositionX {
                target: target.clone(),
            },
            Value::Real(2.0),
        ),
        (
            CalcValue::PositionY {
                target: target.clone(),
            },
            Value::Real(3.0),
        ),
        (
            CalcValue::MouseX {
                target: Some(target.clone()),
            },
            Value::Real(10.0),
        ),
        (
            CalcValue::MouseY {
                target: Some(target),
            },
            Value::Real(20.0),
        ),
        (CalcValue::MouseX { target: None }, Value::Real(12.0)),
        (CalcValue::MouseY { target: None }, Value::Real(23.0)),
        (CalcValue::Key { key: Key::Enter }, Value::Display(true)),
        (CalcValue::Key { key: Key::Escape }, Value::Display(false)),
    ] {
        let mut engine = ActionEngine::default();
        let values = FormValues::new();
        let mut work = Work::new(&root, &context, &values, &mut engine).unwrap();
        assert_eq!(work.value(&Value::Calc(calc), &owner, 0).unwrap(), expected);
    }
}
