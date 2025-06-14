use hyperchad_template2::container;

#[test]
fn test_user_example_srcset() {
    // Simulate the public_img! macro function
    fn public_img(name: &str) -> String {
        format!("/public/{}", name)
    }

    let containers = container! {
        Image srcset={
            (public_img("showcase-2x240.webp"))" 240w, "
            (public_img("showcase-2x540.webp"))" 540w, "
            (public_img("showcase-2.webp"))" 1080w"
        } {
        }
    };

    // Verify the exact behavior requested by the user
    assert_eq!(containers.len(), 1);
    let container = &containers[0];

    if let hyperchad_transformer::Element::Image { source_set, .. } = &container.element {
        assert!(source_set.is_some());
        let srcset_value = source_set.as_ref().unwrap();

        // This should be a single concatenated string, not an array
        println!("Generated srcset: {}", srcset_value);
        assert_eq!(
            srcset_value,
            "/public/showcase-2x240.webp 240w, /public/showcase-2x540.webp 540w, /public/showcase-2.webp 1080w"
        );

        // Verify it's actually a String type, not Vec<String>
        assert_eq!(
            std::any::type_name_of_val(srcset_value),
            "alloc::string::String"
        );
    } else {
        panic!("Expected Image element");
    }
}
