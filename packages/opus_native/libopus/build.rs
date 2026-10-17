use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, process::Command};
#[path = "src/qualification.rs"]
mod qualification;

fn main() {
    let source = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("opus");
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .output()
        .expect("git required");
    assert!(revision.status.success());
    let revision = String::from_utf8(revision.stdout).unwrap();
    let clean = Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--", "."])
        .current_dir(&source)
        .status()
        .expect("verify source")
        .success();
    assert_eq!(revision.trim(), qualification::REVISION);
    assert!(clean, "reference source contains tracked modifications");
    let dst = cmake::Config::new(&source)
        .define("CMAKE_EXPORT_COMPILE_COMMANDS", "ON")
        .define("OPUS_BUILD_PROGRAMS", "OFF")
        .define("OPUS_BUILD_TESTING", "OFF")
        .define("OPUS_BUILD_SHARED_LIBRARY", "OFF")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("OPUS_DISABLE_INTRINSICS", "ON")
        .define("OPUS_ENABLE_FLOAT_API", "OFF")
        .define("OPUS_FIXED_POINT", "ON")
        .build();
    let build = dst.join("build");
    let config = fs::read_to_string(build.join("config.h")).expect("effective config.h");
    let cache = fs::read_to_string(build.join("CMakeCache.txt")).expect("effective CMakeCache");
    let commands_text = fs::read_to_string(build.join("compile_commands.json")).expect(
        "qualification requires a CMake Makefiles or Ninja generator with compile_commands.json",
    );
    let commands: serde_json::Value = serde_json::from_str(&commands_text).unwrap();
    let mut effective_config = config.clone();
    for entry in commands.as_array().expect("compile database") {
        let command = entry["command"].as_str().expect("compile command");
        effective_config = qualification::qualify_command(&config, &cache, command)
            .expect("qualified effective compile command");
    }
    assert!(
        !commands.as_array().unwrap().is_empty(),
        "empty compile database"
    );
    let lib_dir = if dst.join("lib64").exists() {
        dst.join("lib64")
    } else {
        dst.join("lib")
    };
    let archive = lib_dir.join(if env::var("CARGO_CFG_TARGET_ENV").unwrap() == "msvc" {
        "opus.lib"
    } else {
        "libopus.a"
    });
    let hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let mut compiler_metadata = std::collections::BTreeMap::new();
    for entry in fs::read_dir(build.join("CMakeFiles")).unwrap() {
        let path = entry.unwrap().path().join("CMakeCCompiler.cmake");
        if path.is_file() {
            compiler_metadata.insert(
                path.display().to_string(),
                fs::read_to_string(path).unwrap(),
            );
        }
    }
    assert!(!compiler_metadata.is_empty(), "missing C compiler identity");
    let metadata = serde_json::json!({
        "compiler_metadata": compiler_metadata,
        "schema": "moosicbox.opus.reference.v1", "revision": revision.trim(), "tracked_clean": clean,
        "source": source, "archive": archive, "build": build,
        "archive_sha256": hash(&fs::read(&archive).expect("Cargo-built archive")),
        "config_sha256": hash(config.as_bytes()), "cache_sha256": hash(cache.as_bytes()),
        "config": config, "effective_config": effective_config, "cmake_cache": cache,
        "compile_commands": commands, "compile_commands_sha256": hash(commands_text.as_bytes()),
        "target": env::var("TARGET").unwrap(), "host": env::var("HOST").unwrap(),
        "profile": env::var("PROFILE").unwrap(),
        "fixed_point": true, "res24": false, "qext": false, "intrinsics": false, "float_api": false
    });
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("reference.json"),
        metadata.to_string(),
    )
    .unwrap();
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=opus");
    if env::var("CARGO_CFG_TARGET_FAMILY")
        .unwrap()
        .split(',')
        .any(|x| x == "unix")
    {
        println!("cargo:rustc-link-lib=dylib=m");
    }
    println!("cargo:rerun-if-changed=opus/");
    println!("cargo:rerun-if-changed=src/qualification.rs");
}
