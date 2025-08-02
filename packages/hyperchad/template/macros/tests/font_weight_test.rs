use hyperchad_template::container;
use hyperchad_transformer_models::FontWeight;

#[test]
fn test_font_weight_named_variants() {
    let containers = container! {
        div font-weight=bold {
            "Bold text"
        }
    };

    // Verify that the font-weight was parsed correctly
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that font_weight is Some and contains the expected value
    assert!(container.font_weight.is_some());
    assert_eq!(container.font_weight.unwrap(), FontWeight::Bold);
}

#[test]
fn test_font_weight_numeric_variants() {
    let containers = container! {
        div font-weight=700 {
            "Heavy text"
        }
    };

    // Verify numeric font weight
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_weight.is_some());
    assert_eq!(container.font_weight.unwrap(), FontWeight::Weight700);
}

#[test]
fn test_font_weight_hyphenated_variants() {
    let containers = container! {
        div font-weight=semi-bold {
            "Semi-bold text"
        }
    };

    // Verify hyphenated font weight
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_weight.is_some());
    assert_eq!(container.font_weight.unwrap(), FontWeight::SemiBold);
}

#[test]
fn test_font_weight_extra_light() {
    let containers = container! {
        div font-weight=extra-light {
            "Extra light text"
        }
    };

    // Verify extra-light font weight
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_weight.is_some());
    assert_eq!(container.font_weight.unwrap(), FontWeight::ExtraLight);
}

#[test]
fn test_font_weight_normal() {
    let containers = container! {
        div font-weight=normal {
            "Normal text"
        }
    };

    // Verify normal font weight
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_weight.is_some());
    assert_eq!(container.font_weight.unwrap(), FontWeight::Normal);
}

#[test]
fn test_font_weight_relative_variants() {
    let containers = container! {
        div font-weight=lighter {
            "Lighter text"
        }
    };

    // Verify relative font weight
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_weight.is_some());
    assert_eq!(container.font_weight.unwrap(), FontWeight::Lighter);
}

#[test]
fn test_font_weight_all_numeric_values() {
    // Test 100
    let containers = container! {
        div font-weight=100 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight100);

    // Test 200
    let containers = container! {
        div font-weight=200 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight200);

    // Test 300
    let containers = container! {
        div font-weight=300 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight300);

    // Test 400
    let containers = container! {
        div font-weight=400 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight400);

    // Test 500
    let containers = container! {
        div font-weight=500 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight500);

    // Test 600
    let containers = container! {
        div font-weight=600 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight600);

    // Test 700
    let containers = container! {
        div font-weight=700 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight700);

    // Test 800
    let containers = container! {
        div font-weight=800 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight800);

    // Test 900
    let containers = container! {
        div font-weight=900 { "Test text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].font_weight.unwrap(), FontWeight::Weight900);
}

#[test]
fn test_font_weight_expression() {
    let containers = container! {
        div font-weight=(FontWeight::Bold) {
            "Expression-based font weight"
        }
    };

    // Verify expression-based font weight
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_weight.is_some());
    assert_eq!(container.font_weight.unwrap(), FontWeight::Bold);
}
