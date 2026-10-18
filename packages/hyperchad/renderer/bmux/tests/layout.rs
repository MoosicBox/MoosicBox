//! App-shaped layout regressions: definite scrollports and shared album grid tracks.

use bmux_tui::{
    component::{Constraints, LogicalSize},
    geometry::Size,
};
use hyperchad_renderer_bmux::layout::{
    CellRect, DefaultLeafMeasure, LayoutOptions, LeafMeasure, ScrollOffset, layout,
};
use hyperchad_transformer::{
    Container, Element, Flex, Number,
    models::{AlignItems, JustifyContent, LayoutDirection, LayoutOverflow},
};

fn named(id: &str) -> Container {
    Container {
        str_id: Some(id.into()),
        ..Container::default()
    }
}

fn width(cells: i64) -> Option<Number> {
    Some(Number::Integer(cells * 8))
}

fn height(cells: i64) -> Option<Number> {
    Some(Number::Integer(cells * 16))
}

fn text(value: &str) -> Container {
    Container {
        element: Element::Text {
            value: value.into(),
        },
        ..Container::default()
    }
}

fn options(columns: u16, rows: u16) -> LayoutOptions {
    LayoutOptions {
        viewport: Size::new(columns, rows),
        ..LayoutOptions::default()
    }
}

#[test]
fn definite_row_stretches_only_auto_height_children_without_explicit_alignment() {
    for (alignment, auto_y, auto_height) in [
        (None, 0, 20),
        (Some(AlignItems::Start), 0, 2),
        (Some(AlignItems::Center), 9, 2),
        (Some(AlignItems::End), 18, 2),
    ] {
        let root = Container {
            direction: LayoutDirection::Row,
            align_items: alignment,
            children: vec![
                Container {
                    width: width(10),
                    children: vec![Container {
                        height: height(2),
                        ..Container::default()
                    }],
                    ..named("auto")
                },
                Container {
                    width: width(10),
                    height: height(4),
                    ..named("explicit")
                },
            ],
            ..Container::default()
        };
        let result = layout(&root, &options(40, 20), &DefaultLeafMeasure).unwrap();
        assert_eq!(
            result.get("auto").unwrap().bounds,
            CellRect::new(0, auto_y, 10, auto_height)
        );
        let explicit = result.get("explicit").unwrap();
        assert_eq!(explicit.bounds.height, 4);
        assert_eq!(
            explicit.bounds.y,
            match alignment {
                Some(AlignItems::Center) => 8,
                Some(AlignItems::End) => 16,
                _ => 0,
            }
        );
    }
}

#[test]
fn stretch_respects_margins_padding_limits_and_percentage_heights() {
    let root = Container {
        direction: LayoutDirection::Row,
        children: vec![
            Container {
                width: width(10),
                margin_top: height(1),
                margin_bottom: height(2),
                padding_top: height(1),
                padding_bottom: height(1),
                ..named("decorated")
            },
            Container {
                width: width(10),
                max_height: height(3),
                ..named("limited")
            },
            Container {
                width: width(10),
                height: Some(Number::IntegerPercent(50)),
                ..named("percentage")
            },
        ],
        ..Container::default()
    };
    let result = layout(&root, &options(60, 20), &DefaultLeafMeasure).unwrap();
    let decorated = result.get("decorated").unwrap();
    assert_eq!(decorated.bounds.y, 1);
    assert_eq!(decorated.bounds.height, 17);
    assert_eq!(decorated.content_bounds.height, 15);
    assert_eq!(result.get("limited").unwrap().bounds.height, 3);
    assert_eq!(result.get("percentage").unwrap().bounds.height, 10);
}

#[test]
fn auto_height_row_uses_its_line_height_not_the_viewport() {
    let root = Container {
        children: vec![Container {
            direction: LayoutDirection::Row,
            children: vec![
                Container {
                    width: width(10),
                    children: vec![text("A short title")],
                    ..named("auto")
                },
                Container {
                    width: width(10),
                    height: height(4),
                    ..named("explicit")
                },
            ],
            ..named("row")
        }],
        ..Container::default()
    };
    let result = layout(&root, &options(80, 40), &DefaultLeafMeasure).unwrap();
    assert_eq!(result.get("row").unwrap().bounds.height, 4);
    assert_eq!(result.get("auto").unwrap().bounds.height, 4);
}

fn album_grid(justify: JustifyContent) -> Container {
    Container {
        direction: LayoutDirection::Row,
        overflow_x: LayoutOverflow::Wrap { grid: true },
        grid_cell_size: width(20),
        column_gap: width(2),
        row_gap: height(1),
        justify_content: Some(justify),
        children: (0..11)
            .map(|index| Container {
                width: width(20),
                children: vec![
                    Container {
                        element: Element::Image {
                            source: Some(format!("/albums/{index}/cover?size=1200")),
                            alt: None,
                            fit: None,
                            source_set: None,
                            sizes: None,
                            loading: None,
                        },
                        width: width(20),
                        height: height(10),
                        ..named(&format!("cover-{index}"))
                    },
                    text(if index % 3 == 0 {
                        "The Very Long Album Title — Deluxe Remastered Edition with Bonus Recordings"
                    } else {
                        "Short album"
                    }),
                    text("Artist with a realistically long name"),
                    text("2024 · FLAC 24-bit / 192 kHz"),
                ],
                ..named(&format!("album-{index}"))
            })
            .collect(),
        ..named("grid")
    }
}

