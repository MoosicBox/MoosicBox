use hyperchad_template::container;

fn main() {
    container! {
        span.@let x = 1; {
            (x)
        }
    };
}
