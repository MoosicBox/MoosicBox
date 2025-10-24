#[cfg(all(test, nightly))]
#[test]
fn run_warnings() {
    let config = trybuild::TestCases::new();
    config.compile_fail("tests/warnings/*.rs");
}
