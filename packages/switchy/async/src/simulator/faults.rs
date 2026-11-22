//! Seeded executor faults: late wakeups and spurious `Pending`.
//!
//! Both are off unless configured, and both are pure functions of the run's seed: every
//! decision is drawn from the fault's own named `switchy_simulation` stream, and only while
//! its probability is above 0, so a run with a fault off draws nothing from it and replays
//! exactly as one without the fault.
//!
//! * **Late wakeups** ([`set_late_wakeups`], `SIMULATOR_LATE_WAKEUPS`): a woken task is
//!   sometimes queued only after 1 to [`LATE_WAKEUP_MAX_STEPS`] further steps of its runtime
//!   (each `tick`, each turn of `block_on`, `wait` or a blocking task's loop), as on a busy
//!   executor. Under the harness the clock moves one step per tick, so the delay is simulated
//!   time too, and a wakeup can lose a race it would win at once: against a timeout, say. A
//!   woken task already waiting late stays where it is; one already queued is not delayed.
//! * **Spurious `Pending`** ([`set_spurious_pending`], `SIMULATOR_SPURIOUS_PENDING`): a ready
//!   leaf future of the simulator (its sleeps, instants and intervals, its own channels, watch
//!   and barrier, the wrapped `mpsc` and bounded channels, join handles) sometimes returns
//!   `Pending` once and wakes itself at once instead of returning `Ready`, before it consumes
//!   anything. Correct code cannot tell; code that assumes a ready future completes on this
//!   poll can. After a spurious `Pending`, the task's next ready leaf completes without a draw,
//!   so even a probability of 1 makes progress. tokio's own futures (its locks, `Notify`,
//!   `oneshot`, `broadcast`) are not reached.
//!
//! Each probability is a number in `0..=1` or `swarm` (draw it per run from
//! `{0, 0.01, 0.05, 0.2}`, from the fault's stream); unset, it is 0.

use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    sync::{
        Arc, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll},
};

/// The environment variable that sets each run's late wakeup probability.
pub const LATE_WAKEUPS_ENV: &str = "SIMULATOR_LATE_WAKEUPS";

/// The named `switchy_simulation` stream late wakeups are drawn from.
pub const LATE_WAKEUPS_STREAM: &str = "switchy_async::late_wakeups";

/// The environment variable that sets each run's spurious `Pending` probability.
pub const SPURIOUS_PENDING_ENV: &str = "SIMULATOR_SPURIOUS_PENDING";

/// The named `switchy_simulation` stream spurious `Pending`s are drawn from.
pub const SPURIOUS_PENDING_STREAM: &str = "switchy_async::spurious_pending";

/// The most runtime steps a late wakeup waits before its task is queued.
pub const LATE_WAKEUP_MAX_STEPS: u64 = 8;

/// The probabilities a `swarm` run draws among.
const SWARM: [f64; 4] = [0.0, 0.01, 0.05, 0.2];

/// How often each fault was considered and fired in the current run (see [`fault_counts`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FaultCounts {
    /// Wakeups that drew whether to be late.
    pub late_wakeup_draws: u64,
    /// Wakeups that were late.
    pub late_wakeups: u64,
    /// Ready leaf polls that drew whether to return `Pending`.
    pub spurious_pending_draws: u64,
    /// Ready leaf polls that returned `Pending`.
    pub spurious_pendings: u64,
}

/// One fault's state in a run: its probability (as `f64` bits) and its counts.
struct Fault {
    probability: AtomicU64,
    draws: AtomicU64,
    fired: AtomicU64,
}

impl Fault {
    const fn new(probability: f64) -> Self {
        Self {
            probability: AtomicU64::new(probability.to_bits()),
            draws: AtomicU64::new(0),
            fired: AtomicU64::new(0),
        }
    }

    fn get(&self) -> f64 {
        f64::from_bits(self.probability.load(Ordering::Relaxed))
    }

    fn set(&self, probability: f64) {
        self.probability
            .store(clamp(probability).to_bits(), Ordering::Relaxed);
    }
}

struct LateWakeups(Fault);
struct SpuriousPending(Fault);

const fn clamp(probability: f64) -> f64 {
    if probability.is_nan() {
        0.0
    } else {
        probability.clamp(0.0, 1.0)
    }
}

/// A probability from `value`: `None` for `swarm` (draw it per run).
///
/// # Panics
///
/// * If `value` is neither `swarm` nor a number in `0..=1`
fn parse_probability(env: &str, value: &str) -> Option<f64> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("swarm") {
        return None;
    }
    match value.parse::<f64>() {
        Ok(probability) if (0.0..=1.0).contains(&probability) => Some(probability),
        _ => panic!("{env} must be `swarm` or a number in 0..=1, got '{value}'"),
    }
}

