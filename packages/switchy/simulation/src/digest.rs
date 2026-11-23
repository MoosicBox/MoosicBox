//! What a simulation run observed, to check that replaying the run observes the same.
//!
//! Every switchy simulator reports what its run did to the run's [`Digest`] as it happens
//! ([`observe`]: a process spawned, a process exited) and when the run ends (an end-of-run
//! observer registered with [`on_run_end`]: the final file tree, every byte each terminal was
//! written). Application code calls nothing: the simulators observe for it.
//!
//! A digest keeps a hash per source and, for the first [`MAX_CHECKPOINTS`] observations, a
//! checkpoint each with a short preview, so a mismatch between two runs is located at the
//! first observation where they part ([`Digest::divergence`]).
//!
//! `SIMULATOR_DIGEST_BACKTRACE_AT=N` adds a backtrace to observation `N`'s checkpoint, to see
//! what made a diverging observation.

use std::{
    collections::BTreeMap,
    fmt::{self, Write as _},
    sync::{LazyLock, Mutex},
};

use crate::lock;

/// How many observations of a run keep a checkpoint; later ones are only hashed.
pub const MAX_CHECKPOINTS: usize = 100_000;

/// How much of an observation's text its checkpoint keeps, to show where two differ.
pub const TEXT_CHARS: usize = 64 * 1024;

/// How much text around a difference a report shows, on each side.
const CONTEXT_CHARS: usize = 120;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

fn fnv(hash: u64, bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(hash, |h, &b| (h ^ u64::from(b)).wrapping_mul(FNV_PRIME))
}

/// One source's observations: how many, and the hash of all of them in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    /// Observations made.
    pub count: u64,
    /// Hash of every observation, in order.
    pub hash: u64,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            count: 0,
            hash: FNV_OFFSET,
        }
    }
}

/// One observation, as a run's digest keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// What made it (`pty.spawn`, `fs`, ...).
    pub source: String,
    /// The observation's own hash.
    pub hash: u64,
    /// Its text: its label and value, up to [`TEXT_CHARS`].
    pub detail: String,
}

/// Where `a` and `b` first differ, with up to [`CONTEXT_CHARS`] around it from each.
fn first_difference(a: &str, b: &str) -> (usize, String, String) {
    let at = a
        .chars()
        .zip(b.chars())
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| a.chars().count().min(b.chars().count()));
    let window = |s: &str| {
        let start = at.saturating_sub(CONTEXT_CHARS);
        let shown: String = s.chars().skip(start).take(2 * CONTEXT_CHARS).collect();
        let more = s.chars().count() > start + 2 * CONTEXT_CHARS;
        format!(
            "{}{shown}{}",
            if start > 0 { "…" } else { "" },
            if more { "…" } else { "" }
        )
    };
    (at, window(a), window(b))
}

/// Everything one run observed. See the [module documentation](self).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Digest {
    /// Hash of every observation, in order, with its source.
    pub total: u64,
    /// Observations made.
    pub observations: u64,
    /// Each source's observations.
    pub sources: BTreeMap<String, Source>,
    /// The first [`MAX_CHECKPOINTS`] observations.
    pub checkpoints: Vec<Checkpoint>,
}

impl Default for Digest {
    fn default() -> Self {
        Self {
            total: FNV_OFFSET,
            observations: 0,
            sources: BTreeMap::new(),
            checkpoints: Vec::new(),
        }
    }
}

/// Feeds an observation's text into its hash, keeping the start as its preview.
struct Hashing {
    hash: u64,
    preview: String,
    previewed: usize,
}

impl Hashing {
    fn new(label: &str) -> Self {
        let mut hashing = Self {
            hash: FNV_OFFSET,
            preview: String::new(),
            previewed: 0,
        };
        let _ = hashing.write_str(label);
        hashing.hash = fnv(hashing.hash, &[0xff]);
        hashing
    }

    fn preview(&mut self, s: &str) {
        for c in s.chars() {
            if self.previewed >= TEXT_CHARS {
                if self.previewed == TEXT_CHARS {
                    self.preview.push('…');
                    self.previewed += 1;
                }
                return;
            }
            self.preview.push(c);
            self.previewed += 1;
        }
    }
}