#[test]
fn incomplete_album_rows_share_columns_and_clear_long_titles() {
    for justify in [JustifyContent::SpaceBetween, JustifyContent::SpaceEvenly] {
        let root = Container {
            children: vec![album_grid(justify)],
            ..Container::default()
        };
        let result = layout(&root, &options(70, 50), &DefaultLeafMeasure).unwrap();
        // Three full rows and a final two-card row. Last-row distribution must
        // use the same tracks as the full rows, not redistribute two cards.
        for index in 3..11 {
            assert_eq!(
                result.get(&format!("album-{index}")).unwrap().bounds.x,
                result
                    .get(&format!("album-{}", index % 3))
                    .unwrap()
                    .bounds
                    .x
            );
        }
        for row in 0..3 {
            let bottom = (row * 3..row * 3 + 3)
                .map(|index| {
                    result
                        .get(&format!("album-{index}"))
                        .unwrap()
                        .bounds
                        .bottom()
                })
                .max()
                .unwrap();
            assert!(
                result
                    .get(&format!("album-{}", row * 3 + 3))
                    .unwrap()
                    .bounds
                    .y
                    > bottom
            );
        }
    }
}

fn app(columns: u16, rows: u16) -> Container {
    Container {
        children: vec![
            Container {
                direction: LayoutDirection::Row,
                height: height(i64::from(rows) - 5),
                children: vec![
                    Container {
                        width: width(24),
                        height: Some(Number::IntegerPercent(100)),
                        children: vec![text("MoosicBox"), text("My Collection"), text("Albums")],
                        ..named("sidebar")
                    },
                    Container {
                        flex: Some(Flex::default()),
                        overflow_y: LayoutOverflow::Auto,
                        children: vec![album_grid(JustifyContent::SpaceEvenly)],
                        ..named("main")
                    },
                ],
                ..named("section")
            },
            Container {
                width: width(i64::from(columns)),
                height: height(5),
                children: vec![text("Player — Album title and artist remain anchored")],
                ..named("footer")
            },
        ],
        ..named("root")
    }
}

#[test]
fn artwork_resize_and_scroll_keep_sidebar_footer_and_scrollport_anchored() {
    for (columns, rows) in [(110, 40), (70, 30), (150, 50)] {
        let root = app(columns, rows);
        let mut opts = options(columns, rows);
        let before = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
        // Model the real media measurer's decoded image dimensions rather than
        // the one-cell placeholders that previously concealed media overflow.
        let decoded = |node: &Container, constraints: Constraints| {
            if matches!(node.element, Element::Image { .. }) {
                LogicalSize::new(150, 75) // 1200×1200 pixels at 8×16 cell metrics
            } else {
                DefaultLeafMeasure.measure(node, constraints)
            }
        };
        let after = layout(&root, &opts, &decoded).unwrap();
        assert_eq!(
            after.get("sidebar").unwrap().bounds,
            CellRect::new(0, 0, 24, u64::from(rows - 5))
        );
        assert_eq!(
            after.get("footer").unwrap().bounds,
            CellRect::new(0, i64::from(rows - 5), u64::from(columns), 5)
        );
        let main = after.get("main").unwrap();
        assert_eq!(main.bounds.height, u64::from(rows - 5));
        assert_eq!(main.bounds.width, u64::from(columns - 24));
        assert!(main.content_extents.height > main.content_bounds.height);
        assert_eq!(main.text_clip.y, main.bounds.y);
        assert_eq!(main.text_clip.height, main.bounds.height);
        for id in ["sidebar", "footer", "main", "grid", "album-0", "album-10"] {
            assert_eq!(
                before.get(id).unwrap().bounds,
                after.get(id).unwrap().bounds
            );
        }
        opts.scroll_offsets
            .insert("main".into(), ScrollOffset { x: 0, y: 12 });
        let scrolled = layout(&root, &opts, &decoded).unwrap();
        for id in ["sidebar", "footer", "main"] {
            assert_eq!(
                after.get(id).unwrap().bounds,
                scrolled.get(id).unwrap().bounds
            );
        }
        // A wide viewport fits five cards per row, leaving only ten cells
        // below this scrollport. Requests must clamp to the actual extent,
        // while narrower viewports must consume the full twelve-cell request.
        let expected_scroll = 12.min(
            main.content_extents
                .height
                .saturating_sub(main.content_bounds.height),
        );
        assert!(
            expected_scroll >= 10,
            "fixture must scroll a full cover away"
        );
        assert_eq!(
            scrolled.get("main").unwrap().scroll_offset.y,
            expected_scroll
        );
        assert_eq!(
            scrolled.get("album-0").unwrap().bounds.y,
            after.get("album-0").unwrap().bounds.y - i64::try_from(expected_scroll).unwrap()
        );
        let hidden_cover = scrolled.get("cover-0").unwrap();
        assert!(hidden_cover.bounds.bottom() <= hidden_cover.clip.y);
        assert!((1..11).any(|index| {
            let cover = scrolled.get(&format!("cover-{index}")).unwrap();
            cover.bounds.bottom() > cover.clip.y && cover.bounds.y < cover.clip.bottom()
        }));
    }
}
