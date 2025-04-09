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
    Crash,
    Bounce,
}

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn handle_actions(_sim: &mut Sim<'_>) {
    // static BOUNCED: LazyLock<Arc<Mutex<bool>>> = LazyLock::new(|| Arc::new(Mutex::new(false)));

    let mut actions = ACTIONS.lock().unwrap();
    for action in actions.drain(..) {
        match action {
            Action::Crash => {
                log::info!("crashing '{HOST}'");
                // sim.crash(HOST);
            }
            Action::Bounce => {
                log::info!("bouncing '{HOST}'");
                // let mut bounced = BOUNCED.lock().unwrap();
                // if !*bounced {
                //     *bounced = true;
                //     sim.bounce(HOST);
                // }
                // drop(bounced);
            }
        }
    }
    drop(actions);
}

/// # Errors
///
/// * If fails to connect to the TCP stream after 10 tries
pub async fn try_connect(addr: &str) -> Result<TcpStream, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        match turmoil::net::TcpStream::connect(addr).await {
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
    })
}
