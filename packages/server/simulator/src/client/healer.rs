use moosicbox_simulator_harness::{rand::Rng as _, turmoil::Sim};

use crate::{ACTIONS, Action, RNG, SIMULATOR_CANCELLATION_TOKEN};

/// # Panics
///
/// * If `RNG` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    sim.client("McHealer", {
        let actions = ACTIONS.clone();
        async move {
            loop {
                tokio::select! {
                    () = SIMULATOR_CANCELLATION_TOKEN.cancelled() => {
                        break;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_secs(RNG.lock().unwrap().gen_range(0..60))) => {}
                }

                actions.lock().unwrap().push_back(Action::Bounce);
            }

            Ok(())
        }
    });
}
