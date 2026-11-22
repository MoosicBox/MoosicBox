#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! A replicated counter under a nemesis.
//!
//! Three replica hosts each keep a counter. Every few milliseconds a replica may increment it,
//! writes it to its (simulated) disk, syncs the disk now and then, and broadcasts it to the
//! others over the simulated network ([`simvar::switchy::network`]); a replica receiving a
//! larger value adopts it. A restarted replica starts from what its disk had synced: a crash
//! hook throws away unsynced writes, as a power cut would.
//!
//! A nemesis client injects random faults for a while — partitions (symmetric and one-way),
//! isolation, lossy and slow links, and host bounces — then heals everything and checks that
//! the replicas converge on one value, no smaller than any value ever synced.
//!
//! Every fault and delivery is drawn from the run's seed, so a failing seed replays exactly.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use simvar::{
    Sim, SimBootstrap, fault,
    fault::network::{self, Endpoint, LinkPolicy},
    host::HostResult,
    switchy::{random::rng, unsync::time::sleep},
};

/// The replica hosts.
pub const REPLICAS: [&str; 3] = ["r1", "r2", "r3"];

/// One replica's disk: what it wrote, and what of it is synced.
#[derive(Debug, Default, Clone, Copy)]
struct Disk {
    written: Option<u64>,
    synced: u64,
}

/// What the replicas share outside the network: their disks, and their current values (for
/// the nemesis to check).
#[derive(Debug, Default)]
struct World {
    disks: BTreeMap<String, Disk>,
    values: BTreeMap<String, u64>,
    /// The largest value any replica ever synced.
    max_synced: u64,
    /// Set by the nemesis once it is done: replicas stop incrementing, so they can converge.
    quiet: bool,
}

impl World {
    /// What survived on `name`'s disk: a crash hook threw away anything not synced.
    fn restore(&mut self, name: &str) -> u64 {
        let disk = self.disks.entry(name.to_owned()).or_default();
        disk.written.unwrap_or(disk.synced)
    }

    /// One round of `name`: maybe increment `value`, write it, and sync every third round.
    fn advance(&mut self, name: &str, round: u64, mut value: u64) -> u64 {
        if !self.quiet && rng().gen_range(0..10) < 3 {
            value += 1;
        }
        let disk = self.disks.entry(name.to_owned()).or_default();
        disk.written = Some(value);
        if round.is_multiple_of(3) {
            disk.synced = value;
            self.max_synced = self.max_synced.max(value);
        }
        self.values.insert(name.to_owned(), value);
        value
    }
}

type Shared = Arc<Mutex<World>>;

fn boxed(message: String) -> Box<dyn std::error::Error + Send> {
    Box::new(std::io::Error::other(message))
}

/// One incarnation of a replica.
async fn replica(name: String, world: Shared) -> HostResult {
    let mut value = world.lock().unwrap().restore(&name);
    let endpoint = Endpoint::bind(name.clone()).map_err(|e| boxed(e.to_string()))?;
    let peers: Vec<_> = REPLICAS.iter().filter(|x| **x != name).collect();
    let mut round = 0_u64;
    loop {
        round += 1;
        value = world.lock().unwrap().advance(&name, round, value);
        for peer in &peers {
            // A down replica's endpoint fails; it will not be polled again anyway.
            let _ = endpoint.send(peer, value.to_le_bytes().to_vec());
        }
        while let Ok(Some(message)) = endpoint.try_recv() {
            let received = u64::from_le_bytes(message.payload.try_into().unwrap_or_default());
            value = value.max(received);
        }
        sleep(Duration::from_millis(10)).await;
    }
}

/// Inject random faults for a while, heal, and check that the replicas converge.
async fn nemesis(world: Shared) -> Result<(), Box<dyn std::error::Error + Send>> {
    let reliable = network::default_policy();
    for _ in 0..20 {
        sleep(Duration::from_millis(rng().gen_range(50..300))).await;
        let victim = REPLICAS[rng().gen_range(0..REPLICAS.len())];
        let others: Vec<_> = REPLICAS.iter().filter(|x| **x != victim).collect();
        match rng().gen_range(0..6) {
            0 => network::partition([victim], others),
            1 => network::partition_one_way(others, [victim]),
            2 => network::isolate(victim),
            3 => network::set_default_policy(
                LinkPolicy::reliable()
                    .with_latency(Duration::from_millis(5), Duration::from_millis(80))
                    .with_loss(0.3)
                    .with_duplicate(0.1)
                    .with_reorder(true),
            ),
            4 => fault::bounce_after(victim, Duration::from_millis(rng().gen_range(0..500))),
            _ => {
                network::heal();
                network::set_default_policy(reliable);
            }
        }
    }

    network::heal();
    network::set_default_policy(reliable);
    world.lock().unwrap().quiet = true;
    while REPLICAS
        .iter()
        .any(|x| fault::host_status(x) != Some(fault::HostStatus::Running))
    {
        sleep(Duration::from_millis(10)).await;
    }

    // Each sleep takes at least one simulation step, however coarse the run's steps are.
    let mut values = vec![];
    for _ in 0..1000 {
        sleep(Duration::from_millis(10)).await;
        let world = world.lock().unwrap();
        values = REPLICAS.iter().map(|x| world.values[*x]).collect();
        drop(world);
        if values.iter().all(|x| *x == values[0]) {
            break;
        }
    }
    simvar::record(values[0]);
    if values.iter().any(|x| *x != values[0]) {
        return Err(boxed(format!("the replicas did not converge: {values:?}")));
    }
    let world = world.lock().unwrap();
    if values[0] < world.max_synced {
        return Err(boxed(format!(
            "the replicas lost synced writes: {} < {}",
            values[0], world.max_synced
        )));
    }
    drop(world);
    simvar::utils::cancel_simulation();
    Ok(())
}

/// The simulation: three replicas and a nemesis.
pub struct CounterBootstrap;

impl SimBootstrap for CounterBootstrap {
    fn on_start(&self, sim: &mut impl Sim) {
        let world = Shared::default();
        for name in REPLICAS {
            let replica_world = Arc::clone(&world);
            sim.host(name, move || {
                replica(name.to_owned(), Arc::clone(&replica_world))
            });
            // A crash loses what the replica wrote but did not sync.
            let disk_world = Arc::clone(&world);
            sim.on_crash(name, move |host| {
                if let Some(disk) = disk_world.lock().unwrap().disks.get_mut(host) {
                    disk.written = None;
                }
            });
        }
        sim.client("nemesis", nemesis(world));
    }
}
