use hyperchad_template::container;

fn main() {
    container! {
        p.@let x = 1; {
            (x)
        }
    };
}
