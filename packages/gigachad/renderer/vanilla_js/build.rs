use std::env;
use std::path::PathBuf;
use std::str::FromStr;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir = PathBuf::from_str(&manifest_dir).unwrap();
    let web_dir = manifest_dir.join("web");
    let src_dir = web_dir.join("src");
    let dist_dir = web_dir.join("dist");

    if dist_dir.is_dir() {
        std::fs::remove_dir_all(&dist_dir).unwrap();
    }

    println!("Bundling web...");

    if cfg!(feature = "swc") {
        println!("Bundling using swc...");
        gigachad_js_bundler::swc::bundle(
            &src_dir.join("index.ts"),
            &dist_dir.join("index.js"),
            false,
        );
        gigachad_js_bundler::bundle(&src_dir.join("index.ts"), &dist_dir.join("index.min.js"));
    } else if cfg!(feature = "esbuild") {
        println!("Bundling using esbuild...");
        gigachad_js_bundler::node::run_npm_command(&["install"], &web_dir);
        gigachad_js_bundler::node::run_npm_command(&["build"], &web_dir);
        gigachad_js_bundler::bundle(&dist_dir.join("index.js"), &dist_dir.join("index.min.js"));
    } else {
        panic!("Invalid features specified for gigachad_renderer_vanilla_js build. Requires at least `swc` or `esbuild`");
    }

    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", dist_dir.display());
    println!("cargo:rerun-if-changed=build.rs");
}
