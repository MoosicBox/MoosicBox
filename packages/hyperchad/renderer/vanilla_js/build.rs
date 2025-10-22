use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;

fn main() {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let git_hash = String::from_utf8(output.stdout).unwrap().trim().to_string();

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir = PathBuf::from_str(&manifest_dir).unwrap();
    let web_dir = manifest_dir.join("web");
    let src_dir = web_dir.join("src");
    let dist_dir = web_dir.join("dist");
    let hash_dir = dist_dir.join(PLUGIN_HASH_HEX);
    let checksum_file = dist_dir.join(".checksum");

    println!(
        "cargo:rustc-env=HYPERCHAD_VANILLA_JS_EMBED_SCRIPT_DIR={}",
        hash_dir.display()
    );

    if hash_dir.is_dir() {
        std::fs::remove_dir_all(&hash_dir).unwrap();
    }

    println!("Bundling web...");

    let index = src_dir.join("index.ts");

    if index.is_file() {
        std::fs::remove_file(&index).unwrap();
    }

    let plugins: Vec<&str> = vec![
        #[cfg(all(not(feature = "plugin-uuid-insecure"), feature = "plugin-uuid"))]
        "uuid",
        #[cfg(feature = "plugin-uuid-insecure")]
        "uuid-insecure",
        #[cfg(feature = "plugin-idiomorph")]
        "idiomorph",
        #[cfg(not(feature = "plugin-idiomorph"))]
        "replace",
        #[cfg(feature = "plugin-nav")]
        "nav",
        #[cfg(feature = "plugin-sse")]
        "sse",
        #[cfg(feature = "plugin-tauri-event")]
        "tauri-event",
        #[cfg(feature = "plugin-routing")]
        "routing",
        #[cfg(feature = "plugin-event")]
        "event",
        #[cfg(feature = "plugin-canvas")]
        "canvas",
        #[cfg(feature = "plugin-form")]
        "form",
        #[cfg(feature = "plugin-http-events")]
        "actions-http-events",
        #[cfg(feature = "_actions")]
        "actions",
        #[cfg(feature = "plugin-actions-change")]
        "actions-change",
        #[cfg(feature = "plugin-actions-click")]
        "actions-click",
        #[cfg(feature = "plugin-actions-click-outside")]
        "actions-click-outside",
        #[cfg(feature = "plugin-actions-event")]
        "actions-event",
        #[cfg(feature = "plugin-actions-event-key-down")]
        "actions-event-key-down",
        #[cfg(feature = "plugin-actions-event-key-up")]
        "actions-event-key-up",
        #[cfg(feature = "plugin-actions-immediate")]
        "actions-immediate",
        #[cfg(feature = "plugin-actions-mouse-down")]
        "actions-mouse-down",
        #[cfg(feature = "plugin-actions-mouse-over")]
        "actions-mouse-over",
        #[cfg(feature = "plugin-actions-key-down")]
        "actions-key-down",
        #[cfg(feature = "plugin-actions-key-up")]
        "actions-key-up",
        #[cfg(feature = "plugin-actions-resize")]
        "actions-resize",
    ];

    std::fs::write(
        &index,
        format!(
            "import './core';\n{}\nconsole.debug('hyperchad.js {git_hash}');",
            plugins
                .into_iter()
                .map(|x| format!("import './{x}';\n"))
                .collect::<String>()
        ),
    )
    .unwrap();

    let resp = bundle(&index, &hash_dir);

    std::fs::remove_file(&index).unwrap();

    resp.unwrap();

    if !checksum_file.exists() {
        std::fs::File::options()
            .truncate(true)
            .write(true)
            .create(true)
            .open(&checksum_file)
            .unwrap();
    }

    let mut entries: Vec<_> = std::fs::read_dir(src_dir)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|x| x.file_name());

    for entry in entries {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }

    println!("cargo:rerun-if-changed={}", checksum_file.display());
    println!("cargo:rerun-if-changed=build.rs");
}

fn bundle(index: &Path, dist_dir: &Path) -> Result<(), &'static str> {
    println!("Bundling using swc...");
    hyperchad_js_bundler::swc::bundle(index, &dist_dir.join("index.js"), false);
    hyperchad_js_bundler::bundle(index, &dist_dir.join("index.min.js"));

    Ok(())
}
