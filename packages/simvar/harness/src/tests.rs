//! Host lifecycle and fault injection, through whole simulation runs.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use switchy::{
    network::{Endpoint, LinkPolicy},
    tcp::{GenericTcpListener as _, TcpListener, TcpStream},
    unsync::{
        io::{AsyncReadExt as _, AsyncWriteExt as _},
        task, time,
    },
};

use crate::{
    Replay, RunSpec, Sim, SimBootstrap, SimConfig, SimResult, Simulation,
    client::ClientResult,
    fault::{self, HostStatus},
    host::{self, HostResult},
};

/// Run `bootstrap` once with `seed`, returning its result and trace.
fn run(bootstrap: &impl SimBootstrap, seed: u64) -> (SimResult, u64) {
    let outcome =
        Simulation::new(bootstrap).run_once(1, None, Replay::Seed(seed), &RunSpec::fleet(1));
    // Leave no context behind for the next test on this thread.
    switchy::simulation::set_current(None);
    (outcome.result, outcome.trace)
}

fn assert_success(result: &SimResult) {
    assert!(result.is_success(), "{result}");
}

fn boxed(e: impl std::error::Error + Send + 'static) -> Box<dyn std::error::Error + Send> {
    Box::new(e)
}

fn ensure(condition: bool, message: &str) -> ClientResult {
    if condition {
        Ok(())
    } else {
        Err(boxed(std::io::Error::other(message.to_owned())))
    }
}

/// The server of the TCP lifecycle test: every incarnation counts its start, runs a ticker
/// task, and echoes TCP on port 80.
fn tcp_server(
    starts: Arc<AtomicUsize>,
    ticks: Arc<Mutex<Vec<u64>>>,
) -> impl Fn() -> std::pin::Pin<Box<dyn Future<Output = HostResult>>> {
    move || {
        let starts = Arc::clone(&starts);
        let ticks = Arc::clone(&ticks);
        Box::pin(async move {
            let incarnation = starts.fetch_add(1, Ordering::SeqCst);
            ticks.lock().unwrap().push(0);
            task::spawn_local(async move {
                loop {
                    time::sleep(Duration::from_millis(10)).await;
                    ticks.lock().unwrap()[incarnation] += 1;
                }
            });
            let listener = TcpListener::bind("0.0.0.0:80").await.map_err(boxed)?;
            loop {
                let (mut stream, _) = listener.accept().await.map_err(boxed)?;
                task::spawn_local(async move {
                    let mut buf = [0_u8; 4];
                    while stream.read_exact(&mut buf).await.is_ok() {
                        if stream.write_all(&buf).await.is_err() {
                            break;
                        }
                    }
                });
            }
        })
    }
}

struct BounceBootstrap {
    starts: Arc<AtomicUsize>,
    ticks: Arc<Mutex<Vec<u64>>>,
}

impl SimBootstrap for BounceBootstrap {
    fn on_start(&self, sim: &mut impl Sim) {
        sim.host(
            "server",
            tcp_server(Arc::clone(&self.starts), Arc::clone(&self.ticks)),
        );
        let starts = Arc::clone(&self.starts);
        let ticks = Arc::clone(&self.ticks);
        sim.client("nemesis", async move {
            time::sleep(Duration::from_millis(200)).await;
            let mut stream = TcpStream::connect("server:80").await.map_err(boxed)?;
            stream.write_all(b"ping").await.map_err(boxed)?;
            let mut buf = [0_u8; 4];
            stream.read_exact(&mut buf).await.map_err(boxed)?;
            ensure(&buf == b"ping", "no echo")?;

            fault::bounce("server");
            time::sleep(Duration::from_millis(200)).await;

            ensure(
                starts.load(Ordering::SeqCst) == 2,
                "the factory did not run again",
            )?;
            ensure(
                fault::host_status("server") == Some(HostStatus::Running),
                "the server is not running",
            )?;
            // The crash reset the connection to the first incarnation.
            let error = stream.read_exact(&mut buf).await.err();
            ensure(
                error.is_some_and(|e| e.kind() == std::io::ErrorKind::ConnectionReset),
                "the connection survived the crash",
            )?;
            // The first incarnation's tasks never ran again.
            let frozen = ticks.lock().unwrap()[0];
            time::sleep(Duration::from_millis(200)).await;
            ensure(ticks.lock().unwrap()[0] == frozen, "a crashed task ran")?;
            ensure(
                ticks.lock().unwrap()[1] > 0,
                "the new incarnation does not run",
            )?;

            // The restarted server bound its port again.
            let mut stream = TcpStream::connect("server:80").await.map_err(boxed)?;
            stream.write_all(b"back").await.map_err(boxed)?;
            stream.read_exact(&mut buf).await.map_err(boxed)?;
            ensure(&buf == b"back", "no echo after the restart")?;

            simvar_utils::cancel_simulation();
            Ok(())
        });
    }
}

