//! Offscreen regression tests for the production inline component composition.
use super::inline::{render, stops};
use super::recommendations::{Choice, Group};
use bmux_tui_components::scroll_view::ScrollViewState;
use std::cell::Cell;

#[test]
fn height_budget_reserves_wrapped_header_and_cursor_row() {
    let header = vec![
        "Repository: /some/path".to_owned(),
        "Wrapped instruction ".repeat(8),
    ];
    let groups = fixture();
    for width in [40, 90] {
        let buffer = render(
            &groups,
            &header,
            (0, 0),
            width,
            30,
            &Cell::new(ScrollViewState::new()),
        );
        let text = buffer
            .cells()
            .iter()
            .map(|cell| cell.symbol.as_str())
            .collect::<String>();
        assert!(text.contains("Repository:"));
        assert!(
            buffer
                .cells()
                .iter()
                .all(|cell| !cell.symbol.contains(['\n', '\r']))
        );
        let rows = buffer
            .cells()
            .chunks(usize::from(width))
            .map(|row| {
                row.iter()
                    .map(|cell| cell.symbol.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let repository_row = rows
            .iter()
            .position(|row| row.starts_with("Repository: /some/path"))
            .unwrap();
        assert!(rows[repository_row + 1].starts_with("Wrapped instruction"));
        assert!(
            rows[..repository_row]
                .iter()
                .any(|row| row.contains("C L I P P I E R"))
        );
        let mut output = Vec::new();
        bmux_tui::ansi::write_ansi_inline_frame(&mut output, &buffer).unwrap();
        // Small regression fixture; no byte-count dependency needed.
        #[allow(clippy::naive_bytecount)]
        let emitted_rows = output.iter().filter(|byte| **byte == b'\n').count();
        assert_eq!(emitted_rows, usize::from(buffer.area().height));
        assert!(text.contains("[x] tool-0"));
        assert!(text.contains("Submit"), "{width}: {text}");
    }
}

fn fixture() -> Vec<Group> {
    (0..6)
        .map(|index| Group {
            title: format!("Language {index}"),
            choices: vec![Choice {
                name: format!("tool-{index}"),
                reason: "Native config: example.toml".into(),
                selected: true,
                installed: None,
                source_only: false,
                active: false,
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
    let buffer = render(
        &groups,
        &[],
        (0, 0),
        90,
        26,
        &Cell::new(ScrollViewState::new()),
    );
    let text = buffer
        .cells()
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    let first_row = buffer.cells()[..90]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(first_row.contains("Language 0"), "first row: {first_row:?}");
    assert!(first_row.contains('┌'));
    assert!(text.contains("Language 0"));
    assert!(text.contains("[x] tool-0"));
    assert!(text.contains("Capability"));
    assert!(text.contains("Coverage"));
    assert!(text.contains("┌"));
    assert!(buffer.cells().iter().any(|cell| {
        cell.style.bg == Some(bmux_tui::style::Color::Cyan)
            && cell
                .style
                .modifiers
                .contains(bmux_tui::style::Modifier::BOLD)
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
fn reversing_direction_keeps_viewport_until_focus_crosses_edge() {
    let groups = fixture();
    let scroll = Cell::new(ScrollViewState::new());
    let bottom_frame = render(&groups, &[], (5, 0), 60, 18, &scroll);
    let rows = bottom_frame
        .cells()
        .chunks(60)
        .map(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let last_choice = rows
        .iter()
        .position(|row| row.contains("[x] tool-5"))
        .unwrap();
    assert!(
        rows[last_choice + 1].starts_with('└'),
        "last panel border is clipped"
    );
    let bottom = scroll.get().vertical_offset();
    assert!(bottom > 0);
    render(&groups, &[], (4, 0), 60, 18, &scroll);
    assert_eq!(scroll.get().vertical_offset(), bottom);
    let buffer = render(&groups, &[], (0, 0), 60, 18, &scroll);
    assert_eq!(scroll.get().vertical_offset(), 0);
    let top = buffer.cells()[..60]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(top.contains("Language 0"), "missing top panel title: {top}");
    assert!(top.starts_with('┌'));
}

#[test]
fn measured_focus_scrolls_and_small_terminals_are_bounded() {
    let groups = fixture();
    let scroll = Cell::new(ScrollViewState::new());
    let buffer = render(&groups, &[], (5, 0), 60, 18, &scroll);
    let text = buffer
        .cells()
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    assert!(text.contains("[x] tool-5"));
    for height in [6, 12, 18, 30] {
        for index in (0..6).chain((0..6).rev()) {
            let buffer = render(&groups, &[], (index, 0), 60, height, &scroll);
            let text = buffer
                .cells()
                .iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>();
            assert!(
                text.contains(&format!("[x] tool-{index}")),
                "focus missing at height {height}"
            );
            // A partially visible section may have its header above the viewport.
            // Do not scroll just to reveal that header while focus remains visible.
            assert!(text.contains("Submit"), "footer missing at height {height}");
        }
    }
    for (width, height) in [(1, 1), (20, 6), (40, 12)] {
        let buffer = render(&groups, &[], (0, 0), width, height, &scroll);
        assert_eq!(
            buffer.cells().len(),
            usize::from(width) * usize::from(height)
        );
    }
}
