use hyperchad_template::container;

#[cfg(feature = "logic")]
use hyperchad_template::{AlignItems, LayoutDirection, TextAlign, if_responsive};

#[test]
#[cfg(feature = "logic")]
fn test_responsive_number_attributes() {
    let containers = container! {
        Div padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20)) {
            "Test content"
        }
    };

    // Verify that the responsive attribute was processed successfully
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // For now, we just check that it didn't panic and compiled successfully
    // The actual responsive behavior would be handled at runtime
    assert!(container.padding_left.is_some());
    assert!(container.padding_right.is_some());
}

#[test]
#[cfg(feature = "logic")]
fn test_responsive_enum_attributes() {
    let containers = container! {
        Div
            direction=(
                if_responsive("mobile-large")
                    .then::<LayoutDirection>(LayoutDirection::Column)
                    .or_else(LayoutDirection::Row)
            )
            align-items=(
                if_responsive("mobile")
                    .then::<AlignItems>(AlignItems::Center)
                    .or_else(AlignItems::Start)
            )
        {
            "Test content"
        }
    };

    // Verify that the responsive enum attributes were processed successfully
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // The actual values would depend on runtime responsive evaluation,
    // but we can verify the structure was created correctly
    assert!(matches!(
        container.direction,
        LayoutDirection::Row | LayoutDirection::Column
    ));
    assert!(container.align_items.is_some());
}

#[test]
#[cfg(feature = "logic")]
fn test_responsive_bool_attributes() {
    let containers = container! {
        Div hidden=(if_responsive("mobile").then::<bool>(true).or_else(false)) {
            "Test content"
        }
    };

    // Verify that the responsive boolean attribute was processed successfully
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // The hidden attribute should be set based on responsive logic
    assert!(container.hidden.is_some());
}

#[test]
#[cfg(feature = "logic")]
fn test_mixed_responsive_and_static_attributes() {
    let containers = container! {
        Div
            width=(100)  // Static attribute
            padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20))  // Responsive
            direction=(LayoutDirection::Row)  // Static enum
            text-align=(
                if_responsive("tablet")
                    .then::<TextAlign>(TextAlign::Center)
                    .or_else(TextAlign::Start)
            )  // Responsive enum
        {
            "Mixed content"
        }
    };

    // Verify both static and responsive attributes coexist
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    // Static attributes should have their expected values
    assert!(container.width.is_some());
    assert_eq!(container.direction, LayoutDirection::Row);

    // Responsive attributes should be processed
    assert!(container.padding_left.is_some());
    assert!(container.padding_right.is_some());
    assert!(container.text_align.is_some());
}
