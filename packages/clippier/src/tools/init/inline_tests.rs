//! Offscreen regression tests for the production inline component composition.
use super::inline::{render, stops};
use super::recommendations::{Choice, Group};
use bmux_tui_components::scroll_view::ScrollViewState;

#[test]
fn height_budget_reserves_wrapped_header_and_cursor_row() {
    use super::inline::available_height;
    let header = vec![
        "Repository: /some/path".to_owned(),
        "x".repeat(70),
        "x".repeat(103),
    ];
    assert_eq!(available_height(&header, 120, 24), 20);
    assert_eq!(available_height(&header, 40, 24), 17);
    assert_eq!(available_height(&header, 40, 6), 0);
    let unicode = vec!["界".repeat(10)];
    assert_eq!(available_height(&unicode, 10, 10), 7);
}

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
    assert!(
        buffer
            .cells()
            .iter()
            .any(|cell| cell.style.fg == Some(bmux_tui::style::Color::Cyan))
    );
    assert!(
        buffer
            .cells()
            .iter()
            .any(|cell| cell.style.fg == Some(bmux_tui::style::Color::Green))
    );
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
    for height in [6, 12, 18, 30] {
        for index in (0..6).chain((0..6).rev()) {
            let buffer = render(&groups, (index, 0), 60, height, &mut scroll);
            let text = buffer
                .cells()
                .iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>();
            assert!(
                text.contains(&format!("[x] tool-{index}")),
                "focus missing at height {height}"
            );
            assert!(text.contains("Accept"), "footer missing at height {height}");
        }
    }
    for (width, height) in [(1, 1), (20, 6), (40, 12)] {
        let buffer = render(&groups, (0, 0), width, height, &mut scroll);
        assert_eq!(
            buffer.cells().len(),
            usize::from(width) * usize::from(height)
        );
    }
}
