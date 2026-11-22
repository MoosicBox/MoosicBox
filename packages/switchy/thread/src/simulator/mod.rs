//! The deterministic thread scheduler.
//!
//! Every simulation run (a [`switchy_simulation::Context`]) has one [`Scheduler`]. Simulated
//! threads are real OS threads, but only the thread holding the *baton* runs; all others are
//! parked. At each switch point the running thread records why it stopped, the scheduler picks
//! the next thread with the run's seeded strategy, hands it the baton and parks until the baton
//! comes back. Because only one thread ever runs and every choice comes from the seed, a run's
//! order of execution is a function of its seed alone.
//!
//! The thread that first touches a run's scheduler becomes its *main* thread (id 0). Code
//! usually starts a run with [`run`], which creates a fresh context, runs a closure as the main
//! thread and reports what happened.
//!
//! A thread belongs to a *group* (see [`with_group`] and [`thread::Builder::group`]), and
//! [`Scheduler::kill_group`] kills a group's threads, as a crashed process's threads die: see
//! there for what that means for a thread in the middle of its work.
//!
//! Every run records its [`Schedule`]: which thread it picked wherever it had a choice. A
//! schedule replays with [`Strategy::Replay`] (or `SIMULATOR_SCHEDULE`), without the
//! scheduling generator, and [`shrink`] reduces a failing one to the fewest choices that still
//! fail.

mod memory;
pub mod sync;
pub mod thread;

use std::{
    cell::Cell,
    collections::BTreeSet,
    fmt::Write as _,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

pub use switchy_simulation::Context;

/// Index of a simulated thread within its run. The main thread is 0.
pub type ThreadIndex = usize;

/// What a simulated thread is waiting for while blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WaitKey {
    /// A mutex or the write side of a read-write lock to become free.
    Lock(u64),
    /// A condition variable to be notified.
    Condvar(u64),
    /// A channel to have a message or lose its senders.
    Recv(u64),
    /// A channel to have room or lose its receiver.
    Send(u64),
    /// A thread to finish.
    Join(ThreadIndex),
    /// Time to pass.
    Sleep,
    /// A simulated device or kernel object (from another simulator, such as `switchy_pty`) to
    /// change; the key is from [`new_key`].
    Io(u64),
    /// An async executor (such as `switchy_async`'s simulator) with nothing to run, to be woken
    /// by a task's waker; the key is from [`new_key`].
    Executor(u64),
}

/// The operation at which a thread last reached a switch point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum Op {
    Start,
    Spawn,
    Exit,
    Join,
    Yield,
    Sleep,
    Lock,
    Unlock,
    CondvarWait,
    CondvarNotify,
    Send,
    Recv,
    AtomicLoad,
    AtomicStore,
    AtomicUpdate,
    Idle,
    /// An operation on a simulated device or kernel object.
    Io,
    /// An async executor waiting for a task to wake.
    Executor,
}

impl Op {
    const fn code(self) -> u8 {
        self as u8
    }
}

/// How the scheduler picks the next thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// A uniformly random runnable thread at every switch point.
    Random,
    /// Probabilistic concurrency testing: the runnable thread with the highest random
    /// priority runs; at `depth` random decisions the running thread's priority drops below
    /// all others. Finds bugs that need a specific short sequence of switches with a known
    /// probability.
    Pct {
        /// Number of priority-change points.
        depth: u32,
        /// The decision range the change points are drawn from.
        steps: u64,
    },
    /// Follow a recorded [`Schedule`]: its choices where it has one (if that thread can run),
    /// the default choice everywhere else.
    Replay(Arc<Schedule>),
}

/// Where a run had a choice of thread, the ones it did not pick by default.
///
/// A *choice point* is a decision with more than one runnable thread. The default choice keeps
/// the stopping thread running if it can, and otherwise picks the lowest-numbered runnable
/// thread, so a schedule lists only the switches a run made beyond the ones it had to make.
/// Its text form, used by `SIMULATOR_SCHEDULE`, is `choice=thread` pairs separated by commas
/// (`none` for the default schedule), for example `3=1,17=2`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schedule {
    /// The choice points (numbered from 0) where a run did not take the default choice, in
    /// order, with the thread it picked.
    pub choices: Vec<(u64, ThreadIndex)>,
}

impl Schedule {
    /// The default schedule: no choice beyond the default anywhere.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            choices: Vec::new(),
        }
    }

    /// The thread to pick at choice point `point`, if the schedule names one.
    fn at(&self, point: u64) -> Option<ThreadIndex> {
        self.choices
            .binary_search_by_key(&point, |&(p, _)| p)
            .ok()
            .map(|i| self.choices[i].1)
    }
}

impl std::fmt::Display for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.choices.is_empty() {
            return f.write_str("none");
        }
        for (i, (point, thread)) in self.choices.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{point}={thread}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Schedule {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() || text == "none" {
            return Ok(Self::new());
        }
        let mut choices: Vec<(u64, ThreadIndex)> = text
            .split(',')
            .map(|pair| {
                let (point, thread) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("expected choice=thread, got '{pair}'"))?;
                Ok((
                    point
                        .trim()
                        .parse()
                        .map_err(|_| format!("bad choice '{point}'"))?,
                    thread
                        .trim()
                        .parse()
                        .map_err(|_| format!("bad thread '{thread}'"))?,
                ))
            })
            .collect::<Result<_, String>>()?;
        choices.sort_unstable();
        choices.dedup_by_key(|&mut (point, _)| point);
        Ok(Self { choices })
    }
}

/// The scheduler's configuration for one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// How threads are picked.
    pub strategy: Strategy,
    /// Decisions kept in [`Scheduler::recent_decisions`].
    pub recent_decisions: usize,
    /// Model weak memory: atomic loads may return any store the memory model allows (see
    /// `SIMULATOR_WEAK_MEMORY`). Slower; finds bugs in `Relaxed` and `Acquire`/`Release` use.
    pub weak_memory: bool,
}

