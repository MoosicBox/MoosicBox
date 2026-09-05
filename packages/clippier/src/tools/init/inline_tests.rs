//! Offscreen regression tests for the production inline component composition.
use super::inline::{render, stops};
use super::recommendations::{Choice, Group};
use bmux_tui_components::scroll_view::ScrollViewState;

fn fixture() -> Vec<Group> {
    (0..6)
        .map(|index| Group {
            title: format!("Language {index}"),
            choices: vec![Choice {
                name: format!("tool-{index}"),
                reason: "Native config: example.toml".into(),
                selected: true,
            }],
            extensions: vec!["nix".into()],
            files: vec!["flake.nix".into()],
            formatting: true,
        })
        .collect()
}

#[test]
fn components_render_borders_details_and_focused_checkbox() {
    let groups = fixture();
    let buffer = render(&groups, (0, 0), 90, 26, &mut ScrollViewState::new());
    let text = buffer
        .cells()
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("Language 0"));
    assert!(text.contains("[x] tool-0"));
    assert!(text.contains("Capability"));
    assert!(text.contains("Coverage"));
    assert!(text.contains("┌"));
    assert!(buffer.cells().iter().any(|cell| {
        cell.style
            .modifiers
            .contains(bmux_tui::style::Modifier::REVERSED)
    }));
    assert_eq!(stops(&groups), (0..6).map(|g| (g, 0)).collect::<Vec<_>>());
}

#[test]
fn measured_focus_scrolls_and_small_terminals_are_bounded() {
    let groups = fixture();
    let mut scroll = ScrollViewState::new();
    let buffer = render(&groups, (5, 0), 60, 18, &mut scroll);
    assert!(scroll.vertical_offset() > 0);
    let text = buffer
        .cells()
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("[x] tool-5"));
    for (width, height) in [(1, 1), (20, 6), (40, 12)] {
        let buffer = render(&groups, (0, 0), width, height, &mut scroll);
        assert_eq!(
            buffer.cells().len(),
            usize::from(width) * usize::from(height)
        );
    }
}
