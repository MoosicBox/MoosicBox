use hyperchad_template2::{Container, ContainerVecExt, RenderContainer, container};

#[ignore]
#[test]
fn owned_values() {
    let owned = String::from("yay");
    let _ = container! { (owned) };
    // Make sure the `container!` call didn't move it
    let _owned = owned;
}

#[ignore]
#[test]
fn simple_macro() {
    macro_rules! greet {
        () => {
            container! {
                Div { "Hello, Pinkie Pie!" }
            }
        };
    }
    assert!(!greet!().to_string().is_empty());
}

#[ignore]
#[test]
fn render_impl() {
    struct R(&'static str);
    impl RenderContainer for R {
        type Error = String; // Use String instead of core::fmt::Error

        fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
            containers.push(Container {
                element: hyperchad_transformer::Element::Raw {
                    value: self.0.to_string(),
                },
                ..Default::default()
            });
            Ok(())
        }
    }

    let r = R("pinkie");
    let result_a = container! { (r) };
    let result_b = container! { (r) };
    assert_eq!(result_a.to_string(), "pinkie");
    assert_eq!(result_b.to_string(), "pinkie");
}

#[ignore]
#[test]
fn default_test() {
    // Test that String::default() works
    assert_eq!(String::default(), "");
}