impl Config {
    /// Uniformly random scheduling.
    #[must_use]
    pub const fn random() -> Self {
        Self {
            strategy: Strategy::Random,
            recent_decisions: 256,
            weak_memory: false,
        }
    }

    /// PCT scheduling with `depth` change points drawn from the first `steps` decisions.
    #[must_use]
    pub const fn pct(depth: u32, steps: u64) -> Self {
        Self {
            strategy: Strategy::Pct { depth, steps },
            recent_decisions: 256,
            weak_memory: false,
        }
    }

    /// Replay `schedule`.
    #[must_use]
    pub fn replay(schedule: Schedule) -> Self {
        Self {
            strategy: Strategy::Replay(Arc::new(schedule)),
            recent_decisions: 256,
            weak_memory: false,
        }
    }

    /// The configuration from the environment for a run with `seed`:
    ///
    /// * `SIMULATOR_SCHEDULE_STRATEGY`: `random`, `pct`, `alternate` (the default), which
    ///   picks `random` for even seeds and `pct` for odd ones, or `swarm`, which draws the
    ///   strategy and PCT's depth and steps from the seed (one run in three random, the rest
    ///   PCT with a depth from 1 to 6 among the first 1000, 10000 or 100000 decisions).
    /// * `SIMULATOR_PCT_DEPTH` (default 3) and `SIMULATOR_PCT_STEPS` (default 10000); under
    ///   `swarm`, they fix what they name instead of drawing it.
    /// * `SIMULATOR_WEAK_MEMORY`: `1` or `true` to model weak memory, or `swarm` to model it in
    ///   one run in four, drawn from the seed.
    /// * `SIMULATOR_SCHEDULE`: a [`Schedule`] to replay, overriding the strategy.
    ///
    /// # Panics
    ///
    /// * If one of the variables is set to an invalid value
    #[must_use]
    pub fn from_env(seed: u64) -> Self {
        let pct = Strategy::Pct {
            depth: env_number("SIMULATOR_PCT_DEPTH").unwrap_or(3),
            steps: env_number("SIMULATOR_PCT_STEPS").unwrap_or(10_000),
        };
        let replay = std::env::var("SIMULATOR_SCHEDULE").ok().map(|text| {
            text.parse::<Schedule>()
                .unwrap_or_else(|error| panic!("SIMULATOR_SCHEDULE: {error}"))
        });
        let strategy = match std::env::var("SIMULATOR_SCHEDULE_STRATEGY").as_deref() {
            _ if replay.is_some() => Strategy::Replay(Arc::new(replay.unwrap_or_default())),
            Ok("random") => Strategy::Random,
            Ok("pct") => pct,
            Ok("swarm") => {
                if draw(seed, 1, 3) == 0 {
                    Strategy::Random
                } else {
                    Strategy::Pct {
                        depth: env_number("SIMULATOR_PCT_DEPTH")
                            .unwrap_or_else(|| 1 + u32::try_from(draw(seed, 2, 6)).unwrap_or(0)),
                        steps: env_number("SIMULATOR_PCT_STEPS").unwrap_or_else(|| {
                            [1_000, 10_000, 100_000][usize::try_from(draw(seed, 3, 3)).unwrap_or(0)]
                        }),
                    }
                }
            }
            Ok("alternate") | Err(_) => {
                if seed.is_multiple_of(2) {
                    Strategy::Random
                } else {
                    pct
                }
            }
            Ok(other) => {
                panic!("SIMULATOR_SCHEDULE_STRATEGY must be random|pct|alternate, got '{other}'")
            }
        };
        Self {
            strategy,
            recent_decisions: 256,
            weak_memory: match std::env::var("SIMULATOR_WEAK_MEMORY").as_deref() {
                Ok("1" | "true") => true,
                Ok("swarm") => draw(seed, 4, 4) == 0,
                _ => false,
            },
        }
    }

    /// The configuration in a few words, for people: `random`, `pct(3/10000)`, and `+weak`
    /// when it models weak memory.
    #[must_use]
    pub fn describe(&self) -> String {
        let strategy = match &self.strategy {
            Strategy::Random => "random".to_owned(),
            Strategy::Pct { depth, steps } => format!("pct({depth}/{steps})"),
            Strategy::Replay(_) => "replay".to_owned(),
        };
        if self.weak_memory {
            format!("{strategy}+weak")
        } else {
            strategy
        }
    }
}

/// A number below `bound` drawn from `seed` for the configuration choice `salt` (a `SplitMix64` step),
/// so each choice is fixed by the seed and independent of the others.
fn draw(seed: u64, salt: u64, bound: u64) -> u64 {
    let mut z = seed.wrapping_add(salt.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) % bound.max(1)
}

fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().map(|value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("{name} must be a number, got '{value}'"))
    })
}

/// A thread's scheduling state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadState {
    /// Ready to run.
    Runnable,
    /// Waiting for `key`, or until `deadline` if it has one.
    Blocked {
        /// What the thread waits for.
        key: WaitKey,
        /// When the wait times out.
        deadline: Option<Instant>,
    },
    /// The main thread waiting in [`run_until_idle`] for the others to have nothing to do.
    Idle,
    /// Done; waiting for the run to end.
    Finished,
}

/// A read-only view of one simulated thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadSnapshot {
    /// The thread's index in its run.
    pub index: ThreadIndex,
    /// Its name.
    pub name: String,
    /// Its scheduling state.
    pub state: ThreadState,
    /// The operation it last reached a switch point at.
    pub op: Op,
    /// Whether it ended by panicking.
    pub panicked: bool,
}

