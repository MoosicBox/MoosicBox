//! A simulation workspace's determinism configuration.
//!
//! It is the `determinism.toml` beside the workspace's `Cargo.toml`: patterns forbidden in every
//! crate it runs, and the audit of every crate it links from outside the workspace.
//!
//! ```toml
//! # Forbidden in every scanned crate, on top of switchy_cargo's own list.
//! forbid = ["my_crate::real_clock"]
//!
//! [[audit]]
//! name = "uuid"
//! version = "^1"          # a semver requirement (default: any version)
//! verdict = "restricted"
//! reason = "new_v4 reads OS randomness; the shared rules forbid it"
//! ```
//!
//! Every crate from a registry or git repository that a simulation links must match an
//! audit entry, as with `cargo vet`: a new crate, or a version outside its entry's
//! requirement, fails the check until someone audits it.

use serde::Deserialize;

/// What an audit found a crate to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Deterministic in itself: no clocks, randomness, threads, I/O or process-wide state a
    /// run could observe.
    Pure,
    /// A simulator: the switchy crates and simvar, which provide the controlled versions.
    Controlled,
    /// Has entry points no seed controls, which the forbidden patterns ban (the reason names
    /// them).
    Restricted,
    /// Linked, but none of its code runs in the simulation (the reason says why): like a local
    /// crate's `runs = false`, a claim [`crate::Check::run_claims`] checks against coverage.
    NotRun,
    /// Not audited yet: accepted as it was when the list was made (the reason says when).
    Unaudited,
}

/// One audit entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audit {
    /// The crate's name.
    pub name: String,
    /// The versions it covers, as a semver requirement (default: any).
    #[serde(default = "any_version")]
    pub version: String,
    /// What the audit found.
    pub verdict: Verdict,
    /// Why.
    pub reason: String,
}

fn any_version() -> String {
    "*".to_owned()
}

/// A simulation workspace's `determinism.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Patterns forbidden in every scanned crate, on top of [`crate::scan::FORBIDDEN`].
    pub forbid: Vec<String>,
    /// The audit of every crate linked from outside the workspace.
    #[serde(rename = "audit")]
    pub audits: Vec<Audit>,
}

impl Config {
    /// Parse a configuration file's text.
    ///
    /// # Errors
    ///
    /// * If it is not valid TOML of this shape, or a version requirement does not parse
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        for audit in &config.audits {
            semver::VersionReq::parse(&audit.version).map_err(|e| {
                format!("audit of {}: version {:?}: {e}", audit.name, audit.version)
            })?;
            if audit.reason.trim().is_empty() {
                return Err(format!("audit of {}: the reason is empty", audit.name));
            }
        }
        Ok(config)
    }

    /// The entry covering `name` at `version`, if any.
    #[must_use]
    pub fn audit(&self, name: &str, version: &semver::Version) -> Option<&Audit> {
        self.audits.iter().find(|audit| {
            audit.name == name
                && semver::VersionReq::parse(&audit.version)
                    .is_ok_and(|requirement| requirement.matches(version))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audits_match_by_name_and_version_requirement() {
        let config = Config::parse(
            r#"
            forbid = ["x("]
            [[audit]]
            name = "uuid"
            version = "^1"
            verdict = "restricted"
            reason = "new_v4"
            [[audit]]
            name = "itoa"
            verdict = "pure"
            reason = "formats integers"
            [[audit]]
            name = "hyper"
            verdict = "not-run"
            reason = "the server's transport, never started in a run"
            "#,
        )
        .unwrap();
        assert_eq!(config.forbid, ["x("]);
        let v = |s: &str| semver::Version::parse(s).unwrap();
        assert_eq!(config.audit("uuid", &v("1.18.0")).unwrap().verdict, Verdict::Restricted);
        assert!(config.audit("uuid", &v("2.0.0")).is_none());
        assert!(config.audit("itoa", &v("9.9.9")).is_some());
        assert_eq!(config.audit("hyper", &v("1.0.0")).unwrap().verdict, Verdict::NotRun);
        assert!(config.audit("rand", &v("0.8.5")).is_none());
    }

    #[test]
    fn malformed_entries_are_errors() {
        let bad_version = "[[audit]]\nname = \"a\"\nversion = \"~~1\"\nverdict = \"pure\"\nreason = \"r\"";
        assert!(Config::parse(bad_version).unwrap_err().contains("version"));
        let no_reason = "[[audit]]\nname = \"a\"\nverdict = \"pure\"\nreason = \" \"";
        assert!(Config::parse(no_reason).unwrap_err().contains("reason"));
        let bad_verdict = "[[audit]]\nname = \"a\"\nverdict = \"fine\"\nreason = \"r\"";
        assert!(Config::parse(bad_verdict).is_err());
    }
}
