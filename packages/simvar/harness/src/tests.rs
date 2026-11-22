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
    Replay, Sim, SimBootstrap, SimConfig, SimResult, Simulation,
    client::ClientResult,
    fault::{self, HostStatus},
    host::{self, HostResult},
};

/// Run `bootstrap` once with `seed`, returning its result and trace.
fn run(bootstrap: &impl SimBootstrap, seed: u64) -> (SimResult, u64) {
    let outcome = Simulation::new(bootstrap).run_once(1, None, Replay::Seed(seed));
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
