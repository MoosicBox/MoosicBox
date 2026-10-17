//! Production native controls driven exclusively by keyboard events.
#[path = "../examples/support/mod.rs"]
mod support;

use bmux_keyboard::{KeyCode, KeyStroke};
use bmux_tui::{event::Event, geometry::Size};
use hyperchad_renderer::View;
use hyperchad_renderer_bmux::{
    controls::{ControlEvent, ControlValue, Controls},
    layout::{LayoutOptions, layout},
    state::ViewState,
};
use support::{Workflow, document};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyStroke::simple(code))
}

#[test]
fn all_workflows_keyboard_edit_toggle_select_navigate_and_reconcile() {
    for workflow in [
        Workflow::Dashboard,
        Workflow::Documents,
        Workflow::MusicLibrary,
    ] {
        let mut state = ViewState::from_root(document(workflow)).unwrap();
        let mut controls = Controls::new();
        controls.reconcile(state.root());
        let tree = layout(
            state.root(),
            &LayoutOptions {
                viewport: Size::new(120, 60),
                ..LayoutOptions::default()
            },
            &controls.measurer(state.root()),
        )
        .unwrap();
        controls.prepare(&tree);
        // Tab reaches each target without pointer dispatch or direct focus injection.
        for target in ["search", "offline", "sort", "notes", "nav-documents"] {
            for _ in 0..32 {
                if controls.focused() == Some(target) {
                    break;
                }
                controls.event(&key(KeyCode::Tab));
            }
            assert_eq!(
                controls.focused(),
                Some(target),
                "keyboard reachability: {workflow:?}"
            );
            match target {
                "search" => {
                    controls.event(&key(KeyCode::Char('x')));
                    assert_eq!(
                        controls.value("search"),
                        Some(ControlValue::Text("x".into()))
                    );
                    state
                        .apply_view(
                            View::builder()
                                .with_fragment(support::text(
                                    50,
                                    "status",
                                    "Background refresh complete",
                                ))
                                .build(),
                            None,
                        )
                        .unwrap();
                    controls.reconcile(state.root());
                    assert_eq!(controls.focused(), Some("search"));
                    assert_eq!(
                        controls.value("search"),
                        Some(ControlValue::Text("x".into()))
                    );
                    let resized = layout(
                        state.root(),
                        &LayoutOptions {
                            viewport: Size::new(60, 32),
                            ..LayoutOptions::default()
                        },
                        &controls.measurer(state.root()),
                    )
                    .unwrap();
                    controls.prepare(&resized);
                    controls.event(&key(KeyCode::Char('y')));
                    assert_eq!(
                        controls.value("search"),
                        Some(ControlValue::Text("xy".into()))
                    );
                }
                "offline" => {
                    controls.event(&key(KeyCode::Char(' ')));
                    assert_eq!(controls.value("offline"), Some(ControlValue::Checked(true)));
                }
                "sort" => {
                    controls.event(&key(KeyCode::Enter));
                    controls.event(&key(KeyCode::Down));
                    controls.event(&key(KeyCode::Enter));
                    assert_eq!(
                        controls.value("sort"),
                        Some(ControlValue::Text("artist".into()))
                    );
                }
                "notes" => {
                    controls.event(&key(KeyCode::End));
                    controls.event(&key(KeyCode::Char('!')));
                    let Some(ControlValue::Text(value)) = controls.value("notes") else {
                        panic!("notes text");
                    };
                    assert!(value.contains('!'));
                }
                "nav-documents" => {
                    let outcome = controls.event(&key(KeyCode::Enter));
                    assert!(outcome.events.iter().any(|event| matches!(event, ControlEvent::Navigate { href, .. } if href == "/documents")));
                }
                _ => unreachable!(),
            }
        }
    }
}