/// One scheduling decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    /// The decision's number in the run, from 0.
    pub number: u64,
    /// The thread that reached the switch point.
    pub from: ThreadIndex,
    /// The operation it reached.
    pub op: Op,
    /// The thread picked to run next.
    pub to: ThreadIndex,
    /// The simulation step when the decision was made.
    pub step: u64,
}

/// A read-only view of a run's scheduler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchedulerSnapshot {
    /// Every thread of the run.
    pub threads: Vec<ThreadSnapshot>,
    /// The thread holding the baton.
    pub running: ThreadIndex,
    /// Decisions made so far.
    pub decisions: u64,
    /// The running hash of every decision.
    pub trace_hash: u64,
    /// The most recent decisions, oldest first.
    pub recent: Vec<Decision>,
}

struct ThreadRecord {
    name: String,
    state: ThreadState,
    op: Op,
    os: Option<std::thread::Thread>,
    priority: u64,
    timed_out: bool,
    panicked: bool,
    /// The thread's group (0 for none); see [`Scheduler::kill_group`].
    group: u64,
    /// Whether it was killed; see [`Scheduler::kill_group`].
    fate: Fate,
}

/// Whether a thread was killed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    Alive,
    /// Killed: it unwinds at its next switch point, running no more of its code.
    Killed,
    /// Killed, and blocked again while unwinding: parked for good.
    Abandoned,
}

struct Inner {
    threads: Vec<ThreadRecord>,
    running: ThreadIndex,
    decisions: u64,
    trace_hash: u64,
    config: Config,
    /// Choice points so far, and the non-default choices made at them.
    choice_points: u64,
    schedule: Schedule,
    change_points: BTreeSet<u64>,
    lowest_priority: u64,
    recent: std::collections::VecDeque<Decision>,
    failure: Option<String>,
    shutting_down: bool,
    os_handles: Vec<std::thread::JoinHandle<()>>,
    memory: memory::Memory,
    /// The thread waiting in [`Scheduler::kill_group`] for a killed thread to unwind.
    killer: Option<ThreadIndex>,
}

/// Called at every scheduling decision, before the chosen thread runs.
///
/// It gets the decision and a way to take a [`SchedulerSnapshot`], and runs while the
/// scheduler is locked: no simulated thread runs until it returns, so an observer may block in
/// it to pause the run (only real time passes). It must not call back into the scheduler.
pub type DecisionHook = Arc<dyn Fn(&Decision, &dyn Fn() -> SchedulerSnapshot) + Send + Sync>;

/// The scheduler of one simulation run. See the [module documentation](self).
pub struct Scheduler {
    context_id: u64,
    /// Scheduling choices. Replaying a schedule draws nothing from it, so everything else
    /// draws from streams of its own and replays the same.
    rng: Arc<Mutex<switchy_simulation::Rng>>,
    /// Which waiter a `wake_one` wakes.
    wake_rng: Arc<Mutex<switchy_simulation::Rng>>,
    /// Which store a weak-memory load reads.
    memory_rng: Arc<Mutex<switchy_simulation::Rng>>,
    inner: Mutex<Inner>,
    hook: Mutex<Option<DecisionHook>>,
}

impl std::fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scheduler")
            .field("context_id", &self.context_id)
            .finish_non_exhaustive()
    }
}

thread_local! {
    /// The (context id, thread index) of the simulated thread running on this OS thread.
    static IDENTITY: Cell<Option<(u64, ThreadIndex)>> = const { Cell::new(None) };
    /// The group threads spawned from this OS thread join, if set by [`with_group`].
    static GROUP: Cell<Option<u64>> = const { Cell::new(None) };
}

/// The panic payload a killed thread unwinds with (see [`Scheduler::kill_group`]).
///
/// It unwinds with [`std::panic::resume_unwind`], so no panic hook sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Killed;

/// Run `f` with the threads it spawns (directly, from the calling OS thread) in `group`.
///
/// That is unless they are given one with [`thread::Builder::group`]. Threads spawned by those
/// threads inherit their group. An async executor wraps each task's poll in this, so the threads a task
/// starts belong to its executor.
pub fn with_group<R>(group: u64, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<u64>);
    impl Drop for Restore {
        fn drop(&mut self) {
            GROUP.set(self.0);
        }
    }
    let _restore = Restore(GROUP.replace(Some(group)));
    f()
}

