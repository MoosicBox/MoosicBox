use hyperchad_template2::{Containers, container};

#[cfg(feature = "logic")]
use hyperchad_template2::if_responsive;

fn simple_test() -> Containers {
    container! {
        Div width=(100) {
            "Simple test"
        }
    }
}

#[cfg(feature = "logic")]
fn responsive_test() -> Containers {
    container! {
        Div width=(if_responsive("mobile").then::<i32>(50).or_else(100)) {
            "Responsive test"
        }
    }
}

fn main() {
    let containers = simple_test();
    println!("Generated {} containers", containers.len());
    println!("✅ Basic functionality works!");

    #[cfg(feature = "logic")]
    {
        let responsive_containers = responsive_test();
        println!(
            "Generated {} responsive containers",
            responsive_containers.len()
        );
        println!("✅ Responsive functionality works!");
    }
}
