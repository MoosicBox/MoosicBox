#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use hyperchad::{
    app::AppBuilder,
    renderer::{Content, Renderer, View},
    router::{RouteRequest, Router},
    shared_state::{
        fanout::InProcessFanoutBus,
        runtime::{ApplyPreparedCommandResult, SharedStateEngine},
        storage::{SwitchySharedStateStore, migrate_shared_state},
        traits::{EventDraft, EventStore},
    },
    shared_state_bridge::{BridgeError, RouteCommandInput, SharedStateRouteResolver},
    shared_state_models::{
        ChannelId, CommandEnvelope, CommandId, IdempotencyKey, ParticipantId, PayloadBlob, Revision,
    },
    template::{Containers, container},
};
use log::info;

#[cfg(feature = "actix")]
use hyperchad::renderer_html_actix::RuntimeFanoutTransportDispatcher;

#[allow(unused_imports)]
use hyperchad::actions as hyperchad_actions;
#[allow(unused_imports)]
use hyperchad::color as hyperchad_color;
#[allow(unused_imports)]
use hyperchad::template as hyperchad_template;
#[allow(unused_imports)]
use hyperchad::template::template_actions_dsl as hyperchad_template_actions_dsl;
#[allow(unused_imports)]
use hyperchad::transformer as hyperchad_transformer;
#[allow(unused_imports)]
use hyperchad::transformer_models as hyperchad_transformer_models;

#[cfg(feature = "assets")]
use std::sync::LazyLock;

const COUNTER_CHANNEL: &str = "demo-counter";
const MAX_REPLAY_EVENTS: u32 = 10_000;
const COMMAND_APPLY_RETRY_COUNT: usize = 5;

type SharedStateRuntime = SharedStateEngine<
    SwitchySharedStateStore,
    SwitchySharedStateStore,
    SwitchySharedStateStore,
    InProcessFanoutBus,
>;

#[cfg(feature = "assets")]
static ASSETS: LazyLock<Vec<hyperchad::renderer::assets::StaticAssetRoute>> = LazyLock::new(|| {
    vec![
        #[cfg(feature = "vanilla-js")]
        hyperchad::renderer::assets::StaticAssetRoute {
            route: format!(
                "js/{}",
                hyperchad::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
            ),
            target: hyperchad::renderer::assets::AssetPathTarget::FileContents(
                hyperchad::renderer_vanilla_js::SCRIPT.as_bytes().into(),
            ),
            not_found_behavior: None,
        },
    ]
});

#[derive(Debug)]
struct DemoRouteResolver;

impl SharedStateRouteResolver for DemoRouteResolver {
    fn resolve_channel_id(&self, _request: &RouteRequest) -> Result<ChannelId, BridgeError> {
        Ok(ChannelId::new(COUNTER_CHANNEL))
    }

    fn resolve_participant_id(&self, request: &RouteRequest) -> Result<ParticipantId, BridgeError> {
        let participant_id = request
            .cookies
            .get("player_id")
            .cloned()
            .unwrap_or_else(|| "demo-player".to_string());

        Ok(ParticipantId::new(participant_id))
    }
}

fn create_main_page(counter: i64) -> Containers {
    container! {
        div
            id="app"
            data-shared-state-channel=(COUNTER_CHANNEL)
            background=#f7f7f2
            color=#1a1f16
            justify-content=center
            align-items=center
            padding=24
        {
            div
                max-width=640
                width=100%
                background=#ffffff
                border-radius=16
                padding=24
                gap=16
            {
                h1 { "HyperChad Shared State Counter" }

                span color=#5a6b52 {
                    "Open this page in multiple tabs. Every click updates all tabs without custom JS."
                }

                div
                    direction=row
                    justify-content=space-evenly
                    align-items=center
                    padding=16
                    border-radius=12
                    background=#eef3e7
                {
                    button
                        fx-click=fx { custom("'decrement'") }
                        padding-x=16
                        padding-y=12
                        border-radius=10
                        background=#dce6cf
                    {
                        "-1"
                    }

                    div
                        text-align=center
                        min-width=180
                    {
                        span color=#5a6b52 { "Shared counter" }
                        h2 id="counter-value" { (counter.to_string()) }
                    }

                    button
                        fx-click=fx { custom("'increment'") }
                        padding-x=16
                        padding-y=12
                        border-radius=10
                        background=#dce6cf
                    {
                        "+1"
                    }
                }

                span color=#5a6b52 {
                    "Commands flow through /$action + shared-state bridge. Counter updates stream as server-rendered partial fragments."
                }
            }
        }
    }
}

