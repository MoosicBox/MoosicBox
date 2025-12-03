#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template::container;
use hyperchad_transformer::Element;
use hyperchad_transformer_models::LinkTarget;

#[test_log::test]
fn test_anchor_target_blank() {
    let containers = container! {
        anchor href="https://example.com" target="_blank" {
            "External Link"
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Anchor { href, target } = &containers[0].element {
        assert_eq!(href.as_deref(), Some("https://example.com"));
        assert_eq!(*target, Some(LinkTarget::Blank));
    } else {
        panic!("Expected Anchor element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_anchor_target_self() {
    let containers = container! {
        anchor href="/page" target="_self" {
            "Same Tab"
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(*target, Some(LinkTarget::SelfTarget));
    } else {
        panic!("Expected Anchor element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_anchor_target_parent() {
    let containers = container! {
        anchor href="/parent-page" target="_parent" {
            "Parent Frame"
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(*target, Some(LinkTarget::Parent));
    } else {
        panic!("Expected Anchor element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_anchor_target_top() {
    let containers = container! {
        anchor href="/top-page" target="_top" {
            "Top Frame"
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(*target, Some(LinkTarget::Top));
    } else {
        panic!("Expected Anchor element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_anchor_target_custom_frame() {
    let containers = container! {
        anchor href="/framed" target="my-iframe" {
            "Custom Frame"
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Anchor { target, .. } = &containers[0].element {
        assert_eq!(*target, Some(LinkTarget::Custom("my-iframe".to_string())));
    } else {
        panic!("Expected Anchor element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_anchor_no_target() {
    let containers = container! {
        anchor href="/page" {
            "Default Target"
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Anchor { href, target } = &containers[0].element {
        assert_eq!(href.as_deref(), Some("/page"));
        assert_eq!(*target, None);
    } else {
        panic!("Expected Anchor element, got: {:?}", containers[0].element);
    }
}

#[test_log::test]
fn test_anchor_dynamic_href() {
    let url = "/dynamic-path";

    let containers = container! {
        anchor href=(url) target="_blank" {
            "Dynamic URL"
        }
    };

    assert_eq!(containers.len(), 1);

    if let Element::Anchor { href, target } = &containers[0].element {
        assert_eq!(href.as_deref(), Some("/dynamic-path"));
        assert_eq!(*target, Some(LinkTarget::Blank));
    } else {
        panic!("Expected Anchor element, got: {:?}", containers[0].element);
    }
}