impl fmt::Write for Hashing {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.hash = fnv(self.hash, s.as_bytes());
        self.preview(s);
        Ok(())
    }
}

/// The current run's digest.
#[derive(Default)]
struct RunDigest(Mutex<Digest>);

static BACKTRACE_AT: LazyLock<Option<u64>> = LazyLock::new(|| {
    std::env::var("SIMULATOR_DIGEST_BACKTRACE_AT")
        .ok()
        .and_then(|value| value.parse().ok())
});

fn record(source: &str, hashing: Hashing) {
    let digest = crate::state::<RunDigest>();
    let mut digest = lock(&digest.0);
    let index = digest.observations;
    digest.observations += 1;
    let entry = digest.sources.entry(source.to_owned()).or_default();
    entry.count += 1;
    entry.hash = fnv(entry.hash, &hashing.hash.to_le_bytes());
    digest.total = fnv(
        fnv(digest.total, source.as_bytes()),
        &hashing.hash.to_le_bytes(),
    );
    if digest.checkpoints.len() < MAX_CHECKPOINTS {
        let mut detail = hashing.preview;
        if *BACKTRACE_AT == Some(index) {
            let _ = write!(detail, "\n{}", std::backtrace::Backtrace::force_capture());
        }
        digest.checkpoints.push(Checkpoint {
            source: source.to_owned(),
            hash: hashing.hash,
            detail,
        });
    }
}

/// Observe `label` with `bytes` in the current run (as `source`).
pub fn observe(source: &str, label: &str, bytes: &[u8]) {
    let mut hashing = Hashing::new(label);
    hashing.hash = fnv(hashing.hash, bytes);
    if !bytes.is_empty() {
        let shown = String::from_utf8_lossy(&bytes[..bytes.len().min(TEXT_CHARS)]);
        hashing.preview(&format!(": {} bytes {shown:?}", bytes.len()));
    }
    record(source, hashing);
}

/// Observe `label` with `value`'s `Debug` text in the current run (as `source`). The text is
/// hashed as it is written, so even a large value costs no allocation beyond its preview.
pub fn observe_debug(source: &str, label: &str, value: &impl fmt::Debug) {
    let mut hashing = Hashing::new(label);
    hashing.preview(": ");
    let _ = write!(hashing, "{value:?}");
    record(source, hashing);
}

/// What observes a run's final state, for every run of the process.
type EndObserver = fn();

static END_OBSERVERS: Mutex<BTreeMap<&'static str, EndObserver>> = Mutex::new(BTreeMap::new());

/// Have `observer` observe the final state of every run [`finish`]ed from now on.
///
/// It is registered under `name` (registering a name again replaces its observer). Observers
/// run in the order of their names, in the finished run's context.
///
/// Registration is process-wide, so an observer also runs for runs that never used its
/// simulator: it must then observe nothing (see [`crate::existing_state`]).
pub fn on_run_end(name: &'static str, observer: EndObserver) {
    lock(&END_OBSERVERS).insert(name, observer);
}

/// End the current run's digest: the end-of-run observers and every named random stream's
/// final state observe it, and it is taken from the run (which starts a new one).
#[must_use]
pub fn finish() -> Digest {
    let observers: Vec<EndObserver> = lock(&END_OBSERVERS).values().copied().collect();
    for observer in observers {
        observer();
    }
    let context = crate::current();
    let streams: Vec<(String, crate::Rng)> = lock(&context.streams)
        .iter()
        .map(|(name, stream)| (name.clone(), lock(stream).clone()))
        .collect();
    for (name, rng) in streams {
        observe_debug("random", &name, &rng);
    }
    std::mem::take(&mut *lock(&crate::state::<RunDigest>().0))
}

