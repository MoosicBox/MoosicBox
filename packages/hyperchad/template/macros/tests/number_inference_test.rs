use hyperchad_template_macros::container;
use hyperchad_transformer::Number;

#[test]
fn test_raw_number_literals() {
    let result = container! {
        Div padding=(20) margin=(5) opacity=(0.5) {
            "Test content"
        }
    };

    assert_eq!(result.len(), 1);

    let container = &result[0];

    // Check that padding was set correctly
    if let Some(Number::Integer(20)) = container.padding_top {
        // Success
    } else {
        panic!(
            "Expected padding_top to be Integer(20), got: {:?}",
            container.padding_top
        );
    }

    // Check that margin was set correctly
    if let Some(Number::Integer(5)) = container.margin_top {
        // Success
    } else {
        panic!(
            "Expected margin_top to be Integer(5), got: {:?}",
            container.margin_top
        );
    }

    // Check that opacity was set correctly
    if let Some(Number::Real(opacity)) = container.opacity {
        assert!((opacity - 0.5).abs() < f32::EPSILON);
    } else {
        panic!(
            "Expected opacity to be Real(0.5), got: {:?}",
            container.opacity
        );
    }
}

#[test]
fn test_variable_number_expressions() {
    let padding_value = 15;
    let margin_value = 10;

    let result = container! {
        Div padding=(padding_value) margin=(margin_value) {
            "Test content"
        }
    };

    assert_eq!(result.len(), 1);

    let container = &result[0];

    // Check that padding was set correctly from variable
    if let Some(Number::Integer(15)) = container.padding_top {
        // Success
    } else {
        panic!(
            "Expected padding_top to be Integer(15), got: {:?}",
            container.padding_top
        );
    }

    // Check that margin was set correctly from variable
    if let Some(Number::Integer(10)) = container.margin_top {
        // Success
    } else {
        panic!(
            "Expected margin_top to be Integer(10), got: {:?}",
            container.margin_top
        );
    }
}

#[test]
fn test_computed_number_expressions() {
    let base_padding = 10;

    let result = container! {
        Div padding=(base_padding * 2) margin=(base_padding / 2) {
            "Test content"
        }
    };

    assert_eq!(result.len(), 1);

    let container = &result[0];

    // Check that computed padding was set correctly
    if let Some(Number::Integer(20)) = container.padding_top {
        // Success
    } else {
        panic!(
            "Expected padding_top to be Integer(20), got: {:?}",
            container.padding_top
        );
    }

    // Check that computed margin was set correctly
    if let Some(Number::Integer(5)) = container.margin_top {
        // Success
    } else {
        panic!(
            "Expected margin_top to be Integer(5), got: {:?}",
            container.margin_top
        );
    }
}

#[test]
fn test_explicit_number_constructors() {
    use hyperchad_transformer::Number;

    let result = container! {
        Div
            padding=(Number::Integer(25))
            margin=(Number::Real(7.5))
            opacity=(Number::Real(0.8))
        {
            "Test content"
        }
    };

    assert_eq!(result.len(), 1);

    let container = &result[0];

    // Check that explicit Number::Integer works
    if let Some(Number::Integer(25)) = container.padding_top {
        // Success
    } else {
        panic!(
            "Expected padding_top to be Integer(25), got: {:?}",
            container.padding_top
        );
    }

    // Check that explicit Number::Real works
    if let Some(Number::Real(margin)) = container.margin_top {
        assert!((margin - 7.5).abs() < f32::EPSILON);
    } else {
        panic!(
            "Expected margin_top to be Real(7.5), got: {:?}",
            container.margin_top
        );
    }

    // Check that explicit Number::Real for opacity works
    if let Some(Number::Real(opacity)) = container.opacity {
        assert!((opacity - 0.8).abs() < f32::EPSILON);
    } else {
        panic!(
            "Expected opacity to be Real(0.8), got: {:?}",
            container.opacity
        );
    }
}

#[test]
fn test_mixed_raw_and_explicit_numbers() {
    use hyperchad_transformer::Number;

    let raw_padding = 12;

    let result = container! {
        Div
            padding=(raw_padding)                    // Raw variable (Integer)
            margin=(15)                              // Raw literal (Integer)
            width=(Number::Real(100.5))              // Explicit Real
            height=(Number::IntegerPercent(80))      // Explicit IntegerPercent
            opacity=(0.6)                            // Raw float literal (Real)
        {
            "Test content"
        }
    };

    assert_eq!(result.len(), 1);

    let container = &result[0];

    // Check raw variable
    if let Some(Number::Integer(12)) = container.padding_top {
        // Success
    } else {
        panic!(
            "Expected padding_top to be Integer(12), got: {:?}",
            container.padding_top
        );
    }

    // Check raw literal
    if let Some(Number::Integer(15)) = container.margin_top {
        // Success
    } else {
        panic!(
            "Expected margin_top to be Integer(15), got: {:?}",
            container.margin_top
        );
    }

    // Check explicit Real
    if let Some(Number::Real(width)) = container.width {
        assert!((width - 100.5).abs() < f32::EPSILON);
    } else {
        panic!(
            "Expected width to be Real(100.5), got: {:?}",
            container.width
        );
    }

    // Check explicit IntegerPercent
    if let Some(Number::IntegerPercent(80)) = container.height {
        // Success
    } else {
        panic!(
            "Expected height to be IntegerPercent(80), got: {:?}",
            container.height
        );
    }

    // Check raw float literal
    if let Some(Number::Real(opacity)) = container.opacity {
        assert!((opacity - 0.6).abs() < f32::EPSILON);
    } else {
        panic!(
            "Expected opacity to be Real(0.6), got: {:?}",
            container.opacity
        );
    }
}
