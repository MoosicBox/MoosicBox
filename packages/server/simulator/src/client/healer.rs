use moosicbox_simulator_harness::{rand::Rng as _, turmoil::Sim};

use crate::{ACTIONS, Action, RNG, SIMULATOR_CANCELLATION_TOKEN, host::moosicbox_server::HOST};

/// # Panics
///
/// * If `CANCELLATION_TOKEN` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    sim.client("McHealer", {
        async move {
            loop {
                tokio::select! {
                    () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                        break;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(RNG.lock().unwrap().gen_range(0..60))) => {}
                }

                let handle = crate::host::moosicbox_server::HANDLE.lock().unwrap().clone();
                if let Some(handle) = handle {
                    let token = crate::host::moosicbox_server::CANCELLATION_TOKEN.lock().unwrap().clone();
                    if let Some(token) = token {
                        token.cancel();
                    }
                    let gracefully = RNG.lock().unwrap().gen_bool(0.8);
                    log::info!("stopping '{HOST}' gracefully={gracefully}");
                    handle.stop(gracefully).await;
                    log::info!("stopped '{HOST}' gracefully={gracefully}");
                    ACTIONS.lock().unwrap().push_back(Action::Bounce);
                }
            }

            Ok(())
        }
    });
}
