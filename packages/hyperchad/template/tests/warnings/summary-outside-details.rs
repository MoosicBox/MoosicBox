use hyperchad_template::container;

fn main() {
    // Should fail: summary outside details
    let _result = container! {
        div {
            summary { "Invalid" }
        }
    };
}
