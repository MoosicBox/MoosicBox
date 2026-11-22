//! Classic models checked by every strategy.

use std::{borrow::Cow, num::NonZeroUsize};

use simvar_explore::{Checker, Model, Outcome, Properties, Report, SeedSource, Strategy, replay};

// ---------------------------------------------------------------------------------------
// Mutual exclusion
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pc {
    Idle,
    Checked,
    Critical,
}

/// Each process checks the other's flag, *then* raises its own: both can enter.
struct NaiveMutex;

impl Model for NaiveMutex {
    type State = [(Pc, bool); 2];
    type Action = usize;

    fn init_states(&self) -> Vec<Self::State> {
        vec![[(Pc::Idle, false); 2]]
    }

    fn actions(&self, _state: &Self::State, actions: &mut Vec<usize>) {
        actions.extend([0, 1]);
    }

    fn next_state(&self, state: &Self::State, &me: &usize) -> Option<Self::State> {
        let mut next = *state;
        next[me] = match state[me] {
            (Pc::Idle, _) if !state[1 - me].1 => (Pc::Checked, false),
            (Pc::Checked, _) => (Pc::Critical, true),
            (Pc::Critical, _) => (Pc::Idle, false),
            (Pc::Idle, _) => return None,
        };
        Some(next)
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties.always("mutual exclusion", |_, s| {
            !(s[0].0 == Pc::Critical && s[1].0 == Pc::Critical)
        });
        properties.sometimes("process 0 enters", |_, s| s[0].0 == Pc::Critical);
    }
}

/// Peterson's algorithm: correct.
struct Peterson;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PetersonState {
    /// 0 idle, 1 flag raised, 2 turn given away (waiting), 3 critical.
    pc: [u8; 2],
    flag: [bool; 2],
    turn: usize,
}

impl Model for Peterson {
    type State = PetersonState;
    type Action = usize;

    fn init_states(&self) -> Vec<PetersonState> {
        vec![PetersonState {
            pc: [0; 2],
            flag: [false; 2],
            turn: 0,
        }]
    }

    fn actions(&self, _state: &PetersonState, actions: &mut Vec<usize>) {
        actions.extend([0, 1]);
    }

    fn next_state(&self, state: &PetersonState, &me: &usize) -> Option<PetersonState> {
        let other = 1 - me;
        let mut next = state.clone();
        match state.pc[me] {
            0 => {
                next.flag[me] = true;
                next.pc[me] = 1;
            }
            1 => {
                next.turn = other;
                next.pc[me] = 2;
            }
            2 if !state.flag[other] || state.turn == me => next.pc[me] = 3,
            2 => return None,
            _ => {
                next.flag[me] = false;
                next.pc[me] = 0;
            }
        }
        Some(next)
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties.always("mutual exclusion", |_, s| s.pc != [3, 3]);
        properties.sometimes("process 1 enters", |_, s| s.pc[1] == 3);
    }
}

fn violation<'r, S, A>(report: &'r Report<S, A>, name: &str) -> &'r simvar_explore::Trace<S, A> {
    match &report.property(name).expect("property exists").outcome {
        Outcome::Violated(trace) => trace,
        _ => panic!("expected a counterexample for {name}"),
    }
}

#[test]
fn bfs_finds_the_shortest_mutex_violation() {
    let report = Checker::new().bfs(&NaiveMutex);
    assert!(!report.is_success());
    assert!(!report.complete, "stopped at the failure");
    let trace = violation(&report, "mutual exclusion");
    assert_eq!(trace.len(), 4, "both check, then both enter:\n{report}");
    assert_eq!(trace.last()[0].0, Pc::Critical);
    assert_eq!(trace.last()[1].0, Pc::Critical);
    trace.verify(&NaiveMutex).unwrap();
    assert_eq!(
        replay(&NaiveMutex, trace.init(), &trace.actions).unwrap(),
        trace.states
    );
    assert!(
        report
            .to_string()
            .contains("FAIL always \"mutual exclusion\"")
    );
}

#[test]
fn dfs_finds_a_mutex_violation() {
    let report = Checker::new().dfs(&NaiveMutex);
    let trace = violation(&report, "mutual exclusion");
    assert!(trace.len() >= 4);
    trace.verify(&NaiveMutex).unwrap();
}

#[test]
fn peterson_passes_exhaustively() {
    for report in [
        Checker::new().bfs(&Peterson),
        Checker::new().dfs(&Peterson),
        Checker::new()
            .threads(NonZeroUsize::new(4).unwrap())
            .bfs_parallel(&Peterson),
    ] {
        assert!(report.complete, "{report}");
        assert!(report.is_success(), "{report}");
        let outcome = &report.property("mutual exclusion").unwrap().outcome;
        assert!(matches!(outcome, Outcome::Holds));
        assert!(matches!(
            report.property("process 1 enters").unwrap().outcome,
            Outcome::Found(_)
        ));
    }
}

