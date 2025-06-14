use hyperchad_template2::{Containers, container};

fn main() {
    // Example of the new container! macro syntax
    let containers: Containers = container! {
        Section
            align-items="start"
            width="100%"
            height="100%"
        {
            Div
                align-items="end"
                justify-content="center"
                width="100%"
                height="100%"
                .primary-container
                #main-content
            {
                "Hello, world!"

                Button
                    .btn
                    .btn-primary
                {
                    "Click me!"
                }

                @if true {
                    Span {
                        "This is conditionally rendered"
                    }
                }

                @for item in &["Item 1", "Item 2", "Item 3"] {
                    Div .list-item {
                        (item)
                    }
                }
            }
        }
    };

    // The macro returns a Vec<Container> instead of an HTML string
    println!("Generated {} containers", containers.len());

    // You can convert to HTML if needed
    for container in &containers {
        println!("{}", container);
    }

    // Or use programmatically
    let first_container = &containers[0];
    println!("First container element: {:?}", first_container.element);
    println!("First container classes: {:?}", first_container.classes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_macro() {
        let containers = container! {
            Div width="100px" height="50px" {
                "Test content"
            }
        };

        assert_eq!(containers.len(), 1);
        assert!(matches!(
            containers[0].element,
            hyperchad_transformer::Element::Div
        ));
        assert_eq!(containers[0].width.as_ref().unwrap().to_string(), "100px");
        assert_eq!(containers[0].height.as_ref().unwrap().to_string(), "50px");
        assert_eq!(containers[0].children.len(), 1);
    }

    #[test]
    fn test_styled_button() {
        let button = StyledButton {
            text: "Submit".to_string(),
            primary: true,
            disabled: false,
        };

        let containers = button.render();
        assert_eq!(containers.len(), 1);
        assert!(matches!(
            containers[0].element,
            hyperchad_transformer::Element::Button { .. }
        ));
        assert!(containers[0].classes.contains(&"btn".to_string()));
        assert!(containers[0].classes.contains(&"btn-primary".to_string()));
    }
}
