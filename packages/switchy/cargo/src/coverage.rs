//! Evidence that the crates marked `runs = false` do not run: what a simulation's
//! coverage-instrumented runs executed.
//!
//! A crate's `runs = false` (an external crate's `not-run` audit) says simulations link it but
//! never run it inside a run, so it is not scanned (not read). That is a claim the scan cannot
//! check. A build instrumented for coverage
//! (`-C instrument-coverage`) can: run the simulation's fleets, merge the profiles
//! (`llvm-profdata merge`), export a summary (`llvm-cov export -summary-only`), and
//! [`Executed::from_llvm_cov_export`] reads which lines of which files ran.
//! [`crate::Check::run_claims`] then reports every file of a `runs = false` crate that ran.
//!
//! The profile covers the whole process, so a line counts even if it ran outside every run
//! (before the first, say): stricter than the claim, never looser.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// How many lines of each source file ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Executed {
    lines: BTreeMap<PathBuf, u64>,
}

impl Executed {
    /// Read an `llvm-cov export -summary-only` (JSON) of the instrumented runs.
    ///
    /// # Errors
    ///
    /// * If `json` is not such an export
    pub fn from_llvm_cov_export(json: &str) -> Result<Self, String> {
        let export: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("not JSON: {e}"))?;
        let data = export
            .get("data")
            .and_then(serde_json::Value::as_array)
            .ok_or("not an llvm-cov export: no `data`")?;
        let mut lines = BTreeMap::new();
        for file in data
            .iter()
            .filter_map(|unit| unit.get("files").and_then(serde_json::Value::as_array))
            .flatten()
        {
            let name = file
                .get("filename")
                .and_then(serde_json::Value::as_str)
                .ok_or("an exported file has no `filename`")?;
            let covered = file
                .pointer("/summary/lines/covered")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("{name}: no line summary"))?;
            *lines.entry(PathBuf::from(name)).or_insert(0) += covered;
        }
        Ok(Self { lines })
    }

    /// The files under `dir` that ran, with how many of their lines did.
    #[must_use]
    pub fn under(&self, dir: &Path) -> Vec<(PathBuf, u64)> {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        self.lines
            .iter()
            .filter(|(file, lines)| **lines > 0 && file.starts_with(&dir))
            .map(|(file, lines)| (file.clone(), *lines))
            .collect()
    }

    /// How many files the export covers.
    #[must_use]
    pub fn files(&self) -> usize {
        self.lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_are_read_per_file() {
        let dir = std::env::temp_dir().canonicalize().unwrap();
        let ran = dir.join("ran/src/lib.rs");
        let idle = dir.join("idle/src/lib.rs");
        let json = serde_json::json!({
            "data": [{"files": [
                {"filename": ran, "summary": {"lines": {"count": 10, "covered": 3}}},
                {"filename": idle, "summary": {"lines": {"count": 10, "covered": 0}}},
            ]}],
            "type": "llvm.coverage.json.export",
        })
        .to_string();
        let executed = Executed::from_llvm_cov_export(&json).unwrap();
        assert_eq!(executed.files(), 2);
        assert_eq!(executed.under(&dir.join("ran")), [(ran, 3)]);
        assert_eq!(executed.under(&dir.join("idle")), []);
    }

    #[test]
    fn other_input_is_an_error() {
        assert!(Executed::from_llvm_cov_export("{}").is_err());
        assert!(Executed::from_llvm_cov_export("not json").is_err());
    }
}