#[test]
fn keeps_going_past_failures_when_asked() {
    let report = Checker::new().stop_on_failure(false).bfs(&NaiveMutex);
    assert!(matches!(
        report.property("process 0 enters").unwrap().outcome,
        Outcome::Found(ref trace) if trace.len() == 2
    ));
    assert!(matches!(
        report.property("mutual exclusion").unwrap().outcome,
        Outcome::Violated(_)
    ));
}

#[test]
fn parallel_bfs_matches_sequential_bfs() {
    for threads in [1, 2, 3, 8] {
        let checker = Checker::new()
            .stop_on_failure(false)
            .threads(NonZeroUsize::new(threads).unwrap());
        let parallel = checker.bfs_parallel(&NaiveMutex);
        let sequential = checker.bfs(&NaiveMutex);
        assert_eq!(parallel.stats.states, sequential.stats.states);
        assert_eq!(parallel.stats.unique, sequential.stats.unique);
        assert_eq!(parallel.stats.max_depth, sequential.stats.max_depth);
        assert_eq!(
            violation(&parallel, "mutual exclusion"),
            violation(&sequential, "mutual exclusion")
        );
        assert_eq!(parallel.strategy, Strategy::Bfs { threads });
    }
}

// ---------------------------------------------------------------------------------------
// Bounded counter: reachability, boundaries, depth and state limits
// ---------------------------------------------------------------------------------------

/// Counts up by one or two, within 0..=10.
struct Counter;

impl Model for Counter {
    type State = u32;
    type Action = u32;

    fn init_states(&self) -> Vec<u32> {
        vec![0]
    }

    fn actions(&self, _state: &u32, actions: &mut Vec<u32>) {
        actions.extend([1, 2]);
    }

    fn next_state(&self, state: &u32, step: &u32) -> Option<u32> {
        Some(state + step)
    }

    fn within_boundary(&self, state: &u32) -> bool {
        *state <= 10
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties.always("at most 10", |_, s| *s <= 10);
        properties.sometimes("reaches 7", |_, s| *s == 7);
    }
}

#[test]
fn sometimes_finds_the_shortest_example() {
    let report = Checker::new().bfs(&Counter);
    assert!(report.is_success(), "{report}");
    let Outcome::Found(trace) = &report.property("reaches 7").unwrap().outcome else {
        panic!("{report}");
    };
    assert_eq!(trace.len(), 4, "2 + 2 + 2 + 1");
    assert_eq!(*trace.last(), 7);
}

#[test]
fn exhausting_the_space_proves_unreachability() {
    struct Unreachable;
    impl Model for Unreachable {
        type State = u32;
        type Action = u32;
        fn init_states(&self) -> Vec<u32> {
            Counter.init_states()
        }
        fn actions(&self, state: &u32, actions: &mut Vec<u32>) {
            Counter.actions(state, actions);
        }
        fn next_state(&self, state: &u32, step: &u32) -> Option<u32> {
            Counter.next_state(state, step)
        }
        fn within_boundary(&self, state: &u32) -> bool {
            Counter.within_boundary(state)
        }
        fn properties(&self, properties: &mut Properties<Self>) {
            properties.sometimes("reaches 11", |_, s| *s == 11);
        }
    }
    let report = Checker::new().bfs(&Unreachable);
    assert!(report.complete);
    assert_eq!(report.stats.unique, 11);
    assert!(matches!(
        report.property("reaches 11").unwrap().outcome,
        Outcome::Unreachable
    ));
    assert!(!report.is_success());
}

#[test]
fn depth_limits_make_a_check_incomplete() {
    for report in [
        Checker::new().max_depth(2).bfs(&Counter),
        Checker::new().max_depth(2).dfs(&Counter),
    ] {
        assert!(!report.complete);
        assert_eq!(report.stats.max_depth, 2);
        assert_eq!(report.stats.unique, 5, "0..=4: {report}");
        assert!(matches!(
            report.property("at most 10").unwrap().outcome,
            Outcome::Unrefuted
        ));
        assert!(matches!(
            report.property("reaches 7").unwrap().outcome,
            Outcome::NotFound
        ));
    }
}

#[test]
fn a_depth_limit_that_covers_the_space_is_complete() {
    let report = Checker::new().max_depth(10).bfs(&Counter);
    assert!(report.complete, "{report}");
}

#[test]
fn state_limits_make_a_check_incomplete() {
    let report = Checker::new()
        .stop_on_failure(false)
        .max_states(3)
        .bfs(&Counter);
    assert!(!report.complete);
    assert_eq!(report.stats.unique, 3);
}

