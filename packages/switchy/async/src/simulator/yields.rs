//! Seeded yield points: where `#[inject_yields]` and `select!` let other tasks run.
//!
//! Each simulation run picks how often an injected point yields: with probability 0, ¼, ½ or
//! 1, drawn from the run's seed (named stream `switchy_async::yields`). Runs then differ in how
//! often tasks give way, and a probability of 0 reproduces a run without injected yields: the
//! point does not even poll a yield.
//!
//! `SIMULATOR_YIELD_PROBABILITY` overrides the choice with a number in `0..=1`, or `swarm` to
//! draw it per run (the default). [`set_yield_probability`] overrides it for the current run.

use std::{
    future::{Future, IntoFuture},
    pin::Pin,
    sync::{
        Arc, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

/// The environment variable that overrides each run's yield probability.
pub const YIELD_PROBABILITY_ENV: &str = "SIMULATOR_YIELD_PROBABILITY";

/// The named `switchy_simulation` stream the yield probability and each yield are drawn from.
pub const YIELD_STREAM: &str = "switchy_async::yields";

/// The probabilities a run draws among when none is configured.
const SWARM: [f64; 4] = [0.0, 0.25, 0.5, 1.0];

/// This run's yield probability, as `f64` bits.
struct YieldProbability(AtomicU64);

impl YieldProbability {
    fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// `SIMULATOR_YIELD_PROBABILITY`, read once per process.
fn configured() -> Option<f64> {
    static CONFIGURED: OnceLock<Option<f64>> = OnceLock::new();
    *CONFIGURED.get_or_init(|| {
        std::env::var(YIELD_PROBABILITY_ENV)
            .ok()
            .and_then(|value| parse_probability(&value))
    })
}

/// A probability from `value`: `None` for `swarm` (draw per run).
///
/// # Panics
///
/// * If `value` is neither `swarm` nor a number in `0..=1`
fn parse_probability(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("swarm") {
        return None;
    }
    match value.parse::<f64>() {
        Ok(probability) if (0.0..=1.0).contains(&probability) => Some(probability),
        _ => panic!("{YIELD_PROBABILITY_ENV} must be `swarm` or a number in 0..=1: '{value}'"),
    }
}

fn state() -> Arc<YieldProbability> {
    switchy_simulation::state_with(|| {
        let probability = configured().unwrap_or_else(|| SWARM[draw(|rng| rng.index(SWARM.len()))]);
        YieldProbability(AtomicU64::new(probability.to_bits()))
    })
}

/// A draw from the run's yield stream.
fn draw<T>(f: impl FnOnce(&mut switchy_simulation::Rng) -> T) -> T {
    let stream = switchy_simulation::stream(YIELD_STREAM);
    let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
    f(&mut rng)
}

/// The current run's yield probability: how often an injected yield point gives way.
#[must_use]
pub fn yield_probability() -> f64 {
    state().get()
}

/// Override the current run's yield probability (clamped to `0..=1`).
pub fn set_yield_probability(probability: f64) {
    let probability = if probability.is_nan() {
        0.0
    } else {
        probability.clamp(0.0, 1.0)
    };
    state().0.store(probability.to_bits(), Ordering::Relaxed);
}

/// Whether the next injected point yields: drawn with the run's probability. A probability of
/// 0 or 1 draws nothing.
#[must_use]
fn should_yield() -> bool {
    let probability = yield_probability();
    if probability <= 0.0 {
        return false;
    }
    if probability >= 1.0 {
        return true;
    }
    let bits = draw(switchy_simulation::Rng::next_u64) >> 11;
    // 53 random bits as a fraction of 2^53: exact in an f64.
    #[allow(clippy::cast_precision_loss)]
    let draw = bits as f64 / (1u64 << 53) as f64;
    draw < probability
}

/// Lets other tasks run with the run's yield probability (see the [module](self) docs).
///
/// When it does not yield, the future is ready on its first poll and wakes nothing.
#[must_use = "futures do nothing unless awaited"]
pub const fn maybe_yield() -> MaybeYield {
    MaybeYield { state: None }
}

/// The future [`maybe_yield`] returns.
#[derive(Debug)]
pub struct MaybeYield {
    /// `None` before the first poll; then whether it has yielded.
    state: Option<bool>,
}

impl Future for MaybeYield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match self.state {
            Some(_) => Poll::Ready(()),
            None => {
                if should_yield() {
                    self.state = Some(true);
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    self.state = Some(false);
                    Poll::Ready(())
                }
            }
        }
    }
}

/// Awaits `future`, then lets other tasks run with the run's yield probability before
/// returning its output. `#[inject_yields]` rewrites `x.await` to `yield_after(x).await`.
#[must_use = "futures do nothing unless awaited"]
pub fn yield_after<F: IntoFuture>(future: F) -> YieldAfter<F::IntoFuture> {
    YieldAfter {
        future: future.into_future(),
        output: None,
    }
}

pin_project! {
    /// The future [`yield_after`] returns.
    #[derive(Debug)]
    pub struct YieldAfter<F: Future> {
        #[pin]
        future: F,
        output: Option<F::Output>,
    }
}

impl<F: Future> Future for YieldAfter<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.project();
        if let Some(output) = this.output.take() {
            return Poll::Ready(output);
        }
        let output = std::task::ready!(this.future.poll(cx));
        if should_yield() {
            *this.output = Some(output);
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn parses_numbers_and_swarm() {
        assert_eq!(parse_probability("swarm"), None);
        assert_eq!(parse_probability(" SWARM "), None);
        assert_eq!(parse_probability("0"), Some(0.0));
        assert_eq!(parse_probability("0.25"), Some(0.25));
        assert_eq!(parse_probability("1"), Some(1.0));
    }

    #[test_log::test]
    #[should_panic(expected = "SIMULATOR_YIELD_PROBABILITY")]
    fn rejects_out_of_range() {
        let _ = parse_probability("1.5");
    }

    #[test_log::test]
    #[should_panic(expected = "SIMULATOR_YIELD_PROBABILITY")]
    fn rejects_garbage() {
        let _ = parse_probability("often");
    }

    #[test_log::test]
    fn set_clamps() {
        switchy_simulation::with_context(switchy_simulation::Context::new(1), || {
            set_yield_probability(2.0);
            assert!((yield_probability() - 1.0).abs() < f64::EPSILON);
            set_yield_probability(f64::NAN);
            assert!(yield_probability().abs() < f64::EPSILON);
        });
    }
}