/// The run's probability from `env` (unset: 0), drawing a `swarm` one from `stream`.
fn configured(env: &str, stream: &str) -> f64 {
    let Ok(value) = std::env::var(env) else {
        return 0.0;
    };
    parse_probability(env, &value)
        .unwrap_or_else(|| SWARM[draw(stream, |rng| rng.index(SWARM.len()))])
}

/// A draw from the run's `stream`.
fn draw<T>(stream: &str, f: impl FnOnce(&mut switchy_simulation::Rng) -> T) -> T {
    let stream = switchy_simulation::stream(stream);
    let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
    f(&mut rng)
}

fn late() -> Arc<LateWakeups> {
    switchy_simulation::state_with(|| {
        LateWakeups(Fault::new(configured(LATE_WAKEUPS_ENV, LATE_WAKEUPS_STREAM)))
    })
}

fn spurious() -> Arc<SpuriousPending> {
    switchy_simulation::state_with(|| {
        SpuriousPending(Fault::new(configured(
            SPURIOUS_PENDING_ENV,
            SPURIOUS_PENDING_STREAM,
        )))
    })
}

/// Make woken tasks late with probability `probability` (clamped to `0..=1`) in the current
/// run, instead of `SIMULATOR_LATE_WAKEUPS` (see the [module](self) docs).
pub fn set_late_wakeups(probability: f64) {
    late().0.set(probability);
}

/// The current run's late wakeup probability: the one set with [`set_late_wakeups`], or else
/// `SIMULATOR_LATE_WAKEUPS` (default 0).
///
/// # Panics
///
/// * If `SIMULATOR_LATE_WAKEUPS` is neither `swarm` nor a number in `0..=1`
#[must_use]
pub fn late_wakeups() -> f64 {
    late().0.get()
}

/// Make ready leaf futures return `Pending` once with probability `probability` (clamped to
/// `0..=1`) in the current run, instead of `SIMULATOR_SPURIOUS_PENDING` (see the
/// [module](self) docs).
pub fn set_spurious_pending(probability: f64) {
    spurious().0.set(probability);
}

/// The current run's spurious `Pending` probability: the one set with
/// [`set_spurious_pending`], or else `SIMULATOR_SPURIOUS_PENDING` (default 0).
///
/// # Panics
///
/// * If `SIMULATOR_SPURIOUS_PENDING` is neither `swarm` nor a number in `0..=1`
#[must_use]
pub fn spurious_pending() -> f64 {
    spurious().0.get()
}

/// How often each fault was considered and fired so far in the current run.
#[must_use]
pub fn fault_counts() -> FaultCounts {
    let (late, spurious) = (late(), spurious());
    FaultCounts {
        late_wakeup_draws: late.0.draws.load(Ordering::Relaxed),
        late_wakeups: late.0.fired.load(Ordering::Relaxed),
        spurious_pending_draws: spurious.0.draws.load(Ordering::Relaxed),
        spurious_pendings: spurious.0.fired.load(Ordering::Relaxed),
    }
}

/// How many steps a wakeup is late, if it is: `None` when the fault is off (nothing is
/// drawn), when `eligible` says the wakeup cannot be late (nothing is drawn either), or when
/// the draw says it is on time.
pub fn late_wakeup(eligible: impl FnOnce() -> bool) -> Option<u64> {
    let late = late();
    let probability = late.0.get();
    if probability <= 0.0 || !eligible() {
        return None;
    }
    late.0.draws.fetch_add(1, Ordering::Relaxed);
    let steps = draw(LATE_WAKEUPS_STREAM, |rng| {
        rng.chance(probability)
            .then(|| rng.range_inclusive(1..=LATE_WAKEUP_MAX_STEPS))
    })?;
    late.0.fired.fetch_add(1, Ordering::Relaxed);
    Some(steps)
}

thread_local! {
    /// Whether the task (or other future) being polled on this thread returned a spurious
    /// `Pending` and has not completed a ready leaf since: its next ready leaf completes
    /// without a draw.
    static OWED: Cell<bool> = const { Cell::new(false) };
}

