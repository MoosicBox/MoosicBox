//! Exhaustive exploration of a short scenario's schedules, bounded by preemptions.
//!
//! [`explore`] runs a scenario again and again, each time replaying a [`Schedule`]: a prefix
//! of choices, then the default choice everywhere. It enumerates, depth first, every schedule
//! that differs from the default at choice points costing no more than a given number of
//! *preemptions* in all, as CHESS does. A preemption is switching away from a thread that
//! could have gone on; switching because the running thread blocked or finished is free, so
//! within the bound every such choice is explored. Most races need only one or two
//! preemptions, so a small bound finds every race of a short scenario that is reachable at
//! all, for certain.
//!
//! To know the alternatives at each choice point, every run records its [`Trace`]: at each
//! decision, the threads that could run, the one picked, and what the thread that stopped
//! touched since its last switch point (its *transition*).
//!
//! # Pruning
//!
//! With [`ExploreOptions::prune`] (the default), the exploration skips schedules that only
//! reorder independent transitions, with sleep sets (Godefroid). Two transitions are
//! *independent* when neither touched anything the scheduler does not track and no object
//! was touched by both with at least one writing it. The objects are:
//!
//! * a mutex or read-write lock (acquiring, releasing, waiting for or waking it), which also
//!   stands for the data it guards;
//! * a condition variable (waiting on or notifying it); notifying one also draws from the
//!   run's `wake_one` order, so two `notify_one`s are always dependent;
//! * a channel (sending, receiving, cloning or dropping an end, waiting on it);
//! * an atomic (loads read it; stores and read-modify-writes, failed ones too, write it), so
//!   two loads of one atomic are independent;
//! * a thread's completion (exiting writes it; joining and `is_finished` read it).
//!
//! Everything else makes a transition dependent on every other one: spawning a thread,
//! sleeping, reading the scheduler's state (`snapshot`, `others_alive`, ...). A run that uses
//! the scheduler's public switch points or wakeups (as other simulators, such as
//! `switchy_pty` and `switchy_async`, do), kills a group or models weak memory is *untracked*:
//! none of its transitions counts as independent, so nothing in it is pruned.
//!
//! Pruning assumes threads share state only through this crate's primitives. State shared
//! otherwise (a `std` atomic or lock, a static, the run's random generator, the simulated
//! clock set by hand) is invisible to it; explore such code with `prune` off.
//!
//! The independence is observed, not declared: a run records what each transition actually
//! touched, and footprints are only compared within one run (object ids are not stable across
//! runs). A thread `t` is asleep at a choice point when its branch at an earlier choice point
//! of the same path was explored, and its next transition (where it eventually runs, later in
//! this run) is independent of every transition since that point. Since a transition's course
//! depends only on what it touches, it would have done the same at the earlier point, so every
//! schedule picking `t` now is equivalent to one explored in its earlier branch.
//!
//! Sleep sets are sound with a preemption bound only when moving `t`'s transition earlier
//! costs no extra preemption. That holds when `t`'s transition ends with `t` blocked or
//! finished (switching away from it is then free) and picking `t` at the earlier point cost no
//! more than the branch taken there (the default is explored first). Only such transitions
//! put a thread to sleep, so every failure reachable within the bound is still found.

use std::fmt::Display;

use super::{Config, Schedule, ShutdownReport, ThreadIndex, Trace, run_with};

/// How [`explore`] searches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExploreOptions {
    /// The most preemptions a schedule may make.
    pub preemptions: u32,
    /// Stop after this many runs, with [`Exploration::complete`] false.
    pub max_runs: Option<usize>,
    /// Skip schedules that only reorder independent operations (see the module documentation).
    pub prune: bool,
}

impl ExploreOptions {
    /// Every schedule with at most `preemptions` preemptions, pruned, without a run limit.
    #[must_use]
    pub const fn new(preemptions: u32) -> Self {
        Self {
            preemptions,
            max_runs: None,
            prune: true,
        }
    }

    /// Stop after `runs` runs.
    #[must_use]
    pub const fn max_runs(mut self, runs: usize) -> Self {
        self.max_runs = Some(runs);
        self
    }

    /// Whether to prune.
    #[must_use]
    pub const fn prune(mut self, prune: bool) -> Self {
        self.prune = prune;
        self
    }
}

