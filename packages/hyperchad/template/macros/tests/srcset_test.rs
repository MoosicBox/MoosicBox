use hyperchad_template::container;

#[test]
fn test_srcset_concatenation() {
    let containers = container! {
        image srcset={
            "showcase-240.webp 240w, "
            "showcase-540.webp 540w, "
            "showcase-1080.webp 1080w"
        } {
        }
    };

    // Verify that the srcset was concatenated correctly
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    if let hyperchad_transformer::Element::Image { source_set, .. } = &container.element {
        assert!(source_set.is_some());
        let srcset_value = source_set.as_ref().unwrap();
        assert_eq!(
            srcset_value,
            "showcase-240.webp 240w, showcase-540.webp 540w, showcase-1080.webp 1080w"
        );
    } else {
        panic!("Expected Image element");
    }
}

#[test]
fn test_srcset_with_expressions() {
    fn public_img(name: &str) -> String {
        format!("/public/{name}")
    }

    let containers = container! {
        image srcset={
            (public_img("showcase-2x240.webp"))" 240w, "
            (public_img("showcase-2x540.webp"))" 540w, "
            (public_img("showcase-2.webp"))" 1080w"
        } {
        }
    };

    // Verify that the srcset was concatenated correctly with expressions
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    if let hyperchad_transformer::Element::Image { source_set, .. } = &container.element {
        assert!(source_set.is_some());
        let srcset_value = source_set.as_ref().unwrap();
        assert_eq!(
            srcset_value,
            "/public/showcase-2x240.webp 240w, /public/showcase-2x540.webp 540w, /public/showcase-2.webp 1080w"
        );
    } else {
        panic!("Expected Image element");
    }
}

#[test]
fn test_srcset_simple_string() {
    let containers = container! {
        image srcset="showcase.webp 1x, showcase-2x.webp 2x" {
        }
    };

    // Verify simple string srcset
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    if let hyperchad_transformer::Element::Image { source_set, .. } = &container.element {
        assert!(source_set.is_some());
        let srcset_value = source_set.as_ref().unwrap();
        assert_eq!(srcset_value, "showcase.webp 1x, showcase-2x.webp 2x");
    } else {
        panic!("Expected Image element");
    }
}

#[test]
fn test_srcset_expression() {
    let srcset_value = "responsive.webp 480w, responsive-hd.webp 1080w".to_string();
    let containers = container! {
        image srcset=(srcset_value) {
        }
    };

    // Verify expression-based srcset
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    if let hyperchad_transformer::Element::Image { source_set, .. } = &container.element {
        assert!(source_set.is_some());
        let srcset_value = source_set.as_ref().unwrap();
        assert_eq!(
            srcset_value,
            "responsive.webp 480w, responsive-hd.webp 1080w"
        );
    } else {
        panic!("Expected Image element");
    }
}
