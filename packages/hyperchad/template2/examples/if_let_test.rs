use hyperchad_template2::container;

fn main() {
    let auth_method = Some("oauth");
    let settings = Settings {
        auth_method: auth_method.map(|s| s.to_string()),
    };

    let containers = container! {
        Div {
            @if settings.auth_method.is_some() {
                "Authentication is enabled"
            } @else if let Some(auth_method) = &settings.auth_method {
                "Auth method: " (auth_method)
            } @else {
                "No authentication"
            }
        }
    };

    println!("Generated {} containers:", containers.len());
    for (i, container) in containers.iter().enumerate() {
        println!("Container {}: Element = {:?}", i + 1, container.element);
    }
}

struct Settings {
    auth_method: Option<String>,
}
