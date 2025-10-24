use hyperchad_template::container;

fn main() {
    container! {
        image src="test.png" /
        // Make sure we're not stopping on the first error
        input type="text" /
    };
}