/// 0 -a-> 1 -a-> 2 -a-> 3 -a-> 4, and a shortcut 0 -b-> 10 -a-> 3.
struct Shortcut;

impl Model for Shortcut {
    type State = u8;
    type Action = char;

    fn init_states(&self) -> Vec<u8> {
        vec![0]
    }

    fn actions(&self, _state: &u8, actions: &mut Vec<char>) {
        actions.extend(['a', 'b']);
    }

    fn next_state(&self, state: &u8, action: &char) -> Option<u8> {
        match (state, action) {
            (0..=3, 'a') => Some(state + 1),
            (0, 'b') => Some(10),
            (10, 'a') => Some(3),
            _ => None,
        }
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties.sometimes("reaches 4", |_, s| *s == 4);
    }
}

#[test]
fn bounded_dfs_revisits_states_reached_by_shorter_paths() {
    // Depth-first reaches 3 first at depth 3, where the bound stops it, then through the
    // shortcut at depth 2, from where 4 is within the bound.
    let report = Checker::new().max_depth(3).dfs(&Shortcut);
    let Outcome::Found(trace) = &report.property("reaches 4").unwrap().outcome else {
        panic!("{report}");
    };
    assert_eq!(trace.actions, ['b', 'a', 'a']);
}

// ---------------------------------------------------------------------------------------
// Symmetry reduction
// ---------------------------------------------------------------------------------------

/// `n` interchangeable switches, any of which can be toggled.
struct Switches {
    n: usize,
    symmetric: bool,
}

impl Model for Switches {
    type State = Vec<bool>;
    type Action = usize;

    fn init_states(&self) -> Vec<Vec<bool>> {
        vec![vec![false; self.n]]
    }

    fn actions(&self, _state: &Vec<bool>, actions: &mut Vec<usize>) {
        actions.extend(0..self.n);
    }

    fn next_state(&self, state: &Vec<bool>, &i: &usize) -> Option<Vec<bool>> {
        let mut next = state.clone();
        next[i] = !next[i];
        Some(next)
    }

    fn canonicalize<'s>(&self, state: &'s Vec<bool>) -> Cow<'s, Vec<bool>> {
        if self.symmetric {
            let mut sorted = state.clone();
            sorted.sort_unstable();
            Cow::Owned(sorted)
        } else {
            Cow::Borrowed(state)
        }
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties.always("not all on", |_, s| !s.iter().all(|&on| on));
        properties.sometimes("half on", |m, s| {
            s.iter().filter(|&&on| on).count() == m.n / 2
        });
    }
}

#[test]
fn symmetry_reduction_shrinks_the_state_space() {
    let full = Checker::new().stop_on_failure(false);
    let plain = full.bfs(&Switches {
        n: 6,
        symmetric: false,
    });
    let reduced = full.bfs(&Switches {
        n: 6,
        symmetric: true,
    });
    assert_eq!(plain.stats.unique, 64);
    assert_eq!(
        reduced.stats.unique, 7,
        "one state per number of switches on"
    );
    // The counterexample is equally short and is a real path, not a canonical one.
    let plain_trace = violation(&plain, "not all on");
    let reduced_trace = violation(&reduced, "not all on");
    assert_eq!(plain_trace.len(), 6);
    assert_eq!(reduced_trace.len(), 6);
    reduced_trace
        .verify(&Switches {
            n: 6,
            symmetric: true,
        })
        .unwrap();

    let dfs = full.dfs(&Switches {
        n: 6,
        symmetric: true,
    });
    assert_eq!(dfs.stats.unique, 7);
}

// ---------------------------------------------------------------------------------------
// Bounded liveness
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Job {
    Queued,
    Running,
    Done,
    Abandoned,
}

/// A job that runs to completion, or (when `abandonable`) may be abandoned for good.
struct Jobs {
    abandonable: bool,
}

impl Model for Jobs {
    type State = Job;
    type Action = Job;

    fn init_states(&self) -> Vec<Job> {
        vec![Job::Queued]
    }

    fn actions(&self, _state: &Job, actions: &mut Vec<Job>) {
        actions.extend([Job::Running, Job::Done, Job::Abandoned]);
    }

    fn next_state(&self, state: &Job, to: &Job) -> Option<Job> {
        match (state, to) {
            (Job::Queued, Job::Running) | (Job::Running, Job::Done) => Some(*to),
            (Job::Queued, Job::Abandoned) if self.abandonable => Some(*to),
            _ => None,
        }
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties.eventually("finishes", |_, s| *s == Job::Done);
    }
}