#[test_log::test]
fn bounce_restarts_a_host_from_its_factory_on_a_fresh_runtime() {
    let bootstrap = BounceBootstrap {
        starts: Arc::new(AtomicUsize::new(0)),
        ticks: Arc::new(Mutex::new(vec![])),
    };
    let (result, _) = run(&bootstrap, 1);
    assert_success(&result);
    assert_eq!(bootstrap.starts.load(Ordering::SeqCst), 2);
}

struct HooksBootstrap {
    calls: Arc<Mutex<Vec<String>>>,
}

impl SimBootstrap for HooksBootstrap {
    fn on_start(&self, sim: &mut impl Sim) {
        let calls = Arc::clone(&self.calls);
        let starts = Arc::new(AtomicUsize::new(0));
        sim.host("db", move || {
            let calls = Arc::clone(&calls);
            let first = starts.fetch_add(1, Ordering::SeqCst) == 0;
            async move {
                if first {
                    host::on_crash(move |host| {
                        calls
                            .lock()
                            .unwrap()
                            .push(format!("first incarnation of {host}"));
                    });
                }
                loop {
                    time::sleep(Duration::from_millis(10)).await;
                }
            }
        });
        let calls = Arc::clone(&self.calls);
        sim.on_crash("db", move |host| {
            calls.lock().unwrap().push(format!("every crash of {host}"));
        });
        let calls = Arc::clone(&self.calls);
        sim.client("nemesis", async move {
            time::sleep(Duration::from_millis(50)).await;
            fault::crash("db");
            time::sleep(Duration::from_millis(50)).await;
            ensure(
                fault::host_status("db") == Some(HostStatus::Down),
                "the crashed host is not down",
            )?;
            ensure(!switchy::network::is_up("db"), "its network node is up")?;
            ensure(calls.lock().unwrap().len() == 2, "the hooks did not run")?;

            fault::restart("db");
            time::sleep(Duration::from_millis(50)).await;
            ensure(
                fault::host_status("db") == Some(HostStatus::Running),
                "the restarted host is not running",
            )?;
            ensure(switchy::network::is_up("db"), "its network node is down")?;

            fault::bounce_after("db", Duration::from_millis(500));
            time::sleep(Duration::from_millis(250)).await;
            ensure(
                fault::host_status("db") == Some(HostStatus::Down),
                "the host restarted before its delay",
            )?;
            time::sleep(Duration::from_millis(500)).await;
            ensure(
                fault::host_status("db") == Some(HostStatus::Running),
                "the host did not restart after its delay",
            )?;
            simvar_utils::cancel_simulation();
            Ok(())
        });
    }
}

#[test_log::test]
fn crash_hooks_run_and_crashed_hosts_stay_down_until_restarted() {
    let bootstrap = HooksBootstrap {
        calls: Arc::new(Mutex::new(vec![])),
    };
    let (result, _) = run(&bootstrap, 2);
    assert_success(&result);
    assert_eq!(
        *bootstrap.calls.lock().unwrap(),
        [
            "every crash of db",
            "first incarnation of db",
            "every crash of db",
        ]
    );
}

/// Three nodes exchanging messages over a lossy, reordering network, with a nemesis making
/// random partitions and bounces. Every delivery is recorded in the run's trace.
struct ChaosBootstrap {
    deliveries: Arc<AtomicU64>,
}

