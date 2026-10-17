//! Full native pages must paint opaque cells through navigation and partial updates.
#![cfg(feature = "bmux")]

use bmux_tui_runtime::{Program, RuntimeEvent};
use hyperchad::{
    renderer::View,
    renderer_bmux::{
        program::{BmuxProgram, Message},
        renderer::BmuxConfig,
    },
    transformer::{Container, Element},
};
use moosicbox_app_models::Connection;

#[test]
fn settings_navigation_and_connection_fragments_keep_opaque_backgrounds() {
    let (actions, _) = flume::bounded(8);
    let (resize, _) = flume::bounded(8);
    let mut program = BmuxProgram::new(BmuxConfig::default(), actions, resize);
    program
        .update(RuntimeEvent::Message(Message::Background(Some(
            hyperchad::color::Color::from_hex("#181a1b"),
        ))))
        .unwrap();
    program.set_graphics_available(false);
    let state = moosicbox_app_native_ui::state::State::default();
    let connections = vec![
        Connection {
            name: "Disconnected A".into(),
            api_url: "http://127.0.0.1:1".into(),
        },
        Connection {
            name: "Disconnected B".into(),
            api_url: "http://127.0.0.1:2".into(),
        },
    ];
    let settings: Container = moosicbox_app_native_ui::settings::settings(
        &state,
        "Desktop",
        &connections,
        Some(&connections[0]),
        &[],
    )
    .into();
    let home: Container = moosicbox_app_native_ui::page(
        &state,
        &vec![Container {
            element: Element::Raw {
                value: "Home".into(),
            },
            ..Default::default()
        }],
    )
    .into();
    for page in [home.clone(), settings.clone(), home, settings] {
        program.apply_view(View::from(page), None).unwrap();
        assert_opaque(&mut program);
    }
    let fragment: Container =
        moosicbox_app_native_ui::settings::connections_content(&connections, Some(&connections[1]))
            .into();
    program
        .apply_view(View::from(fragment), Some("settings-connections"))
        .unwrap();
    assert_opaque(&mut program);
}

fn assert_opaque(program: &mut BmuxProgram) {
    program.prepare().unwrap();
    let mut buffer = bmux_tui::buffer::Buffer::empty(bmux_tui::geometry::Rect::new(0, 0, 80, 24));
    let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
    program.paint(&mut bmux_tui::paint::PaintCx::new(&mut frame));
    for cell in buffer.cells() {
        assert!(
            cell.style.bg.is_some(),
            "transparent cell: {:?}",
            cell.symbol
        );
    }
    program.commit_prepared();
}