#[test]
fn eventually_finds_paths_that_terminate_without_the_condition() {
    for report in [
        Checker::new().bfs(&Jobs { abandonable: true }),
        Checker::new().dfs(&Jobs { abandonable: true }),
    ] {
        let trace = violation(&report, "finishes");
        assert_eq!(trace.actions, [Job::Abandoned], "{report}");
    }
    let report = Checker::new()
        .seed(1)
        .walks(50)
        .random_walk(&Jobs { abandonable: true });
    assert_eq!(violation(&report, "finishes").actions, [Job::Abandoned]);
}

#[test]
fn eventually_holds_when_every_terminating_path_satisfies_it() {
    let report = Checker::new().bfs(&Jobs { abandonable: false });
    assert!(report.complete);
    assert!(matches!(
        report.property("finishes").unwrap().outcome,
        Outcome::Holds
    ));
}

#[test]
fn eventually_does_not_detect_cycles() {
    // Documented limitation: a loop that never finishes is not a counterexample.
    struct Spinner;
    impl Model for Spinner {
        type State = bool;
        type Action = ();
        fn init_states(&self) -> Vec<bool> {
            vec![false]
        }
        fn actions(&self, _state: &bool, actions: &mut Vec<()>) {
            actions.push(());
        }
        fn next_state(&self, state: &bool, (): &()) -> Option<bool> {
            Some(!state)
        }
        fn properties(&self, properties: &mut Properties<Self>) {
            properties.eventually("never", |_, _| false);
        }
    }
    assert!(Checker::new().bfs(&Spinner).is_success());
}

// ---------------------------------------------------------------------------------------
// Random walks
// ---------------------------------------------------------------------------------------

/// A register that can be incremented, decremented or reset; 7 is a bug.
struct Register;

impl Model for Register {
    type State = i32;
    type Action = &'static str;

    fn init_states(&self) -> Vec<i32> {
        vec![0, 1]
    }

    fn actions(&self, _state: &i32, actions: &mut Vec<&'static str>) {
        actions.extend(["inc", "inc", "dec", "reset"]);
    }

    fn next_state(&self, state: &i32, action: &&'static str) -> Option<i32> {
        Some(match *action {
            "inc" => state + 1,
            "dec" => state - 1,
            _ => 0,
        })
    }

    fn properties(&self, properties: &mut Properties<Self>) {
        properties.always("never 7", |_, s| *s != 7);
    }
}

#[test]
fn random_walks_reproduce_from_their_seed() {
    let checker = Checker::new().seed(42).walks(500).max_depth(40);
    let first = checker.random_walk(&Register);
    let second = checker.random_walk(&Register);
    let trace = violation(&first, "never 7");
    assert_eq!(trace, violation(&second, "never 7"));
    assert_eq!(first.stats.states, second.stats.states);
    assert_eq!(first.stats.walks, second.stats.walks);
    trace.verify(&Register).unwrap();
    assert!(matches!(
        first.strategy,
        Strategy::RandomWalk {
            seeds: SeedSource::Explicit(42),
            ..
        }
    ));

    // The failing walk replays alone from its own seed.
    let walk_seed = trace.walk_seed.expect("walks record their seed");
    let replayed = checker.replay_walk(&Register, walk_seed);
    assert_eq!(replayed.stats.walks, 1);
    assert_eq!(violation(&replayed, "never 7"), trace);
    let shown = first.to_string();
    assert!(
        shown.contains(&format!("SIMULATOR_EXPLORE_WALK_SEED={walk_seed}")),
        "{shown}"
    );
}

#[test]
fn random_walks_follow_the_simulation_runs_seed() {
    let run = |seed| {
        switchy_simulation::with_context(switchy_simulation::Context::new(seed), || {
            Checker::new()
                .walks(500)
                .max_depth(40)
                .random_walk(&Register)
        })
    };
    let first = run(7);
    let second = run(7);
    assert_eq!(
        first.strategy,
        Strategy::RandomWalk {
            walks: first.stats.walks,
            max_depth: 40,
            seeds: SeedSource::Run { run_seed: 7 },
        }
    );
    assert_eq!(violation(&first, "never 7"), violation(&second, "never 7"));
    assert!(first.to_string().contains("SIMULATOR_SEED=7"));
    assert_ne!(
        violation(&first, "never 7").walk_seed,
        violation(&run(8), "never 7").walk_seed
    );
}

#[test]
fn random_walks_never_claim_completeness() {
    let report = Checker::new().seed(3).walks(20).random_walk(&Peterson);
    assert!(!report.complete);
    assert!(report.is_success(), "{report}");
    assert!(matches!(
        report.property("mutual exclusion").unwrap().outcome,
        Outcome::Unrefuted
    ));
}