fn create_counter_value_fragment(counter: i64) -> hyperchad::transformer::Container {
    container! {
        h2 id="counter-value" { (counter.to_string()) }
    }
    .into_iter()
    .next()
    .expect("counter fragment container should exist")
}

async fn publish_counter_fragment<R: Renderer>(renderer: &R, store: &SwitchySharedStateStore) {
    let counter = match load_counter_value(store).await {
        Ok(counter) => counter,
        Err(error) => {
            log::error!("Failed to load counter for fragment render: {error}");
            return;
        }
    };

    let view = View::builder()
        .with_fragment(create_counter_value_fragment(counter))
        .build();

    if let Err(error) = renderer.render(view).await {
        log::error!("Failed to publish counter fragment update: {error}");
    }
}

async fn load_counter_value(
    store: &SwitchySharedStateStore,
) -> Result<i64, Box<dyn std::error::Error>> {
    let events = store
        .read_events(&ChannelId::new(COUNTER_CHANNEL), None, MAX_REPLAY_EVENTS)
        .await?;

    let mut counter = 0_i64;
    for event in events {
        if event.event_name != "COUNTER_DELTA" {
            continue;
        }

        match event.payload.deserialize::<i64>() {
            Ok(delta) => {
                counter += delta;
            }
            Err(error) => {
                log::warn!(
                    "Failed to deserialize COUNTER_DELTA payload for event {}: {error}",
                    event.event_id
                );
            }
        }
    }

    Ok(counter)
}

fn create_router(store: SwitchySharedStateStore) -> Router {
    Router::new().with_route("/", move |_req: RouteRequest| {
        let store = store.clone();

        async move {
            let counter = match load_counter_value(&store).await {
                Ok(counter) => counter,
                Err(error) => {
                    log::error!("Failed to load shared counter value: {error}");
                    0
                }
            };

            Content::from(create_main_page(counter))
        }
    })
}

async fn apply_command_with_retry(
    renderer: &impl Renderer,
    engine: &SharedStateRuntime,
    store: &SwitchySharedStateStore,
    command: CommandEnvelope,
) {
    for attempt in 0..COMMAND_APPLY_RETRY_COUNT {
        let mut resolved_command = command.clone();

        let latest_revision = match store.latest_revision(&resolved_command.channel_id).await {
            Ok(Some(revision)) => revision,
            Ok(None) => Revision::new(0),
            Err(error) => {
                log::error!(
                    "Failed to load latest revision for channel {}: {error}",
                    resolved_command.channel_id
                );
                return;
            }
        };
        resolved_command.expected_revision = latest_revision;

        let drafts = vec![EventDraft::new(
            resolved_command.command_name.clone(),
            resolved_command.payload.clone(),
            resolved_command.metadata.clone(),
        )];

        match engine
            .apply_prepared(&resolved_command, &drafts, None)
            .await
        {
            Ok(ApplyPreparedCommandResult::Conflict { actual_revision }) => {
                log::warn!(
                    "Conflict applying command {} (attempt {}), expected {} actual {}",
                    resolved_command.command_id,
                    attempt + 1,
                    resolved_command.expected_revision,
                    actual_revision
                );

                if attempt + 1 == COMMAND_APPLY_RETRY_COUNT {
                    log::error!(
                        "Giving up on command {} after {} conflicts",
                        resolved_command.command_id,
                        COMMAND_APPLY_RETRY_COUNT
                    );
                }
            }
            Ok(result) => {
                log::debug!(
                    "Applied command {} with result {:?}",
                    resolved_command.command_id,
                    result
                );

                publish_counter_fragment(renderer, store).await;
                return;
            }
            Err(error) => {
                log::error!(
                    "Failed to apply command {}: {error}",
                    resolved_command.command_id
                );
                return;
            }
        }
    }
}