/// The group set by [`with_group`] on the calling OS thread, if any.
pub(crate) fn current_group() -> Option<u64> {
    GROUP.get()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

const FNV_PRIME: u64 = 0x0100_0000_01b3;

fn hash_u64(hash: u64, value: u64) -> u64 {
    value
        .to_le_bytes()
        .iter()
        .fold(hash, |h, &b| (h ^ u64::from(b)).wrapping_mul(FNV_PRIME))
}

impl Scheduler {
    /// The current run's scheduler, created with the calling thread as its main thread on
    /// first use.
    ///
    /// # Panics
    ///
    /// * If the calling OS thread is neither the run's main thread nor one of its simulated
    ///   threads
    #[must_use]
    pub fn current() -> Arc<Self> {
        let context = switchy_simulation::current();
        let scheduler = context.state_with(|| Self::new(&context));
        if IDENTITY.get().is_none_or(|(id, _)| id != context.id()) {
            let mut inner = lock(&scheduler.inner);
            assert!(
                inner.threads[0].os.is_none(),
                "this OS thread is not a simulated thread of the current run; spawn it with \
                 switchy_thread::thread::spawn"
            );
            inner.threads[0].os = Some(std::thread::current());
            drop(inner);
            IDENTITY.set(Some((context.id(), 0)));
        }
        scheduler
    }

    fn new(context: &Context) -> Self {
        let config = context
            .state_with(|| ConfigOverride(None))
            .0
            .clone()
            .unwrap_or_else(|| Config::from_env(context.seed()));
        let rng = context.stream("switchy_thread");
        let mut change_points = BTreeSet::new();
        let mut main_priority = 0;
        let mut lowest_priority = 0;
        if let Strategy::Pct { depth, steps } = &config.strategy {
            let (depth, steps) = (*depth, *steps);
            let mut rng = lock(&rng);
            for _ in 0..depth {
                change_points.insert(rng.below(steps.max(1)));
            }
            lowest_priority = u64::from(depth) + 1;
            main_priority = lowest_priority + rng.below(u64::MAX / 2);
        }
        Self {
            context_id: context.id(),
            rng,
            wake_rng: context.stream("switchy_thread::wake"),
            memory_rng: context.stream("switchy_thread::memory"),
            hook: Mutex::new(None),
            inner: Mutex::new(Inner {
                threads: vec![ThreadRecord {
                    name: std::thread::current().name().unwrap_or("main").to_owned(),
                    state: ThreadState::Runnable,
                    op: Op::Start,
                    os: None,
                    priority: main_priority,
                    timed_out: false,
                    panicked: false,
                    group: 0,
                    fate: Fate::Alive,
                }],
                running: 0,
                decisions: 0,
                trace_hash: 0xcbf2_9ce4_8422_2325,
                config,
                choice_points: 0,
                schedule: Schedule::new(),
                change_points,
                lowest_priority,
                recent: std::collections::VecDeque::new(),
                failure: None,
                shutting_down: false,
                os_handles: Vec::new(),
                memory: memory::Memory::default(),
                killer: None,
            }),
        }
    }

    /// Whether any simulated thread but the caller has not finished.
    ///
    /// # Panics
    ///
    /// * If the calling OS thread is not a simulated thread of this run
    #[must_use]
    pub fn others_alive(&self) -> bool {
        let me = self.me();
        lock(&self.inner)
            .threads
            .iter()
            .enumerate()
            .any(|(index, thread)| index != me && thread.state != ThreadState::Finished)
    }

    /// The calling thread's index in this run.
    ///
    /// # Panics
    ///
    /// * If the calling OS thread is not a simulated thread of this run
    #[must_use]
    pub fn me(&self) -> ThreadIndex {
        match IDENTITY.get() {
            Some((context, index)) if context == self.context_id => index,
            _ => panic!("this OS thread is not a simulated thread of this run"),
        }
    }

    /// A read-only view of the scheduler. Taking it never changes the run.
    #[must_use]
    pub fn snapshot(&self) -> SchedulerSnapshot {
        Self::snapshot_of(&lock(&self.inner))
    }

    /// Observe every decision of this run with `hook` (or stop, with `None`).
    pub fn set_decision_hook(&self, hook: Option<DecisionHook>) {
        *lock(&self.hook) = hook;
    }

    fn snapshot_of(inner: &Inner) -> SchedulerSnapshot {
        SchedulerSnapshot {
            threads: inner
                .threads
                .iter()
                .enumerate()
                .map(|(index, t)| ThreadSnapshot {
                    index,
                    name: t.name.clone(),
                    state: t.state,
                    op: t.op,
                    panicked: t.panicked,
                })
                .collect(),
            running: inner.running,
            decisions: inner.decisions,
            trace_hash: inner.trace_hash,
            recent: inner.recent.iter().copied().collect(),
        }
    }

    /// The run's configuration.
    #[must_use]
    pub fn config(&self) -> Config {
        lock(&self.inner).config.clone()
    }

    /// The running hash of every scheduling decision so far.
    #[must_use]
    pub fn trace_hash(&self) -> u64 {
        lock(&self.inner).trace_hash
    }

    /// The number of scheduling decisions so far.
    #[must_use]
    pub fn decisions(&self) -> u64 {
        lock(&self.inner).decisions
    }

    /// Mix an observable value of the run (an output, an event) into the trace hash, so a
    /// determinism check compares results as well as schedules.
    pub fn record(&self, value: u64) {
        let mut inner = lock(&self.inner);
        inner.trace_hash = hash_u64(inner.trace_hash, value);
    }

    /// Register a new simulated thread, returning its index. It starts runnable but does not
    /// run until picked.
    fn add_thread(&self, parent: ThreadIndex, name: String, group: Option<u64>) -> ThreadIndex {
        let priority = {
            let mut rng = lock(&self.rng);
            let inner = lock(&self.inner);
            match &inner.config.strategy {
                Strategy::Pct { depth, .. } => u64::from(*depth) + 1 + rng.below(u64::MAX / 2),
                Strategy::Random | Strategy::Replay(_) => 0,
            }
        };
        let mut inner = lock(&self.inner);
        let group = group.unwrap_or(inner.threads[parent].group);
        inner.threads.push(ThreadRecord {
            name,
            state: ThreadState::Runnable,
            op: Op::Start,
            os: None,
            priority,
            timed_out: false,
            panicked: false,
            group,
            fate: Fate::Alive,
        });
        let child = inner.threads.len() - 1;
        if inner.config.weak_memory {
            inner.memory.fence(parent);
            inner.memory.spawned(parent, child);
        }
        child
    }

    fn attach(&self, index: ThreadIndex, handle: std::thread::JoinHandle<()>) {
        let mut inner = lock(&self.inner);
        inner.threads[index].os = Some(handle.thread().clone());
        inner.os_handles.push(handle);
    }

    /// A switch point: the calling thread stays runnable, but another may run first.
    pub fn yield_point(&self, op: Op) {
        if std::thread::panicking() && self.is_shutting_down() {
            return;
        }
        self.switch(op, None);
    }

    /// Block the calling thread on `key` until another thread wakes it, or until `deadline`.
    /// Returns whether the wait timed out.
    pub fn block(&self, op: Op, key: WaitKey, deadline: Option<Instant>) -> bool {
        self.switch(op, Some((key, deadline)))
    }

    /// Whether this run models weak memory.
    pub(crate) fn weak_memory(&self) -> bool {
        lock(&self.inner).config.weak_memory
    }

    /// Pick one of `count` allowed stores: the latest half the time, otherwise any.
    fn choose_store(&self, count: usize) -> usize {
        if count <= 1 {
            return 0;
        }
        let mut rng = lock(&self.memory_rng);
        if rng.chance(0.5) {
            count - 1
        } else {
            rng.index(count)
        }
    }

    /// A weak-memory load of location `id` (whose latest value is `current`).
    pub(crate) fn atomic_load(
        &self,
        id: u64,
        current: u64,
        order: std::sync::atomic::Ordering,
    ) -> u64 {
        let me = self.me();
        let mut inner = lock(&self.inner);
        let location = usize::try_from(id).unwrap_or(usize::MAX);
        inner.memory.load(me, location, current, order, |count| {
            self.choose_store(count)
        })
    }

    /// A weak-memory store to location `id`.
    pub(crate) fn atomic_store(
        &self,
        id: u64,
        current: u64,
        bits: u64,
        order: std::sync::atomic::Ordering,
    ) {
        let me = self.me();
        let location = usize::try_from(id).unwrap_or(usize::MAX);
        lock(&self.inner)
            .memory
            .store(me, location, current, bits, order);
    }

    /// A weak-memory read-modify-write of location `id`; returns the value read.
    pub(crate) fn atomic_update(
        &self,
        id: u64,
        current: u64,
        order: std::sync::atomic::Ordering,
        update: impl FnOnce(u64) -> Option<u64>,
    ) -> u64 {
        let me = self.me();
        let location = usize::try_from(id).unwrap_or(usize::MAX);
        lock(&self.inner)
            .memory
            .update(me, location, current, order, update)
    }

    /// Make every thread blocked on `key` runnable. They run when picked.
    pub fn wake_all(&self, key: WaitKey) {
        let mut inner = lock(&self.inner);
        for thread in &mut inner.threads {
            if matches!(thread.state, ThreadState::Blocked { key: k, .. } if k == key) {
                thread.state = ThreadState::Runnable;
            }
        }
    }

    /// Make one thread blocked on `key` runnable, picked with the run's seed. Returns whether
    /// there was one.
    pub fn wake_one(&self, key: WaitKey) -> bool {
        let mut inner = lock(&self.inner);
        let waiting: Vec<ThreadIndex> = inner
            .threads
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t.state, ThreadState::Blocked { key: k, .. } if k == key))
            .map(|(i, _)| i)
            .collect();
        if waiting.is_empty() {
            return false;
        }
        let chosen = waiting[lock(&self.wake_rng).index(waiting.len())];
        inner.threads[chosen].state = ThreadState::Runnable;
        true
    }

    fn is_shutting_down(&self) -> bool {
        lock(&self.inner).shutting_down
    }

    fn switch(&self, op: Op, block: Option<(WaitKey, Option<Instant>)>) -> bool {
        let me = self.me();
        let mut inner = lock(&self.inner);
        if inner.threads[me].fate != Fate::Alive {
            // A destructor of a killed thread reached a switch point while it unwound. A
            // yield passes; a wait could never end well, so the thread goes no further.
            if block.is_none() {
                return false;
            }
            self.abandon(inner, me);
        }
        if inner.config.weak_memory
            && !matches!(op, Op::AtomicLoad | Op::AtomicStore | Op::AtomicUpdate)
        {
            // Non-atomic synchronization orders everything, conservatively.
            inner.memory.fence(me);
        }
        inner.threads[me].op = op;
        inner.threads[me].timed_out = false;
        inner.threads[me].state = match block {
            None => ThreadState::Runnable,
            Some((key, deadline)) => ThreadState::Blocked { key, deadline },
        };
        self.schedule(&mut inner, me, op);
        drop(inner);
        self.wait_turn(me);
        let mut inner = lock(&self.inner);
        if inner.threads[me].fate != Fate::Alive {
            drop(inner);
            std::panic::resume_unwind(Box::new(Killed));
        }
        std::mem::take(&mut inner.threads[me].timed_out)
    }

    /// Kill every unfinished thread of `group` (other than the caller), in index order.
    ///
    /// A killed thread runs none of its code again. It is stopped where it is: at a switch
    /// point (blocked, or runnable and waiting for its turn), since only the caller runs. It
    /// gets the baton once, right away, to unwind from that switch point with a [`Killed`]
    /// payload, so the values on its stack are dropped, as a dropped future's are (locks it
    /// holds are released, poisoned), and then finishes. Joining it returns that payload as an
    /// error. A thread killed before it started drops its closure without calling it. A
    /// destructor that blocks while the thread unwinds (waiting for a lock, say) stops it there
    /// for good: the thread is parked forever, keeping whatever it still holds. Each kill hands the baton straight back
    /// to the caller, so no other thread runs meanwhile, and killing is deterministic.
    ///
    /// Group 0 (threads without a group) is never killed.
    ///
    /// # Panics
    ///
    /// * If the calling OS thread is not a simulated thread of this run
    pub fn kill_group(&self, group: u64) {
        if group == 0 {
            return;
        }
        let me = self.me();
        let victims: Vec<ThreadIndex> = {
            let inner = lock(&self.inner);
            inner
                .threads
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    *i != me && *i != 0 && t.group == group && t.state != ThreadState::Finished
                })
                .map(|(i, _)| i)
                .collect()
        };
        for victim in victims {
            let mut inner = lock(&self.inner);
            if inner.threads[victim].state == ThreadState::Finished {
                continue;
            }
            inner.trace_hash = hash_u64(inner.trace_hash, victim as u64);
            inner.threads[victim].fate = Fate::Killed;
            inner.threads[victim].state = ThreadState::Runnable;
            inner.killer = Some(me);
            inner.running = victim;
            if let Some(os) = &inner.threads[victim].os {
                os.unpark();
            }
            drop(inner);
            self.wait_turn(me);
            lock(&self.inner).killer = None;
        }
    }

    /// Whether the calling thread has been killed (and should not run its code).
    pub(crate) fn is_killed(&self, me: ThreadIndex) -> bool {
        lock(&self.inner).threads[me].fate != Fate::Alive
    }

    /// Park a killed thread for good, handing the baton back to its killer.
    fn abandon(&self, mut inner: MutexGuard<'_, Inner>, me: ThreadIndex) -> ! {
        inner.threads[me].fate = Fate::Abandoned;
        inner.threads[me].state = ThreadState::Finished;
        inner.threads[me].op = Op::Exit;
        self.return_to_killer(&mut inner, me);
        drop(inner);
        loop {
            std::thread::park();
        }
    }

    /// Hand the baton from the killed thread `me` back to the thread that killed it.
    fn return_to_killer(&self, inner: &mut Inner, me: ThreadIndex) {
        let _ = self;
        let killer = inner.killer.unwrap_or(0);
        inner.running = killer;
        if killer != me
            && let Some(os) = &inner.threads[killer].os
        {
            os.unpark();
        }
    }

    /// Pick the next thread after `from` reached a switch point at `op`, and hand it the
    /// baton.
    fn schedule(&self, inner: &mut Inner, from: ThreadIndex, op: Op) {
        let next = loop {
            Self::expire_timers(inner);
            let runnable: Vec<ThreadIndex> = inner
                .threads
                .iter()
                .enumerate()
                .filter(|(_, t)| t.state == ThreadState::Runnable)
                .map(|(i, _)| i)
                .collect();
            if !runnable.is_empty() {
                break self.pick(inner, from, &runnable);
            }
            if let Some(idle) = inner
                .threads
                .iter()
                .position(|t| t.state == ThreadState::Idle)
            {
                inner.threads[idle].state = ThreadState::Runnable;
                break idle;
            }
            let earliest = inner
                .threads
                .iter()
                .filter_map(|t| match t.state {
                    ThreadState::Blocked { deadline, .. } => deadline,
                    _ => None,
                })
                .min();
            if let Some(deadline) = earliest {
                let _ = switchy_time::simulator::advance_to(deadline);
                continue;
            }
            inner.failure = Some(Self::deadlock_report(inner));
            break 0;
        };
        let step = switchy_time::simulator::current_step();
        let number = inner.decisions;
        inner.decisions += 1;
        let mut hash = inner.trace_hash;
        for value in [number, from as u64, u64::from(op.code()), next as u64, step] {
            hash = hash_u64(hash, value);
        }
        inner.trace_hash = hash;
        let decision = Decision {
            number,
            from,
            op,
            to: next,
            step,
        };
        if inner.config.recent_decisions > 0 {
            if inner.recent.len() == inner.config.recent_decisions {
                inner.recent.pop_front();
            }
            inner.recent.push_back(decision);
        }
        inner.running = next;
        let hook = lock(&self.hook).clone();
        if let Some(hook) = hook {
            let view: &Inner = inner;
            hook(&decision, &|| Self::snapshot_of(view));
        }
        if next != from
            && let Some(os) = &inner.threads[next].os
        {
            os.unpark();
        }
    }

    fn expire_timers(inner: &mut Inner) {
        let now = switchy_time::simulator::instant_now();
        for thread in &mut inner.threads {
            if let ThreadState::Blocked {
                deadline: Some(deadline),
                ..
            } = thread.state
                && deadline <= now
            {
                thread.state = ThreadState::Runnable;
                thread.timed_out = true;
            }
        }
    }

    /// Pick among `runnable`, recording the choice in the run's schedule.
    fn pick(&self, inner: &mut Inner, from: ThreadIndex, runnable: &[ThreadIndex]) -> ThreadIndex {
        if runnable.len() == 1 {
            return runnable[0];
        }
        let default = if runnable.contains(&from) {
            from
        } else {
            runnable[0]
        };
        let point = inner.choice_points;
        inner.choice_points += 1;
        let chosen = self.choose(inner, from, runnable, point, default);
        if chosen != default {
            inner.schedule.choices.push((point, chosen));
        }
        chosen
    }

    fn choose(
        &self,
        inner: &mut Inner,
        from: ThreadIndex,
        runnable: &[ThreadIndex],
        point: u64,
        default: ThreadIndex,
    ) -> ThreadIndex {
        let strategy = inner.config.strategy.clone();
        match &strategy {
            Strategy::Replay(schedule) => schedule
                .at(point)
                .filter(|thread| runnable.contains(thread))
                .unwrap_or(default),
            Strategy::Random => runnable[lock(&self.rng).index(runnable.len())],
            Strategy::Pct { .. } => {
                if inner.change_points.remove(&inner.decisions) {
                    // Drop the running thread below every priority given so far: initial
                    // priorities are all above `depth`, and each change point goes lower.
                    inner.lowest_priority = inner.lowest_priority.saturating_sub(1);
                    inner.threads[from].priority = inner.lowest_priority;
                }
                *runnable
                    .iter()
                    .max_by_key(|&&i| (inner.threads[i].priority, std::cmp::Reverse(i)))
                    .unwrap_or(&runnable[0])
            }
        }
    }

    fn deadlock_report(inner: &Inner) -> String {
        let mut report = String::from("simulation deadlock: every thread is blocked\n");
        for (index, thread) in inner.threads.iter().enumerate() {
            let _ = writeln!(
                report,
                "  #{index} {:?}: {:?} (at {:?})",
                thread.name, thread.state, thread.op
            );
        }
        report
    }

    /// Park until the calling thread holds the baton.
    fn wait_turn(&self, me: ThreadIndex) {
        loop {
            {
                let inner = lock(&self.inner);
                if me == 0
                    && let Some(failure) = &inner.failure
                {
                    if std::thread::panicking() {
                        // Already unwinding: let it finish rather than abort on a second panic.
                        return;
                    }
                    let failure = failure.clone();
                    drop(inner);
                    panic!("{failure}");
                }
                if inner.running == me && inner.threads[me].state == ThreadState::Runnable {
                    return;
                }
                if inner.shutting_down
                    && inner.threads[me].state == ThreadState::Finished
                    && inner.threads[me].fate != Fate::Abandoned
                {
                    return;
                }
            }
            std::thread::park();
        }
    }

    /// Mark the calling simulated thread finished and hand the baton on.
    fn exit(&self, me: ThreadIndex, panicked: bool) {
        let mut inner = lock(&self.inner);
        if inner.config.weak_memory {
            inner.memory.fence(me);
        }
        inner.threads[me].state = ThreadState::Finished;
        inner.threads[me].panicked = panicked;
        inner.threads[me].op = Op::Exit;
        for thread in &mut inner.threads {
            if matches!(thread.state, ThreadState::Blocked { key: WaitKey::Join(j), .. } if j == me)
            {
                thread.state = ThreadState::Runnable;
            }
        }
        if inner.threads[me].fate == Fate::Alive {
            self.schedule(&mut inner, me, Op::Exit);
        } else {
            self.return_to_killer(&mut inner, me);
        }
        drop(inner);
    }

    /// Whether the thread finished, and whether it panicked.
    fn finished(&self, index: ThreadIndex) -> bool {
        lock(&self.inner).threads[index].state == ThreadState::Finished
    }

    /// Run the other simulated threads until none can make progress at the current simulated
    /// time, then return to the calling (main) thread. The clock is left for the caller to
    /// advance. For harnesses that drive time themselves, such as simvar.
    ///
    /// # Panics
    ///
    /// * If called from a thread other than the run's main thread
    /// * If the run deadlocked
    pub fn run_until_idle(&self) {
        let me = self.me();
        assert_eq!(me, 0, "run_until_idle must be called from the main thread");
        let mut inner = lock(&self.inner);
        inner.threads[me].op = Op::Idle;
        inner.threads[me].state = ThreadState::Idle;
        self.schedule(&mut inner, me, Op::Idle);
        drop(inner);
        self.wait_turn(me);
    }

    /// End the run: let finished threads exit and join them, so nothing of the run keeps
    /// running. Threads that never finished stay parked forever and are reported.
    ///
    /// # Panics
    ///
    /// * If called from a thread other than the run's main thread
    #[must_use]
    pub fn shutdown(&self) -> ShutdownReport {
        assert_eq!(self.me(), 0, "shutdown must be called from the main thread");
        let (handles, report) = {
            let mut inner = lock(&self.inner);
            inner.shutting_down = true;
            let report = ShutdownReport {
                leaked: inner
                    .threads
                    .iter()
                    .skip(1)
                    .filter(|t| t.state != ThreadState::Finished)
                    .map(|t| t.name.clone())
                    .collect(),
                panicked: inner
                    .threads
                    .iter()
                    .filter(|t| t.panicked)
                    .map(|t| t.name.clone())
                    .collect(),
                decisions: inner.decisions,
                trace_hash: inner.trace_hash,
                schedule: inner.schedule.clone(),
            };
            let finished: Vec<std::thread::Thread> = inner
                .threads
                .iter()
                .skip(1)
                .filter(|t| t.state == ThreadState::Finished && t.fate != Fate::Abandoned)
                .filter_map(|t| t.os.clone())
                .collect();
            for thread in &finished {
                thread.unpark();
            }
            let handles = std::mem::take(&mut inner.os_handles);
            drop(inner);
            (handles, report)
        };
        let finished_ids: Vec<std::thread::ThreadId> = {
            let inner = lock(&self.inner);
            inner
                .threads
                .iter()
                .skip(1)
                .filter(|t| t.state == ThreadState::Finished && t.fate != Fate::Abandoned)
                .filter_map(|t| t.os.as_ref().map(std::thread::Thread::id))
                .collect()
        };
        for handle in handles {
            if finished_ids.contains(&handle.thread().id()) {
                let _ = handle.join();
            }
        }
        report
    }
}

