use moosicbox_simulator_harness::{random::RNG, turmoil::Sim};
use moosicbox_simulator_utils::SIMULATOR_CANCELLATION_TOKEN;

pub fn start(sim: &mut Sim<'_>) {
    sim.client("McFaultInjector", async move {
        SIMULATOR_CANCELLATION_TOKEN
            .run_until_cancelled(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(RNG.gen_range(0..60))).await;
                }
            })
            .await
            .transpose()
            .map(|x| x.unwrap_or(()))
    });
}
