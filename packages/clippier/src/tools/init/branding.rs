//! Responsive ASCII branding for the inline setup screen.

use bmux_tui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

const LARGE: &[&str] = &[
    "   ____ _     ___ ____  ____ ___ _____ ____",
    "  / ___| |   |_ _|  _ \\|  _ \\_ _| ____|  _ \\",
    " | |   | |    | || |_) | |_) | ||  _| | |_) |",
    " | |___| |___ | ||  __/|  __/| || |___|  _ <",
    "  \\____|_____|___|_|   |_|  |___|_____|_| \\_\\",
];

/// Choose branding using the available viewport, before normal text measurement.
/// Short terminals prioritize the checklist rather than decorative rows.
pub(super) fn logo(width: u16, height: u16) -> Text {
    let large_width = LARGE.iter().map(|line| line.len()).max().unwrap_or(0);
    let lines = if height >= 32 && usize::from(width) >= large_width {
        LARGE.to_vec()
    } else if height >= 22 && width >= 29 {
        vec![
            "  /-----------------------/",
            " /   C L I P P I E R      /",
            "/-----------------------/",
        ]
    } else {
        vec!["CLIPPIER"]
    };
    let count = lines.len();
    Text::from_lines(
        lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let color = if index < count / 2 {
                    Color::Cyan
                } else {
                    Color::Blue
                };
                Line::from_spans(vec![Span::styled(
                    line,
                    Style::new().fg(color).add_modifier(Modifier::BOLD),
                )])
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branding_adapts_without_overflowing_or_embedding_controls() {
        for (width, height, rows) in [(100, 40, 5), (40, 24, 3), (100, 12, 1), (20, 40, 1)] {
            let text = logo(width, height);
            assert_eq!(text.lines.len(), rows);
            assert!(text.width() <= usize::from(width));
            assert!(
                text.lines
                    .iter()
                    .all(|line| !line.plain_text().contains(['\n', '\r', '\x1b']))
            );
        }
    }
}
