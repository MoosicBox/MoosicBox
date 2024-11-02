#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod server;

fn main() {
    server::serve()
}
