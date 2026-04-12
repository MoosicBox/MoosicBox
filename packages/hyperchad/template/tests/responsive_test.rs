use std::collections::BTreeMap;

use hyperchad_renderer::HtmlTagRenderer;
use hyperchad_renderer_html::{DefaultHtmlTagRenderer, html::elements_to_html};
use hyperchad_template::container;
use hyperchad_transformer::{Number, OverrideCondition, OverrideItem, ResponsiveTrigger};

#[cfg(feature = "logic")]
use hyperchad_template::{AlignItems, LayoutDirection, TextAlign, if_responsive};

#[test]
#[cfg(feature = "logic")]
fn test_responsive_number_attributes() {
    let containers = container! {
        div #test-num padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20)) {
            "Test content"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert_eq!(container.padding_left, Some(Number::Integer(20)));
    assert_eq!(container.padding_right, Some(Number::Integer(20)));
    assert_eq!(container.overrides.len(), 2);

    let mut found_left = false;
    let mut found_right = false;

    for config in &container.overrides {
        assert_eq!(
            config.condition,
            OverrideCondition::ResponsiveTarget {
                name: "mobile".to_string()
            }
        );
        assert_eq!(config.overrides.len(), 1);

        match &config.overrides[0] {
            OverrideItem::PaddingLeft(value) => {
                assert_eq!(value, &Number::Integer(10));
                assert_eq!(
                    config.default,
                    Some(OverrideItem::PaddingLeft(Number::Integer(20)))
                );
                found_left = true;
            }
            OverrideItem::PaddingRight(value) => {
                assert_eq!(value, &Number::Integer(10));
                assert_eq!(
                    config.default,
                    Some(OverrideItem::PaddingRight(Number::Integer(20)))
                );
                found_right = true;
            }
            item => panic!("unexpected override item: {item:?}"),
        }
    }

    assert!(found_left);
    assert!(found_right);
}

#[test]
#[cfg(feature = "logic")]
fn test_responsive_enum_attributes() {
    let containers = container! {
        div #test-enum
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

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert_eq!(container.direction, LayoutDirection::Row);
    assert_eq!(container.align_items, Some(AlignItems::Start));
    assert_eq!(container.overrides.len(), 2);

    let mut found_direction = false;
    let mut found_align_items = false;

    for config in &container.overrides {
        match &config.overrides[0] {
            OverrideItem::Direction(value) => {
                assert_eq!(
                    config.condition,
                    OverrideCondition::ResponsiveTarget {
                        name: "mobile-large".to_string()
                    }
                );
                assert_eq!(value, &LayoutDirection::Column);
                assert_eq!(
                    config.default,
                    Some(OverrideItem::Direction(LayoutDirection::Row))
                );
                found_direction = true;
            }
            OverrideItem::AlignItems(value) => {
                assert_eq!(
                    config.condition,
                    OverrideCondition::ResponsiveTarget {
                        name: "mobile".to_string()
                    }
                );
                assert_eq!(value, &AlignItems::Center);
                assert_eq!(
                    config.default,
                    Some(OverrideItem::AlignItems(AlignItems::Start))
                );
                found_align_items = true;
            }
            item => panic!("unexpected override item: {item:?}"),
        }
    }

    assert!(found_direction);
    assert!(found_align_items);
}

#[test]
#[cfg(feature = "logic")]
fn test_responsive_bool_attributes() {
    let containers = container! {
        div #test-bool hidden=(if_responsive("mobile").then::<bool>(true).or_else(false)) {
            "Test content"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert_eq!(container.hidden, Some(false));
    assert_eq!(container.overrides.len(), 1);

    let config = &container.overrides[0];
    assert_eq!(
        config.condition,
        OverrideCondition::ResponsiveTarget {
            name: "mobile".to_string()
        }
    );
    assert_eq!(config.overrides, vec![OverrideItem::Hidden(true)]);
    assert_eq!(config.default, Some(OverrideItem::Hidden(false)));
}

#[test]
#[cfg(feature = "logic")]
fn test_mixed_responsive_and_static_attributes() {
    let containers = container! {
        div #mixed
            width=(100)
            padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20))
            direction=(LayoutDirection::Row)
            text-align=(
                if_responsive("tablet")
                    .then::<TextAlign>(TextAlign::Center)
                    .or_else(TextAlign::Start)
            )
        {
            "Mixed content"
        }
    };

    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    assert_eq!(container.width, Some(Number::Integer(100)));
    assert_eq!(container.direction, LayoutDirection::Row);
    assert_eq!(container.padding_left, Some(Number::Integer(20)));
    assert_eq!(container.padding_right, Some(Number::Integer(20)));
    assert_eq!(container.text_align, Some(TextAlign::Start));

    assert_eq!(container.overrides.len(), 3);
}

#[test]
#[cfg(feature = "logic")]
fn test_responsive_overrides_render_media_query_css() {
    let containers = container! {
        div #responsive-css-test
            padding-left=(if_responsive("mobile").then::<i32>(10).or_else(24))
        {
            "Responsive css"
        }
    };

    let mut renderer = DefaultHtmlTagRenderer::default()
        .with_responsive_trigger("mobile", ResponsiveTrigger::MaxWidth(Number::Integer(768)));
    renderer.add_responsive_trigger(
        "tablet".to_string(),
        ResponsiveTrigger::MaxWidth(Number::Integer(1024)),
    );

    let mut content = Vec::new();
    elements_to_html(&mut content, &containers, &renderer, false).unwrap();
    let content = String::from_utf8(content).unwrap();

    let html = renderer.root_html(
        &BTreeMap::new(),
        &containers[0],
        content,
        None,
        None,
        None,
        None,
        &[],
        &[],
        &[],
    );

    assert!(html.contains("@media(max-width:768px)"));
    assert!(html.contains("#responsive-css-test"));
    assert!(html.contains("padding-left:10px !important;"));
}
