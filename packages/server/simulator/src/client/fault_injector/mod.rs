use moosicbox_simulator_harness::{Sim, plan::InteractionPlan as _, switchy::random::rng};
use plan::{FaultInjectionInteractionPlan, Interaction};

pub mod plan;

use crate::{host::moosicbox_server::HOST, queue_bounce};

pub fn start(sim: &mut impl Sim) {
    log::debug!("Generating initial test plan");

    let mut plan = FaultInjectionInteractionPlan::new().with_gen_interactions(1000);

    sim.client("FaultInjector", async move {
        loop {
            while let Some(interaction) = plan.step() {
                perform_interaction(interaction).await?;
            }

            plan.gen_interactions(1000);
        }
    });
}

async fn perform_interaction(
    interaction: &Interaction,
) -> Result<(), Box<dyn std::error::Error + Send>> {
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
                let gracefully = rng().gen_bool(0.8);
                log::info!("stopping '{HOST}' gracefully={gracefully}");
                handle.stop(gracefully).await;
                log::info!("stopped '{HOST}' gracefully={gracefully}");
                queue_bounce(HOST.to_string());
            }
        }
    }

    Ok(())
}
