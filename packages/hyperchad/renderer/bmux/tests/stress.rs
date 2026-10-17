//! Repeatable workload measurements, never machine-dependent timing thresholds.
#[path = "../examples/support/mod.rs"]
mod support;

use bmux_tui::geometry::Size;
use hyperchad_renderer::View;
use hyperchad_renderer_bmux::{
    layout::{DefaultLeafMeasure, LayoutOptions, layout},
    state::ViewState,
};
use hyperchad_transformer::{Container, Element, Number};
use std::time::Instant;

fn table(rows: usize) -> Container {
    let rows = (0..rows)
        .map(|row| {
            let base = 10 + row * 10;
            let cells = (0..3)
                .map(|column| {
                    let id = base + column * 2 + 1;
                    support::node(
                        id,
                        &format!("cell-{row}-{column}"),
                        Element::TD {
                            rows: None,
                            columns: None,
                        },
                        vec![support::text(
                            id + 1,
                            &format!("text-{row}-{column}"),
                            &format!("{row}:{column} Núria 星 e\u{301} 🎵"),
                        )],
                    )
                })
                .collect();
            support::node(base, &format!("row-{row}"), Element::TR, cells)
        })
        .collect();
    let mut table = support::node(
        2,
        "stress-table",
        Element::Table,
        vec![support::node(3, "stress-body", Element::TBody, rows)],
    );
    table.width = Some(Number::IntegerPercent(100));
    support::node(
        1,
        "stress-root",
        Element::Main,
        vec![
            support::text(5, "stress-heading", "Unicode library stress"),
            table,
            support::text(4, "stress-status", "initial"),
        ],
    )
}

#[test]
fn unicode_large_tables_and_rapid_atomic_fragments() {
    // 7 nodes per row plus five structural/status nodes: 1055 and 4905.
    for rows in [150, 700] {
        let started = Instant::now();
        let mut state = ViewState::from_root(table(rows)).unwrap();
        let options = LayoutOptions {
            viewport: Size::new(100, 32),
            ..LayoutOptions::default()
        };
        let tree = layout(state.root(), &options, &DefaultLeafMeasure).unwrap();
        assert_eq!(tree.nodes.len(), rows * 7 + 5);
        assert!(tree.get(&format!("text-{}-2", rows - 1)).is_some());
        let initial = started.elapsed();
        let updates = Instant::now();
        for sequence in 0..64 {
            state
                .apply_view(
                    View::builder()
                        .with_fragment(support::text(
                            4,
                            "stress-status",
                            &format!("refresh-{sequence} 星"),
                        ))
                        .build(),
                    None,
                )
                .unwrap();
        }
        let update_time = updates.elapsed();
        let resized = Instant::now();
        let compact = layout(
            state.root(),
            &LayoutOptions {
                viewport: Size::new(48, 16),
                ..options
            },
            &DefaultLeafMeasure,
        )
        .unwrap();
        assert_eq!(compact.nodes.len(), tree.nodes.len());
        let Element::Text { value } = &state.find_node("stress-status").unwrap().element else {
            panic!("status text");
        };
        assert_eq!(value, "refresh-63 星");
        println!(
            "nodes={} rows={rows} initial_layout={initial:?} partial_updates_64={update_time:?} resize_layout={:?}",
            tree.nodes.len(),
            resized.elapsed()
        );
    }
}