fn spawn_command_processor(
    renderer: impl Renderer + Clone + 'static,
    runtime_handle: &switchy::unsync::runtime::Handle,
    command_rx: flume::Receiver<CommandEnvelope>,
    engine: Arc<SharedStateRuntime>,
    store: SwitchySharedStateStore,
) {
    runtime_handle.spawn(async move {
        while let Ok(command) = command_rx.recv_async().await {
            apply_command_with_retry(&renderer, engine.as_ref(), &store, command).await;
        }
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting HyperChad shared-state transport example");

    let runtime = switchy::unsync::runtime::Builder::new().build()?;

    #[cfg_attr(not(feature = "actix"), allow(unused_variables))]
    let (store, engine, fanout_bus) = runtime.block_on(async {
        let database = switchy_database_connection::init_sqlite_sqlx(None).await?;
        let database = Arc::new(database);
        migrate_shared_state(database.as_ref().as_ref()).await?;

        let store = SwitchySharedStateStore::new(database);
        let fanout_bus = Arc::new(InProcessFanoutBus::new());
        let command_store = Arc::new(store.clone());
        let event_store = Arc::new(store.clone());
        let snapshot_store = Arc::new(store.clone());
        let engine = Arc::new(SharedStateEngine::new(
            command_store,
            event_store,
            snapshot_store,
            fanout_bus.clone(),
        ));

        Ok::<_, Box<dyn std::error::Error>>((store, engine, fanout_bus))
    })?;

    #[cfg(feature = "actix")]
    let dispatcher = Arc::new(RuntimeFanoutTransportDispatcher::new(
        engine.clone(),
        fanout_bus,
    ));

    let command_sequence = Arc::new(AtomicU64::new(1));
    let (command_tx, command_rx) = flume::unbounded();
    let runtime_handle = runtime.handle();
    let command_runtime_handle = runtime_handle.clone();

    let router = create_router(store.clone());

    let app_builder = AppBuilder::new()
        .with_router(router)
        .with_runtime_handle(runtime_handle)
        .with_title("HyperChad Shared State Counter".to_string())
        .with_description(
            "Shared-state multiplayer counter with zero custom JavaScript".to_string(),
        )
        .with_shared_state_route_bridge(command_tx, Arc::new(DemoRouteResolver), {
            let command_sequence = Arc::clone(&command_sequence);
            move |action: &str, _value| {
                let delta = match action {
                    "increment" => 1_i64,
                    "decrement" => -1_i64,
                    _ => return None,
                };

                let sequence = command_sequence.fetch_add(1, Ordering::Relaxed);
                let payload = match PayloadBlob::from_serializable(&delta) {
                    Ok(payload) => payload,
                    Err(error) => {
                        log::error!("Failed to serialize command payload: {error}");
                        return None;
                    }
                };

                Some(RouteCommandInput {
                    command_id: CommandId::new(format!("demo-command-{sequence}")),
                    idempotency_key: IdempotencyKey::new(format!("demo-idem-{sequence}")),
                    expected_revision: Revision::new(0),
                    command_name: "COUNTER_DELTA".to_string(),
                    payload,
                    metadata: BTreeMap::new(),
                })
            }
        });

    #[cfg(feature = "actix")]
    let app_builder = app_builder.with_shared_state_transport_dispatcher(dispatcher);

    #[cfg_attr(not(feature = "assets"), allow(unused_mut))]
    let mut app_builder = app_builder;

    #[cfg(feature = "assets")]
    for asset in ASSETS.iter().cloned() {
        app_builder.static_asset_route_result(asset)?;
    }

    info!("Server running on http://localhost:8343");
    info!("Open two tabs and click +/- to see synchronized updates");

    let app = app_builder.build_default()?;
    let renderer = app.renderer.clone();

    spawn_command_processor(renderer, &command_runtime_handle, command_rx, engine, store);

    app.run()?;

    Ok(())
}
