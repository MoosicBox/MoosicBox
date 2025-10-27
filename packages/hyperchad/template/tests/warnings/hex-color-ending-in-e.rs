use hyperchad_template::container;

fn main() {
    // This should fail at the lexer level with a helpful message
    let _ = container! {
        div color=#12e { "test" }
    };
}