/// The first failing run of an [`explore`]ation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExploreFailure {
    /// The schedule the run followed. It replays the failure with [`Config::replay`] (or
    /// `SIMULATOR_SCHEDULE`, in its text form) and the same seed, and [`super::shrink`]s.
    pub schedule: Schedule,
    /// Why it failed: the closure's error or panic message (a deadlock report, for one).
    pub message: String,
    /// Which run it was, from 1.
    pub run: usize,
}

/// What an [`explore`]ation did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exploration {
    /// The runs made.
    pub runs: usize,
    /// Alternatives within the bound that pruning skipped.
    pub pruned: usize,
    /// Whether every schedule within the bound was explored (or pruned) without a failure.
    pub complete: bool,
    /// The first failing run, if any; the exploration stops there.
    pub failure: Option<ExploreFailure>,
}

/// One run of [`explore_runs`]: whether it failed, and what remained of it.
#[derive(Debug)]
pub struct ExploredRun {
    /// Why the run failed, or `None` if it passed.
    pub failure: Option<String>,
    /// The run's report, with its trace (from [`super::Scheduler::shutdown`]).
    pub report: ShutdownReport,
}

/// Explore `f` as the main thread of runs seeded with `seed` (see the
/// [module documentation](self)). A run fails when `f` returns an error or panics, which a
/// deadlock does.
pub fn explore<E: Display>(
    seed: u64,
    options: &ExploreOptions,
    mut f: impl FnMut() -> Result<(), E>,
) -> Exploration {
    explore_runs(options, |config| {
        let outcome = run_with(config, seed, &mut f);
        let failure = match outcome.result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error.to_string()),
            Err(panic) => Some(panic_message(&*panic)),
        };
        ExploredRun {
            failure,
            report: outcome.report,
        }
    })
}

/// Explore runs that `run` makes itself, for harnesses that create their runs (such as simvar).
///
/// Each call must make one run with its scheduler configured by the given configuration (with
/// [`super::configure`], or [`run_with`]), everything else seeded the same every time, and
/// return how it went.
pub fn explore_runs(
    options: &ExploreOptions,
    mut run: impl FnMut(Config) -> ExploredRun,
) -> Exploration {
    let mut result = Exploration {
        runs: 0,
        pruned: 0,
        complete: false,
        failure: None,
    };
    let mut frames: Vec<Frame> = Vec::new();
    let mut kept = 0;
    let mut schedule = Schedule::new();
    let fail = |result: &mut Exploration, schedule: Schedule, message: String| {
        result.failure = Some(ExploreFailure {
            schedule,
            message,
            run: result.runs,
        });
    };
    loop {
        if options.max_runs.is_some_and(|max| result.runs >= max) {
            return result;
        }
        let explored = run(Config::explore(schedule.clone()));
        result.runs += 1;
        if let Some(message) = explored.failure {
            fail(&mut result, explored.report.schedule, message);
            return result;
        }
        let Some(trace) = explored.report.trace else {
            fail(
                &mut result,
                schedule,
                "the run recorded no trace: configure its scheduler with the configuration \
                 the exploration gives"
                    .to_owned(),
            );
            return result;
        };
        if let Err(message) = extend(&mut frames, kept, &trace) {
            fail(&mut result, schedule, message);
            return result;
        }
        if options.prune && !trace.untracked {
            prune(&mut frames, &trace);
        }
        let mut next = false;
        while let Some(frame) = frames.last_mut() {
            let bound = options.preemptions;
            let within = |frame: &Frame, thread: ThreadIndex| {
                !frame.explored.contains(&thread)
                    && frame.preemptions_before + frame.cost(thread) <= bound
            };
            if !frame.redundant
                && let Some(thread) = frame
                    .runnable
                    .iter()
                    .copied()
                    .find(|&t| within(frame, t) && !frame.pruned.contains(&t))
            {
                frame.explored.push(thread);
                frame.chosen = thread;
                next = true;
                break;
            }
            result.pruned += frame.runnable.iter().filter(|&&t| within(frame, t)).count();
            frames.pop();
        }
        if !next {
            result.complete = true;
            return result;
        }
        kept = frames.len();
        schedule = Schedule {
            choices: frames
                .iter()
                .filter(|frame| frame.chosen != frame.default)
                .map(|frame| (frame.point, frame.chosen))
                .collect(),
        };
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "panicked".to_owned())
}