/// What remained of a run when it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownReport {
    /// Simulated threads that never finished (blocked forever). Their OS threads stay parked.
    pub leaked: Vec<String>,
    /// Simulated threads that ended by panicking.
    pub panicked: Vec<String>,
    /// The number of scheduling decisions in the run.
    pub decisions: u64,
    /// The run's final trace hash.
    pub trace_hash: u64,
    /// The run's choices, to replay it with [`Strategy::Replay`].
    pub schedule: Schedule,
}

/// The current run's scheduler; see [`Scheduler::current`].
#[must_use]
pub fn scheduler() -> Arc<Scheduler> {
    Scheduler::current()
}

/// The current run's scheduler, if the calling OS thread is one of its simulated threads.
///
/// That includes the run's main thread once it has used the scheduler. Unlike [`scheduler`],
/// it never makes the caller the run's main thread, so code that may run on any thread (a
/// waker, for example) can use it.
#[must_use]
pub fn try_scheduler() -> Option<Arc<Scheduler>> {
    let (context_id, _) = IDENTITY.get()?;
    let context = switchy_simulation::current();
    (context.id() == context_id).then(Scheduler::current)
}

/// Observe every decision of the current run; see [`Scheduler::set_decision_hook`].
pub fn set_decision_hook(hook: Option<DecisionHook>) {
    scheduler().set_decision_hook(hook);
}

