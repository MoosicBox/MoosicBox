use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let opus_source = manifest_dir.join("opus");

    const REVISION: &str = "34bba701ae97c913de719b1f7c10686f62cddb15";
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&opus_source)
        .output()
        .expect("git required to verify reference provenance");
    assert!(revision.status.success());
    assert_eq!(
        String::from_utf8(revision.stdout).unwrap().trim(),
        REVISION,
        "Reference source revision changed; explicitly update provenance"
    );
    assert!(
        std::process::Command::new("git")
            .args(["diff", "--quiet", "HEAD", "--", "."])
            .current_dir(&opus_source)
            .status()
            .expect("verify reference tree")
            .success(),
        "Reference source contains tracked modifications"
    );

    let mut config = cmake::Config::new(&opus_source);

    let dst = config
        .define("OPUS_BUILD_PROGRAMS", "OFF")
        .define("OPUS_BUILD_TESTING", "OFF")
        .define("OPUS_BUILD_SHARED_LIBRARY", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("OPUS_DISABLE_INTRINSICS", "ON")
        .define("OPUS_ENABLE_FLOAT_API", "OFF")
        .define("OPUS_FIXED_POINT", "ON") // Force fixed-point mode for bit-exact decoding
        .build();

    let lib_dir = if dst.join("lib64").exists() {
        dst.join("lib64")
    } else {
        dst.join("lib")
    };

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=opus");

    #[cfg(unix)]
    println!("cargo:rustc-link-lib=dylib=m");

    println!("cargo:rerun-if-changed=opus/");
}
