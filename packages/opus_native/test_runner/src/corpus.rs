//! Inventory and persistent-stream adapter for the versioned vector contract.
use super::{Result, TestVector, hash, load, read};
use moosicbox_opus_native_test_vectors::TestSequence;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

pub struct Entry {
    pub path: String,
    pub sequence: bool,
    pub count: usize,
}

fn relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub fn inventory(root: &Path) -> Result<(BTreeMap<String, Entry>, Value)> {
    let inventory: Value = serde_json::from_slice(&read(&root.join("inventory.json"))?)
        .map_err(|error| error.to_string())?;
    let provenance: Value = serde_json::from_slice(&read(&root.join("provenance.json"))?)
        .map_err(|error| error.to_string())?;
    if inventory["format_version"] != 1
        || provenance["format_version"] != 1
        || provenance["pcm_semantics"] != "raw_decoder_output"
        || provenance["fixed_point"] != true
        || provenance["intrinsics"] != false
        || provenance["float_api"] != false
        || provenance["checksum_algorithm"] != "fnv1a64"
        || provenance["reference_revision"]
            != moosicbox_opus_native_test_vectors::REFERENCE_REVISION
        || provenance["generator"] != "moosicbox_opus_native_test_vectors/build.rs:v2"
    {
        return Err("missing or unsupported inventory/provenance version".into());
    }
    let cases = inventory["cases"]
        .as_array()
        .ok_or("missing inventory cases")?;
    let mut entries = BTreeMap::new();
    for case in cases {
        let id = case["id"].as_str().ok_or("missing inventory ID")?;
        let path = case["path"].as_str().ok_or("missing inventory path")?;
        if !relative(id) || !relative(path) || id != path {
            return Err("noncanonical inventory ID/path".into());
        }
        let sequence = match case["kind"].as_str() {
            Some("single") => false,
            Some("sequence") => true,
            _ => return Err("unknown inventory kind".into()),
        };
        let count = usize::try_from(
            case["packet_count"]
                .as_u64()
                .ok_or("missing packet_count")?,
        )
        .map_err(|error| error.to_string())?;
        if count == 0
            || (!sequence && count != 1)
            || entries
                .insert(
                    id.to_owned(),
                    Entry {
                        path: path.into(),
                        sequence,
                        count,
                    },
                )
                .is_some()
        {
            return Err("empty, duplicate, or invalid inventory entry".into());
        }
    }
    if entries.len() != super::required::CASES.len()
        || super::required::CASES.iter().any(|(id, sequence, count)| {
            entries
                .get(*id)
                .is_none_or(|entry| entry.sequence != *sequence || entry.count != *count)
        })
    {
        return Err(
            "inventory differs from frozen built-in corpus v1 (required IDs/kinds/counts)".into(),
        );
    }
    Ok((entries, provenance))
}

pub fn packets(root: &Path, id: &str, entry: &Entry) -> Result<(Vec<TestVector>, String)> {
    let path = root.join(&entry.path);
    if entry.sequence {
        let sequence = TestSequence::load(&path).map_err(|error| error.to_string())?;
        if sequence.packets.len() != entry.count {
            return Err("sequence packet count disagrees with inventory".into());
        }
        let mut bytes = read(&path.join("sequence.json"))?;
        for index in 0..entry.count {
            let packet = if index == 0 {
                path.clone()
            } else {
                path.join(format!("packet_{index:03}"))
            };
            let meta: Value = serde_json::from_slice(&read(&packet.join("meta.json"))?)
                .map_err(|error| error.to_string())?;
            if meta["pcm_semantics"] != "raw_decoder_output"
                || meta["frame_samples"]
                    .as_u64()
                    .is_none_or(|count| count == 0)
            {
                return Err("empty or non-raw sequence PCM".into());
            }
            for file in ["packet.bin", "expected.pcm", "meta.json"] {
                let data = read(&packet.join(file))?;
                bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
                bytes.extend_from_slice(&data);
            }
        }
        Ok((sequence.packets, hash(&bytes)))
    } else {
        let (vector, identity, _) = load(root, id)?;
        Ok((vec![vector], identity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_cannot_escape_corpus() {
        for path in ["", "../x", "/tmp/x", "a/../b", "."] {
            assert!(!relative(path));
        }
        assert!(relative("sequences/celt_fb_mono"));
    }
}
