use hyperchad_template::container;
use hyperchad_transformer_models::*;

#[test]
fn test_bare_identifier_enum_attributes() {
    // Test visibility with kebab-case bare identifiers (new syntax)
    let containers = container! {
        div visibility=hidden { "Hidden div" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Hidden));

    let containers = container! {
        div visibility=visible { "Visible div" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Visible));

    // Test layout direction with kebab-case bare identifiers
    let containers = container! {
        div direction=row { "Row layout" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].direction, LayoutDirection::Row);

    let containers = container! {
        div direction=column { "Column layout" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].direction, LayoutDirection::Column);

    // Test position with kebab-case bare identifiers
    let containers = container! {
        div position=fixed { "Fixed position" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].position, Some(Position::Fixed));

    let containers = container! {
        div position=relative { "Relative position" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].position, Some(Position::Relative));

    // Test align-items with kebab-case bare identifiers
    let containers = container! {
        div align-items=center { "Centered items" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].align_items, Some(AlignItems::Center));

    let containers = container! {
        div align-items=start { "Start alignment" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].align_items, Some(AlignItems::Start));

    // Test justify-content with kebab-case bare identifiers
    let containers = container! {
        div justify-content=center { "Centered content" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].justify_content, Some(JustifyContent::Center));

    let containers = container! {
        div justify-content=space-between { "Space between" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(
        containers[0].justify_content,
        Some(JustifyContent::SpaceBetween)
    );

    // Test cursor with kebab-case bare identifiers
    let containers = container! {
        div cursor=pointer { "Pointer cursor" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].cursor, Some(Cursor::Pointer));

    let containers = container! {
        div cursor=auto { "Auto cursor" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].cursor, Some(Cursor::Auto));
}

#[test]
fn test_string_literal_enum_attributes() {
    // Test that string literals work (kebab-case conversion)
    let containers = container! {
        div visibility="hidden" { "Hidden with string" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Hidden));

    let containers = container! {
        div visibility="visible" { "Visible with string" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Visible));

    let containers = container! {
        div direction="row" { "Row layout" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].direction, LayoutDirection::Row);

    let containers = container! {
        div direction="column" { "Column layout" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].direction, LayoutDirection::Column);

    let containers = container! {
        div position="fixed" { "Fixed position" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].position, Some(Position::Fixed));

    let containers = container! {
        div position="absolute" { "Absolute position" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].position, Some(Position::Absolute));

    let containers = container! {
        div position="relative" { "Relative position" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].position, Some(Position::Relative));
}

#[test]
fn test_alignment_enum_attributes() {
    // Test align-items with string literals - using correct enum variants
    let containers = container! {
        div align-items="center" { "Centered items" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].align_items, Some(AlignItems::Center));

    let containers = container! {
        div align-items="start" { "Start alignment" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].align_items, Some(AlignItems::Start));

    let containers = container! {
        div align-items="end" { "End alignment" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].align_items, Some(AlignItems::End));

    // Test justify-content with string literals
    let containers = container! {
        div justify-content="center" { "Centered content" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].justify_content, Some(JustifyContent::Center));

    let containers = container! {
        div justify-content="space-between" { "Space between" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(
        containers[0].justify_content,
        Some(JustifyContent::SpaceBetween)
    );

    let containers = container! {
        div justify-content="space-evenly" { "Space evenly" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(
        containers[0].justify_content,
        Some(JustifyContent::SpaceEvenly)
    );
}

#[test]
fn test_text_enum_attributes() {
    // Test text-align with string literals - using correct enum variants
    let containers = container! {
        div text-align="center" { "Centered text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].text_align, Some(TextAlign::Center));

    let containers = container! {
        div text-align="start" { "Start aligned text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].text_align, Some(TextAlign::Start));

    let containers = container! {
        div text-align="end" { "End aligned text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].text_align, Some(TextAlign::End));

    let containers = container! {
        div text-align="justify" { "Justified text" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].text_align, Some(TextAlign::Justify));
}

#[test]
fn test_overflow_enum_attributes() {
    // Test overflow-x with string literals - using correct enum variants
    let containers = container! {
        div overflow-x="hidden" { "Hidden overflow-x" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].overflow_x, LayoutOverflow::Hidden);

    let containers = container! {
        div overflow-x="scroll" { "Scroll overflow-x" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].overflow_x, LayoutOverflow::Scroll);

    let containers = container! {
        div overflow-x="auto" { "Auto overflow-x" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].overflow_x, LayoutOverflow::Auto);

    // Test overflow-y with string literals
    let containers = container! {
        div overflow-y="auto" { "Auto overflow-y" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].overflow_y, LayoutOverflow::Auto);
}

#[test]
fn test_cursor_enum_attributes() {
    // Test cursor with string literals - using correct enum variants
    let containers = container! {
        div cursor="pointer" { "Pointer cursor" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].cursor, Some(Cursor::Pointer));

    let containers = container! {
        div cursor="auto" { "Auto cursor" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].cursor, Some(Cursor::Auto));

    let containers = container! {
        div cursor="text" { "Text cursor" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].cursor, Some(Cursor::Text));

    let containers = container! {
        div cursor="grab" { "Grab cursor" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].cursor, Some(Cursor::Grab));
}

#[test]
fn test_expression_enum_attributes() {
    // Test using expressions with enum values
    let containers = container! {
        div visibility=(Visibility::Hidden) { "Hidden with expression" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Hidden));

    let containers = container! {
        div align-items=(AlignItems::Center) { "Center with expression" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].align_items, Some(AlignItems::Center));

    let containers = container! {
        div justify-content=(JustifyContent::SpaceBetween) { "Space between with expression" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(
        containers[0].justify_content,
        Some(JustifyContent::SpaceBetween)
    );

    let containers = container! {
        div text-align=(TextAlign::Center) { "Center with expression" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].text_align, Some(TextAlign::Center));
}

#[test]
fn test_image_enum_attributes() {
    // Test image fit with string literals
    let containers = container! {
        image fit="cover" { }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(fit, &Some(ImageFit::Cover));
    } else {
        panic!("Expected Image element");
    }

    let containers = container! {
        image fit="contain" { }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(fit, &Some(ImageFit::Contain));
    } else {
        panic!("Expected Image element");
    }

    let containers = container! {
        image fit="fill" { }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Image { fit, .. } = &containers[0].element {
        assert_eq!(fit, &Some(ImageFit::Fill));
    } else {
        panic!("Expected Image element");
    }

    // Test image loading with string literals
    let containers = container! {
        image loading="lazy" { }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Image { loading, .. } = &containers[0].element {
        assert_eq!(loading, &Some(ImageLoading::Lazy));
    } else {
        panic!("Expected Image element");
    }

    let containers = container! {
        image loading="eager" { }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Image { loading, .. } = &containers[0].element {
        assert_eq!(loading, &Some(ImageLoading::Eager));
    } else {
        panic!("Expected Image element");
    }
}

#[test]
fn test_anchor_enum_attributes() {
    // Test anchor target with string literals
    let containers = container! {
        anchor target="_blank" { "Link opens in new tab" }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(target, &Some(LinkTarget::Blank));
    } else {
        panic!("Expected Anchor element");
    }

    let containers = container! {
        anchor target="_self" { "Link opens in same tab" }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(target, &Some(LinkTarget::SelfTarget));
    } else {
        panic!("Expected Anchor element");
    }

    let containers = container! {
        anchor target="_parent" { "Link opens in parent frame" }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(target, &Some(LinkTarget::Parent));
    } else {
        panic!("Expected Anchor element");
    }

    let containers = container! {
        anchor target="_top" { "Link opens in top frame" }
    };
    assert_eq!(containers.len(), 1);
    if let hyperchad_transformer::Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(target, &Some(LinkTarget::Top));
    } else {
        panic!("Expected Anchor element");
    }
}

#[test]
fn test_mixed_enum_attribute_syntax() {
    // Test mixing strings and expressions
    let is_hidden = false;
    let containers = container! {
        div
            visibility="hidden"
            align-items="center"
            justify-content=(if is_hidden { JustifyContent::End } else { JustifyContent::Center })
            direction="row"
        {
            "Mixed syntax"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Hidden));
    assert_eq!(containers[0].align_items, Some(AlignItems::Center));
    assert_eq!(containers[0].justify_content, Some(JustifyContent::Center));
    assert_eq!(containers[0].direction, LayoutDirection::Row);
}

#[test]
fn test_complex_expression_enum_attributes() {
    // Test using complex expressions that evaluate to enum values
    let use_center_alignment = true;
    let containers = container! {
        div align-items=(if use_center_alignment { AlignItems::Center } else { AlignItems::Start }) {
            "Dynamic alignment"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].align_items, Some(AlignItems::Center));

    // Test with expressions
    let containers = container! {
        div position=(Position::Fixed) { "Expression position" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].position, Some(Position::Fixed));

    // Test with function calls
    fn get_text_alignment() -> TextAlign {
        TextAlign::End
    }
    let containers = container! {
        div text-align=(get_text_alignment()) { "Function-determined alignment" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].text_align, Some(TextAlign::End));
}

#[test]
fn test_brace_wrapped_enum_attributes() {
    // Test expressions wrapped in braces
    let containers = container! {
        div visibility={Visibility::Hidden} { "Brace-wrapped expression" }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Hidden));

    // Test complex expressions in braces
    let should_be_visible = true;
    let containers = container! {
        div visibility={if should_be_visible { Visibility::Visible } else { Visibility::Hidden }} {
            "Brace-wrapped expression"
        }
    };
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].visibility, Some(Visibility::Visible));
}

#[test]
fn test_multiple_enum_attributes_on_single_element() {
    // Test an element with multiple enum attributes using different syntax styles
    let containers = container! {
        div
            visibility="visible"
            direction="column"
            align-items="center"
            justify-content="space-between"
            position="relative"
            cursor="pointer"
            overflow-x="hidden"
            overflow-y="scroll"
            text-align="center"
        {
            "Element with many enum attributes"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert_eq!(container.visibility, Some(Visibility::Visible));
    assert_eq!(container.direction, LayoutDirection::Column);
    assert_eq!(container.align_items, Some(AlignItems::Center));
    assert_eq!(
        container.justify_content,
        Some(JustifyContent::SpaceBetween)
    );
    assert_eq!(container.position, Some(Position::Relative));
    assert_eq!(container.cursor, Some(Cursor::Pointer));
    assert_eq!(container.overflow_x, LayoutOverflow::Hidden);
    assert_eq!(container.overflow_y, LayoutOverflow::Scroll);
    assert_eq!(container.text_align, Some(TextAlign::Center));
}

#[test]
fn test_default_enum_values() {
    // Test that elements get correct default values when no enum attributes are specified
    let containers = container! {
        div { "Default values" }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // These should be None/Default as they weren't explicitly set
    assert_eq!(container.visibility, None);
    assert_eq!(container.direction, LayoutDirection::default());
    assert_eq!(container.align_items, None);
    assert_eq!(container.justify_content, None);
    assert_eq!(container.position, None);
    assert_eq!(container.cursor, None);
    assert_eq!(container.overflow_x, LayoutOverflow::default());
    assert_eq!(container.overflow_y, LayoutOverflow::default());
    assert_eq!(container.text_align, None);
}