/// A choice point on the path being explored.
#[derive(Debug)]
struct Frame {
    /// The decision's index in the run's trace.
    decision: usize,
    /// The choice point's number.
    point: u64,
    /// The thread that reached the switch point.
    from: ThreadIndex,
    /// Whether it could go on (so picking another thread is a preemption).
    from_runnable: bool,
    /// The threads that could run.
    runnable: Vec<ThreadIndex>,
    /// The default choice, explored first.
    default: ThreadIndex,
    /// The branch being explored.
    chosen: ThreadIndex,
    /// The branches explored or being explored, in order.
    explored: Vec<ThreadIndex>,
    /// Branches that only reorder independent transitions of an explored one.
    pruned: Vec<ThreadIndex>,
    /// Everything below an earlier transition already explored: no branch here is new.
    redundant: bool,
    /// The preemptions of the path before this choice point.
    preemptions_before: u32,
}

impl Frame {
    /// The preemptions picking `thread` here costs.
    fn cost(&self, thread: ThreadIndex) -> u32 {
        u32::from(self.from_runnable && thread != self.from)
    }
}

/// Check that a run followed the kept path, and add frames for its choice points beyond it.
fn extend(frames: &mut Vec<Frame>, kept: usize, trace: &Trace) -> Result<(), String> {
    let mut index = 0;
    let mut preemptions = 0;
    for (decision, step) in trace.steps.iter().enumerate() {
        if step.runnable.len() < 2 {
            continue;
        }
        let from_runnable = step.runnable.contains(&step.from);
        let default = if from_runnable {
            step.from
        } else {
            step.runnable[0]
        };
        if index < kept {
            let frame = &frames[index];
            if frame.decision != decision
                || frame.from != step.from
                || frame.runnable != step.runnable
                || frame.chosen != step.to
            {
                return Err(format!(
                    "the run diverged from its schedule at choice point {index}: the scenario \
                     is not deterministic"
                ));
            }
            preemptions = frame.preemptions_before + frame.cost(frame.chosen);
        } else {
            if step.to != default {
                return Err(format!(
                    "the run did not take the default choice at choice point {index}"
                ));
            }
            frames.push(Frame {
                decision,
                point: u64::try_from(index).unwrap_or(u64::MAX),
                from: step.from,
                from_runnable,
                runnable: step.runnable.clone(),
                default,
                chosen: default,
                explored: vec![default],
                pruned: Vec::new(),
                redundant: false,
                preemptions_before: preemptions,
            });
        }
        index += 1;
    }
    if index < kept {
        return Err(format!(
            "the run ended after {index} of its schedule's {kept} choice points: the scenario \
             is not deterministic"
        ));
    }
    Ok(())
}

/// Mark the branches of the path's choice points that sleep sets skip (see the module
/// documentation), from what this run's transitions touched.
fn prune(frames: &mut [Frame], trace: &Trace) {
    let steps = &trace.steps;
    for p in 0..steps.len() {
        let thread = steps[p].to;
        // The transition `thread` makes after decision `p` is recorded at the next decision.
        let Some(next) = steps.get(p + 1) else {
            continue;
        };
        let footprint = &next.footprint;
        if next.from != thread || footprint.any || next.runnable.contains(&thread) {
            // Untracked, or `thread` goes on afterwards: moving it earlier could cost a
            // preemption.
            continue;
        }
        // The earliest decision from which every transition up to `p` is by another thread
        // and independent of this one.
        let mut lo = p;
        while lo > 0 {
            let d = lo - 1;
            if steps[d].to == thread || steps[d + 1].footprint.conflicts(footprint) {
                break;
            }
            lo = d;
        }
        // The earliest choice point in that span where `thread`'s branch was explored before
        // the one taken, at no greater cost.
        let Some(start) = frames
            .iter()
            .find(|f| {
                f.decision >= lo
                    && f.decision < p
                    && f.chosen != thread
                    && f.explored.contains(&thread)
                    && f.cost(thread) <= f.cost(f.chosen)
            })
            .map(|f| f.decision)
        else {
            continue;
        };
        for frame in frames.iter_mut() {
            if frame.decision > start
                && frame.decision < p
                && frame.runnable.contains(&thread)
                && !frame.explored.contains(&thread)
                && !frame.pruned.contains(&thread)
            {
                frame.pruned.push(thread);
            }
            // The run itself made the sleeping transition at `p`: whatever follows was
            // explored in `thread`'s earlier branch.
            if frame.decision > p {
                frame.redundant = true;
            }
        }
    }
}