/// Run other simulated threads until idle; see [`Scheduler::run_until_idle`].
pub fn run_until_idle() {
    scheduler().run_until_idle();
}

/// Mix an observable value into the current run's trace hash; see [`Scheduler::record`].
pub fn record(value: u64) {
    scheduler().record(value);
}

/// The outcome of [`run`].
#[derive(Debug)]
pub struct Outcome<R> {
    /// What the closure returned, or its panic.
    pub result: std::thread::Result<R>,
    /// What remained when the run ended.
    pub report: ShutdownReport,
}

/// A configuration fixed for a run by [`run_with`], taking precedence over the environment.
struct ConfigOverride(Option<Config>);

/// Like [`run`], with the scheduler configured by `config` instead of the environment.
pub fn run_with<R>(config: Config, seed: u64, f: impl FnOnce() -> R) -> Outcome<R> {
    let context = Context::new(seed);
    let _ = context.state_with(|| ConfigOverride(Some(config)));
    run_in(context, f)
}

/// Configure the scheduler of the current run, instead of the environment. For harnesses that
/// create runs themselves (such as simvar); call it before anything in the run touches the
/// scheduler.
///
/// # Panics
///
/// * If the current run already has a scheduler or a configuration
pub fn configure(config: Config) {
    let context = switchy_simulation::current();
    let mut config = Some(config);
    let set = context.state_with(|| ConfigOverride(config.take()));
    assert!(
        config.is_none() && set.0.is_some(),
        "the current run is already configured"
    );
}

