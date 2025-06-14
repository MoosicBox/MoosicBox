// Make sure `std` is available but the prelude isn't
#![no_std]
extern crate std;

use std::vec::Vec;

use hyperchad_template2::container;

#[ignore]
#[test]
fn issue_170() {
    let number = 42;
    let _ = container! { (number) };
}
