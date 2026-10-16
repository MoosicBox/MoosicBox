
use {
    x as y
};
type logging = u64;
fn main() {
    let func = |log: u32| {
        log as logging
    };
    func(42);
}