/// The attempts [`shrink`] makes at most.
pub const SHRINK_ATTEMPTS: usize = 1000;

/// Reduce a failing run's schedule to fewer choices that still fail.
///
/// `fails` runs the scenario once replaying a candidate schedule (with the failing run's seed,
/// for everything but scheduling) and returns the schedule that run actually followed if it
/// failed, or `None` if it passed. This is delta debugging over the schedule's choices: it
/// tries the default schedule, then each chunk of choices alone and the schedule without each
/// chunk, halving the chunks until single choices. Whenever a candidate fails, the schedule
/// that run followed (dropping a choice changes the choice points after it) becomes the new
/// best. Stops after `max_attempts` runs.
///
/// The result fails, and (unless the attempts ran out) dropping any one of its choices makes
/// the run pass. It is `schedule` itself if nothing smaller fails.
pub fn shrink(
    schedule: &Schedule,
    max_attempts: usize,
    mut fails: impl FnMut(&Schedule) -> Option<Schedule>,
) -> Schedule {
    let mut best = schedule.clone();
    let attempts = Cell::new(0);
    // Whether `candidate` fails with a smaller schedule (which becomes the best); false once
    // the attempts run out.
    let mut try_candidate = |candidate: Schedule, best: &mut Schedule| {
        if attempts.get() >= max_attempts {
            return false;
        }
        attempts.set(attempts.get() + 1);
        match fails(&candidate) {
            Some(followed) if followed.choices.len() < best.choices.len() => {
                *best = followed;
                true
            }
            _ => false,
        }
    };
    if best.choices.is_empty() || try_candidate(Schedule::new(), &mut best) {
        return best;
    }
    let mut parts = 2;
    loop {
        let len = best.choices.len();
        if len == 0 {
            break;
        }
        let chunk = len.div_ceil(parts.min(len));
        let ranges: Vec<_> = (0..len)
            .step_by(chunk)
            .map(|start| start..(start + chunk).min(len))
            .collect();
        let alone = ranges.len() > 1
            && ranges.iter().any(|range| {
                try_candidate(
                    Schedule {
                        choices: best.choices[range.clone()].to_vec(),
                    },
                    &mut best,
                )
            });
        if alone {
            parts = 2;
            continue;
        }
        let without = ranges.iter().any(|range| {
            let mut choices = best.choices.clone();
            choices.drain(range.clone());
            try_candidate(Schedule { choices }, &mut best)
        });
        if without {
            parts = (parts - 1).max(2);
            continue;
        }
        if parts >= len || attempts.get() >= max_attempts {
            break;
        }
        parts = (parts * 2).min(len);
    }
    best
}

