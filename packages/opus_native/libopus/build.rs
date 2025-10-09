use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let opus_source = manifest_dir.join("opus");

    let mut config = cmake::Config::new(&opus_source);

    let dst = config
        .define("OPUS_BUILD_PROGRAMS", "OFF")
        .define("OPUS_BUILD_TESTING", "OFF")
        .define("OPUS_BUILD_SHARED_LIBRARY", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
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
