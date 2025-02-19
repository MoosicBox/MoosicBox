use std::env;
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir = PathBuf::from_str(&manifest_dir).unwrap();
    let web_dir = manifest_dir.join("web");
    let src_dir = web_dir.join("src");
    let dist_dir = web_dir.join("dist");
    let checksum_file = dist_dir.join(".checksum");

    if dist_dir.is_dir() {
        remove_all_except(&dist_dir, &checksum_file).unwrap();
    }

    println!("Bundling web...");

    if cfg!(feature = "swc") {
        println!("Bundling using swc...");
        hyperchad_js_bundler::swc::bundle(
            &src_dir.join("index.ts"),
            &dist_dir.join("index.js"),
            false,
        );
        hyperchad_js_bundler::bundle(&src_dir.join("index.ts"), &dist_dir.join("index.min.js"));
    } else if cfg!(feature = "esbuild") {
        println!("Bundling using esbuild...");
        hyperchad_js_bundler::node::run_npm_command(&["install"], &web_dir);
        hyperchad_js_bundler::node::run_npm_command(&["build"], &web_dir);
        hyperchad_js_bundler::bundle(&dist_dir.join("index.js"), &dist_dir.join("index.min.js"));
    } else {
        panic!("Invalid features specified for hyperchad_renderer_vanilla_js build. Requires at least `swc` or `esbuild`");
    }

    if !checksum_file.exists() {
        std::fs::File::options()
            .truncate(true)
            .write(true)
            .create(true)
            .open(&checksum_file)
            .unwrap();
    }

    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-changed={}", checksum_file.display());
    println!("cargo:rerun-if-changed=build.rs");
}

fn remove_all_except(path: &Path, except: &Path) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(path)?.filter_map(Result::ok) {
        let path = entry.path();
        if path != except {
            if Path::is_dir(&path) {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
    }

    Ok(())
}
