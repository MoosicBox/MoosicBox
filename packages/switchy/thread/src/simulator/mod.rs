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
}

impl Op {
    const fn code(self) -> u8 {
        self as u8
    }
}

/// How the scheduler picks the next thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

/// The scheduler's configuration for one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// How threads are picked.
    pub strategy: Strategy,
    /// Decisions kept in [`Scheduler::recent_decisions`].
    pub recent_decisions: usize,
}

impl Config {
    /// Uniformly random scheduling.
    #[must_use]
    pub const fn random() -> Self {
        Self {
            strategy: Strategy::Random,
            recent_decisions: 256,
        }
    }

    /// PCT scheduling with `depth` change points drawn from the first `steps` decisions.
    #[must_use]
    pub const fn pct(depth: u32, steps: u64) -> Self {
        Self {
            strategy: Strategy::Pct { depth, steps },
            recent_decisions: 256,
        }
    }

    /// The configuration from the environment for a run with `seed`:
    ///
    /// * `SIMULATOR_SCHEDULE_STRATEGY`: `random`, `pct` or `alternate` (the default), which
    ///   picks `random` for even seeds and `pct` for odd ones.
    /// * `SIMULATOR_PCT_DEPTH` (default 3) and `SIMULATOR_PCT_STEPS` (default 10000).
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
        let strategy = match std::env::var("SIMULATOR_SCHEDULE_STRATEGY").as_deref() {
            Ok("random") => Strategy::Random,
            Ok("pct") => pct,
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
        }
    }
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
}

struct Inner {
    threads: Vec<ThreadRecord>,
    running: ThreadIndex,
    decisions: u64,
    trace_hash: u64,
    config: Config,
    change_points: BTreeSet<u64>,
    lowest_priority: u64,
    recent: std::collections::VecDeque<Decision>,
    failure: Option<String>,
    shutting_down: bool,
    os_handles: Vec<std::thread::JoinHandle<()>>,
}

/// The scheduler of one simulation run. See the [module documentation](self).
pub struct Scheduler {
    context_id: u64,
    rng: Arc<Mutex<switchy_simulation::Rng>>,
    inner: Mutex<Inner>,
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
            .unwrap_or_else(|| Config::from_env(context.seed()));
        let rng = context.stream("switchy_thread");
        let mut change_points = BTreeSet::new();
        let mut main_priority = 0;
        let mut lowest_priority = 0;
        if let Strategy::Pct { depth, steps } = config.strategy {
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
            inner: Mutex::new(Inner {
                threads: vec![ThreadRecord {
                    name: std::thread::current().name().unwrap_or("main").to_owned(),
                    state: ThreadState::Runnable,
                    op: Op::Start,
                    os: None,
                    priority: main_priority,
                    timed_out: false,
                    panicked: false,
                }],
                running: 0,
                decisions: 0,
                trace_hash: 0xcbf2_9ce4_8422_2325,
                config,
                change_points,
                lowest_priority,
                recent: std::collections::VecDeque::new(),
                failure: None,
                shutting_down: false,
                os_handles: Vec::new(),
            }),
        }
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
        let inner = lock(&self.inner);
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
    fn add_thread(&self, name: String) -> ThreadIndex {
        let priority = {
            let mut rng = lock(&self.rng);
            let inner = lock(&self.inner);
            match inner.config.strategy {
                Strategy::Pct { depth, .. } => u64::from(depth) + 1 + rng.below(u64::MAX / 2),
                Strategy::Random => 0,
            }
        };
        let mut inner = lock(&self.inner);
        inner.threads.push(ThreadRecord {
            name,
            state: ThreadState::Runnable,
            op: Op::Start,
            os: None,
            priority,
            timed_out: false,
            panicked: false,
        });
        inner.threads.len() - 1
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
        let chosen = waiting[lock(&self.rng).index(waiting.len())];
        inner.threads[chosen].state = ThreadState::Runnable;
        true
    }

    fn is_shutting_down(&self) -> bool {
        lock(&self.inner).shutting_down
    }

    fn switch(&self, op: Op, block: Option<(WaitKey, Option<Instant>)>) -> bool {
        let me = self.me();
        let mut inner = lock(&self.inner);
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
        std::mem::take(&mut inner.threads[me].timed_out)
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

    fn pick(&self, inner: &mut Inner, from: ThreadIndex, runnable: &[ThreadIndex]) -> ThreadIndex {
        match inner.config.strategy {
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
                if inner.shutting_down && inner.threads[me].state == ThreadState::Finished {
                    return;
                }
            }
            std::thread::park();
        }
    }

    /// Mark the calling simulated thread finished and hand the baton on.
    fn exit(&self, me: ThreadIndex, panicked: bool) {
        let mut inner = lock(&self.inner);
        inner.threads[me].state = ThreadState::Finished;
        inner.threads[me].panicked = panicked;
        inner.threads[me].op = Op::Exit;
        for thread in &mut inner.threads {
            if matches!(thread.state, ThreadState::Blocked { key: WaitKey::Join(j), .. } if j == me)
            {
                thread.state = ThreadState::Runnable;
            }
        }
        self.schedule(&mut inner, me, Op::Exit);
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
            };
            let finished: Vec<std::thread::Thread> = inner
                .threads
                .iter()
                .skip(1)
                .filter(|t| t.state == ThreadState::Finished)
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
                .filter(|t| t.state == ThreadState::Finished)
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
}

/// The current run's scheduler; see [`Scheduler::current`].
#[must_use]
pub fn scheduler() -> Arc<Scheduler> {
    Scheduler::current()
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

/// A unique key for a synchronization object's waits.
pub(crate) fn new_key() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The deadline `timeout` from now on the run's clock, or `None` if it overflows.
pub(crate) fn deadline_after(timeout: Duration) -> Option<Instant> {
    switchy_time::simulator::instant_now().checked_add(timeout)
}