impl SimBootstrap for ChaosBootstrap {
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration = Duration::from_secs(5);
        config
    }

    fn on_start(&self, sim: &mut impl Sim) {
        sim.set_default_link_policy(
            LinkPolicy::reliable()
                .with_latency(Duration::from_millis(1), Duration::from_millis(30))
                .with_loss(0.1)
                .with_duplicate(0.05)
                .with_reorder(true),
        );
        let names = ["n1", "n2", "n3"];
        for (i, name) in names.into_iter().enumerate() {
            let deliveries = Arc::clone(&self.deliveries);
            sim.host(name, move || {
                let deliveries = Arc::clone(&deliveries);
                async move {
                    let endpoint = Endpoint::bind_current().map_err(boxed)?;
                    let next = names[(i + 1) % names.len()];
                    let mut sequence = 0_u8;
                    loop {
                        let _ = endpoint.send(next, vec![u8::try_from(i).unwrap(), sequence]);
                        sequence = sequence.wrapping_add(1);
                        if let Ok(message) = endpoint.recv_timeout(Duration::from_millis(20)).await
                        {
                            deliveries.fetch_add(1, Ordering::SeqCst);
                            crate::record(
                                u64::from(message.payload[0]) << 8 | u64::from(message.payload[1]),
                            );
                        }
                    }
                }
            });
        }
        sim.client("nemesis", async move {
            loop {
                time::sleep(Duration::from_millis(300)).await;
                let victim = names[switchy::random::rng().gen_range(0..names.len())];
                match switchy::random::rng().gen_range(0..3) {
                    0 => fault::network::isolate(victim),
                    1 => fault::bounce_after(victim, Duration::from_millis(100)),
                    _ => fault::network::heal(),
                }
            }
        });
    }
}

#[test_log::test]
fn the_same_seed_replays_the_same_faults_and_deliveries() {
    let traces: Vec<_> = [7, 7, 8]
        .into_iter()
        .map(|seed| {
            let bootstrap = ChaosBootstrap {
                deliveries: Arc::new(AtomicU64::new(0)),
            };
            let (result, trace) = run(&bootstrap, seed);
            assert_success(&result);
            assert!(bootstrap.deliveries.load(Ordering::SeqCst) > 0);
            trace
        })
        .collect();
    assert_eq!(traces[0], traces[1]);
    assert_ne!(traces[0], traces[2]);
}

/// Counts its drops.
struct DropFlag(Arc<AtomicUsize>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// What the crash test observed.
#[derive(Default)]
struct CrashLog {
    /// Each incarnation's progress: its spawned tasks' (and threads') loop iterations.
    ticks: Vec<u64>,
    /// Drops of the first incarnation's spawned tasks and threads.
    dropped: usize,
    /// `dropped` when the crash hook ran.
    dropped_at_hook: Option<usize>,
}

type SharedLog = Arc<Mutex<CrashLog>>;

/// How many tasks (and threads) the first incarnation of the crash test's worker spawns.
const SPAWNED: usize = if cfg!(feature = "thread") { 4 } else { 3 };

/// Count a tick of incarnation `incarnation`.
fn tick(log: &SharedLog, incarnation: usize) {
    log.lock().unwrap().ticks[incarnation] += 1;
}

/// A worker whose every incarnation spawns a local task, two `JoinSet` tasks and (with
/// `thread`) a blocking closure, all counting ticks forever and holding drop flags.
fn crash_worker(
    log: SharedLog,
    drops: Arc<AtomicUsize>,
) -> impl Fn() -> std::pin::Pin<Box<dyn Future<Output = HostResult>>> {
    move || {
        let log = Arc::clone(&log);
        let drops = Arc::clone(&drops);
        Box::pin(async move {
            let incarnation = {
                let mut log = log.lock().unwrap();
                log.ticks.push(0);
                log.ticks.len() - 1
            };
            let flag = || DropFlag(Arc::clone(&drops));
            let local_log = Arc::clone(&log);
            let local_flag = std::rc::Rc::new(flag());
            task::spawn_local(async move {
                let _flag = local_flag;
                loop {
                    time::sleep(Duration::from_millis(5)).await;
                    tick(&local_log, incarnation);
                }
            });
            let mut set = task::JoinSet::new();
            for _ in 0..2 {
                let log = Arc::clone(&log);
                let flag = flag();
                set.spawn(async move {
                    let _flag = flag;
                    loop {
                        time::sleep(Duration::from_millis(5)).await;
                        tick(&log, incarnation);
                    }
                });
            }
            // A blocking closure on a simulated thread: forever in the first incarnation (the
            // crash kills it), once after (so the run ends with no thread left).
            #[cfg(feature = "thread")]
            {
                let log = Arc::clone(&log);
                let flag = flag();
                drop(task::spawn_blocking(move || {
                    let _flag = flag;
                    if incarnation == 0 {
                        loop {
                            switchy::thread::thread::sleep(Duration::from_millis(5));
                            tick(&log, incarnation);
                        }
                    }
                    tick(&log, incarnation);
                }));
            }
            loop {
                time::sleep(Duration::from_millis(5)).await;
                let _ = &set;
            }
        })
    }
}

struct CrashBootstrap {
    log: SharedLog,
}

impl SimBootstrap for CrashBootstrap {
    fn on_start(&self, sim: &mut impl Sim) {
        let drops = Arc::new(AtomicUsize::new(0));
        sim.host(
            "worker",
            crash_worker(Arc::clone(&self.log), Arc::clone(&drops)),
        );
        let log = Arc::clone(&self.log);
        let hook_drops = Arc::clone(&drops);
        sim.on_crash("worker", move |_| {
            log.lock().unwrap().dropped_at_hook = Some(hook_drops.load(Ordering::SeqCst));
        });
        let log = Arc::clone(&self.log);
        sim.client("nemesis", async move {
            // Every sleep takes at least one step, however coarse the run's steps are.
            let wait_for = |what: &'static str, done: Box<dyn Fn(&CrashLog) -> bool>| {
                let log = Arc::clone(&log);
                async move {
                    for _ in 0..10_000 {
                        if done(&log.lock().unwrap()) {
                            return Ok(());
                        }
                        time::sleep(Duration::from_millis(1)).await;
                    }
                    ensure(false, what)
                }
            };
            wait_for(
                "the worker never ran",
                Box::new(|log| {
                    log.ticks
                        .first()
                        .is_some_and(|x| *x >= u64::try_from(SPAWNED).unwrap() * 2)
                }),
            )
            .await?;
            fault::crash("worker");
            time::sleep(Duration::from_millis(1)).await;
            let frozen = log.lock().unwrap().ticks[0];
            for _ in 0..100 {
                time::sleep(Duration::from_millis(1)).await;
            }
            ensure(
                log.lock().unwrap().ticks[0] == frozen,
                "a crashed task or thread ran",
            )?;
            log.lock().unwrap().dropped = drops.load(Ordering::SeqCst);

            fault::restart("worker");
            wait_for(
                "the restarted worker does not run",
                Box::new(|log| log.ticks.len() == 2 && log.ticks[1] > 0),
            )
            .await?;
            let log = log.lock().unwrap();
            crate::record(log.ticks[0]);
            crate::record(log.ticks[1]);
            drop(log);
            simvar_utils::cancel_simulation();
            Ok(())
        });
    }
}

