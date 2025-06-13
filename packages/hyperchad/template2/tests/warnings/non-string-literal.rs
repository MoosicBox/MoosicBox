use hyperchad_template2::container;

fn main() {
    container! {
        42
        42usize
        42.0
        'a'
        b"a"
        b'a'

        // `true` and `false` are only considered literals in attribute values
        input disabled=true;
        input disabled=false;
    };
}
