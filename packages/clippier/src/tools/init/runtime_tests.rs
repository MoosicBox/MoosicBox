use super::recommendations::{Choice, Group};
use super::runtime::Setup;
use bmux_keyboard::{KeyCode, KeyStroke, Modifiers};
use bmux_tui::{event::Event, geometry::Size};
use bmux_tui_runtime::{
    HeadlessPresenter, Invalidation, Program, Runtime, RuntimeConfig, RuntimeEvent,
};

fn groups() -> Vec<Group> {
    vec![Group {
        title: "Test".into(),
        choices: (0..10)
            .map(|i| Choice {
                name: format!("tool-{i}"),
                reason: String::new(),
                selected: false,
                installed: None,
                source_only: false,
                active: false,
            })
            .collect(),
        extensions: vec![],
        files: vec![],
        formatting: true,
    }]
}
const fn key(key: KeyCode) -> Event {
    Event::Key(KeyStroke {
        key,
        modifiers: Modifiers::NONE,
    })
}
const fn submit_key() -> Event {
    Event::Key(KeyStroke {
        key: KeyCode::Enter,
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        },
    })
}
#[test]
fn enter_only_submits_on_button_and_cancel_button_interrupts() {
    let mut groups = groups();
    let mut setup = program(&mut groups);
    setup
        .update(RuntimeEvent::Terminal(key(KeyCode::Enter)))
        .unwrap();
    assert!(!setup.accepted);
    assert!(setup.groups[0].choices[0].selected);
    setup
        .update(RuntimeEvent::Terminal(key(KeyCode::Enter)))
        .unwrap();
    assert!(!setup.groups[0].choices[0].selected);
    setup.focus = setup.positions.len();
    setup
        .update(RuntimeEvent::Terminal(key(KeyCode::Enter)))
        .unwrap();
    assert!(setup.accepted);
    let mut groups = self::groups();
    let mut setup = program(&mut groups);
    setup.focus = setup.positions.len() + 1;
    assert!(
        setup
            .update(RuntimeEvent::Terminal(key(KeyCode::Enter)))
            .is_err()
    );
    assert!(!setup.accepted);
}
fn program(groups: &mut [Group]) -> Setup<'_> {
    Setup {
        positions: super::inline::stops(groups),
        groups,
        focus: 0,
        size: Size::new(80, 24),
        accepted: false,
        pending_since: None,
        hits: Vec::new(),
        pressed: None,
    }
}
#[test]
fn q_and_escape_cancel_without_accepting() {
    for code in [KeyCode::Char('q'), KeyCode::Escape] {
        let mut groups = groups();
        let mut setup = program(&mut groups);
        let Err(error) = setup.update(RuntimeEvent::Terminal(key(code))) else {
            panic!("cancel key should interrupt setup");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert!(!setup.accepted);
    }
}

#[test]
fn horizontal_navigation_and_mouse_use_rendered_component_regions() {
    use bmux_tui::event::{MouseButton, MouseEvent, MouseEventKind};
    use bmux_tui::geometry::Point;
    let mut groups = (0..6).map(|_| self::groups().remove(0)).collect::<Vec<_>>();
    for group in &mut groups {
        group.choices.truncate(1);
    }
    let mut setup = program(&mut groups);
    setup.size = Size::new(121, 31);
    setup
        .update(RuntimeEvent::Terminal(key(KeyCode::Right)))
        .unwrap();
    assert_eq!(setup.focus, 3);
    setup
        .update(RuntimeEvent::Terminal(key(KeyCode::Left)))
        .unwrap();
    assert_eq!(setup.focus, 0);
    let (_, regions) = super::inline::render_scene(
        setup.groups,
        &[],
        (0, 0),
        120,
        30,
        &std::cell::Cell::new(bmux_tui_components::scroll_view::ScrollViewState::new()),
    );
    setup.hits = regions;
    for (id, toggles) in [("section-3", false), ("choice-3-0", true)] {
        let area = setup
            .hits
            .iter()
            .find(|region| region.id.as_str() == id)
            .expect(id)
            .area;
        let point = Point::new(area.x, area.y);
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            setup
                .update(RuntimeEvent::Terminal(Event::Mouse(MouseEvent::new(
                    kind, point,
                ))))
                .unwrap();
        }
        assert_eq!(setup.focus, 3);
        assert_eq!(setup.groups[3].choices[0].selected, toggles);
    }
    let area = setup
        .hits
        .iter()
        .find(|region| region.id.as_str() == "action-0")
        .unwrap()
        .area;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        setup
            .update(RuntimeEvent::Terminal(Event::Mouse(MouseEvent::new(
                kind,
                Point::new(area.x, area.y),
            ))))
            .unwrap();
    }
    assert!(setup.accepted);
}

#[test]
fn ignored_and_boundary_events_do_not_request_frames() {
    let mut groups = groups();
    let mut setup = program(&mut groups);
    for event in [
        key(KeyCode::Up),
        key(KeyCode::Char('z')),
        Event::Resize(Size::new(80, 24)),
    ] {
        assert_eq!(
            setup
                .update(RuntimeEvent::Terminal(event))
                .unwrap()
                .invalidation,
            Invalidation::None
        );
    }
    let release = crossterm::event::KeyEvent::new_with_kind(
        crossterm::event::KeyCode::Down,
        crossterm::event::KeyModifiers::NONE,
        crossterm::event::KeyEventKind::Release,
    );
    assert!(
        bmux_tui::crossterm::event_from_crossterm(crossterm::event::Event::Key(release)).is_none()
    );
}
#[test]
fn final_dirty_state_is_presented_after_input_stops() {
    let mut groups = groups();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (runtime, handle) = Runtime::new(
                program(&mut groups),
                HeadlessPresenter::default(),
                RuntimeConfig::default(),
            );
            let observer = handle.clone();
            let producer = tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                handle.send_terminal(key(KeyCode::Down)).await.unwrap();
                handle.send_terminal(key(KeyCode::Space)).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(60)).await;
                assert!(
                    observer.stats().frames_presented >= 2,
                    "dirty state must render without another input"
                );
                handle.send_terminal(submit_key()).await.unwrap();
            });
            let result = runtime.run().await.map_err(|_| "runtime failed").unwrap();
            producer.await.unwrap();
            assert!(result.program.groups[0].choices[1].selected);
        });
}

#[test]
fn burst_preserves_order_coalesces_frames_and_stops_at_enter() {
    let mut groups = groups();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (runtime, handle) = Runtime::new(
                program(&mut groups),
                HeadlessPresenter::default(),
                RuntimeConfig::default(),
            );
            for event in [
                key(KeyCode::Down),
                key(KeyCode::Down),
                key(KeyCode::Space),
                key(KeyCode::Up),
                key(KeyCode::Space),
                submit_key(),
                key(KeyCode::Space),
            ] {
                handle.send_terminal(event).await.unwrap();
            }
            let result = runtime.run().await.map_err(|_| "runtime failed").unwrap();
            assert_eq!(result.program.focus, 1);
            assert!(result.program.groups[0].choices[1].selected);
            assert!(result.program.groups[0].choices[2].selected);
            assert_eq!(result.stats.frames_presented, 1);
            assert!(result.stats.redraw_coalesced > 0);
        });
}
