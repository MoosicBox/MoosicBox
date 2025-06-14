use hyperchad_template2::*;

pub fn test_flex() -> Containers {
    container! {
        Div flex=(1) {
            "Test"
        }
    }
}

fn main() {
    let containers = test_flex();
    println!("Generated {} containers", containers.len());
}
