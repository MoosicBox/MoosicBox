#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use host::moosicbox_server::HOST;
use moosicbox_simulator_harness::{
    getrandom,
    rand::{SeedableRng as _, rngs::SmallRng},
    turmoil::{self, Sim, net::TcpStream},
};
use tokio_util::sync::CancellationToken;

pub mod client;
pub mod host;
pub mod http;

pub static SIMULATOR_CANCELLATION_TOKEN: LazyLock<CancellationToken> =
    LazyLock::new(CancellationToken::new);
pub static SEED: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("SIMULATOR_SEED")
        .ok()
        .and_then(|x| x.parse::<u64>().ok())
        .unwrap_or_else(|| getrandom::u64().unwrap())
});
pub static RNG: LazyLock<Arc<Mutex<SmallRng>>> =
    LazyLock::new(|| Arc::new(Mutex::new(SmallRng::seed_from_u64(*SEED))));
pub static ACTIONS: LazyLock<Arc<Mutex<VecDeque<Action>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::new())));

pub enum Action {
    Bounce,
}

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn handle_actions(sim: &mut Sim<'_>) {
    let actions = ACTIONS.lock().unwrap().drain(..).collect::<Vec<_>>();
    for action in actions {
        match action {
            Action::Bounce => {
                log::info!("bouncing '{HOST}'");
                sim.bounce(HOST);
            }
        }
    }
}

/// # Errors
///
/// * If fails to connect to the TCP stream after 10 tries
pub async fn try_connect(addr: &str) -> Result<TcpStream, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        tokio::select! {
            resp = turmoil::net::TcpStream::connect(addr) => {
                match resp {
                    Ok(x) => break x,
                    Err(e) => {
                        const MAX_ATTEMPTS: usize = 10;

                        count += 1;

                        log::debug!("failed to bind tcp: {e:?} (attempt {count}/{MAX_ATTEMPTS})");

                        if !matches!(e.kind(), std::io::ErrorKind::ConnectionRefused)
                            || count >= MAX_ATTEMPTS
                        {
                            return Err(e);
                        }

                        tokio::time::sleep(Duration::from_millis(5000)).await;
                    }
                }
            }
            () = tokio::time::sleep(Duration::from_millis(5000)) => {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Timed out after 5000ms"));
            }
        }
    })
}
