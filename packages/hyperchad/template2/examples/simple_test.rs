use hyperchad_template2::container;

fn main() {
    let containers = container! {
        Div {
            "Hello World"
        }
    };

    println!("Generated {} containers:", containers.len());
    for container in containers {
        println!("{}", container);
    }
}
