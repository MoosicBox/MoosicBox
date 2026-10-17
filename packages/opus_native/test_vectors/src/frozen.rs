//! Complete mandatory corpus identity, independent of generator output.
use crate::format;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read as _,
    path::{Component, Path},
};

/// Reviewed SHA-256 of the complete source-controlled manifest.
pub const FROZEN_MANIFEST_SHA256: &str =
    "8361b967ee0ac2fc4385e3b52d46dafd96921077aea3b23928b45f3d6251e8c3";
const MANIFEST: &[u8] = include_bytes!("../frozen/manifest.json");
fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            write!(text, "{byte:02x}").expect("writing string");
            text
        })
}
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|p| matches!(p, Component::Normal(_)))
}
// Manifest is embedded, not opened from an untrusted corpus directory.
const MAX_MANIFEST_BYTES: usize = 128 * 1024;
fn read(root: &Path, relative: &str, expected_len: u64) -> Result<Vec<u8>, String> {
    if !safe(relative) {
        return Err("Unsafe frozen corpus path".into());
    }
    let root_metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("Frozen corpus root is not a real directory".into());
    }
    let mut path = root.to_path_buf();
    let mut components = Path::new(relative).components().peekable();
    while let Some(part) = components.next() {
        path.push(part);
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink()
            || if components.peek().is_some() {
                !metadata.is_dir()
            } else {
                !metadata.is_file()
            }
        {
            return Err("Nonregular file or unsafe directory in frozen corpus path".into());
        }
    }
    let before = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !before.is_file() || before.len() != expected_len {
        return Err(format!("Frozen file type/length mismatch: {relative}"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Linux UAPI O_NONBLOCK | O_NOFOLLOW. No libc dependency/unsafe FFI.
        // A FIFO swapped in after lstat must not block open; fstat rejects it.
        options.custom_flags(0x800 | 0x2_0000);
    }
    let file = options.open(&path).map_err(|e| e.to_string())?;
    let opened = file.metadata().map_err(|e| e.to_string())?;
    if !opened.is_file() || opened.len() != expected_len {
        return Err(format!(
            "Opened frozen file type/length mismatch: {relative}"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if before.dev() != opened.dev() || before.ino() != opened.ino() {
            return Err("Frozen file replaced during open".into());
        }
    }
    read_bounded(file, expected_len)
}
fn read_bounded(reader: impl std::io::Read, expected_len: u64) -> Result<Vec<u8>, String> {
    let limit = expected_len
        .checked_add(1)
        .ok_or("Frozen file size overflow")?;
    let mut bytes = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != expected_len {
        return Err("Frozen file changed length while reading".into());
    }
    Ok(bytes)
}

fn json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

/// Verifies all 43 mandatory cases / 58 packets against the reviewed SHA-256 manifest.
///
/// Checks complete inventory, exact bytes, mode IDs, TOCs, PCM sizes and packet counts.
/// No native codec implementation or successfully-loaded-case discovery is used.
///
/// # Errors
/// * Rejects missing, changed, empty or unsafe files, symlink paths, incomplete inventory,
///   manifest tampering, invalid TOC labels/durations, or incorrect raw PCM lengths.
/// * Rejects nonregular files; reads at most pinned length plus one byte.
///   The embedded manifest is capped before hashing/parsing. Linux x86/x86-64
///   and `AArch64` also use nonblocking no-follow opens; Unix checks opened inode
///   identity. Other platforms retain pre-open and post-open type validation.
pub fn verify_frozen_corpus(root: &Path) -> Result<(), String> {
    verify(root, MANIFEST)
}
#[allow(clippy::too_many_lines)] // One fail-closed mandatory coverage validation.
fn verify(root: &Path, manifest_bytes: &[u8]) -> Result<(), String> {
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err("Frozen manifest exceeds size cap".into());
    }
    if digest(manifest_bytes) != FROZEN_MANIFEST_SHA256 {
        return Err("Frozen manifest SHA-256 mismatch".into());
    }
    let manifest = json(manifest_bytes)?;
    if manifest["format_version"] != 1
        || manifest["reference_revision"] != format::REFERENCE_REVISION
        || manifest["reference_config"] != format::REFERENCE_CONFIG
    {
        return Err("Frozen provenance mismatch".into());
    }
    let files = manifest["files"]
        .as_object()
        .ok_or("Missing frozen file manifest")?;
    let read = |relative: &str| {
        let length = files
            .get(relative)
            .and_then(|entry| entry["bytes"].as_u64())
            .ok_or("Missing pinned file length")?;
        read(root, relative, length)
    };
    for (path, expected) in files {
        let bytes = read(path)?;
        if expected["bytes"].as_u64() != Some(bytes.len() as u64)
            || expected["sha256"] != digest(&bytes)
        {
            return Err(format!("Frozen corpus identity mismatch: {path}"));
        }
    }
    let inventory = json(&read("inventory.json")?)?;
    if inventory["format_version"] != 1 || inventory["cases"] != manifest["cases"] {
        return Err("Mandatory inventory differs from freeze".into());
    }
    let mut expected_files =
        BTreeSet::from(["inventory.json".to_owned(), "provenance.json".to_owned()]);
    let mut packets = 0;
    for case in inventory["cases"].as_array().ok_or("Missing cases")? {
        let id = case["id"].as_str().ok_or("Missing case ID")?;
        if case["path"] != id || !safe(id) {
            return Err("Unsafe/noncanonical case ID".into());
        }
        let count = case["packet_count"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or("Empty case packets")?;
        let sequence = case["kind"] == "sequence";
        if sequence {
            let path = format!("{id}/sequence.json");
            let seq = json(&read(&path)?)?;
            if seq["packets"].as_array().map(Vec::len)
                != Some(usize::try_from(count).map_err(|e| e.to_string())?)
            {
                return Err("Sequence count mismatch".into());
            }
            expected_files.insert(path);
        } else if case["kind"] != "single" || count != 1 {
            return Err("Invalid case kind/count".into());
        }
        for index in 0..count {
            let dir = if index == 0 {
                id.to_owned()
            } else {
                format!("{id}/packet_{index:03}")
            };
            let meta = json(&read(&format!("{dir}/meta.json"))?)?;
            let packet = read(&format!("{dir}/packet.bin"))?;
            let pcm = read(&format!("{dir}/expected.pcm"))?;
            let mode = meta["mode"].as_str().ok_or("Missing mode")?;
            if (if sequence {
                !id.strip_prefix("sequences/")
                    .is_some_and(|name| name.starts_with(&format!("{mode}_")))
            } else {
                id.split('/').next() != Some(mode)
            }) || meta["packet_index"] != index
                || meta["pcm_semantics"] != "raw_decoder_output"
            {
                return Err("Mode ID or raw packet metadata mismatch".into());
            }
            let samples = usize::try_from(meta["frame_samples"].as_u64().ok_or("Missing samples")?)
                .map_err(|e| e.to_string())?;
            let channels = u8::try_from(meta["channels"].as_u64().ok_or("Missing channels")?)
                .map_err(|e| e.to_string())?;
            let rate = u32::try_from(meta["sample_rate"].as_u64().ok_or("Missing rate")?)
                .map_err(|e| e.to_string())?;
            format::validate_toc(
                &packet,
                mode,
                meta["bandwidth"].as_str().ok_or("Missing bandwidth")?,
                rate,
                channels,
                samples,
            )?;
            if samples
                .checked_mul(usize::from(channels))
                .and_then(|n| n.checked_mul(2))
                != Some(pcm.len())
                || pcm.is_empty()
            {
                return Err("Frozen raw PCM length mismatch".into());
            }
            for name in ["meta.json", "packet.bin", "expected.pcm"] {
                expected_files.insert(format!("{dir}/{name}"));
            }
            packets += 1;
        }
    }
    if packets != 58
        || manifest["packet_count"] != packets
        || files.keys().cloned().collect::<BTreeSet<_>>() != expected_files
    {
        return Err("Incomplete frozen manifest coverage".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_freeze_rejects_single_sequence_and_metadata_tampering() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!(".freeze-test-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let manifest = json(MANIFEST).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("frozen/corpus");
        for relative in manifest["files"].as_object().unwrap().keys() {
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(source.join(relative), destination).unwrap();
        }
        assert!(verify_frozen_corpus(&root).is_ok());
        for relative in [
            "silk/mb/mixed_stereo/packet.bin",
            "silk/mb/mixed_stereo/expected.pcm",
            "sequences/celt_fb_mono/packet_001/packet.bin",
            "sequences/celt_fb_mono/packet_001/expected.pcm",
            "sequences/celt_fb_mono/sequence.json",
        ] {
            let path = root.join(relative);
            let bytes = fs::read(&path).unwrap();
            let mut changed = bytes.clone();
            changed[0] ^= 1;
            fs::write(&path, changed).unwrap();
            assert!(verify_frozen_corpus(&root).is_err());
            fs::write(&path, []).unwrap();
            assert!(verify_frozen_corpus(&root).is_err());
            fs::remove_file(&path).unwrap();
            assert!(verify_frozen_corpus(&root).is_err());
            fs::write(path, bytes).unwrap();
        }
        for (relative, field, value) in [
            (
                "silk/mb/mixed_stereo/meta.json",
                "mode",
                serde_json::json!("celt"),
            ),
            (
                "silk/mb/mixed_stereo/meta.json",
                "frame_samples",
                serde_json::json!(0),
            ),
            (
                "sequences/celt_fb_mono/sequence.json",
                "packets",
                serde_json::json!([]),
            ),
            ("inventory.json", "cases", serde_json::json!([])),
        ] {
            let path = root.join(relative);
            let bytes = fs::read(&path).unwrap();
            let mut changed = json(&bytes).unwrap();
            changed[field] = value;
            fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
            assert!(verify_frozen_corpus(&root).is_err());
            fs::write(path, bytes).unwrap();
        }
        for relative in [
            "silk/mb/mixed_stereo/packet.bin",
            "silk/mb/mixed_stereo/expected.pcm",
            "sequences/celt_fb_mono/packet_001/packet.bin",
            "sequences/celt_fb_mono/packet_001/expected.pcm",
        ] {
            let mut tampered = manifest.clone();
            tampered["files"][relative]["sha256"] = serde_json::json!("0".repeat(64));
            assert!(verify(&root, &serde_json::to_vec(&tampered).unwrap()).is_err());
            tampered["files"].as_object_mut().unwrap().remove(relative);
            assert!(verify(&root, &serde_json::to_vec(&tampered).unwrap()).is_err());
        }
        #[cfg(unix)]
        {
            let path = root.join("silk/mb/mixed_stereo/packet.bin");
            let bytes = fs::read(&path).unwrap();
            fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(source.join("silk/mb/mixed_stereo/packet.bin"), &path)
                .unwrap();
            assert!(verify_frozen_corpus(&root).is_err());
            fs::remove_file(&path).unwrap();
            fs::write(&path, bytes).unwrap();
        }
        assert!(format::validate_toc(&[0], "celt", "nb", 8000, 1, 160).is_err());
        assert!(format::validate_toc(&[], "silk", "nb", 8000, 1, 160).is_err());
        assert!(format::validate_toc(&[3, 0], "silk", "nb", 8000, 1, 160).is_err());
        assert!(verify_frozen_corpus(&root).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn manifest_and_paths_reject_tampering() {
        let mut bytes = MANIFEST.to_vec();
        bytes[0] ^= 1;
        assert!(verify(Path::new("."), &bytes).is_err());
        for path in ["", "/tmp/x", "../escape", "x/../y", "a\\b"] {
            assert!(!safe(path));
        }
    }
}

#[cfg(test)]
#[path = "frozen/bounded_tests.rs"]
mod bounded_tests;
