#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};

use simvar::{Sim, switchy::tcp::TcpStream};

pub mod client;
pub mod host;
pub mod http;

static ACTIONS: LazyLock<Arc<Mutex<VecDeque<Action>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::new())));

enum Action {
    Bounce(String),
}

/// # Panics
///
/// * If the `ACTIONS` `Mutex` fails to lock
pub fn queue_bounce(host: impl Into<String>) {
    ACTIONS
        .lock()
        .unwrap()
        .push_back(Action::Bounce(host.into()));
}

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn handle_actions(sim: &mut impl Sim) {
    let actions = ACTIONS.lock().unwrap().drain(..).collect::<Vec<_>>();
    for action in actions {
        match action {
            Action::Bounce(host) => {
                log::debug!("bouncing '{host}'");
                sim.bounce(host);
            }
        }
    }
}

/// # Errors
///
/// * If fails to connect to the TCP stream after `max_attempts` tries
pub async fn try_connect(addr: &str, max_attempts: usize) -> Result<TcpStream, std::io::Error> {
    let mut count = 0;
    Ok(loop {
        tokio::select! {
            resp = TcpStream::connect(addr) => {
                match resp {
                    Ok(x) => break x,
                    Err(e) => {
                        count += 1;

                        log::debug!("failed to bind tcp: {e:?} (attempt {count}/{max_attempts})");

                        if !matches!(e.kind(), std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset)
                            || count >= max_attempts
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
