use hyperchad_template::container;

fn main() {
    // Should fail: summary must be first child of details
    let _result = container! {
        details {
            div { "First child" }
            summary { "Not first!" }
        }
    };
}
