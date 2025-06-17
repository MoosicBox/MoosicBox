use hyperchad_template::{Container, RenderContainer, container};

struct TodoItem {
    text: String,
    completed: bool,
}

impl RenderContainer for TodoItem {
    type Error = core::fmt::Error;

    fn render_to(&self, containers: &mut Vec<Container>) -> Result<(), Self::Error> {
        let rendered = container! {
            div .todo-item {
                "Task: "
                span { (self.text.clone()) }
                @if self.completed {
                    " (completed)"
                }
            }
        };
        containers.extend(rendered);
        Ok(())
    }
}

#[test]
fn test_basic_container_with_render_trait() {
    let items = vec![
        TodoItem {
            text: "Learn Rust".to_string(),
            completed: false,
        },
        TodoItem {
            text: "Build hyperchad app".to_string(),
            completed: true,
        },
    ];

    let containers = container! {
        section align-items="Start" width="100%" {
            h1 { "Todo List" }
            "Here are your tasks:"
            @for item in items {
                (item)
            }
        }
    };

    // Verify we got containers
    assert!(
        !containers.is_empty(),
        "Should generate at least one container"
    );

    // Verify the HTML output contains expected content
    let html = containers[0].to_string();
    assert!(html.contains("Todo List"), "Should contain the title");
    assert!(
        html.contains("Learn Rust"),
        "Should contain first todo item"
    );
    assert!(
        html.contains("Build hyperchad app"),
        "Should contain second todo item"
    );
    assert!(html.contains("completed"), "Should show completed status");
}

#[test]
fn test_render_container_trait() {
    let item = TodoItem {
        text: "Test Task".to_string(),
        completed: true,
    };

    let mut containers = Vec::new();
    item.render_to(&mut containers)
        .expect("Should render successfully");

    assert_eq!(containers.len(), 1, "Should render exactly one container");

    let html = containers[0].to_string();
    assert!(html.contains("Test Task"), "Should contain task text");
    assert!(html.contains("completed"), "Should show completed status");
    assert!(html.contains("todo-item"), "Should have CSS class");
}