fn crash_run(seed: u64) -> (CrashLog, u64) {
    let bootstrap = CrashBootstrap {
        log: SharedLog::default(),
    };
    let (result, trace) = run(&bootstrap, seed);
    assert_success(&result);
    let log = std::mem::take(&mut *bootstrap.log.lock().unwrap());
    (log, trace)
}

#[test_log::test]
fn a_crash_drops_every_spawned_task_and_thread_before_the_hooks_run() {
    for seed in [3, 4, 5] {
        let (log, trace) = crash_run(seed);
        assert_eq!(log.dropped, SPAWNED, "seed {seed}");
        assert_eq!(log.dropped_at_hook, Some(SPAWNED), "seed {seed}");
        // The same seed replays the same crash.
        let (again, replayed) = crash_run(seed);
        assert_eq!(trace, replayed, "seed {seed}");
        assert_eq!(log.ticks, again.ticks, "seed {seed}");
    }
}

/// Two blocking closures contending for a lock, on simulated threads; with `covering`, the run
/// records its interleaving coverage, which [`SimBootstrap::on_result`] keeps.
#[cfg(feature = "thread")]
struct CoverageBootstrap {
    covering: bool,
    coverage: Mutex<Vec<Option<switchy::thread::simulator::Coverage>>>,
}

#[cfg(feature = "thread")]
impl SimBootstrap for CoverageBootstrap {
    fn init(&self) {
        if self.covering {
            let seed = switchy::random::simulator::seed();
            switchy::thread::simulator::configure(switchy::thread::simulator::Config {
                interleaving_coverage: true,
                ..switchy::thread::simulator::Config::from_env(seed)
            });
        }
    }

