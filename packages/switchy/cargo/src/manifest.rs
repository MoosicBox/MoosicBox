//! What a crate's manifest says about how simulations run it.
//!
//! ```toml
//! [package.metadata.switchy.simulation]
//! # Simulations link this crate but never run its code inside a run (it is linked for its
//! # types, or used only by the program around the runs):
//! runs = false
//! reason = "configuration types only; files are read before any run starts"
//!
//! # Or, for a crate they run:
//! only = ["plugin_state.rs"]            # only these files run (path prefixes under src/)
//! native = ["platform/unix/"]           # never built or run in simulation (prefixes)
//! exempt = ["ids.rs"]                   # the controlled versions themselves (exact files)
//! os-named-in = ["platform/"]           # where naming an operating system is allowed
//! forbid = ["Session::new("]            # patterns forbidden here on top of the shared ones
//! ```
//!
//! A crate without the table is scanned whole, so a crate a simulation starts linking is
//! checked from the start.

use std::path::Path;

use serde::Deserialize;

/// A crate's simulation rules, from `[package.metadata.switchy.simulation]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Rules {
    /// Whether simulations run the crate's code inside a run (the default).
    pub runs: bool,
    /// Why they do not, when `runs` is false.
    pub reason: Option<String>,
    /// Only these files run in simulations (path prefixes under `src/`).
    pub only: Vec<String>,
    /// Native backends, never built or run in simulation (path prefixes under `src/`).
    pub native: Vec<String>,
    /// Files exempt as a whole (exact paths under `src/`): the controlled versions themselves.
    pub exempt: Vec<String>,
    /// Where naming an operating system is allowed (path prefixes under `src/`).
    pub os_named_in: Vec<String>,
    /// Patterns forbidden in this crate on top of the shared ones.
    pub forbid: Vec<String>,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            runs: true,
            reason: None,
            only: Vec::new(),
            native: Vec::new(),
            exempt: Vec::new(),
            os_named_in: Vec::new(),
            forbid: Vec::new(),
        }
    }
}

impl Rules {
    /// The rules in a package's `metadata` (as `cargo metadata` reports it), checked against
    /// its sources under `src`.
    ///
    /// # Errors
    ///
    /// * If the table is malformed, contradicts itself, or names files that do not exist
    pub fn from_metadata(metadata: &serde_json::Value, src: &Path) -> Result<Self, String> {
        let Some(table) = metadata.get("switchy").and_then(|s| s.get("simulation")) else {
            return Ok(Self::default());
        };
        let rules: Self = serde_json::from_value(table.clone())
            .map_err(|e| format!("[package.metadata.switchy.simulation]: {e}"))?;
        rules.validate(src)?;
        Ok(rules)
    }

    fn validate(&self, src: &Path) -> Result<(), String> {
        if !self.runs {
            if self.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
                return Err("`runs = false` needs a `reason`".to_owned());
            }
            if !(self.only.is_empty()
                && self.native.is_empty()
                && self.exempt.is_empty()
                && self.os_named_in.is_empty()
                && self.forbid.is_empty())
            {
                return Err(
                    "`runs = false` scans nothing, so it takes no other rules".to_owned(),
                );
            }
            return Ok(());
        }
        if self.reason.is_some() {
            return Err("`reason` explains `runs = false`; this crate runs".to_owned());
        }
        // Prefixes and files must name something, so a rename cannot leave a rule behind
        // that silently covers nothing (or, for `only`, everything).
        let lists = [
            ("only", &self.only),
            ("native", &self.native),
            ("exempt", &self.exempt),
            ("os-named-in", &self.os_named_in),
        ];
        for (name, list) in lists {
            for entry in list {
                if !src.join(entry.trim_end_matches('/')).exists() {
                    return Err(format!("`{name}` names src/{entry}, which does not exist"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(metadata: &serde_json::Value) -> Result<Rules, String> {
        Rules::from_metadata(metadata, Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path())
    }

    #[test]
    fn a_crate_without_the_table_runs_and_is_scanned_whole() {
        assert_eq!(rules(&serde_json::json!(null)), Ok(Rules::default()));
        assert_eq!(rules(&serde_json::json!({ "other": 1 })), Ok(Rules::default()));
    }

    #[test]
    fn rules_are_read_from_kebab_case_keys() {
        let read = rules(&serde_json::json!({ "switchy": { "simulation": {
            "native": ["scan.rs"], "os-named-in": ["manifest.rs"], "forbid": ["x("]
        }}}))
        .unwrap();
        assert_eq!(read.native, ["scan.rs"]);
        assert_eq!(read.os_named_in, ["manifest.rs"]);
        assert_eq!(read.forbid, ["x("]);
    }

    #[test]
    fn a_crate_that_does_not_run_says_why_and_nothing_else() {
        let ok = serde_json::json!({ "switchy": { "simulation": { "runs": false, "reason": "types" }}});
        assert!(rules(&ok).is_ok());
        let no_reason = serde_json::json!({ "switchy": { "simulation": { "runs": false }}});
        assert!(rules(&no_reason).unwrap_err().contains("reason"));
        let more = serde_json::json!({ "switchy": { "simulation": {
            "runs": false, "reason": "types", "exempt": ["scan.rs"]
        }}});
        assert!(rules(&more).unwrap_err().contains("no other rules"));
    }

    #[test]
    fn rules_naming_missing_files_or_unknown_keys_are_errors() {
        let missing = serde_json::json!({ "switchy": { "simulation": { "exempt": ["gone.rs"] }}});
        assert!(rules(&missing).unwrap_err().contains("src/gone.rs"));
        let unknown = serde_json::json!({ "switchy": { "simulation": { "exempts": ["scan.rs"] }}});
        assert!(rules(&unknown).unwrap_err().contains("exempts"));
    }
}
