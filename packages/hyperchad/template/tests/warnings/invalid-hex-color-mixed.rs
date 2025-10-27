use hyperchad_template::container;

fn main() {
    container! {
        div {
            div color=#1a { "2-digit hex - invalid" }
            div color=#5abc { "4-digit hex - invalid" }
            div color=#1234567 { "7-digit hex - invalid" }
            div color=#123456789 { "9-digit hex - invalid" }
        }
    };
}
