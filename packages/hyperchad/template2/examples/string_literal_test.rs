use hyperchad_template2::container;

fn print_container(container: &hyperchad_transformer::Container, indent: usize) {
    let indent_str = "  ".repeat(indent);
    match &container.element {
        hyperchad_transformer::Element::Raw { value } => {
            println!("{}Raw: '{}'", indent_str, value);
        }
        element => {
            println!(
                "{}{:?} (children: {})",
                indent_str,
                element,
                container.children.len()
            );
            for child in &container.children {
                print_container(child, indent + 1);
            }
        }
    }
}

fn main() {
    let containers = container! {
        Div {
            "This is a string literal"
            "Another string literal"
            Span { "String inside span" }
        }
    };

    println!("Generated {} containers:", containers.len());
    for (i, container) in containers.iter().enumerate() {
        println!("Container {}:", i + 1);
        print_container(container, 1);
    }
}