/// Escape `s` onto one line.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl Digest {
    /// The digest as text, one item per line, for [`Self::from_text`] (another process's).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = format!(
            "total {:016x}\nobservations {}\n",
            self.total, self.observations
        );
        for (name, source) in &self.sources {
            let _ = writeln!(
                text,
                "source {:016x} {} {}",
                source.hash,
                source.count,
                escape(name)
            );
        }
        for checkpoint in &self.checkpoints {
            let _ = writeln!(
                text,
                "checkpoint {:016x} {} {}",
                checkpoint.hash,
                escape(&checkpoint.source).replace(' ', "\\s"),
                escape(&checkpoint.detail)
            );
        }
        text
    }

    /// Parse [`Self::to_text`]'s text, skipping lines that are not part of it.
    ///
    /// # Errors
    ///
    /// * If a digest line is malformed
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut digest = Self::default();
        let hex = |s: Option<&str>| {
            s.and_then(|s| u64::from_str_radix(s, 16).ok())
                .ok_or_else(|| format!("bad hash in digest: {s:?}"))
        };
        for line in text.lines() {
            let mut parts = line.splitn(2, ' ');
            let (Some(kind), rest) = (parts.next(), parts.next().unwrap_or_default()) else {
                continue;
            };
            match kind {
                "total" => digest.total = hex(Some(rest))?,
                "observations" => {
                    digest.observations = rest
                        .parse()
                        .map_err(|e| format!("bad observation count {rest:?}: {e}"))?;
                }
                "source" => {
                    let mut fields = rest.splitn(3, ' ');
                    let hash = hex(fields.next())?;
                    let count = fields
                        .next()
                        .and_then(|c| c.parse().ok())
                        .ok_or_else(|| format!("bad source line {line:?}"))?;
                    let name = unescape(fields.next().unwrap_or_default());
                    digest.sources.insert(name, Source { count, hash });
                }
                "checkpoint" => {
                    let mut fields = rest.splitn(3, ' ');
                    let hash = hex(fields.next())?;
                    let source = unescape(&fields.next().unwrap_or_default().replace("\\s", " "));
                    let detail = unescape(fields.next().unwrap_or_default());
                    digest.checkpoints.push(Checkpoint {
                        source,
                        hash,
                        detail,
                    });
                }
                _ => {}
            }
        }
        Ok(digest)
    }

    /// How `other` (a replay's digest) differs from this one, described for a person: each
    /// source that differs, and the first observation where the two part. `None` when they
    /// observed the same.
    #[must_use]
    pub fn divergence(&self, other: &Self) -> Option<String> {
        if self.total == other.total
            && self.observations == other.observations
            && self.sources == other.sources
        {
            return None;
        }
        let mut report = String::from("sources that differ (this run / the replay):");
        let names: std::collections::BTreeSet<&String> =
            self.sources.keys().chain(other.sources.keys()).collect();
        for name in names {
            let ours = self.sources.get(name).copied().unwrap_or_default();
            let theirs = other.sources.get(name).copied().unwrap_or_default();
            if ours != theirs {
                let _ = write!(
                    report,
                    "\n  {name}: {} observations / {}",
                    ours.count, theirs.count
                );
                if ours.count == theirs.count {
                    report.push_str(" (same count, different content)");
                }
            }
        }
        let first = self
            .checkpoints
            .iter()
            .zip(&other.checkpoints)
            .position(|(a, b)| a.source != b.source || a.hash != b.hash);
        if let Some(index) = first {
            let (ours, theirs) = (&self.checkpoints[index], &other.checkpoints[index]);
            let (at, ours_text, theirs_text) = first_difference(&ours.detail, &theirs.detail);
            let _ = write!(
                report,
                "\nfirst difference at observation {index}, character {at}:\n  this run: [{}] \
                 {ours_text}\n  the replay: [{}] {theirs_text}\n\
                 (SIMULATOR_DIGEST_BACKTRACE_AT={index} adds a backtrace to it)",
                ours.source, theirs.source
            );
            if ours.detail == theirs.detail {
                let _ = write!(
                    report,
                    "\n(their first {TEXT_CHARS} characters agree; they differ later)"
                );
            }
        } else {
            let shared = self.checkpoints.len().min(other.checkpoints.len());
            if self.checkpoints.len() == other.checkpoints.len() {
                let _ = write!(
                    report,
                    "\nthe first {shared} observations agree; the difference comes later"
                );
            } else {
                let (longer, which) = if self.checkpoints.len() > shared {
                    (&self.checkpoints[shared], "this run")
                } else {
                    (&other.checkpoints[shared], "the replay")
                };
                let (_, text, _) = first_difference(&longer.detail, "");
                let _ = write!(
                    report,
                    "\nthe first {shared} observations agree; then only {which} observed:\n  [{}] {text}",
                    longer.source
                );
            }
        }
        Some(report)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{Context, with_context};

    fn digest_of(f: impl FnOnce()) -> Digest {
        with_context(Context::new(1), || {
            f();
            finish()
        })
    }

    #[test]
    fn the_same_observations_digest_the_same() {
        let a = digest_of(|| {
            observe("pty.spawn", "1 sh", b"");
            observe_debug("fs", "tree", &vec![1, 2, 3]);
        });
        let b = digest_of(|| {
            observe("pty.spawn", "1 sh", b"");
            observe_debug("fs", "tree", &vec![1, 2, 3]);
        });
        assert_eq!(a, b);
        assert_eq!(a.divergence(&b), None);
        assert_eq!(a.observations, 2);
    }

    #[test]
    fn a_divergence_names_the_source_and_first_observation() {
        let a = digest_of(|| {
            observe("pty.spawn", "1 sh", b"");
            observe("pty.exit", "1", b"0");
        });
        let b = digest_of(|| {
            observe("pty.spawn", "1 sh", b"");
            observe("pty.exit", "1", b"9");
        });
        let report = a.divergence(&b).expect("they differ");
        assert!(
            report.contains("pty.exit: 1 observations / 1 (same count"),
            "{report}"
        );
        assert!(
            report.contains("first difference at observation 1, character"),
            "{report}"
        );
        assert!(report.contains("\"0\""), "{report}");
        assert!(report.contains("\"9\""), "{report}");
    }

    #[test]
    fn a_missing_observation_is_located() {
        let a = digest_of(|| observe("x", "one", b""));
        let b = digest_of(|| {
            observe("x", "one", b"");
            observe("y", "two", b"");
        });
        let report = a.divergence(&b).expect("they differ");
        assert!(report.contains("then only the replay observed"), "{report}");
        assert!(report.contains("[y] two"), "{report}");
    }

    #[test]
    fn random_streams_are_observed_by_their_final_state() {
        let a = digest_of(|| {
            let _ = crate::stream("s").lock().unwrap().next_u64();
        });
        let b = digest_of(|| {
            let stream = crate::stream("s");
            let _ = stream.lock().unwrap().next_u64();
            let _ = stream.lock().unwrap().next_u64();
        });
        assert_eq!(a.sources["random"].count, 1);
        assert!(a.divergence(&b).is_some());
    }

    #[test]
    fn a_difference_deep_in_a_large_observation_is_shown_in_context() {
        let tree = |leaf: u32| {
            let mut tree: BTreeMap<u32, u32> = (0..2000).map(|i| (i, i)).collect();
            tree.insert(1500, leaf);
            tree
        };
        let a = digest_of(|| observe_debug("fs", "tree", &tree(1500)));
        let b = digest_of(|| observe_debug("fs", "tree", &tree(7)));
        let report = a.divergence(&b).expect("they differ");
        assert!(report.contains("1500: 1500"), "{report}");
        assert!(report.contains("1500: 7"), "{report}");
        assert!(report.contains('…'), "{report}");
    }

    #[test]
    fn digests_survive_their_text_form() {
        let digest = digest_of(|| {
            observe("pty terminal", "a\nb \\ c", b"bytes\n");
            observe_debug("fs", "tree", &"x".repeat(500));
        });
        assert_eq!(Digest::from_text(&digest.to_text()), Ok(digest));
    }

    static OBSERVED: Mutex<u64> = Mutex::new(0);

    #[derive(Default)]
    struct Used;

    fn observe_used() {
        if crate::existing_state::<Used>().is_some() {
            *lock(&OBSERVED) += 1;
            observe("used", "final", b"");
        }
    }

    #[test]
    fn end_observers_observe_only_runs_that_used_their_simulator() {
        on_run_end("used", observe_used);
        let unused = digest_of(|| {});
        assert!(!unused.sources.contains_key("used"));
        let used = digest_of(|| {
            let _: Arc<Used> = crate::state();
        });
        assert_eq!(used.sources["used"].count, 1);
        assert!(*lock(&OBSERVED) >= 1);
    }
}
