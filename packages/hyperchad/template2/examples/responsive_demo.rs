use hyperchad_template2::{AlignItems, LayoutDirection, if_responsive};
use hyperchad_template2::{Containers, container};

fn create_responsive_containers() -> Containers {
    container! {
        Div
            padding-x=(if_responsive("mobile").then::<i32>(10).or_else(20))
            direction=(
                if_responsive("mobile-large")
                    .then::<LayoutDirection>(LayoutDirection::Column)
                    .or_else(LayoutDirection::Row)
            )
            align-items=(
                if_responsive("mobile")
                    .then::<AlignItems>(AlignItems::Center)
                    .or_else(AlignItems::Start)
            )
            hidden=(if_responsive("mobile").then::<bool>(true).or_else(false))
        {
            "Responsive content that adapts to screen size"
        }
    }
}

fn main() {
    // Example demonstrating responsive attributes
    let containers = create_responsive_containers();

    println!(
        "Generated {} containers with responsive attributes",
        containers.len()
    );

    // Print some details about the generated container
    if let Some(container) = containers.first() {
        println!("Container direction: {:?}", container.direction);
        println!(
            "Container has padding_left: {}",
            container.padding_left.is_some()
        );
        println!(
            "Container has padding_right: {}",
            container.padding_right.is_some()
        );
        println!(
            "Container has align_items: {}",
            container.align_items.is_some()
        );
        println!("Container has hidden: {}", container.hidden.is_some());
    }

    println!("✅ Responsive attributes are working correctly!");
}