/// Run `f` as the main thread of a new simulation run seeded with `seed`, then end the run.
///
/// The run gets a fresh [`Context`], so it shares nothing with the caller. Threads `f` spawns
/// with [`thread::spawn`] are scheduled deterministically; their order depends only on `seed`
/// (and the `SIMULATOR_SCHEDULE_STRATEGY` environment).
pub fn run<R>(seed: u64, f: impl FnOnce() -> R) -> Outcome<R> {
    run_in(Context::new(seed), f)
}

fn run_in<R>(context: Arc<Context>, f: impl FnOnce() -> R) -> Outcome<R> {
    switchy_simulation::with_context(context, || {
        let scheduler = scheduler();
        let result = std::panic::catch_unwind(AssertUnwindSafe(f));
        let report = scheduler.shutdown();
        Outcome { result, report }
    })
}

/// A unique key for a synchronization object's waits, such as [`WaitKey::Io`].
#[must_use]
pub fn new_key() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The deadline `timeout` from now on the run's clock, or `None` if it overflows.
#[must_use]
pub fn deadline_after(timeout: Duration) -> Option<Instant> {
    switchy_time::simulator::instant_now().checked_add(timeout)
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn swarm_draws_differ_by_seed_and_choice_and_stay_in_bounds() {
        let draws: BTreeSet<u64> = (0..64).map(|seed| draw(seed, 2, 6)).collect();
        assert_eq!(draws, (0..6).collect(), "every depth is drawn");
        assert!((0..64).any(|seed| draw(seed, 1, 3) != draw(seed, 4, 3)));
        assert_eq!(draw(7, 1, 3), draw(7, 1, 3));
    }

    #[test]
    fn a_configuration_describes_itself() {
        assert_eq!(Config::random().describe(), "random");
        let mut config = Config::pct(3, 10_000);
        assert_eq!(config.describe(), "pct(3/10000)");
        config.weak_memory = true;
        assert_eq!(config.describe(), "pct(3/10000)+weak");
    }
}