    fn on_start(&self, sim: &mut impl Sim) {
        sim.client("contender", async move {
            let lock = Arc::new(switchy::thread::sync::Mutex::new(0));
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    let lock = Arc::clone(&lock);
                    task::spawn_blocking(move || *lock.lock().unwrap() += 1)
                })
                .collect();
            for handle in handles {
                handle.await.map_err(boxed)?;
            }
            simvar_utils::cancel_simulation();
            Ok(())
        });
    }

    fn on_result(&self, result: &SimResult) {
        self.coverage
            .lock()
            .unwrap()
            .push(result.run().coverage.clone());
    }
}

#[cfg(feature = "thread")]
#[test_log::test]
fn a_bootstrap_reads_each_runs_interleaving_coverage_from_its_result() {
    for seed in [1, 2] {
        let bootstrap = CoverageBootstrap {
            covering: true,
            coverage: Mutex::new(Vec::new()),
        };
        let (first, _) = run(&bootstrap, seed);
        assert_success(&first);
        assert!(first.run().coverage.as_ref().is_some_and(|c| !c.is_empty()));
        let (second, _) = run(&bootstrap, seed);
        assert_eq!(first.run().coverage, second.run().coverage, "seed {seed}");
        let seen = bootstrap.coverage.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], first.run().coverage);
        assert_eq!(seen[1], second.run().coverage);
    }

    let bootstrap = CoverageBootstrap {
        covering: false,
        coverage: Mutex::new(Vec::new()),
    };
    let (result, _) = run(&bootstrap, 1);
    assert_success(&result);
    assert!(result.run().coverage.is_none());
    assert_eq!(*bootstrap.coverage.lock().unwrap(), vec![None]);
}

/// Reconfigures the run's clock as it starts, as simulations that pick a tick per run do.
struct RetimedBootstrap;

impl SimBootstrap for RetimedBootstrap {
    fn on_start(&self, sim: &mut impl Sim) {
        switchy::time::simulator::set_step_multiplier(7);
        sim.client("sleeper", async move {
            time::sleep(Duration::from_millis(70)).await;
            simvar_utils::cancel_simulation();
            Ok(())
        });
    }
}

#[test_log::test]
fn a_run_whose_clock_is_reconfigured_as_it_starts_still_reports_its_times() {
    let (result, _) = run(&RetimedBootstrap, 1);
    assert_success(&result);
    assert!(result.run().steps > 0, "{result}");
    assert!(result.run().sim_time_millis > 0, "{result}");
}

/// Observes how many runs this process made before it: a run that depends on its process's
/// history, as a cache or counter in a static would make it.
struct HistoryBootstrap;

static RUNS_BEFORE: AtomicU64 = AtomicU64::new(0);

impl SimBootstrap for HistoryBootstrap {
    fn on_start(&self, sim: &mut impl Sim) {
        let before = RUNS_BEFORE.fetch_add(1, Ordering::SeqCst);
        sim.client("historian", async move {
            switchy::simulation::observe("test", "runs before", &before.to_le_bytes());
            simvar_utils::cancel_simulation();
            Ok(())
        });
    }
}

fn outcome(bootstrap: &impl SimBootstrap, seed: u64) -> crate::RunOutcome {
    let outcome =
        Simulation::new(bootstrap).run_once(1, None, Replay::Seed(seed), &RunSpec::fleet(1));
    switchy::simulation::set_current(None);
    outcome
}

fn replayed(outcome: crate::RunOutcome) -> crate::Replayed {
    crate::Replayed {
        trace: outcome.trace,
        digest: outcome.digest,
        #[cfg(feature = "thread")]
        schedule: Some(outcome.schedule),
    }
}

#[test_log::test]
fn a_run_that_depends_on_its_process_history_diverges_at_that_observation() {
    let first = outcome(&HistoryBootstrap, 1);
    assert_success(&first.result);
    let again = replayed(outcome(&HistoryBootstrap, 1));
    let error = crate::divergence(&first, &again).expect("the runs observed differently");
    assert!(error.contains("non-deterministic simulation"), "{error}");
    assert!(
        error.contains("test: 1 observations / 1 (same count"),
        "{error}"
    );
    assert!(error.contains("[test] runs before"), "{error}");
}

#[test_log::test]
fn the_same_run_replayed_observes_the_same() {
    let first = outcome(&RetimedBootstrap, 3);
    assert_success(&first.result);
    assert!(first.digest.sources.contains_key("time"));
    assert!(first.digest.sources.contains_key("random"));
    let again = replayed(outcome(&RetimedBootstrap, 3));
    assert_eq!(crate::divergence(&first, &again), None);
}
