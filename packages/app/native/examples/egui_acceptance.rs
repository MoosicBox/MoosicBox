//! Production-template acceptance fixture, not a connected application.
//! Fonts are caller-supplied files; no server, global STATE, or downloads are used.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use hyperchad::{
    actions::{Action, ActionTrigger, ActionType},
    renderer::{
        Content, Renderer, ToRenderRunner, View,
        canvas::{CanvasAction, CanvasUpdate, Pos},
    },
    renderer_egui::{
        EguiRenderer, eframe::egui, font_metrics::EguiFontMetrics, layout::EguiCalc,
        text_layout::FontRegistry,
    },
    router::{ClientInfo, Router},
    template::{Containers, container},
    transformer::{
        Container,
        layout::{
            Calc, LayoutError,
            calc::{Calculator, CalculatorDefaults},
        },
    },
};
use std::sync::Arc;

#[derive(Clone, Default)]
struct AcceptanceCalc(Option<egui::Context>);
impl EguiCalc for AcceptanceCalc {
    fn with_context(self, context: egui::Context) -> Self {
        Self(Some(context))
    }
}
impl Calc for AcceptanceCalc {
    fn calc(&self, container: &mut Container) -> bool {
        self.try_calc(container).expect("acceptance layout")
    }
    fn try_calc(&self, container: &mut Container) -> Result<bool, LayoutError> {
        let defaults = CalculatorDefaults {
            font_size: 16.0,
            font_margin_top: 0.0,
            font_margin_bottom: 0.0,
            h1_font_size: 28.0,
            h1_font_margin_top: 0.0,
            h1_font_margin_bottom: 0.0,
            h2_font_size: 24.0,
            h2_font_margin_top: 0.0,
            h2_font_margin_bottom: 0.0,
            h3_font_size: 20.0,
            h3_font_margin_top: 0.0,
            h3_font_margin_bottom: 0.0,
            h4_font_size: 18.0,
            h4_font_margin_top: 0.0,
            h4_font_margin_bottom: 0.0,
            h5_font_size: 16.0,
            h5_font_margin_top: 0.0,
            h5_font_margin_bottom: 0.0,
            h6_font_size: 14.0,
            h6_font_margin_top: 0.0,
            h6_font_margin_bottom: 0.0,
        };
        Calculator::new(
            EguiFontMetrics::new(self.0.clone().expect("initialized context")),
            defaults,
        )
        .try_calc(container)
    }
}

fn custom(container: &mut Container, name: &str) {
    container.actions.push(Action {
        trigger: ActionTrigger::Click,
        effect: ActionType::Custom {
            action: name.into(),
        }
        .into(),
    });
}

fn page() -> Container {
    let header = container! { h2 id="modal-heading" { "Production modal overflow" } };
    let content: Containers = (0..30)
        .flat_map(|index| {
            let id = if index == 0 {
                "modal-first-row".to_owned()
            } else if index == 29 {
                "modal-last-row".to_owned()
            } else {
                format!("modal-row-{index}")
            };
            container! { div id=(id) height=40 { (format!("Modal content row {index}")) } }
        })
        .collect();
    let modal = moosicbox_app_native_ui::modal("acceptance-modal", &header, &content);
    let mut root = Container::from(container! {
        div id="acceptance-root" width=100% height=100% background=#181a1b color=#ffffff padding=24 gap=12 {
            h1 { "Production-template fixture — not connected App" }
            button id="open-modal" width=180 height=32 fx-click=fx { element_by_id("acceptance-modal").show() } { "Open modal" }
            div id="font-light-300" font-weight=300 { "Bundled Ubuntu Light 300" }
            div id="font-regular-400" font-family="FixtureSans" font-weight=400 { "Registered regular 400 — café Ω" }
            div id="font-bold-700" font-family="FixtureSans" font-weight=700 { "Registered bold 700 — café Ω" }
            div direction=row gap=12 {
                button id="canvas-draw" width=120 height=32 { "Draw" }
                button id="canvas-clear" width=120 height=32 { "Clear" }
                button id="canvas-replay" width=120 height=32 { "Replay" }
            }
            canvas id="acceptance-canvas" width=400 height=120 {}
            (modal)
        }
    });
    for (id, action) in [
        ("canvas-draw", "canvas-draw"),
        ("canvas-clear", "canvas-clear"),
        ("canvas-replay", "canvas-replay"),
    ] {
        custom(
            root.find_element_by_str_id_mut(id)
                .expect("fixture semantic ID"),
            action,
        );
    }
    root
}

fn registry() -> Result<Arc<FontRegistry>, Box<dyn std::error::Error>> {
    let mut builder = FontRegistry::builder();
    // Genuine bundled Light bytes, not a regular face relabeled as weight 300.
    let bundled = egui::FontDefinitions::default();
    let light = bundled
        .font_data
        .get("Ubuntu-Light")
        .ok_or_else(|| std::io::Error::other("bundled Ubuntu-Light missing"))?;
    builder.register_face("FixtureSans", 300, light.as_ref().clone())?;
    builder.alias("sans-serif", "FixtureSans");
    for (variable, weight) in [
        ("HYPERCHAD_ACCEPTANCE_FONT_REGULAR", 400),
        ("HYPERCHAD_ACCEPTANCE_FONT_BOLD", 700),
    ] {
        let path = std::env::var_os(variable).ok_or_else(|| {
            std::io::Error::other(format!("required opt-in font path: {variable}"))
        })?;
        let bytes = std::fs::read(&path)?;
        let digest = std::process::Command::new("sha256sum")
            .arg(&path)
            .output()?;
        if !digest.status.success() {
            return Err(std::io::Error::other("font SHA-256 calculation failed").into());
        }
        eprintln!(
            "acceptance font {variable} weight={weight}: {}",
            String::from_utf8(digest.stdout)?
        );
        builder.register_face("FixtureSans", weight, egui::FontData::from_owned(bytes))?;
    }
    builder.set_default_face("FixtureSans", 400);
    let registry = Arc::new(builder.build()?);
    preflight_registry(&registry)?;
    Ok(registry)
}

