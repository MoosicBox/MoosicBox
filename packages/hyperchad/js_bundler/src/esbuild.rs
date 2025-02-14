use std::path::Path;

use crate::{
    node::{run_command, run_npm_command},
    MANIFEST_DIR,
};

pub fn bundle(target: &Path, out: &Path) {
    run_npm_command(&["install"], &MANIFEST_DIR);
    run_command(
        std::iter::once(
            MANIFEST_DIR
                .join("node_modules")
                .join(".bin")
                .join("esbuild")
                .to_str()
                .unwrap()
                .to_string(),
        ),
        &[
            target.to_str().unwrap(),
            "--minify",
            "--bundle",
            &format!("--outfile={}", out.display()),
        ],
        &MANIFEST_DIR,
    );
}
