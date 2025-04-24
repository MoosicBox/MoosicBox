use moosicbox_simulator_harness::{
    plan::InteractionPlan as _, random::RNG, turmoil::Sim, utils::SIMULATOR_CANCELLATION_TOKEN,
};
use plan::{FaultInjectionInteractionPlan, Interaction};

pub mod plan;

use crate::{host::moosicbox_server::HOST, queue_bounce};

/// # Panics
///
/// * If `ACTIONS` `Mutex` fails to lock
pub fn start(sim: &mut Sim<'_>) {
    log::debug!("Generating initial test plan");

    let mut plan = FaultInjectionInteractionPlan::new().with_gen_interactions(1000);

    sim.client("FaultInjector", async move {
        SIMULATOR_CANCELLATION_TOKEN
            .run_until_cancelled(async move {
                loop {
                    while let Some(interaction) = plan.step() {
                        perform_interaction(interaction).await?;
                    }

                    plan.gen_interactions(1000);
                }
            })
            .await
            .transpose()
            .map(|x| x.unwrap_or(()))
    });
}

async fn perform_interaction(interaction: &Interaction) -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("perform_interaction: interaction={interaction:?}");

    match interaction {
        Interaction::Sleep(duration) => {
            log::debug!("perform_interaction: sleeping for duration={duration:?}");
            tokio::time::sleep(*duration).await;
        }
        Interaction::Bounce(host) => {
            let handle = crate::host::moosicbox_server::HANDLE
                .lock()
                .unwrap()
                .clone();
            if let Some(handle) = handle {
                log::debug!("perform_interaction: queueing bouncing '{host}'");
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
    }

    Ok(())
}
