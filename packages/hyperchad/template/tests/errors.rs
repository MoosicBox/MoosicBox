use trybuild::TestCases;

#[ignore]
#[test]
fn run_warnings() {
    let config = TestCases::new();
    config.compile_fail("tests/warnings/*.rs");
}
