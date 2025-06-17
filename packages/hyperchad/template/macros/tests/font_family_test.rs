use hyperchad_template::container;

#[test]
fn test_font_family_parsing() {
    let containers = container! {
        div font-family="Arial, sans-serif" {
            "Test text"
        }
    };

    // Verify that the font-family was parsed correctly
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Check that font_family is Some and contains the expected values
    assert!(container.font_family.is_some());
    let font_family = container.font_family.as_ref().unwrap();
    assert_eq!(font_family.len(), 2);
    assert_eq!(font_family[0], "Arial");
    assert_eq!(font_family[1], "sans-serif");
}

#[test]
fn test_font_family_single_font() {
    let containers = container! {
        div font-family="Helvetica" {
            "Test text"
        }
    };

    // Verify single font
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_family.is_some());
    let font_family = container.font_family.as_ref().unwrap();
    assert_eq!(font_family.len(), 1);
    assert_eq!(font_family[0], "Helvetica");
}

#[test]
fn test_font_family_with_spaces() {
    let containers = container! {
        div font-family="Times New Roman, Georgia, serif" {
            "Test text"
        }
    };

    // Verify parsing with quoted font names and spaces
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_family.is_some());
    let font_family = container.font_family.as_ref().unwrap();
    assert_eq!(font_family.len(), 3);
    assert_eq!(font_family[0], "Times New Roman");
    assert_eq!(font_family[1], "Georgia");
    assert_eq!(font_family[2], "serif");
}

#[test]
fn test_font_family_expression() {
    let font_list = "Monaco, 'Courier New', monospace".to_string();
    let containers = container! {
        div font-family=(font_list) {
            "Test text"
        }
    };

    // Verify expression-based font family
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert!(container.font_family.is_some());
    let font_family = container.font_family.as_ref().unwrap();
    assert_eq!(font_family.len(), 3);
    assert_eq!(font_family[0], "Monaco");
    assert_eq!(font_family[1], "'Courier New'");
    assert_eq!(font_family[2], "monospace");
}