/// Poll with `owed` as the polled future's spurious `Pending` state (see [`OWED`]), saving it
/// back when the poll returns or unwinds.
pub fn with_owed<R>(owed: &AtomicBool, poll: impl FnOnce() -> R) -> R {
    struct Restore<'a> {
        owed: &'a AtomicBool,
        outer: bool,
    }
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            self.owed.store(OWED.replace(self.outer), Ordering::Relaxed);
        }
    }
    let outer = OWED.replace(owed.load(Ordering::Relaxed));
    let _restore = Restore { owed, outer };
    poll()
}

/// Whether this poll of a leaf future returns a spurious `Pending`: only when the fault is on
/// and `ready` (called with no lock of the caller held) says the leaf would complete. If so,
/// the waker is woken at once. Nothing is drawn while the fault is off or the leaf not ready.
pub fn spurious_pending_now(cx: &Context<'_>, ready: impl FnOnce() -> bool) -> bool {
    let spurious = spurious();
    let probability = spurious.0.get();
    if probability <= 0.0 || !ready() || OWED.replace(false) {
        return false;
    }
    spurious.0.draws.fetch_add(1, Ordering::Relaxed);
    if !draw(SPURIOUS_PENDING_STREAM, |rng| rng.chance(probability)) {
        return false;
    }
    spurious.0.fired.fetch_add(1, Ordering::Relaxed);
    OWED.set(true);
    cx.waker().wake_by_ref();
    true
}

/// A point before a leaf operation of an `async fn` (a receive, a send) that returns a
/// spurious `Pending` at most once, on its first poll, when `ready` says the operation would
/// complete at once (see [`spurious_pending_now`]).
pub const fn spurious_point<F: FnOnce() -> bool + Unpin>(ready: F) -> SpuriousPoint<F> {
    SpuriousPoint { ready: Some(ready) }
}

/// The future [`spurious_point`] returns.
pub struct SpuriousPoint<F> {
    ready: Option<F>,
}

impl<F: FnOnce() -> bool + Unpin> Future for SpuriousPoint<F> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let spurious = self
            .get_mut()
            .ready
            .take()
            .is_some_and(|ready| spurious_pending_now(cx, ready));
        if spurious { Poll::Pending } else { Poll::Ready(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn parses_numbers_and_swarm() {
        assert_eq!(parse_probability("X", "swarm"), None);
        assert_eq!(parse_probability("X", " SWARM "), None);
        assert_eq!(parse_probability("X", "0"), Some(0.0));
        assert_eq!(parse_probability("X", "0.05"), Some(0.05));
        assert_eq!(parse_probability("X", "1"), Some(1.0));
    }

    #[test_log::test]
    #[should_panic(expected = "SIMULATOR_LATE_WAKEUPS")]
    fn rejects_out_of_range() {
        let _ = parse_probability(LATE_WAKEUPS_ENV, "1.5");
    }

    #[test_log::test]
    #[should_panic(expected = "SIMULATOR_SPURIOUS_PENDING")]
    fn rejects_garbage() {
        let _ = parse_probability(SPURIOUS_PENDING_ENV, "often");
    }

    #[test_log::test]
    fn set_clamps_and_off_draws_nothing() {
        switchy_simulation::with_context(switchy_simulation::Context::new(1), || {
            set_late_wakeups(2.0);
            assert!((late_wakeups() - 1.0).abs() < f64::EPSILON);
            set_late_wakeups(f64::NAN);
            assert!(late_wakeups().abs() < f64::EPSILON);
            set_spurious_pending(-1.0);
            assert!(spurious_pending().abs() < f64::EPSILON);
            assert_eq!(late_wakeup(|| true), None);
            let waker = futures::task::noop_waker();
            assert!(!spurious_pending_now(&Context::from_waker(&waker), || true));
            assert_eq!(fault_counts(), FaultCounts::default());
        });
    }

    #[test_log::test]
    fn a_spurious_pending_is_owed_back() {
        switchy_simulation::with_context(switchy_simulation::Context::new(1), || {
            set_spurious_pending(1.0);
            let owed = AtomicBool::new(false);
            let waker = futures::task::noop_waker();
            let cx = Context::from_waker(&waker);
            with_owed(&owed, || {
                // Not ready: no draw, nothing owed.
                assert!(!spurious_pending_now(&cx, || false));
                assert!(spurious_pending_now(&cx, || true));
            });
            assert!(owed.load(Ordering::Relaxed));
            with_owed(&owed, || assert!(!spurious_pending_now(&cx, || true)));
            assert!(!owed.load(Ordering::Relaxed));
            let counts = fault_counts();
            assert_eq!(counts.spurious_pending_draws, 1);
            assert_eq!(counts.spurious_pendings, 1);
        });
    }
}
