use hyperchad_template::container;

fn main() {
    let name = "href";
    container! {
        a (name)="about:blank" {}
    };
}
