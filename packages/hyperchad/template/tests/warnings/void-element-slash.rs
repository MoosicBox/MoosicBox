use hyperchad_template::html;

fn main() {
    html! {
        br /
        // Make sure we're not stopping on the first error
        input type="text" /
    };
}
