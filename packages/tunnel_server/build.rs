use std::process::Command;
fn main() {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=GIT_HASH={git_hash}");

    // Provide a default TUNNEL_ACCESS_TOKEN for tests if not set
    if std::env::var("TUNNEL_ACCESS_TOKEN").is_err() {
        println!("cargo:rustc-env=TUNNEL_ACCESS_TOKEN=test_token_for_unit_tests");
    }
}
