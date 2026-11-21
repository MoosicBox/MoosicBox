//! A bank whose tellers are simulated threads, used to show and test deterministic
//! multithreaded simulation with simvar.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::time::Duration;

use simvar::{Sim, SimBootstrap, SimConfig};
use switchy_thread::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

/// Accounts in the bank.
pub const ACCOUNTS: usize = 4;
/// Starting balance of each account.
pub const STARTING_BALANCE: u64 = 100;

/// Runs the bank, optionally with a deliberate bug.
///
/// With `nondeterministic`, tellers deliberately mix std's per-process random hasher keys into
/// their decisions, which determinism verification must catch. With `racy`, tellers write
/// back balances they read before letting go of the locks, so a transfer that runs in between
/// is lost and some interleavings create or destroy money.
#[derive(Debug, Clone, Copy, Default)]
pub struct BankBootstrap {
    /// Mix randomness from outside the simulation into the run.
    pub nondeterministic: bool,
    /// Lose updates under some interleavings.
    pub racy: bool,
}

impl SimBootstrap for BankBootstrap {
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        config.duration = Duration::from_secs(1);
        config
    }

    fn on_start(&self, sim: &mut impl Sim) {
        let Self {
            nondeterministic,
            racy,
        } = *self;
        sim.client("bank", async move {
            let balances = run_bank(nondeterministic, racy);
            let total: u64 = balances.iter().sum();
            assert_eq!(
                total,
                STARTING_BALANCE * ACCOUNTS as u64,
                "money was created or destroyed: {balances:?}"
            );
            Ok(())
        });
    }
}

/// Three tellers transfer between accounts while an auditor reads the ledger (racily, with
/// `racy`; see [`BankBootstrap`]). Returns the final balances; every transfer is recorded in
/// the run's trace.
///
/// # Panics
///
/// * If a teller or the auditor panics
#[must_use]
pub fn run_bank(nondeterministic: bool, racy: bool) -> Vec<u64> {
    let accounts: Arc<Vec<Mutex<u64>>> = Arc::new(
        (0..ACCOUNTS)
            .map(|_| Mutex::new(STARTING_BALANCE))
            .collect(),
    );
    let (ledger_tx, ledger_rx) = mpsc::sync_channel::<(usize, usize, u64)>(4);

    let tellers: Vec<_> = (0..3)
        .map(|teller| {
            let accounts = Arc::clone(&accounts);
            let ledger = ledger_tx.clone();
            thread::Builder::new()
                .name(format!("teller-{teller}"))
                .spawn(move || {
                    for round in 0..5 {
                        let from = if nondeterministic {
                            // Deliberately non-deterministic: std's hasher keys are random
                            // per process and per call, so they pick a different account.
                            use std::hash::BuildHasher as _;
                            let random =
                                std::collections::hash_map::RandomState::new().hash_one(round);
                            usize::try_from(random % ACCOUNTS as u64).unwrap()
                        } else {
                            (teller + round) % ACCOUNTS
                        };
                        let to = (from + 1 + teller) % ACCOUNTS;
                        if from == to {
                            continue;
                        }
                        // Lock in a fixed order so tellers cannot deadlock.
                        let (first, second) = (from.min(to), from.max(to));
                        let mut a = accounts[first].lock().unwrap();
                        let mut b = accounts[second].lock().unwrap();
                        let (source, target) = if from == first {
                            (&mut *a, &mut *b)
                        } else {
                            (&mut *b, &mut *a)
                        };
                        let amount = (*source).min(10);
                        if racy {
                            let (source, target) = (*source, *target);
                            drop(a);
                            drop(b);
                            thread::yield_now();
                            *accounts[from].lock().unwrap() = source - amount;
                            *accounts[to].lock().unwrap() = target + amount;
                        } else {
                            *source -= amount;
                            *target += amount;
                            drop(a);
                            drop(b);
                        }
                        ledger.send((from, to, amount)).unwrap();
                    }
                })
                .unwrap()
        })
        .collect();
    drop(ledger_tx);

    let auditor = thread::Builder::new()
        .name("auditor".into())
        .spawn(move || {
            for (from, to, amount) in ledger_rx {
                simvar::record(((from as u64) << 32) | ((to as u64) << 16) | amount);
            }
        })
        .unwrap();

    for teller in tellers {
        teller.join().unwrap();
    }
    auditor.join().unwrap();

    let balances: Vec<u64> = accounts.iter().map(|a| *a.lock().unwrap()).collect();
    for &balance in &balances {
        simvar::record(balance);
    }
    balances
}
