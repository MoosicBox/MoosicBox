use hyperchad_template2::{RenderContainer, container};

struct TodoItem {
    text: String,
    completed: bool,
}

impl RenderContainer for TodoItem {
    type Error = core::fmt::Error;

    fn render_to(
        &self,
        containers: &mut Vec<hyperchad_transformer::Container>,
    ) -> Result<(), Self::Error> {
        let rendered = container! {
            Div .todo-item {
                "Task: "
                Span { (self.text.clone()) }
                @if self.completed {
                    " (completed)"
                }
            }
        };
        containers.extend(rendered);
        Ok(())
    }
}

fn main() {
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
        Section align-items="Start" width="100%" {
            H1 { "Todo List" }
            "Here are your tasks:"
            @for item in items {
                (item)
            }
        }
    };

    println!("{}", containers[0]);
}
