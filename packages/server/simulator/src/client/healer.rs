use moosicbox_simulator_harness::{random::RNG, turmoil::Sim};
use moosicbox_simulator_utils::SIMULATOR_CANCELLATION_TOKEN;

use crate::{host::moosicbox_server::HOST, queue_bounce};

/// # Panics
///
/// * If `CANCELLATION_TOKEN` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    sim.client("McHealer", async move {
        SIMULATOR_CANCELLATION_TOKEN
            .run_until_cancelled(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(RNG.gen_range(0..60))).await;

                    let handle = crate::host::moosicbox_server::HANDLE
                        .lock()
                        .unwrap()
                        .clone();
                    if let Some(handle) = handle {
                        let token = crate::host::moosicbox_server::CANCELLATION_TOKEN
                            .lock()
                            .unwrap()
                            .clone();
                        if let Some(token) = token {
                            token.cancel();
                        }
                        let gracefully = RNG.gen_bool(0.8);
                        log::info!("stopping '{HOST}' gracefully={gracefully}");
                        handle.stop(gracefully).await;
                        log::info!("stopped '{HOST}' gracefully={gracefully}");
                        queue_bounce(HOST.to_string());
                    }
                }
            })
            .await
            .transpose()
            .map(|x| x.unwrap_or(()))
    });
}
