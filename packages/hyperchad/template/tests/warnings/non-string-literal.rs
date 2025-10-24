use hyperchad_template::container;

fn main() {
    container! {
        42
        42usize
        42.0
        'a'
        b"a"
        b'a'

        // `true` and `false` are only considered literals in attribute values
        input type=text disabled=true;
        input type=text disabled=false;
    };
}