fn preflight_registry(registry: &FontRegistry) -> Result<(), String> {
    for weight in [300, 400, 700] {
        registry.resolve(Some(&["FixtureSans".to_owned()]), Some(weight), 16.0)?;
        registry.resolve(Some(&["sans-serif".to_owned()]), Some(weight), 16.0)?;
        registry.resolve(None, Some(weight), 16.0)?;
    }
    registry.resolve(None, None, 16.0)?;
    Ok(())
}

fn drawing(clear: bool) -> CanvasUpdate {
    CanvasUpdate {
        target: "acceptance-canvas".into(),
        canvas_actions: if clear {
            vec![CanvasAction::Clear]
        } else {
            vec![
                CanvasAction::Clear,
                CanvasAction::StrokeColor(hyperchad::color::Color {
                    r: 0,
                    g: 220,
                    b: 180,
                    a: None,
                }),
                CanvasAction::StrokeSize(4.0),
                CanvasAction::Line(Pos(20.0, 20.0), Pos(360.0, 100.0)),
                CanvasAction::FillRect(Pos(50.0, 40.0), Pos(110.0, 90.0)),
            ]
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_fixture_registry_resolves_actual_authored_requests() {
        if std::env::var_os("HYPERCHAD_ACCEPTANCE_FONT_REGULAR").is_none() {
            return;
        }
        let registry = registry().expect("supplied fixture fonts");
        preflight_registry(&registry).unwrap();
        assert_eq!(registry.compatibility_weight(), Some(300));
    }

    #[test]
    fn executor_progresses_while_ui_thread_is_blocked() {
        let runtime = switchy::unsync::runtime::Builder::new()
            .max_blocking_threads(2_u16)
            .build()
            .unwrap();
        let ui_thread = std::thread::current().id();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        runtime.spawn(async move {
            send.send(std::thread::current().id()).unwrap();
        });
        let worker = receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("executor must progress without UI-thread block_on");
        assert_ne!(worker, ui_thread);
    }

    #[test]
    fn production_modal_and_canvas_semantics_are_present() {
        let mut root = page();
        for id in [
            "acceptance-modal",
            "modal-heading",
            "modal-first-row",
            "modal-last-row",
            "open-modal",
            "acceptance-canvas",
            "canvas-draw",
            "canvas-clear",
            "canvas-replay",
            "font-light-300",
            "font-regular-400",
            "font-bold-700",
        ] {
            assert!(
                root.find_element_by_str_id_mut(id).is_some(),
                "missing {id}"
            );
        }
        assert_eq!(drawing(true).canvas_actions.len(), 1);
        assert!(matches!(
            drawing(true).canvas_actions[0],
            CanvasAction::Clear
        ));
        assert_eq!(drawing(false).canvas_actions.len(), 5);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send>> {
    let error =
        |e: String| -> Box<dyn std::error::Error + Send> { Box::new(std::io::Error::other(e)) };
    let registry = registry().map_err(|e| error(e.to_string()))?;
    let runtime = switchy::unsync::runtime::Builder::new()
        .max_blocking_threads(2_u16)
        .build()
        .map_err(|e| error(e.to_string()))?;
    let router = Router::new()
        .with_route("/acceptance", |_| async { View::from(page()) })
        .with_route("/public/img/cross-white.svg", |_| async {
            Content::Raw {
                data: include_bytes!("../public/img/cross-white.svg")
                    .as_slice()
                    .into(),
                content_type: "image/svg+xml".into(),
            }
        });
    let (actions, receiver) = flume::bounded(64);
    let (resize, _resize_receiver) = flume::unbounded();
    let mut renderer = EguiRenderer::new(
        router,
        actions,
        resize,
        Arc::new(ClientInfo::default()),
        AcceptanceCalc::default(),
    )
    .with_context_setup(move |ctx| {
        registry.install(ctx);
        Ok(())
    });
    runtime.block_on(renderer.init(
        1000.0,
        700.0,
        None,
        None,
        None,
        Some("Production-template acceptance fixture"),
        None,
        None,
    ))?;
    runtime.block_on(renderer.render(View::from(page())))?;
    let producer = renderer.clone();
    runtime.spawn(async move {
        let mut callback_count = 0_u64;
        let mut enqueued = 0_u64;
        while let Ok((action, _value)) = receiver.recv_async().await {
            callback_count = callback_count.saturating_add(1);
            if callback_count <= 64 {
                eprintln!("acceptance callback received={callback_count} action={action}");
            }
            if matches!(
                action.as_str(),
                "canvas-draw" | "canvas-clear" | "canvas-replay"
            ) {
                producer
                    .render_canvas(drawing(action == "canvas-clear"))
                    .await
                    .expect("canvas producer callback");
                enqueued = enqueued.saturating_add(1);
                if enqueued <= 64 {
                    eprintln!("acceptance canvas enqueued={enqueued} action={action}; mailbox admission, not paint acknowledgement");
                }
            }
        }
    });
    renderer.to_runner(runtime.handle())?.run()
}
