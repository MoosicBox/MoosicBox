use hyperchad_template2::container;

fn main() {
    let auth_method = Some("oauth".to_string());
    let user_role = Some("admin".to_string());
    let is_enabled = true;
    let count = 5;

    let containers = container! {
        Div {
            "Testing various if conditions:"

            // Regular if/else if/else
            @if count > 10 {
                "Count is greater than 10"
            } @else if count > 0 {
                "Count is positive but not greater than 10"
            } @else {
                "Count is zero or negative"
            }

            // if let patterns
            @if let Some(method) = &auth_method {
                "Auth method: " (method.clone())
            } @else {
                "No authentication method"
            }

            // Nested if let with regular if
            @if let Some(role) = &user_role {
                @if role == "admin" {
                    "User is an admin with role: " (role.clone())
                } @else {
                    "User has role: " (role.clone())
                }
            } @else if is_enabled {
                "No role but system is enabled"
            } @else {
                "No role and system is disabled"
            }

            // Complex if let with multiple patterns
            @if let Some(method) = &auth_method {
                @if let Some(role) = &user_role {
                    "Both auth method (" (method.clone()) ") and role (" (role.clone()) ") are set"
                } @else {
                    "Auth method is set but no role"
                }
            } @else {
                "No authentication configured"
            }
        }
    };

    println!("Generated {} containers:", containers.len());
    for (i, container) in containers.iter().enumerate() {
        println!("Container {}: Element = {:?}", i + 1, container.element);
        if !container.children.is_empty() {
            println!("  Children: {}", container.children.len());
            for (j, child) in container.children.iter().enumerate() {
                println!("    Child {}: {:?}", j + 1, child.element);
            }
        }
    }
}
